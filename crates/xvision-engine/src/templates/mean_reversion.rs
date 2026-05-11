use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are a mean-reversion crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: RSI(14), Bollinger(20, 2), ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Mean-reversion logic: enter long when RSI < 30 AND price < lower_bollinger;
enter short when RSI > 70 AND price > upper_bollinger; otherwise flat or hold.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

const REGIME_PROMPT: &str = r#"Classify the current crypto market regime as one of:
trending_bull | trending_bear | range_bound | chop.
Use indicator_panel + recent ohlcv_history. Return JSON: {regime, confidence (0-1)}.
"#;

pub struct MeanReversion;

impl Template for MeanReversion {
    fn name(&self) -> &'static str {
        "mean_reversion"
    }

    fn display_name(&self) -> &'static str {
        "Buys dips"
    }

    fn plain_summary(&self) -> &'static str {
        "Buys when prices drop below normal range and sells when they recover. \
         Best in calm sideways markets."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "mean_reversion".into(),
                regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
                asset_universe: vec!["ETH/USD".into()],
                decision_cadence_minutes: 15,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
            },
            regime_slot: Some(LLMSlot {
                role: "regime".into(),
                prompt: REGIME_PROMPT.into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["indicator_panel".into()],
            }),
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: TRADER_PROMPT.into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            }),
            risk: RiskPreset::Balanced.expand(),
            capital: xvision_core::Capital::default(),
            risk_caps: xvision_core::RiskCaps::default(),
            mechanical_params: serde_json::json!({
                "rsi_oversold": 30, "rsi_overbought": 70,
                "bollinger_period": 20, "bollinger_sigma": 2.0,
                "atr_period": 14
            }),
        }
    }
}
