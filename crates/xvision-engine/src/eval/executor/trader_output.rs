use std::fmt;

use serde::{Deserialize, Serialize};

use crate::agent::llm::{LlmResponse, StopReason};

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraderOutput {
    pub(crate) action: String,
    /// Trader's confidence in the decision, 0.0–1.0.
    ///
    /// Ollama-hosted small models consistently omit this field while making
    /// sound action/justification calls. Defaulting to 0.5 (neutral/medium
    /// conviction) accepts the decision without a wasted schema-patch repair
    /// round-trip that the same model would also fail.
    #[serde(default = "default_conviction")]
    pub(crate) conviction: f64,
    pub(crate) justification: String,
    #[serde(default)]
    pub(crate) stop_loss_pct: Option<f32>,
    #[serde(default)]
    pub(crate) take_profit_pct: Option<f32>,
    #[serde(default)]
    pub(crate) trailing_stop_pct: Option<f64>,
    #[serde(default)]
    pub(crate) breakeven_trigger_pct: Option<f64>,
    #[serde(default)]
    pub(crate) breakeven_offset_pct: Option<f64>,
    #[serde(default)]
    pub(crate) fade_sl_bars: Option<u32>,
    #[serde(default)]
    pub(crate) fade_sl_start_pct: Option<f64>,
    #[serde(default)]
    pub(crate) fade_sl_end_pct: Option<f64>,
    #[serde(default)]
    pub(crate) max_bars_held: Option<u32>,
    #[serde(default)]
    pub(crate) sl_atr_mult: Option<f64>,
    #[serde(default)]
    pub(crate) tp_atr_mult: Option<f64>,
    #[serde(default)]
    pub(crate) tp1_pct: Option<f64>,
    #[serde(default)]
    pub(crate) tp1_close_fraction: Option<f64>,
    #[serde(default)]
    pub(crate) tp2_pct: Option<f64>,
    /// Set when the parser coerced a non-canonical `action` to a canonical
    /// one (R1). `(from, to)`. Not part of the wire schema — `#[serde(skip)]`
    /// keeps `deny_unknown_fields` happy and `Default` leaves it `None`. The
    /// eval executor reads this after a successful parse to emit a non-fatal
    /// `trader_output_action_coerced` event.
    #[serde(skip)]
    pub(crate) coerced_action: Option<(String, String)>,
}


/// Serde default for `conviction`. Ollama small models consistently omit
/// this field; defaulting to 0.5 (neutral/medium confidence) lets us accept
/// decisions that carry a valid `action` + `justification` without a wasted
/// schema-patch repair attempt the same model would also fail.
fn default_conviction() -> f64 {
    0.5
}
/// Outcome of mapping a raw trader `action` string onto the canonical
/// vocabulary (`long_open` / `short_open` / `flat` / `hold`).
enum ActionCoercion {
    /// Already canonical; leave action and conviction untouched.
    Canonical,
    /// A recognized synonym mapped to a canonical action — keep the model's
    /// conviction (its confidence in the *decision* still applies).
    Synonym(&'static str),
    /// Unrecognized value — map to `hold` and zero the conviction, since we
    /// cannot trust a confidence attached to an action we did not understand.
    Unknown,
}

/// Coerce a (lowercased) trader `action` to the canonical enum (R1). Local
/// reasoning models routinely emit out-of-enum actions (`skip`, `buy`,
/// `close`, `none`, …); coercing here — before `validate()` — keeps a bad
/// action from triggering an `InvalidField` skip or a paid schema-patch
/// retry, in every eval phase (primary, random-baseline, honesty canary).
fn coerce_action(action: &str) -> ActionCoercion {
    match action {
        "long_open" | "short_open" | "flat" | "hold" => ActionCoercion::Canonical,
        "skip" | "none" | "no_trade" | "wait" | "pass" | "stay" | "nothing" | "noop" => {
            ActionCoercion::Synonym("hold")
        }
        "buy" | "enter_long" | "open_long" | "long" => ActionCoercion::Synonym("long_open"),
        "sell" | "short" | "enter_short" => ActionCoercion::Synonym("short_open"),
        "close" | "exit" | "flat_all" | "close_all" => ActionCoercion::Synonym("flat"),
        _ => ActionCoercion::Unknown,
    }
}

/// Stable classification of trader-output failure modes. Persisted as part
/// of `eval_runs.error` via the `trader_output[<tag>]:` prefix on the
/// `TraderOutputError` Display, so review/UI consumers can grep the class
/// without parsing the full error message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TraderFailureKind {
    /// Provider returned a response with no text content (and no tool use).
    EmptyText,
    /// Response carries only ToolUse blocks; no final text trader payload.
    ToolUseOnly,
    /// Response stopped at `MaxTokens`; raw text was empty or unparseable.
    Truncated,
    /// Text was present but not valid JSON.
    InvalidJson,
    /// JSON parsed but a required field was missing.
    MissingField,
    /// Fields present but failed validation (unknown action, conviction out
    /// of range, empty justification, ...).
    InvalidField,
    /// The trader pipeline produced no response slot at all.
    MissingResponse,
}

impl TraderFailureKind {
    pub fn tag(self) -> &'static str {
        match self {
            Self::EmptyText => "empty",
            Self::ToolUseOnly => "tool_use_only",
            Self::Truncated => "truncated",
            Self::InvalidJson => "invalid_json",
            Self::MissingField => "missing_field",
            Self::InvalidField => "invalid_field",
            Self::MissingResponse => "missing_response",
        }
    }

    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag {
            "empty" => Some(Self::EmptyText),
            "tool_use_only" => Some(Self::ToolUseOnly),
            "truncated" => Some(Self::Truncated),
            "invalid_json" => Some(Self::InvalidJson),
            "missing_field" => Some(Self::MissingField),
            "invalid_field" => Some(Self::InvalidField),
            "missing_response" => Some(Self::MissingResponse),
            _ => None,
        }
    }
}

/// Typed trader-output failure carrying enough raw provider diagnostics to
/// distinguish empty / truncated / parser-failure cases at review time.
/// Display is stable: `run <id> decision <n>: trader_output[<tag>]: <detail>
/// (stop_reason=..., input_tokens=..., output_tokens=..., raw_excerpt=...)`.
#[derive(Debug, Clone)]
pub struct TraderOutputError {
    pub kind: TraderFailureKind,
    pub run_id: String,
    pub decision_index: u32,
    pub stop_reason: Option<StopReason>,
    pub input_tokens: u32,
    pub output_tokens: u32,
    /// First N characters of the raw provider text. `<no_response>` when the
    /// upstream pipeline produced no trader slot at all; `<empty>` when the
    /// response was present but text-empty.
    pub raw_excerpt: String,
    pub detail: String,
}

impl TraderOutputError {
    const RAW_EXCERPT_LIMIT: usize = 240;

    fn build(
        kind: TraderFailureKind,
        run_id: &str,
        decision_index: u32,
        response: Option<&LlmResponse>,
        raw_text: Option<&str>,
        detail: String,
    ) -> Self {
        let raw_excerpt = match raw_text {
            Some("") => "<empty>".to_string(),
            Some(text) => {
                let mut excerpt: String = text.chars().take(Self::RAW_EXCERPT_LIMIT).collect();
                if text.chars().count() > Self::RAW_EXCERPT_LIMIT {
                    excerpt.push('…');
                }
                excerpt
            }
            None => "<no_response>".to_string(),
        };
        Self {
            kind,
            run_id: run_id.to_string(),
            decision_index,
            stop_reason: response.map(|r| r.stop_reason),
            input_tokens: response.map(|r| r.input_tokens).unwrap_or(0),
            output_tokens: response.map(|r| r.output_tokens).unwrap_or(0),
            raw_excerpt,
            detail,
        }
    }

