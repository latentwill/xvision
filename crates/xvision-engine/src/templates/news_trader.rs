use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;
use crate::templates::Template;

// NOTE: news/sentiment as a tool isn't wired in this plan slice (2a).
// The trader prompt acknowledges this and operates on price + indicators only
// as a fallback. Plan 2c adds the real news_sentiment tool, at which point
// this template's `required_tools` should grow `news_sentiment`.
const TRADER_PROMPT: &str = r#"You are a news-aware crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: ATR(14), recent volatility
- portfolio_state: open positions, available capital
- (would normally have a news_sentiment tool — NOT yet wired in this MVP)

Decide ONE of: long_open | short_open | flat | hold.
Logic:
  You would normally have access to a news_sentiment tool. In this MVP it is
  not yet wired — operate on price action only and emit `flat` UNLESS extreme
  volatility appears in ohlcv_history (defined as a >3 ATR move in the last
  4 bars), in which case bias the decision in the direction of the move.
  Conservative stance is the default; do not synthesize news without the tool.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

pub struct NewsTrader;

impl Template for NewsTrader {
    fn name(&self) -> &'static str {
        "news_trader"
    }

    fn display_name(&self) -> &'static str {
        "Trades news events"
    }

    fn plain_summary(&self) -> &'static str {
        "Reacts to news and sentiment changes. Requires a news API key \
         (configured separately)."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "news_trader".into(),
                regime_fit: vec![RegimeFit::EventDriven, RegimeFit::HighVol],
                asset_universe: vec!["ETH/USD".into()],
                decision_cadence_minutes: 15,
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
            mechanical_params: serde_json::json!({
                "extreme_move_atr_multiple": 3.0,
                "lookback_bars": 4
            }),
        }
    }
}
