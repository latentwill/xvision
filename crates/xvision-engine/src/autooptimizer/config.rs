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
    /// compile compiled DSRs into Patterns after each evening cycle.
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
    vec!["prose".into(), "param".into(), "tool".into()]
}

impl Default for AutoOptimizerConfig {
    fn default() -> Self {
        Self {
            min_improvement: 0.05,
            baseline_untouched_window: BaselineUntouchedWindow {
                start: NaiveDate::from_ymd_opt(2025, 9, 1).expect("valid date"),
                end: NaiveDate::from_ymd_opt(2025, 12, 1).expect("valid date"),
            },
            day_window: DayWindow {
                start: NaiveDate::from_ymd_opt(2024, 1, 1).expect("valid date"),
                end: NaiveDate::from_ymd_opt(2025, 9, 1).expect("valid date"),
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
        let home = dirs::home_dir().ok_or_else(|| anyhow::anyhow!("cannot determine home directory"))?;
        Ok(home.join(".xvn/autooptimizer.toml"))
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
