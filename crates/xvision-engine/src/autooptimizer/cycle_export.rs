//! Read-side export of a complete optimizer CYCLE as a high-fidelity,
//! agent-feedable document.
//!
//! WS-11c. This is the *flywheel feedback artifact*: a self-contained record
//! of one optimizer cycle — every [`CycleProgressEvent`] (operator-labeled), the
//! per-experiment outcomes (gated Active / Suspect / Rejected + day-Sharpe
//! delta + the nested candidate eval-run id from WS-11b), the honesty check, the
//! judge findings, and the compiled DSPy flywheel summary — drivable from the
//! CLI so it's usable without the dashboard.
//!
//! Two stable deliverables, mirroring the WS-7 agent-run export pattern
//! ([`crate`]-external `xvision_observability::export`):
//!
//! - [`CycleExport`] — serializes to the [`SCHEMA_VERSION`] JSON shape. Carries
//!   the raw event sequence plus a derived per-experiment summary so an agent
//!   doesn't have to re-fold the events.
//! - [`render_cycle_report_markdown`] — a plain-text Markdown document. Header
//!   (cycle id, started/finished ts, counts), then each event in chronological
//!   order with its OPERATOR-surface label, per-experiment blocks (proposal →
//!   gate outcome + day-Sharpe delta + nested `eval_run_id`), honesty check,
//!   judge findings, and the flywheel-compiled summary.
//!
//! The export is **read-only**: it loads from `autooptimizer_events` (migration
//! 057) via the SAME query the dashboard `get_cycle_events` route uses and never
//! mutates rows.
//!
//! Operator-surface labels follow the 2026-05-27 terminology lock
//! (`docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`):
//! Mutation → "Experiment". The label map here is the canonical engine-side
//! mirror of the dashboard's `display_label`; both must stay in lock-step.

use std::fmt::Write as _;

use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::autooptimizer::progress::CycleProgressEvent;
use crate::eval::report::aggregate_run_token_totals;

/// Schema-version tag stamped onto every cycle export.
///
/// Schema-version discipline is load-bearing: a shape change bumps this tag
/// rather than mutating v1 in place, so a downstream consumer can detect the new
/// shape instead of silently breaking.
pub const SCHEMA_VERSION: &str = "xvn.optimizer_cycle.v2";

/// serde `default` for the skipped-on-deserialize `schema_version` field.
fn default_schema_version() -> &'static str {
    SCHEMA_VERSION
}

/// Returns the operator-facing display label for a [`CycleProgressEvent`].
///
/// The wire name (the `type` serde discriminant) is the persistence /
/// SSE-protocol identifier and never changes; the label is what an operator
/// reads. This is the canonical engine-side mirror of the dashboard's
/// `crates/xvision-dashboard/src/sse/autooptimizer_labels.rs::display_label`
/// (which can't be reused directly — the dashboard depends on the engine, not
/// the reverse). Keep the two in lock-step.
pub fn operator_label(event: &CycleProgressEvent) -> &'static str {
    use CycleProgressEvent::*;
    match event {
        CycleStarted { .. } => "Optimizer run started",
        ParentSelected { .. } => "Parent selected",
        MutationProposed { .. } => "Experiment proposed",
        NoCandidate { .. } => "No experiment produced",
        CandidateError { .. } => "Candidate eval failed",
        MutationGated { outcome, .. } if outcome == "suspect" => "Experiment suspect",
        MutationGated { passed: true, .. } => "Experiment kept",
        MutationGated { passed: false, .. } => "Experiment dropped",
        HonestyCheckRun { .. } => "Honesty check result",
        JudgeFinding { .. } => "Reviewer finished notes",
        CycleFinished { .. } => "Optimizer run finished",
        PhaseStarted { .. } => "Phase started",
        PhaseFinished { .. } => "Phase finished",
        EvalProgress { .. } => "Backtest progress",
        Heartbeat { .. } => "Working…",
        SessionStateChanged { .. } => "Run state changed",
        FlywheelCompiled { .. } => "Findings compiled into prompt pattern",
    }
}

/// Map the 3-way gate `outcome` wire value (`"kept"` | `"suspect"` |
/// `"dropped"`) to the operator-surface lineage label. Falls back to the
/// two-way `passed` flag when `outcome` is empty (older rows).
fn experiment_outcome_label(outcome: &str, passed: bool) -> &'static str {
    match outcome {
        "kept" => "Active",
        "suspect" => "Suspect",
        "dropped" => "Rejected",
        _ if passed => "Active",
        _ => "Rejected",
    }
}

