//! Cheap-model history summarize for F-5 phase-2c recovery
//! (`harness-recovery-context-overflow`).
//!
//! When the dispatcher returns a `FailureClass::ContextOverflow` (the
//! provider's wire body matched a known context-window phrase), the
//! harness compresses the conversation history through this module's
//! cheap-model dispatch and re-calls the original dispatch once with
//! the summarized transcript. The seam lives here (rather than inline
//! in `execute.rs`) so the prompt + truncation rules are unit-testable
//! in isolation and so future improvements (token-accurate counting,
//! per-strategy summarize prompts, summary caching) can land as edits
//! to a single module.
//!
//! ## Contract guarantees (acceptance criteria from the contract)
//!
//! - System prompt is preserved verbatim — when callers prepend it to
//!   the request, this module never touches it.
//! - The latest user message (the most recent verbatim turn from the
//!   tail of the transcript) survives the summarize pass intact.
//! - Hard input cap of 2000 tokens (char/4 heuristic); when exceeded,
//!   the OLDEST turns are summarized while the most recent turns stay
//!   verbatim.
//! - Summary preserves: proper names, numeric quantities, explicit
//!   risk constraints. Drops chain-of-thought.
//! - One retry only; if the second attempt also overflows, the harness
//!   emits `recovery.failed` and surfaces the second error.
//! - Catalog-driven cheap-model selection (lowest
//!   `pricing_per_million_input_usd`); when no catalog is supplied or
//!   no model has pricing, returns `Ok(None)` so the caller falls back
//!   to surfacing the original error.

use std::sync::Arc;

use crate::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, Message};
use xvision_core::providers::{Catalog, ModelEntry};

/// Hard cap on input tokens for the summarize prompt itself. Picked
/// from the contract: char/4 heuristic, 2000 token budget. The wire
/// max_tokens on the summarize dispatch is sized so the response stays
/// under ~800 tokens (≤ the contract's "≤ 1500 token summary block"
/// ceiling with comfortable headroom).
pub const SUMMARIZE_INPUT_TOKEN_CAP: u32 = 2000;
/// Maximum output tokens for the summarize dispatch. The contract
/// caps the rendered summary block at ≤ 1500 tokens; we ask for 800
/// to leave the model some room for the framing prose without
/// blowing past the cap.
pub const SUMMARIZE_OUTPUT_TOKEN_CAP: u32 = 800;

/// Char/4 token-count heuristic. The contract explicitly excludes
/// tokenizer dependencies: "char/4 heuristic only".
fn estimated_tokens(s: &str) -> u32 {
    s.chars().count().div_ceil(4) as u32
}

/// Sum the estimated tokens of every text/tool_result block in a
/// message. Tool_use input is serialized to JSON for the count so
/// large tool-call payloads contribute proportionally.
fn estimated_message_tokens(m: &Message) -> u32 {
    let mut total = 0u32;
    for block in &m.content {
        match block {
            ContentBlock::Text { text } => total = total.saturating_add(estimated_tokens(text)),
            ContentBlock::ToolUse { input, name, .. } => {
                let s = serde_json::to_string(input).unwrap_or_default();
                total = total
                    .saturating_add(estimated_tokens(&s))
                    .saturating_add(estimated_tokens(name));
            }
            ContentBlock::ToolResult { content, .. } => {
                total = total.saturating_add(estimated_tokens(content));
            }
        }
    }
    total
}

/// The summarize prompt. Hardcoded for v1 — the contract explicitly
/// rules out per-strategy customization ("if operators ask for
/// customization later it's a separate spec").
pub const SUMMARIZE_SYSTEM_PROMPT: &str =
    "You are summarizing the middle of a trading-agent conversation so it can be \
     re-fed under a tighter context budget. Constraints:\n\
     - PRESERVE: proper names (symbols, model ids, broker ids, tool names), \
       numeric quantities (prices, sizes, percentages, dates), and explicit \
       risk constraints (caps, max-drawdown, leverage).\n\
     - DROP: chain-of-thought, hedging language, restated prompts, pleasantries.\n\
     - LENGTH: ≤ 1500 tokens. Prefer concise factual bullet points over prose.\n\
     - FORMAT: start with one line `[history summarized]`, then bullets. Do not \
       fabricate facts not present in the source.";