    /// Stable wire-format tag for this failure class. Persisted callers
    /// parse the `trader_output[<tag>]:` slice on `eval_runs.error`.
    pub fn class_tag(&self) -> &'static str {
        self.kind.tag()
    }

    /// Extract the offending JSON field name(s) from the structured
    /// `detail` text. F-5 phase 2b (`harness-recovery-schema-missing-field`)
    /// uses this to drive the targeted-patch repair: the dispatcher asks
    /// the model for just the bad fields rather than the full JSON shape.
    ///
    /// Recognised shapes (stable across the codebase's parser output;
    /// see `parse_with_response_inner` for the producers):
    ///
    /// - `MissingField`: serde's `missing field `<name>`` shape, plus the
    ///   `missing required trader field `action`` decorator we add for
    ///   the `action` case. Returns each matched name in order, deduped.
    /// - `InvalidField`: the `validate()` step produces three stable
    ///   detail strings — `"... action must be one of ..."`,
    ///   `"... conviction must be between 0 and 1 ..."`, and
    ///   `"... justification is required"`. Returns `["action"]` /
    ///   `["conviction"]` / `["justification"]` respectively.
    ///
    /// Returns an empty vec for any other kind (the caller should NOT
    /// invoke this for non-schema failures — `recovery::is_schema_missing_field_recoverable`
    /// gates the call).
    pub fn problem_fields(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        match self.kind {
            TraderFailureKind::MissingField => {
                // Walk `missing field `<name>`` occurrences. Multiple may
                // appear if serde aggregates (rare in practice but cheap
                // to handle defensively).
                extract_backticked_after(&self.detail, "missing field ", &mut out);
                extract_backticked_after(&self.detail, "missing required trader field ", &mut out);
            }
            TraderFailureKind::InvalidField => {
                // The `validate()` step in this module produces three
                // stable wordings; match each by the field name verbatim
                // because the wordings name the field directly (no
                // backticks around the field).
                if self.detail.contains("action must be one of") {
                    push_unique(&mut out, "action");
                }
                if self.detail.contains("conviction must be between") {
                    push_unique(&mut out, "conviction");
                }
                if self.detail.contains("justification is required") {
                    push_unique(&mut out, "justification");
                }
            }
            _ => {}
        }
        out
    }

    /// Replace the generic `detail` with an actionable hint when the
    /// failure is a reasoning-class model running out of budget before
    /// any visible text emerged — the QA15 item 5 footprint. No-op when:
    ///
    /// - `kind` is not `Truncated`
    /// - `raw_excerpt` is anything other than the `<empty>` sentinel
    /// - the model id is unknown or non-reasoning
    /// - `model_id` is `None`
    ///
    /// Designed as a fluent post-hoc wrapper so `parse_response` can stay
    /// model-blind and callers attach the hint only where they actually
    /// have the trader's model id (eval executor).
    pub fn with_model_hint(mut self, model_id: Option<&str>) -> Self {
        const EMPTY_RAW_SENTINEL: &str = "<empty>";
        if self.kind != TraderFailureKind::Truncated || self.raw_excerpt != EMPTY_RAW_SENTINEL {
            return self;
        }
        let Some(id) = model_id.map(str::trim).filter(|s| !s.is_empty()) else {
            return self;
        };
        let meta = xvision_core::providers::lookup_model(id);
        if !meta.is_reasoning() {
            return self;
        }
        self.detail = format!(
            "trader output truncated before any text emerged on reasoning-class model `{id}` \
             (hidden reasoning likely consumed the budget). Raise the agent's max_tokens \
             above {} or pick a non-reasoning model.",
            self.output_tokens,
        );
        self
    }

    fn diagnostics(&self) -> String {
        let stop = self
            .stop_reason
            .map(|s| format!("{s:?}"))
            .unwrap_or_else(|| "none".to_string());
        format!(
            "stop_reason={stop}, input_tokens={}, output_tokens={}, raw_excerpt={:?}",
            self.input_tokens, self.output_tokens, self.raw_excerpt
        )
    }
}

impl fmt::Display for TraderOutputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "run {run} decision {idx}: trader_output[{tag}]: {detail} ({diag})",
            run = self.run_id,
            idx = self.decision_index,
            tag = self.kind.tag(),
            detail = self.detail,
            diag = self.diagnostics(),
        )
    }
}

impl std::error::Error for TraderOutputError {}

impl TraderOutput {
    pub(crate) fn parse_response(
        response: &LlmResponse,
        run_id: &str,
        decision_index: u32,
    ) -> Result<Self, TraderOutputError> {
        let raw = response.text();

        if raw.trim().is_empty() {
            // No usable final text. Distinguish three causes:
            //  - Response has only tool_use blocks: model wanted more tool
            //    calls but its loop exited.
            //  - stop_reason == MaxTokens: response was truncated before
            //    text was emitted.
            //  - otherwise: model returned end_turn with empty content
            //    (provider returned "no final text").
            let has_tool_use = response
                .content
                .iter()
                .any(|c| matches!(c, crate::agent::llm::ContentBlock::ToolUse { .. }));
            let kind = if has_tool_use {
                TraderFailureKind::ToolUseOnly
            } else if response.stop_reason == StopReason::MaxTokens {
                TraderFailureKind::Truncated
            } else {
                TraderFailureKind::EmptyText
            };
            let detail = match kind {
                TraderFailureKind::ToolUseOnly => {
                    "trader output had only tool_use blocks; expected final text".to_string()
                }
                TraderFailureKind::Truncated => {
                    "trader output truncated at MaxTokens before any text was emitted".to_string()
                }
                _ => "trader output is empty: provider returned no final text".to_string(),
            };
            return Err(TraderOutputError::build(
                kind,
                run_id,
                decision_index,
                Some(response),
                Some(raw.as_str()),
                detail,
            ));
        }

        Self::parse_with_response(&raw, run_id, decision_index, response)
    }

    /// Build a `MissingResponse` error for the case where the pipeline never
    /// produced a trader slot at all.
    pub(crate) fn missing_response_error(run_id: &str, decision_index: u32) -> TraderOutputError {
        TraderOutputError::build(
            TraderFailureKind::MissingResponse,
            run_id,
            decision_index,
            None,
            None,
            "trader pipeline returned no trader response slot".to_string(),
        )
    }

    #[cfg(test)]
    pub(crate) fn parse_strict(
        raw: &str,
        run_id: &str,
        decision_index: u32,
    ) -> Result<Self, TraderOutputError> {
        Self::parse_with_response_inner(raw, run_id, decision_index, None)
    }

    fn parse_with_response(
        raw: &str,
        run_id: &str,
        decision_index: u32,
        response: &LlmResponse,
    ) -> Result<Self, TraderOutputError> {
        Self::parse_with_response_inner(raw, run_id, decision_index, Some(response))
    }

