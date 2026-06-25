//! Memory-aware findings — flag stale memory recalls that drove bad
//! per-decision outcomes.
//!
//! # Background
//!
//! The `memory-provenance-in-decisions-trace` track (PR #523, merged
//! 2026-05-22) added per-decision provenance: every time a memory-enabled
//! slot recalls items into a briefing, the dispatcher emits a
//! `RunEvent::MemoryRecall` keyed by `(run_id, decision_id, items[])`
//! into the `events` table (`kind = 'memory_recall'`).
//!
//! This module is the eval-review extractor that consumes that join.
//! For each decision in a finished run we look up the memory items
//! recalled into its briefing and the decision's outcome judgement
//! (`good` / `bad` / `inconclusive`) and emit findings:
//!
//! - **`memory_recalled_into_bad_decision`** (`severity = warning`):
//!   bad outcome + at least one recalled memory item. Body names the
//!   memory item ids so operators can trace from the finding back to
//!   the responsible patterns in the memory store.
//! - **`memory_recalled_into_good_decision`** (`severity = info`):
//!   good outcome + at least one recalled memory item. **Opt-in.**
//!   Default off — emitting an info per good decision would drown the
//!   findings list with positive noise.
//! - Inconclusive outcomes (no fill / zero pnl / missing pnl): nothing.
//!
//! # Outcome derivation
//!
//! The current store does NOT carry a per-decision outcome judgement
//! column — outcome is derived from the decision row's realized pnl:
//!
//! - `pnl_realized > 0`  → `good`
//! - `pnl_realized < 0`  → `bad`
//! - `pnl_realized == 0` or `None` → `inconclusive`
//!
//! This is intentionally coarse — flat / no-trade decisions land in
//! the inconclusive bucket so the extractor doesn't emit warnings for
//! decisions where the memory items couldn't have affected the result.
//!
//! # Read-only / advisory
//!
//! The extractor is observability only — it does NOT change recall
//! behavior, gate runs, or promote / demote memory items. Findings
//! emit on **new runs only**; retroactive backfill is out of scope.
//!
//! # Usage
//!
//! ```rust,ignore
//! let decisions = store.read_decisions(&run_id).await?;
//! let recalls = read_memory_recalls(&store, &run_id).await?;
//! let findings = detect_memory_aware_findings(
//!     &decisions,
//!     &recalls,
//!     MemoryFindingOptions::default(),
//! );
//! for f in findings {
//!     store.record_finding(&f).await?;
//! }
//! ```
//!
//! [`detect_memory_aware_findings`] is a pure function — no DB, no I/O.

use std::collections::BTreeMap;

use chrono::Utc;
use ulid::Ulid;
use xvision_observability::events::MemoryRecallEvent;

use crate::eval::findings::{Finding, Severity, FINDING_SCHEMA_VERSION};
use crate::eval::store::DecisionRow;

/// `kind` string for the bad-outcome memory finding.
pub const KIND_MEMORY_BAD_OUTCOME: &str = "memory_recalled_into_bad_decision";

/// `kind` string for the good-outcome memory finding (opt-in).
pub const KIND_MEMORY_GOOD_OUTCOME: &str = "memory_recalled_into_good_decision";

/// `produced_by_check` value attached to memory-aware findings.
pub const PRODUCED_BY_MEMORY: &str = "memory:provenance";

/// Configuration knobs for [`detect_memory_aware_findings`]. The
/// detector is pure — these are passed in rather than read from env so
/// the caller (auto-review, eval finalize, CLI) decides the policy.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryFindingOptions {
    /// Emit `info`-severity `memory_recalled_into_good_decision`
    /// findings for each good outcome that had recalled memory. Default
    /// **off** — turning this on adds one info per good decision, which
    /// drowns the findings list under positive noise on most runs. The
    /// operator can flip this on when investigating "is memory actually
    /// helping?" specifically.
    pub emit_good_outcomes: bool,
}

/// Per-decision outcome judgement. Derived from `pnl_realized` because
/// the store does not carry a separate outcome column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DecisionOutcome {
    Good,
    Bad,
    Inconclusive,
}

