//! Rule-based eval-review auto-runner.
//!
//! Given a completed eval run, this module reads the run's
//! `eval_findings` rows and writes ONE `eval_reviews` row with a verdict
//! and score derived from finding severity counts. **No LLM call.** It is
//! the rule-based companion to the LLM-driven engine in
//! [`super::engine::run_review`], suitable for being fired from the
//! eval-finalize success path so every completed run gets a quick,
//! cheap, deterministic verdict without depending on a provider being
//! configured.
//!
//! Mapping rules (verdict):
//! * any `critical` finding → `failed`
//! * ≥ 2 `warning` findings and no `critical` → `weak`
//! * only `info` findings → `promising`
//! * no findings → `inconclusive`
//!
//! Score bands (0..=100):
//! * `failed`       → 0..=25  (decreasing in `critical_count`)
//! * `weak`         → 25..=50 (decreasing in `warning_count`)
//! * `promising`    → 75..=100 (decreasing in `info_count`)
//! * `inconclusive` → 50 (fixed midpoint)
//!
//! Idempotency: re-invoking `run_auto_review` on the same `eval_run_id`
//! is a no-op if a row already exists for the `AUTO_AGENT_PROFILE_ID`
//! pair (pre-existence guard). Documented choice: the contract calls
//! for either UPSERT or pre-existence guard; pre-existence guard is
//! simpler given `eval_reviews.id` is a ULID — there is no natural
//! UPSERT key to target without a schema change, which is out of
//! scope.
//!
//! Agent-profile resolution: the schema's
//! `eval_reviews.agent_profile_id` column is `NOT NULL` and FKs into
//! `agent_profiles(id)`. The rule-based runner is not tied to any
//! profile, but we still need a stable, always-present id to keep the
//! FK happy. We default to the seeded `fast-trader-agent` row
//! (migration 016) and document this as the "agent profile not
//! resolved at finalize" fallback. Callers that *do* have a profile
//! id at the seam can pass it in via
//! [`AutoReviewOptions::agent_profile_id`].

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::json;
use ulid::Ulid;

use crate::eval::findings::{uniformity::detect_uniformity, Finding, Severity};
use crate::eval::review::{EvalReview, ReviewAnnotation, ReviewStatus, ReviewVerdict};
use crate::eval::store::RunStore;

// ── Tunable constants ────────────────────────────────────────────────
// All score-mapping constants live in one place so operators can shift
// the bands without hunting through call sites. Keep the bands
// contiguous (failed.max == weak.min, weak.max < promising.min etc.)
// but note that the contract intentionally leaves a 50..=75 gap so the
// "promising" band starts clearly above "weak".

/// Stable fallback profile id used when the finalize seam has no
/// `agent_profile_id` to pass through. The seed row exists from
/// migration 016 onwards.
pub const AUTO_AGENT_PROFILE_ID: &str = "fast-trader-agent";

/// Score band ceiling for the `failed` verdict.
const FAILED_BAND_MAX: i32 = 25;
/// Per-critical penalty inside the `failed` band.
const FAILED_PER_CRITICAL: i32 = 5;
/// Cap on how many criticals count toward the penalty (avoids slamming
/// to 0 on a high-noise run).
const FAILED_CRITICAL_CAP: i32 = 5;

/// Score band ceiling for the `weak` verdict (also `failed.max` + 1's
/// floor when a future operator wants tighter contiguity).
const WEAK_BAND_MAX: i32 = 50;
const WEAK_BAND_MIN: i32 = 26;
/// Per-warning penalty inside the `weak` band.
const WEAK_PER_WARNING: i32 = 5;
/// Cap on warnings counted in the `weak` band.
const WEAK_WARNING_CAP: i32 = 5;

/// Score band ceiling for the `promising` verdict.
const PROMISING_BAND_MAX: i32 = 100;
const PROMISING_BAND_MIN: i32 = 75;
/// Per-info penalty inside the `promising` band — info findings are
/// not bad, but more of them slightly nudges the score down so a
/// silent run scores higher than an information-heavy one.
const PROMISING_PER_INFO: i32 = 2;
/// Cap on infos counted in the `promising` band.
const PROMISING_INFO_CAP: i32 = 12;

/// Fixed score for the `inconclusive` verdict.
const INCONCLUSIVE_SCORE: i32 = 50;

/// Maximum summary length (chars). The contract calls for ≤ 240.
const SUMMARY_MAX_CHARS: usize = 240;

/// Number of findings included in the headline summary line.
const SUMMARY_TOP_N: usize = 3;