    fn parse_with_response_inner(
        raw: &str,
        run_id: &str,
        decision_index: u32,
        response: Option<&LlmResponse>,
    ) -> Result<Self, TraderOutputError> {
        let mut first_error: Option<(String, bool)> = None; // (message, was_missing_field)
        for candidate in trader_output_candidates(raw) {
            match serde_json::from_str::<Self>(&candidate) {
                Ok(mut parsed) => {
                    // Normalize the action to lowercase before validating
                    // against the canonical vocabulary. Qwen 3.6 and other
                    // models occasionally emit title-cased forms ("Hold",
                    // "Long_Open"); the underlying enum stays lowercase so
                    // downstream code is unaffected. Diagnostics that name
                    // `self.action` therefore show the normalized form the
                    // parser actually tested.
                    parsed.action = parsed.action.to_ascii_lowercase();
                    // R1: coerce non-canonical action synonyms BEFORE
                    // validating. A recognized synonym keeps the model's
                    // conviction; an unrecognized value becomes `hold` with
                    // conviction forced to 0. `coerced_action` records the
                    // (from, to) so the caller can emit a non-fatal
                    // `trader_output_action_coerced` event. This kills the
                    // `skip` class in every phase without a schema-patch retry.
                    let (mapped_to, unknown) = match coerce_action(&parsed.action) {
                        ActionCoercion::Canonical => (None, false),
                        ActionCoercion::Synonym(canonical) => (Some(canonical), false),
                        ActionCoercion::Unknown => (Some("hold"), true),
                    };
                    if let Some(to) = mapped_to {
                        let from = std::mem::replace(&mut parsed.action, to.to_string());
                        if unknown {
                            parsed.conviction = 0.0;
                        }
                        // Non-fatal `trader_output_action_coerced` event. Debug
                        // level so a skip-happy local model can't spam one line
                        // per decision; the `coerced_action` field below is the
                        // structured signal the caller can surface/aggregate.
                        tracing::debug!(
                            target: "xvision::eval",
                            from = %from,
                            to,
                            unknown,
                            "trader_output_action_coerced"
                        );
                        parsed.coerced_action = Some((from, to.to_string()));
                    }
                    // ERROR-3: a model emits `stop_loss_pct`/`take_profit_pct`
                    // = 0 to mean "no bracket of my own; rely on the configured
                    // risk gate". Normalize an exact 0 to `None` so it validates
                    // first-try (no schema-patch repair retry) and the R1
                    // config-ATR fallback supplies the protective stop. A
                    // nonzero out-of-range value still fails validate() below.
                    if parsed.stop_loss_pct == Some(0.0) {
                        parsed.stop_loss_pct = None;
                    }
                    if parsed.take_profit_pct == Some(0.0) {
                        parsed.take_profit_pct = None;
                    }
                    parsed.validate(run_id, decision_index, response, raw)?;
                    return Ok(parsed);
                }
                Err(e) => {
                    if first_error.is_none() {
                        let msg = e.to_string();
                        let missing_field = msg.contains("missing field");
                        first_error = Some((trader_output_error_detail(&e), missing_field));
                    }
                }
            }
        }

        let (detail_inner, missing_field) =
            first_error.unwrap_or_else(|| ("no JSON object found".into(), false));

        // Classify: if the response stopped at MaxTokens, blame truncation
        // even when the partial text doesn't parse — operators usually want
        // to fix max_tokens before investigating the JSON shape. Otherwise
        // pick MissingField vs InvalidJson based on the serde error.
        let stopped_at_max = response
            .map(|r| r.stop_reason == StopReason::MaxTokens)
            .unwrap_or(false);
        let (kind, detail) = if stopped_at_max {
            (
                TraderFailureKind::Truncated,
                format!("trader output truncated at MaxTokens; final text was invalid JSON: {detail_inner}"),
            )
        } else if missing_field {
            (
                TraderFailureKind::MissingField,
                format!("trader output is invalid JSON: {detail_inner}"),
            )
        } else {
            (
                TraderFailureKind::InvalidJson,
                format!("trader output is invalid JSON: {detail_inner}"),
            )
        };

        Err(TraderOutputError::build(
            kind,
            run_id,
            decision_index,
            response,
            Some(raw),
            detail,
        ))
    }

    fn validate(
        &self,
        run_id: &str,
        decision_index: u32,
        response: Option<&LlmResponse>,
        raw: &str,
    ) -> Result<(), TraderOutputError> {
        if !matches!(self.action.as_str(), "long_open" | "short_open" | "flat" | "hold") {
            return Err(TraderOutputError::build(
                TraderFailureKind::InvalidField,
                run_id,
                decision_index,
                response,
                Some(raw),
                format!(
                    "trader output action must be one of long_open, short_open, flat, hold (got `{}`)",
                    self.action
                ),
            ));
        }
        if !(0.0..=1.0).contains(&self.conviction) {
            return Err(TraderOutputError::build(
                TraderFailureKind::InvalidField,
                run_id,
                decision_index,
                response,
                Some(raw),
                format!(
                    "trader output conviction must be between 0 and 1 (got {})",
                    self.conviction
                ),
            ));
        }
        if self.justification.trim().is_empty() {
            return Err(TraderOutputError::build(
                TraderFailureKind::InvalidField,
                run_id,
                decision_index,
                response,
                Some(raw),
                "trader output justification is required".to_string(),
            ));
        }
        // Protective-bracket range validation. Mirrors the ranges enforced on
        // `xvision_core::trading::TraderDecision` (SL 0.1–20.0%, TP 0.1–50.0%)
        // so the eval `TraderOutput` cannot smuggle a degenerate bracket (e.g.
        // `stop_loss_pct = 0`, which the SL/TP engine would treat as "no stop")
        // past the schema. Only validated when present — the fields are
        // optional and `None` is the back-compatible default.
        if let Some(sl) = self.stop_loss_pct {
            if !sl.is_finite() || !(0.001..=20.0).contains(&sl) {
                return Err(TraderOutputError::build(
                    TraderFailureKind::InvalidField,
                    run_id,
                    decision_index,
                    response,
                    Some(raw),
                    format!("trader output stop_loss_pct must be between 0.001 and 20.0 (got {sl})"),
                ));
            }
        }
        if let Some(tp) = self.take_profit_pct {
            if !tp.is_finite() || !(0.001..=50.0).contains(&tp) {
                return Err(TraderOutputError::build(
                    TraderFailureKind::InvalidField,
                    run_id,
                    decision_index,
                    response,
                    Some(raw),
                    format!("trader output take_profit_pct must be between 0.001 and 50.0 (got {tp})"),
                ));
            }
        }
        Ok(())
    }
}

/// Validate text against the same strict `trader_output` contract used by
/// eval executors, without exposing the internal parsed representation.
pub fn validate_trader_output_text(
    raw: &str,
    run_id: &str,
    decision_index: u32,
) -> Result<(), TraderOutputError> {
    TraderOutput::parse_with_response_inner(raw, run_id, decision_index, None).map(|_| ())
}

