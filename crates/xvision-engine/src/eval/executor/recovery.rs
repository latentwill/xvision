//! Shared eval-executor recovery helpers. F-5 phase 2a
//! (`harness-recovery-malformed-json`).
//!
//! Phase 1 of the F-5 audit recovery state machine (PR #499) wired the
//! typed dispatcher (`crate::agent::recovery::FailureClass` /
//! `RecoveryFamily`) and the `ObsEmitter::emit_recovery_*` seam. Phase 2
//! picks off the recovery families one at a time. Phase 2a — this file
//! — owns the MalformedJson family: when the trader's response fails to
//! parse as the canonical `TraderOutput` JSON shape (`InvalidJson` or
//! `Truncated`), the eval executor invokes a single-shot repair attempt
//! before propagating the original error.
//!
//! ## Why this lives in `eval::executor`, not in `agent::recovery`
//!
//! The repair policy needs the trader slot's prompt + model + the seed
//! inputs the trader was originally given — none of which the
//! `agent::recovery` module has visibility into. The agent module owns
//! the *classification* + the wire-stable repair-message body
//! (`agent::recovery::build_malformed_json_repair_message`); this
//! module owns the *dispatch* policy that sits between the executor's
//! `parse_response` call and the propagation path.
//!
//! ## Shared between paper.rs and backtest.rs
//!
//! Both executors invoke this helper at the same seam — immediately
//! after `TraderOutput::parse_response` fails — so a repair attempt has
//! identical semantics across the two run modes. The repeated-tool
//! block-list (phase 1) is per-slot and lives inside `execute_slot`; it
//! is not relevant here because the repair dispatch is a single-shot
//! LlmRequest with no tools — there is no tool-call loop to track.
//!
//! ## Bounded retry
//!
//! ONE repair attempt. If the second response also fails to parse, the
//! caller propagates the ORIGINAL `TraderOutputError` (per the
//! contract's "operator wants the first failure as the surfacing class"
//! wording) — the second error is recorded on the `recovery.failed`
//! span's `final_error` attribute but not stacked on top of the
//! returned anyhow chain.

use std::sync::Arc;

use crate::agent::llm::{ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, ResponseSchema};
use crate::agent::observability::{fresh_span_id, ObsEmitter};
use crate::agent::recovery::build_malformed_json_repair_message;
use crate::eval::executor::trader_output::{TraderFailureKind, TraderOutput, TraderOutputError};

/// Slot fields required to re-dispatch the trader for a repair attempt.
/// Both the legacy `Strategy.trader_slot` path and the agent-slot path
/// project into this shape before calling [`try_repair_malformed_json`]
/// so the helper stays oblivious to which path produced the original
/// failure.
///
/// Field-by-field semantics match the equivalent shape constructed
/// inside `execute_slot`:
/// - `system_prompt`: the slot's free-form prompt body (no preamble
///   added; the dispatcher's response-schema preamble is re-applied via
///   `response_schema` below).
/// - `model`: the effective model id — `LLMSlot::effective_model()` for
///   the legacy path, `ResolvedAgentSlot::slot.effective_model()` for
///   the agent path.
/// - `max_tokens`: the operator's per-request budget; `None` lets the
///   provider decide. Mirrors the value the original trader call used.
/// - `temperature`: same — pass-through verbatim.
pub struct TraderRepairContext<'a> {
    pub system_prompt: &'a str,
    pub model: String,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

