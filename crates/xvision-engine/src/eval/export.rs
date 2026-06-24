//! `EvalRunExport` — single-object JSON snapshot of a completed eval run.
//!
//! Built read-only over the existing persistence layer (no executor or
//! schema changes). Backs:
//!
//! - `GET /api/eval/runs/:id/export` in the dashboard.
//! - `xvn eval export <run_id>` in the CLI.
//! - The "Download JSON" button on the run-detail page.
//!
//! See:
//! - `docs/superpowers/specs/2026-05-16-q15-eval-resilience-and-contracts.md` §3
//! - `team/contracts/q15-eval-json-export.md`
//!
//! ## Schema versioning
//!
//! `schema_version` is pinned at `"1"`. Any breaking change to the shape
//! must bump it and ship a migration helper. Additive fields (new
//! `Option<…>` keys, new sub-arrays) do not bump the version.
//!
//! ## Aspirational vs. persisted fields
//!
//! The spec lists `events`, `errors`, and a richer per-decision shape
//! (`bar`, `trader_input`, `trader_output_raw`, `trader_output_parsed`,
//! `risk_decision`, `fill`, `errors[]`). Today the engine persists:
//!
//! - `eval_runs` (one row per run, including `error: Option<String>`),
//! - `eval_decisions` (one row per decision; carries the parsed
//!   `TraderDecision` + the broker-side fill + `reasoning`),
//! - `eval_equity_samples`, `eval_attestations`, `eval_reviews`,
//!   `eval_findings`.
//!
//! There is no `eval_events` table, and per-decision raw provider output
//! is not persisted (the executor stores only the parsed action +
//! reasoning + fill). The export reflects what's actually stored:
//!
//! - `events` is always `[]` (forward-compat placeholder; emit when an
//!   event log lands).
//! - `errors` wraps `run.error` as a single-element array when set, else
//!   `[]`.
//! - `decisions[]` exports the persisted DecisionRow fields. Adding raw
//!   provider trader output to this surface is a future task and would
//!   pair with a new column on `eval_decisions`.
//!
//! Round-trip semantics: the export is `Serialize + Deserialize`, so the
//! same bytes parse back into the same struct.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::agents::{Agent, AgentStore};
use crate::api::{self, ApiContext, ApiError, ApiResult};
use crate::eval::attestation::{EvalAttestation, TokensUsed};
use crate::eval::cost::compute_token_cost_usd_from_catalog;
use crate::eval::findings::{Finding, Severity, FINDING_SCHEMA_VERSION};
use crate::eval::review::EvalReview;
use crate::eval::run::{MetricsSummary, Run, RunStatus};
use crate::eval::scenario::Scenario;
use crate::eval::scenario_store;
use crate::eval::store::{DecisionRow, RunStore};
use crate::strategies::Strategy;
use xvision_filters::{FilterEventV1, FilterSummary};

/// Pinned export schema version. Bump only on breaking shape changes;
/// additive fields stay on `"1"`.
pub const SCHEMA_VERSION: &str = "1";

/// Full eval-run JSON snapshot.
///
/// `schema_version` is always present. Every other field is best-effort:
/// `scenario` / `strategy` may be `None` if the referenced record was
/// deleted after the run completed (cleanup paths exist; the export
/// stays usable). Arrays are always present, possibly empty.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalRunExport {
    pub schema_version: String,
    pub run: Run,
    pub scenario: Option<Scenario>,
    pub strategy: Option<Strategy>,
    pub agents: Vec<Agent>,
    pub metrics: Option<MetricsSummary>,
    pub decisions: Vec<DecisionExportRow>,
    pub equity_samples: Vec<EquitySample>,
    /// Event-log compatible rows for this run. Filter v1 emits
    /// `FilterEventV1` objects here as JSON values.
    pub events: Vec<serde_json::Value>,
    #[serde(default)]
    pub filter_events: Vec<FilterEventV1>,
    #[serde(default)]
    pub filter_summaries: Vec<FilterSummary>,
    /// Run-level error log. Today wraps `run.error` as a single entry
    /// (or empty when the run completed cleanly). Per-decision errors
    /// live alongside their decision row in `decisions[]`.
    pub errors: Vec<RunErrorEntry>,
    pub reviews: Vec<ReviewExportRow>,
    pub provider_diagnostics: Option<ProviderDiagnostics>,
}

/// Per-decision export row. Mirrors the persisted `DecisionRow` plus an
/// `ix` field that matches `decision_index` — exposed by the explicit
/// name from the spec so external tools don't have to know the internal
/// column name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionExportRow {
    pub ix: u32,
    pub ts: DateTime<Utc>,
    pub asset: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conviction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fill_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fee: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_realized: Option<f64>,
}

impl From<&DecisionRow> for DecisionExportRow {
    fn from(row: &DecisionRow) -> Self {
        Self {
            ix: row.decision_index,
            ts: row.timestamp,
            asset: row.asset.clone(),
            action: row.action.clone(),
            conviction: row.conviction,
            justification: row.justification.clone(),
            reasoning: row.reasoning.clone(),
            order_size: row.order_size,
            fill_price: row.fill_price,
            fill_size: row.fill_size,
            fee: row.fee,
            pnl_realized: row.pnl_realized,
        }
    }
}

/// Single (timestamp, equity) point on the equity curve. Mirrors the
/// `eval_equity_samples` rows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquitySample {
    pub ts: DateTime<Utc>,
    pub equity_usd: f64,
}

/// Single run-level error entry. Today there is at most one (the
/// terminal `eval_runs.error` string); the array shape leaves room for a
/// future per-decision or per-stage error log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunErrorEntry {
    pub message: String,
}

/// One review row + its findings, denormalized for round-trip.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewExportRow {
    pub review: EvalReview,
    pub findings: Vec<Finding>,
}

/// A single `(provider, model)` pair observed across the run's model calls,
/// with a count of how many calls used that pair.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderModel {
    pub provider: String,
    pub model: String,
    pub call_count: u64,
}

