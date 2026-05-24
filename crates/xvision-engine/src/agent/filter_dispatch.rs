//! Phase C — LLM Filter dispatcher.
//!
//! Given an `AgentSlot` with `Capability::Filter` and the briefing JSON,
//! runs the slot's LLM with an output-schema constraint forcing the
//! model to return a `FilterSignal`-shaped JSON of `{ name, payload,
//! granularity }`. The dispatcher then parses + validates the response
//! via `serde_json` with `deny_unknown_fields`. Malformed output emits
//! a `FilterParseError` observability event and bubbles up a typed
//! error so the caller can decide how to surface it (the contract
//! resolution: propagate `None` into the signal map; downstream edges
//! evaluate against the missing signal and predicate returns false —
//! edges do not fire).
//!
//! See `team/contracts/agent-graph-filter-capability.md` for the
//! authoritative behavior.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::agent::dispatch_capability::{DispatchInput, FilterGranularity, FilterSignal};
use crate::agent::execute::{execute_slot, SlotInput};
use crate::agent::llm::{LlmResponse, ResponseSchema};
use crate::agent::observability::ObsEmitter;

#[derive(Debug)]
pub struct FilterDispatchResult {
    pub signal: FilterSignal,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[derive(Debug, thiserror::Error)]
pub enum FilterDispatchError {
    #[error(transparent)]
    Dispatch(#[from] anyhow::Error),
    #[error("filter output parse failed: {0}")]
    Parse(anyhow::Error),
}

/// The strict JSON shape we ask the model to return. `deny_unknown_fields`
/// means a model that adds extra keys is treated as a parse error — the
/// dispatcher refuses to silently ignore drift.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FilterLlmResponse {
    name: String,
    payload: serde_json::Value,
    #[serde(default)]
    granularity: FilterGranularity,
}

/// Run an LLM Filter slot end-to-end: wrap the slot's `system_prompt`
/// with the schema constraint, dispatch through `execute_slot`, parse
/// the response, and return a typed `FilterSignal` whose `ts` is the
/// bar timestamp the dispatcher was called with.
///
/// On parse failure, the dispatcher emits a `filter_parse_error`
/// observability event with the truncated raw text and bubbles up the
/// error so the pipeline's caller can decide whether to propagate
/// `None` into the signal map.
pub async fn run_llm_filter(input: DispatchInput<'_>) -> Result<FilterDispatchResult, FilterDispatchError> {
    let role = input.resolved.role.clone();
    let obs = input.obs.clone();
    let bar_ts = input.scenario_start.unwrap_or_else(Utc::now);

    // Wrap the operator's prompt with the schema constraint. The
    // operator-supplied prompt remains the leading content; the schema
    // hint appends a contract block matching the rest of the engine's
    // schema-constrained dispatches (Router / Trader). This keeps the
    // prompt prefix stable across runs so prompt-cache prefixes hit.
    let schema = filter_response_schema();
    let system_prompt = if input.system_prompt.is_empty() {
        filter_prompt_contract()
    } else {
        format!("{}\n\n{}", input.system_prompt, filter_prompt_contract())
    };

    let resp = execute_slot(SlotInput {
        slot: input.slot,
        system_prompt,
        upstream_inputs: input.upstream_inputs,
        dispatch: input.dispatch,
        tools: input.tools,
        response_schema: Some(schema),
        max_tokens: input.max_tokens,
        temperature: input.temperature,
        obs: obs.clone(),
        memory: input.memory,
        memory_mode: input.memory_mode,
        agent_id: input.agent_id,
        scenario_start: input.scenario_start,
        run_id: input.run_id,
        scenario_id: input.scenario_id,
        cycle_idx: input.cycle_idx,
        catalog: input.catalog,
        delta_briefing: input.delta_briefing,
        prev_briefing: input.prev_briefing,
    })
    .await?;

    let input_tokens = resp.input_tokens;
    let output_tokens = resp.output_tokens;
    match parse_filter_response(&resp, &role, bar_ts) {
        Ok(signal) => Ok(FilterDispatchResult {
            signal,
            input_tokens,
            output_tokens,
        }),
        Err(e) => {
            emit_parse_error(obs.as_ref(), &role, &resp, &e).await;
            Err(FilterDispatchError::Parse(e))
        }
    }
}

/// Pin the Filter's JSON Schema response shape. The schema is closed
/// (`additionalProperties: false`) so providers that respect strict
/// JSON Schema reject extra keys at the wire layer; the engine-side
/// `serde_json::Deserialize` with `deny_unknown_fields` is the fallback
/// check for providers that don't.
fn filter_response_schema() -> ResponseSchema {
    ResponseSchema {
        name: "filter_output".to_string(),
        schema: serde_json::json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["name", "payload"],
            "properties": {
                "name": { "type": "string", "minLength": 1 },
                "payload": { "type": "object" },
                "granularity": {
                    "type": "string",
                    "enum": ["bar", "minute", "decision"],
                }
            }
        }),
    }
}

