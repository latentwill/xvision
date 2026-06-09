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

/// Default candidate experiments per parent per cycle. Was a hard-coded `1`
/// (one experiment/cycle, nothing to compare); 5 gives the optimizer a real
/// candidate pool by default.
fn default_experiments_per_cycle() -> u32 {
    5
}

/// Trade-direction mode the optimizer's random baseline mirrors. A
/// "no-intelligence" baseline for a LONG-only strategy must randomly pick
/// between LONG and FLAT (never SHORT), otherwise it measures the wrong
/// counterfactual. `Both` (default) admits long+short+flat. Set per optimizer
/// run via autooptimizer.toml / the CLI; the optimizer agent chooses long,
/// short, or both. (Lives on the run config, not the Strategy, so existing
/// strategy JSON files are untouched.)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TradeDirection {
    Long,
    Short,
    #[default]
    Both,
}

impl TradeDirection {
    /// The `trader_output.action` values a no-intelligence random baseline may
    /// emit for this direction. Always includes `"flat"` (the no-position
    /// counterfactual).
    pub fn baseline_actions(&self) -> &'static [&'static str] {
        match self {
            TradeDirection::Long => &["long_open", "flat"],
            TradeDirection::Short => &["short_open", "flat"],
            TradeDirection::Both => &["long_open", "short_open", "flat"],
        }
    }
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
    /// Number of candidate experiments the optimizer generates per parent each
    /// cycle (`CycleConfig.mutations_per_parent`). Bumped from the old hard-coded
    /// `1` so a cycle gives the optimizer a real candidate pool to compare;
    /// operators can override per run via the CLI `--experiments-per-cycle` flag
    /// or the dashboard run form. Validated to `1..=64`. Back-compat: absent from
    /// existing autooptimizer.toml ⇒ the default.
    #[serde(default = "default_experiments_per_cycle")]
    pub experiments_per_cycle: u32,
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

    /// Trade-direction mode the random-baseline edge metric mirrors. The
    /// per-cycle `edge_over_random` / `parent_edge` / `edge_delta` numbers
    /// compare child/parent against a fixed-seed random agent that picks
    /// uniformly from this direction's action set. `Both` (default) =
    /// long+short+flat; `Long`/`Short` restrict it so a directional strategy is
    /// measured against the right counterfactual. Informational only — never
    /// gates promotion. Back-compat: absent from existing configs ⇒ `Both`.
    #[serde(default)]
    pub baseline_direction: TradeDirection,
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
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
            experiments_per_cycle: default_experiments_per_cycle(),
            lineage_root: None,
            dspy_enabled: false,
            dspy_pattern_cohort_threshold: default_dspy_pattern_cohort_threshold(),
            tournament_enabled: false,
            objective: crate::autooptimizer::gate::Objective::default(),
            regime_set: vec![],
            baseline_direction: TradeDirection::Both,
        }
    }
}

