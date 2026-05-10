//! Stage 2 — LLM-driven trader.
//!
//! The Trader receives an [`InternBriefing`] and a [`PortfolioState`] and emits
//! a [`TraderDecision`]. After CV extraction (ADR 0011) the Trader is a vanilla
//! LLM caller against an OpenAI-compatible HTTP backend — no candle, no
//! steering hooks. The trait surface mirrors `xvision_intern::InternBackend`.
//!
//! Schema robustness: schema-validate with one corrective retry on parse
//! failure. Validation failures (range / length violations) are NOT retried —
//! we surface them loudly.

pub mod backend;
pub mod error;
pub mod params;
pub mod parse;
pub mod prompt;
pub mod run;

pub use backend::{OpenAiCompatBackend, TraderBackend};
pub use error::TraderError;
pub use params::TraderParams;
pub use parse::parse_trader_response;
pub use prompt::{build_trader_prompt, TraderPromptOpts};
pub use run::{preview_prompt, run_trader};