fn judge_outcome(d: &DecisionRow) -> DecisionOutcome {
    match d.pnl_realized {
        Some(p) if p > 0.0 => DecisionOutcome::Good,
        Some(p) if p < 0.0 => DecisionOutcome::Bad,
        _ => DecisionOutcome::Inconclusive,
    }
}

/// Pure detector. For each decision with recalled memory items + a
/// non-inconclusive outcome, emit at most one finding. Order is
/// deterministic: ascending `decision_index`, then ascending `kind`.
///
/// `recalls` is the full list of `MemoryRecallEvent`s for the run; the
/// detector groups them by `decision_id` internally. Multiple recall
/// events can target the same decision (one per memory-enabled slot
/// in the pipeline); the detector unions their item ids in the
/// resulting finding body.
pub fn detect_memory_aware_findings(
    decisions: &[DecisionRow],
    recalls: &[MemoryRecallEvent],
    opts: MemoryFindingOptions,
) -> Vec<Finding> {
    if decisions.is_empty() {
        return Vec::new();
    }

    // Group recall events by decision_id. The dispatcher emits decision_id
    // as the engine's `cycle_idx: i64`, which equals the decision row's
    // `decision_index: u32` (1:1 by construction in `xvision_engine::agent`).
    let mut by_decision: BTreeMap<i64, Vec<&MemoryRecallEvent>> = BTreeMap::new();
    for ev in recalls {
        if ev.items.is_empty() {
            // Empty recall events are recorded for timeline completeness
            // but carry no candidate items to attribute. Skip.
            continue;
        }
        by_decision.entry(ev.decision_id).or_default().push(ev);
    }

    if by_decision.is_empty() {
        return Vec::new();
    }

    let run_id = decisions[0].run_id.clone();
    let mut findings: Vec<Finding> = Vec::new();

    // Iterate decisions in ascending decision_index for deterministic output.
    let mut sorted: Vec<&DecisionRow> = decisions.iter().collect();
    sorted.sort_by_key(|d| d.decision_index);

    for d in sorted {
        let key = d.decision_index as i64;
        let Some(events) = by_decision.get(&key) else {
            continue;
        };

        let outcome = judge_outcome(d);
        match outcome {
            DecisionOutcome::Bad => {
                findings.push(build_memory_finding(
                    &run_id,
                    d,
                    events,
                    Severity::Warning,
                    KIND_MEMORY_BAD_OUTCOME,
                ));
            }
            DecisionOutcome::Good => {
                if opts.emit_good_outcomes {
                    findings.push(build_memory_finding(
                        &run_id,
                        d,
                        events,
                        Severity::Info,
                        KIND_MEMORY_GOOD_OUTCOME,
                    ));
                }
            }
            DecisionOutcome::Inconclusive => {
                // Per the contract: inconclusive outcomes emit nothing.
            }
        }
    }

    findings
}

