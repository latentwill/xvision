//! Phase 4.4.1 introspection hook — wraps any `LayerHook` and captures
//! per-layer diagnostics without modifying the residual stream.
//!
//! Captures are opt-in via `CaptureFlags`; when all flags are `false` the
//! overhead is near-zero (a mutex lock + flag check per layer call).
//!
//! **Logit lens** requires `ln_f` (final layer norm) and `lm_head` (unembedding
//! matrix) to be provided at construction time. If either is absent the
//! `logit_lens` flag is silently ignored.

use std::sync::{Arc, Mutex};

use candle_core::Tensor;
use serde::{Deserialize, Serialize};
use xianvec_inference::hooks::{HookContext, LayerHook};

// ---------------------------------------------------------------------------
// CaptureFlags
// ---------------------------------------------------------------------------

/// Which diagnostics to record on each `apply()` call.
#[derive(Debug, Clone, Copy, Default)]
pub struct CaptureFlags {
    /// Record `||residual||_2` before and after the inner hook.
    pub residual_norms: bool,
    /// Record the per-element difference `post - pre`.
    pub activation_diff: bool,
    /// Record `cosine(post, steering_vec)` (requires a steering vector ref).
    pub vector_residual_cosine: bool,
    /// Project `post` through `ln_f → lm_head` to get a vocabulary distribution.
    pub logit_lens: bool,
    /// Capture the logit vector at this layer (synonym for logit_lens on each
    /// decision-relevant token; stored separately for clarity).
    pub decision_token_logits: bool,
    /// Record Shannon entropy of the logit distribution.
    pub decision_token_entropy: bool,
}

// ---------------------------------------------------------------------------
// CaptureBuffer + InspectionReport
// ---------------------------------------------------------------------------

/// One residual-norm sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResidualNormSample {
    pub layer_idx: usize,
    pub token_index: u32,
    pub pre_norm: f32,
    pub post_norm: f32,
}

/// One logit-lens sample (top-5 token indices + logits for compactness).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogitLensSample {
    pub layer_idx: usize,
    pub token_index: u32,
    /// Top-5 (token_id, logit) pairs.
    pub top5: Vec<(u32, f32)>,
    /// Shannon entropy of the full distribution.
    pub entropy: f32,
}

/// One activation-diff sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActivationDiffSample {
    pub layer_idx: usize,
    pub token_index: u32,
    /// L2 norm of `(post - pre)`.
    pub diff_norm: f32,
}

/// Accumulated captures from all `apply()` calls since the last `drain_report`.
#[derive(Debug, Default)]
pub struct CaptureBuffer {
    pub residual_norms: Vec<ResidualNormSample>,
    pub activation_diffs: Vec<ActivationDiffSample>,
    pub logit_lens: Vec<LogitLensSample>,
}

/// JSON-serializable inspection report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionReport {
    pub residual_norms: Vec<ResidualNormSample>,
    pub activation_diffs: Vec<ActivationDiffSample>,
    pub logit_lens: Vec<LogitLensSample>,
}

// ---------------------------------------------------------------------------
// IntrospectionHook
// ---------------------------------------------------------------------------

/// Wraps any `LayerHook`, optionally capturing diagnostic data, then delegates
/// to `inner`. The residual stream is never modified by this hook itself.
pub struct IntrospectionHook<H: LayerHook> {
    inner: H,
    flags: CaptureFlags,
    capture: Arc<Mutex<CaptureBuffer>>,
    /// Final layer norm: `Fn(&Tensor) -> Result<Tensor>`. Required for logit lens.
    ln_f: Option<Arc<dyn Fn(&Tensor) -> candle_core::Result<Tensor> + Send + Sync>>,
    /// Unembedding matrix `(vocab_size, hidden_dim)`. Required for logit lens.
    lm_head: Option<Arc<Tensor>>,
}

impl<H: LayerHook> IntrospectionHook<H> {
    /// Create a new `IntrospectionHook` wrapping `inner`.
    ///
    /// `ln_f` and `lm_head` are optional; if either is absent the `logit_lens`,
    /// `decision_token_logits`, and `decision_token_entropy` flags are silently
    /// ignored.
    pub fn new(inner: H, flags: CaptureFlags) -> Self {
        Self {
            inner,
            flags,
            capture: Arc::new(Mutex::new(CaptureBuffer::default())),
            ln_f: None,
            lm_head: None,
        }
    }

