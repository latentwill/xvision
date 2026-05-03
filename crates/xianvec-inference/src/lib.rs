//! xianvec-inference — candle wrapper, steering hooks, NPZ vector loader.
//!
//! v1 surface:
//! - `engine::Qwen3Engine` — load a Q4 GGUF + tokenizer, run forward passes.
//! - `hooks::LayerHook` — trait with `Context` (Tier 1 fix #5); v1 hooks are
//!   no-ops pending per-layer residual injection (see engine.rs).
//! - `substrate::load_vector` — Phase 4.3 NPZ loader with manifest validation.

pub mod engine;
pub mod hooks;
pub mod substrate;
pub mod vendor_qwen3;

pub use engine::{EngineError, Qwen3Engine};
pub use hooks::{HookContext, IdentityHook, LayerHook};
pub use substrate::{SubstrateError, VectorBundle};
