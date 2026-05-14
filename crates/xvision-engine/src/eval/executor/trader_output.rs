use anyhow::{anyhow, Result};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraderOutput {
    pub(crate) action: String,
    pub(crate) conviction: f64,
    pub(crate) justification: String,
}

impl TraderOutput {
    pub(crate) fn parse_strict(raw: &str, run_id: &str, decision_index: u32) -> Result<Self> {
        let parsed = serde_json::from_str::<Self>(raw).map_err(|e| {
            anyhow!(
                "run {} decision {}: trader output is invalid JSON: {}",
                run_id,
                decision_index,
                trader_output_error_detail(&e)
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
}
