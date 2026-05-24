//! A/B orchestration. Composes a shared Intern + Trader HTTP backend +
//! trajectory-keyed briefing replay across N arms (one `TraderArm` plus
//! optional baselines) and drives them through `BacktestRunner` over
//! historical OHLCV.
//!
//! Post-CV-extraction (ADR 0011) the arm split is no longer "vectors on /
//! off / random / orthogonal" — there is one TraderArm (LLM-without-
//! steering) and any number of classical baselines.
//!
//! ## A/B pairing under trajectories (Stage 3, Task 6 — three modes)
//!
//! Each slot's recording is keyed by `TrajectoryKey.fingerprint()`. There
//! are three possible pairing modes:
//!
//! 1. **shared-briefing** — one recording per cycle regardless of arm
//!    (historical pre-trajectory behavior for the intern briefing).
//! 2. **shared-slot, fingerprint-driven** — arms that resolve to the SAME
//!    slot identity (provider + model) share a recording (`arm_scope =
//!    None`); arms with a DIFFERENT identity get distinct recordings purely
//!    because their fingerprint differs (the model is part of the key). No
//!    `arm_scope` plumbing needed.
//! 3. **per-arm per-slot** — `arm_scope = Some(arm_id)` forces isolation
//!    even when the slot identity is the same.
//!
//! **xvision uses mode 2 (shared-slot, fingerprint-driven) for the intern
//! briefing.** Two trader arms with the same intern provider/model share one
//! intern recording (preserving the old shared-briefing pairing exactly);
//! arms whose trader model differs but whose intern model matches still
//! share the intern recording — divergence then reflects the trader, not
//! intern non-determinism. An arm that pins a distinct intern model records
//! its own briefing automatically (its fingerprint differs).

use std::sync::Arc;

use anyhow::anyhow;

use xvision_core::market::MarketSnapshot;
use xvision_core::slot::SlotRef;
use xvision_risk::RiskLayer;
use xvision_trader::TraderParams;

use crate::algorithm::Algorithm;
use crate::backtest::MarketBar;
use crate::baselines::{
    AlwaysLong, AlwaysShort, BriefingReplay, BuyAndHold, MaCrossover, MacdMomentum, PortfolioProvider,
    RandomDirection, RsiMeanReversion, TraderArm,
};
use crate::harness::{ArmConfig, BacktestRunConfig, BacktestRunner};
use crate::provider_registry::ProviderRegistry;
use crate::result::BacktestResult;

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
    RandomDirection {
        seed: u64,
    },
    RsiMeanReversion,
    MaCrossover {
        fast: usize,
        slow: usize,
    },
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

/// Mutates `specs` in place so any two `trader_arm` entries with distinct
/// slot configs end up with distinct names. Bare `trader_arm` (no slot
/// overrides) keeps its name unchanged so existing scripts/reports keep
/// working.
pub fn auto_suffix_arm_names(specs: &mut [ArmSpec]) {
    // Pass 1: derive the candidate suffix per spec.
    let mut suffixes: Vec<Option<String>> = specs
        .iter()
        .map(|spec| match &spec.kind {
            ArmKind::Trader { intern, trader } => match (trader, intern) {
                (None, None) => None, // bare trader_arm — leave alone
                (Some(t), _) => Some(short_model_segment(&t.model)),
                (None, Some(i)) => Some(format!("i:{}", short_model_segment(&i.model))),
            },
            _ => None,
        })
        .collect();

    // Pass 2: detect collisions on the candidate suffix and promote to
    // `<model>@<provider>` when needed. Only applies among `Trader` specs.
    let mut by_suffix: std::collections::HashMap<String, Vec<usize>> = std::collections::HashMap::new();
    for (i, s) in suffixes.iter().enumerate() {
        if let Some(s) = s {
            by_suffix.entry(s.clone()).or_default().push(i);
        }
    }
    let mut promotions: Vec<(usize, String)> = vec![];
    for (_, idxs) in by_suffix.iter().filter(|(_, v)| v.len() > 1) {
        for &i in idxs {
            if let ArmKind::Trader { trader, intern } = &specs[i].kind {
                let (model, provider) = match (trader, intern) {
                    (Some(t), _) => (&t.model, &t.provider),
                    (None, Some(j)) => (&j.model, &j.provider),
                    _ => continue,
                };
                let suffix = format!("{}@{}", short_model_segment(model), provider);
                promotions.push((i, suffix));
            }
        }
    }
    for (i, suf) in promotions {
        suffixes[i] = Some(suf);
    }

    // Pass 3: apply.
    for (spec, suffix) in specs.iter_mut().zip(suffixes) {
        if let Some(suf) = suffix {
            spec.name = format!("{}[{}]", spec.name, suf);
        }
    }
}

