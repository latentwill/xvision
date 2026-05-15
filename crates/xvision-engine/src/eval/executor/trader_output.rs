use anyhow::{anyhow, Result};
use serde::Deserialize;

use crate::agent::llm::LlmResponse;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraderOutput {
    pub(crate) action: String,
    pub(crate) conviction: f64,
    pub(crate) justification: String,
}

impl TraderOutput {
    pub(crate) fn parse_response(
        response: &LlmResponse,
        run_id: &str,
        decision_index: u32,
    ) -> Result<Self> {
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

    pub(crate) fn parse_strict(raw: &str, run_id: &str, decision_index: u32) -> Result<Self> {
        Self::parse_with_metadata(raw, run_id, decision_index, None)
    }

    fn parse_with_metadata(
        raw: &str,
        run_id: &str,
        decision_index: u32,
        metadata: Option<&str>,
    ) -> Result<Self> {
        let parsed = serde_json::from_str::<Self>(raw).map_err(|e| {
            let mut detail = trader_output_error_detail(&e);
            if let Some(metadata) = metadata {
                detail = format!("{detail} ({metadata})");
            }
            anyhow!(
                "run {} decision {}: trader output is invalid JSON: {}",
                run_id,
                decision_index,
                detail
            )
        })?;
        parsed.validate(run_id, decision_index)?;
        Ok(parsed)
    }

    fn validate(&self, run_id: &str, decision_index: u32) -> Result<()> {
        if !matches!(
            self.action.as_str(),
            "long_open" | "short_open" | "flat" | "hold"
        ) {
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
    use crate::agent::llm::{ContentBlock, LlmResponse, StopReason};

    use super::TraderOutput;

    #[test]
    fn missing_action_has_field_level_diagnostic() {
        for run_id in [
            "01KRK9Y45K1MKS9FTH4TY4SK47",
            "01KRKATKTK331A08TQ2MBN6FYC",
        ] {
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
            content: vec![ContentBlock::Text {
                text: "{".into(),
            }],
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
}
