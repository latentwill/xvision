//! Uniformity smell-tests for eval decision sets.
//!
//! Detects signals that the LLM is not producing real output — stub
//! providers, model-collapse failures, or parser defaults that swallow
//! the response and return a fixed fallback.
//!
//! # Background
//!
//! In QA run `01KS4D0MZBD5VGEQ9ACJDRBFBG` (xvnej-app, 2026-05-21),
//! 217/217 decisions returned the literal justification
//! `"stub Gemini Flash 3.1 response"` with `action="long_open"` and
//! `conviction=0.42`. The eval pipeline shipped `status=completed` with
//! `sharpe=-7.84` derived entirely from that mock. The auto-reviewer
//! emitted `verdict: inconclusive, score: 50, findings: []`. Nothing
//! flagged it.
//!
//! A simple uniformity check on the `eval_decisions` table would have
//! caught this in milliseconds.
//!
//! # Checks
//!
//! 1. **Identical justification** (`kind = "uniform_justification"`):
//!    if `unique_justifications.len() == 1` AND `n_decisions >= 10` →
//!    `severity: critical`. Fires first; check 2 is skipped when this
//!    fires.
//!
//! 2. **Identical action + conviction** (`kind = "uniform_decision"`):
//!    if `unique_(action, conviction)_pairs.len() == 1` AND
//!    `n_decisions >= 10` AND check 1 did NOT fire → `severity: critical`.
//!
//! 3. **Near-uniform justification** (`kind = "near_uniform_justification"`):
//!    if the most-common justification covers ≥ 90% of decisions AND
//!    `n_decisions >= 20` AND check 1 did NOT fire → `severity: warning`.
//!
//! # Usage
//!
//! ```rust,ignore
//! let decisions = store.read_decisions(&run_id).await?;
//! let extra_findings = detect_uniformity(&decisions);
//! for f in extra_findings {
//!     store.record_finding(&f).await?;
//! }
//! ```
//!
//! [`detect_uniformity`] is a pure function — no DB, no I/O.

use std::collections::HashMap;

use chrono::Utc;
use ulid::Ulid;

use crate::eval::findings::{Finding, Severity, FINDING_SCHEMA_VERSION};
use crate::eval::store::DecisionRow;

/// Threshold: minimum number of decisions before any check fires.
const MIN_DECISIONS_FOR_IDENTICAL: usize = 10;
/// Threshold: minimum number of decisions before the near-uniform check fires.
const MIN_DECISIONS_FOR_NEAR_UNIFORM: usize = 20;
/// Fraction of decisions that must share the same justification to trigger
/// the near-uniform warning.
const NEAR_UNIFORM_THRESHOLD: f64 = 0.90;
/// Maximum length of the justification sample in the finding body.
const SAMPLE_MAX_CHARS: usize = 200;

/// `produced_by_check` value for uniformity findings.
pub const PRODUCED_BY_UNIFORMITY: &str = "smell:uniformity";