/// Extract every name inside `` `<name>` `` immediately following `prefix`
/// in `s`, pushing each unique result into `out`. Helper for
/// [`TraderOutputError::problem_fields`]. The serde message format is
/// stable enough that a literal anchor + a closing-backtick search
/// suffices — no regex required.
fn extract_backticked_after(s: &str, prefix: &str, out: &mut Vec<String>) {
    let mut cursor = 0usize;
    while let Some(rel) = s[cursor..].find(prefix) {
        let start = cursor + rel + prefix.len();
        if !s[start..].starts_with('`') {
            cursor = start;
            continue;
        }
        let after_tick = start + 1;
        if let Some(end_rel) = s[after_tick..].find('`') {
            let name = &s[after_tick..after_tick + end_rel];
            if !name.is_empty() {
                push_unique(out, name);
            }
            cursor = after_tick + end_rel + 1;
        } else {
            break;
        }
    }
}

fn push_unique(out: &mut Vec<String>, name: &str) {
    if !out.iter().any(|n| n == name) {
        out.push(name.to_string());
    }
}

/// Merge the partial original trader text (best-effort parsed as a JSON
/// object) with a follow-up patch (also a JSON object), then attempt to
/// parse the merged value as a `TraderOutput`. F-5 phase 2b
/// (`harness-recovery-schema-missing-field`): the targeted-patch retry
/// asks the model to emit *only* the failing fields; merging here
/// overlays them on top of the original response so fields the model
/// already produced correctly are preserved verbatim.
///
/// `original_raw` is the verbatim text from the first attempt — we feed
/// it through the same candidate-extraction pipeline as
/// `parse_response` so code-fenced / wrapper-wrapped first responses
/// are recovered as the base object. If no object candidate parses, the
/// merge starts from an empty object (the patch alone must contain
/// every required field).
///
/// `patch_raw` is the verbatim text from the second attempt. Same
/// candidate-extraction is applied.
///
/// On success, returns the parsed `TraderOutput`. On failure, returns a
/// `TraderOutputError` describing the merged value's parse failure —
/// callers in the recovery module surface the ORIGINAL error per the
/// contract, but the returned error here drives the
/// `recovery.failed.second_detail` diagnostic.
pub(crate) fn merge_and_reparse_trader_output(
    original_raw: &str,
    patch_raw: &str,
    run_id: &str,
    decision_index: u32,
) -> Result<TraderOutput, TraderOutputError> {
    fn first_object(raw: &str) -> serde_json::Map<String, serde_json::Value> {
        for candidate in trader_output_candidates(raw) {
            if let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(&candidate)
            {
                return map;
            }
        }
        serde_json::Map::new()
    }

    let base = first_object(original_raw);
    let patch = first_object(patch_raw);

    // Right-biased merge: patch keys win. Shallow merge only —
    // `TraderOutput` is a flat struct so deep-merge would never matter,
    // and shallow keeps the semantics legible.
    let mut merged = base;
    for (k, v) in patch {
        merged.insert(k, v);
    }

    let merged_raw = serde_json::Value::Object(merged).to_string();
    // Reparse using the same strict pipeline. No `LlmResponse` is
    // available here (we're past the dispatch boundary), so the
    // diagnostics use `raw_text=Some(merged_raw)` + `response=None`.
    TraderOutput::parse_with_response_inner(&merged_raw, run_id, decision_index, None)
}

fn trader_output_candidates(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    push_candidate(&mut out, raw.trim());

    if let Some(stripped) = strip_code_fence(raw.trim()) {
        push_candidate(&mut out, stripped.trim());
    }
    if let Some(extracted) = extract_first_json_object(raw) {
        push_candidate(&mut out, &extracted);
    }

    let mut i = 0;
    while i < out.len() {
        let candidate = out[i].clone();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&candidate) {
            append_wrapped_candidates(&mut out, &value);
        }
        i += 1;
    }
    out
}

fn append_wrapped_candidates(out: &mut Vec<String>, value: &serde_json::Value) {
    let Some(obj) = value.as_object() else {
        return;
    };

    for key in ["output", "text", "content", "response"] {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            push_candidate(out, s.trim());
            if let Some(stripped) = strip_code_fence(s.trim()) {
                push_candidate(out, stripped.trim());
            }
            if let Some(extracted) = extract_first_json_object(s) {
                push_candidate(out, &extracted);
            }
        }
    }

    for key in ["decision", "trader_output", "arguments", "parameters"] {
        if let Some(v) = obj.get(key).filter(|v| v.is_object()) {
            push_candidate(out, &v.to_string());
        }
    }

    // Function-call wrapper stripping: some models (especially ollama-hosted
    // small models) emit the decision payload mixed with tool-call metadata
    // keys at the same level, e.g. {"name":"submit_decision","action":"long_open",...}
    // or {"tool":"submit_decision","arguments":{...}}. If the object carries
    // a tool-name key, emit a single stripped copy with ALL tool-name keys
    // removed (not one per key — a per-key clone would leave other unknown
    // keys and fail #[serde(deny_unknown_fields)]). The separate
    // "arguments"/"parameters" unwrap above handles the nested-object case;
    // this handles the flattened case where the decision fields and the
    // tool-name key share the same object.
    let tool_name_keys: &[&str] = &["name", "tool", "function"];
    if tool_name_keys.iter().any(|k| obj.contains_key(*k)) {
        let mut stripped = obj.clone();
        for k in tool_name_keys {
            stripped.remove(*k);
        }
        // Guard: if the object consisted only of tool-name keys (e.g.
        // {"name":"submit_decision"}), stripping produces an empty object —
        // don't emit a candidate that can never parse as TraderOutput.
        if !stripped.is_empty() {
            push_candidate(out, &serde_json::Value::Object(stripped).to_string());
        }
    }
}

fn push_candidate(out: &mut Vec<String>, candidate: &str) {
    if candidate.is_empty() {
        return;
    }
    if !out.iter().any(|seen| seen == candidate) {
        out.push(candidate.to_string());
    }
}

fn strip_code_fence(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    let rest = raw.strip_prefix("```")?;
    let rest = rest
        .strip_prefix("json")
        .or_else(|| rest.strip_prefix("JSON"))
        .unwrap_or(rest)
        .trim_start_matches(['\r', '\n']);
    let end = rest.rfind("```")?;
    Some(&rest[..end])
}

