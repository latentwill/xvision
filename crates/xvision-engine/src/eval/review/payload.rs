//! Build the bounded review-input payload from persisted artifacts.
//!
//! The spec at `docs/superpowers/specs/2026-05-15-eval-review-agent.md`
//! defines the payload shape and the rule that nothing may be invented —
//! if the engine has no orders, positions, market metadata, or logs, those
//! arrays stay empty and the prompt is responsible for telling the model
//! exactly what is and is not present.
//!
//! The payload also carries a `valid_evidence_refs` allowlist consumed by
//! `parser::parse_review_output` to reject findings whose evidence
//! references aren't grounded in the data we actually gave the model.

use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::BTreeSet;

use crate::eval::run::{Run, RunMode, RunStatus};
use crate::eval::store::DecisionRow;

use super::AgentProfile;

/// Maximum number of decisions included in the review payload. Runs with
/// thousands of decisions (e.g. high-frequency or multi-year backtests)
/// can overflow the model's context window; we sample uniformly when
/// the count exceeds this cap.
const MAX_DECISIONS: usize = 120;

/// Maximum number of equity curve samples included in the review payload.
/// Sampled uniformly when the count exceeds this cap.
const MAX_EQUITY_POINTS: usize = 240;

/// Top-level payload handed to the review agent. Owns the strict
/// allowlist of legal evidence references; the parser rejects any finding
/// that references something outside this set.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewPayload {
    pub eval_run_id: String,
    pub agent_id: String,
    pub scenario_id: String,
    pub mode: String,
    pub status: String,
    pub metrics: serde_json::Value,
    pub equity_curve: Vec<EquityPoint>,
    pub decisions: Vec<DecisionSummary>,
    pub events: Vec<serde_json::Value>,
    pub errors: Vec<String>,
    pub agent_profile: ReviewProfileSummary,
    pub scenario: Option<ReviewScenarioSummary>,
    /// Allowlist of evidence-reference strings that findings may cite.
    /// Skipped on the wire because the model doesn't see it; the parser
    /// uses it to reject hallucinated references.
    #[serde(skip_serializing)]
    pub valid_evidence_refs: BTreeSet<String>,
    /// True when the payload is too thin to support a substantive review
    /// (no metrics, no decisions, no equity). The engine track converts
    /// any review on a sparse payload to `inconclusive` rather than
    /// asking the model to invent findings.
    #[serde(skip_serializing)]
    pub is_sparse: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct EquityPoint {
    pub timestamp: String,
    pub equity_usd: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionSummary {
    pub decision_index: u32,
    pub timestamp: String,
    pub asset: String,
    pub action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conviction: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub justification: Option<String>,
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

/// Public subset of `AgentProfile` that the model needs to see (id, model,
/// temperature, max_tokens). The system prompt is applied separately, not
/// embedded in the payload.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewProfileSummary {
    pub id: String,
    #[serde(rename = "type")]
    pub profile_type: String,
    pub provider: String,
    pub model: String,
    pub temperature: f64,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewScenarioSummary {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub asset: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub granularity: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

/// Uniformly sample `items` down to at most `max_count` elements, always
/// keeping the first and last elements. Returns the items as-is when
/// `items.len() <= max_count`.
fn capped_sample<T: Clone>(items: &[T], max_count: usize) -> Vec<T> {
    let n = items.len();
    if n <= max_count || max_count == 0 {
        return items.to_vec();
    }
    // We need at least 2 to keep first + last, and the math below
    // requires max_count >= 2.
    if max_count < 2 {
        return vec![items[0].clone()];
    }
    let step = (n - 1) as f64 / (max_count - 1) as f64;
    let mut sampled = Vec::with_capacity(max_count);
    for i in 0..max_count {
        let idx = (i as f64 * step).round() as usize;
        // SAFETY: `step = (n-1)/(max_count-1)`, so the maximum index is
        // `(max_count-1) * (n-1)/(max_count-1) = n-1`, which is in bounds.
        sampled.push(items[idx].clone());
    }
    sampled
}

/// Build the review payload. `equity_curve` is `(timestamp, equity_usd)`
/// as returned by `RunStore::read_equity_curve`. Decisions and scenario
/// are passed in by the caller so this stays a pure function — the engine
/// orchestrator does the DB I/O.
pub fn build_review_payload(
    run: &Run,
    decisions: Vec<DecisionRow>,
    equity_curve: Vec<(DateTime<Utc>, f64)>,
    scenario: Option<ReviewScenarioSummary>,
    profile: &AgentProfile,
) -> ReviewPayload {
    let metrics = run
        .metrics
        .as_ref()
        .map(|m| serde_json::to_value(m).unwrap_or(serde_json::Value::Null))
        .unwrap_or(serde_json::Value::Null);

    let equity_curve: Vec<EquityPoint> = capped_sample(&equity_curve, MAX_EQUITY_POINTS)
        .into_iter()
        .map(|(ts, equity_usd)| EquityPoint {
            timestamp: ts.to_rfc3339(),
            equity_usd,
        })
        .collect();

    let decisions: Vec<DecisionSummary> = capped_sample(&decisions, MAX_DECISIONS)
        .into_iter()
        .map(|d| DecisionSummary {
            decision_index: d.decision_index,
            timestamp: d.timestamp.to_rfc3339(),
            asset: d.asset,
            action: d.action,
            conviction: d.conviction,
            justification: d.justification,
            order_size: d.order_size,
            fill_price: d.fill_price,
            fill_size: d.fill_size,
            fee: d.fee,
            pnl_realized: d.pnl_realized,
        })
        .collect();

    let errors = run.error.as_ref().map(|e| vec![e.clone()]).unwrap_or_default();

    let valid_evidence_refs = build_evidence_allowlist(&metrics, &equity_curve, &decisions);

    let is_sparse = metrics_is_empty(&metrics) && equity_curve.is_empty() && decisions.is_empty();

    ReviewPayload {
        eval_run_id: run.id.clone(),
        agent_id: run.agent_id.clone(),
        scenario_id: run.scenario_id.clone(),
        mode: run_mode_str(run.mode).into(),
        status: run_status_str(run.status).into(),
        metrics,
        equity_curve,
        decisions,
        events: Vec::new(),
        errors,
        agent_profile: ReviewProfileSummary {
            id: profile.id.clone(),
            profile_type: profile.profile_type.clone(),
            provider: profile.provider.clone(),
            model: profile.model.clone(),
            temperature: profile.temperature,
            max_tokens: profile.max_tokens,
        },
        scenario,
        valid_evidence_refs,
        is_sparse,
    }
}

fn run_mode_str(mode: RunMode) -> &'static str {
    match mode {
        RunMode::Backtest => "backtest",
        RunMode::Live => "live",
    }
}

fn run_status_str(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Queued => "queued",
        RunStatus::Running => "running",
        RunStatus::Completed => "completed",
        RunStatus::Failed => "failed",
        RunStatus::Cancelled => "cancelled",
        RunStatus::Disconnected => "disconnected",
    }
}

fn metrics_is_empty(metrics: &serde_json::Value) -> bool {
    match metrics {
        serde_json::Value::Null => true,
        serde_json::Value::Object(map) => map.is_empty(),
        _ => false,
    }
}

/// Compose the legal evidence-reference allowlist. The model may cite:
///
/// * `metric:<key>` for any top-level field of `metrics`
///   (e.g. `metric:sharpe`).
/// * `decision:<index>` for any decision in the payload
///   (`decision:0` … `decision:N-1`).
/// * `equity:<index>` for any equity sample (`equity:0` … `equity:N-1`).
/// * `equity_range:0..<N>` — the whole window, always legal when any
///   equity is present.
/// * `time_range:<start>..<end>` — anchored to first/last decision
///   or equity timestamp when present.
fn build_evidence_allowlist(
    metrics: &serde_json::Value,
    equity_curve: &[EquityPoint],
    decisions: &[DecisionSummary],
) -> BTreeSet<String> {
    let mut refs = BTreeSet::new();

    if let Some(obj) = metrics.as_object() {
        for k in obj.keys() {
            refs.insert(format!("metric:{k}"));
        }
    }

    // Use the source-of-truth `decision_index` (the column the engine
    // recorded), not the array position. The prompt promises citations
    // of the form `decision:<decision_index>`, and runs with non-zero-
    // based or sparse indices would otherwise produce correctly-grounded
    // citations that the parser would reject as hallucinations.
    for d in decisions.iter() {
        refs.insert(format!("decision:{}", d.decision_index));
    }

    if !equity_curve.is_empty() {
        for i in 0..equity_curve.len() {
            refs.insert(format!("equity:{i}"));
        }
        refs.insert(format!("equity_range:0..{}", equity_curve.len()));
    }

    let first_ts = decisions
        .first()
        .map(|d| d.timestamp.as_str())
        .or_else(|| equity_curve.first().map(|e| e.timestamp.as_str()));
    let last_ts = decisions
        .last()
        .map(|d| d.timestamp.as_str())
        .or_else(|| equity_curve.last().map(|e| e.timestamp.as_str()));
    if let (Some(start), Some(end)) = (first_ts, last_ts) {
        refs.insert(format!("time_range:{start}..{end}"));
    }

    refs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run::MetricsSummary;
    use chrono::TimeZone;

    fn sample_profile() -> AgentProfile {
        AgentProfile {
            id: "reasoning-agent".into(),
            name: "Reasoning".into(),
            profile_type: "reasoning".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            temperature: 0.2,
            max_tokens: 8000,
            system_prompt: "be helpful".into(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn sample_run() -> Run {
        let mut r = Run::new_queued("agent-1".into(), "sc-1".into(), RunMode::Backtest);
        r.status = RunStatus::Completed;
        r.metrics = Some(MetricsSummary {
            total_return_pct: 5.0,
            sharpe: 1.2,
            max_drawdown_pct: -3.0,
            win_rate: 0.55,
            n_trades: 4,
            n_decisions: 10,
            baselines: None,
            ..Default::default()
        });
        r
    }

    fn sample_decision(i: u32, ts: DateTime<Utc>) -> DecisionRow {
        DecisionRow {
            run_id: "run-1".into(),
            decision_index: i,
            timestamp: ts,
            asset: "BTC-USD".into(),
            action: "long_open".into(),
            conviction: Some(0.7),
            justification: Some("momentum".into()),
            reasoning: None,
            order_size: Some(0.01),
            fill_price: Some(50_000.0),
            fill_size: Some(0.01),
            fee: Some(1.0),
            pnl_realized: Some(0.0),
            delayed: None,
        }
    }

    #[test]
    fn payload_includes_run_decisions_metrics_and_profile() {
        let profile = sample_profile();
        let run = sample_run();
        let t0 = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let payload = build_review_payload(
            &run,
            vec![sample_decision(0, t0), sample_decision(1, t0)],
            vec![(t0, 100_000.0), (t0, 100_500.0)],
            None,
            &profile,
        );
        assert_eq!(payload.eval_run_id, run.id);
        assert_eq!(payload.mode, "backtest");
        assert_eq!(payload.status, "completed");
        assert_eq!(payload.decisions.len(), 2);
        assert_eq!(payload.equity_curve.len(), 2);
        assert_eq!(payload.agent_profile.id, "reasoning-agent");
        assert!(!payload.is_sparse);
    }

    #[test]
    fn payload_marks_empty_run_as_sparse() {
        let profile = sample_profile();
        let mut run = Run::new_queued("agent-1".into(), "sc-1".into(), RunMode::Backtest);
        run.status = RunStatus::Completed;
        run.metrics = None;
        let payload = build_review_payload(&run, vec![], vec![], None, &profile);
        assert!(payload.is_sparse);
        assert!(payload.metrics.is_null());
    }

    #[test]
    fn evidence_allowlist_uses_source_decision_index_not_array_position() {
        // If a run records decisions with non-zero-based or sparse
        // indices, the allowlist must follow the recorded indices —
        // otherwise correctly grounded citations get rejected.
        let profile = sample_profile();
        let run = sample_run();
        let t0 = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let mut d0 = sample_decision(0, t0);
        d0.decision_index = 12;
        let mut d1 = sample_decision(0, t0);
        d1.decision_index = 47;
        let payload = build_review_payload(&run, vec![d0, d1], vec![], None, &profile);
        assert!(payload.valid_evidence_refs.contains("decision:12"));
        assert!(payload.valid_evidence_refs.contains("decision:47"));
        assert!(!payload.valid_evidence_refs.contains("decision:0"));
        assert!(!payload.valid_evidence_refs.contains("decision:1"));
    }

    #[test]
    fn evidence_allowlist_covers_metric_keys_decision_indices_and_equity_range() {
        let profile = sample_profile();
        let run = sample_run();
        let t0 = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let payload = build_review_payload(
            &run,
            vec![sample_decision(0, t0)],
            vec![(t0, 100_000.0), (t0, 100_500.0), (t0, 99_000.0)],
            None,
            &profile,
        );
        assert!(payload.valid_evidence_refs.contains("metric:sharpe"));
        assert!(payload.valid_evidence_refs.contains("metric:total_return_pct"));
        assert!(payload.valid_evidence_refs.contains("decision:0"));
        assert!(!payload.valid_evidence_refs.contains("decision:1"));
        assert!(payload.valid_evidence_refs.contains("equity:0"));
        assert!(payload.valid_evidence_refs.contains("equity:2"));
        assert!(payload.valid_evidence_refs.contains("equity_range:0..3"));
    }

    #[test]
    fn capped_sample_preserves_first_and_last() {
        let items: Vec<i32> = (0..100).collect();
        let sampled = capped_sample(&items, 10);
        assert_eq!(sampled.len(), 10);
        assert_eq!(sampled[0], 0, "must keep first");
        assert_eq!(sampled[9], 99, "must keep last");
        // Middle elements should be roughly evenly spaced.
        // With 10 samples from 0..99, indices should be roughly 0,11,22,33,44,55,66,77,88,99.
        assert!(
            sampled[1] >= 8 && sampled[1] <= 14,
            "sample[1] should be ~11, got {}",
            sampled[1]
        );
        assert!(
            sampled[8] >= 85 && sampled[8] <= 91,
            "sample[8] should be ~88, got {}",
            sampled[8]
        );
    }

    #[test]
    fn capped_sample_returns_original_when_under_limit() {
        let items: Vec<i32> = (0..5).collect();
        let sampled = capped_sample(&items, 10);
        assert_eq!(sampled.len(), 5);
        assert_eq!(sampled, vec![0, 1, 2, 3, 4]);
    }

    #[test]
    fn capped_sample_empty_input() {
        let items: Vec<i32> = vec![];
        let sampled = capped_sample(&items, 10);
        assert!(sampled.is_empty());
    }
}