/// Provider-side diagnostics persisted alongside the run.
///
/// `attestation` is only present when the run was attested (a separate
/// `xvn eval attest` step). When an attestation is attached, its
/// `tokens_used` is authoritative — the same bytes are part of the
/// signed payload, so duplicating a divergent number elsewhere in the
/// export would let a tampered export pass a casual eyeball check. For
/// non-attested runs the counters fall back to `run.actual_*_tokens`
/// so the QA-round-trip footprint (e.g. QA15 item 5's `output_tokens=1000`
/// truncation) is still surfaced.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderDiagnostics {
    /// Aggregate input/output/total tokens for the run. Sourced from the
    /// attestation when present (authoritative), else from the run's
    /// observed counters.
    pub tokens_used: TokensUsed,
    /// Original run timestamp (attestation `ran_at` when present, else
    /// `run.started_at`).
    pub ran_at: DateTime<Utc>,
    /// Signed attestation. `None` when the run hasn't been attested yet.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attestation: Option<EvalAttestation>,
    /// Truncation/empty-output footprint mirrored from `run.error`. Only
    /// populated when the persisted error string matches one of the
    /// stable `trader_output[<tag>]:` failure classes; otherwise `None`
    /// so consumers can distinguish "no provider failure" from "unknown
    /// classification".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trader_output_failure: Option<String>,
    /// Best-effort USD cost computed from observed token counts and the
    /// provider model catalog. `None` means unknown (missing pricing,
    /// missing catalog, or mixed executable models in the same run).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Distinct `(provider, model)` pairs observed across the run's model
    /// calls. Populated from the `model_calls` rows for this run id. Empty
    /// when no model calls were recorded (e.g. all decisions came from a
    /// non-LLM baseline arm).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub providers_used: Vec<ProviderModel>,
    /// Per-launch `(provider, model)` override applied to this run via
    /// `xvn eval run --provider X --model Y` (Wave B #5). `None` for the
    /// common case where the run used the strategy's bound provider.
    /// Sourced from the `provider_override` supervisor_notes row.
    #[serde(default, rename = "override", skip_serializing_if = "Option::is_none")]
    pub override_receipt: Option<crate::api::eval::ProviderOverride>,
}

/// Build the export for a *terminal* run id (`completed` / `failed` /
/// `cancelled`). Read-only — uses the same `RunStore` and helper
/// functions the dashboard/CLI already lean on.
///
/// Rejects non-terminal runs (`queued` / `running`) with
/// `ApiError::Validation` so the snapshot can't capture a moving
/// in-flight state. The contract scope explicitly excludes streaming
/// export for in-flight runs; the UI gates the "Download JSON" button
/// the same way, and this guard makes the rule load-bearing rather
/// than convention-only.
///
/// Returns `ApiError::NotFound` when the run id is unknown. Other
/// failures (deleted scenario, deleted agents, etc.) degrade the
/// matching field to `None` / `[]` rather than failing the whole export
/// — the goal is a usable QA artifact even when downstream records have
/// been cleaned up.
pub async fn build_export(ctx: &ApiContext, run_id: &str) -> ApiResult<EvalRunExport> {
    let store = RunStore::new(ctx.db.clone());
    let run = api::eval::get(ctx, run_id).await?;
    if !is_terminal(run.status) {
        return Err(ApiError::Validation(format!(
            "run {} is in status `{}`; export is only defined for terminal runs (completed/failed/cancelled/disconnected)",
            run.id,
            run.status.as_str(),
        )));
    }

    // Scenario / strategy / agents are best-effort; cleanup paths may
    // have removed them after the run completed.
    let scenario = scenario_store::get_scenario(ctx, &run.scenario_id)
        .await
        .ok()
        .flatten();
    let strategy = api::strategy::get(ctx, &run.agent_id).await.ok();
    let agents = match &strategy {
        Some(s) => load_strategy_agents(ctx, s).await,
        None => Vec::new(),
    };

    let decisions: Vec<DecisionExportRow> = store
        .read_decisions(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("read_decisions: {e:#}")))?
        .iter()
        .map(DecisionExportRow::from)
        .collect();

    let equity_samples: Vec<EquitySample> = store
        .read_equity_curve(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("read_equity_curve: {e:#}")))?
        .into_iter()
        .map(|(ts, equity_usd)| EquitySample { ts, equity_usd })
        .collect();

    let filter_events = store
        .read_filter_events(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("read_filter_events: {e:#}")))?;
    let filter_summaries = store
        .read_filter_summaries(&run.id)
        .await
        .map_err(|e| ApiError::Internal(format!("read_filter_summaries: {e:#}")))?;
    let events = filter_events
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| ApiError::Internal(format!("serialize filter_events: {e:#}")))?;

    let reviews: Vec<ReviewExportRow> = match store.list_reviews_for_run(&run.id).await {
        Ok(rs) => {
            let mut out = Vec::with_capacity(rs.len());
            for review in rs {
                let findings = store
                    .read_findings_for_review(&review.id)
                    .await
                    .unwrap_or_default();
                out.push(ReviewExportRow { review, findings });
            }
            out
        }
        Err(_) => Vec::new(),
    };

    let attestation = store.get_attestation(&run.id).await.ok().flatten();
    let cost_usd = compute_export_cost_usd(ctx, &run, strategy.as_ref(), &agents).await;
    let providers_used = load_providers_used(&ctx.db, &run.id).await;
    let override_receipt = api::eval::load_provider_override(ctx, &run.id).await;
    let provider_diagnostics =
        build_provider_diagnostics(&run, attestation, cost_usd, providers_used, override_receipt);

    // Emit the provider-mismatch finding (best-effort, idempotent). We
    // do this at export-build time because the export is the moment we
    // materialise both `providers_used` and the strategy's
    // `attested_with` together. Failures are swallowed so an
    // unavailable DB row doesn't block the export.
    if let (Some(s), Some(diag)) = (strategy.as_ref(), provider_diagnostics.as_ref()) {
        emit_provider_mismatch_finding_if_needed(&store, &run.id, s, diag).await;
    }

    let errors = match run.error.as_deref() {
        Some(msg) if !msg.trim().is_empty() => vec![RunErrorEntry {
            message: msg.to_string(),
        }],
        _ => Vec::new(),
    };

    let metrics = run.metrics.clone();
    Ok(EvalRunExport {
        schema_version: SCHEMA_VERSION.to_string(),
        run,
        scenario,
        strategy,
        agents,
        metrics,
        decisions,
        equity_samples,
        events,
        filter_events,
        filter_summaries,
        errors,
        reviews,
        provider_diagnostics,
    })
}

