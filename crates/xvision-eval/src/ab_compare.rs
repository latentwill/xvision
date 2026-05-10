//! A/B orchestration. Composes a shared Intern + Trader HTTP backend +
//! BriefingCache across N arms (one `TraderArm` plus optional baselines)
//! and drives them through `BacktestRunner` over historical OHLCV.
//!
//! Post-CV-extraction (ADR 0011) the arm split is no longer "vectors on /
//! off / random / orthogonal" — there is one TraderArm (LLM-without-
//! steering) and any number of classical baselines.

use std::sync::Arc;

use anyhow::anyhow;

use xvision_core::market::MarketSnapshot;
use xvision_core::slot::SlotRef;
use xvision_intern::{BriefingCache, InternBackend};
use xvision_risk::RiskLayer;
use xvision_trader::{TraderBackend, TraderParams};

use crate::backtest::MarketBar;
use crate::baselines::{
    AlwaysLong, AlwaysShort, BuyAndHold, MaCrossover, MacdMomentum, PortfolioProvider,
    RandomDirection, RsiMeanReversion, TraderArm,
};
use crate::harness::{ArmConfig, BacktestRunConfig, BacktestRunner};
use crate::result::BacktestResult;
use crate::algorithm::Algorithm;

/// One arm spec parsed from the CLI.
#[derive(Debug, Clone)]
pub struct ArmSpec {
    pub name: String,
    pub kind: ArmKind,
}

/// Which strategy this arm wraps. Post-CV-extraction the only LLM arm is
/// `TraderArm` (no per-arm vector config); classical baselines are listed
/// for explicit selection from the CLI. The `Trader` variant carries optional
/// `intern` / `trader` slot overrides so a single `xvn ab-compare` run can
/// pit the same strategy against multiple LLM (provider, model) combinations
/// — see Plan #7 (LLM providers + per-arm models). When both slots are
/// `None`, the arm uses the global `[intern]` / `[trader]` config defaults.
#[derive(Debug, Clone)]
pub enum ArmKind {
    Trader {
        intern: Option<SlotRef>,
        trader: Option<SlotRef>,
    },
    BuyAndHold,
    AlwaysLong,
    AlwaysShort,
    RandomDirection { seed: u64 },
    RsiMeanReversion,
    MaCrossover { fast: usize, slow: usize },
    MacdMomentum,
}

/// Parse a CLI arm string. Forms accepted:
/// - `trader_arm`        — the LLM-driven TraderArm (Stage 1 + 2 pipeline).
/// - `buy_and_hold` | `always_long` | `always_short`
/// - `random_direction:seed=<u64>`
/// - `rsi_mean_reversion`
/// - `ma_crossover:fast=<usize>:slow=<usize>`
/// - `macd_momentum`
pub fn parse_arm_spec(s: &str) -> anyhow::Result<ArmSpec> {
    let mut parts = s.splitn(2, ':');
    let head = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    match head {
        "trader_arm" => {
            let kv = parse_kv(rest);
            const ALLOWED: &[&str] = &["intern", "trader", "intern_model", "trader_model"];
            for k in kv.keys() {
                if !ALLOWED.contains(&k.as_str()) {
                    return Err(anyhow!("unknown key `{k}` in trader_arm spec"));
                }
            }
            if kv.contains_key("intern") && kv.contains_key("intern_model") {
                return Err(anyhow!(
                    "`intern=` and `intern_model=` are mutually exclusive on trader_arm"
                ));
            }
            if kv.contains_key("trader") && kv.contains_key("trader_model") {
                return Err(anyhow!(
                    "`trader=` and `trader_model=` are mutually exclusive on trader_arm"
                ));
            }
            // Empty-provider trick (`SlotRef { provider: "", model: ... }`) is
            // the marker for "shorthand — fill provider from CLI flag default at
            // ProviderRegistry resolve time". Only produced by intern_model= /
            // trader_model= shorthand, only consumed by Phase 3's resolver.
            let intern = match (kv.get("intern"), kv.get("intern_model")) {
                (Some(slot), _) => Some(
                    slot.parse::<SlotRef>()
                        .map_err(|e| anyhow!("intern slot ref: {e}"))?,
                ),
                (_, Some(model)) => Some(SlotRef::new("", model.clone())),
                _ => None,
            };
            let trader = match (kv.get("trader"), kv.get("trader_model")) {
                (Some(slot), _) => Some(
                    slot.parse::<SlotRef>()
                        .map_err(|e| anyhow!("trader slot ref: {e}"))?,
                ),
                (_, Some(model)) => Some(SlotRef::new("", model.clone())),
                _ => None,
            };
            Ok(ArmSpec {
                name: "trader_arm".into(),
                kind: ArmKind::Trader { intern, trader },
            })
        }
        "buy_and_hold" => Ok(ArmSpec {
            name: "buy_and_hold".into(),
            kind: ArmKind::BuyAndHold,
        }),
        "always_long" => Ok(ArmSpec {
            name: "always_long".into(),
            kind: ArmKind::AlwaysLong,
        }),
        "always_short" => Ok(ArmSpec {
            name: "always_short".into(),
            kind: ArmKind::AlwaysShort,
        }),
        "random_direction" => {
            let kv = parse_kv(rest);
            let seed = kv.get("seed").and_then(|s| s.parse().ok()).unwrap_or(42);
            Ok(ArmSpec {
                name: "random_direction".into(),
                kind: ArmKind::RandomDirection { seed },
            })
        }
        "rsi_mean_reversion" => Ok(ArmSpec {
            name: "rsi_mean_reversion".into(),
            kind: ArmKind::RsiMeanReversion,
        }),
        "ma_crossover" => {
            let kv = parse_kv(rest);
            let fast = kv.get("fast").and_then(|s| s.parse().ok()).unwrap_or(30);
            let slow = kv.get("slow").and_then(|s| s.parse().ok()).unwrap_or(90);
            Ok(ArmSpec {
                name: "ma_crossover".into(),
                kind: ArmKind::MaCrossover { fast, slow },
            })
        }
        "macd_momentum" => Ok(ArmSpec {
            name: "macd_momentum".into(),
            kind: ArmKind::MacdMomentum,
        }),
        other => Err(anyhow!("unknown arm head: `{other}`")),
    }
}