/// Analyse a slice of `DecisionRow`s for uniformity signals. Pure — no I/O.
///
/// Returns a `Vec<Finding>` containing zero, one, or two findings depending on
/// which checks fire. At most one `critical` finding is emitted for the
/// identical-justification / identical-decision pair (check 1 suppresses
/// check 2); the near-uniform warning is independent of check 2 but
/// suppressed by check 1.
pub fn detect_uniformity(decisions: &[DecisionRow]) -> Vec<Finding> {
    let n = decisions.len();
    if n == 0 {
        return vec![];
    }

    // ── Check 1: Identical justification ─────────────────────────────────

    let justifications: Vec<&str> = decisions
        .iter()
        .filter_map(|d| d.justification.as_deref())
        .collect();

    // Only consider runs where every decision has a non-empty justification.
    let has_all_justifications = justifications.len() == n;
    let unique_justifications: std::collections::HashSet<&str> = justifications.iter().copied().collect();

    let check1_fired =
        has_all_justifications && unique_justifications.len() == 1 && n >= MIN_DECISIONS_FOR_IDENTICAL;

    let mut findings = Vec::new();

    if check1_fired {
        let sample = justifications
            .first()
            .map(|s| truncate_str(s, SAMPLE_MAX_CHARS))
            .unwrap_or_default();
        // run_id is the same across all rows in a valid slice.
        let run_id = decisions[0].run_id.clone();
        findings.push(make_finding(
            &run_id,
            Severity::Critical,
            "uniform_justification",
            &format!("all {n} decisions returned identical justification text"),
            &format!(
                "Sample: {sample}. This is overwhelming evidence the model is not producing \
                 real output (stub provider, model collapse, or a parser default). The eval \
                 metrics from this run should not be trusted."
            ),
        ));
        // Check 2 is redundant when check 1 fires.
        // Check 3 is also suppressed when check 1 fires.
        return findings;
    }

    // ── Check 2: Identical action + conviction ────────────────────────────
    // Only runs when check 1 did NOT fire.

    if n >= MIN_DECISIONS_FOR_IDENTICAL {
        let unique_pairs: std::collections::HashSet<(String, String)> = decisions
            .iter()
            .map(|d| {
                let action = d.action.clone();
                // Normalise conviction to a canonical string so f64 NaN/None
                // doesn't cause spurious uniqueness.
                let conviction = conviction_key(d.conviction);
                (action, conviction)
            })
            .collect();

        if unique_pairs.len() == 1 {
            let run_id = decisions[0].run_id.clone();
            let (action, conv) = unique_pairs.into_iter().next().unwrap();
            findings.push(make_finding(
                &run_id,
                Severity::Critical,
                "uniform_decision",
                &format!("all {n} decisions returned action={action} conviction={conv}"),
                &format!(
                    "Every decision in this run produced the same (action, conviction) pair \
                     ({action}, {conv}). This strongly suggests a stub provider, model-collapse \
                     failure, or a parser default that swallowed the real response. The eval \
                     metrics from this run should not be trusted."
                ),
            ));
        }
    }

    // ── Check 3: Near-uniform justification ──────────────────────────────
    // Only runs when check 1 did NOT fire, regardless of check 2.

    if has_all_justifications && n >= MIN_DECISIONS_FOR_NEAR_UNIFORM {
        let mut freq: HashMap<&str, usize> = HashMap::new();
        for j in &justifications {
            *freq.entry(j).or_insert(0) += 1;
        }
        if let Some((&most_common, &count)) = freq.iter().max_by_key(|(_, &c)| c) {
            let pct = (count as f64 / n as f64) * 100.0;
            if pct >= NEAR_UNIFORM_THRESHOLD * 100.0 {
                let run_id = decisions[0].run_id.clone();
                let pct_int = pct.round() as u64;
                let sample = truncate_str(most_common, SAMPLE_MAX_CHARS);
                findings.push(make_finding(
                    &run_id,
                    Severity::Warning,
                    "near_uniform_justification",
                    &format!("{pct_int}% of decisions returned the same justification text"),
                    &format!(
                        "{count} of {n} decisions share the same justification. \
                         Sample: {sample}. This may indicate a partial-stub scenario or \
                         model-collapse with noise. Review provider logs and consider \
                         re-running with a known-good provider before trusting these metrics."
                    ),
                ));
            }
        }
    }

    findings
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn conviction_key(c: Option<f64>) -> String {
    match c {
        None => "none".to_string(),
        Some(v) if v.is_nan() => "nan".to_string(),
        Some(v) => format!("{v:.4}"),
    }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        out.push('…');
        out
    }
}

