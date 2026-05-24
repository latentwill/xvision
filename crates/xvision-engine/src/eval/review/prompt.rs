//! Render the system prompt + strict-JSON contract for the review model.
//!
//! Two layers:
//!
//! * `system_prompt`: the persona prompt persisted on the `AgentProfile`
//!   row, verbatim. Operator edits flow through here without code changes.
//! * `contract`: the response-shape contract appended to the system
//!   prompt — enumerates required keys, valid verdicts, and the explicit
//!   "no inventing" rule.
//!
//! Keeping these separate makes it easy to audit what the contract layer
//! is actually demanding (it's a single string in this file) without
//! grepping across operator-edited profile rows.

use super::AgentProfile;

/// JSON response contract appended to every review prompt. Single source
/// of truth for required output keys and the no-hallucination rule.
const RESPONSE_CONTRACT: &str = r#"
You must respond with exactly one JSON object. No markdown fences, no prose
before or after, no comments inside the JSON.

The JSON object MUST have these exact keys:
{
  "summary": string,
  "verdict": "promising" | "weak" | "failed" | "inconclusive",
  "confidence": number in [0.0, 1.0],
  "score": integer in [0, 100],
  "findings": array of finding objects,
  "annotations": array of annotation objects,
  "risks": array of strings,
  "next_tests": array of strings,
  "questions": array of strings
}

Each finding object MUST have:
{
  "type": "performance" | "risk" | "regime" | "behavior" | "execution" | "data_quality" | "anomaly" | "opportunity",
  "severity": "low" | "medium" | "high" | "critical",
  "confidence": number in [0.0, 1.0],
  "title": string,
  "description": string,
  "evidence": array of { "kind": string, "reference": string } objects,
  "recommendation": string
}

Each annotation object is optional chart markup for `/charts/annotated` and MUST have:
{
  "idx": integer candle/decision index,
  "side": "top" | "bottom",
  "type": "PATTERN" | "FLOW" | "RISK" | "REVERSION" | "STRUCTURE",
  "title": string,
  "body": string,
  "conf": number in [0.0, 1.0],
  "action": "WATCH" | "LONG" | "SHORT" | "CAUTION",
  "danger": boolean
}

Counts:
- `findings`: 3..=10 for completed runs. If the payload is too sparse to
  support 3 grounded findings, return verdict "inconclusive" and an empty
  findings array.
- `annotations`: 0..=8. Use only moments grounded in the payload.
- `risks`: 1..=5.
- `next_tests`: 3..=7.

Evidence rules (HARD):
- Each finding `evidence[].reference` MUST be one of the strings listed in
  the "Valid evidence references" section of the user payload. If you
  cannot ground a finding in those references, drop it. Do not invent
  references, trade ids, market events, log lines, or order fills that
  are not in the payload.
- The payload contains ONLY what is present in `metrics`, `equity_curve`,
  `decisions`, `events`, and `errors`. If something is missing (e.g.
  orders, positions, market regime data, intraday bars), say so in the
  summary or mark the review inconclusive — do not fabricate it.

Determinism rules:
- Do not include timestamps you do not have. Refer to specific decisions
  by `decision:<index>` (where the index matches the payload's
  `decisions[].decision_index`).
- Refer to metrics by `metric:<key>` matching keys present in
  `metrics`.
- Refer to the equity curve by `equity:<index>` for a single sample,
  `equity_range:0..N` for a window over the whole curve, or
  `time_range:<start>..<end>` only when both endpoints come from
  decisions or equity samples in the payload.
"#;

/// Compose the operator persona prompt with the response contract. The
/// contract is the last thing the model sees so it takes precedence over
/// any persona free-text that drifts toward freeform narrative.
pub fn build_system_prompt(profile: &AgentProfile) -> String {
    let mut s = String::with_capacity(profile.system_prompt.len() + RESPONSE_CONTRACT.len() + 64);
    s.push_str(profile.system_prompt.trim());
    s.push_str("\n\n");
    s.push_str(RESPONSE_CONTRACT.trim());
    s
}

/// User-message header that summarizes which evidence-reference strings
/// are legal for this payload. Appended to the serialized payload before
/// it is handed to the model, so the model can see the exact allowlist
/// the parser will enforce.
pub fn render_evidence_legend(refs: &std::collections::BTreeSet<String>) -> String {
    let mut s = String::from("Valid evidence references for this payload:\n");
    if refs.is_empty() {
        s.push_str("  (none — payload is sparse; return verdict \"inconclusive\" with no findings)\n");
        return s;
    }
    for r in refs {
        s.push_str("  - ");
        s.push_str(r);
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use std::collections::BTreeSet;

    fn profile_with(system_prompt: &str) -> AgentProfile {
        AgentProfile {
            id: "reasoning-agent".into(),
            name: "Reasoning".into(),
            profile_type: "reasoning".into(),
            provider: "anthropic".into(),
            model: "claude-sonnet-4-6".into(),
            temperature: 0.2,
            max_tokens: 8000,
            system_prompt: system_prompt.into(),
            enabled: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn system_prompt_appends_contract_to_persona() {
        let profile = profile_with("You are a careful analyst.");
        let out = build_system_prompt(&profile);
        assert!(out.starts_with("You are a careful analyst."));
        assert!(out.contains("\"verdict\": \"promising\" | \"weak\" | \"failed\" | \"inconclusive\""));
        assert!(out.contains("HARD"));
    }

    #[test]
    fn legend_lists_each_allowlisted_reference() {
        let mut refs = BTreeSet::new();
        refs.insert("metric:sharpe".to_string());
        refs.insert("decision:0".to_string());
        let out = render_evidence_legend(&refs);
        assert!(out.contains("metric:sharpe"));
        assert!(out.contains("decision:0"));
    }

    #[test]
    fn legend_calls_out_sparse_payload() {
        let refs = BTreeSet::new();
        let out = render_evidence_legend(&refs);
        assert!(out.contains("sparse"));
        assert!(out.contains("inconclusive"));
    }
}
