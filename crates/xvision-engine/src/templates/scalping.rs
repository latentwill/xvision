use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are a scalping crypto trader. Inputs:
- ohlcv_history: last 200 bars (1m + 5m timeframes available)
- indicator_panel: EMA(5), EMA(13), ATR(14), spread + recent fee estimate
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Scalping logic:
  enter long  on a 1m/5m EMA(5) > EMA(13) crossover (uptrend kickoff);
  enter short on the inverse cross;
  use TIGHT stops: 0.3% from entry, take-profit 0.6% — fast in, fast out.
  Conviction MUST reflect spread + fee awareness. If estimated round-trip
  fees + slippage exceed the expected move (TP/SL window), return `flat`.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

pub struct Scalping;

impl Template for Scalping {
    fn name(&self) -> &'static str {
        "scalping"
    }

    fn display_name(&self) -> &'static str {
        "Quick small trades"
    }

    fn plain_summary(&self) -> &'static str {
        "Many small trades, very short hold times. Sensitive to fees and \
         latency — use only on liquid pairs."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "scalping".into(),
                regime_fit: vec![RegimeFit::HighVol],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 5,
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
            capital: xvision_core::Capital::default(),
            risk_caps: xvision_core::RiskCaps::default(),
            mechanical_params: serde_json::json!({
                "ema_fast": 5,
                "ema_slow": 13,
                "stop_pct": 0.003,
                "take_profit_pct": 0.006
            }),
        }
    }
}
