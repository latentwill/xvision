//! Layer-hook trait. Phase 4 wires steering + introspection hooks against
//! this surface. v1 ships an `IdentityHook` for testing.
//!
//! **Tier 1 fix #5 — entropy gating placement**: `EntropyGated` is enforced by
//! the *outer caller* (the trader's constrained-generation loop), not inside
//! `apply()`. Inside `apply()` the `EntropyGated` strategy is treated as
//! `Always` — the gate decision is fed in by the caller via `Context::gate_open`.
//! This keeps the hook interface stateless and lets the outer loop make the
//! decision at the logits level (after the `"action"` choice token) rather than
//! inspecting the hidden state inside the hook.

use candle_core::Tensor;

/// Per-call context forwarded from the engine's generation loop to every hook.
#[derive(Debug, Clone)]
pub struct HookContext {
    /// Zero-based index of the current token being generated (0 = first
    /// completion token after the prompt).
    pub token_index: u32,
    /// Logits from the previous step, if the caller wants to surface them to
    /// the hook (e.g. for entropy inspection). `None` on the first step.
    pub last_logits: Option<Tensor>,
}

impl HookContext {
    pub fn new(token_index: u32) -> Self {
        Self {
            token_index,
            last_logits: None,
        }
    }
}

pub trait LayerHook: Send + Sync {
    /// Apply a transformation to the residual stream at `layer_idx`.
    ///
    /// The `ctx` carries the generation-loop position and optionally the prior
    /// step's logits so gating strategies that need entropy can read them
    /// without re-entering the forward pass.
    fn apply(
        &self,
        layer_idx: usize,
        residual: &Tensor,
        ctx: &HookContext,
    ) -> candle_core::Result<Tensor>;
}

pub struct IdentityHook;

impl LayerHook for IdentityHook {
    fn apply(
        &self,
        _layer_idx: usize,
        residual: &Tensor,
        _ctx: &HookContext,
    ) -> candle_core::Result<Tensor> {
        Ok(residual.clone())
    }
}