/// `meta-llama/Llama-3.3-70B-Instruct-Turbo` → `Llama-3.3-70B-Instruct-Turbo`.
/// Trims to 32 chars to keep BacktestResult arm names readable.
fn short_model_segment(model: &str) -> String {
    let last = model.rsplit('/').next().unwrap_or(model);
    last.chars().take(32).collect()
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
///
/// Per-arm `ArmKind::Trader` entries may carry inline `intern` / `trader`
/// `SlotRef` overrides; `None` falls back to the registry's
/// `default_intern` / `default_trader`. Two arms that resolve to the same
/// `(provider, model)` share a backend `Arc` via `ProviderRegistry`'s
/// memoization so they reuse one HTTP client.
#[allow(clippy::too_many_arguments)]
pub async fn run_ab_compare(
    snapshots: Vec<MarketSnapshot>,
    bars: Vec<MarketBar>,
    arms: Vec<ArmSpec>,
    config: BacktestRunConfig,
    registry: Arc<ProviderRegistry>,
    trader_params: TraderParams,
    portfolio_provider: PortfolioProvider,
    risk: &RiskLayer,
) -> anyhow::Result<BacktestResult> {
    // Shared briefing-replay store (replaces the old in-memory
    // `BriefingCache`). Pairing falls out of `TrajectoryKey.fingerprint()`:
    // arms with the same intern identity share one recording, arms with a
    // distinct intern model record independently (Task 6, mode 2).
    let replay = Arc::new(BriefingReplay::new());

    let arm_configs: Vec<ArmConfig> = arms
        .into_iter()
        .map(|spec| -> anyhow::Result<ArmConfig> {
            // Leak the arm name into a 'static str — harness wants &'static str.
            // The leak is bounded (one per arm per process invocation, ≤8 in
            // practice) and the runtime is short-lived.
            let static_name: &'static str = Box::leak(spec.name.clone().into_boxed_str());
            let strategy: Box<dyn Algorithm> = match spec.kind {
                ArmKind::Trader { intern, trader } => {
                    let intern_slot = intern.unwrap_or_else(|| registry.default_intern.clone());
                    let trader_slot = trader.unwrap_or_else(|| registry.default_trader.clone());
                    let intern_backend = registry.intern_backend(&intern_slot)?;
                    let trader_backend = registry.trader_backend(&trader_slot)?;
                    // Resolve the empty-provider sentinel (from `intern_model=` /
                    // `trader_model=` shorthand) to the registry default so the
                    // briefing cache key sees the explicit provider.
                    let resolved_intern = if intern_slot.provider.is_empty() {
                        SlotRef::new(
                            registry.default_intern.provider.clone(),
                            intern_slot.model.clone(),
                        )
                    } else {
                        intern_slot
                    };
                    tracing::info!(
                        target: "ab_compare",
                        arm = %spec.name,
                        intern = %resolved_intern,
                        trader = %trader_slot,
                        "arm dispatch"
                    );
                    Box::new(TraderArm::new(
                        static_name,
                        intern_backend,
                        resolved_intern.provider.clone(),
                        resolved_intern.model.clone(),
                        // Mode 2 (shared-slot, fingerprint-driven): the
                        // intern slot is shared across arms — `arm_scope =
                        // None`. Distinctness for arms that pin a different
                        // intern model flows from the model being part of the
                        // fingerprint, so no per-arm scope is needed here.
                        None,
                        Arc::clone(&replay),
                        trader_backend,
                        trader_params.clone(),
                        Arc::clone(&portfolio_provider),
                    ))
                }
                ArmKind::BuyAndHold => Box::new(BuyAndHold::new()),
                ArmKind::AlwaysLong => Box::new(AlwaysLong),
                ArmKind::AlwaysShort => Box::new(AlwaysShort),
                ArmKind::RandomDirection { seed } => Box::new(RandomDirection::new(seed)),
                ArmKind::RsiMeanReversion => Box::new(RsiMeanReversion::new()),
                ArmKind::MaCrossover { fast, slow } => Box::new(MaCrossover::new(fast, slow)),
                ArmKind::MacdMomentum => Box::new(MacdMomentum::new()),
            };
            Ok(ArmConfig {
                name: spec.name,
                strategy,
            })
        })
        .collect::<anyhow::Result<Vec<_>>>()?;

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
    fn auto_suffix_uses_last_path_segment_of_model() {
        let mut specs = vec![
            parse_arm_spec("trader_arm:trader=anthropic/claude-opus-4-7").unwrap(),
            parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
        ];
        auto_suffix_arm_names(&mut specs);
        assert_eq!(specs[0].name, "trader_arm[claude-opus-4-7]");
        assert_eq!(specs[1].name, "trader_arm[gpt-4o]");
    }

    #[test]
    fn auto_suffix_strips_provider_path_from_model_id() {
        let mut specs =
            vec![
                parse_arm_spec("trader_arm:trader=together/meta-llama/Llama-3.3-70B-Instruct-Turbo").unwrap(),
            ];
        auto_suffix_arm_names(&mut specs);
        assert_eq!(specs[0].name, "trader_arm[Llama-3.3-70B-Instruct-Turbo]");
    }

    #[test]
    fn auto_suffix_appends_provider_when_models_collide() {
        let mut specs = vec![
            parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
            parse_arm_spec("trader_arm:trader=together/gpt-4o").unwrap(),
        ];
        auto_suffix_arm_names(&mut specs);
        assert_eq!(specs[0].name, "trader_arm[gpt-4o@openai]");
        assert_eq!(specs[1].name, "trader_arm[gpt-4o@together]");
    }

    #[test]
    fn auto_suffix_leaves_bare_trader_arm_alone() {
        let mut specs = vec![
            parse_arm_spec("trader_arm").unwrap(),
            parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
        ];
        auto_suffix_arm_names(&mut specs);
        assert_eq!(specs[0].name, "trader_arm");
        assert_eq!(specs[1].name, "trader_arm[gpt-4o]");
    }

    #[test]
    fn auto_suffix_ignores_non_trader_arms() {
        let mut specs = vec![
            parse_arm_spec("buy_and_hold").unwrap(),
            parse_arm_spec("trader_arm:trader=openai/gpt-4o").unwrap(),
        ];
        auto_suffix_arm_names(&mut specs);
        assert_eq!(specs[0].name, "buy_and_hold");
        assert_eq!(specs[1].name, "trader_arm[gpt-4o]");
    }

    #[test]
    fn auto_suffix_handles_intern_only_override() {
        let mut specs = vec![
            parse_arm_spec("trader_arm:intern=anthropic/claude-opus-4-7").unwrap(),
            parse_arm_spec("trader_arm:intern=anthropic/claude-haiku-4-5").unwrap(),
        ];
        auto_suffix_arm_names(&mut specs);
        // When only intern differs, suffix is the intern model id.
        assert_eq!(specs[0].name, "trader_arm[i:claude-opus-4-7]");
        assert_eq!(specs[1].name, "trader_arm[i:claude-haiku-4-5]");
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
        let a = parse_arm_spec("trader_arm:intern=anthropic/claude-haiku-4-5:trader=openai/gpt-4o").unwrap();
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
        let err = parse_arm_spec("trader_arm:intern=anthropic/x:intern_model=y").unwrap_err();
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
