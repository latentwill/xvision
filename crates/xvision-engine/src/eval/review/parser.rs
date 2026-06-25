//! Strict-JSON parser for review-agent responses.
//!
//! Why this lives in its own module: parse failures, missing keys,
//! out-of-range numbers, and ungrounded evidence references all collapse
//! to the same caller-visible outcome — the review becomes
//! `inconclusive`. Centralising that logic keeps the engine orchestrator
//! short and prevents accidental "partial parse" code paths from leaking
//! invented findings through.
//!
//! The parser DOES NOT call `anyhow::Result` / `?` for validation
//! failures — those return `ReviewParseError` so the engine can decide
//! whether to mark the review failed (transport-level problem) or
//! inconclusive (the model produced something the contract rejects).

use serde::Deserialize;
use thiserror::Error;

use super::payload::ReviewPayload;
use super::ReviewAnnotation;

/// Allowed finding `type` values. Matches the spec contract; the parser
/// enforces them so a typo'd type fails the review rather than silently
/// persisting as junk.
const ALLOWED_FINDING_TYPES: &[&str] = &[
    "performance",
    "risk",
    "regime",
    "behavior",
    "execution",
    "data_quality",
    "anomaly",
    "opportunity",
];

const ALLOWED_SEVERITIES: &[&str] = &["low", "medium", "high", "critical"];
const ALLOWED_ANNOTATION_SIDES: &[&str] = &["top", "bottom"];
const ALLOWED_ANNOTATION_TYPES: &[&str] = &["PATTERN", "FLOW", "RISK", "REVERSION", "STRUCTURE"];
const ALLOWED_ANNOTATION_ACTIONS: &[&str] = &["WATCH", "LONG", "SHORT", "CAUTION"];