// ── Public API ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Default)]
pub struct AutoReviewOptions {
    /// Explicit agent-profile id to attribute the auto-review to.
    /// `None` falls back to [`AUTO_AGENT_PROFILE_ID`].
    pub agent_profile_id: Option<String>,
}

/// Outcome of an auto-review attempt.
#[derive(Debug, Clone)]
pub enum AutoReviewOutcome {
    /// A new `eval_reviews` row was written.
    Inserted {
        review_id: String,
        verdict: ReviewVerdict,
        score: i32,
    },
    /// A row already existed for this `(run, agent_profile)` pair —
    /// idempotency guard skipped insertion.
    AlreadyExists { review_id: String },
}

/// Drive the rule-based review against a completed eval run. Reads
/// `eval_findings`, computes verdict + score, persists one
/// `eval_reviews` row. Safe to call multiple times for the same run —
/// the second invocation returns `AlreadyExists` without writing.
///
/// **Best-effort by design.** Callers wrap this in a `warn!` log and
/// never propagate failures up the finalize path (see [`fire_auto_review`]).
pub async fn run_auto_review(
    store: &RunStore,
    run_id: &str,
    options: AutoReviewOptions,
) -> Result<AutoReviewOutcome> {
    let agent_profile_id = options
        .agent_profile_id
        .unwrap_or_else(|| AUTO_AGENT_PROFILE_ID.to_string());

    // Idempotency: if any review for this run already exists with the
    // same agent_profile_id, no-op. Pre-existence guard, not UPSERT.
    let existing = store
        .list_reviews_for_run(run_id)
        .await
        .context("auto-review: list reviews for run")?;
    if let Some(prior) = existing.iter().find(|r| r.agent_profile_id == agent_profile_id) {
        return Ok(AutoReviewOutcome::AlreadyExists {
            review_id: prior.id.clone(),
        });
    }

    // ── Uniformity smell-tests ────────────────────────────────────────
    // Read decisions and run the pure uniformity detector *before* reading
    // the existing findings, so any new findings it emits are visible to
    // the verdict-mapping step below. This must run before classify_verdict.
    let decisions = store
        .read_decisions(run_id)
        .await
        .context("auto-review: read decisions for uniformity check")?;
    let uniformity_findings = detect_uniformity(&decisions);
    for uf in &uniformity_findings {
        if let Err(e) = store.record_finding(uf).await {
            tracing::warn!(
                error = %e,
                run_id,
                kind = %uf.kind,
                "auto-review: failed to persist uniformity finding (continuing)"
            );
        }
    }

    let findings = store
        .read_findings(run_id)
        .await
        .context("auto-review: read findings")?;

    let verdict = classify_verdict(&findings);
    let score = score_for(verdict, &findings);
    let summary = build_summary(&findings);
    let annotations = annotations_from_findings(&findings);
    let raw = serialize_findings_snapshot(&findings, verdict, score);

    // Build a "completed" review row directly. The auto-runner doesn't
    // need the queued → running → completed state machine the LLM
    // engine uses; the row lands in its terminal state.
    let now = Utc::now();
    let id = Ulid::new().to_string();
    let review = EvalReview {
        id: id.clone(),
        eval_run_id: run_id.to_string(),
        agent_profile_id,
        status: ReviewStatus::Completed,
        verdict: Some(verdict),
        confidence: None,
        score: Some(score),
        summary: Some(summary),
        raw_output_json: Some(raw),
        annotations,
        error: None,
        created_at: now,
        updated_at: now,
    };
    store
        .create_review(&review)
        .await
        .context("auto-review: create eval_reviews row")?;

    Ok(AutoReviewOutcome::Inserted {
        review_id: id,
        verdict,
        score,
    })
}

/// Best-effort wrapper used by the finalize seam in
/// `api::eval::run_inner` / the executor finalize-success path. Mirrors
/// the `findings postprocess failed (run still ok)` pattern: log
/// `warn!` on any error and swallow it; the run remains successful.
pub async fn fire_auto_review(store: &RunStore, run_id: &str) {
    match run_auto_review(store, run_id, AutoReviewOptions::default()).await {
        Ok(AutoReviewOutcome::Inserted {
            review_id,
            verdict,
            score,
        }) => {
            tracing::debug!(
                run_id,
                review_id,
                verdict = verdict.as_str(),
                score,
                "auto-review persisted"
            );
        }
        Ok(AutoReviewOutcome::AlreadyExists { review_id }) => {
            tracing::debug!(run_id, review_id, "auto-review skipped (pre-existing row)");
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                run_id,
                "auto-review postprocess failed (run still ok)"
            );
        }
    }
}

// ── Pure functions (unit-testable) ───────────────────────────────────

