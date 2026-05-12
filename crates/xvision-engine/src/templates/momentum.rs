use crate::strategies::manifest::{PublicManifest, RegimeFit};
use crate::strategies::risk::RiskPreset;
use crate::strategies::slot::LLMSlot;
use crate::strategies::{PipelineDef, Strategy};
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are a momentum crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: MACD(12, 26, 9), ADX(14), ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Momentum logic:
  enter long  on a bullish MACD crossover (signal-line cross up) when ADX > 25;
  enter short on a bearish MACD crossover (signal-line cross down) when ADX > 25;
  cut (flat) when ADX falls below 20 — the trend has lost strength;
  hold when within an existing position and ADX > 25 with no opposing cross.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

pub struct Momentum;

impl Template for Momentum {
    fn name(&self) -> &'static str {
        "momentum"
    }

    fn display_name(&self) -> &'static str {
        "Rides momentum"
    }

    fn plain_summary(&self) -> &'static str {
        "Holds positions while momentum is strong; cuts when it fades."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> Strategy {
        Strategy {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "momentum".into(),
                regime_fit: vec![RegimeFit::TrendingBull, RegimeFit::TrendingBear],
                asset_universe: vec!["BTC/USD".into(), "ETH/USD".into()],
                decision_cadence_minutes: 60,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
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
            }),
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({
                "macd_fast": 12,
                "macd_slow": 26,
                "macd_signal": 9,
                "adx_period": 14,
                "adx_threshold": 25
            }),
        }
    }
}
