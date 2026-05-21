//! Per-run guardrail rewrite summary — emitted at run finalize.
//!
//! ## Why this module exists
//!
//! Each per-decision guardrail rewrite emits a `supervisor_notes` row (the
//! durable audit record) and previously also emitted a per-decision
//! `tracing::warn!` line. In a QA rerun on `xvnej-app` (2026-05-21),
//! 432 of 432 trader actions over 24 h were rewritten, producing 432 WARN
//! lines that buried the actual pattern signal.
//!
//! This module collapses that noise: the per-decision logs are demoted to
//! `tracing::debug!` in both executors; at finalize, this module reads the
//! `supervisor_notes` rows whose `role='guard'`, aggregates them into counts
//! by `(reason, original_action, applied_action)`, and:
//!
//! 1. Emits **one** `tracing::warn!` per run (only when at least one block
//!    occurred) with a summary line any operator can grep.
//! 2. Writes **one** `eval_findings` row via [`crate::eval::store::RunStore`]
//!    with severity determined by the rewrite-rate fraction.
//!
//! ## Severity rules
//!
//! | Fraction of decisions rewritten | Severity |
//! |---------------------------------|----------|
//! | ≥ 50 %                          | critical — strategy fights guardrail every bar |
//! | ≥ 10 % and < 50 %               | warning  |
//! | > 0 % and < 10 %                | info     |
//! | 0 %                             | no finding emitted |
//!
//! ## Finding shape
//!
//! - `kind = "guardrail_rewrite_rate"`
//! - `produced_by_check = "guard:rewrite_rate"`
//! - `title = "guardrail rewrote {n}/{total} trader actions ({pct}%)"`
//! - `description` — per-reason counts + most-common `(original, applied)` pair
//! - `evidence` — structured JSON blob (counts, top pair, total_decisions)

use std::collections::HashMap;

use anyhow::Result;
use chrono::Utc;
use ulid::Ulid;

use crate::eval::findings::{Finding, Severity, FINDING_SCHEMA_VERSION};
use crate::eval::store::RunStore;

/// The `kind` value stored in `eval_findings` for guardrail-rewrite-rate
/// findings.
pub const KIND_GUARDRAIL_REWRITE_RATE: &str = "guardrail_rewrite_rate";

/// `produced_by_check` value for this finding kind.
pub const PRODUCED_BY_GUARD: &str = "guard:rewrite_rate";

// ── Public data types ─────────────────────────────────────────────────────────

/// Aggregated counts from parsing guardrail `supervisor_notes` rows.
#[derive(Debug, Clone, PartialEq)]
pub struct GuardrailSummaryResult {
    /// Total guardrail-rewrite events found in supervisor_notes.
    pub rewrite_count: usize,
    /// Total trader decisions (denominator for the rate).
    pub total_decisions: usize,
    /// Counts by `(reason, original_action, applied_action)`.
    pub counts_by_reason: HashMap<(String, String, String), usize>,
    /// The most-common `(original_action, applied_action)` pair across all
    /// reasons.
    pub top_pair: (String, String),
    /// The severity level determined by the rewrite rate.
    pub severity: Severity,
}

impl GuardrailSummaryResult {
    /// Rewrite rate as a percentage, rounded to the nearest integer.
    pub fn pct(&self) -> u64 {
        if self.total_decisions == 0 {
            return 0;
        }
        ((self.rewrite_count as f64 / self.total_decisions as f64) * 100.0).round() as u64
    }
}

// ── Pure summarisation logic ──────────────────────────────────────────────────

/// Parse a guardrail supervisor-note content string into its fields.
///
/// The format (from `guardrails::supervisor_note_content`) is:
/// ```text
/// <reason>: original=<action> applied=<action> asset=<asset> decision_index=<i>
/// ```
///
/// Returns `Some((reason, original, applied))` on success, `None` if the
/// content does not match the expected shape.
fn parse_guard_note(content: &str) -> Option<(String, String, String)> {
    // Split on the first ": " to get the reason prefix.
    let (reason, rest) = content.split_once(": ")?;
    // Extract original= and applied= fields by splitting on " " and looking
    // for "key=value" tokens. This is resilient to field ordering.
    let mut original = None;
    let mut applied = None;
    for token in rest.split_whitespace() {
        if let Some(v) = token.strip_prefix("original=") {
            original = Some(v.to_string());
        } else if let Some(v) = token.strip_prefix("applied=") {
            applied = Some(v.to_string());
        }
    }
    Some((reason.to_string(), original?, applied?))
}