/// Result of one summarize call. The synthetic message is ready to
/// splice into the conversation in place of the truncated middle; the
/// caller is responsible for preserving the system prompt and the
/// recent verbatim turns.
#[derive(Debug, Clone)]
pub struct SummarizedHistory {
    /// Synthetic user message carrying the summary. Role is `"user"`
    /// because Anthropic/OpenAI-compat both require the conversation
    /// to alternate user/assistant after the system block, and
    /// injecting a synthetic assistant turn would make the model
    /// think it had already responded.
    pub summary_message: Message,
    /// Number of messages that were folded into the summary (i.e.
    /// dropped from the original middle of the transcript). When 0 the
    /// caller should treat the result as a no-op (history was already
    /// under cap).
    pub summarized_count: usize,
}

/// Split a conversation history into `(prefix_to_summarize,
/// recent_verbatim)`. The contract says: preserve the latest user
/// message verbatim (bottom of transcript). For multi-turn tool-use
/// loops the "latest user message" is usually a `ToolResult` carrier;
/// we keep the trailing tail of the transcript starting from the last
/// `role=="user"` block so the model sees its most recent input
/// without modification.
fn split_for_summarize(history: &[Message]) -> (Vec<Message>, Vec<Message>) {
    if history.is_empty() {
        return (Vec::new(), Vec::new());
    }
    // Find the index of the last `role == "user"` message; everything
    // from there to the end is "recent verbatim". If no user message
    // is present (unusual but possible in degenerate fixtures), keep
    // the final message only.
    let last_user = history
        .iter()
        .enumerate()
        .rev()
        .find(|(_, m)| m.role == "user")
        .map(|(i, _)| i)
        .unwrap_or(history.len().saturating_sub(1));
    let (prefix, tail) = history.split_at(last_user);
    (prefix.to_vec(), tail.to_vec())
}

/// Truncate the prefix from the OLDEST end until the prefix's
/// estimated token count drops below `cap`. Returns the truncated
/// prefix and the number of messages dropped from the head.
fn truncate_prefix_to_budget(prefix: Vec<Message>, cap: u32) -> (Vec<Message>, usize) {
    let mut total: u32 = prefix
        .iter()
        .map(estimated_message_tokens)
        .fold(0u32, |a, b| a.saturating_add(b));
    if total <= cap {
        return (prefix, 0);
    }
    let mut deque: std::collections::VecDeque<Message> = prefix.into();
    let mut dropped = 0usize;
    while total > cap && !deque.is_empty() {
        if let Some(m) = deque.pop_front() {
            total = total.saturating_sub(estimated_message_tokens(&m));
            dropped += 1;
        }
    }
    (Vec::from(deque), dropped)
}

/// Render a conversation prefix as a single human-readable string for
/// the cheap-model summarizer to consume. We deliberately do NOT
/// pretty-print the full JSON tool inputs — those balloon the prompt
/// and rarely carry decision-relevant detail. The serializer trims
/// tool inputs to 240 chars per call.
fn render_prefix_for_summarize(prefix: &[Message]) -> String {
    let mut out = String::with_capacity(
        prefix
            .iter()
            .map(|m| estimated_message_tokens(m) as usize * 4)
            .sum(),
    );
    for (i, m) in prefix.iter().enumerate() {
        out.push_str(&format!("\n--- turn {} ({}):\n", i, m.role));
        for block in &m.content {
            match block {
                ContentBlock::Text { text } => {
                    out.push_str(text);
                    out.push('\n');
                }
                ContentBlock::ToolUse { name, input, .. } => {
                    let body = serde_json::to_string(input).unwrap_or_default();
                    let body: String = body.chars().take(240).collect();
                    out.push_str(&format!("[tool_use {name}] {body}\n"));
                }
                ContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    let marker = if matches!(is_error, Some(true)) {
                        "[tool_result is_error=true]"
                    } else {
                        "[tool_result]"
                    };
                    let body: String = content.chars().take(240).collect();
                    out.push_str(&format!("{marker} {body}\n"));
                }
            }
        }
    }
    out
}

