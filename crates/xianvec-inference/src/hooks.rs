//! Layer-hook trait. Phase 4 wires steering + introspection hooks against
//! this surface. v1 ships an `IdentityHook` for testing.

use candle_core::Tensor;

pub trait LayerHook: Send + Sync {
    /// Apply a transformation to the residual stream at `layer_idx`. v1's
    /// quantized_qwen3 forward path does not call this — the hook is held for
    /// Phase 3+ wiring and the Phase 0.3 spike uses an alternative
    /// embedding-bias path documented in `decisions/0002-spike-validation.md`.
    fn apply(&self, layer_idx: usize, residual: &Tensor) -> candle_core::Result<Tensor>;
}

pub struct IdentityHook;

impl LayerHook for IdentityHook {
    fn apply(&self, _layer_idx: usize, residual: &Tensor) -> candle_core::Result<Tensor> {
        Ok(residual.clone())
    }
}