/// One persisted event in the cycle, paired with its operator label so the JSON
/// document is self-describing (a consumer doesn't have to re-derive labels).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LabeledEvent {
    /// The operator-surface display label (e.g. "Experiment kept").
    pub label: String,
    /// The full structured event.
    pub event: CycleProgressEvent,
}

/// Derived per-experiment summary: a candidate's proposal joined to its gate
/// outcome and the nested eval-run id, so the document groups the cycle by
/// experiment instead of leaving the reader to correlate `child_hash` by hand.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ExperimentSummary {
    /// Candidate strategy blob hash (the experiment's identity within the cycle).
    pub child_hash: String,
    /// Parent the experiment was forked from, when the proposal carried it.
    pub parent_hash: Option<String>,
    /// Experiment-writer (mutator) model that produced the candidate, if known.
    pub mutator_model: Option<String>,
    /// Operator-surface gate label: "Active" | "Suspect" | "Rejected".
    /// `None` until the candidate is gated.
    pub outcome: Option<String>,
    /// Day-window Sharpe of the child minus the parent's day-window Sharpe.
    /// `None` when gate scores weren't computed.
    pub delta_day: Option<f64>,
    /// WS-11b: the persisted eval `Run.id` for this candidate's primary
    /// day-window evaluation, so an agent can drill into that run's trace.
    pub eval_run_id: Option<String>,
    /// Token usage recorded on the linked eval run, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_tokens: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_tokens: Option<u64>,
    /// Judge findings raised against this candidate (severity, code).
    pub judge_findings: Vec<JudgeFindingSummary>,
}

/// A single judge finding on an experiment (operator-surface "Reviewer note").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JudgeFindingSummary {
    pub severity: String,
    pub code: String,
}

/// Honesty-check ("honesty check") outcome derived from the
/// [`CycleProgressEvent::HonestyCheckRun`] event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HonestyCheckSummary {
    pub passed: bool,
    pub sabotage_variant: String,
    pub message: String,
}

/// Compiled-flywheel summary derived from the
/// [`CycleProgressEvent::FlywheelCompiled`] event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlywheelSummary {
    pub optimization_run_id: String,
    pub pattern_id: String,
}

/// The `xvn.optimizer_cycle.v2` payload — a complete record of one optimizer
/// cycle, JSON-serializable for the `--format json` export.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleExport {
    /// Always [`SCHEMA_VERSION`]. Skipped on deserialize (the const is the
    /// source of truth) so the struct can round-trip through serde without
    /// requiring a `'static`-lifetime input.
    #[serde(default = "default_schema_version", skip_deserializing)]
    pub schema_version: &'static str,
    pub cycle_id: String,
    /// Session that ran the cycle, when carried by the events.
    pub session_id: Option<String>,
    /// Number of parents selected this cycle (from `CycleStarted`).
    pub parent_count: Option<usize>,
    /// Final gated counts (from `CycleFinished`).
    pub active_count: Option<usize>,
    pub suspect_count: Option<usize>,
    pub rejected_count: Option<usize>,
    /// Every persisted event for the cycle, in chronological (seq) order, each
    /// paired with its operator label.
    pub events: Vec<LabeledEvent>,
    /// Per-experiment summary, grouped by `child_hash`.
    pub experiments: Vec<ExperimentSummary>,
    /// Honesty-check outcome, when the cycle ran one.
    pub honesty_check: Option<HonestyCheckSummary>,
    /// Compiled-flywheel summary, when the DSPy flywheel ran.
    pub flywheel: Option<FlywheelSummary>,
}

/// Load the persisted [`CycleProgressEvent`]s for `cycle_id` in chronological
/// order. Reuses the exact query the dashboard `get_cycle_events` route uses
/// (`WHERE cycle_id = ? ORDER BY seq ASC`) against `autooptimizer_events`.
///
/// Returns an empty vec — never an error — when the table is absent (fresh
/// install) or the cycle has no rows. Rows whose `payload_json` fails to
/// deserialize into a `CycleProgressEvent` are skipped rather than aborting the
/// whole load (forward-compat with future event kinds).
pub async fn load_cycle_events(
    pool: &SqlitePool,
    cycle_id: &str,
) -> Result<Vec<CycleProgressEvent>, sqlx::Error> {
    if !table_exists(pool, "autooptimizer_events").await? {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        "SELECT payload_json FROM autooptimizer_events \
         WHERE cycle_id = ?1 ORDER BY seq ASC LIMIT 1000",
    )
    .bind(cycle_id)
    .fetch_all(pool)
    .await?;

    let mut events = Vec::with_capacity(rows.len());
    for row in rows {
        let payload: String = row.try_get("payload_json")?;
        if let Ok(ev) = serde_json::from_str::<CycleProgressEvent>(&payload) {
            events.push(ev);
        }
    }
    Ok(events)
}