fn make_finding(run_id: &str, severity: Severity, kind: &str, title: &str, body: &str) -> Finding {
    let now = Utc::now();
    Finding {
        id: Ulid::new().to_string(),
        run_id: run_id.to_string(),
        kind: kind.to_string(),
        severity,
        summary: title.to_string(),
        evidence: serde_json::json!({
            "produced_by_check": PRODUCED_BY_UNIFORMITY,
        }),
        extracted_at: now,
        schema_version: FINDING_SCHEMA_VERSION.to_string(),
        evidence_cycle_ids: Some(vec![]),
        produced_by_check: Some(PRODUCED_BY_UNIFORMITY.to_string()),
        eval_review_id: None,
        review_type: None,
        confidence: None,
        title: Some(title.to_string()),
        description: Some(body.to_string()),
        recommendation: Some(
            "Inspect the provider logs for this run. If the provider is a tunnel or local \
             fixture (e.g. Serveo), verify it is returning real model output and not a stub. \
             Re-run after confirming the provider is healthy."
                .to_string(),
        ),
        created_at: Some(now),
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn decision(
        run_id: &str,
        action: &str,
        conviction: Option<f64>,
        justification: Option<&str>,
    ) -> DecisionRow {
        DecisionRow {
            run_id: run_id.to_string(),
            decision_index: 0,
            timestamp: Utc::now(),
            asset: "BTC/USDT".to_string(),
            action: action.to_string(),
            conviction,
            justification: justification.map(str::to_string),
            reasoning: None,
            order_size: None,
            fill_price: None,
            fill_size: None,
            fee: None,
            pnl_realized: None,
            delayed: None,
        }
    }

    fn identical_decisions(run_id: &str, n: usize) -> Vec<DecisionRow> {
        (0..n)
            .map(|_| {
                decision(
                    run_id,
                    "long_open",
                    Some(0.42),
                    Some("stub Gemini Flash 3.1 response"),
                )
            })
            .collect()
    }

    // ── No findings in edge cases ─────────────────────────────────────────

    #[test]
    fn empty_slice_produces_no_findings() {
        let findings = detect_uniformity(&[]);
        assert!(findings.is_empty(), "expected no findings for empty slice");
    }

    #[test]
    fn five_identical_decisions_under_threshold_no_finding() {
        // n=5 < MIN_DECISIONS_FOR_IDENTICAL=10 → no finding.
        let decisions = identical_decisions("run-a", 5);
        let findings = detect_uniformity(&decisions);
        assert!(
            findings.is_empty(),
            "expected no finding below threshold, got {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }

    // ── Check 1: Identical justification ─────────────────────────────────

    #[test]
    fn twelve_identical_decisions_emits_critical_uniform_justification() {
        let decisions = identical_decisions("run-b", 12);
        let findings = detect_uniformity(&decisions);
        assert_eq!(findings.len(), 1, "expected exactly 1 finding");
        let f = &findings[0];
        assert_eq!(f.kind, "uniform_justification");
        assert_eq!(f.severity, Severity::Critical);
        assert!(
            f.summary.contains("12"),
            "summary should mention decision count: {}",
            f.summary
        );
        assert_eq!(f.run_id, "run-b");
        assert_eq!(f.produced_by_check.as_deref(), Some(PRODUCED_BY_UNIFORMITY));
    }

    #[test]
    fn check1_suppresses_check2_when_both_would_fire() {
        // Identical justification AND identical (action, conviction) → only
        // check 1 fires (check 2 would be redundant).
        let decisions = identical_decisions("run-c", 15);
        let findings = detect_uniformity(&decisions);
        // Must be exactly 1 finding with kind=uniform_justification.
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "uniform_justification");
    }

    // ── Check 2: Identical action + conviction ────────────────────────────

    #[test]
    fn twelve_same_action_conviction_varied_justification_emits_critical_uniform_decision() {
        // Same (action, conviction) pair but varied justifications → check 1
        // does NOT fire (unique_justifications.len() > 1); check 2 fires.
        let decisions: Vec<DecisionRow> = (0..12)
            .map(|i| decision("run-d", "long_open", Some(0.42), Some(&format!("reason {i}"))))
            .collect();
        // Ensure uniqueness of justifications (guaranteed by index above).
        let findings = detect_uniformity(&decisions);
        assert_eq!(
            findings.len(),
            1,
            "expected exactly 1 finding, got: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
        let f = &findings[0];
        assert_eq!(f.kind, "uniform_decision");
        assert_eq!(f.severity, Severity::Critical);
        assert!(f.summary.contains("long_open"), "summary: {}", f.summary);
    }

    // ── Check 3: Near-uniform justification ──────────────────────────────

    #[test]
    fn twenty_five_decisions_where_23_share_justification_emits_warning() {
        // 23/25 = 92% > 90% threshold. n=25 >= 20.
        let run_id = "run-e";
        let mut decisions: Vec<DecisionRow> = (0..23)
            .map(|_| decision(run_id, "long_open", Some(0.3), Some("stub response")))
            .collect();
        // Two unique justifications to prevent check 1 from firing.
        decisions.push(decision(run_id, "flat", Some(0.1), Some("unique reason A")));
        decisions.push(decision(run_id, "flat", Some(0.1), Some("unique reason B")));

        let findings = detect_uniformity(&decisions);
        assert_eq!(
            findings.len(),
            1,
            "expected 1 near-uniform warning, got: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
        let f = &findings[0];
        assert_eq!(f.kind, "near_uniform_justification");
        assert_eq!(f.severity, Severity::Warning);
        assert!(
            f.summary.contains("92") || f.summary.contains("23"),
            "summary: {}",
            f.summary
        );
    }

    #[test]
    fn near_uniform_suppressed_when_check1_fires() {
        // 100% identical → check 1 fires and returns early; check 3 is not reached.
        let decisions = identical_decisions("run-f", 25);
        let findings = detect_uniformity(&decisions);
        // Only one finding — check1. No near_uniform_justification alongside it.
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, "uniform_justification");
    }

    // ── Fully varied decisions: no findings ──────────────────────────────

    #[test]
    fn fully_varied_decisions_produce_no_findings() {
        let run_id = "run-g";
        let decisions: Vec<DecisionRow> = (0..20)
            .map(|i| {
                let action = if i % 3 == 0 {
                    "long_open"
                } else if i % 3 == 1 {
                    "flat"
                } else {
                    "short_open"
                };
                let conviction = Some(0.1 + (i as f64) * 0.04);
                let justification = Some(format!("unique analysis for bar {i}"));
                decision(run_id, action, conviction, justification.as_deref())
            })
            .collect();

        let findings = detect_uniformity(&decisions);
        assert!(
            findings.is_empty(),
            "expected no findings for varied decisions, got: {:?}",
            findings.iter().map(|f| &f.kind).collect::<Vec<_>>()
        );
    }
}
