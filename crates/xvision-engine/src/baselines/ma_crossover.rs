use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::{PipelineDef, StrategyBundle};
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are wrapping a deterministic moving-average
crossover rule in an LLM confirmation step. Inputs:
- mechanical_signal: { kind: "ma_crossover", direction: "up"|"down"|"flat" }
- ohlcv_history: last 200 bars
- portfolio_state

Rule: when fast MA crosses above slow MA, the mechanical signal is "up";
crossover below is "down"; otherwise "flat".

Your job: confirm or veto the mechanical signal based on price context (sudden
spike, illiquid wick, gap). If you confirm "up", emit long_open. If "down",
emit short_open or flat depending on whether shorts are allowed. If "flat",
emit hold.

Output JSON: {action: long_open|short_open|flat|hold, conviction (0-1), justification}.
"#;

pub fn ma_crossover_template() -> Box<dyn Template> {
    Box::new(MaCrossover)
}

struct MaCrossover;

impl Template for MaCrossover {
    fn name(&self) -> &'static str {
        "ma_crossover_baseline"
    }

    fn display_name(&self) -> &'static str {
        "MA crossover (baseline)"
    }

    fn plain_summary(&self) -> &'static str {
        "Wraps the classic fast/slow moving-average crossover rule in an LLM confirmation step. \
         Used as a marketplace seed listing."
    }

    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().into(),
                creator,
                template: "ma_crossover_baseline".into(),
                regime_fit: vec![RegimeFit::TrendingBull, RegimeFit::TrendingBear],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "conservative".into(),
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
            risk: RiskPreset::Conservative.expand(),
            mechanical_params: serde_json::json!({
                "fast_ma_period": 20,
                "slow_ma_period": 50
            }),
        }
    }
}
