use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;
use crate::templates::Template;

// Single-LLM-agent freeform template. Intentionally minimal — for sophisticated
// authors who want full discretion over their trader prompt. Only `trader_slot`
// is populated; no regime classifier, no intern. Risk preset defaults to
// `conservative` to err on the safe side for free-form authors.
const TRADER_PROMPT: &str = r#"You are a trading agent. Decide based on the inputs provided.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

pub struct Custom;

impl Template for Custom {
    fn name(&self) -> &'static str {
        "custom"
    }

    fn display_name(&self) -> &'static str {
        "Single-agent freeform"
    }

    fn plain_summary(&self) -> &'static str {
        "A blank canvas. One LLM trader agent with no scaffold — for \
         sophisticated authors who want full discretion."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "custom".into(),
                regime_fit: vec![
                    RegimeFit::TrendingBull,
                    RegimeFit::TrendingBear,
                    RegimeFit::RangeBound,
                    RegimeFit::Chop,
                ],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "conservative".into(),
                published_at: None,
            },
            regime_slot: None,
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: TRADER_PROMPT.into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            }),
            risk: RiskPreset::Conservative.expand(),
            mechanical_params: serde_json::json!({}),
        }
    }
}
