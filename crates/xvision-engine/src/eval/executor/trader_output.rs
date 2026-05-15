use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::Value;

use crate::agent::llm::LlmResponse;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraderOutput {
    pub(crate) action: String,
    pub(crate) conviction: f64,
    pub(crate) justification: String,
}

impl TraderOutput {
    pub(crate) fn parse_response(response: &LlmResponse, run_id: &str, decision_index: u32) -> Result<Self> {
        let raw = response.text();
        let metadata = format!(
            "stop_reason={:?}, input_tokens={}, output_tokens={}",
            response.stop_reason, response.input_tokens, response.output_tokens
        );

        if raw.trim().is_empty() {
            anyhow::bail!(
                "run {} decision {}: trader output is empty: provider returned no final text ({})",
                run_id,
                decision_index,
                metadata
            );
        }

        Self::parse_with_metadata(&raw, run_id, decision_index, Some(metadata.as_str()))
    }

    #[cfg(test)]
    pub(crate) fn parse_strict(raw: &str, run_id: &str, decision_index: u32) -> Result<Self> {
        Self::parse_with_metadata(raw, run_id, decision_index, None)
    }

    fn parse_with_metadata(
        raw: &str,
        run_id: &str,
        decision_index: u32,
        metadata: Option<&str>,
    ) -> Result<Self> {
        let mut first_error: Option<String> = None;
        for candidate in trader_output_candidates(raw) {
            match serde_json::from_str::<Self>(&candidate) {
                Ok(parsed) => {
                    parsed.validate(run_id, decision_index)?;
                    return Ok(parsed);
                }
                Err(e) => {
                    if first_error.is_none() {
                        first_error = Some(trader_output_error_detail(&e));
                    }
                }
            }
        }

        let mut detail = first_error.unwrap_or_else(|| "no JSON object found".into());
        if let Some(metadata) = metadata {
            detail = format!("{detail} ({metadata})");
        }
        Err(anyhow!(
            "run {} decision {}: trader output is invalid JSON: {}",
            run_id,
            decision_index,
            detail
        ))
    }

    fn validate(&self, run_id: &str, decision_index: u32) -> Result<()> {
        if !matches!(self.action.as_str(), "long_open" | "short_open" | "flat" | "hold") {
            anyhow::bail!(
                "run {} decision {}: trader output action must be one of long_open, short_open, flat, hold (got `{}`)",
                run_id,
                decision_index,
                self.action
            );
        }
        if !(0.0..=1.0).contains(&self.conviction) {
            anyhow::bail!(
                "run {} decision {}: trader output conviction must be between 0 and 1 (got {})",
                run_id,
                decision_index,
                self.conviction
            );
        }
        if self.justification.trim().is_empty() {
            anyhow::bail!(
                "run {} decision {}: trader output justification is required",
                run_id,
                decision_index
            );
        }
        Ok(())
    }
}

fn trader_output_candidates(raw: &str) -> Vec<String> {
    let mut out = Vec::new();
    push_candidate(&mut out, raw.trim());

    if let Some(stripped) = strip_code_fence(raw.trim()) {
        push_candidate(&mut out, stripped.trim());
    }
    if let Some(extracted) = extract_first_json_object(raw) {
        push_candidate(&mut out, &extracted);
    }

    let mut i = 0;
    while i < out.len() {
        let candidate = out[i].clone();
        if let Ok(value) = serde_json::from_str::<Value>(&candidate) {
            append_wrapped_candidates(&mut out, &value);
        }
        i += 1;
    }
    out
}

fn append_wrapped_candidates(out: &mut Vec<String>, value: &Value) {
    let Some(obj) = value.as_object() else {
        return;
    };

    for key in ["output", "text", "content", "response"] {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            push_candidate(out, s.trim());
            if let Some(stripped) = strip_code_fence(s.trim()) {
                push_candidate(out, stripped.trim());
            }
            if let Some(extracted) = extract_first_json_object(s) {
                push_candidate(out, &extracted);
            }
        }
    }

    for key in ["decision", "trader_output", "arguments"] {
        if let Some(v) = obj.get(key).filter(|v| v.is_object()) {
            push_candidate(out, &v.to_string());
        }
    }
}

fn push_candidate(out: &mut Vec<String>, candidate: &str) {
    if candidate.is_empty() {
        return;
    }
    if !out.iter().any(|seen| seen == candidate) {
        out.push(candidate.to_string());
    }
}

fn strip_code_fence(raw: &str) -> Option<&str> {
    let raw = raw.trim();
    let rest = raw.strip_prefix("```")?;
    let rest = rest
        .strip_prefix("json")
        .or_else(|| rest.strip_prefix("JSON"))
        .unwrap_or(rest)
        .trim_start_matches(['\r', '\n']);
    let end = rest.rfind("```")?;
    Some(&rest[..end])
}

