use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LooseningSchedule {
    pub day_n_thresholds: Vec<f64>,
}

fn default_dspy_pattern_cohort_threshold() -> usize {
    5
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoOptimizerConfig {
    pub min_improvement: f64,
    pub baseline_untouched_window: BaselineUntouchedWindow,
    pub day_window: DayWindow,
    #[serde(default)]
    pub loosening_schedule: Option<LooseningSchedule>,
    pub mutator: MutatorConfig,
    #[serde(default = "default_allowed_mutation_kinds")]
    pub allowed_mutation_kinds: Vec<String>,
    #[serde(default)]
    pub lineage_root: Option<PathBuf>,
    /// Enable DSPy flywheel: write judge findings as Observations and
    /// compile compiled DSRs into Patterns after each optimizer cycle.
    #[serde(default)]
    pub dspy_enabled: bool,
    /// Minimum number of Observations in the namespace before a DSPy
    /// compilation pass is triggered. Default 5.
    #[serde(default = "default_dspy_pattern_cohort_threshold")]
    pub dspy_pattern_cohort_threshold: usize,
    /// When true, each mutation proposal runs through the three-candidate
    /// Borda-count tournament instead of a single `mutator.propose()` call.
    /// Defaults to false; set in autooptimizer.toml to opt in.
    #[serde(default)]
    pub tournament_enabled: bool,
    /// F24: the metric the mutation cycle optimizes (gate objective). Defaults to
    /// Sharpe; operators can select `total_return`, `max_drawdown`, or `win_rate`
    /// via autooptimizer.toml or the CLI `--objective` flag.
    #[serde(default)]
    pub objective: crate::autooptimizer::gate::Objective,

    /// Optional regime windows for the regime-matrix optimizer feature.
    /// Defaults to empty (back-compat: existing configs without this key are unchanged).
    #[serde(default)]
    pub regime_set: Vec<RegimeWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineUntouchedWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutatorConfig {
    pub provider: String,
    pub model: String,
    pub max_retries: u32,
}

fn default_allowed_mutation_kinds() -> Vec<String> {
    // "filter" is enabled by default (Phase 2). Existing autooptimizer.toml
    // files that pin the `allowed_mutation_kinds` list keep their pin; only
    // configs that rely on the #[serde(default)] path pick up "filter" here.
    vec!["prose".into(), "param".into(), "tool".into(), "filter".into()]
}

/// Date range expressed as ISO-8601 strings (YYYY-MM-DD).
/// Used inside `RegimeWindow` so that regime windows do not depend on
/// the `NaiveDate`-backed `DayWindow` / `BaselineUntouchedWindow` types.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioWindow {
    pub start: String,
    pub end: String,
}

/// Which directional regime a `RegimeWindow` represents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegimeSide {
    Bull,
    BearOrShock,
    Chop,
}

/// One labeled regime window used by the Optimizer regime-matrix feature.
/// `day` is the training / candidate-evaluation range; `baseline` is the
/// held-out comparison range for that regime.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeWindow {
    pub label: String,
    pub side: RegimeSide,
    pub day: ScenarioWindow,
    pub baseline: ScenarioWindow,
}

impl Default for AutoOptimizerConfig {
    fn default() -> Self {
        Self {
            min_improvement: 0.05,
            // F3 (QA 2026-06-04): the previous default spanned ~20 months of
            // 1h bars (day 2024-01→2025-09) plus a 3-month held-out window,
            // so a no-config `run-cycle` silently fetched ~16k bars per
            // candidate. Default to a compact, recent, contiguous span
            // (3-month day window + 1-month held-out baseline) that keeps the
            // train-before-holdout ordering; operators who want the larger
            // window set it in autooptimizer.toml or via the --day-*/
            // --baseline-* flags.
            day_window: DayWindow {
                start: NaiveDate::from_ymd_opt(2025, 1, 1).expect("valid date"),
                end: NaiveDate::from_ymd_opt(2025, 4, 1).expect("valid date"),
            },
            baseline_untouched_window: BaselineUntouchedWindow {
                start: NaiveDate::from_ymd_opt(2025, 4, 1).expect("valid date"),
                end: NaiveDate::from_ymd_opt(2025, 5, 1).expect("valid date"),
            },
            loosening_schedule: None,
            mutator: MutatorConfig {
                provider: "test".into(),
                model: "test-model".into(),
                max_retries: 2,
            },
            allowed_mutation_kinds: default_allowed_mutation_kinds(),
            lineage_root: None,
            dspy_enabled: false,
            dspy_pattern_cohort_threshold: default_dspy_pattern_cohort_threshold(),
            tournament_enabled: false,
            objective: crate::autooptimizer::gate::Objective::default(),
            regime_set: vec![],
        }
    }
}