async fn table_exists(pool: &SqlitePool, table: &str) -> Result<bool, sqlx::Error> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await?;
    Ok(count > 0)
}

/// Build a [`CycleExport`] for `cycle_id` from the given pool.
pub async fn build_cycle_export(pool: &SqlitePool, cycle_id: &str) -> Result<CycleExport, sqlx::Error> {
    let events = load_cycle_events(pool, cycle_id).await?;
    let mut export = assemble_cycle_export(cycle_id, events);
    hydrate_experiment_usage(pool, &mut export).await;
    Ok(export)
}

async fn hydrate_experiment_usage(pool: &SqlitePool, export: &mut CycleExport) {
    for exp in &mut export.experiments {
        let Some(run_id) = exp.eval_run_id.as_deref() else {
            continue;
        };
        let totals = aggregate_run_token_totals(pool, run_id).await;
        exp.input_tokens = totals.input_tokens;
        exp.output_tokens = totals.output_tokens;
    }
}

fn render_tokens(input_tokens: Option<u64>, output_tokens: Option<u64>) -> Option<String> {
    match (input_tokens, output_tokens) {
        (None, None) => None,
        (Some(input), Some(output)) => Some(format!("{input} in / {output} out ({} total)", input + output)),
        (Some(input), None) => Some(format!("{input} in / — out")),
        (None, Some(output)) => Some(format!("— in / {output} out")),
    }
}

/// Fold a loaded event sequence into a [`CycleExport`]. Public so callers that
/// already hold the events (e.g. a CLI run that just produced them) don't have
/// to re-query.
pub fn assemble_cycle_export(cycle_id: &str, events: Vec<CycleProgressEvent>) -> CycleExport {
    use CycleProgressEvent::*;

    let mut session_id: Option<String> = None;
    let mut parent_count: Option<usize> = None;
    let mut active_count: Option<usize> = None;
    let mut suspect_count: Option<usize> = None;
    let mut rejected_count: Option<usize> = None;
    let mut honesty_check: Option<HonestyCheckSummary> = None;
    let mut flywheel: Option<FlywheelSummary> = None;

    // Per-experiment accumulation keyed by child_hash, preserving first-seen
    // order so the document reads in cycle order.
    let mut experiments: Vec<ExperimentSummary> = Vec::new();
    let mut index_of = std::collections::HashMap::<String, usize>::new();
    let mut entry_for = |hash: &str, experiments: &mut Vec<ExperimentSummary>| -> usize {
        if let Some(&i) = index_of.get(hash) {
            return i;
        }
        let i = experiments.len();
        experiments.push(ExperimentSummary {
            child_hash: hash.to_string(),
            ..Default::default()
        });
        index_of.insert(hash.to_string(), i);
        i
    };

    let labeled: Vec<LabeledEvent> = events
        .iter()
        .map(|ev| LabeledEvent {
            label: operator_label(ev).to_string(),
            event: ev.clone(),
        })
        .collect();

    for ev in &events {
        // Capture the first non-empty session id seen.
        if session_id.is_none() {
            let sid = event_session_id(ev);
            if let Some(s) = sid {
                if !s.is_empty() {
                    session_id = Some(s.to_string());
                }
            }
        }
        match ev {
            CycleStarted { parent_count: pc, .. } => parent_count = Some(*pc),
            CycleFinished {
                active_count: a,
                suspect_count: s,
                rejected_count: r,
                ..
            } => {
                active_count = Some(*a);
                suspect_count = Some(*s);
                rejected_count = Some(*r);
            }
            MutationProposed {
                child_hash,
                parent_hash,
                mutator_model,
                ..
            } if !child_hash.is_empty() => {
                let i = entry_for(child_hash, &mut experiments);
                if !parent_hash.is_empty() {
                    experiments[i].parent_hash = Some(parent_hash.clone());
                }
                if !mutator_model.is_empty() {
                    experiments[i].mutator_model = Some(mutator_model.clone());
                }
            }
            MutationGated {
                child_hash,
                passed,
                outcome,
                delta_day,
                eval_run_id,
                ..
            } if !child_hash.is_empty() => {
                let i = entry_for(child_hash, &mut experiments);
                experiments[i].outcome = Some(experiment_outcome_label(outcome, *passed).to_string());
                experiments[i].delta_day = *delta_day;
                experiments[i].eval_run_id = eval_run_id.clone();
            }
            JudgeFinding {
                child_hash,
                severity,
                code,
                ..
            } if !child_hash.is_empty() => {
                let i = entry_for(child_hash, &mut experiments);
                experiments[i].judge_findings.push(JudgeFindingSummary {
                    severity: severity.clone(),
                    code: code.clone(),
                });
            }
            HonestyCheckRun {
                passed,
                sabotage_variant,
                message,
                ..
            } => {
                honesty_check = Some(HonestyCheckSummary {
                    passed: *passed,
                    sabotage_variant: sabotage_variant.clone(),
                    message: message.clone(),
                });
            }
            FlywheelCompiled {
                optimization_run_id,
                pattern_id,
                ..
            } => {
                flywheel = Some(FlywheelSummary {
                    optimization_run_id: optimization_run_id.clone(),
                    pattern_id: pattern_id.clone(),
                });
            }
            _ => {}
        }
    }

    CycleExport {
        schema_version: SCHEMA_VERSION,
        cycle_id: cycle_id.to_string(),
        session_id,
        parent_count,
        active_count,
        suspect_count,
        rejected_count,
        events: labeled,
        experiments,
        honesty_check,
        flywheel,
    }
}

