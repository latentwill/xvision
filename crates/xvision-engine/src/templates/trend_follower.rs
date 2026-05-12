use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::{PipelineDef, StrategyBundle};
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are a trend-following crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: EMA(12), EMA(26), EMA(50), ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Trend logic:
  enter long  when EMA(12) > EMA(26) > EMA(50) AND price > EMA(12);
  enter short when EMA(12) < EMA(26) < EMA(50) AND price < EMA(12);
  otherwise flat or hold (preserve capital while the trend is unclear).
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

pub struct TrendFollower;

impl Template for TrendFollower {
    fn name(&self) -> &'static str {
        "trend_follower"
    }

    fn display_name(&self) -> &'static str {
        "Catches uptrends"
    }

    fn plain_summary(&self) -> &'static str {
        "Buys when crypto starts trending up, sells when momentum fades. \
         Best when markets are moving."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "trend_follower".into(),
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
                "ema_fast": 12,
                "ema_mid": 26,
                "ema_slow": 50
            }),
        }
    }
}