/// Verdict mapping. Pure function over the findings slice — no I/O.
pub fn classify_verdict(findings: &[Finding]) -> ReviewVerdict {
    if findings.is_empty() {
        return ReviewVerdict::Inconclusive;
    }
    let (criticals, warnings, infos) = severity_counts(findings);
    if criticals > 0 {
        ReviewVerdict::Failed
    } else if warnings >= 2 {
        ReviewVerdict::Weak
    } else if warnings == 1 {
        // Borderline: 1 warning, 0 criticals. The contract's 4
        // documented paths don't name this case explicitly; we treat
        // it as `weak` (conservative: any warning is a warning) so
        // a single concerning finding is never auto-promoted to
        // `promising`. The score-mapping in [`score_for`] still
        // rewards the low count: a 1-warning run lands at the top of
        // the `weak` band.
        ReviewVerdict::Weak
    } else if infos > 0 {
        // Only info findings → promising.
        ReviewVerdict::Promising
    } else {
        // Unreachable: empty findings already returned Inconclusive
        // above; the only way to reach this arm is if every finding
        // has an unknown severity, which Severity::parse rejects at
        // read time. Default to Inconclusive defensively.
        ReviewVerdict::Inconclusive
    }
}

/// Score mapping. Pure function over `(verdict, findings)`. Monotone
/// within each band: more findings of the band-driving severity → lower
/// score (down to the band floor).
pub fn score_for(verdict: ReviewVerdict, findings: &[Finding]) -> i32 {
    let (criticals, warnings, infos) = severity_counts(findings);
    match verdict {
        ReviewVerdict::Failed => {
            let penalty = criticals.min(FAILED_CRITICAL_CAP as usize) as i32 * FAILED_PER_CRITICAL;
            (FAILED_BAND_MAX - penalty).max(0)
        }
        ReviewVerdict::Weak => {
            let penalty = warnings.min(WEAK_WARNING_CAP as usize) as i32 * WEAK_PER_WARNING;
            (WEAK_BAND_MAX - penalty).max(WEAK_BAND_MIN)
        }
        ReviewVerdict::Promising => {
            let penalty = infos.min(PROMISING_INFO_CAP as usize) as i32 * PROMISING_PER_INFO;
            (PROMISING_BAND_MAX - penalty).max(PROMISING_BAND_MIN)
        }
        ReviewVerdict::Inconclusive => INCONCLUSIVE_SCORE,
    }
}

/// Build the top-N summary string. Pure function over the findings
/// slice. Ranks by severity (critical > warning > info), then by the
/// order findings appear in the input (which matches DB
/// `ORDER BY extracted_at ASC, id ASC` from `read_findings`).
pub fn build_summary(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "no findings".to_string();
    }
    let mut ranked: Vec<(usize, &Finding)> = findings.iter().enumerate().collect();
    ranked.sort_by(|(ai, a), (bi, b)| {
        severity_rank(b.severity)
            .cmp(&severity_rank(a.severity))
            .then(ai.cmp(bi))
    });
    let parts: Vec<String> = ranked
        .into_iter()
        .take(SUMMARY_TOP_N)
        .map(|(_, f)| {
            format!(
                "{}:{} - {}",
                f.severity.as_str(),
                f.kind,
                first_sentence(&f.summary)
            )
        })
        .collect();
    truncate_to(&parts.join("; "), SUMMARY_MAX_CHARS)
}

/// Convert the same finding snapshot that drives the deterministic
/// verdict into chart annotations. The rule-based runner does not have
/// cycle timestamps for every finding, so annotations are evenly spread
/// across the demo/review candle range unless the future producer
/// enriches them with timestamps.
pub fn annotations_from_findings(findings: &[Finding]) -> Vec<ReviewAnnotation> {
    findings
        .iter()
        .take(8)
        .enumerate()
        .map(|(i, finding)| {
            let title = finding
                .title
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or_else(|| first_sentence(&finding.summary));
            let body = finding
                .description
                .as_deref()
                .or(finding.recommendation.as_deref())
                .unwrap_or(&finding.summary);
            ReviewAnnotation {
                idx: ((i as u32) * 18 + 12).min(169),
                side: if i % 2 == 0 { "top" } else { "bottom" }.to_string(),
                kind: annotation_kind_for(finding),
                title: truncate_to(title.trim(), 64),
                body: truncate_to(body.trim(), 220),
                conf: finding
                    .confidence
                    .unwrap_or_else(|| confidence_for(finding.severity))
                    .clamp(0.0, 1.0),
                action: action_for(finding.severity).to_string(),
                danger: matches!(finding.severity, Severity::Critical | Severity::Warning),
                ts: None,
            }
        })
        .collect()
}