/// Pure summarisation function. Takes the raw `supervisor_notes` rows for a run
/// (as `(role, severity, content)` triples — the shape returned by
/// `RunStore::read_supervisor_notes`) and the total number of trader decisions,
/// and returns `None` when no guardrail blocks occurred, or `Some(result)` with
/// aggregated counts and chosen severity.
///
/// This function is `pub` so it can be unit-tested without DB access.
pub fn summarise_notes(
    notes: &[(String, String, String)],
    total_decisions: usize,
) -> Option<GuardrailSummaryResult> {
    // Collect only guard-role notes. Severity field in the notes table is the
    // per-note severity at write time ("warn"); we ignore it here — the summary
    // severity is recomputed from the rewrite rate.
    let guard_notes: Vec<&str> = notes
        .iter()
        .filter(|(role, _, _)| role == "guard")
        .map(|(_, _, content)| content.as_str())
        .collect();

    let rewrite_count = guard_notes.len();
    if rewrite_count == 0 {
        return None;
    }

    // Aggregate counts by (reason, original, applied).
    let mut counts_by_reason: HashMap<(String, String, String), usize> = HashMap::new();
    let mut pair_counts: HashMap<(String, String), usize> = HashMap::new();

    for content in &guard_notes {
        if let Some((reason, original, applied)) = parse_guard_note(content) {
            *counts_by_reason
                .entry((reason, original.clone(), applied.clone()))
                .or_insert(0) += 1;
            *pair_counts.entry((original, applied)).or_insert(0) += 1;
        }
    }

    // Most-common (original, applied) pair. Fall back to a placeholder when
    // no notes were parseable (shouldn't happen in practice).
    let top_pair = pair_counts
        .into_iter()
        .max_by_key(|(_, count)| *count)
        .map(|(pair, _)| pair)
        .unwrap_or_else(|| ("unknown".to_string(), "unknown".to_string()));

    // Severity from rewrite rate.
    let severity = if total_decisions == 0 {
        Severity::Info
    } else {
        let ratio = rewrite_count as f64 / total_decisions as f64;
        if ratio >= 0.5 {
            Severity::Critical
        } else if ratio >= 0.1 {
            Severity::Warning
        } else {
            Severity::Info
        }
    };

    Some(GuardrailSummaryResult {
        rewrite_count,
        total_decisions,
        counts_by_reason,
        top_pair,
        severity,
    })
}

// ── Finding constructor ───────────────────────────────────────────────────────

/// Build a `guardrail_rewrite_rate` finding from a summary result.
pub fn make_guardrail_summary_finding(run_id: &str, result: &GuardrailSummaryResult) -> Finding {
    let pct = result.pct();
    let n = result.rewrite_count;
    let total = result.total_decisions;
    let title = format!("guardrail rewrote {n}/{total} trader actions ({pct}%)");

    // Per-reason breakdown for the description.
    let mut reason_lines: Vec<String> = result
        .counts_by_reason
        .iter()
        .map(|((reason, orig, app), count)| {
            format!("{reason}: original={orig} applied={app} count={count}")
        })
        .collect();
    reason_lines.sort(); // deterministic order for tests

    let (top_orig, top_app) = &result.top_pair;
    let description = format!(
        "Guardrail rewrote {n} of {total} trader decisions ({pct}%). \
         Most common rewrite: original={top_orig} applied={top_app}. \
         Per-reason counts: {}",
        reason_lines.join("; "),
    );

    // Evidence blob — structured JSON.
    let mut evidence_counts = serde_json::Map::new();
    for ((reason, orig, app), count) in &result.counts_by_reason {
        let key = format!("{reason}(orig={orig},app={app})");
        evidence_counts.insert(key, serde_json::json!(count));
    }

    let evidence = serde_json::json!({
        "rewrite_count": n,
        "total_decisions": total,
        "rewrite_pct": pct,
        "top_pair": { "original": top_orig, "applied": top_app },
        "counts_by_reason": evidence_counts,
    });

    let now = Utc::now();
    Finding {
        id: Ulid::new().to_string(),
        run_id: run_id.to_string(),
        kind: KIND_GUARDRAIL_REWRITE_RATE.to_string(),
        severity: result.severity,
        summary: title.clone(),
        evidence,
        extracted_at: now,
        schema_version: FINDING_SCHEMA_VERSION.to_string(),
        evidence_cycle_ids: Some(vec![]),
        produced_by_check: Some(PRODUCED_BY_GUARD.to_string()),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: Some(title),
        description: Some(description),
        recommendation: Some(
            "Review whether the strategy's model is emitting an action that violates \
             the guardrail on every bar. Consider adjusting the strategy's prompt or \
             risk profile so it does not fight the guardrail."
                .to_string(),
        ),
        created_at: Some(now),
    }
}

// ── Async finalize hook ───────────────────────────────────────────────────────