/// Extract the `session_id` field from any event variant that carries one.
fn event_session_id(ev: &CycleProgressEvent) -> Option<&str> {
    use CycleProgressEvent::*;
    match ev {
        CycleStarted { session_id, .. }
        | ParentSelected { session_id, .. }
        | MutationProposed { session_id, .. }
        | NoCandidate { session_id, .. }
        | CandidateError { session_id, .. }
        | MutationGated { session_id, .. }
        | HonestyCheckRun { session_id, .. }
        | JudgeFinding { session_id, .. }
        | CycleFinished { session_id, .. }
        | PhaseStarted { session_id, .. }
        | PhaseFinished { session_id, .. }
        | EvalProgress { session_id, .. }
        | Heartbeat { session_id, .. }
        | FlywheelCompiled { session_id, .. } => Some(session_id),
        SessionStateChanged { session_id, .. } => Some(session_id),
    }
}

/// Render a loaded event sequence as a complete Markdown document. Public so a
/// CLI run that just produced the events can render without re-querying.
pub fn render_cycle_report_markdown(cycle_id: &str, events: &[CycleProgressEvent]) -> String {
    let export = assemble_cycle_export(cycle_id, events.to_vec());
    render_cycle_export_markdown(&export)
}

/// Render an already-assembled [`CycleExport`] as Markdown.
pub fn render_cycle_export_markdown(export: &CycleExport) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# Optimizer cycle `{}`", export.cycle_id);
    let _ = writeln!(out);
    let _ = writeln!(out, "- Schema: {}", export.schema_version);
    if let Some(ref sid) = export.session_id {
        let _ = writeln!(out, "- Session: {sid}");
    }

    // Started / finished timestamps come from the first/last event rows; the
    // events themselves don't carry a wall-clock ts, but the first and last
    // operator-labeled lines bound the cycle. Surface the event-count and
    // boundary labels so the header is scannable.
    if let Some(first) = export.events.first() {
        let _ = writeln!(out, "- Started: {}", first.label);
    }
    if let Some(last) = export.events.last() {
        let _ = writeln!(out, "- Finished: {}", last.label);
    }
    let _ = writeln!(out, "- Events: {}", export.events.len());

    let counts = (
        export.parent_count,
        export.active_count,
        export.suspect_count,
        export.rejected_count,
    );
    if let Some(pc) = counts.0 {
        let _ = writeln!(out, "- Parents selected: {pc}");
    }
    if export.active_count.is_some() || export.suspect_count.is_some() || export.rejected_count.is_some() {
        let _ = writeln!(
            out,
            "- Outcome: {} Active · {} Suspect · {} Rejected",
            export.active_count.unwrap_or(0),
            export.suspect_count.unwrap_or(0),
            export.rejected_count.unwrap_or(0),
        );
    }
    let _ = writeln!(out);

    if export.events.is_empty() {
        let _ = writeln!(out, "_No events recorded for cycle `{}`._", export.cycle_id);
        return out;
    }

    // ── Per-experiment summary ───────────────────────────────────────────────
    if !export.experiments.is_empty() {
        let _ = writeln!(out, "## Experiments");
        let _ = writeln!(out);
        for (i, exp) in export.experiments.iter().enumerate() {
            let _ = writeln!(out, "### Experiment {} — `{}`", i + 1, exp.child_hash);
            let _ = writeln!(out);
            if let Some(ref parent) = exp.parent_hash {
                let _ = writeln!(out, "- Parent: `{parent}`");
            }
            if let Some(ref model) = exp.mutator_model {
                let _ = writeln!(out, "- Experiment writer: {model}");
            }
            match exp.outcome.as_deref() {
                Some(outcome) => {
                    let _ = writeln!(out, "- Gate outcome: {outcome}");
                }
                None => {
                    let _ = writeln!(out, "- Gate outcome: (not gated)");
                }
            }
            match exp.delta_day {
                Some(d) => {
                    let _ = writeln!(out, "- Day-Sharpe delta (vs parent): {d:+.4}");
                }
                None => {
                    let _ = writeln!(out, "- Day-Sharpe delta (vs parent): —");
                }
            }
            match exp.eval_run_id.as_deref() {
                Some(run_id) => {
                    let _ = writeln!(
                        out,
                        "- Eval run: `{run_id}` (drill into this run's trace for the candidate)"
                    );
                }
                None => {
                    let _ = writeln!(out, "- Eval run: —");
                }
            }
            if let Some(tokens) = render_tokens(exp.input_tokens, exp.output_tokens) {
                let _ = writeln!(out, "- Tokens: {tokens}");
            }
            if !exp.judge_findings.is_empty() {
                let _ = writeln!(out, "- Reviewer findings:");
                for f in &exp.judge_findings {
                    let _ = writeln!(out, "  - [{}] {}", f.severity, f.code);
                }
            }
            let _ = writeln!(out);
        }
    }

    // ── Honesty check ────────────────────────────────────────────────────────
    if let Some(ref hc) = export.honesty_check {
        let _ = writeln!(out, "## Honesty check");
        let _ = writeln!(out);
        let _ = writeln!(
            out,
            "- Result: {} (sabotage `{}`)",
            if hc.passed { "passed" } else { "FAILED" },
            hc.sabotage_variant
        );
        if !hc.message.is_empty() {
            let _ = writeln!(out, "- {}", hc.message);
        }
        let _ = writeln!(out);
    }

    // ── Compiled flywheel ────────────────────────────────────────────────────
    if let Some(ref fw) = export.flywheel {
        let _ = writeln!(out, "## Findings compiled into prompt pattern");
        let _ = writeln!(out);
        let _ = writeln!(out, "- Optimization run: `{}`", fw.optimization_run_id);
        let _ = writeln!(out, "- Pattern: `{}`", fw.pattern_id);
        let _ = writeln!(out);
    }

    // ── Full event timeline ──────────────────────────────────────────────────
    let _ = writeln!(out, "## Event timeline");
    let _ = writeln!(out);
    for le in &export.events {
        let mut detail = event_detail(&le.event);
        if let CycleProgressEvent::MutationGated {
            eval_run_id: Some(run_id),
            ..
        } = &le.event
        {
            if let Some(exp) = export
                .experiments
                .iter()
                .find(|exp| exp.eval_run_id.as_deref() == Some(run_id.as_str()))
            {
                if let Some(tokens) = render_tokens(exp.input_tokens, exp.output_tokens) {
                    if !detail.is_empty() {
                        detail.push_str(" · ");
                    }
                    let _ = write!(detail, "tokens {tokens}");
                }
            }
        }
        if detail.is_empty() {
            let _ = writeln!(out, "- **{}**", le.label);
        } else {
            let _ = writeln!(out, "- **{}** — {detail}", le.label);
        }
    }
    let _ = writeln!(out);

    out
}