async fn load_strategy_agents(ctx: &ApiContext, strategy: &Strategy) -> Vec<Agent> {
    if strategy.agents.is_empty() {
        return Vec::new();
    }
    let store = AgentStore::new(ctx.db.clone());
    let mut out = Vec::with_capacity(strategy.agents.len());
    for r in &strategy.agents {
        if let Ok(Some(agent)) = store.get(&r.agent_id).await {
            out.push(agent);
        }
    }
    out
}

fn build_provider_diagnostics(
    run: &Run,
    attestation: Option<EvalAttestation>,
    cost_usd: Option<f64>,
    providers_used: Vec<ProviderModel>,
    override_receipt: Option<crate::api::eval::ProviderOverride>,
) -> Option<ProviderDiagnostics> {
    // Attestation wins when present — its `tokens_used` is part of the
    // signed payload, so the export must mirror it exactly. Falling
    // back to `run.actual_*_tokens` only when the run hasn't been
    // attested keeps the export usable as a QA artifact for un-attested
    // runs without ever shipping a tokens_used envelope that disagrees
    // with the attached signed attestation.
    let tokens_used = match attestation.as_ref() {
        Some(att) => att.tokens_used.clone(),
        None => match (run.actual_input_tokens, run.actual_output_tokens) {
            (Some(input), Some(output)) => TokensUsed {
                input,
                output,
                total: input.saturating_add(output),
            },
            // Run has no observed usage and no attestation. Surface a
            // zeroed envelope so consumers can still rely on the field
            // shape — the `trader_output_failure` slot below stays
            // useful even when token counts weren't captured.
            _ => TokensUsed {
                input: 0,
                output: 0,
                total: 0,
            },
        },
    };
    let ran_at = attestation.as_ref().map(|a| a.ran_at).unwrap_or(run.started_at);

    let trader_output_failure = run
        .error
        .as_deref()
        .and_then(extract_trader_output_class)
        .map(str::to_string);

    // If nothing meaningful is set, return None so consumers can
    // distinguish "no diagnostics captured" from "captured zeros".
    if attestation.is_none()
        && tokens_used.total == 0
        && run.actual_input_tokens.is_none()
        && run.actual_output_tokens.is_none()
        && trader_output_failure.is_none()
        && cost_usd.is_none()
        && providers_used.is_empty()
        && override_receipt.is_none()
    {
        return None;
    }

    Some(ProviderDiagnostics {
        tokens_used,
        ran_at,
        attestation,
        trader_output_failure,
        cost_usd,
        providers_used,
        override_receipt,
    })
}

