//! Stage 1 Intern — emits balanced bull/bear/flat case briefings.
//!
//! The Intern receives a [`MarketSnapshot`] and writes an
//! [`InternBriefing`]. It must NOT recommend a direction; the steering
//! vectors live in Stage 2 and want a clean surface to operate on.
//!
//! Backends speak either OpenAI or Anthropic wire formats. The cache layer
//! ensures that paired arms (vectors-on / vectors-off) read the SAME briefing
//! per `setup_id` (Tier 1 fix #1).

pub mod backend;
pub mod cache;
pub mod prompt;
pub mod reasoning;

pub use backend::{AcpxIntern, AnthropicIntern, InternBackend, InternError, OpenAICompatIntern};
pub use cache::BriefingCache;
pub use prompt::{build_intern_prompt, PromptOpts};
pub use reasoning::strip_reasoning;