/// Plain-text prompt suffix appended to the operator's system_prompt.
/// We keep this as a small constant so prompt prefixes stay stable —
/// any tweak here invalidates upstream prompt caches.
fn filter_prompt_contract() -> String {
    "You are a Filter. Emit exactly one JSON object matching the response \
     schema: {\"name\": <string>, \"payload\": <object>, \"granularity\": \
     \"bar\" | \"minute\" | \"decision\"}. No markdown, no prose. The \
     `payload` is the structured signal downstream agents read via edge \
     predicates."
        .to_string()
}

/// Parse a filter LLM response into a typed `FilterSignal`. Failures
/// carry the raw response text (truncated) so the upstream error event
/// is debuggable without leaking the full body to logs.
fn parse_filter_response(resp: &LlmResponse, role: &str, ts: DateTime<Utc>) -> anyhow::Result<FilterSignal> {
    let text = resp.text();
    let parsed: FilterLlmResponse = serde_json::from_str(&text).map_err(|e| {
        anyhow::anyhow!("filter output is not valid JSON (role={role}, error={e}, raw={text:.200})",)
    })?;
    Ok(FilterSignal {
        name: if parsed.name.trim().is_empty() {
            role.to_string()
        } else {
            parsed.name
        },
        payload: parsed.payload,
        granularity: parsed.granularity,
        ts,
        scope: crate::agent::dispatch_capability::SignalScope::Global,
    })
}

/// Emit a `filter_parse_error` engine event so the trace surfaces the
/// failure even when the caller swallows the `Err` and propagates
/// `None` into the signal map. The payload carries the role and the
/// truncated raw response text — enough context to diagnose the drift
/// in `xvn trace`.
async fn emit_parse_error(obs: Option<&ObsEmitter>, role: &str, resp: &LlmResponse, err: &anyhow::Error) {
    if let Some(emitter) = obs {
        let payload = serde_json::json!({
            "role": role,
            "error": err.to_string(),
            "raw_text": truncate(&resp.text(), 200),
        });
        emitter
            .emit_engine_event("filter_parse_error", None, Some(payload.to_string()))
            .await;
    }
}

fn truncate(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n.min(s.len())])
    }
}

/// Granularity-fallback event payload. The dispatcher's caller emits
/// this when the Filter's declared granularity is `Minute` but the
/// scenario's bar period is `> 1` minute — the runtime degrades to
/// `Bar` and the trace records the demotion.
pub async fn emit_granularity_fallback(obs: Option<&ObsEmitter>, role: &str, bar_period_minutes: u32) {
    if let Some(emitter) = obs {
        let payload = serde_json::json!({
            "role": role,
            "requested": "minute",
            "effective": "bar",
            "reason": "bar_period_exceeds_granularity",
            "bar_period_minutes": bar_period_minutes,
        });
        emitter
            .emit_engine_event("granularity_fallback", None, Some(payload.to_string()))
            .await;
    }
}

/// Multi-Filter cardinality knob. Phase C ships a configurable
/// threshold (operator Q3 resolution 2026-05-22) gating whether a
/// cycle's Filter signals coalesce into a single Trader briefing
/// (short bars: `period_minutes < threshold`) or fan out into one
/// Trader invocation per emitting Filter (long bars: `period_minutes
/// >= threshold`).
///
/// The default is `30`: short bars (5m / 15m) coalesce so the Trader
/// doesn't burn token budget re-running on every Filter; long bars
/// (30m / 1h / 1d) fan out so each Filter's signal gets a fresh
/// Trader read.
#[derive(Debug, Clone, Copy)]
pub struct MultiFilterConfig {
    pub multi_fire_bar_threshold_minutes: u32,
}