/// Query the `model_calls` table (via the observability span join) for all
/// calls belonging to this eval run, then group by `(provider, model)` and
/// return the distinct pairs with call counts.
///
/// Uses the same join path as `eval::cost::aggregate_eval_run_inference_cost`:
///   `eval_runs.id → agent_runs.eval_run_id → spans.run_id → model_calls.span_id`
///
/// Returns an empty vec on any error (tables may not exist in old test
/// contexts, or the run may have had no agent involved).
///
/// This is `pub` so the CLI `xvn eval show` can surface the pairs in its
/// text output without going through the heavier `build_export` path.
pub async fn load_providers_used(pool: &sqlx::SqlitePool, eval_run_id: &str) -> Vec<ProviderModel> {
    let rows: Result<Vec<(String, String, i64)>, _> = sqlx::query_as(
        "SELECT mc.provider, mc.model, COUNT(*) AS call_count \
         FROM model_calls mc \
         JOIN spans s ON s.id = mc.span_id \
         JOIN agent_runs ar ON ar.id = s.run_id \
         WHERE ar.eval_run_id = ? \
         GROUP BY mc.provider, mc.model \
         ORDER BY call_count DESC, mc.provider ASC, mc.model ASC",
    )
    .bind(eval_run_id)
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => rows
            .into_iter()
            .map(|(provider, model, call_count)| ProviderModel {
                provider,
                model,
                call_count: call_count as u64,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Emit a `provider_mismatch` warning finding when the strategy's
/// `trader_slot.attested_with` names a model that is not present in
/// `providers_used`. Best-effort and idempotent: a pre-existing finding of
/// the same kind for this run is detected by scanning existing findings, and
/// a second emission is skipped.
///
/// `attested_with` uses the dot-separated form `"provider.model"` (e.g.
/// `"anthropic.claude-sonnet-4.6"`). The check compares
/// `format!("{}.{}", pm.provider, pm.model)` against the requirement string
/// after trimming.
async fn emit_provider_mismatch_finding_if_needed(
    store: &RunStore,
    run_id: &str,
    strategy: &Strategy,
    diag: &ProviderDiagnostics,
) {
    let required = match strategy
        .trader_slot
        .as_ref()
        .map(|s| s.attested_with.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        Some(r) => r,
        None => return, // no attested_with set — nothing to compare
    };

    if diag.providers_used.is_empty() {
        return; // no model calls observed — cannot make the comparison
    }

    // Check if the required model was actually used.
    let matched = diag
        .providers_used
        .iter()
        .any(|pm| format!("{}.{}", pm.provider, pm.model) == required);
    if matched {
        return; // requirement satisfied — no finding needed
    }

    // Idempotency: skip if a provider_mismatch finding already exists for this run.
    let existing = store.read_findings(run_id).await.unwrap_or_default();
    if existing.iter().any(|f| f.kind == "provider_mismatch") {
        return;
    }

    let actual_list = diag
        .providers_used
        .iter()
        .map(|pm| format!("{}/{} ({}x)", pm.provider, pm.model, pm.call_count))
        .collect::<Vec<_>>()
        .join(", ");

    let body = format!(
        "The strategy's trader_slot.attested_with is \"{required}\" but the run's model calls \
         used: {actual_list}. \
         attested_with is advisory — the operator may rebind the agent to the intended \
         provider/model or update the strategy manifest's attested_with to reflect the \
         provider actually in use.",
    );

    let finding = Finding {
        id: Ulid::new().to_string(),
        run_id: run_id.to_string(),
        kind: "provider_mismatch".to_string(),
        severity: Severity::Warning,
        summary: format!("strategy requested {required}, run used {actual_list}"),
        evidence: serde_json::json!({
            "attested_with": required,
            "providers_used": diag.providers_used.iter().map(|pm| serde_json::json!({
                "provider": pm.provider,
                "model": pm.model,
                "call_count": pm.call_count,
            })).collect::<Vec<_>>(),
        }),
        extracted_at: Utc::now(),
        schema_version: FINDING_SCHEMA_VERSION.to_string(),
        evidence_cycle_ids: Some(vec![]),
        produced_by_check: Some("export:provider_attestation".to_string()),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: Some(format!("strategy requested {required}, run used {actual_list}")),
        description: Some(body),
        recommendation: Some(
            "Rebind the agent slot to the intended provider/model, or update \
             trader_slot.attested_with in the strategy manifest to reflect the \
             provider actually in use."
                .to_string(),
        ),
        created_at: Some(Utc::now()),
    };

    if let Err(e) = store.record_finding(&finding).await {
        tracing::warn!(
            run_id,
            error = %e,
            "provider_mismatch finding write failed (export still ok)"
        );
    }
}

async fn compute_export_cost_usd(
    ctx: &ApiContext,
    run: &Run,
    strategy: Option<&Strategy>,
    agents: &[Agent],
) -> Option<f64> {
    let input_tokens = run.actual_input_tokens?;
    let output_tokens = run.actual_output_tokens?;
    let (provider, model) = single_executable_provider_model(strategy?, agents)?;
    let catalog = crate::providers::load_cached_catalog(&ctx.xvn_home, &provider)
        .await
        .ok()
        .flatten()?;
    compute_token_cost_usd_from_catalog(input_tokens, output_tokens, &model, &catalog)
}

fn single_executable_provider_model(strategy: &Strategy, agents: &[Agent]) -> Option<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::new();

    if strategy.agents.is_empty() {
        for slot in [strategy.regime_slot.as_ref(), strategy.trader_slot.as_ref()]
            .into_iter()
            .flatten()
        {
            pairs.push(provider_model_pair(
                slot.provider.as_deref(),
                slot.model.as_deref(),
            )?);
        }
    } else {
        for agent_ref in &strategy.agents {
            let agent = agents.iter().find(|agent| agent.agent_id == agent_ref.agent_id)?;
            let slot = agent.slots.first()?;
            pairs.push(provider_model_pair(Some(&slot.provider), Some(&slot.model))?);
        }
    }

    let first = pairs.first()?.clone();
    if pairs.iter().all(|pair| pair == &first) {
        Some(first)
    } else {
        None
    }
}

fn provider_model_pair(provider: Option<&str>, model: Option<&str>) -> Option<(String, String)> {
    let provider = provider.map(str::trim).filter(|s| !s.is_empty());
    let model = model.map(str::trim).filter(|s| !s.is_empty());
    Some((provider?.to_string(), model?.to_string()))
}

fn is_terminal(status: RunStatus) -> bool {
    matches!(
        status,
        RunStatus::Completed | RunStatus::Failed | RunStatus::Cancelled | RunStatus::Disconnected,
    )
}

/// Pull the stable `trader_output[<class>]:` tag out of a persisted
/// `eval_runs.error` string. Matches the wire format set by
/// `TraderOutputError::Display` in `eval::executor::trader_output`.
fn extract_trader_output_class(message: &str) -> Option<&str> {
    let needle = "trader_output[";
    let start = message.find(needle)?;
    let after = &message[start + needle.len()..];
    let end = after.find(']')?;
    Some(&after[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_trader_output_class_handles_known_wire_format() {
        let msg = "run 01TEST decision 3: trader_output[truncated]: trader output truncated at MaxTokens before any text was emitted (stop_reason=MaxTokens, input_tokens=422, output_tokens=1000, raw_excerpt=\"<empty>\")";
        assert_eq!(extract_trader_output_class(msg), Some("truncated"));
    }

    #[test]
    fn extract_trader_output_class_returns_none_for_unrelated_errors() {
        let msg = "broker timeout: connection reset";
        assert_eq!(extract_trader_output_class(msg), None);
    }

    #[test]
    fn schema_version_pinned_at_one() {
        // The contract pins schema_version to "1" from day one. Any
        // breaking change must bump it; this test fails loudly on a
        // silent change.
        assert_eq!(SCHEMA_VERSION, "1");
    }
}

#[cfg(test)]
mod roundtrip {
    //! End-to-end canary: build_export → serialize → deserialize, then
    //! assert top-level shape + `decisions[].ix` contiguous, as required
    //! by the q15-eval-json-export contract acceptance.

    use chrono::{DateTime, Utc};
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;
    use crate::api::{Actor, ApiContext};
    use crate::eval::run::{Run, RunMode, RunStatus};
    use crate::eval::store::{DecisionRow, RunStore};
    use crate::strategies::manifest::{PublicManifest, RegimeFit};
    use crate::strategies::risk::RiskPreset;
    use crate::strategies::slot::LLMSlot;
    use crate::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use crate::strategies::{PipelineDef, Strategy};
    use xvision_core::providers::{Catalog, ModelEntry};

    async fn ctx_with_eval_tables() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("eval_export.sqlite");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&db_url)
            .await
            .unwrap();
        // Migrations the export touches transitively: 001 (api_audit),
        // 002 (eval_*), 014 (agent_id rename), 015 (reasoning column).
        // The rest (scenarios, reviews, etc.) aren't needed for the
        // top-level shape canary — get_scenario / list_reviews_for_run
        // degrade to None / [] when their tables don't exist.
        for migration in [
            include_str!("../../migrations/001_api_audit.sql"),
            include_str!("../../migrations/002_eval.sql"),
            include_str!("../../migrations/013_cli_jobs.sql"),
            include_str!("../../migrations/014_eval_agent_id.sql"),
            include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
            include_str!("../../migrations/016_eval_reviews.sql"),
            include_str!("../../migrations/022_eval_runs_agents_agent_id.sql"),
            // 027 added bars_content_hash + manifest_canonical + bars_manifest
            // columns that RunStore::create references. Pre-existing scaffold
            // gap from PR #415 — fixed alongside cli-operator-safety-p0 slice 2/3.
            include_str!("../../migrations/027_run_bars_manifest.sql"),
            include_str!("../../migrations/037_review_annotations_and_autofire.sql"),
            include_str!("../../migrations/038_eval_runs_live_config.sql"),
            // 065 added source + unrealized_pnl_usd, projected by the shared
            // RunStore get/list SELECTs (CT5 Wave 3a). Same precedent as 038.
            include_str!("../../migrations/065_eval_run_source_and_unrealized_pnl.sql"),
        ] {
            sqlx::query(migration).execute(&pool).await.unwrap();
        }
        let ctx = ApiContext::new(
            pool,
            Actor::Cli {
                user: "operator".into(),
            },
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }

    fn decision(run_id: &str, ix: u32, ts_offset: i64, action: &str) -> DecisionRow {
        DecisionRow {
            run_id: run_id.to_string(),
            decision_index: ix,
            timestamp: DateTime::<Utc>::from_timestamp(1_700_000_000 + ts_offset, 0).unwrap(),
            asset: "BTC/USD".into(),
            action: action.into(),
            conviction: Some(0.5),
            justification: Some(format!("decision {ix} justification")),
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
        }
    }

    async fn seed_completed_run(ctx: &ApiContext) -> String {
        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued("agent-A".into(), "scen-A".into(), RunMode::Backtest);
        run.status = RunStatus::Completed;
        run.actual_input_tokens = Some(422);
        run.actual_output_tokens = Some(1000);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        // Three contiguous decisions (ix=0,1,2) plus a couple of equity
        // samples so the round-trip arrays aren't empty.
        for ix in 0..3u32 {
            let d = decision(&run.id, ix, ix as i64 * 60, "hold");
            store.record_decision(&d).await.unwrap();
        }
        for (off, eq) in [(0, 100_000.0), (60, 100_500.0), (120, 99_800.0)] {
            store
                .record_equity(
                    &run.id,
                    DateTime::<Utc>::from_timestamp(1_700_000_000 + off, 0).unwrap(),
                    eq,
                )
                .await
                .unwrap();
        }
        run.id
    }

    fn openrouter_strategy(id: &str, model: &str) -> Strategy {
        let slot = LLMSlot {
            role: "trader".into(),
            attested_with: format!("openrouter.{model}"),
            allowed_tools: Vec::new(),
            provider: Some("openrouter".into()),
            model: Some(model.into()),
        };
        Strategy {
            manifest: PublicManifest {
                id: id.into(),
                display_name: "Cost strategy".into(),
                plain_summary: "x".into(),
                creator: "@test".into(),
                template: "mean_reversion".into(),
                regime_fit: vec![RegimeFit::RangeBound],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 15,
                timeframe_requirements: Default::default(),
                attested_with: vec![model.into()],
                required_tools: Vec::new(),
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            trader_slot: Some(slot),
            risk: RiskPreset::Balanced.expand(),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        }
    }

    async fn save_openrouter_catalog(ctx: &ApiContext) {
        let catalog = Catalog {
            provider: "openrouter".into(),
            fetched_at: Utc::now(),
            source_url: "https://openrouter.ai/api/v1/models".into(),
            models: vec![ModelEntry {
                id: "anthropic/claude-opus-4.7".into(),
                display_name: Some("Anthropic: Claude Opus 4.7".into()),
                context_window: Some(200_000),
                max_output_tokens: Some(8192),
                supports_reasoning: Some(true),
                supports_tools: Some(true),
                pricing_per_million_input_usd: Some(15.0),
                pricing_per_million_output_usd: Some(75.0),
                raw: serde_json::Value::Null,
            }],
        };
        crate::providers::save_cached_catalog(&ctx.xvn_home, &catalog)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn export_roundtrips_through_json_with_all_top_level_keys() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let run_id = seed_completed_run(&ctx).await;

        let export = build_export(&ctx, &run_id).await.expect("build_export");
        assert_eq!(export.schema_version, "1");
        assert_eq!(export.decisions.len(), 3);
        assert_eq!(export.equity_samples.len(), 3);

        // Round-trip: serialize → parse → match the spec's top-level keys.
        let json = serde_json::to_string(&export).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
        for key in [
            "schema_version",
            "run",
            "scenario",
            "strategy",
            "agents",
            "metrics",
            "decisions",
            "equity_samples",
            "events",
            "errors",
            "reviews",
            "provider_diagnostics",
        ] {
            assert!(
                parsed.get(key).is_some(),
                "expected top-level key `{key}` in export JSON; got: {json}",
            );
        }

        // Spec: decisions[].ix is contiguous (0,1,2,…). The executor
        // writes them in order; the export preserves that ordering.
        let ix_values: Vec<u64> = parsed["decisions"]
            .as_array()
            .expect("decisions array")
            .iter()
            .map(|d| d["ix"].as_u64().expect("ix is integer"))
            .collect();
        assert_eq!(ix_values, vec![0, 1, 2], "decisions[].ix must be contiguous");

        // Round-trip back into the typed struct — this asserts the
        // Serialize/Deserialize pair is symmetric. Bumps to the shape
        // that break round-trip surface here as a Deserialize error.
        let decoded: EvalRunExport = serde_json::from_str(&json).expect("deserialize EvalRunExport");
        assert_eq!(decoded.schema_version, export.schema_version);
        assert_eq!(decoded.decisions.len(), export.decisions.len());
    }

    #[tokio::test]
    async fn unknown_run_id_returns_not_found() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let err = build_export(&ctx, "01NOSUCHRUN0000000000000")
            .await
            .expect_err("unknown id must error");
        assert!(matches!(err, ApiError::NotFound(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn errors_array_captures_run_error_string() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "s".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        // QA15 footprint: trader_output[truncated] with stop_reason=MaxTokens.
        let trader_err = "run 01TEST decision 3: trader_output[truncated]: trader output truncated at MaxTokens before any text was emitted (stop_reason=MaxTokens, input_tokens=422, output_tokens=1000, raw_excerpt=\"<empty>\")";
        store
            .update_status(&run.id, RunStatus::Failed, Some(trader_err))
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        assert_eq!(export.errors.len(), 1);
        assert!(export.errors[0].message.contains("trader_output[truncated]"));

        // Spec acceptance: the QA15 reproducer's truncation diagnostics
        // are surfaced under provider_diagnostics, not just buried in
        // the error string.
        let diag = export
            .provider_diagnostics
            .expect("provider_diagnostics on a failed run");
        assert_eq!(diag.trader_output_failure.as_deref(), Some("truncated"));
    }

    #[tokio::test]
    async fn queued_run_export_is_validation_error() {
        // Snapshotting an in-flight run would produce a moving,
        // partial envelope. The contract scope is terminal-only;
        // build_export enforces it instead of relying on the UI to
        // hide the button.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        // No update_status call — the run stays in Queued.

        let err = build_export(&ctx, &run.id)
            .await
            .expect_err("queued run must be rejected");
        match err {
            ApiError::Validation(msg) => {
                assert!(msg.contains("queued"), "expected status in message, got: {msg}");
                assert!(
                    msg.contains("terminal"),
                    "expected terminal-only hint, got: {msg}"
                );
            }
            other => panic!("expected Validation, got: {other:?}"),
        }
    }

    #[tokio::test]
    async fn running_run_export_is_validation_error() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Running, None)
            .await
            .unwrap();

        let err = build_export(&ctx, &run.id)
            .await
            .expect_err("running run must be rejected");
        assert!(matches!(err, ApiError::Validation(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn cancelled_run_exports_successfully() {
        // `cancelled` is a terminal status; export must succeed even
        // when the run never produced metrics.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Cancelled, Some("operator stop"))
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("cancelled exports");
        assert_eq!(export.run.status.as_str(), "cancelled");
    }

    #[tokio::test]
    async fn provider_diagnostics_mirrors_attestation_when_present() {
        // When a signed attestation is attached, `tokens_used` must
        // equal `attestation.tokens_used` — the same bytes are part of
        // the signed payload, and the export must not ship a divergent
        // number in a sibling field.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());

        // Run reports 422/1000 (the QA15 footprint) but the attached
        // attestation reports a different total (e.g. a later re-run
        // or an aggregator update). The attestation wins.
        let mut run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        run.actual_input_tokens = Some(422);
        run.actual_output_tokens = Some(1000);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let attested_tokens = TokensUsed {
            input: 7_000,
            output: 3_000,
            total: 10_000,
        };
        let attestation = EvalAttestation {
            agent_id: run.agent_id.clone(),
            scenario_id: run.scenario_id.clone(),
            metrics: MetricsSummary {
                total_return_pct: 0.0,
                sharpe: 0.0,
                max_drawdown_pct: 0.0,
                win_rate: 0.0,
                n_trades: 0,
                n_decisions: 0,
                baselines: None,
                ..Default::default()
            },
            tokens_used: attested_tokens.clone(),
            ran_at: run.started_at,
            signing_pubkey_hex: "ed25519-test".into(),
            signature_hex: "sig-test".into(),
        };
        store.record_attestation(&run.id, &attestation).await.unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        let diag = export
            .provider_diagnostics
            .expect("provider_diagnostics when attestation is present");
        assert_eq!(
            diag.tokens_used, attested_tokens,
            "tokens_used must mirror the signed attestation payload, not run.actual_*"
        );
        let att = diag.attestation.expect("attestation attached");
        assert_eq!(att.tokens_used, attested_tokens);
    }

    #[tokio::test]
    async fn provider_diagnostics_falls_back_to_run_counters_without_attestation() {
        // Un-attested runs still surface the per-call truncation
        // footprint via the run's observed counters — QA15 reproducer
        // case.
        let (ctx, _d) = ctx_with_eval_tables().await;
        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued("a".into(), "crypto-bull-q1-2025".into(), RunMode::Backtest);
        run.actual_input_tokens = Some(422);
        run.actual_output_tokens = Some(1000);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        let diag = export
            .provider_diagnostics
            .expect("provider_diagnostics with observed counters");
        assert_eq!(diag.tokens_used.input, 422);
        assert_eq!(diag.tokens_used.output, 1000);
        assert_eq!(diag.tokens_used.total, 1422);
        assert!(diag.attestation.is_none(), "no attestation attached");
    }

    #[tokio::test]
    async fn provider_diagnostics_includes_catalog_priced_token_cost() {
        let (ctx, _d) = ctx_with_eval_tables().await;
        let strategy_id = "01H8N7ZCOST";
        let strategy_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        strategy_store
            .save(&openrouter_strategy(strategy_id, "anthropic/claude-opus-4.7"))
            .await
            .unwrap();
        save_openrouter_catalog(&ctx).await;

        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued(
            strategy_id.into(),
            "crypto-bull-q1-2025".into(),
            RunMode::Backtest,
        );
        run.actual_input_tokens = Some(10_000);
        run.actual_output_tokens = Some(2_000);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        let diag = export
            .provider_diagnostics
            .expect("provider_diagnostics with observed counters");
        assert_eq!(diag.tokens_used.input, 10_000);
        assert_eq!(diag.tokens_used.output, 2_000);
        assert_eq!(diag.cost_usd, Some(0.30));
    }
}

// ─── Provider-attestation tests ──────────────────────────────────────────────
//
// Unit + integration tests for eval-provider-attestation track:
//   - `providers_used` populator
//   - provider-mismatch finding emitter (match, mismatch, empty requirement,
//     empty providers_used)
//   - Integration: export JSON contains providers_used for a seeded run
#[cfg(test)]
mod provider_attestation {
    use super::*;
    use crate::api::{Actor, ApiContext};
    use crate::eval::run::{Run, RunMode, RunStatus};
    use crate::eval::store::RunStore;
    use crate::strategies::manifest::{PublicManifest, RegimeFit};
    use crate::strategies::risk::RiskPreset;
    use crate::strategies::slot::LLMSlot;
    use crate::strategies::store::{strategy_store_dir, FilesystemStore, StrategyStore};
    use crate::strategies::{PipelineDef, Strategy};
    use sqlx::sqlite::SqlitePoolOptions;

    async fn ctx_with_tables() -> (ApiContext, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("provider_attestation.sqlite");
        let db_url = format!("sqlite://{}?mode=rwc", db_path.display());
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect(&db_url)
            .await
            .unwrap();
        for migration in [
            include_str!("../../migrations/001_api_audit.sql"),
            include_str!("../../migrations/002_eval.sql"),
            include_str!("../../migrations/013_cli_jobs.sql"),
            include_str!("../../migrations/014_eval_agent_id.sql"),
            include_str!("../../migrations/015_eval_decisions_reasoning.sql"),
            // 016 seeds agent_profiles (needed by eval_reviews FK)
            include_str!("../../migrations/016_eval_reviews.sql"),
            // 017 adds review-linked columns to eval_findings
            include_str!("../../migrations/017_eval_findings_review_columns.sql"),
            include_str!("../../migrations/018_agent_run_observability.sql"),
            include_str!("../../migrations/022_eval_runs_agents_agent_id.sql"),
            // 026 adds evidence_cycle_ids_json + produced_by_check columns to
            // eval_findings which record_finding requires
            include_str!("../../migrations/026_trace_surface_foundation.sql"),
            include_str!("../../migrations/027_run_bars_manifest.sql"),
            include_str!("../../migrations/037_review_annotations_and_autofire.sql"),
            include_str!("../../migrations/038_eval_runs_live_config.sql"),
            // 065 added source + unrealized_pnl_usd, projected by the shared
            // RunStore get/list SELECTs (CT5 Wave 3a). Same precedent as 038.
            include_str!("../../migrations/065_eval_run_source_and_unrealized_pnl.sql"),
        ] {
            sqlx::query(migration).execute(&pool).await.unwrap();
        }
        let ctx = ApiContext::new(
            pool,
            Actor::Cli {
                user: "operator".into(),
            },
            dir.path().to_path_buf(),
        );
        (ctx, dir)
    }

    /// Seed fake `agent_runs`, `spans`, and `model_calls` rows so that
    /// `load_providers_used` can join through them.
    async fn seed_model_calls(
        pool: &sqlx::SqlitePool,
        eval_run_id: &str,
        calls: &[(&str, &str)], // (provider, model) list
    ) {
        // agent_runs row
        let agent_run_id = format!("ar-{eval_run_id}");
        sqlx::query(
            "INSERT OR IGNORE INTO agent_runs \
             (id, eval_run_id, objective, status, started_at, retention_mode) \
             VALUES (?, ?, 'test', 'completed', '2026-01-01T00:00:00Z', 'hash_only')",
        )
        .bind(&agent_run_id)
        .bind(eval_run_id)
        .execute(pool)
        .await
        .unwrap();

        for (i, (provider, model)) in calls.iter().enumerate() {
            let span_id = format!("sp-{eval_run_id}-{i}");
            sqlx::query(
                "INSERT OR IGNORE INTO spans \
                 (id, run_id, kind, name, status, started_at, ended_at) \
                 VALUES (?, ?, 'model.call', 'llm', 'ok', '2026-01-01T00:00:00Z', '2026-01-01T00:00:01Z')",
            )
            .bind(&span_id)
            .bind(&agent_run_id)
            .execute(pool)
            .await
            .unwrap();

            sqlx::query(
                "INSERT OR IGNORE INTO model_calls \
                 (span_id, provider, model, input_token_count, output_token_count, prompt_hash) \
                 VALUES (?, ?, ?, 10, 20, 'testhash')",
            )
            .bind(&span_id)
            .bind(provider)
            .bind(model)
            .execute(pool)
            .await
            .unwrap();
        }
    }

    // ── Unit: providers_used populator ──────────────────────────────────────

    #[tokio::test]
    async fn providers_used_groups_two_distinct_providers() {
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        // 3 calls to anthropic, 2 calls to gemini-local
        seed_model_calls(
            &ctx.db,
            &run.id,
            &[
                ("anthropic", "claude-sonnet-4.6"),
                ("anthropic", "claude-sonnet-4.6"),
                ("anthropic", "claude-sonnet-4.6"),
                ("gemini-local", "gemini-3.1-flash"),
                ("gemini-local", "gemini-3.1-flash"),
            ],
        )
        .await;

        let used = load_providers_used(&ctx.db, &run.id).await;
        assert_eq!(used.len(), 2, "two distinct (provider, model) pairs");

        // Ordered by call_count DESC
        assert_eq!(used[0].provider, "anthropic");
        assert_eq!(used[0].model, "claude-sonnet-4.6");
        assert_eq!(used[0].call_count, 3);

        assert_eq!(used[1].provider, "gemini-local");
        assert_eq!(used[1].model, "gemini-3.1-flash");
        assert_eq!(used[1].call_count, 2);
    }

    #[tokio::test]
    async fn providers_used_empty_when_no_model_calls() {
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let used = load_providers_used(&ctx.db, &run.id).await;
        assert!(used.is_empty(), "no model calls → empty providers_used");
    }

    // ── Unit: provider-mismatch finding emitter ─────────────────────────────

    fn make_diag_with_providers(providers: Vec<ProviderModel>) -> ProviderDiagnostics {
        ProviderDiagnostics {
            tokens_used: TokensUsed {
                input: 0,
                output: 0,
                total: 0,
            },
            ran_at: Utc::now(),
            attestation: None,
            trader_output_failure: None,
            cost_usd: None,
            providers_used: providers,
            override_receipt: None,
        }
    }

    fn strategy_with_requirement(attested_with: &str) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id: "test-strategy".into(),
                display_name: "Test".into(),
                plain_summary: "x".into(),
                creator: "@test".into(),
                template: "mean_reversion".into(),
                regime_fit: vec![RegimeFit::RangeBound],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 15,
                timeframe_requirements: Default::default(),
                attested_with: vec![attested_with.into()],
                required_tools: Vec::new(),
                risk_preset_or_config: "balanced".into(),
                published_at: None,
                min_warmup_bars: None,
                color: None,
                execution_mode: Default::default(),
                capital_mode: Default::default(),
            },
            hypothesis: None,
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                attested_with: attested_with.into(),
                allowed_tools: Vec::new(),
                provider: None,
                model: None,
            }),
            risk: RiskPreset::Balanced.expand(),
            activation_mode: xvision_filters::ActivationMode::EveryBar,
            filter: None,
            acknowledge_no_filter: false,
            decision_mode: Default::default(),
            mechanistic_config: None,
            briefing_indicators: Vec::new(),
            tunable_bounds: Vec::new(),
        }
    }

    #[tokio::test]
    async fn mismatch_finding_emitted_when_required_model_not_used() {
        // The xvnej-app reproducer: strategy requests anthropic.claude-sonnet-4.6
        // but the run used gemini-local/gemini-3.1-flash.
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let strategy = strategy_with_requirement("anthropic.claude-sonnet-4.6");
        let diag = make_diag_with_providers(vec![ProviderModel {
            provider: "gemini-local".into(),
            model: "gemini-3.1-flash".into(),
            call_count: 217,
        }]);

        emit_provider_mismatch_finding_if_needed(&store, &run.id, &strategy, &diag).await;

        let findings = store.read_findings(&run.id).await.unwrap();
        assert_eq!(findings.len(), 1, "exactly one finding emitted");
        let f = &findings[0];
        assert_eq!(f.kind, "provider_mismatch");
        assert_eq!(f.severity, Severity::Warning);
        assert!(
            f.summary.contains("anthropic.claude-sonnet-4.6"),
            "summary mentions required model"
        );
        assert!(
            f.summary.contains("gemini-local"),
            "summary mentions actual provider"
        );
    }

    #[tokio::test]
    async fn no_finding_when_required_model_is_used() {
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let strategy = strategy_with_requirement("anthropic.claude-sonnet-4.6");
        let diag = make_diag_with_providers(vec![ProviderModel {
            provider: "anthropic".into(),
            model: "claude-sonnet-4.6".into(),
            call_count: 100,
        }]);

        emit_provider_mismatch_finding_if_needed(&store, &run.id, &strategy, &diag).await;

        let findings = store.read_findings(&run.id).await.unwrap();
        assert!(findings.is_empty(), "no finding when requirement is satisfied");
    }

    #[tokio::test]
    async fn no_finding_when_attested_with_is_empty() {
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let strategy = strategy_with_requirement(""); // empty = no requirement
        let diag = make_diag_with_providers(vec![ProviderModel {
            provider: "gemini-local".into(),
            model: "gemini-3.1-flash".into(),
            call_count: 50,
        }]);

        emit_provider_mismatch_finding_if_needed(&store, &run.id, &strategy, &diag).await;

        let findings = store.read_findings(&run.id).await.unwrap();
        assert!(findings.is_empty(), "no finding when attested_with is empty");
    }

    #[tokio::test]
    async fn no_finding_when_providers_used_is_empty() {
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let strategy = strategy_with_requirement("anthropic.claude-sonnet-4.6");
        let diag = make_diag_with_providers(vec![]); // no model calls

        emit_provider_mismatch_finding_if_needed(&store, &run.id, &strategy, &diag).await;

        let findings = store.read_findings(&run.id).await.unwrap();
        assert!(
            findings.is_empty(),
            "no finding when providers_used is empty (non-LLM baseline run)"
        );
    }

    #[tokio::test]
    async fn mismatch_finding_is_idempotent() {
        // Calling emit twice (e.g. two export requests) must not create
        // duplicate findings.
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let strategy = strategy_with_requirement("anthropic.claude-sonnet-4.6");
        let diag = make_diag_with_providers(vec![ProviderModel {
            provider: "gemini-local".into(),
            model: "gemini-3.1-flash".into(),
            call_count: 5,
        }]);

        emit_provider_mismatch_finding_if_needed(&store, &run.id, &strategy, &diag).await;
        emit_provider_mismatch_finding_if_needed(&store, &run.id, &strategy, &diag).await;

        let findings = store.read_findings(&run.id).await.unwrap();
        assert_eq!(
            findings.iter().filter(|f| f.kind == "provider_mismatch").count(),
            1,
            "idempotency: second call must not create a duplicate finding"
        );
    }

    // ── Integration: export JSON contains providers_used for a seeded run ───

    #[tokio::test]
    async fn export_json_contains_providers_used_when_model_calls_exist() {
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let mut run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        run.actual_input_tokens = Some(100);
        run.actual_output_tokens = Some(200);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        // Seed two distinct model call providers
        seed_model_calls(
            &ctx.db,
            &run.id,
            &[
                ("anthropic", "claude-sonnet-4.6"),
                ("openrouter", "anthropic/claude-haiku-4.5"),
                ("openrouter", "anthropic/claude-haiku-4.5"),
            ],
        )
        .await;

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        let diag = export
            .provider_diagnostics
            .as_ref()
            .expect("provider_diagnostics must be present");
        assert_eq!(diag.providers_used.len(), 2, "two distinct pairs");

        // Verify the JSON shape
        let json = serde_json::to_string(&export).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
        let pu = &parsed["provider_diagnostics"]["providers_used"];
        assert!(pu.is_array(), "providers_used must be a JSON array in the export");
        let arr = pu.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        // Each entry has provider, model, call_count
        for entry in arr {
            assert!(entry.get("provider").is_some());
            assert!(entry.get("model").is_some());
            assert!(entry.get("call_count").is_some());
        }
    }

    #[tokio::test]
    async fn build_export_emits_provider_mismatch_finding_for_saved_strategy() {
        let (ctx, _dir) = ctx_with_tables().await;
        let strategy = strategy_with_requirement("anthropic.claude-sonnet-4.6");
        let strategy_id = strategy.manifest.id.clone();
        let strategy_store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
        strategy_store.save(&strategy).await.unwrap();

        let store = RunStore::new(ctx.db.clone());
        let mut run = Run::new_queued(strategy_id, "sc".into(), RunMode::Backtest);
        run.actual_input_tokens = Some(100);
        run.actual_output_tokens = Some(200);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        seed_model_calls(&ctx.db, &run.id, &[("gemini-local", "gemini-3.1-flash")]).await;

        build_export(&ctx, &run.id).await.expect("build_export");

        let findings = store.read_findings(&run.id).await.unwrap();
        assert_eq!(
            findings.iter().filter(|f| f.kind == "provider_mismatch").count(),
            1,
            "build_export should persist one provider_mismatch finding"
        );
    }

    #[tokio::test]
    async fn export_json_omits_providers_used_when_no_model_calls() {
        // `skip_serializing_if = "Vec::is_empty"` must keep the JSON clean
        let (ctx, _dir) = ctx_with_tables().await;
        let store = RunStore::new(ctx.db.clone());

        let mut run = Run::new_queued("s".into(), "sc".into(), RunMode::Backtest);
        run.actual_input_tokens = Some(10);
        run.actual_output_tokens = Some(20);
        store.create(&run).await.unwrap();
        store
            .update_status(&run.id, RunStatus::Completed, None)
            .await
            .unwrap();

        let export = build_export(&ctx, &run.id).await.expect("build_export");
        let json = serde_json::to_string(&export).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("parse");
        // providers_used must be absent when empty (skip_serializing_if)
        let pu = parsed["provider_diagnostics"].get("providers_used");
        assert!(pu.is_none(), "providers_used must be absent from JSON when empty");
    }
}
