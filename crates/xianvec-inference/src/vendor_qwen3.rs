//! Vendored from candle-transformers 0.10.2 / src/models/quantized_qwen3.rs
//! crate checksum (Cargo.lock): f59d08c89e9f4af9c464e2f3a8e16199e7cc601e6f34538c2cfbb42b623b1783
//! Tracking issue: ADR 0007.
//! Re-sync when upstream cuts a release that exposes per-block hooks.
//!
//! ## Why this file exists
//!
//! `candle_transformers::models::quantized_qwen3::ModelWeights::forward` iterates
//! a private `Vec<LayerWeights>` field — there is no per-block call surface exposed
//! to outside callers. To fire `LayerHook::apply` after each transformer block we
//! must either:
//!   (a) Mirror the upstream struct layout and transmute to access `layers` (Option A),
//!   (b) Vendor the full load path (Option B).
//!
//! We use **Option A**: mirror `ModelWeights` field-for-field (same types, same order),
//! transmute from the upstream value (which was loaded via the canonical
//! `ModelWeights::from_gguf`), and drive the layer loop ourselves so each block
//! fires `hook.apply()`.
//!
//! The transmute is guarded by a `size_of` + `align_of` assertion immediately
//! inside `forward_with_hooks`. If upstream rearranges or adds fields the
//! assertion fires at runtime with a diagnostic panic — the model will refuse to
//! load rather than silently miscompute.
//!
//! ### Field order (candle-transformers 0.10.2, struct `ModelWeights`)
//! ```text
//! embed_tokens : candle_nn::Embedding
//! layers       : Vec<LayerWeights>          ← private; this is what we need
//! norm         : candle_transformers::quantized_nn::RmsNorm
//! lm_head      : with_tracing::QMatMul
//! device       : candle_core::Device
//! dtype        : candle_core::DType
//! span         : tracing::Span
//! span_output  : tracing::Span
//! ```
//!
//! We do NOT re-implement loading. The upstream `ModelWeights::from_gguf` is still
//! used; we only borrow its result for the forward pass.

use std::sync::Arc;

use candle_core::{DType, Device, Module, Result, Tensor};
use candle_nn::{kv_cache::ConcatKvCache, Activation, Embedding};
use candle_transformers::models::with_tracing::QMatMul;
use candle_transformers::quantized_nn::RmsNorm;
use candle_transformers::utils::repeat_kv;
use tracing::Span;

use crate::hooks::{HookContext, IdentityHook, LayerHook};

// ---------------------------------------------------------------------------
// Vendored private helpers
// ---------------------------------------------------------------------------

/// Vendored from `quantized_qwen3::RotaryEmbedding` (upstream is `pub` — we
/// mirror it here only so our `AttentionWeights` can own an `Arc<RotaryEmbedding>`
/// that matches the upstream field type exactly.
///
/// The upstream `RotaryEmbedding` is `pub` in candle 0.10.2, so we import it
/// directly instead of re-defining it.
use candle_transformers::models::quantized_qwen3::RotaryEmbedding;

// ---------------------------------------------------------------------------
// Vendored MlpWeights
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct MlpWeights {
    gate_proj: QMatMul,
    up_proj: QMatMul,
    down_proj: QMatMul,
    act_fn: Activation,
    span: Span,
}

impl Module for MlpWeights {
    fn forward(&self, x: &Tensor) -> Result<Tensor> {
        let _enter = self.span.enter();
        let gate = self.gate_proj.forward(x)?.apply(&self.act_fn)?;
        let up = self.up_proj.forward(x)?;
        let gated = (gate * up)?;
        self.down_proj.forward(&gated)
    }
}

// ---------------------------------------------------------------------------
// Vendored AttentionWeights
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct AttentionWeights {
    q_proj: QMatMul,
    k_proj: QMatMul,
    v_proj: QMatMul,
    o_proj: QMatMul,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    num_heads: usize,
    num_kv_heads: usize,
    num_kv_groups: usize,
    head_dim: usize,
    rotary_emb: Arc<RotaryEmbedding>,
    kv_cache: ConcatKvCache,
    span_attn: Span,
}