/// Validate a regime set for structural correctness:
///
/// 1. No two `RegimeWindow`s share the same `label` (the DB PK and parent-cache
///    key both key on label; duplicates silently overwrite).
/// 2. For each window, `day` and `baseline` date ranges must be disjoint
///    (overlapping ranges mix train and held-out data, invalidating the gate).
///
/// Returns `Ok(())` when the set is empty (back-compat: empty = legacy path).
pub fn validate_regime_set(regimes: &[RegimeWindow]) -> anyhow::Result<()> {
    // Check 1: duplicate labels.
    let mut seen = std::collections::HashSet::new();
    for rw in regimes {
        if !seen.insert(rw.label.as_str()) {
            bail!("duplicate regime label '{}' — labels must be unique", rw.label);
        }
    }

    // Check 2: day / baseline overlap per window.
    for rw in regimes {
        let day_start: NaiveDate = rw
            .day
            .start
            .parse()
            .with_context(|| format!("regime '{}': invalid day.start '{}'", rw.label, rw.day.start))?;
        let day_end: NaiveDate = rw
            .day
            .end
            .parse()
            .with_context(|| format!("regime '{}': invalid day.end '{}'", rw.label, rw.day.end))?;
        let base_start: NaiveDate = rw
            .baseline
            .start
            .parse()
            .with_context(|| format!("regime '{}': invalid baseline.start '{}'", rw.label, rw.baseline.start))?;
        let base_end: NaiveDate = rw
            .baseline
            .end
            .parse()
            .with_context(|| format!("regime '{}': invalid baseline.end '{}'", rw.label, rw.baseline.end))?;

        // Overlap when: day_start < base_end AND base_start < day_end
        let overlaps = day_start < base_end && base_start < day_end;
        if overlaps {
            bail!(
                "regime '{}': day window ({} – {}) overlaps with baseline ({} – {}); \
                 they must be disjoint to keep train and held-out data separate",
                rw.label, day_start, day_end, base_start, base_end,
            );
        }
    }

    Ok(())
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
        if self.experiments_per_cycle < 1 || self.experiments_per_cycle > 64 {
            bail!(
                "experiments_per_cycle must be between 1 and 64 (got {})",
                self.experiments_per_cycle,
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
        // Fix 3: validate regime_set so duplicate/overlapping windows are caught
        // at config-load time, before any cycle is launched.
        validate_regime_set(&self.regime_set)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_regime(label: &str, day_start: &str, day_end: &str, base_start: &str, base_end: &str) -> RegimeWindow {
        RegimeWindow {
            label: label.to_string(),
            side: RegimeSide::Bull,
            day: ScenarioWindow { start: day_start.to_string(), end: day_end.to_string() },
            baseline: ScenarioWindow { start: base_start.to_string(), end: base_end.to_string() },
        }
    }

    #[test]
    fn experiments_per_cycle_defaults_to_five() {
        // The old hard-coded behavior was 1 experiment/cycle; the default is now 5.
        assert_eq!(AutoOptimizerConfig::default().experiments_per_cycle, 5);
        assert_eq!(default_experiments_per_cycle(), 5);
    }

    #[test]
    fn validate_rejects_out_of_range_experiments_per_cycle() {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.experiments_per_cycle = 0;
        assert!(cfg.validate().is_err(), "0 experiments must be rejected");
        cfg.experiments_per_cycle = 65;
        assert!(cfg.validate().is_err(), "65 (>64) must be rejected");
        cfg.experiments_per_cycle = 5;
        assert!(cfg.validate().is_ok(), "5 is in range");
    }

    #[test]
    fn validate_regime_set_empty_is_ok() {
        assert!(validate_regime_set(&[]).is_ok());
    }

    #[test]
    fn validate_regime_set_unique_non_overlapping_is_ok() {
        let regimes = vec![
            make_regime("bull", "2024-01-01", "2024-03-01", "2024-03-01", "2024-04-01"),
            make_regime("bear", "2023-01-01", "2023-03-01", "2023-03-01", "2023-04-01"),
        ];
        assert!(validate_regime_set(&regimes).is_ok());
    }

    #[test]
    fn validate_regime_set_duplicate_label_is_err() {
        let regimes = vec![
            make_regime("bull", "2024-01-01", "2024-03-01", "2024-03-01", "2024-04-01"),
            make_regime("bull", "2023-01-01", "2023-03-01", "2023-03-01", "2023-04-01"),
        ];
        let err = validate_regime_set(&regimes).unwrap_err();
        assert!(err.to_string().contains("duplicate regime label 'bull'"), "got: {err}");
    }

    #[test]
    fn validate_regime_set_overlap_is_err() {
        // day 2024-01 to 2024-04, baseline 2024-03 to 2024-05 → overlaps in March
        let regimes = vec![
            make_regime("bull", "2024-01-01", "2024-04-01", "2024-03-01", "2024-05-01"),
        ];
        let err = validate_regime_set(&regimes).unwrap_err();
        assert!(err.to_string().contains("overlaps"), "got: {err}");
    }

    #[test]
    fn validate_regime_set_adjacent_windows_are_ok() {
        // day ends exactly where baseline starts — no overlap (open interval semantics)
        let regimes = vec![
            make_regime("bull", "2024-01-01", "2024-03-01", "2024-03-01", "2024-04-01"),
        ];
        assert!(validate_regime_set(&regimes).is_ok());
    }

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