/// One-line operator-readable detail for an event in the timeline, surfacing the
/// load-bearing fields (the experiment gate outcome + delta + eval run id, the
/// honesty/judge/flywheel specifics) inline.
fn event_detail(ev: &CycleProgressEvent) -> String {
    use CycleProgressEvent::*;
    match ev {
        CycleStarted { parent_count, .. } => format!("{parent_count} parent(s)"),
        ParentSelected { parent_hash, .. } => format!("parent `{parent_hash}`"),
        MutationProposed {
            child_hash,
            mutator_model,
            ..
        } => {
            let mut s = String::new();
            if !child_hash.is_empty() {
                let _ = write!(s, "candidate `{child_hash}`");
            }
            if !mutator_model.is_empty() {
                if !s.is_empty() {
                    s.push_str(" · ");
                }
                let _ = write!(s, "writer {mutator_model}");
            }
            s
        }
        NoCandidate { reason, .. } | CandidateError { reason, .. } => reason.clone(),
        MutationGated {
            child_hash,
            passed,
            outcome,
            delta_day,
            eval_run_id,
            ..
        } => {
            let label = experiment_outcome_label(outcome, *passed);
            let mut s = format!("`{child_hash}` → {label}");
            if let Some(d) = delta_day {
                let _ = write!(s, " · day-Sharpe Δ {d:+.4}");
            }
            if let Some(run) = eval_run_id {
                let _ = write!(s, " · eval run `{run}`");
            }
            s
        }
        HonestyCheckRun {
            passed,
            sabotage_variant,
            message,
            ..
        } => {
            let head = if *passed { "passed" } else { "FAILED" };
            if message.is_empty() {
                format!("{head} (sabotage `{sabotage_variant}`)")
            } else {
                format!("{head} (sabotage `{sabotage_variant}`) — {message}")
            }
        }
        JudgeFinding {
            child_hash,
            severity,
            code,
            ..
        } => format!("`{child_hash}` [{severity}] {code}"),
        CycleFinished {
            active_count,
            suspect_count,
            rejected_count,
            ..
        } => format!("{active_count} Active · {suspect_count} Suspect · {rejected_count} Rejected"),
        PhaseStarted { detail, .. } => detail.clone(),
        PhaseFinished { duration_ms, .. } => format!("{duration_ms}ms"),
        EvalProgress {
            decisions, elapsed_s, ..
        } => format!("{decisions} decisions · {elapsed_s}s"),
        Heartbeat { elapsed_s, .. } => format!("{elapsed_s}s"),
        SessionStateChanged { state, .. } => state.clone(),
        FlywheelCompiled {
            optimization_run_id,
            pattern_id,
            ..
        } => format!("run `{optimization_run_id}` → pattern `{pattern_id}`"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::autooptimizer::events_store::append_event;
    use sqlx::sqlite::SqlitePoolOptions;

    /// A representative cycle: started → parent → proposed → gated(kept) →
    /// honesty → judge → flywheel → finished.
    fn representative_events() -> Vec<CycleProgressEvent> {
        vec![
            CycleProgressEvent::CycleStarted {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                parent_count: 1,
            },
            CycleProgressEvent::ParentSelected {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                parent_hash: "parent-abc".into(),
            },
            CycleProgressEvent::MutationProposed {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                parent_hash: "parent-abc".into(),
                child_hash: "child-xyz".into(),
                mutator_model: "claude-haiku-4-5".into(),
            },
            CycleProgressEvent::MutationGated {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                child_hash: "child-xyz".into(),
                passed: true,
                outcome: "kept".into(),
                delta_day: Some(0.0420),
                eval_run_id: Some("01EVALRUN".into()),
                gate_reason: None,
            },
            CycleProgressEvent::HonestyCheckRun {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                passed: true,
                sabotage_variant: "kill-trades".into(),
                message: "sabotaged variant correctly rejected".into(),
            },
            CycleProgressEvent::JudgeFinding {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                child_hash: "child-xyz".into(),
                severity: "low".into(),
                code: "J001".into(),
            },
            CycleProgressEvent::FlywheelCompiled {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                optimization_run_id: "opt-run-7".into(),
                pattern_id: "pat-9".into(),
            },
            CycleProgressEvent::CycleFinished {
                session_id: "sess-1".into(),
                cycle_id: "cyc-1".into(),
                active_count: 1,
                suspect_count: 0,
                rejected_count: 0,
            },
        ]
    }

    /// The Markdown over a representative sequence carries the operator labels,
    /// the Active/Suspect/Rejected outcome, the day-Sharpe delta, the nested
    /// eval_run_id, and the flywheel summary.
    #[test]
    fn markdown_carries_labels_outcome_delta_runid_and_flywheel() {
        let md = render_cycle_report_markdown("cyc-1", &representative_events());

        // Operator-surface labels (NOT the developer wire names).
        assert!(md.contains("Optimizer run started"), "missing start label:\n{md}");
        assert!(
            md.contains("Experiment proposed"),
            "missing proposed label:\n{md}"
        );
        assert!(md.contains("Experiment kept"), "missing kept label:\n{md}");
        assert!(
            md.contains("Honesty check result"),
            "missing honesty label:\n{md}"
        );
        assert!(
            md.contains("Reviewer finished notes"),
            "missing judge label:\n{md}"
        );
        assert!(
            md.contains("Findings compiled into prompt pattern"),
            "missing flywheel label:\n{md}"
        );
        assert!(
            md.contains("Optimizer run finished"),
            "missing finished label:\n{md}"
        );

        // Developer wire names must NOT leak to the operator surface.
        assert!(!md.contains("mutation_gated"), "wire name leaked:\n{md}");
        assert!(
            !md.to_lowercase().contains("mutator"),
            "banned term leaked:\n{md}"
        );

        // Gate outcome (Active), day-Sharpe delta, nested eval run id.
        assert!(md.contains("Active"), "missing Active outcome:\n{md}");
        assert!(md.contains("+0.0420"), "missing day-Sharpe delta:\n{md}");
        assert!(md.contains("01EVALRUN"), "missing nested eval_run_id:\n{md}");

        // Flywheel summary fields.
        assert!(md.contains("opt-run-7"), "missing optimization run id:\n{md}");
        assert!(md.contains("pat-9"), "missing pattern id:\n{md}");

        // Header + cycle id.
        assert!(md.contains("Optimizer cycle `cyc-1`"), "missing header:\n{md}");
        assert!(md.contains(SCHEMA_VERSION), "missing schema version:\n{md}");
    }

    /// A gated-suspect and gated-dropped experiment surface "Suspect" /
    /// "Rejected" respectively.
    #[test]
    fn markdown_renders_suspect_and_rejected_outcomes() {
        let events = vec![
            CycleProgressEvent::MutationProposed {
                session_id: "s".into(),
                cycle_id: "c".into(),
                parent_hash: "p".into(),
                child_hash: "susp".into(),
                mutator_model: "m".into(),
            },
            CycleProgressEvent::MutationGated {
                session_id: "s".into(),
                cycle_id: "c".into(),
                child_hash: "susp".into(),
                passed: false,
                outcome: "suspect".into(),
                delta_day: Some(-0.01),
                eval_run_id: None,
                gate_reason: None,
            },
            CycleProgressEvent::MutationProposed {
                session_id: "s".into(),
                cycle_id: "c".into(),
                parent_hash: "p".into(),
                child_hash: "drop".into(),
                mutator_model: "m".into(),
            },
            CycleProgressEvent::MutationGated {
                session_id: "s".into(),
                cycle_id: "c".into(),
                child_hash: "drop".into(),
                passed: false,
                outcome: "dropped".into(),
                delta_day: None,
                eval_run_id: None,
                gate_reason: None,
            },
        ];
        let md = render_cycle_report_markdown("c", &events);
        assert!(md.contains("Suspect"), "missing Suspect:\n{md}");
        assert!(md.contains("Rejected"), "missing Rejected:\n{md}");
        assert!(md.contains("Experiment suspect"), "missing suspect label:\n{md}");
        assert!(md.contains("Experiment dropped"), "missing dropped label:\n{md}");
    }

    /// `assemble_cycle_export` folds the per-experiment summary and the
    /// honesty/flywheel/count fields, and round-trips through serde.
    #[test]
    fn build_cycle_export_round_trips_through_serde() {
        let export = assemble_cycle_export("cyc-1", representative_events());

        assert_eq!(export.cycle_id, "cyc-1");
        assert_eq!(export.session_id.as_deref(), Some("sess-1"));
        assert_eq!(export.parent_count, Some(1));
        assert_eq!(export.active_count, Some(1));
        assert_eq!(export.events.len(), 8);

        // One experiment, gated Active, delta + eval run carried through, with
        // a judge finding attached.
        assert_eq!(export.experiments.len(), 1);
        let exp = &export.experiments[0];
        assert_eq!(exp.child_hash, "child-xyz");
        assert_eq!(exp.parent_hash.as_deref(), Some("parent-abc"));
        assert_eq!(exp.outcome.as_deref(), Some("Active"));
        assert_eq!(exp.delta_day, Some(0.0420));
        assert_eq!(exp.eval_run_id.as_deref(), Some("01EVALRUN"));
        assert_eq!(exp.judge_findings.len(), 1);
        assert_eq!(exp.judge_findings[0].code, "J001");

        assert!(export.honesty_check.as_ref().unwrap().passed);
        assert_eq!(export.flywheel.as_ref().unwrap().pattern_id, "pat-9");

        // serde round-trip.
        let json = serde_json::to_string(&export).expect("serialize");
        assert!(json.contains(SCHEMA_VERSION));
        let back: CycleExport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.cycle_id, export.cycle_id);
        assert_eq!(back.experiments.len(), 1);
        assert_eq!(back.experiments[0].eval_run_id.as_deref(), Some("01EVALRUN"));
        assert_eq!(back.flywheel.unwrap().optimization_run_id, "opt-run-7");
    }

    async fn open_test_pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS autooptimizer_events (
              seq         INTEGER PRIMARY KEY AUTOINCREMENT,
              session_id  TEXT NOT NULL,
              cycle_id    TEXT,
              kind        TEXT NOT NULL,
              payload_json TEXT NOT NULL,
              ts          TEXT NOT NULL
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        pool
    }

    /// `load_cycle_events` reads seeded rows for the cycle in seq order, parses
    /// them back into `CycleProgressEvent`s, and ignores other cycles' rows.
    #[tokio::test]
    async fn load_cycle_events_reads_seeded_rows_in_order() {
        let pool = open_test_pool().await;

        // Seed the representative sequence for cyc-1 plus a stray row for cyc-2.
        for ev in representative_events() {
            let kind = serde_json::to_value(&ev).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string();
            let payload = serde_json::to_string(&ev).unwrap();
            append_event(&pool, "sess-1", Some("cyc-1"), &kind, &payload)
                .await
                .unwrap();
        }
        let stray = CycleProgressEvent::CycleStarted {
            session_id: "sess-2".into(),
            cycle_id: "cyc-2".into(),
            parent_count: 9,
        };
        append_event(
            &pool,
            "sess-2",
            Some("cyc-2"),
            "cycle_started",
            &serde_json::to_string(&stray).unwrap(),
        )
        .await
        .unwrap();

        let loaded = load_cycle_events(&pool, "cyc-1").await.unwrap();
        assert_eq!(loaded.len(), 8, "only cyc-1's rows, in order");
        assert!(
            matches!(loaded[0], CycleProgressEvent::CycleStarted { .. }),
            "first event is CycleStarted"
        );
        assert!(
            matches!(loaded[7], CycleProgressEvent::CycleFinished { .. }),
            "last event is CycleFinished"
        );

        // build_cycle_export over the pool produces the same shape.
        let export = build_cycle_export(&pool, "cyc-1").await.unwrap();
        assert_eq!(export.events.len(), 8);
        assert_eq!(export.experiments.len(), 1);
        assert_eq!(export.experiments[0].eval_run_id.as_deref(), Some("01EVALRUN"));
    }

    #[tokio::test]
    async fn build_cycle_export_hydrates_eval_run_tokens() {
        let pool = open_test_pool().await;
        sqlx::query(
            "CREATE TABLE eval_runs (
                id TEXT PRIMARY KEY,
                actual_input_tokens INTEGER,
                actual_output_tokens INTEGER
            )",
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO eval_runs (id, actual_input_tokens, actual_output_tokens) \
             VALUES ('01EVALRUN', 410969, 34665)",
        )
        .execute(&pool)
        .await
        .unwrap();
        for ev in representative_events() {
            let kind = serde_json::to_value(&ev).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string();
            append_event(
                &pool,
                "sess-1",
                Some("cyc-1"),
                &kind,
                &serde_json::to_string(&ev).unwrap(),
            )
            .await
            .unwrap();
        }

        let export = build_cycle_export(&pool, "cyc-1").await.unwrap();
        let exp = &export.experiments[0];
        assert_eq!(exp.input_tokens, Some(410_969));
        assert_eq!(exp.output_tokens, Some(34_665));

        let md = render_cycle_export_markdown(&export);
        assert!(
            md.contains("410969 in / 34665 out (445634 total)"),
            "missing per-experiment tokens:\n{md}"
        );
        assert!(
            md.contains("tokens 410969 in / 34665 out (445634 total)"),
            "missing timeline tokens:\n{md}"
        );
    }

    /// An unknown cycle yields an empty export and a graceful "no events"
    /// Markdown doc — never a panic.
    #[tokio::test]
    async fn unknown_cycle_yields_empty_export_and_graceful_markdown() {
        let pool = open_test_pool().await;
        let export = build_cycle_export(&pool, "does-not-exist").await.unwrap();
        assert!(export.events.is_empty());
        assert!(export.experiments.is_empty());

        let md = render_cycle_export_markdown(&export);
        assert!(md.contains("No events recorded"), "graceful empty doc:\n{md}");
        assert!(md.contains("does-not-exist"));
    }

    /// A missing `autooptimizer_events` table (fresh install) loads empty, not
    /// an error.
    #[tokio::test]
    async fn missing_table_loads_empty() {
        let pool = SqlitePoolOptions::new().connect("sqlite::memory:").await.unwrap();
        let events = load_cycle_events(&pool, "anything").await.unwrap();
        assert!(events.is_empty());
    }

    /// The engine operator-label map matches the dashboard's display_label
    /// strings for the labels exercised by the export (terminology lock).
    #[test]
    fn operator_labels_match_terminology_lock() {
        let kept = CycleProgressEvent::MutationGated {
            session_id: "".into(),
            cycle_id: "c".into(),
            child_hash: "h".into(),
            passed: true,
            outcome: "kept".into(),
            delta_day: None,
            eval_run_id: None,
            gate_reason: None,
        };
        assert_eq!(operator_label(&kept), "Experiment kept");
        let suspect = CycleProgressEvent::MutationGated {
            session_id: "".into(),
            cycle_id: "c".into(),
            child_hash: "h".into(),
            passed: false,
            outcome: "suspect".into(),
            delta_day: None,
            eval_run_id: None,
            gate_reason: None,
        };
        assert_eq!(operator_label(&suspect), "Experiment suspect");
    }
}