fn extract_first_json_object(raw: &str) -> Option<String> {
    for (start, ch) in raw.char_indices() {
        if ch != '{' {
            continue;
        }
        let mut depth = 0i32;
        let mut in_string = false;
        let mut escaped = false;
        for (offset, c) in raw[start..].char_indices() {
            if in_string {
                if escaped {
                    escaped = false;
                } else if c == '\\' {
                    escaped = true;
                } else if c == '"' {
                    in_string = false;
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '{' => depth += 1,
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(raw[start..start + offset + c.len_utf8()].to_string());
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn trader_output_error_detail(error: &serde_json::Error) -> String {
    let message = error.to_string();
    if message.contains("missing field `action`") || message.contains("missing field action") {
        format!(
            "{message}; missing required trader field `action` (expected one of long_open, short_open, flat, hold)"
        )
    } else {
        message
    }
}

#[cfg(test)]
mod tests {
    use crate::agent::llm::{LlmResponse, StopReason};

    use super::TraderOutput;

    #[test]
    fn missing_action_has_field_level_diagnostic() {
        for run_id in ["01KRK9Y45K1MKS9FTH4TY4SK47", "01KRKATKTK331A08TQ2MBN6FYC"] {
            let err = TraderOutput::parse_strict(
                r#"{"conviction":0.7,"justification":"trend continuation"}"#,
                run_id,
                0,
            )
            .expect_err("missing action must fail");
            let message = err.to_string();

            assert!(message.contains(run_id));
            assert!(message.contains("decision 0"));
            assert!(message.contains("missing required trader field `action`"));
        }
    }

    #[test]
    fn invalid_action_has_field_level_diagnostic() {
        let err = TraderOutput::parse_strict(
            r#"{"action":"buy","conviction":0.7,"justification":"trend continuation"}"#,
            "01TEST",
            3,
        )
        .expect_err("invalid action must fail");

        assert!(err
            .to_string()
            .contains("action must be one of long_open, short_open, flat, hold"));
    }

    #[test]
    fn empty_justification_has_field_level_diagnostic() {
        let err = TraderOutput::parse_strict(
            r#"{"action":"hold","conviction":0.7,"justification":" "}"#,
            "01TEST",
            3,
        )
        .expect_err("empty justification must fail");

        assert!(err
            .to_string()
            .contains("trader output justification is required"));
    }

    #[test]
    fn empty_response_has_provider_diagnostic_instead_of_json_eof() {
        let response = LlmResponse {
            content: Vec::new(),
            stop_reason: StopReason::EndTurn,
            input_tokens: 981,
            output_tokens: 0,
        };

        let err = TraderOutput::parse_response(&response, "01KRMKWZ1KJ2BGRNWGP518ZQ3Q", 4)
            .expect_err("empty trader text must fail before JSON parsing");
        let message = err.to_string();

        assert!(message.contains("trader output is empty"));
        assert!(message.contains("decision 4"));
        assert!(message.contains("stop_reason=EndTurn"));
        assert!(message.contains("output_tokens=0"));
        assert!(
            !message.contains("EOF while parsing"),
            "empty response should not be reported as JSON EOF: {message}"
        );
    }

    #[test]
    fn response_parse_errors_include_provider_metadata() {
        let response = LlmResponse {
            content: vec![crate::agent::llm::ContentBlock::Text { text: "{".into() }],
            stop_reason: StopReason::MaxTokens,
            input_tokens: 1000,
            output_tokens: 1000,
        };

        let err = TraderOutput::parse_response(&response, "01TEST", 2)
            .expect_err("truncated trader JSON must fail");
        let message = err.to_string();

        assert!(message.contains("invalid JSON"));
        assert!(message.contains("stop_reason=MaxTokens"));
        assert!(message.contains("output_tokens=1000"));
    }

    #[test]
    fn recovers_json_from_code_fence_and_trailing_text() {
        let parsed = TraderOutput::parse_strict(
            "Here is the decision:\n```json\n{\"action\":\"hold\",\"conviction\":0.4,\"justification\":\"range chop\"}\n```\nDone.",
            "01TEST",
            5,
        )
        .expect("valid fenced JSON should parse");

        assert_eq!(parsed.action, "hold");
        assert_eq!(parsed.justification, "range chop");
    }

    #[test]
    fn recovers_json_from_provider_output_wrapper() {
        let parsed = TraderOutput::parse_strict(
            r#"{"output":"{\"action\":\"long_open\",\"conviction\":0.8,\"justification\":\"breakout\"}"}"#,
            "01TEST",
            6,
        )
        .expect("wrapped JSON string should parse");

        assert_eq!(parsed.action, "long_open");
        assert_eq!(parsed.conviction, 0.8);
    }
}