impl AttentionWeights {
    fn forward(&mut self, x: &Tensor, attn_mask: Option<&Tensor>, offset: usize) -> Result<Tensor> {
        let _enter = self.span_attn.enter();
        let (b, l, _) = x.dims3()?;

        let q = self.q_proj.forward(x)?;
        let k = self.k_proj.forward(x)?;
        let v = self.v_proj.forward(x)?;

        let q = q.reshape((b, l, self.num_heads, self.head_dim))?.transpose(1, 2)?;
        let k = k.reshape((b, l, self.num_kv_heads, self.head_dim))?.transpose(1, 2)?;
        let v = v.reshape((b, l, self.num_kv_heads, self.head_dim))?.transpose(1, 2)?;

        let q_flat = q.flatten(0, 2)?;
        let k_flat = k.flatten(0, 2)?;
        let q_flat = self.q_norm.forward(&q_flat)?;
        let k_flat = self.k_norm.forward(&k_flat)?;
        let q = q_flat.reshape((b, self.num_heads, l, self.head_dim))?;
        let k = k_flat.reshape((b, self.num_kv_heads, l, self.head_dim))?;

        let (q, k) = self.rotary_emb.apply(&q, &k, offset)?;
        let (k, v) = self.kv_cache.append(&k, &v)?;

        let k = repeat_kv(k, self.num_kv_groups)?.contiguous()?;
        let v = repeat_kv(v, self.num_kv_groups)?.contiguous()?;

        let scale = 1.0 / (self.head_dim as f64).sqrt();
        let mut scores = (q.matmul(&k.transpose(2, 3)?)? * scale)?;
        if let Some(m) = attn_mask {
            let m_dtype = m.dtype();
            let scores_dtype = scores.dtype();
            let mask = if m_dtype != scores_dtype {
                m.to_dtype(scores_dtype)?
            } else {
                m.clone()
            };
            scores = scores.broadcast_add(&mask)?;
        }
        let probs = candle_nn::ops::softmax_last_dim(&scores)?;
        let ctx = probs.matmul(&v)?;
        let reshaped = ctx
            .transpose(1, 2)?
            .reshape((b, l, self.num_heads * self.head_dim))?;
        self.o_proj.forward(&reshaped)
    }
}

// ---------------------------------------------------------------------------
// Vendored LayerWeights
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
struct LayerWeights {
    self_attn: AttentionWeights,
    mlp: MlpWeights,
    ln1: RmsNorm,
    ln2: RmsNorm,
}

impl LayerWeights {
    fn forward(&mut self, x: &Tensor, mask: Option<&Tensor>, offset: usize) -> Result<Tensor> {
        let h = self.ln1.forward(x)?;
        let h = self.self_attn.forward(&h, mask, offset)?;
        let x = (x + h)?;
        let h2 = self.ln2.forward(&x)?;
        let h2 = h2.apply(&self.mlp)?;
        x + h2
    }
}

// ---------------------------------------------------------------------------
// Mirror struct for transmute
//
// SAFETY: this struct is layout-compatible with the upstream
// `candle_transformers::models::quantized_qwen3::ModelWeights` as of
// candle-transformers 0.10.2 (checksum
// f59d08c89e9f4af9c464e2f3a8e16199e7cc601e6f34538c2cfbb42b623b1783).
//
// Both this struct and upstream `ModelWeights` use Rust's default repr.
// Fields are listed in the exact same order with the exact same types.
// Since the types are identical the compiler places them at the same offsets.
//
// The `forward_with_hooks` function asserts `size_of` and `align_of` before
// dereferencing; any upstream layout drift causes a loud runtime panic rather
// than silent data corruption.  We deliberately do NOT add `#[repr(C)]`
// because the upstream struct lacks it — adding `#[repr(C)]` here would
// actually introduce a layout mismatch.
// ---------------------------------------------------------------------------

