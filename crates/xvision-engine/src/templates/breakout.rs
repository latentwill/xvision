use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::{PipelineDef, StrategyBundle};
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are a breakout crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: Donchian(20), SMA(volume, 20), ATR(14)
  (if Donchian isn't in the panel, use highest-high / lowest-low of the last
  20 bars from ohlcv_history as a fallback)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Breakout logic:
  enter long when close > donchian_high(20) AND volume > 1.5 * SMA(volume, 20);
  exit (flat) when momentum stalls — close inside the prior range or volume
  falls below 1× SMA(volume, 20).
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

pub struct Breakout;

impl Template for Breakout {
    fn name(&self) -> &'static str {
        "breakout"
    }

    fn display_name(&self) -> &'static str {
        "Buys breakouts"
    }

    fn plain_summary(&self) -> &'static str {
        "Buys when price breaks above its recent range, exits on stall."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "breakout".into(),
                regime_fit: vec![RegimeFit::TrendingBull, RegimeFit::HighVol],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 30,
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
                "donchian_period": 20,
                "volume_confirm_multiple": 1.5
            }),
        }
    }
}