impl AutoOptimizerConfig {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading autooptimizer config at {}", path.display()))?;
        toml::from_str(&raw).with_context(|| format!("parsing autooptimizer config at {}", path.display()))
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        Self::from_path(path)
    }

    pub fn default_path() -> anyhow::Result<PathBuf> {
        // Honor `$XVN_HOME` first (same precedence as the CLI's
        // `resolve_xvn_home`: explicit override → `$XVN_HOME` → `$HOME/.xvn`).
        // The CLI layer already overrides this with the resolved home (the T1
        // fix), but a direct caller of `default_path()` previously got the
        // stale `~/.xvn` regardless of `$XVN_HOME` — a latent path landmine
        // (QA 2026-06-04, finding F7).
        if let Ok(home) = std::env::var("XVN_HOME") {
            if !home.is_empty() {
                return Ok(PathBuf::from(home).join("autooptimizer.toml"));
            }
        }
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        Ok(home.join(".xvn").join("autooptimizer.toml"))
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.min_improvement <= 0.0 {
            bail!(
                "min_improvement must be greater than 0 (got {})",
                self.min_improvement
            );
        }
        if self.baseline_untouched_window.start >= self.baseline_untouched_window.end {
            bail!(
                "baseline_untouched_window start ({}) must be before end ({})",
                self.baseline_untouched_window.start,
                self.baseline_untouched_window.end,
            );
        }
        if self.day_window.start >= self.day_window.end {
            bail!(
                "day_window start ({}) must be before end ({})",
                self.day_window.start,
                self.day_window.end,
            );
        }
        if self.mutator.max_retries > 10 {
            bail!(
                "mutator max_retries must be <= 10 (got {})",
                self.mutator.max_retries,
            );
        }
        if self.mutator.model.is_empty() {
            bail!("mutator model must not be empty");
        }
        if self.mutator.provider.is_empty() {
            bail!("mutator provider must not be empty");
        }
        if let Some(schedule) = &self.loosening_schedule {
            for threshold in &schedule.day_n_thresholds {
                if *threshold <= 0.0 {
                    bail!(
                        "loosening_schedule thresholds must be greater than 0 (got {})",
                        threshold
                    );
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_allowed_mutation_kinds_includes_filter() {
        let defaults = default_allowed_mutation_kinds();
        assert!(
            defaults.contains(&"filter".to_string()),
            "default allowed_mutation_kinds must include \"filter\"; got: {defaults:?}"
        );
        // Existing defaults must still be present.
        assert!(defaults.contains(&"prose".to_string()), "prose missing from defaults");
        assert!(defaults.contains(&"param".to_string()), "param missing from defaults");
        assert!(defaults.contains(&"tool".to_string()), "tool missing from defaults");
    }

    #[test]
    fn autooptimizer_config_default_includes_filter_kind() {
        let config = AutoOptimizerConfig::default();
        assert!(
            config.allowed_mutation_kinds.contains(&"filter".to_string()),
            "AutoOptimizerConfig::default must include \"filter\" in allowed_mutation_kinds"
        );
    }

    #[test]
    fn regime_set_defaults_empty_and_parses_toml() {
        // AutoOptimizerConfig has required fields (min_improvement, day_window,
        // baseline_untouched_window, mutator) with no serde defaults, so
        // toml::from_str("") would fail. Use Default::default() to verify
        // regime_set starts empty, then parse a full-config TOML with one entry.
        let cfg = AutoOptimizerConfig::default();
        assert!(cfg.regime_set.is_empty(), "regime_set must default empty (back-compat)");

        let cfg2: AutoOptimizerConfig = toml::from_str(r#"
            min_improvement = 0.05

            [day_window]
            start = "2025-01-01"
            end   = "2025-04-01"

            [baseline_untouched_window]
            start = "2025-04-01"
            end   = "2025-05-01"

            [mutator]
            provider   = "test"
            model      = "test-model"
            max_retries = 2

            [[regime_set]]
            label    = "bull"
            side     = "bull"
            [regime_set.day]
            start = "2024-01-01"
            end   = "2024-03-01"
            [regime_set.baseline]
            start = "2024-03-01"
            end   = "2024-04-01"
        "#).unwrap();
        assert_eq!(cfg2.regime_set.len(), 1);
        assert_eq!(cfg2.regime_set[0].label, "bull");
        assert!(matches!(cfg2.regime_set[0].side, RegimeSide::Bull));
    }
}