fn build_memory_finding(
    run_id: &str,
    decision: &DecisionRow,
    events: &[&MemoryRecallEvent],
    severity: Severity,
    kind: &str,
) -> Finding {
    // Union item ids + namespaces across every recall event that targeted
    // this decision. We keep ordering stable by inserting into a Vec and
    // de-duplicating with a small set — typical k is ≤ 8, so this stays
    // cheap.
    let mut item_ids: Vec<String> = Vec::new();
    let mut namespaces: Vec<String> = Vec::new();
    for ev in events {
        if !namespaces.iter().any(|n| n == &ev.namespace) {
            namespaces.push(ev.namespace.clone());
        }
        for item in &ev.items {
            if !item_ids.iter().any(|id| id == &item.id) {
                item_ids.push(item.id.clone());
            }
        }
    }

    let pnl_text = decision
        .pnl_realized
        .map(|p| format!("{p:+.4}"))
        .unwrap_or_else(|| "unknown".to_string());

    let outcome_label = match severity {
        Severity::Warning => "bad",
        Severity::Info => "good",
        Severity::Critical => "bad", // currently unused; future-proof.
    };

    let item_list = item_ids.join(", ");

    let summary = format!(
        "Decision {} ({} outcome, pnl={}) had {} memory item(s) recalled into its briefing: {}",
        decision.decision_index,
        outcome_label,
        pnl_text,
        item_ids.len(),
        item_list,
    );

    let description = match severity {
        Severity::Warning => Some(format!(
            "Decision {idx} closed with a losing realized pnl ({pnl}) and was driven (in part) \
             by {n} recalled memory item(s): [{ids}]. The recalled patterns may be stale — \
             review them in the memory store and consider demoting or rewriting any that \
             no longer reflect current market behavior. This finding is advisory; no recall \
             behavior is changed.",
            idx = decision.decision_index,
            pnl = pnl_text,
            n = item_ids.len(),
            ids = item_list,
        )),
        Severity::Info => Some(format!(
            "Decision {idx} closed with a winning realized pnl ({pnl}) and was driven (in part) \
             by {n} recalled memory item(s): [{ids}]. Surfaced because the operator opted in \
             to good-outcome memory findings; useful for auditing whether the memory store is \
             actually contributing to wins.",
            idx = decision.decision_index,
            pnl = pnl_text,
            n = item_ids.len(),
            ids = item_list,
        )),
        Severity::Critical => None,
    };

    let now = Utc::now();
    Finding {
        id: Ulid::new().to_string(),
        run_id: run_id.to_string(),
        kind: kind.to_string(),
        severity,
        summary: summary.clone(),
        evidence: serde_json::json!({
            "decision_index": decision.decision_index,
            "pnl_realized": decision.pnl_realized,
            "outcome": outcome_label,
            "namespaces": namespaces,
            "memory_item_ids": item_ids,
            "produced_by_check": PRODUCED_BY_MEMORY,
        }),
        extracted_at: now,
        schema_version: FINDING_SCHEMA_VERSION.to_string(),
        evidence_cycle_ids: Some(vec![]),
        produced_by_check: Some(PRODUCED_BY_MEMORY.to_string()),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: Some(match severity {
            Severity::Warning => "Memory recalled into bad decision".to_string(),
            Severity::Info => "Memory recalled into good decision".to_string(),
            Severity::Critical => "Memory recall finding".to_string(),
        }),
        description,
        recommendation: match severity {
            Severity::Warning => Some(
                "Open the named memory items in the workspace memory store; demote or rewrite \
                 patterns that no longer hold. Re-run after editing to see whether the same \
                 decision still loses without the stale recall."
                    .to_string(),
            ),
            _ => None,
        },
        created_at: Some(now),
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use xvision_observability::events::{MemoryRecallEvent, MemoryRecallItem};

    fn decision(run_id: &str, idx: u32, pnl: Option<f64>) -> DecisionRow {
        DecisionRow {
            run_id: run_id.to_string(),
            decision_index: idx,
            timestamp: Utc::now(),
            asset: "BTC/USDT".to_string(),
            action: "long_open".to_string(),
            conviction: Some(0.5),
            justification: Some("test".to_string()),
            reasoning: None,
            order_size: Some(1.0),
            fill_price: Some(100.0),
            fill_size: Some(1.0),
            fee: Some(0.0),
            pnl_realized: pnl,
            delayed: None,
        }
    }

    fn recall(run_id: &str, decision_id: i64, ns: &str, ids: &[&str]) -> MemoryRecallEvent {
        MemoryRecallEvent {
            run_id: run_id.to_string(),
            flywheel_cycle_id: None,
            decision_id,
            namespace: ns.to_string(),
            items: ids
                .iter()
                .map(|id| MemoryRecallItem {
                    id: id.to_string(),
                    score: 0.5,
                    text_preview: format!("preview of {id}"),
                })
                .collect(),
        }
    }

    #[test]
    fn empty_decisions_produces_no_findings() {
        let findings = detect_memory_aware_findings(&[], &[], MemoryFindingOptions::default());
        assert!(findings.is_empty());
    }

    #[test]
    fn bad_outcome_with_recall_emits_warning_naming_item_ids() {
        let decisions = vec![decision("run-a", 3, Some(-12.5))];
        let recalls = vec![recall("run-a", 3, "agent:alpha", &["mem-1", "mem-2"])];

        let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());

        assert_eq!(findings.len(), 1);
        let f = &findings[0];
        assert_eq!(f.kind, KIND_MEMORY_BAD_OUTCOME);
        assert_eq!(f.severity, Severity::Warning);
        assert_eq!(f.run_id, "run-a");
        // Body must name the memory item ids so operators can trace
        // findings back to the responsible patterns.
        assert!(
            f.summary.contains("mem-1") && f.summary.contains("mem-2"),
            "summary should name both item ids: {}",
            f.summary
        );
        // Evidence must carry the structured list for programmatic consumers.
        let ids = f
            .evidence
            .get("memory_item_ids")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        assert_eq!(ids.len(), 2);
    }

    #[test]
    fn good_outcome_default_off_produces_no_finding() {
        let decisions = vec![decision("run-b", 1, Some(8.0))];
        let recalls = vec![recall("run-b", 1, "global", &["mem-x"])];

        let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());
        assert!(
            findings.is_empty(),
            "good outcomes must not emit by default, got: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn good_outcome_opt_in_emits_info() {
        let decisions = vec![decision("run-c", 1, Some(8.0))];
        let recalls = vec![recall("run-c", 1, "global", &["mem-x"])];

        let findings = detect_memory_aware_findings(
            &decisions,
            &recalls,
            MemoryFindingOptions {
                emit_good_outcomes: true,
            },
        );
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, KIND_MEMORY_GOOD_OUTCOME);
        assert_eq!(findings[0].severity, Severity::Info);
        assert!(findings[0].summary.contains("mem-x"));
    }

    #[test]
    fn inconclusive_outcome_emits_nothing() {
        // pnl == 0 → inconclusive.
        let mut decisions = vec![decision("run-d", 1, Some(0.0))];
        // pnl == None → also inconclusive.
        decisions.push(decision("run-d", 2, None));
        let recalls = vec![
            recall("run-d", 1, "agent:a", &["m1"]),
            recall("run-d", 2, "agent:a", &["m2"]),
        ];

        let findings = detect_memory_aware_findings(
            &decisions,
            &recalls,
            MemoryFindingOptions {
                emit_good_outcomes: true,
            },
        );
        assert!(
            findings.is_empty(),
            "inconclusive outcomes must emit nothing under any option, got: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    #[test]
    fn decision_without_recall_emits_nothing() {
        let decisions = vec![decision("run-e", 1, Some(-5.0))];
        // No recall events at all.
        let findings = detect_memory_aware_findings(&decisions, &[], MemoryFindingOptions::default());
        assert!(findings.is_empty());
    }

    #[test]
    fn multiple_recall_events_same_decision_union_item_ids() {
        // Two memory-enabled slots in the pipeline each emit a recall
        // event for the same decision; the resulting finding must list
        // the union of their item ids.
        let decisions = vec![decision("run-f", 5, Some(-3.0))];
        let recalls = vec![
            recall("run-f", 5, "agent:trader", &["a1", "a2"]),
            recall("run-f", 5, "global", &["g1", "a2"]), // a2 dup
        ];
        let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());
        assert_eq!(findings.len(), 1);
        let ids = findings[0]
            .evidence
            .get("memory_item_ids")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        // a1, a2, g1 — deduplicated.
        assert_eq!(ids.len(), 3);
    }

    #[test]
    fn empty_recall_event_skipped() {
        // Recorder emits empty recall events for timeline completeness.
        // The detector must not emit a finding when there's nothing to
        // attribute.
        let decisions = vec![decision("run-g", 1, Some(-1.0))];
        let recalls = vec![recall("run-g", 1, "agent:a", &[])];
        let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());
        assert!(findings.is_empty());
    }

    #[test]
    fn findings_emit_in_ascending_decision_order() {
        let decisions = vec![
            decision("run-h", 7, Some(-1.0)),
            decision("run-h", 2, Some(-2.0)),
            decision("run-h", 5, Some(-3.0)),
        ];
        let recalls = vec![
            recall("run-h", 7, "agent:a", &["m7"]),
            recall("run-h", 2, "agent:a", &["m2"]),
            recall("run-h", 5, "agent:a", &["m5"]),
        ];
        let findings = detect_memory_aware_findings(&decisions, &recalls, MemoryFindingOptions::default());
        assert_eq!(findings.len(), 3);
        // Summaries should mention decisions in ascending order: 2, 5, 7.
        assert!(findings[0].summary.contains("Decision 2"));
        assert!(findings[1].summary.contains("Decision 5"));
        assert!(findings[2].summary.contains("Decision 7"));
    }
}
