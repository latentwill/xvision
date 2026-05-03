//! Stage 2 — local Qwen3 trader.
//!
//! The Trader receives an [`InternBriefing`] and a [`PortfolioState`] and emits
//! a [`TraderDecision`]. v1 wraps `xianvec-inference::Qwen3Engine` (Q4 GGUF via
//! candle); Phase 4 will install steering hooks on the same engine surface.
//!
//! v1 chooses **schema-validate with one corrective retry** over grammar-
//! constrained generation: candle's `quantized_qwen3` does not currently expose
//! token-level masking, and the intern crate's experience shows JSON-only
//! prompts plus a corrective retry hit the 95% / 99% acceptance bar without
//! the constrained-decoding plumbing.

pub mod error;
pub mod params;
pub mod parse;
pub mod prompt;
pub mod run;

pub use error::TraderError;
pub use params::TraderParams;
pub use parse::parse_trader_response;
pub use prompt::{build_trader_prompt, TraderPromptOpts};
pub use run::run_trader;