impl Default for MultiFilterConfig {
    fn default() -> Self {
        Self {
            multi_fire_bar_threshold_minutes: 30,
        }
    }
}

impl MultiFilterConfig {
    /// Returns `true` when the cycle should run the Trader once per
    /// emitting Filter (long-bar regime). When `false`, all Filter
    /// signals coalesce into a single Trader briefing.
    pub fn should_multi_fire(&self, bar_period_minutes: u32) -> bool {
        bar_period_minutes >= self.multi_fire_bar_threshold_minutes
    }
}

// `Arc<MultiFilterConfig>` is exported as a convenience handle so the
// executor can build the config once at startup and clone the Arc into
// every per-cycle dispatch — Arc-cloning is cheaper than value-cloning
// the struct, and the config is immutable for the lifetime of the run.
pub type MultiFilterConfigHandle = Arc<MultiFilterConfig>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::llm::{ContentBlock, StopReason};

    fn resp(text: &str) -> LlmResponse {
        LlmResponse {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 0,
            output_tokens: 0,
        }
    }

    #[test]
    fn parse_filter_response_accepts_minimal_payload() {
        let r = resp(r#"{"name":"regime_filter","payload":{"regime":"trend"}}"#);
        let ts = Utc::now();
        let sig = parse_filter_response(&r, "regime_filter", ts).unwrap();
        assert_eq!(sig.name, "regime_filter");
        assert_eq!(sig.payload, serde_json::json!({"regime": "trend"}));
        // Granularity omitted → default `Bar` (Serde default = enum's `Default`).
        assert_eq!(sig.granularity, FilterGranularity::Bar);
        assert_eq!(sig.ts, ts);
    }

    #[test]
    fn parse_filter_response_reads_granularity_when_present() {
        let r = resp(r#"{"name":"f","payload":{"a":1},"granularity":"minute"}"#);
        let sig = parse_filter_response(&r, "f", Utc::now()).unwrap();
        assert_eq!(sig.granularity, FilterGranularity::Minute);
    }

    #[test]
    fn parse_filter_response_rejects_unknown_field() {
        let r = resp(r#"{"name":"f","payload":{},"granularity":"bar","extra":1}"#);
        let err = parse_filter_response(&r, "f", Utc::now()).unwrap_err();
        assert!(
            err.to_string().contains("unknown field") || err.to_string().contains("not valid JSON"),
            "got: {err}"
        );
    }

    #[test]
    fn parse_filter_response_rejects_non_json() {
        let r = resp("nope");
        let err = parse_filter_response(&r, "f", Utc::now()).unwrap_err();
        assert!(err.to_string().contains("not valid JSON"), "got: {err}");
    }

    #[test]
    fn parse_filter_response_falls_back_to_role_when_name_empty() {
        let r = resp(r#"{"name":"","payload":{}}"#);
        let sig = parse_filter_response(&r, "vol_filter", Utc::now()).unwrap();
        // Defensive: empty `name` field falls back to the slot role so
        // downstream `filter_signals[name]` keys never collide on "".
        assert_eq!(sig.name, "vol_filter");
    }

    #[test]
    fn multi_filter_config_default_threshold_is_30() {
        let c = MultiFilterConfig::default();
        assert_eq!(c.multi_fire_bar_threshold_minutes, 30);
    }

    #[test]
    fn multi_filter_below_threshold_coalesces() {
        let c = MultiFilterConfig::default();
        assert!(!c.should_multi_fire(5));
        assert!(!c.should_multi_fire(15));
        assert!(!c.should_multi_fire(29));
    }

    #[test]
    fn multi_filter_at_or_above_threshold_fans_out() {
        let c = MultiFilterConfig::default();
        assert!(c.should_multi_fire(30));
        assert!(c.should_multi_fire(60));
        assert!(c.should_multi_fire(1440));
    }

    #[test]
    fn multi_filter_threshold_zero_forces_multi_fire_everywhere() {
        let c = MultiFilterConfig {
            multi_fire_bar_threshold_minutes: 0,
        };
        // Acceptance: `multi_fire_bar_threshold_minutes = 0` forces
        // multi-fire even on short bars.
        assert!(c.should_multi_fire(5));
        assert!(c.should_multi_fire(1));
    }
}
