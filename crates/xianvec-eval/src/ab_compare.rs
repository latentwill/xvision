//! Phase 9.1 — A/B orchestration. Composes a shared Intern + Qwen3Engine
//! + BriefingCache across N `TraderArm`s (each with its own `VectorConfig`)
//! and drives them through `BacktestRunner` over historical OHLCV.
//!
//! The CLI parses arm spec strings (`off`, `on:<npz>:<manifest>:<alpha>`,
//! `random:k=v:...`, `orthogonal:k=v:...`) and hands the resulting
//! `Vec<ArmSpec>` to `run_ab_compare`. v1 only the `off` arm is end-to-end
//! tested; the other three short-circuit-with-warn until F1+F2 land
//! (see `crates/xianvec-eval/src/baselines/trader_arm.rs`).

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Context};
use tokio::sync::Mutex;

use xianvec_core::market::MarketSnapshot;
use xianvec_core::trading::DispositionAxis;
use xianvec_core::Manifest;
use xianvec_inference::engine::Qwen3Engine;
use xianvec_intern::{BriefingCache, InternBackend};
use xianvec_risk::RiskLayer;
use xianvec_trader::TraderParams;

use crate::backtest::MarketBar;
use crate::baselines::{PortfolioProvider, TraderArm, VectorConfig};
use crate::harness::{ArmConfig, BacktestRunConfig, BacktestRunner};
use crate::result::BacktestResult;

/// One arm spec parsed from the CLI.
#[derive(Debug, Clone)]
pub struct ArmSpec {
    pub name: String,
    pub vector: VectorConfig,
}

/// Parse a CLI arm string. Forms accepted:
/// - `off`
/// - `on:<npz_path>:<manifest_path>:<alpha>`
/// - `random:layer=<u16>:dim=<usize>:alpha=<f32>:seed=<u64>`
/// - `orthogonal:axis=<conviction|patience|risk|trend>:path=<npz>:alpha=<f32>:seed=<u64>`
pub fn parse_arm_spec(s: &str) -> anyhow::Result<ArmSpec> {
    let mut parts = s.splitn(2, ':');
    let head = parts.next().unwrap_or("");
    let rest = parts.next().unwrap_or("");
    match head {
        "off" => Ok(ArmSpec {
            name: "vectors_off".into(),
            vector: VectorConfig::Off,
        }),
        "on" => {
            let mut p = rest.splitn(3, ':');
            let npz = p.next().ok_or_else(|| anyhow!("on: missing npz path"))?;
            let manifest_path = p.next().ok_or_else(|| anyhow!("on: missing manifest path"))?;
            let alpha: f32 = p
                .next()
                .unwrap_or("1.0")
                .parse()
                .context("on: alpha parse")?;
            let manifest_bytes = std::fs::read(manifest_path)?;
            let manifest: Manifest = serde_json::from_slice(&manifest_bytes)
                .context("on: parse manifest sidecar")?;
            Ok(ArmSpec {
                name: "vectors_on".into(),
                vector: VectorConfig::On {
                    manifest,
                    npz_path: PathBuf::from(npz),
                    alpha,
                },
            })
        }
        "random" => {
            let kv = parse_kv(rest);
            Ok(ArmSpec {
                name: "vectors_random".into(),
                vector: VectorConfig::Random {
                    seed: kv.get("seed").and_then(|s| s.parse().ok()).unwrap_or(42),
                    layer: kv.get("layer").and_then(|s| s.parse().ok()).unwrap_or(20),
                    hidden_dim: kv
                        .get("dim")
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(5120),
                    alpha: kv.get("alpha").and_then(|s| s.parse().ok()).unwrap_or(1.0),
                },
            })
        }
        "orthogonal" => {
            let kv = parse_kv(rest);
            let axis = match kv.get("axis").map(String::as_str) {
                Some("conviction") | None => DispositionAxis::Conviction,
                Some("patience") => DispositionAxis::Patience,
                Some("risk") => DispositionAxis::RiskAppetite,
                Some("trend") => DispositionAxis::TrendDisposition,
                Some(other) => anyhow::bail!("orthogonal: unknown axis `{other}`"),
            };
            let path = kv
                .get("path")
                .map(PathBuf::from)
                .ok_or_else(|| anyhow!("orthogonal: missing path=<npz>"))?;
            Ok(ArmSpec {
                name: "vectors_orthogonal".into(),
                vector: VectorConfig::Orthogonal {
                    axis,
                    seed: kv.get("seed").and_then(|s| s.parse().ok()).unwrap_or(42),
                    npz_path: path,
                    alpha: kv.get("alpha").and_then(|s| s.parse().ok()).unwrap_or(1.0),
                },
            })
        }
        other => anyhow::bail!("unknown arm head: `{other}`"),
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
    engine: Arc<Mutex<Qwen3Engine>>,
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
            let arm = TraderArm::new(
                static_name,
                Arc::clone(&intern),
                intern_provider.clone(),
                intern_model.clone(),
                Arc::clone(&cache),
                Arc::clone(&engine),
                trader_params.clone(),
                spec.vector,
                Arc::clone(&portfolio_provider),
            );
            ArmConfig {
                name: spec.name,
                strategy: Box::new(arm),
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
    fn parse_off_arm() {
        let a = parse_arm_spec("off").unwrap();
        assert_eq!(a.name, "vectors_off");
        assert!(matches!(a.vector, VectorConfig::Off));
    }

    #[test]
    fn parse_random_arm() {
        let a = parse_arm_spec("random:layer=20:dim=5120:alpha=1.5:seed=7").unwrap();
        assert_eq!(a.name, "vectors_random");
        match a.vector {
            VectorConfig::Random {
                seed,
                layer,
                hidden_dim,
                alpha,
            } => {
                assert_eq!(seed, 7);
                assert_eq!(layer, 20);
                assert_eq!(hidden_dim, 5120);
                assert!((alpha - 1.5).abs() < 1e-6);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_orthogonal_arm_with_axis() {
        let a = parse_arm_spec("orthogonal:axis=patience:path=/tmp/v.npz:alpha=0.5:seed=11")
            .unwrap();
        assert_eq!(a.name, "vectors_orthogonal");
        match a.vector {
            VectorConfig::Orthogonal {
                axis,
                seed,
                npz_path,
                alpha,
            } => {
                assert_eq!(axis, DispositionAxis::Patience);
                assert_eq!(seed, 11);
                assert_eq!(npz_path, PathBuf::from("/tmp/v.npz"));
                assert!((alpha - 0.5).abs() < 1e-6);
            }
            _ => panic!("wrong variant"),
        }
    }

    #[test]
    fn parse_orthogonal_missing_path_errors() {
        assert!(parse_arm_spec("orthogonal:axis=conviction").is_err());
    }

    #[test]
    fn parse_unknown_head_errors() {
        assert!(parse_arm_spec("bogus").is_err());
    }
}