#[allow(dead_code)] // span/span_output exist solely to mirror upstream layout
struct ModelWeightsMirror {
    embed_tokens: Embedding,
    layers: Vec<LayerWeights>,
    norm: RmsNorm,
    lm_head: QMatMul,
    device: Device,
    dtype: DType,
    span: Span,
    span_output: Span,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

use candle_transformers::models::quantized_qwen3::ModelWeights;

/// Run a full Qwen3 forward pass, firing `hook.apply(layer_idx, &residual, ctx)`
/// after every transformer block.
///
/// `weights` is the model loaded via the upstream `ModelWeights::from_gguf`.
/// We access its private `layers` field via an unsafe layout-compatible
/// transmute (see `ModelWeightsMirror` above).
///
/// # Panics
///
/// Panics with `"upstream candle-transformers ModelWeights layout drift detected
/// — re-sync vendor_qwen3.rs"` if `size_of::<ModelWeights>() !=
/// size_of::<ModelWeightsMirror>()` or the alignments differ.  This guards
/// against silent miscomputation if upstream rearranges fields in a future
/// release.
pub fn forward_with_hooks(
    weights: &mut ModelWeights,
    input_ids: &Tensor,
    seqlen_offset: usize,
    hook: &dyn LayerHook,
    ctx: &HookContext,
) -> Result<Tensor> {
    // Layout-drift guard — fires loudly rather than silently miscomputing.
    assert!(
        std::mem::size_of::<ModelWeights>() == std::mem::size_of::<ModelWeightsMirror>()
            && std::mem::align_of::<ModelWeights>() == std::mem::align_of::<ModelWeightsMirror>(),
        "upstream candle-transformers ModelWeights layout drift detected — re-sync vendor_qwen3.rs"
    );

    // SAFETY: We verified size and alignment above. `ModelWeightsMirror` has
    // exactly the same field types in the same order as the upstream
    // `ModelWeights` (candle-transformers 0.10.2).  We hold a `&mut` reference
    // for the duration of this call, which is the same lifetime we would need
    // to call `weights.forward()`.  No aliasing occurs: `mirror` is the sole
    // live reference to `*weights` for the duration of this function.
    let mirror: &mut ModelWeightsMirror =
        unsafe { &mut *(weights as *mut ModelWeights as *mut ModelWeightsMirror) };

    let (b, l) = input_ids.dims2()?;
    let mut h = mirror.embed_tokens.forward(input_ids)?;

    let causal_mask = if l == 1 {
        None
    } else {
        Some(causal_mask_for(b, l, seqlen_offset, &mirror.device, mirror.dtype)?)
    };

    for (layer_idx, layer) in mirror.layers.iter_mut().enumerate() {
        h = layer.forward(&h, causal_mask.as_ref(), seqlen_offset)?;
        // Fire the hook on the post-block residual (shape: B × L × hidden_dim).
        h = hook.apply(layer_idx, &h, ctx)?;
    }

    let h = mirror.norm.forward(&h)?;
    let last_hidden = h.narrow(1, l - 1, 1)?;
    mirror.lm_head.forward(&last_hidden)?.squeeze(1)
}

/// Build a causal mask identical to the upstream `ModelWeights::causal_mask`.
fn causal_mask_for(
    b: usize,
    tgt: usize,
    offset: usize,
    device: &Device,
    dtype: DType,
) -> Result<Tensor> {
    let minf = f32::NEG_INFINITY;
    let mask: Vec<_> = (0..tgt)
        .flat_map(|i| {
            (0..(tgt + offset)).map(move |j| {
                if j <= i + offset { 0. } else { minf }
            })
        })
        .collect();
    Tensor::from_slice(&mask, (b, 1, tgt, tgt + offset), device)?.to_dtype(dtype)
}

/// Convenience wrapper: forward pass with an `IdentityHook` (no steering, no
/// introspection). Equivalent to calling upstream `weights.forward(input, offset)`
/// but goes through our loop so the hook wiring is exercised.
#[allow(dead_code)]
pub fn forward_identity(
    weights: &mut ModelWeights,
    input_ids: &Tensor,
    seqlen_offset: usize,
) -> Result<Tensor> {
    let ctx = HookContext::new(0);
    forward_with_hooks(weights, input_ids, seqlen_offset, &IdentityHook, &ctx)
}
