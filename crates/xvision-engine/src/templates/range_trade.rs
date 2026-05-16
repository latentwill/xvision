use crate::strategies::manifest::{PublicManifest, RegimeFit};
use crate::strategies::risk::RiskPreset;
use crate::strategies::slot::LLMSlot;
use crate::strategies::{PipelineDef, Strategy};
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are a range-bound crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: Bollinger(20, 2) %B oscillator, ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Range logic:
  enter long  when %B < 0.10 AND close > prior close (oversold + reversal);
  enter short when %B > 0.90 AND close < prior close (overbought + reversal);
  otherwise flat or hold — only trade the range during sideways markets.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

pub struct RangeTrade;

impl Template for RangeTrade {
    fn name(&self) -> &'static str {
        "range_trade"
    }

    fn display_name(&self) -> &'static str {
        "Trades the range"
    }

    fn plain_summary(&self) -> &'static str {
        "Buys near support, sells near resistance — only during sideways markets."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "range_trade".into(),
                regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
                asset_universe: vec!["ETH/USD".into()],
                decision_cadence_minutes: 30,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "conservative".into(),
                published_at: None,

                min_warmup_bars: None,
            },
            agents: Vec::new(),
            pipeline: PipelineDef::default(),
            regime_slot: None,
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: TRADER_PROMPT.into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                provider: None,
                model: None,
            }),
            risk: RiskPreset::Conservative.expand(),
            mechanical_params: serde_json::json!({
                "bb_period": 20,
                "bb_sigma": 2.0,
                "lower_threshold": 0.1,
                "upper_threshold": 0.9
            }),
        }
    }
}
