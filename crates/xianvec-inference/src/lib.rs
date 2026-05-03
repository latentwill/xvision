//! xianvec-inference — candle wrapper, steering hooks, FAISS vector load.
//!
//! v1 surface:
//! - `engine::Qwen3Engine` — load a Q4 GGUF + tokenizer, run forward passes,
//!   stream sampled tokens.
//! - `hooks::LayerHook` — trait carved out so the Phase 4 `SteeringHook` and
//!   `IntrospectionHook` can drop in without an API break. v1 hooks are
//!   no-ops since candle's `quantized_qwen3::ModelWeights` does not expose
//!   per-layer residual mutation. The Phase 0.3 spike applies the steering
//!   shift via the input-embedding bias path described in
//!   `decisions/0002-spike-validation.md`.
//! - `substrate::load_vector` — Phase 4.3 sync FAISS loader (stub here).

pub mod engine;
pub mod hooks;
pub mod substrate;

pub use engine::{EngineError, Qwen3Engine};
pub use hooks::{IdentityHook, LayerHook};