#[derive(Debug, Error)]
pub enum ReviewParseError {
    #[error("review response is not valid JSON: {0}")]
    InvalidJson(String),
    #[error("review response missing required field `{0}`")]
    MissingField(&'static str),
    #[error("review response field `{field}` failed validation: {reason}")]
    InvalidField { field: String, reason: String },
    #[error(
        "review finding {index} references unknown evidence `{reference}`; only references in \
         the payload's evidence allowlist are accepted"
    )]
    UngroundedEvidence { index: usize, reference: String },
    #[error(
        "review must include 3..=10 findings for completed runs (got {count}); mark verdict \
         `inconclusive` with zero findings instead"
    )]
    FindingsCountInvalid { count: usize },
    #[error("review must include 1..=5 risks (got {0})")]
    RisksCountInvalid(usize),
    #[error("review must include 3..=7 next_tests (got {0})")]
    NextTestsCountInvalid(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParsedReview {
    pub summary: String,
    pub verdict: String,
    pub confidence: f64,
    pub score: i32,
    pub findings: Vec<ReviewFinding>,
    pub annotations: Vec<ReviewAnnotation>,
    pub risks: Vec<String>,
    pub next_tests: Vec<String>,
    pub questions: Vec<String>,
    /// The exact JSON text that parsed cleanly (whitespace + slicing
    /// applied, but no field rewrites). Persisted on
    /// `eval_reviews.raw_output_json` for audit.
    pub raw_json: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewFinding {
    pub finding_type: String,
    pub severity: String,
    pub confidence: f64,
    pub title: String,
    pub description: String,
    pub evidence: Vec<EvidenceRef>,
    pub recommendation: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceRef {
    pub kind: String,
    pub reference: String,
}

#[derive(Deserialize)]
struct RawReview {
    summary: Option<String>,
    verdict: Option<String>,
    confidence: Option<f64>,
    score: Option<i64>,
    findings: Option<Vec<RawFinding>>,
    risks: Option<Vec<String>>,
    next_tests: Option<Vec<String>>,
    questions: Option<Vec<String>>,
    annotations: Option<Vec<RawAnnotation>>,
}

#[derive(Deserialize)]
struct RawFinding {
    #[serde(rename = "type")]
    finding_type: Option<String>,
    severity: Option<String>,
    confidence: Option<f64>,
    title: Option<String>,
    description: Option<String>,
    evidence: Option<Vec<RawEvidence>>,
    recommendation: Option<String>,
}

#[derive(Deserialize)]
struct RawEvidence {
    kind: Option<String>,
    reference: Option<String>,
}

#[derive(Deserialize)]
struct RawAnnotation {
    idx: Option<u32>,
    side: Option<String>,
    #[serde(rename = "type")]
    kind: Option<String>,
    title: Option<String>,
    body: Option<String>,
    conf: Option<f64>,
    action: Option<String>,
    danger: Option<bool>,
}

/// Parse + validate. The caller decides what to do on error:
///
/// * `Ok` → persist as completed.
/// * `Err(UngroundedEvidence | FindingsCountInvalid | InvalidField | MissingField)`
///   → the engine converts to an `inconclusive` review with the error in
///   `summary` and zero findings.
/// * `Err(InvalidJson)` → engine may retry once, then fail the review.
pub fn parse_review_output(text: &str, payload: &ReviewPayload) -> Result<ParsedReview, ReviewParseError> {
    let slice = slice_json_object(text)
        .ok_or_else(|| ReviewParseError::InvalidJson("no `{` … `}` block found in response".into()))?;
    let raw: RawReview = serde_json::from_str(slice)
        .map_err(|e| ReviewParseError::InvalidJson(format!("{e} (slice: {slice:?})")))?;

    let summary = raw.summary.ok_or(ReviewParseError::MissingField("summary"))?;
    let verdict_str = raw.verdict.ok_or(ReviewParseError::MissingField("verdict"))?;
    let verdict = validate_verdict(&verdict_str)?;
    let confidence = raw
        .confidence
        .ok_or(ReviewParseError::MissingField("confidence"))?;
    validate_unit_interval("confidence", confidence)?;
    let score_i64 = raw.score.ok_or(ReviewParseError::MissingField("score"))?;
    let score = validate_score(score_i64)?;
    let findings_raw = raw.findings.ok_or(ReviewParseError::MissingField("findings"))?;
    let risks = raw.risks.ok_or(ReviewParseError::MissingField("risks"))?;
    let next_tests = raw
        .next_tests
        .ok_or(ReviewParseError::MissingField("next_tests"))?;
    let questions = raw.questions.ok_or(ReviewParseError::MissingField("questions"))?;
    let annotations_raw = raw.annotations.unwrap_or_default();

    // Inconclusive verdicts may have zero findings; everything else must
    // hit the 3..=10 band. The engine layer maps a parse failure to a
    // synthesised inconclusive review separately.
    let is_inconclusive = verdict == "inconclusive";
    if !is_inconclusive && !(3..=10).contains(&findings_raw.len()) {
        return Err(ReviewParseError::FindingsCountInvalid {
            count: findings_raw.len(),
        });
    }

    if !is_inconclusive {
        if !(1..=5).contains(&risks.len()) {
            return Err(ReviewParseError::RisksCountInvalid(risks.len()));
        }
        if !(3..=7).contains(&next_tests.len()) {
            return Err(ReviewParseError::NextTestsCountInvalid(next_tests.len()));
        }
    }

    let mut findings = Vec::with_capacity(findings_raw.len());
    for (i, raw) in findings_raw.into_iter().enumerate() {
        findings.push(parse_finding(i, raw, payload)?);
    }
    let mut annotations = Vec::with_capacity(annotations_raw.len().min(8));
    for (i, raw) in annotations_raw.into_iter().take(8).enumerate() {
        annotations.push(parse_annotation(i, raw)?);
    }

    Ok(ParsedReview {
        summary,
        verdict,
        confidence,
        score,
        findings,
        annotations,
        risks,
        next_tests,
        questions,
        raw_json: slice.to_string(),
    })
}

fn parse_annotation(index: usize, raw: RawAnnotation) -> Result<ReviewAnnotation, ReviewParseError> {
    let idx = raw
        .idx
        .ok_or(ReviewParseError::MissingField("annotations[].idx"))?;
    let side = raw
        .side
        .ok_or(ReviewParseError::MissingField("annotations[].side"))?;
    if !ALLOWED_ANNOTATION_SIDES.contains(&side.as_str()) {
        return Err(ReviewParseError::InvalidField {
            field: format!("annotations[{index}].side"),
            reason: format!("`{side}` not in {ALLOWED_ANNOTATION_SIDES:?}"),
        });
    }
    let kind = raw
        .kind
        .ok_or(ReviewParseError::MissingField("annotations[].type"))?;
    if !ALLOWED_ANNOTATION_TYPES.contains(&kind.as_str()) {
        return Err(ReviewParseError::InvalidField {
            field: format!("annotations[{index}].type"),
            reason: format!("`{kind}` not in {ALLOWED_ANNOTATION_TYPES:?}"),
        });
    }
    let title = raw
        .title
        .ok_or(ReviewParseError::MissingField("annotations[].title"))?;
    let body = raw
        .body
        .ok_or(ReviewParseError::MissingField("annotations[].body"))?;
    let raw_conf = raw
        .conf
        .ok_or(ReviewParseError::MissingField("annotations[].conf"))?;
    if raw_conf.is_nan() {
        return Err(ReviewParseError::InvalidField {
            field: format!("annotations[{index}].conf"),
            reason: "expected number in [0.0, 1.0]".into(),
        });
    }
    let conf = raw_conf.clamp(0.0, 1.0);
    let action = raw
        .action
        .ok_or(ReviewParseError::MissingField("annotations[].action"))?;
    if !ALLOWED_ANNOTATION_ACTIONS.contains(&action.as_str()) {
        return Err(ReviewParseError::InvalidField {
            field: format!("annotations[{index}].action"),
            reason: format!("`{action}` not in {ALLOWED_ANNOTATION_ACTIONS:?}"),
        });
    }

    Ok(ReviewAnnotation {
        idx,
        side,
        kind,
        title,
        body,
        conf,
        action,
        danger: raw.danger.unwrap_or(false),
        ts: None,
    })
}

fn parse_finding(
    index: usize,
    raw: RawFinding,
    payload: &ReviewPayload,
) -> Result<ReviewFinding, ReviewParseError> {
    let finding_type = raw
        .finding_type
        .ok_or(ReviewParseError::MissingField("findings[].type"))?;
    if !ALLOWED_FINDING_TYPES.contains(&finding_type.as_str()) {
        return Err(ReviewParseError::InvalidField {
            field: format!("findings[{index}].type"),
            reason: format!("`{finding_type}` not in {ALLOWED_FINDING_TYPES:?}"),
        });
    }
    let severity = raw
        .severity
        .ok_or(ReviewParseError::MissingField("findings[].severity"))?;
    if !ALLOWED_SEVERITIES.contains(&severity.as_str()) {
        return Err(ReviewParseError::InvalidField {
            field: format!("findings[{index}].severity"),
            reason: format!("`{severity}` not in {ALLOWED_SEVERITIES:?}"),
        });
    }
    let confidence = raw
        .confidence
        .ok_or(ReviewParseError::MissingField("findings[].confidence"))?;
    validate_unit_interval(&format!("findings[{index}].confidence"), confidence)?;
    let title = raw
        .title
        .ok_or(ReviewParseError::MissingField("findings[].title"))?;
    let description = raw
        .description
        .ok_or(ReviewParseError::MissingField("findings[].description"))?;
    let evidence_raw = raw
        .evidence
        .ok_or(ReviewParseError::MissingField("findings[].evidence"))?;
    let recommendation = raw
        .recommendation
        .ok_or(ReviewParseError::MissingField("findings[].recommendation"))?;

    let mut evidence = Vec::with_capacity(evidence_raw.len());
    for e in evidence_raw {
        let kind = e
            .kind
            .ok_or(ReviewParseError::MissingField("findings[].evidence[].kind"))?;
        let reference = e
            .reference
            .ok_or(ReviewParseError::MissingField("findings[].evidence[].reference"))?;
        if !payload.valid_evidence_refs.contains(&reference) {
            return Err(ReviewParseError::UngroundedEvidence { index, reference });
        }
        evidence.push(EvidenceRef { kind, reference });
    }

    Ok(ReviewFinding {
        finding_type,
        severity,
        confidence,
        title,
        description,
        evidence,
        recommendation,
    })
}

fn validate_verdict(s: &str) -> Result<String, ReviewParseError> {
    match s {
        "promising" | "weak" | "failed" | "inconclusive" => Ok(s.to_string()),
        _ => Err(ReviewParseError::InvalidField {
            field: "verdict".into(),
            reason: format!("`{s}` not in [promising, weak, failed, inconclusive]"),
        }),
    }
}

fn validate_unit_interval(field: &str, v: f64) -> Result<(), ReviewParseError> {
    if v.is_nan() || !(0.0..=1.0).contains(&v) {
        return Err(ReviewParseError::InvalidField {
            field: field.into(),
            reason: format!("expected number in [0.0, 1.0], got {v}"),
        });
    }
    Ok(())
}

fn validate_score(v: i64) -> Result<i32, ReviewParseError> {
    if !(0..=100).contains(&v) {
        return Err(ReviewParseError::InvalidField {
            field: "score".into(),
            reason: format!("expected integer in [0, 100], got {v}"),
        });
    }
    Ok(v as i32)
}

/// Find the first `{` and matching closing `}` so we can tolerate a small
/// amount of pre/post-prose. Brace counting respects strings so a `{` or
/// `}` inside a JSON string doesn't throw off the depth.
fn slice_json_object(text: &str) -> Option<&str> {
    let bytes = text.as_bytes();
    let start = bytes.iter().position(|&b| b == b'{')?;
    let mut depth = 0_i32;
    let mut in_string = false;
    let mut escape = false;
    for (i, &b) in bytes[start..].iter().enumerate() {
        if in_string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                in_string = false;
            }
            continue;
        }
        match b {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::review::payload::build_review_payload;
    use crate::eval::review::AgentProfile;
    use crate::eval::run::{MetricsSummary, Run, RunMode, RunStatus};
    use crate::eval::store::DecisionRow;
    use chrono::{TimeZone, Utc};

    fn fixture_profile() -> AgentProfile {
        AgentProfile {
            id: "reasoning-agent".into(),
            name: "Reasoning".into(),
            profile_type: "reasoning".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            temperature: 0.2,
            max_tokens: 8000,
            system_prompt: "be careful".into(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    fn fixture_payload() -> ReviewPayload {
        let mut run = Run::new_queued("agent-1".into(), "sc-1".into(), RunMode::Backtest);
        run.status = RunStatus::Completed;
        run.metrics = Some(MetricsSummary {
            total_return_pct: 5.0,
            sharpe: 1.2,
            max_drawdown_pct: -3.0,
            win_rate: 0.55,
            n_trades: 4,
            n_decisions: 10,
            baselines: None,
            ..Default::default()
        });
        let t0 = Utc.with_ymd_and_hms(2026, 5, 1, 12, 0, 0).unwrap();
        let decisions: Vec<DecisionRow> = (0..3)
            .map(|i| DecisionRow {
                run_id: "run-1".into(),
                decision_index: i,
                timestamp: t0,
                asset: "BTC-USD".into(),
                action: "long_open".into(),
                conviction: Some(0.7),
                justification: None,
                reasoning: None,
                order_size: Some(0.01),
                fill_price: Some(50_000.0),
                fill_size: Some(0.01),
                fee: Some(1.0),
                pnl_realized: Some(0.0),
                delayed: None,
            })
            .collect();
        let equity = vec![(t0, 100_000.0), (t0, 100_500.0), (t0, 99_000.0)];
        build_review_payload(&run, decisions, equity, None, &fixture_profile())
    }

    fn finding_obj(reference: &str) -> serde_json::Value {
        serde_json::json!({
            "type": "performance",
            "severity": "medium",
            "confidence": 0.6,
            "title": "Modest sharpe",
            "description": "Sharpe is 1.2, modest given the 5% return.",
            "evidence": [{"kind": "metric", "reference": reference}],
            "recommendation": "Test on a longer window."
        })
    }

    fn well_formed_response(reference: &str) -> String {
        serde_json::json!({
            "summary": "Strategy looks plausible.",
            "verdict": "promising",
            "confidence": 0.7,
            "score": 70,
            "findings": [finding_obj(reference), finding_obj(reference), finding_obj(reference)],
            "risks": ["concentration"],
            "next_tests": ["longer backtest", "stress test", "out-of-sample"],
            "questions": ["does it survive 2022 chop?"],
        })
        .to_string()
    }

    #[test]
    fn parses_well_formed_response() {
        let payload = fixture_payload();
        let response = well_formed_response("metric:sharpe");
        let parsed = parse_review_output(&response, &payload).expect("well-formed parses");
        assert_eq!(parsed.verdict, "promising");
        assert_eq!(parsed.findings.len(), 3);
        assert_eq!(parsed.score, 70);
    }

    #[test]
    fn rejects_evidence_not_in_payload() {
        let payload = fixture_payload();
        let response = well_formed_response("metric:invented");
        let err = parse_review_output(&response, &payload).unwrap_err();
        assert!(
            matches!(err, ReviewParseError::UngroundedEvidence { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn rejects_unknown_verdict() {
        let payload = fixture_payload();
        let mut r: serde_json::Value = serde_json::from_str(&well_formed_response("metric:sharpe")).unwrap();
        r["verdict"] = serde_json::json!("amazing");
        let err = parse_review_output(&r.to_string(), &payload).unwrap_err();
        assert!(
            matches!(err, ReviewParseError::InvalidField { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn rejects_score_out_of_range() {
        let payload = fixture_payload();
        let mut r: serde_json::Value = serde_json::from_str(&well_formed_response("metric:sharpe")).unwrap();
        r["score"] = serde_json::json!(150);
        let err = parse_review_output(&r.to_string(), &payload).unwrap_err();
        assert!(
            matches!(err, ReviewParseError::InvalidField { .. }),
            "got: {err:?}"
        );
    }

    #[test]
    fn rejects_findings_count_outside_3_to_10_for_non_inconclusive() {
        let payload = fixture_payload();
        let mut r: serde_json::Value = serde_json::from_str(&well_formed_response("metric:sharpe")).unwrap();
        r["findings"] = serde_json::json!([finding_obj("metric:sharpe")]);
        let err = parse_review_output(&r.to_string(), &payload).unwrap_err();
        assert!(matches!(err, ReviewParseError::FindingsCountInvalid { count: 1 }));
    }

    #[test]
    fn allows_empty_findings_when_inconclusive() {
        let payload = fixture_payload();
        let mut r: serde_json::Value = serde_json::from_str(&well_formed_response("metric:sharpe")).unwrap();
        r["verdict"] = serde_json::json!("inconclusive");
        r["findings"] = serde_json::json!([]);
        r["risks"] = serde_json::json!([]);
        r["next_tests"] = serde_json::json!([]);
        let parsed = parse_review_output(&r.to_string(), &payload).expect("inconclusive parses");
        assert_eq!(parsed.findings.len(), 0);
        assert_eq!(parsed.verdict, "inconclusive");
    }

    #[test]
    fn tolerates_pre_and_post_prose() {
        let payload = fixture_payload();
        let body = well_formed_response("metric:sharpe");
        let wrapped = format!("Here is the review:\n\n{body}\n\nThanks!");
        let parsed = parse_review_output(&wrapped, &payload).expect("sliced JSON parses");
        assert_eq!(parsed.verdict, "promising");
    }

    #[test]
    fn missing_required_field_is_an_error() {
        let payload = fixture_payload();
        let mut r: serde_json::Value = serde_json::from_str(&well_formed_response("metric:sharpe")).unwrap();
        r.as_object_mut().unwrap().remove("summary");
        let err = parse_review_output(&r.to_string(), &payload).unwrap_err();
        assert!(matches!(err, ReviewParseError::MissingField("summary")));
    }

    #[test]
    fn slice_json_object_handles_braces_in_strings() {
        let s = r#"prelude {"summary": "weird { brace", "x": 1} postlude"#;
        let slice = slice_json_object(s).expect("should slice");
        assert_eq!(slice, r#"{"summary": "weird { brace", "x": 1}"#);
    }
}