/// Single-shot repair attempt for `MalformedJson` family failures
/// (`InvalidJson` / `Truncated`). Returns `Ok(parsed)` on success and
/// emits a `recovery.attempt` span with `outcome: Recovered`. Returns
/// the ORIGINAL [`TraderOutputError`] on second-attempt failure and
/// emits `recovery.failed` carrying the second-attempt error as
/// `final_error`. Callers propagate the returned error verbatim — the
/// wire-stable `[<tag>]` prefix on `eval_runs.error` stays exactly as
/// today's path produces it.
///
/// The dispatched repair LlmRequest carries:
///
///   1. The same `system_prompt` + `model` + `max_tokens` + `temperature`
///      the original trader call used, so the model has identical
///      context.
///   2. The same `response_schema` so OpenAI-compat providers re-emit
///      the strict json_schema response_format and Anthropic re-injects
///      the schema preamble.
///   3. A three-turn conversation log: the original user prompt (derived
///      from `seed_inputs` in the same shape `execute_slot` would have
///      produced), an assistant turn carrying the verbatim raw text the
///      model just emitted, and a user turn with the repair message
///      built by [`build_malformed_json_repair_message`].
///
/// The repair dispatch does NOT pass any tools — the model must emit a
/// single JSON object, not a tool_use. This is intentional: the contract
/// says "do not include prose, code fences, or tool calls" and removing
/// the tool definitions removes the temptation to emit one.
///
/// ## A/B cache pairing
///
/// The repair message body is deterministic for a given
/// `(parse_error, schema)` pair (see
/// [`build_malformed_json_repair_message`]). The seed-derived user
/// prompt is also deterministic because the eval executor's seed is
/// reconstructed from the scenario + bar history every cycle. Together
/// these mean the repair dispatch's prompt hash is reproducible across
/// re-runs of the same strategy/cycle, so a strategy that hits the
/// repair path once will hit the same repair path on every replay —
/// matching the existing A/B-compare deterministic-recovery
/// expectation.
#[allow(clippy::too_many_arguments)]
pub async fn try_repair_malformed_json(
    failed_response: &LlmResponse,
    original_error: TraderOutputError,
    repair_ctx: TraderRepairContext<'_>,
    seed_inputs: &serde_json::Value,
    dispatch: Arc<dyn LlmDispatch>,
    obs: Option<&ObsEmitter>,
    run_id: &str,
    decision_index: u32,
) -> Result<TraderOutput, TraderOutputError> {
    // Only the MalformedJson family is eligible for the repair path.
    // The contract reserves Truncated + InvalidJson; SchemaMissingField
    // / EmptyData / Tool* are owned by sibling contracts (or already
    // surfaced as today). The check is defensive — paper.rs and
    // backtest.rs only call this helper after they've matched on the
    // kind.
    let class_tag = match original_error.kind {
        TraderFailureKind::InvalidJson => "invalid_json",
        TraderFailureKind::Truncated => "truncated",
        _ => return Err(original_error),
    };

    let schema = ResponseSchema::trader_output();

    // Reconstruct the original user prompt body in the same shape
    // `execute_slot` would have produced. The wording is identical so
    // the model sees byte-stable context across the original + repair
    // call. We deliberately drop the `agent_error_feedback` hoist that
    // `execute_slot` applies (it isn't relevant on the repair path —
    // the broker self-healing seam belongs to the first attempt).
    let initial_user_body = format!(
        "Inputs:\n{inputs}\n\nFollow the slot's instructions. You may call tools \
         to fetch additional data; emit your final decision as JSON.",
        inputs = serde_json::to_string_pretty(seed_inputs)
            .unwrap_or_else(|_| "<seed-serialize-error>".to_string()),
    );

    // The verbatim raw text the model just emitted. Anthropic / OpenAI
    // both accept an assistant turn with a single Text block, so we
    // re-build from `LlmResponse.text()` to keep the shape minimal.
    // Including only the text (no tool_use blocks) is the right call
    // because the malformed-json failure is text-side; any tool_use
    // blocks the model emitted before the parse failure are not part
    // of the response under repair.
    let assistant_raw = failed_response.text();

    let repair_user_body = build_malformed_json_repair_message(&original_error.detail, &schema);

    let messages = vec![
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: initial_user_body,
            }],
        },
        Message {
            role: "assistant".into(),
            content: vec![ContentBlock::Text { text: assistant_raw }],
        },
        Message {
            role: "user".into(),
            content: vec![ContentBlock::Text {
                text: repair_user_body,
            }],
        },
    ];

    let req = LlmRequest {
        model: repair_ctx.model,
        system_prompt: repair_ctx.system_prompt.to_string(),
        messages,
        max_tokens: repair_ctx.max_tokens,
        // No tools on the repair turn — the model must emit a single
        // JSON object. Stripping the tool definitions removes the
        // temptation to emit one (and matches the repair-message
        // "do not include tool calls" instruction).
        tools: Vec::new(),
        temperature: repair_ctx.temperature,
        response_schema: Some(schema),
        cache_control: None,
    };

    let repair_resp = match dispatch.complete(req).await {
        Ok(r) => r,
        Err(e) => {
            // Dispatcher-level transport failure during the repair
            // attempt — emit `recovery.failed` and surface the
            // original parse error as the contract requires.
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_failed(
                        &fresh_span_id(),
                        None,
                        class_tag,
                        1,
                        &format!("repair dispatch failed: {e:#}"),
                    )
                    .await;
            }
            return Err(original_error);
        }
    };

    match TraderOutput::parse_response(&repair_resp, run_id, decision_index) {
        Ok(parsed) => {
            // Repair landed — emit a `recovery.attempt` span carrying
            // `outcome: Recovered` (via `class_tag` + ok status; the
            // failure-side companion uses `emit_recovery_failed` with
            // status=Error). retry_count=1 because exactly one repair
            // attempt was made.
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_attempt(&fresh_span_id(), None, class_tag, 1)
                    .await;
            }
            tracing::info!(
                event = "trader_output_repair_recovered",
                run_id = %run_id,
                decision_index,
                class_tag,
                original_detail = %original_error.detail,
                "F-5 MalformedJson repair succeeded on retry 1",
            );
            Ok(parsed)
        }
        Err(second_err) => {
            if let Some(emitter) = obs {
                emitter
                    .emit_recovery_failed(
                        &fresh_span_id(),
                        None,
                        class_tag,
                        1,
                        &format!("second attempt also failed to parse: {second_err}"),
                    )
                    .await;
            }
            tracing::warn!(
                event = "trader_output_repair_failed",
                run_id = %run_id,
                decision_index,
                class_tag,
                original_detail = %original_error.detail,
                second_detail = %second_err.detail,
                "F-5 MalformedJson repair exhausted (1 retry); surfacing original error",
            );
            // Contract: propagate the ORIGINAL error (not the second
            // attempt's) so `eval_runs.error` carries `[invalid_json]`
            // / `[truncated]` exactly as it did pre-F-5.
            Err(original_error)
        }
    }
}

/// Convenience predicate: returns `true` when the `TraderOutputError`
/// falls into the MalformedJson family and is therefore eligible for
/// the repair path. Callers in paper.rs / backtest.rs check this
/// before invoking [`try_repair_malformed_json`] so the helper isn't
/// invoked for `MissingField` / `InvalidField` / `EmptyText` failures
/// (those are owned by sibling phase-2 contracts).
pub fn is_malformed_json_recoverable(err: &TraderOutputError) -> bool {
    matches!(
        err.kind,
        TraderFailureKind::InvalidJson | TraderFailureKind::Truncated
    )
}