pub(crate) fn extract_first_json_object(raw: &str) -> Option<String> {
    for (start, ch) in raw.char_indices() {
        if ch != '{' {
            continue;
        }
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, c) in raw[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(raw[start..start + offset + c.len_utf8()].to_string());
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn trader_output_error_detail(error: &serde_json::Error) -> String {
    let message = error.to_string();
    if message.contains("missing field `action`") || message.contains("missing field action") {
        format!(
            "{message}; missing required trader field `action` (expected one of long_open, short_open, flat, hold)"
        )
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::llm::{ContentBlock, LlmResponse, StopReason};

    use super::{TraderFailureKind, TraderOutput, TraderOutputError};

    // ─── F-5 phase 2b: problem_fields extraction ──────────────────────

    // ─── U1: protective-bracket parse + range validation ──────────────

    #[test]
    fn brackets_absent_parses_as_none() {
        // Back-compat: an output with no SL/TP fields parses and leaves both
        // brackets None (no error).
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"long_open","conviction":0.7,"justification":"breakout"}"#,
            "01TEST",
            0,
        )
        .expect("output without brackets must still parse");
        assert_eq!(parsed.stop_loss_pct, None);
        assert_eq!(parsed.take_profit_pct, None);
    }

    #[test]
    fn valid_brackets_round_trip() {
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"long_open","conviction":0.7,"justification":"breakout","stop_loss_pct":2.0,"take_profit_pct":6.0}"#,
            "01TEST",
            0,
        )
        .expect("valid brackets must parse and validate");
        assert_eq!(parsed.stop_loss_pct, Some(2.0));
        assert_eq!(parsed.take_profit_pct, Some(6.0));
    }

    #[test]
    fn zero_brackets_normalize_to_none() {
        // ERROR-3 (docs/QA/2026-06-14-eval-test-gemini-flash-churn-findings.md):
        // a model told to "let the deterministic ATR stop manage the exit"
        // emits `stop_loss_pct: 0` ("no bracket of my own"). Pre-fix the parser
        // REJECTED 0 as a degenerate bracket, forcing a schema-patch repair
        // retry on every such call (the 58 `trader_output_schema_patch_recovered`
        // events). Since the R1 fix planted a config ATR-stop floor for any
        // position lacking a model SL, 0 is no longer degenerate — normalize it
        // to `None` so the first emission validates and the config ATR stop
        // takes over. Same for `take_profit_pct: 0`.
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"long_open","conviction":0.7,"justification":"breakout","stop_loss_pct":0.0,"take_profit_pct":0.0}"#,
            "01TEST",
            0,
        )
        .expect("zero brackets must normalize to None, not error");
        assert_eq!(parsed.stop_loss_pct, None);
        assert_eq!(parsed.take_profit_pct, None);
    }

    #[test]
    fn nonzero_out_of_range_stop_loss_pct_still_rejected() {
        // The degenerate-bracket guard still rejects a genuinely invalid
        // nonzero stop (e.g. 0.05% — below the 0.1 floor): only an exact 0
        // means "no model stop". This keeps a churny micro-stop from slipping
        // through while letting the "no stop" intent validate first-try.
        let err = TraderOutput::parse_strict(
            r#"{"action":"long_open","conviction":0.7,"justification":"breakout","stop_loss_pct":0.05}"#,
            "01TEST",
            0,
        )
        .expect_err("nonzero out-of-range stop_loss_pct must still be rejected");
        assert_eq!(err.kind, TraderFailureKind::InvalidField);
        assert!(err.to_string().contains("stop_loss_pct must be between"));
    }

    #[test]
    fn out_of_range_take_profit_pct_is_rejected() {
        let err = TraderOutput::parse_strict(
            r#"{"action":"long_open","conviction":0.7,"justification":"breakout","take_profit_pct":99.0}"#,
            "01TEST",
            0,
        )
        .expect_err("take_profit_pct above 50 must be rejected");
        assert_eq!(err.kind, TraderFailureKind::InvalidField);
        assert!(err.to_string().contains("take_profit_pct must be between"));
    }

    #[test]
    fn unknown_bracket_field_still_rejected() {
        // `deny_unknown_fields` must stay intact even with the new optional
        // bracket fields present. A misspelled bracket key (`stop_loss`
        // instead of `stop_loss_pct`) must NOT be silently accepted.
        let err = TraderOutput::parse_strict(
            r#"{"action":"long_open","conviction":0.7,"justification":"x","stop_loss":2.0}"#,
            "01TEST",
            0,
        )
        .expect_err("misspelled/unknown bracket field must be rejected");
        // Unknown field is a serde parse failure, not a validate() failure.
        assert!(matches!(
            err.kind,
            TraderFailureKind::InvalidJson | TraderFailureKind::MissingField
        ));
    }

    #[test]
    fn missing_conviction_defaults_to_0_5() {
        // Ollama small models consistently omit conviction.
        // The serde default (0.5) lets us accept these decisions.
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"hold","justification":"hold pattern"}"#,
            "01TEST",
            0,
        )
        .expect("missing conviction must default to 0.5, not fail");
        assert_eq!(parsed.action, "hold");
        assert_eq!(parsed.conviction, 0.5);
        assert_eq!(parsed.justification, "hold pattern");
    }

    #[test]
    fn problem_fields_extracts_missing_action_via_decorator() {
        // `action` triggers the `missing required trader field `action``
        // decorator in addition to serde's bare `missing field `action``.
        // Either anchor matching is sufficient; just one occurrence of
        // `action` should appear in the deduped output.
        let err = TraderOutput::parse_strict(
            r#"{"conviction":0.7,"justification":"trend continuation"}"#,
            "01TEST",
            0,
        )
        .expect_err("missing action must fail");
        assert_eq!(err.kind, TraderFailureKind::MissingField);
        let fields = err.problem_fields();
        assert!(
            fields.iter().any(|f| f == "action"),
            "expected action in {fields:?}; detail={}",
            err.detail,
        );
    }

    #[test]
    fn problem_fields_extracts_invalid_action() {
        // R1 coerces non-canonical actions before validation, so this error
        // is no longer reachable through `parse_strict`. The `validate()`
        // action guard remains as a defensive invariant, and `problem_fields()`
        // must still extract "action" from its detail — construct the error
        // directly to keep that diagnostic path covered.
        let err = TraderOutputError::build(
            TraderFailureKind::InvalidField,
            "01TEST",
            0,
            None,
            Some(r#"{"action":"BUY_BIG"}"#),
            "trader output action must be one of long_open, short_open, flat, hold (got `buy_big`)"
                .to_string(),
        );
        assert_eq!(err.kind, TraderFailureKind::InvalidField);
        assert_eq!(err.problem_fields(), vec!["action".to_string()]);
    }

    #[test]
    fn problem_fields_extracts_invalid_conviction() {
        let err = TraderOutput::parse_strict(
            r#"{"action":"hold","conviction":1.5,"justification":"out of range"}"#,
            "01TEST",
            0,
        )
        .expect_err("out-of-range conviction must fail");
        assert_eq!(err.kind, TraderFailureKind::InvalidField);
        assert_eq!(err.problem_fields(), vec!["conviction".to_string()]);
    }

    #[test]
    fn problem_fields_extracts_invalid_justification() {
        let err = TraderOutput::parse_strict(
            r#"{"action":"hold","conviction":0.5,"justification":""}"#,
            "01TEST",
            0,
        )
        .expect_err("empty justification must fail");
        assert_eq!(err.kind, TraderFailureKind::InvalidField);
        assert_eq!(err.problem_fields(), vec!["justification".to_string()]);
    }

    #[test]
    fn problem_fields_empty_for_non_schema_kinds() {
        let err = TraderOutput::parse_strict("not json at all", "01TEST", 0).expect_err("garbage must fail");
        assert_eq!(err.kind, TraderFailureKind::InvalidJson);
        assert!(
            err.problem_fields().is_empty(),
            "InvalidJson must not produce field names: {:?}",
            err.problem_fields(),
        );
    }
    #[test]
    fn merge_and_reparse_patch_overrides_defaulted_conviction() {
        use super::merge_and_reparse_trader_output;
        // Original parses fine with default conviction 0.5; patch overrides it.
        let merged = merge_and_reparse_trader_output(
            r#"{"action":"hold","justification":"range chop"}"#,
            r#"{"conviction":0.7}"#,
            "01TEST",
            0,
        )
        .expect("merge must produce a valid TraderOutput");
        assert_eq!(merged.action, "hold");
        assert_eq!(merged.conviction, 0.7);
        assert_eq!(merged.justification, "range chop");
    }

    #[test]
    fn merge_and_reparse_patch_overrides_invalid_field() {
        use super::merge_and_reparse_trader_output;
        // Original had invalid action `BUY_BIG`; patch supplies `hold`.
        let merged = merge_and_reparse_trader_output(
            r#"{"action":"BUY_BIG","conviction":0.6,"justification":"go big"}"#,
            r#"{"action":"hold"}"#,
            "01TEST",
            0,
        )
        .expect("merge must produce a valid TraderOutput");
        assert_eq!(merged.action, "hold");
    }

    #[test]
    fn merge_and_reparse_still_fails_when_patch_is_incomplete() {
        use super::merge_and_reparse_trader_output;
        // Both original and patch are missing `action`. Merge fails.
        let err = merge_and_reparse_trader_output(
            r#"{"conviction":0.7,"justification":"x"}"#,
            r#"{"justification":"better explanation"}"#,
            "01TEST",
            0,
        )
        .expect_err("merge must still fail when patch is incomplete");
        // The remaining failure is still MissingField (action).
        assert_eq!(err.kind, TraderFailureKind::MissingField);
    }

    #[test]
    fn missing_action_has_field_level_diagnostic() {
        for run_id in ["01KRK9Y45K1MKS9FTH4TY4SK47", "01KRKATKTK331A08TQ2MBN6FYC"] {
            let err = TraderOutput::parse_strict(
                r#"{"conviction":0.7,"justification":"trend continuation"}"#,
                run_id,
                0,
            )
            .expect_err("missing action must fail");
            let message = err.to_string();

            assert_eq!(err.kind, TraderFailureKind::MissingField);
            assert!(message.contains(run_id));
            assert!(message.contains("decision 0"));
            assert!(message.contains("trader_output[missing_field]"));
            assert!(message.contains("missing required trader field `action`"));
        }
    }

    #[test]
    fn unknown_action_coerced_to_hold_with_zero_conviction() {
        // R1: an action outside both the canonical set AND the synonym table
        // is coerced to `hold` with conviction forced to 0 — we cannot trust a
        // confidence attached to an action we did not understand.
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"yolo_moon","conviction":0.9,"justification":"vibes"}"#,
            "01TEST",
            3,
        )
        .expect("unknown action must coerce to hold, not fail");

        assert_eq!(parsed.action, "hold");
        assert_eq!(parsed.conviction, 0.0, "unknown action zeroes conviction");
        assert_eq!(
            parsed.coerced_action,
            Some(("yolo_moon".to_string(), "hold".to_string()))
        );
    }

    #[test]
    fn coerces_every_action_synonym_class() {
        // R1: exhaustive coverage of the synonym table across all four
        // canonical targets, plus the canonical-passthrough (no coercion).
        let cases = [
            ("skip", "hold"),
            ("none", "hold"),
            ("wait", "hold"),
            ("pass", "hold"),
            ("noop", "hold"),
            ("buy", "long_open"),
            ("long", "long_open"),
            ("sell", "short_open"),
            ("short", "short_open"),
            ("close", "flat"),
            ("exit", "flat"),
        ];
        for (raw, expected) in cases {
            let json = format!(r#"{{"action":"{raw}","conviction":0.5,"justification":"x"}}"#);
            let parsed = TraderOutput::parse_strict(&json, "01TEST", 0)
                .unwrap_or_else(|e| panic!("synonym {raw} must coerce: {e}"));
            assert_eq!(parsed.action, expected, "synonym {raw} -> {expected}");
            assert_eq!(parsed.conviction, 0.5, "synonym {raw} keeps conviction");
            assert_eq!(
                parsed.coerced_action,
                Some((raw.to_string(), expected.to_string()))
            );
        }

        // Canonical actions are passed through untouched (no coercion record).
        for canon in ["long_open", "short_open", "flat", "hold"] {
            let json = format!(r#"{{"action":"{canon}","conviction":0.5,"justification":"x"}}"#);
            let parsed = TraderOutput::parse_strict(&json, "01TEST", 0)
                .unwrap_or_else(|e| panic!("canonical {canon} must parse: {e}"));
            assert_eq!(parsed.action, canon);
            assert_eq!(parsed.coerced_action, None, "canonical {canon} not coerced");
        }
    }

    #[test]
    fn action_accepts_title_case() {
        // Repro from operator's 2026-05-18 Qwen 3.6 run
        // `01KRWHHBR8FVKM1NVJPQXD4D4B decision 0`: model emitted
        // `"action": "Hold"` (title-cased) which the pre-fix strict
        // match rejected. After the parser-side lowercase, "Hold"
        // normalises to "hold" and validates cleanly.
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"Hold","conviction":0.7,"justification":"range chop"}"#,
            "01KRWHHBR8FVKM1NVJPQXD4D4B",
            0,
        )
        .expect("title-cased Hold must parse after lowercase normalisation");
        assert_eq!(parsed.action, "hold");
    }

    #[test]
    fn action_accepts_upper_case() {
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"LONG_OPEN","conviction":0.9,"justification":"breakout"}"#,
            "01TEST",
            1,
        )
        .expect("UPPER_CASE action must parse after lowercase normalisation");
        assert_eq!(parsed.action, "long_open");
    }

    #[test]
    fn action_accepts_mixed_case() {
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"Short_Open","conviction":0.6,"justification":"downtrend confirmed"}"#,
            "01TEST",
            2,
        )
        .expect("mixed-case action must parse after lowercase normalisation");
        assert_eq!(parsed.action, "short_open");
    }

    #[test]
    fn synonym_action_buy_coerced_to_long_open() {
        // R1: a recognized synonym ("Buy" -> lowercase "buy" -> "long_open")
        // is coerced before validation instead of failing, and the model's
        // conviction is preserved (its confidence in the decision still holds).
        let parsed = TraderOutput::parse_strict(
            r#"{"action":"Buy","conviction":0.7,"justification":"momentum"}"#,
            "01TEST",
            4,
        )
        .expect("synonym action 'Buy' must coerce, not fail");

        assert_eq!(parsed.action, "long_open");
        assert_eq!(parsed.conviction, 0.7, "synonym keeps the model's conviction");
        assert_eq!(
            parsed.coerced_action,
            Some(("buy".to_string(), "long_open".to_string())),
            "coercion is recorded as (normalised-from, to)"
        );
    }

    #[test]
    fn empty_justification_has_field_level_diagnostic() {
        let err = TraderOutput::parse_strict(
            r#"{"action":"hold","conviction":0.7,"justification":" "}"#,
            "01TEST",
            3,
        )
        .expect_err("empty justification must fail");

        assert_eq!(err.kind, TraderFailureKind::InvalidField);
        assert!(err
            .to_string()
            .contains("trader output justification is required"));
    }

    #[test]
    fn empty_response_has_provider_diagnostic_instead_of_json_eof() {
        let response = LlmResponse {
            content: Vec::new(),
            stop_reason: StopReason::EndTurn,
            input_tokens: 981,
            output_tokens: 0,
        };

        let err = TraderOutput::parse_response(&response, "01KRMKWZ1KJ2BGRNWGP518ZQ3Q", 4)
            .expect_err("empty trader text must fail before JSON parsing");
        let message = err.to_string();

        assert_eq!(err.kind, TraderFailureKind::EmptyText);
        assert!(message.contains("trader_output[empty]"));
        assert!(message.contains("trader output is empty"));
        assert!(message.contains("decision 4"));
        assert!(message.contains("stop_reason=EndTurn"));
        assert!(message.contains("output_tokens=0"));
        assert!(
            !message.contains("EOF while parsing"),
            "empty response should not be reported as JSON EOF: {message}"
        );
    }

    #[test]
    fn tool_use_only_response_classifies_as_tool_use_only() {
        let response = LlmResponse {
            content: vec![ContentBlock::ToolUse {
                id: "abc".into(),
                name: "fetch_bars".into(),
                input: serde_json::json!({}),
            }],
            stop_reason: StopReason::ToolUse,
            input_tokens: 200,
            output_tokens: 12,
        };

        let err = TraderOutput::parse_response(&response, "01TOOL", 7)
            .expect_err("tool-use-only response should not parse");

        assert_eq!(err.kind, TraderFailureKind::ToolUseOnly);
        let message = err.to_string();
        assert!(message.contains("trader_output[tool_use_only]"));
        assert!(message.contains("only tool_use blocks"));
        assert!(message.contains("stop_reason=ToolUse"));
    }

    #[test]
    fn max_tokens_empty_response_classifies_as_truncated() {
        let response = LlmResponse {
            content: Vec::new(),
            stop_reason: StopReason::MaxTokens,
            input_tokens: 1000,
            output_tokens: 0,
        };

        let err = TraderOutput::parse_response(&response, "01TRUNC", 2)
            .expect_err("max-tokens empty response should not parse");

        assert_eq!(err.kind, TraderFailureKind::Truncated);
        let message = err.to_string();
        assert!(message.contains("trader_output[truncated]"));
        assert!(message.contains("truncated at MaxTokens"));
        assert!(message.contains("stop_reason=MaxTokens"));
        assert!(message.contains("output_tokens=0"));
    }

    #[test]
    fn response_parse_errors_include_provider_metadata() {
        let response = LlmResponse {
            content: vec![ContentBlock::Text { text: "{".into() }],
            stop_reason: StopReason::MaxTokens,
            input_tokens: 1000,
            output_tokens: 1000,
        };

        let err = TraderOutput::parse_response(&response, "01TEST", 2)
            .expect_err("truncated trader JSON must fail");
        let message = err.to_string();

        // MaxTokens + unparseable text → Truncated kind (the operator should
        // raise max_tokens before reasoning about the JSON shape).
        assert_eq!(err.kind, TraderFailureKind::Truncated);
        assert!(message.contains("trader_output[truncated]"));
        assert!(message.contains("invalid JSON"));
        assert!(message.contains("stop_reason=MaxTokens"));
        assert!(message.contains("output_tokens=1000"));
        // The raw partial text is preserved so reviewers can see what came
        // back before the cut-off.
        assert!(message.contains("raw_excerpt"));
    }

    #[test]
    fn recovers_json_from_code_fence_and_trailing_text() {
        let parsed = TraderOutput::parse_strict(
            "Here is the decision:\n```json\n{\"action\":\"hold\",\"conviction\":0.4,\"justification\":\"range chop\"}\n```\nDone.",
            "01TEST",
            5,
        )
        .expect("valid fenced JSON should parse");

        assert_eq!(parsed.action, "hold");
        assert_eq!(parsed.justification, "range chop");
    }

    #[test]
    fn recovers_json_from_provider_output_wrapper() {
        let parsed = TraderOutput::parse_strict(
            r#"{"output":"{\"action\":\"long_open\",\"conviction\":0.8,\"justification\":\"breakout\"}"}"#,
            "01TEST",
            6,
        )
        .expect("wrapped JSON string should parse");

        assert_eq!(parsed.action, "long_open");
        assert_eq!(parsed.conviction, 0.8);
    }

    #[test]
    fn raw_excerpt_is_truncated_at_limit() {
        // 300-char single-line string of garbage — should be truncated to
        // 240 chars with an ellipsis. The exact length isn't asserted (the
        // ellipsis adds a char), only that the marker is present.
        let garbage = "z".repeat(300);
        let err = TraderOutput::parse_strict(&garbage, "01TEST", 0).expect_err("garbage must not parse");
        let message = err.to_string();
        assert!(message.contains('…'), "expected truncation marker in {message}");
    }

    #[test]
    fn failure_kind_round_trips_through_tag() {
        for kind in [
            TraderFailureKind::EmptyText,
            TraderFailureKind::ToolUseOnly,
            TraderFailureKind::Truncated,
            TraderFailureKind::InvalidJson,
            TraderFailureKind::MissingField,
            TraderFailureKind::InvalidField,
            TraderFailureKind::MissingResponse,
        ] {
            let tag = kind.tag();
            assert_eq!(TraderFailureKind::from_tag(tag), Some(kind), "tag {tag}");
        }
    }

    #[test]
    fn missing_response_helper_classifies_as_missing_response() {
        let err = TraderOutput::missing_response_error("01TEST", 9);
        assert_eq!(err.kind, TraderFailureKind::MissingResponse);
        let message = err.to_string();
        assert!(message.contains("trader_output[missing_response]"));
        assert!(message.contains("trader pipeline returned no trader response slot"));
        assert!(message.contains("raw_excerpt=\"<no_response>\""));
    }

    /// Reasoning-class truncation hint (q15 §1 acceptance). The eval
    /// executor decorates a `Truncated` + empty-raw error with the
    /// model-specific "raise max_tokens or pick a non-reasoning model"
    /// hint when (and only when) the trader's model is reasoning-class.
    mod truncated_hint {
        use super::*;

        fn truncated_empty(run_id: &str) -> super::super::TraderOutputError {
            // Reproduce the QA15 "stop_reason=MaxTokens / output_tokens=N
            // / raw_excerpt=<empty>" failure shape.
            let response = LlmResponse {
                content: Vec::new(),
                stop_reason: StopReason::MaxTokens,
                input_tokens: 422,
                output_tokens: 1000,
            };
            TraderOutput::parse_response(&response, run_id, 0)
                .expect_err("truncated empty response must fail")
        }

        #[test]
        fn reasoning_class_model_swaps_in_actionable_hint() {
            // DeepSeek R1 is canonical reasoning-class in the metadata
            // table. Sonnet 4.6 is conservatively kept as Standard until
            // a future revision tracks Anthropic's `thinking` toggle
            // explicitly — operators on that path can still raise
            // max_tokens manually based on the generic Truncated message.
            let hinted = truncated_empty("01HINT").with_model_hint(Some("deepseek-r1"));
            let msg = hinted.to_string();

            assert_eq!(hinted.kind, TraderFailureKind::Truncated);
            assert!(
                msg.contains("reasoning-class model"),
                "expected reasoning-class hint, got: {msg}",
            );
            assert!(
                msg.contains("max_tokens"),
                "expected actionable max_tokens guidance, got: {msg}",
            );
            assert!(
                msg.contains("non-reasoning"),
                "expected fallback-model suggestion, got: {msg}",
            );
            // The provider diagnostics are still preserved.
            assert!(msg.contains("stop_reason=MaxTokens"));
            assert!(msg.contains("output_tokens=1000"));
            assert!(msg.contains("raw_excerpt=\"<empty>\""));
        }

        #[test]
        fn non_reasoning_model_leaves_generic_message() {
            let hinted = truncated_empty("01HINT").with_model_hint(Some("claude-haiku-4-5"));
            let msg = hinted.to_string();

            assert!(
                msg.contains("truncated at MaxTokens"),
                "non-reasoning models keep the generic detail, got: {msg}",
            );
            assert!(
                !msg.contains("reasoning-class model"),
                "must not promise reasoning-class context for a non-reasoning model, got: {msg}",
            );
        }

        #[test]
        fn unknown_model_falls_back_to_generic_message() {
            let hinted = truncated_empty("01HINT").with_model_hint(Some("acme/nightly-7b"));
            let msg = hinted.to_string();
            // Unknown ids default to non-reasoning class — the hint is a no-op.
            assert!(msg.contains("truncated at MaxTokens"));
            assert!(!msg.contains("reasoning-class model"));
        }

        #[test]
        fn missing_model_id_is_a_noop() {
            let baseline = truncated_empty("01HINT").to_string();
            let hinted = truncated_empty("01HINT").with_model_hint(None);
            assert_eq!(baseline, hinted.to_string());
        }

        #[test]
        fn non_truncated_kinds_are_not_decorated() {
            // ToolUseOnly carries a different detail; the hint must not
            // hijack it even when the model id is reasoning-class.
            let response = LlmResponse {
                content: vec![ContentBlock::ToolUse {
                    id: "abc".into(),
                    name: "fetch_bars".into(),
                    input: serde_json::json!({}),
                }],
                stop_reason: StopReason::ToolUse,
                input_tokens: 100,
                output_tokens: 5,
            };
            let err = TraderOutput::parse_response(&response, "01HINT", 0)
                .expect_err("tool-use-only must fail")
                .with_model_hint(Some("claude-sonnet-4-6"));
            assert_eq!(err.kind, TraderFailureKind::ToolUseOnly);
            assert!(err.to_string().contains("only tool_use blocks"));
        }

        #[test]
        fn truncated_with_partial_text_is_not_a_reasoning_hint_case() {
            // The hint targets the QA15 footprint where raw_excerpt is
            // `<empty>`. When the model emitted partial text before the
            // cut-off, the raw_excerpt is non-empty and the generic
            // truncation message stays — operators see what came back.
            let response = LlmResponse {
                content: vec![ContentBlock::Text { text: "{".into() }],
                stop_reason: StopReason::MaxTokens,
                input_tokens: 1000,
                output_tokens: 1000,
            };
            let err = TraderOutput::parse_response(&response, "01HINT", 0)
                .expect_err("truncated partial text must fail")
                .with_model_hint(Some("deepseek-r1"));
            assert_eq!(err.kind, TraderFailureKind::Truncated);
            assert!(!err.to_string().contains("reasoning-class model"));
        }
    }

    #[test]
    fn function_call_wrapper_unwrapped_via_arguments_key() {
        // Repro: ollama models output the function-call wrapper format
        // {"name":"submit_decision","arguments":{...}} as raw text.
        // try_nodecision_recovery extracts this as decision_json, and
        // trader_output_candidates must unwrap the "arguments" key.
        let wrapper = r#"{"name":"submit_decision","arguments":{"action":"long_open","conviction":0.8,"justification":"RSI oversold bounce setup"}}"#;
        let parsed = TraderOutput::parse_strict(wrapper, "01TEST", 0)
            .expect("function-call wrapper must be unwrapped via arguments key");
        assert_eq!(parsed.action, "long_open");
        assert_eq!(parsed.conviction, 0.8);
        assert_eq!(parsed.justification, "RSI oversold bounce setup");
    }

    #[test]
    fn flattened_function_call_stripped_of_name_key() {
        // Variant where the model emits the tool-call metadata at the same
        // level as the decision fields — no nested "arguments" object.
        let wrapper = r#"{"name":"submit_decision","action":"short_open","conviction":0.75,"justification":"bearish engulfing on 5m"}"#;
        let parsed = TraderOutput::parse_strict(wrapper, "01TEST", 0)
            .expect("flattened function-call wrapper must strip name key");
        assert_eq!(parsed.action, "short_open");
        assert_eq!(parsed.conviction, 0.75);
        assert_eq!(parsed.justification, "bearish engulfing on 5m");
    }

    #[test]
    fn function_call_wrapper_unwrapped_via_parameters_key() {
        // Some APIs/agents use "parameters" instead of "arguments".
        let wrapper = r#"{"name":"submit_decision","parameters":{"action":"flat","conviction":0.3,"justification":"no clear signal"}}"#;
        let parsed = TraderOutput::parse_strict(wrapper, "01TEST", 0)
            .expect("function-call wrapper with 'parameters' key must be unwrapped");
        assert_eq!(parsed.action, "flat");
        assert_eq!(parsed.conviction, 0.3);
        assert_eq!(parsed.justification, "no clear signal");
    }

    #[test]
    fn tool_key_stripped_from_flattened_wrapper() {
        // Models might use "tool" instead of "name" as the tool-call key.
        let wrapper = r#"{"tool":"submit_decision","action":"hold","conviction":0.5,"justification":"waiting for breakout"}"#;
        let parsed = TraderOutput::parse_strict(wrapper, "01TEST", 0)
            .expect("flattened wrapper with 'tool' key must be stripped");
        assert_eq!(parsed.action, "hold");
        assert_eq!(parsed.conviction, 0.5);
        assert_eq!(parsed.justification, "waiting for breakout");
    }

    #[test]
    fn function_key_stripped_from_flattened_wrapper() {
        // OpenAI-style function_call wrappers use "function" as the key.
        let wrapper = r#"{"function":"submit_decision","action":"flat","conviction":0.2,"justification":"no edge"}"#;
        let parsed = TraderOutput::parse_strict(wrapper, "01TEST", 0)
            .expect("flattened wrapper with 'function' key must be stripped");
        assert_eq!(parsed.action, "flat");
        assert_eq!(parsed.conviction, 0.2);
        assert_eq!(parsed.justification, "no edge");
    }

    #[test]
    fn multiple_tool_name_keys_stripped_in_single_pass() {
        // If a model emits both "name" and "tool" keys, a single-pass strip
        // must remove ALL of them — a per-key clone would leave the other
        // unknown key and fail #[serde(deny_unknown_fields)].
        let wrapper = r#"{"name":"submit_decision","tool":"make_trade","action":"long_open","conviction":0.6,"justification":"multi-key wrapper"}"#;
        let parsed = TraderOutput::parse_strict(wrapper, "01TEST", 0)
            .expect("multiple tool-name keys must all be stripped in one pass");
        assert_eq!(parsed.action, "long_open");
        assert_eq!(parsed.conviction, 0.6);
        assert_eq!(parsed.justification, "multi-key wrapper");
    }

    #[test]
    fn tool_name_only_object_rejected_not_default_parsed() {
        // An object with only a tool-name key and no decision fields must
        // still be rejected — the empty-object guard prevents emitting a
        // candidate that could accidentally parse as TraderOutput::default().
        let err = TraderOutput::parse_strict(
            r#"{"name":"submit_decision"}"#,
            "01TEST",
            0,
        )
        .expect_err("tool-name-only object must be rejected");
        assert!(matches!(
            err.kind,
            TraderFailureKind::InvalidJson | TraderFailureKind::MissingField
        ));
    }
}
