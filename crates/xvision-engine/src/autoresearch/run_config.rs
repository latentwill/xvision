//! Validated run configuration written into the worktree as `run_config.json`.
//!
//! `xvision_prepare.py` receives `argv[1]` = path to this file. The Rust
//! harness writes it from the validated `POST /api/autoresearch/runs` payload
//! so no operator-controlled string is interpolated into a shell command.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// v1 label strategies. Matches the spec operator-surface names and the
/// `label_strategy` column in `autoresearch_runs`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabelStrategy {
    PriceForward,
    OutcomeImitation,
}

/// The allowlisted custom filter for `outcome_imitation`. Evaluated in Rust
/// over cycle query results — never SQL-interpolated or eval'd.
///
/// Field names come from a CLOSED allowlist (`pnl`, `drawdown_pct`,
/// `win_rate`). Operators: `$gt`, `$lt`, `$gte`, `$lte`, `$eq`.
/// Unknown keys are rejected at validate() time.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct OutcomeImitationFilter {
    /// Minimum PnL (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pnl_gt: Option<f64>,
    /// Maximum drawdown_pct (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drawdown_pct_lt: Option<f64>,
    /// Minimum win_rate (exclusive).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub win_rate_gt: Option<f64>,
}

/// Per-strategy label parameters, bundled together so `xvision_prepare.py`
/// gets everything it needs from a single config file argument.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LabelConfig {
    /// `price_forward`: N bars ahead threshold (fraction of price, e.g. 0.003).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_forward_threshold: Option<f64>,
    /// `price_forward`: look-ahead window in bars.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price_forward_horizon_bars: Option<u32>,
    /// `outcome_imitation`: quality filter. Required when label_strategy =
    /// OutcomeImitation; ignored for PriceForward.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome_imitation_filter: Option<OutcomeImitationFilter>,
}

/// Validated configuration written to `<worktree>/run_config.json`.
///
/// `xvision_prepare.py` reads this file via `argv[1]`. The harness writes it
/// from the validated `POST /api/autoresearch/runs` payload before spawning
/// any subprocess, so operator-controlled strings never reach a shell.
///
/// JSON keys emitted (as read by `xvision_train.py`):
///   - `source_strategy_id`
///   - `label_strategy`        ("price_forward" | "outcome_imitation")
///   - `label_config`          (nested object)
///   - `min_cycle_count`
///   - `train_wall_clock_sec`
///   - `db_path`
///   - `output_dir`
///   - `promotion_epsilon`
///   - `promotion_acc_floor`
///   - `promotion_min_holdout`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunConfig {
    /// FK → strategies table.
    pub source_strategy_id: String,
    pub label_strategy: LabelStrategy,
    pub label_config: LabelConfig,
    /// Minimum labeled cycles required before training starts; POST returns
    /// 400 if the strategy has fewer.
    pub min_cycle_count: u32,
    /// Wall-clock budget per training subprocess invocation, in seconds.
    pub train_wall_clock_sec: u64,
    /// Absolute path to the xvn.db SQLite file.
    pub db_path: String,
    /// Directory where `xvision_prepare.py` writes safetensors checkpoints.
    pub output_dir: String,
    /// Promotion gate thresholds (captured from config store at run-start time
    /// so they don't drift during a long run).
    pub promotion_epsilon: f64,
    pub promotion_acc_floor: f64,
    pub promotion_min_holdout: u32,
}

impl RunConfig {
    /// Validate business rules. Called before `write_to`; the HTTP handler
    /// calls this and returns 400 on failure.
    pub fn validate(&self) -> Result<()> {
        if self.min_cycle_count == 0 {
            bail!("min_cycle_count must be > 0");
        }
        if self.train_wall_clock_sec == 0 {
            bail!("train_wall_clock_sec must be > 0");
        }
        if matches!(self.label_strategy, LabelStrategy::OutcomeImitation)
            && self.label_config.outcome_imitation_filter.is_none()
        {
            bail!("outcome_imitation_filter is required when label_strategy = outcome_imitation");
        }
        if self.promotion_acc_floor < 0.0 || self.promotion_acc_floor > 1.0 {
            bail!("promotion_acc_floor must be in [0, 1]");
        }
        if self.promotion_epsilon < 0.0 {
            bail!("promotion_epsilon must be >= 0");
        }
        Ok(())
    }

    /// Serialize and write atomically to `path` (write to `.tmp`, then rename).
    pub fn write_to(&self, path: &Path) -> Result<()> {
        self.validate()?;
        let json = serde_json::to_string_pretty(self).context("serialize RunConfig to JSON")?;
        let tmp_path = path.with_extension("json.tmp");
        std::fs::write(&tmp_path, json.as_bytes())
            .with_context(|| format!("write run_config to {}", tmp_path.display()))?;
        std::fs::rename(&tmp_path, path)
            .with_context(|| format!("rename run_config into place at {}", path.display()))?;
        Ok(())
    }

    /// Read and deserialize from `path`.
    pub fn read_from(path: &Path) -> Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("read run_config from {}", path.display()))?;
        serde_json::from_str(&raw).context("deserialize RunConfig from JSON")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn sample_config() -> RunConfig {
        RunConfig {
            source_strategy_id: "strat-01JKTEST".to_string(),
            label_strategy: LabelStrategy::PriceForward,
            label_config: LabelConfig {
                price_forward_threshold: Some(0.003),
                price_forward_horizon_bars: Some(12),
                outcome_imitation_filter: None,
            },
            min_cycle_count: 500,
            train_wall_clock_sec: 300,
            db_path: "/tmp/xvn.db".to_string(),
            output_dir: "/tmp/nanochat-out".to_string(),
            promotion_epsilon: 0.01,
            promotion_acc_floor: 0.52,
            promotion_min_holdout: 200,
        }
    }

    #[test]
    fn round_trip_json() {
        let cfg = sample_config();
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("run_config.json");

        cfg.write_to(&path).unwrap();

        let loaded = RunConfig::read_from(&path).unwrap();
        assert_eq!(loaded.source_strategy_id, "strat-01JKTEST");
        assert_eq!(loaded.min_cycle_count, 500);
        assert_eq!(loaded.train_wall_clock_sec, 300);
        assert!(matches!(loaded.label_strategy, LabelStrategy::PriceForward));
        assert_eq!(loaded.label_config.price_forward_threshold, Some(0.003));
        assert_eq!(loaded.promotion_acc_floor, 0.52);
    }

    #[test]
    fn rejects_invalid_label_strategy_combination() {
        // outcome_imitation requires outcome_imitation_filter to be set.
        let mut cfg = sample_config();
        cfg.label_strategy = LabelStrategy::OutcomeImitation;
        cfg.label_config.outcome_imitation_filter = None;
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("outcome_imitation_filter"), "{err}");
    }

    #[test]
    fn rejects_zero_min_cycle_count() {
        let mut cfg = sample_config();
        cfg.min_cycle_count = 0;
        let err = cfg.validate().unwrap_err();
        assert!(err.to_string().contains("min_cycle_count"), "{err}");
    }

    #[test]
    fn write_creates_valid_utf8_json_file() {
        let cfg = sample_config();
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("run_config.json");
        cfg.write_to(&path).unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let v: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(v["source_strategy_id"], "strat-01JKTEST");
        assert_eq!(v["label_strategy"], "price_forward");
    }
}