    /// Attach the layer-norm and lm_head needed for logit-lens captures.
    pub fn with_lm_head(
        mut self,
        ln_f: Arc<dyn Fn(&Tensor) -> candle_core::Result<Tensor> + Send + Sync>,
        lm_head: Arc<Tensor>,
    ) -> Self {
        self.ln_f = Some(ln_f);
        self.lm_head = Some(lm_head);
        self
    }

    /// Drain the capture buffer and return an `InspectionReport`.
    pub fn drain_report(&self) -> InspectionReport {
        let mut buf = self.capture.lock().expect("capture lock poisoned");
        InspectionReport {
            residual_norms: std::mem::take(&mut buf.residual_norms),
            activation_diffs: std::mem::take(&mut buf.activation_diffs),
            logit_lens: std::mem::take(&mut buf.logit_lens),
        }
    }

    /// Return a clone of the `CaptureBuffer` arc (for external consumers).
    pub fn capture_arc(&self) -> Arc<Mutex<CaptureBuffer>> {
        self.capture.clone()
    }
}

impl<H: LayerHook> LayerHook for IntrospectionHook<H> {
    fn apply(
        &self,
        layer_idx: usize,
        residual: &Tensor,
        ctx: &HookContext,
    ) -> candle_core::Result<Tensor> {
        // Fast path: when all flags are off, skip all capture work (including
        // the mutex lock) and delegate directly. This is the common prod path
        // when the hook is installed but no capture is requested.
        let any_flag = self.flags.residual_norms
            || self.flags.activation_diff
            || self.flags.vector_residual_cosine
            || self.flags.logit_lens
            || self.flags.decision_token_logits
            || self.flags.decision_token_entropy;

        if !any_flag {
            return self.inner.apply(layer_idx, residual, ctx);
        }

        // --- pre-hook captures ---
        let pre_norm_val: Option<f32> = if self.flags.residual_norms {
            Some(l2_norm(residual)?)
        } else {
            None
        };

        let pre_clone: Option<Tensor> = if self.flags.activation_diff {
            Some(residual.clone())
        } else {
            None
        };

        // --- delegate ---
        let post = self.inner.apply(layer_idx, residual, ctx)?;

        // --- post-hook captures ---
        let mut buf = self.capture.lock().expect("capture lock poisoned");

        if let Some(pre_norm) = pre_norm_val {
            let post_norm = l2_norm(&post)?;
            buf.residual_norms.push(ResidualNormSample {
                layer_idx,
                token_index: ctx.token_index,
                pre_norm,
                post_norm,
            });
        }

        if let Some(pre) = pre_clone {
            let diff = post.broadcast_sub(&pre)?;
            let diff_norm = l2_norm(&diff)?;
            buf.activation_diffs.push(ActivationDiffSample {
                layer_idx,
                token_index: ctx.token_index,
                diff_norm,
            });
        }

        if (self.flags.logit_lens || self.flags.decision_token_logits || self.flags.decision_token_entropy)
            && self.ln_f.is_some()
            && self.lm_head.is_some()
        {
            let ln_f = self.ln_f.as_ref().unwrap();
            let lm_head = self.lm_head.as_ref().unwrap();
            // post may be (seq_len, hidden) or (hidden,); normalise to (1, hidden).
            let post_2d = ensure_2d(&post)?;
            let normed = ln_f(&post_2d)?;
            // lm_head: (vocab, hidden) → logits: (1, vocab)
            let logits = normed.matmul(&lm_head.t()?)?;
            let logits_1d = logits.squeeze(0)?;
            let logits_vec: Vec<f32> = logits_1d.to_vec1()?;

            // Compute entropy + top5.
            let max_l = logits_vec
                .iter()
                .copied()
                .fold(f32::NEG_INFINITY, f32::max);
            let exps: Vec<f32> = logits_vec.iter().map(|x| (x - max_l).exp()).collect();
            let sum_exp: f32 = exps.iter().sum();
            let probs: Vec<f32> = exps.iter().map(|x| x / sum_exp).collect();
            let entropy = -probs
                .iter()
                .filter(|&&p| p > 0.0)
                .map(|&p| p * p.ln())
                .sum::<f32>();

            let mut indexed: Vec<(u32, f32)> = probs
                .iter()
                .enumerate()
                .map(|(i, &p)| (i as u32, p))
                .collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let top5: Vec<(u32, f32)> = indexed.into_iter().take(5).collect();

            buf.logit_lens.push(LogitLensSample {
                layer_idx,
                token_index: ctx.token_index,
                top5,
                entropy,
            });
        }

        Ok(post)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

fn l2_norm(t: &Tensor) -> candle_core::Result<f32> {
    t.flatten_all()?.sqr()?.sum_all()?.sqrt()?.to_scalar::<f32>()
}

fn ensure_2d(t: &Tensor) -> candle_core::Result<Tensor> {
    match t.dims() {
        [_] => t.unsqueeze(0),
        [_, _] => {
            // Take last token row.
            let len = t.dim(0)?;
            t.narrow(0, len - 1, 1)
        }
        _ => t.flatten(0, t.dims().len() - 2)?.narrow(0, 0, 1),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;
    use xianvec_inference::hooks::IdentityHook;

    fn random_tensor(dim: usize, device: &Device) -> Tensor {
        // Use deterministic "random" values for reproducibility.
        let data: Vec<f32> = (0..dim)
            .map(|i| ((i as f32 * 1.6180339887) % 2.0) - 1.0)
            .collect();
        Tensor::from_vec(data, (dim,), device).unwrap()
    }

    #[test]
    fn introspection_hook_identity_passthrough() {
        let device = Device::Cpu;
        let hook = IntrospectionHook::new(IdentityHook, CaptureFlags::default());
        let t = random_tensor(64, &device);
        let ctx = HookContext::new(0);
        let out = hook.apply(0, &t, &ctx).unwrap();
        let a: Vec<f32> = t.to_vec1().unwrap();
        let b: Vec<f32> = out.to_vec1().unwrap();
        assert_eq!(a, b, "identity passthrough should preserve tensor");
    }

    #[test]
    fn capture_flags_all_off_no_allocation() {
        let device = Device::Cpu;
        let hook = IntrospectionHook::new(IdentityHook, CaptureFlags::default());
        let t = random_tensor(64, &device);
        let ctx = HookContext::new(0);
        for _ in 0..10 {
            hook.apply(0, &t, &ctx).unwrap();
        }
        let report = hook.drain_report();
        assert!(
            report.residual_norms.is_empty(),
            "no norms captured when flag off"
        );
        assert!(
            report.activation_diffs.is_empty(),
            "no diffs captured when flag off"
        );
    }

    #[test]
    fn capture_residual_norms() {
        let device = Device::Cpu;
        let flags = CaptureFlags {
            residual_norms: true,
            ..Default::default()
        };
        let hook = IntrospectionHook::new(IdentityHook, flags);
        let t = random_tensor(64, &device);
        let ctx = HookContext::new(0);
        hook.apply(0, &t, &ctx).unwrap();
        hook.apply(1, &t, &ctx).unwrap();
        let report = hook.drain_report();
        assert_eq!(report.residual_norms.len(), 2);
        // For identity hook, pre == post norm.
        for s in &report.residual_norms {
            assert!(
                (s.pre_norm - s.post_norm).abs() < 1e-5,
                "identity: pre_norm should equal post_norm"
            );
        }
    }

    #[test]
    fn drain_report_clears_buffer() {
        let device = Device::Cpu;
        let flags = CaptureFlags {
            residual_norms: true,
            ..Default::default()
        };
        let hook = IntrospectionHook::new(IdentityHook, flags);
        let t = random_tensor(64, &device);
        let ctx = HookContext::new(0);
        hook.apply(0, &t, &ctx).unwrap();
        let r1 = hook.drain_report();
        assert_eq!(r1.residual_norms.len(), 1);
        let r2 = hook.drain_report();
        assert!(r2.residual_norms.is_empty(), "buffer should be empty after drain");
    }
}