fn parse_kv(s: &str) -> std::collections::BTreeMap<String, String> {
    s.split(':')
        .filter_map(|kv| {
            let mut it = kv.splitn(2, '=');
            let k = it.next()?.to_string();
            let v = it.next()?.to_string();
            Some((k, v))
        })
        .collect()
}

/// Default arm set used by the CLI when `--arms` is omitted: the LLM
/// `trader_arm` plus a `buy_and_hold` reference baseline.
pub fn default_arms() -> Vec<ArmSpec> {
    vec![
        ArmSpec {
            name: "trader_arm".into(),
            kind: ArmKind::Trader {
                intern: None,
                trader: None,
            },
        },
        ArmSpec {
            name: "buy_and_hold".into(),
            kind: ArmKind::BuyAndHold,
        },
    ]
}

/// Run an N-arm A/B comparison. Returns the `BacktestResult` for serialisation.
#[allow(clippy::too_many_arguments)]
pub async fn run_ab_compare(
    snapshots: Vec<MarketSnapshot>,
    bars: Vec<MarketBar>,
    arms: Vec<ArmSpec>,
    config: BacktestRunConfig,
    intern: Arc<dyn InternBackend>,
    intern_provider: String,
    intern_model: String,
    trader: Arc<dyn TraderBackend>,
    trader_params: TraderParams,
    portfolio_provider: PortfolioProvider,
    risk: &RiskLayer,
) -> anyhow::Result<BacktestResult> {
    let cache = Arc::new(BriefingCache::new());

    let arm_configs: Vec<ArmConfig> = arms
        .into_iter()
        .map(|spec| {
            // Leak the arm name into a 'static str — harness wants &'static str.
            // The leak is bounded (one per arm per process invocation, ≤8 in
            // practice) and the runtime is short-lived.
            let static_name: &'static str = Box::leak(spec.name.clone().into_boxed_str());
            let strategy: Box<dyn Algorithm> = match spec.kind {
                // The slot fields are read-but-ignored at this point in Plan #7
                // (Phase 3 wires them through ProviderRegistry). Phase 2 just
                // lands the type plumbing so the parser + CLI can populate them.
                ArmKind::Trader { intern: _, trader: _ } => Box::new(TraderArm::new(
                    static_name,
                    Arc::clone(&intern),
                    intern_provider.clone(),
                    intern_model.clone(),
                    Arc::clone(&cache),
                    Arc::clone(&trader),
                    trader_params.clone(),
                    Arc::clone(&portfolio_provider),
                )),
                ArmKind::BuyAndHold => Box::new(BuyAndHold::new()),
                ArmKind::AlwaysLong => Box::new(AlwaysLong),
                ArmKind::AlwaysShort => Box::new(AlwaysShort),
                ArmKind::RandomDirection { seed } => Box::new(RandomDirection::new(seed)),
                ArmKind::RsiMeanReversion => Box::new(RsiMeanReversion::new()),
                ArmKind::MaCrossover { fast, slow } => Box::new(MaCrossover::new(fast, slow)),
                ArmKind::MacdMomentum => Box::new(MacdMomentum::new()),
            };
            ArmConfig {
                name: spec.name,
                strategy,
            }
        })
        .collect();

    let mut runner = BacktestRunner::new(config, arm_configs)?;
    let result = runner.run(&snapshots, &bars, risk).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_trader_arm() {
        let a = parse_arm_spec("trader_arm").unwrap();
        assert_eq!(a.name, "trader_arm");
        assert!(matches!(
            a.kind,
            ArmKind::Trader {
                intern: None,
                trader: None
            }
        ));
    }

    #[test]
    fn trader_arm_kind_carries_optional_slots() {
        let spec = ArmSpec {
            name: "trader_arm".into(),
            kind: ArmKind::Trader {
                intern: Some(SlotRef::new("anthropic", "claude-opus-4-7")),
                trader: None,
            },
        };
        match spec.kind {
            ArmKind::Trader { intern, trader } => {
                assert_eq!(intern.unwrap().model, "claude-opus-4-7");
                assert!(trader.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_trader_arm_with_intern_slot() {
        let a = parse_arm_spec("trader_arm:intern=anthropic/claude-opus-4-7").unwrap();
        match a.kind {
            ArmKind::Trader { intern, trader } => {
                let s = intern.expect("intern slot must be present");
                assert_eq!(s.provider, "anthropic");
                assert_eq!(s.model, "claude-opus-4-7");
                assert!(trader.is_none());
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_trader_arm_with_trader_slot_only() {
        let a = parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap();
        match a.kind {
            ArmKind::Trader { intern, trader } => {
                assert!(intern.is_none());
                let s = trader.expect("trader slot must be present");
                assert_eq!(s.provider, "openai");
                assert_eq!(s.model, "gpt-4o");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_trader_arm_with_both_slots() {
        let a = parse_arm_spec(
            "trader_arm:intern=anthropic/claude-haiku-4-5:trader=openai/gpt-4o",
        )
        .unwrap();
        match a.kind {
            ArmKind::Trader { intern, trader } => {
                assert_eq!(intern.unwrap().model, "claude-haiku-4-5");
                assert_eq!(trader.unwrap().model, "gpt-4o");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parses_trader_model_shorthand() {
        let a = parse_arm_spec("trader_arm:trader_model=gpt-4o-mini").unwrap();
        match a.kind {
            ArmKind::Trader { intern, trader } => {
                assert!(intern.is_none());
                let s = trader.expect("trader slot must be present");
                assert_eq!(s.provider, "");
                assert_eq!(s.model, "gpt-4o-mini");
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn rejects_intern_and_intern_model_together() {
        let err =
            parse_arm_spec("trader_arm:intern=anthropic/x:intern_model=y").unwrap_err();
        assert!(format!("{err}").contains("mutually exclusive"));
    }

    #[test]
    fn rejects_trader_arm_with_unknown_kv() {
        let err = parse_arm_spec("trader_arm:bogus=x").unwrap_err();
        assert!(format!("{err}").contains("unknown key"));
    }

    #[test]
    fn parse_buy_and_hold() {
        let a = parse_arm_spec("buy_and_hold").unwrap();
        assert_eq!(a.name, "buy_and_hold");
        assert!(matches!(a.kind, ArmKind::BuyAndHold));
    }

    #[test]
    fn parse_random_with_seed() {
        let a = parse_arm_spec("random_direction:seed=7").unwrap();
        assert_eq!(a.name, "random_direction");
        match a.kind {
            ArmKind::RandomDirection { seed } => assert_eq!(seed, 7),
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_ma_crossover_with_windows() {
        let a = parse_arm_spec("ma_crossover:fast=20:slow=80").unwrap();
        assert_eq!(a.name, "ma_crossover");
        match a.kind {
            ArmKind::MaCrossover { fast, slow } => {
                assert_eq!(fast, 20);
                assert_eq!(slow, 80);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_unknown_head_errors() {
        assert!(parse_arm_spec("bogus").is_err());
    }

    #[test]
    fn default_arms_includes_trader_and_buy_and_hold() {
        let arms = default_arms();
        let names: Vec<_> = arms.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"trader_arm"));
        assert!(names.contains(&"buy_and_hold"));
    }
}