/// Pick the cheapest model from the catalog by
/// `pricing_per_million_input_usd`. Returns `None` when the catalog
/// is empty or no entry has pricing data (most OpenAI-compat
/// providers don't expose it via `/v1/models`).
pub fn pick_cheap_model(catalog: &Catalog) -> Option<&ModelEntry> {
    catalog
        .models
        .iter()
        .filter(|m| m.pricing_per_million_input_usd.is_some())
        .min_by(|a, b| {
            a.pricing_per_million_input_usd
                .unwrap()
                .partial_cmp(&b.pricing_per_million_input_usd.unwrap())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
}

/// Compress a conversation history into a single synthetic user
/// message ready to splice in front of the recent verbatim turns.
///
/// - `history`: the conversation up to the point of failure. The
///   caller passes only the assistant/user transcript — the system
///   prompt is OWNED by the caller and never seen here.
/// - `dispatch`: the cheap-model dispatch seam. The contract says the
///   summarizer "uses the cheapest model from the catalog"; when no
///   catalog model is usable, the caller short-circuits before calling
///   us. We accept the dispatch as a trait object so tests can inject
///   a fake without spinning up an HTTP server.
/// - `model_id`: the cheap-model id to send on the request.
///
/// Returns `Ok(SummarizedHistory)` on success. The synthetic
/// `summary_message` carries the cheap-model's compressed bullets;
/// `summarized_count` is the number of messages folded in (0 means
/// the history was already short enough that the summary is a
/// passthrough — the caller should still re-feed the unmodified
/// history rather than the summary).
pub async fn summarize_history(
    history: &[Message],
    dispatch: Arc<dyn LlmDispatch>,
    model_id: &str,
) -> anyhow::Result<SummarizedHistory> {
    if history.is_empty() {
        return Ok(SummarizedHistory {
            summary_message: Message::user_text("[history summarized] (empty conversation)"),
            summarized_count: 0,
        });
    }
    let (prefix, _recent_tail) = split_for_summarize(history);
    let prefix_tokens: u32 = prefix
        .iter()
        .map(estimated_message_tokens)
        .fold(0u32, |a, b| a.saturating_add(b));
    // If the prefix is under cap and is the entire prefix, we still
    // run the summarize — the caller invoked us *because* the provider
    // returned ContextOverflow on the full transcript. The cap shapes
    // the summarizer's INPUT budget (so the cheap-model call itself
    // doesn't overflow); it doesn't gate whether we summarize at all.
    let (truncated_prefix, dropped_from_head) = truncate_prefix_to_budget(prefix, SUMMARIZE_INPUT_TOKEN_CAP);
    let rendered = render_prefix_for_summarize(&truncated_prefix);
    let summarized_count = truncated_prefix.len() + dropped_from_head;

    let req = LlmRequest {
        model: model_id.to_string(),
        system_prompt: SUMMARIZE_SYSTEM_PROMPT.to_string(),
        messages: vec![Message::user_text(format!(
            "Summarize the conversation history below per the system prompt rules. Source \
             estimated_tokens={prefix_tokens} dropped_from_head={dropped_from_head}.\n\n{rendered}"
        ))],
        max_tokens: Some(SUMMARIZE_OUTPUT_TOKEN_CAP),
        tools: Vec::new(),
        temperature: Some(0.0),
        response_schema: None,
        cache_control: None,
        force_json: false,
    };
    let resp = dispatch.complete(req).await?;
    let summary_text = resp
        .content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("");
    let summary_text = if summary_text.trim().is_empty() {
        "[history summarized] (no content returned)".to_string()
    } else if summary_text.starts_with("[history summarized]") {
        summary_text
    } else {
        format!("[history summarized]\n{summary_text}")
    };
    Ok(SummarizedHistory {
        summary_message: Message::user_text(summary_text),
        summarized_count,
    })
}

/// Build the rewritten conversation history that the harness sends on
/// the retry: the synthetic summary message followed by the recent
/// verbatim tail of the original history. The system prompt is the
/// caller's responsibility — they hand it to the dispatcher
/// alongside this `Vec<Message>`. This split keeps the function pure
/// and unit-testable.
pub fn build_summarized_messages(original: &[Message], summary: &SummarizedHistory) -> Vec<Message> {
    let (_prefix, recent_tail) = split_for_summarize(original);
    let mut out = Vec::with_capacity(1 + recent_tail.len());
    out.push(summary.summary_message.clone());
    out.extend(recent_tail);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{LlmResponse, StopReason};
    use async_trait::async_trait;
    use std::sync::Mutex;
    use xvision_core::providers::ModelEntry;

    fn user(t: &str) -> Message {
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: t.into() }],
        }
    }
    fn assistant(t: &str) -> Message {
        Message {
            role: "assistant".into(),
            content: vec![ContentBlock::Text { text: t.into() }],
        }
    }

    struct FakeDispatch {
        seen: Mutex<Vec<LlmRequest>>,
        reply: String,
    }

    #[async_trait]
    impl LlmDispatch for FakeDispatch {
        async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
            self.seen.lock().unwrap().push(req);
            Ok(LlmResponse {
                content: vec![ContentBlock::Text {
                    text: self.reply.clone(),
                }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 10,
                output_tokens: 5,
            })
        }
    }

    #[test]
    fn estimated_tokens_uses_char_div_4() {
        assert_eq!(estimated_tokens(""), 0);
        // 4 chars → 1 token; 5 chars → 2 tokens (ceil).
        assert_eq!(estimated_tokens("abcd"), 1);
        assert_eq!(estimated_tokens("abcde"), 2);
    }

    #[test]
    fn split_for_summarize_keeps_last_user_message_in_tail() {
        let history = vec![user("hello"), assistant("hi"), user("decide on BTC at $50k")];
        let (prefix, tail) = split_for_summarize(&history);
        assert_eq!(prefix.len(), 2);
        assert_eq!(tail.len(), 1);
        // The last user message is preserved verbatim in the tail.
        if let ContentBlock::Text { text } = &tail[0].content[0] {
            assert!(text.contains("BTC"));
        } else {
            panic!("expected text block");
        }
    }

    #[test]
    fn empty_history_returns_empty_summary() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let dispatch = Arc::new(FakeDispatch {
            seen: Mutex::new(Vec::new()),
            reply: "should-not-be-called".into(),
        });
        let result = rt
            .block_on(summarize_history(&[], dispatch.clone(), "cheap"))
            .unwrap();
        assert_eq!(result.summarized_count, 0);
        // Dispatch was NOT called for empty input.
        assert!(dispatch.seen.lock().unwrap().is_empty());
    }

    #[test]
    fn over_cap_history_truncates_oldest_messages() {
        // Force a tight cap by making each message large enough that
        // the SUMMARIZE_INPUT_TOKEN_CAP forces truncation. Each "x"*8000
        // is ~2000 tokens.
        let big = "x".repeat(8000);
        let history = vec![
            user(&big), // tokens >> cap on its own
            assistant(&big),
            user(&big),
            user("final"),
        ];
        let (prefix, _) = split_for_summarize(&history);
        let (truncated, dropped) = truncate_prefix_to_budget(prefix, SUMMARIZE_INPUT_TOKEN_CAP);
        // At minimum the oldest one or two messages should drop.
        assert!(dropped >= 1, "must drop oldest to meet budget; dropped={dropped}");
        let final_tokens: u32 = truncated
            .iter()
            .map(estimated_message_tokens)
            .fold(0u32, |a, b| a.saturating_add(b));
        assert!(
            final_tokens <= SUMMARIZE_INPUT_TOKEN_CAP || truncated.is_empty(),
            "truncated prefix must respect cap or empty; got {final_tokens}"
        );
    }

    #[test]
    fn pick_cheap_model_returns_lowest_priced_entry() {
        let cat = Catalog::new(
            "test",
            "x",
            vec![
                ModelEntry {
                    id: "expensive".into(),
                    pricing_per_million_input_usd: Some(10.0),
                    ..ModelEntry::minimal("expensive")
                },
                ModelEntry {
                    id: "cheap".into(),
                    pricing_per_million_input_usd: Some(0.5),
                    ..ModelEntry::minimal("cheap")
                },
                ModelEntry {
                    id: "no-pricing".into(),
                    ..ModelEntry::minimal("no-pricing")
                },
            ],
        );
        assert_eq!(pick_cheap_model(&cat).map(|m| m.id.as_str()), Some("cheap"));
    }

    #[test]
    fn pick_cheap_model_returns_none_when_no_pricing() {
        let cat = Catalog::new(
            "test",
            "x",
            vec![ModelEntry::minimal("a"), ModelEntry::minimal("b")],
        );
        assert!(pick_cheap_model(&cat).is_none());
    }

    #[test]
    fn build_summarized_messages_concatenates_summary_and_tail() {
        let history = vec![user("first"), assistant("middle"), user("final question")];
        let summary = SummarizedHistory {
            summary_message: Message::user_text("[history summarized] keep BTC focus"),
            summarized_count: 2,
        };
        let out = build_summarized_messages(&history, &summary);
        assert_eq!(out.len(), 2); // summary + tail (the final user message)
        assert!(matches!(out[0].content[0], ContentBlock::Text { .. }));
        if let ContentBlock::Text { text } = &out[1].content[0] {
            assert_eq!(text, "final question");
        } else {
            panic!("tail must be the final user message verbatim");
        }
    }
}
