//! Stage 1 Intern — emits balanced bull/bear/flat case briefings.
//!
//! The Intern receives a [`MarketSnapshot`] and writes an
//! [`InternBriefing`]. It must NOT recommend a direction.
//!
//! Backends speak either OpenAI-compat or Anthropic wire formats; replay
//! determinism is handled by the eval/trajectory layer.

pub mod backend;
pub mod prompt;
pub mod reasoning;

pub use backend::{AnthropicIntern, InternBackend, InternError, OpenAICompatIntern};
pub use prompt::{build_intern_prompt, PromptOpts};
pub use reasoning::strip_reasoning;