fn annotation_kind_for(finding: &Finding) -> String {
    let kind = finding
        .review_type
        .as_deref()
        .unwrap_or(&finding.kind)
        .to_ascii_lowercase();
    let mapped = if kind.contains("risk")
        || kind.contains("drawdown")
        || kind.contains("tail")
        || kind.contains("lookahead")
        || kind.contains("violation")
    {
        "RISK"
    } else if kind.contains("regime") || kind.contains("structure") {
        "STRUCTURE"
    } else if kind.contains("execution") || kind.contains("overtrad") || kind.contains("flow") {
        "FLOW"
    } else if kind.contains("performance") || kind.contains("underperformance") {
        "REVERSION"
    } else {
        "PATTERN"
    };
    mapped.to_string()
}

fn action_for(severity: Severity) -> &'static str {
    match severity {
        Severity::Critical => "CAUTION",
        Severity::Warning => "WATCH",
        Severity::Info => "WATCH",
    }
}

fn confidence_for(severity: Severity) -> f64 {
    match severity {
        Severity::Critical => 0.86,
        Severity::Warning => 0.72,
        Severity::Info => 0.58,
    }
}

fn severity_rank(s: Severity) -> u8 {
    match s {
        Severity::Critical => 2,
        Severity::Warning => 1,
        Severity::Info => 0,
    }
}

fn severity_counts(findings: &[Finding]) -> (usize, usize, usize) {
    let mut c = 0;
    let mut w = 0;
    let mut i = 0;
    for f in findings {
        match f.severity {
            Severity::Critical => c += 1,
            Severity::Warning => w += 1,
            Severity::Info => i += 1,
        }
    }
    (c, w, i)
}

fn first_sentence(s: &str) -> &str {
    let s = s.trim();
    if let Some(idx) = s.find('.') {
        s[..idx].trim()
    } else {
        s
    }
}