/// Best-effort guardrail summary hook. Called from the eval-finalize success
/// path (alongside `fire_auto_review`). Reads guard-role supervisor notes,
/// aggregates them, emits one `tracing::warn!` and one `eval_findings` row.
///
/// Failures are logged with `tracing::warn!` and swallowed — same pattern as
/// `fire_auto_review`. The run is never failed because of this hook.
pub async fn fire_guardrail_summary(store: &RunStore, run_id: &str) {
    // Read supervisor notes and decisions in parallel.
    let notes_res = store.read_supervisor_notes(run_id).await;
    let decisions_res = store.read_decisions(run_id).await;

    let notes = match notes_res {
        Ok(n) => n,
        Err(e) => {
            tracing::warn!(
                run_id,
                error = %e,
                "guardrail summary: failed to read supervisor_notes (run still ok)"
            );
            return;
        }
    };
    let decisions = match decisions_res {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                run_id,
                error = %e,
                "guardrail summary: failed to read eval_decisions (run still ok)"
            );
            return;
        }
    };

    let total_decisions = decisions.len();

    let result = match summarise_notes(&notes, total_decisions) {
        Some(r) => r,
        None => return, // no guardrail blocks — nothing to do
    };

    // Emit the one-per-run summary warn.
    let pct = result.pct();
    let n = result.rewrite_count;

    // Build a compact per-reason string for the log line.
    let mut by_reason: Vec<String> = result
        .counts_by_reason
        .iter()
        .map(|((reason, orig, app), count)| {
            format!("{reason}(orig={orig},app={app})={count}")
        })
        .collect();
    by_reason.sort();

    let distinct_originals: Vec<String> = {
        let mut v: Vec<String> = result
            .counts_by_reason
            .keys()
            .map(|(_, orig, _)| orig.clone())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        v.sort();
        v
    };

    tracing::warn!(
        run_id,
        rewrite_count = n,
        total_decisions,
        rewrite_pct = pct,
        by_reason = by_reason.join(" "),
        distinct_originals = format!("[{}]", distinct_originals.join(",")),
        "eval guardrail summary",
    );

    // Persist the finding.
    let finding = make_guardrail_summary_finding(run_id, &result);
    if let Err(e) = store.record_finding(&finding).await {
        tracing::warn!(
            run_id,
            error = %e,
            "guardrail summary: failed to persist finding (run still ok)"
        );
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn note(content: &str) -> (String, String, String) {
        ("guard".to_string(), "warn".to_string(), content.to_string())
    }

    fn non_guard_note(content: &str) -> (String, String, String) {
        ("system".to_string(), "info".to_string(), content.to_string())
    }

    fn pyramid_note(idx: u32) -> (String, String, String) {
        note(&format!(
            "pyramid blocked: original=long_open applied=hold asset=BTC/USD decision_index={idx}"
        ))
    }

    fn flip_note(idx: u32) -> (String, String, String) {
        note(&format!(
            "one-step flip blocked: original=short_open applied=flat asset=BTC/USD decision_index={idx}"
        ))
    }

    // ── summarise_notes ──────────────────────────────────────────────────────

    #[test]
    fn no_notes_returns_none() {
        let result = summarise_notes(&[], 100);
        assert!(result.is_none(), "0 notes → no finding");
    }

    #[test]
    fn non_guard_notes_only_returns_none() {
        let notes = vec![
            non_guard_note("some system note"),
            non_guard_note("another system note"),
        ];
        let result = summarise_notes(&notes, 50);
        assert!(
            result.is_none(),
            "only non-guard notes → no finding (0 guard rewrites)"
        );
    }

    #[test]
    fn one_note_in_100_decisions_is_info() {
        // 1 / 100 = 1% → info
        let notes = vec![pyramid_note(5)];
        let result = summarise_notes(&notes, 100).expect("should produce a result");
        assert_eq!(result.rewrite_count, 1);
        assert_eq!(result.total_decisions, 100);
        assert_eq!(result.severity, Severity::Info);
        assert_eq!(result.pct(), 1);
        assert_eq!(result.top_pair, ("long_open".to_string(), "hold".to_string()));
    }

    #[test]
    fn fifteen_in_100_decisions_is_warning() {
        // 15 / 100 = 15% → warning
        let notes: Vec<_> = (0..15).map(pyramid_note).collect();
        let result = summarise_notes(&notes, 100).expect("should produce a result");
        assert_eq!(result.rewrite_count, 15);
        assert_eq!(result.severity, Severity::Warning);
        assert_eq!(result.pct(), 15);
    }

    #[test]
    fn sixty_in_100_decisions_is_critical() {
        // 60 / 100 = 60% → critical
        let notes: Vec<_> = (0..60).map(pyramid_note).collect();
        let result = summarise_notes(&notes, 100).expect("should produce a result");
        assert_eq!(result.rewrite_count, 60);
        assert_eq!(result.severity, Severity::Critical);
        assert_eq!(result.pct(), 60);
    }

    #[test]
    fn exactly_50_pct_is_critical() {
        // 50 / 100 = 50% → critical (boundary)
        let notes: Vec<_> = (0..50).map(pyramid_note).collect();
        let result = summarise_notes(&notes, 100).expect("50% boundary");
        assert_eq!(result.severity, Severity::Critical);
    }

    #[test]
    fn exactly_10_pct_is_warning() {
        // 10 / 100 = 10% → warning (boundary)
        let notes: Vec<_> = (0..10).map(pyramid_note).collect();
        let result = summarise_notes(&notes, 100).expect("10% boundary");
        assert_eq!(result.severity, Severity::Warning);
    }

    #[test]
    fn mixed_reasons_aggregated_correctly() {
        // 3 pyramid + 2 flip = 5 total; top pair is long_open→hold (3 vs 2).
        let mut notes: Vec<_> = (0..3).map(pyramid_note).collect();
        notes.extend((0..2).map(flip_note));
        let result = summarise_notes(&notes, 50).expect("mixed reasons");
        assert_eq!(result.rewrite_count, 5);
        assert_eq!(result.pct(), 10); // 5/50 = 10%
        assert_eq!(result.severity, Severity::Warning);
        // Top pair is long_open→hold (3 occurrences vs 2 for short_open→flat).
        assert_eq!(result.top_pair.0, "long_open");
        assert_eq!(result.top_pair.1, "hold");
        // Two distinct reason+pair keys.
        assert_eq!(result.counts_by_reason.len(), 2);
    }

    #[test]
    fn zero_total_decisions_with_notes_is_info() {
        // Edge case: notes present but 0 total decisions (shouldn't happen
        // in practice but must not panic or divide-by-zero).
        let notes = vec![pyramid_note(0)];
        let result = summarise_notes(&notes, 0).expect("result even with 0 total");
        assert_eq!(result.severity, Severity::Info);
        assert_eq!(result.pct(), 0);
    }

    // ── parse_guard_note ─────────────────────────────────────────────────────

    #[test]
    fn parse_pyramid_note_content() {
        let content = "pyramid blocked: original=long_open applied=hold asset=BTC/USD decision_index=3";
        let (reason, orig, app) = parse_guard_note(content).expect("parseable");
        assert_eq!(reason, "pyramid blocked");
        assert_eq!(orig, "long_open");
        assert_eq!(app, "hold");
    }

    #[test]
    fn parse_flip_note_content() {
        let content =
            "one-step flip blocked: original=short_open applied=flat asset=ETH/USD decision_index=7";
        let (reason, orig, app) = parse_guard_note(content).expect("parseable");
        assert_eq!(reason, "one-step flip blocked");
        assert_eq!(orig, "short_open");
        assert_eq!(app, "flat");
    }

    #[test]
    fn parse_malformed_returns_none() {
        assert!(parse_guard_note("not a guardrail note").is_none());
        assert!(parse_guard_note("prefix: no fields here").is_none());
    }

    // ── make_guardrail_summary_finding ───────────────────────────────────────

    #[test]
    fn finding_title_matches_spec() {
        let notes: Vec<_> = (0..3).map(pyramid_note).collect();
        let result = summarise_notes(&notes, 10).unwrap();
        let finding = make_guardrail_summary_finding("run-xyz", &result);
        let title = finding.title.as_deref().unwrap_or("");
        assert_eq!(title, "guardrail rewrote 3/10 trader actions (30%)");
    }

    #[test]
    fn finding_kind_and_produced_by_are_set() {
        let notes = vec![pyramid_note(0)];
        let result = summarise_notes(&notes, 5).unwrap();
        let finding = make_guardrail_summary_finding("run-abc", &result);
        assert_eq!(finding.kind, KIND_GUARDRAIL_REWRITE_RATE);
        assert_eq!(
            finding.produced_by_check.as_deref(),
            Some(PRODUCED_BY_GUARD)
        );
    }

    #[test]
    fn finding_description_contains_top_pair_and_counts() {
        let mut notes: Vec<_> = (0..5).map(pyramid_note).collect();
        notes.extend((0..2).map(flip_note));
        let result = summarise_notes(&notes, 20).unwrap();
        let finding = make_guardrail_summary_finding("run-def", &result);
        let desc = finding.description.as_deref().unwrap_or("");
        assert!(
            desc.contains("original=long_open applied=hold"),
            "description must reference top pair; got: {desc}"
        );
        assert!(
            desc.contains("count=5"),
            "description must include pyramid count=5; got: {desc}"
        );
    }
}