fn truncate_to(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    // Take `max - 1` chars and append the unicode ellipsis to make the
    // truncation visible without inflating past the limit.
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

fn serialize_findings_snapshot(findings: &[Finding], verdict: ReviewVerdict, score: i32) -> String {
    // Wrap the findings array in an envelope describing this is an
    // auto-runner artifact, not an LLM reply. The shape stays
    // forward-compatible: downstream readers that expect a JSON object
    // can pick `findings` out without confusing it for an LLM output.
    let snapshot = json!({
        "auto_runner": {
            "version": "v1",
            "verdict": verdict.as_str(),
            "score": score,
        },
        "findings": findings,
    });
    serde_json::to_string(&snapshot).unwrap_or_else(|_| "{}".to_string())
}

// ── Unit tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use serde_json::Value;

    fn mk(severity: Severity, kind: &str, summary: &str) -> Finding {
        Finding {
            id: Ulid::new().to_string(),
            run_id: "run-x".into(),
            kind: kind.into(),
            severity,
            summary: summary.into(),
            evidence: Value::Null,
            extracted_at: Utc.with_ymd_and_hms(2026, 5, 19, 0, 0, 0).unwrap(),
            schema_version: "v1".into(),
            evidence_cycle_ids: None,
            produced_by_check: None,
            eval_review_id: None,
            review_type: None,
            confidence: None,
            title: None,
            description: None,
            recommendation: None,
            created_at: None,
        }
    }

    // Verdict mapping table — covers the four documented paths plus
    // the borderline "one warning, no critical" tiebreaker.
    #[test]
    fn verdict_mapping_table() {
        // (critical_count, warning_count, info_count) → expected verdict
        let cases: &[(usize, usize, usize, ReviewVerdict)] = &[
            (0, 0, 0, ReviewVerdict::Inconclusive),
            (1, 0, 0, ReviewVerdict::Failed),
            (1, 5, 3, ReviewVerdict::Failed),
            (0, 2, 0, ReviewVerdict::Weak),
            (0, 3, 1, ReviewVerdict::Weak),
            (0, 1, 0, ReviewVerdict::Weak),
            (0, 1, 3, ReviewVerdict::Weak),
            (0, 0, 1, ReviewVerdict::Promising),
            (0, 0, 4, ReviewVerdict::Promising),
        ];
        for (c, w, i, expected) in cases {
            let mut findings = Vec::new();
            for _ in 0..*c {
                findings.push(mk(Severity::Critical, "k", "s"));
            }
            for _ in 0..*w {
                findings.push(mk(Severity::Warning, "k", "s"));
            }
            for _ in 0..*i {
                findings.push(mk(Severity::Info, "k", "s"));
            }
            let got = classify_verdict(&findings);
            assert_eq!(
                got, *expected,
                "verdict mismatch for (c={c}, w={w}, i={i}): got {got:?}, expected {expected:?}"
            );
        }
    }

    #[test]
    fn score_is_monotone_within_failed_band() {
        // More critical findings → lower or equal score, never higher.
        let mut prev = i32::MAX;
        for n in 1..=8 {
            let findings: Vec<Finding> = (0..n).map(|_| mk(Severity::Critical, "k", "s")).collect();
            let v = classify_verdict(&findings);
            let s = score_for(v, &findings);
            assert!(s <= prev, "score not monotone at n={n}: {prev} → {s}");
            assert!((0..=FAILED_BAND_MAX).contains(&s), "score {s} out of failed band");
            prev = s;
        }
    }

    #[test]
    fn score_is_monotone_within_weak_band() {
        let mut prev = i32::MAX;
        for n in 2..=10 {
            let findings: Vec<Finding> = (0..n).map(|_| mk(Severity::Warning, "k", "s")).collect();
            let v = classify_verdict(&findings);
            assert_eq!(v, ReviewVerdict::Weak);
            let s = score_for(v, &findings);
            assert!(s <= prev, "score not monotone at n={n}: {prev} → {s}");
            assert!(
                (WEAK_BAND_MIN..=WEAK_BAND_MAX).contains(&s),
                "score {s} out of weak band"
            );
            prev = s;
        }
    }

    #[test]
    fn score_is_monotone_within_promising_band() {
        let mut prev = i32::MAX;
        for n in 1..=20 {
            let findings: Vec<Finding> = (0..n).map(|_| mk(Severity::Info, "k", "s")).collect();
            let v = classify_verdict(&findings);
            assert_eq!(v, ReviewVerdict::Promising);
            let s = score_for(v, &findings);
            assert!(s <= prev, "score not monotone at n={n}: {prev} → {s}");
            assert!(
                (PROMISING_BAND_MIN..=PROMISING_BAND_MAX).contains(&s),
                "score {s} out of promising band"
            );
            prev = s;
        }
    }

    #[test]
    fn score_inconclusive_is_fixed_midpoint() {
        let s = score_for(ReviewVerdict::Inconclusive, &[]);
        assert_eq!(s, INCONCLUSIVE_SCORE);
        assert_eq!(s, 50);
    }

    #[test]
    fn summary_ranks_critical_then_warning_then_info() {
        let findings = vec![
            mk(Severity::Info, "info_a", "Info A finding. Trailing."),
            mk(Severity::Warning, "warn_b", "Warning B noted."),
            mk(Severity::Critical, "crit_c", "Critical C blew up."),
            mk(Severity::Warning, "warn_d", "Warning D noted."),
        ];
        let s = build_summary(&findings);
        // Critical comes first.
        let crit_idx = s.find("critical:crit_c").expect("critical in summary");
        let warn_idx = s.find("warning:warn_b").expect("warning b in summary");
        let info_idx = s.find("info:info_a");
        assert!(crit_idx < warn_idx, "critical should precede warning");
        // Top-3: critical, warn_b, warn_d — info should NOT appear.
        assert!(info_idx.is_none(), "info should not appear in top-3 summary: {s}");
    }

    #[test]
    fn summary_truncates_to_240_chars() {
        let long = "a".repeat(500);
        let findings = vec![mk(Severity::Warning, "long_kind", &long)];
        let s = build_summary(&findings);
        assert!(
            s.chars().count() <= SUMMARY_MAX_CHARS,
            "summary {} chars exceeds max {}",
            s.chars().count(),
            SUMMARY_MAX_CHARS
        );
    }

    #[test]
    fn summary_no_findings() {
        let s = build_summary(&[]);
        assert_eq!(s, "no findings");
    }

    #[test]
    fn snapshot_envelope_contains_findings_and_metadata() {
        let findings = vec![mk(Severity::Warning, "k", "s")];
        let raw = serialize_findings_snapshot(&findings, ReviewVerdict::Weak, 45);
        let v: Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["auto_runner"]["version"], "v1");
        assert_eq!(v["auto_runner"]["verdict"], "weak");
        assert_eq!(v["auto_runner"]["score"], 45);
        assert!(v["findings"].is_array());
        assert_eq!(v["findings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn first_sentence_picks_to_period() {
        assert_eq!(first_sentence("Hello world. Trailing."), "Hello world");
        assert_eq!(first_sentence("no period here"), "no period here");
        assert_eq!(first_sentence("  spaced. yes"), "spaced");
    }
}
