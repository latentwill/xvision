use std::path::{Path, PathBuf};

use anyhow::{bail, Context};
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// Maximum allowed span (in days) for any single evaluation window
/// (day_window, baseline_untouched_window, or a regime's day window).
///
/// B22: bars for the whole window are loaded fully into memory per asset per
/// candidate during a cycle's backtest (see `eval_adapter.rs`). A multi-month
/// span (e.g. ~20 months of 1h bars) blows the container's memory budget and
/// OOMs *after* the cycle lock is acquired, stranding the lock. Capping the
/// span at config-validation time — before the lock and before any bars load —
/// keeps the cycle from ever entering that trap.
pub const MAX_WINDOW_DAYS: i64 = 120;
/// Maximum number of benchmark windows GEPA real-eval will accept in one
/// config. Each window can trigger a full benchmark per surviving candidate, so
/// this is intentionally conservative until production evaluator budgeting is
/// wired end-to-end.
pub const MAX_GEPA_BENCHMARK_POOL_SIZE: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LooseningSchedule {
    pub day_n_thresholds: Vec<f64>,
}

fn default_dspy_pattern_cohort_threshold() -> usize {
    5
}

fn default_gepa_candidates() -> usize {
    3
}

fn default_gepa_generations() -> usize {
    2
}

fn default_gepa_real_eval_min_llm_score() -> f64 {
    0.30
}

/// Default holdout min-improvement threshold: 0.005 (0.5%).
/// Small but strictly positive — a candidate must genuinely improve on
/// out-of-sample data, but the bar is lower than the in-sample `min_improvement`.
fn default_holdout_min_improvement() -> f64 {
    0.005
}

/// Default minimum realized-return ratio: 0.25 (25%).
fn default_min_realized_return_ratio() -> f64 {
    0.25
}

/// Default minimum parent-trade retention ratio: 0.5 (50%).
/// The child must execute at least `floor(parent.n_trades * 0.5)` fill legs,
/// with a hard floor of 1. Prevents 0-trade strategies from gaming Sharpe.
fn default_min_trade_retention_ratio() -> f64 {
    0.5
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
    /// Minimum improvement a candidate must beat the parent by on the holdout
    /// (baseline-untouched) window. Defaults to 0.005 (0.5%). Must be > 0.
    /// Separate from `min_improvement` so operators can require a smaller bar
    /// for out-of-sample generalization than for in-sample training improvement.
    #[serde(default = "default_holdout_min_improvement")]
    pub holdout_min_improvement: f64,
    /// Minimum fraction of total return that must be realized (booked) profit.
    /// 0.25 means at least 25% of the strategy's gross return must come from
    /// closed positions. Set to 0.0 to disable.
    #[serde(default = "default_min_realized_return_ratio")]
    pub min_realized_return_ratio: f64,
    /// Minimum fraction of parent trades a candidate must retain to pass the
    /// gate. 0.5 means the child must execute at least 50% of the parent's fill
    /// legs, with a hard floor of 1. Prevents 0-trade degenerate strategies
    /// from gaming the Sharpe metric. Range: (0.0, 1.0].
    #[serde(default = "default_min_trade_retention_ratio")]
    pub min_trade_retention_ratio: f64,
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

    /// B19: optional pool of (day, baseline-untouched) window pairs the cycle
    /// SAMPLES round-robin across candidates, so different candidates are
    /// evaluated on different regimes and a strategy tuned to a single fixed
    /// window can no longer dominate the whole cycle (overfitting guard).
    ///
    /// This is DISTINCT from `regime_set`: `regime_set` evaluates every regime
    /// for every candidate (exhaustive, gate requires improvement across all
    /// regimes); `scenario_pool` picks ONE pair per candidate via
    /// `pool[mutation_idx % pool.len()]` (sampling). Both can be empty.
    ///
    /// Empty (the default) ⇒ the cycle uses the single
    /// `day_window`/`baseline_untouched_window` pair for every candidate,
    /// exactly as before (100% back-compat). When non-empty, the single pair is
    /// the fallback only when the pool is empty; the pool drives sampling.
    #[serde(default)]
    pub scenario_pool: Vec<ScenarioWindowPair>,

    /// Trade-direction mode the random-baseline edge metric mirrors. The
    /// per-cycle `edge_over_random` / `parent_edge` / `edge_delta` numbers
    /// compare child/parent against a fixed-seed random agent that picks
    /// uniformly from this direction's action set. `Both` (default) =
    /// long+short+flat; `Long`/`Short` restrict it so a directional strategy is
    /// measured against the right counterfactual. Informational only — never
    /// gates promotion. Back-compat: absent from existing configs ⇒ `Both`.
    #[serde(default)]
    pub baseline_direction: TradeDirection,

    /// Number of candidate instructions GEPA generates per generation.
    /// Each candidate is proposed independently and scored. Default: 3.
    #[serde(default = "default_gepa_candidates")]
    pub gepa_candidates: usize,

    /// Number of reflection→proposal generations the GEPA loop runs.
    /// More generations improve quality at the cost of more LLM calls. Default: 2.
    #[serde(default = "default_gepa_generations")]
    pub gepa_generations: usize,

    /// Reserved opt-in for a GEPA scorer that uses benchmark backtests after the
    /// cheap LLM cull. Validation rejects `true` until a production benchmark
    /// evaluator is wired, so enabling this flag cannot fail mid-cycle.
    #[serde(default)]
    pub gepa_real_eval: bool,

    /// Minimum fast LLM score a candidate must achieve before the optimizer pays
    /// for real benchmark evaluation. Range: 0.0..=1.0.
    #[serde(default = "default_gepa_real_eval_min_llm_score")]
    pub gepa_real_eval_min_llm_score: f64,

    /// Fixed benchmark windows for the reserved GEPA real-eval scorer. The pool
    /// is structurally validated even while `gepa_real_eval=false` so invalid
    /// benchmark definitions fail during config preflight, not during scoring.
    #[serde(default)]
    pub gepa_benchmark_pool: Vec<GepaBenchmarkWindow>,

    /// B22 follow-up (xvision-71k): opt-in override of the [`MAX_WINDOW_DAYS`]
    /// evaluation-window cap for the primary `day_window` and
    /// `baseline_untouched_window`. Unset (the default) keeps the safe
    /// `MAX_WINDOW_DAYS` (120-day) cap that guards against the per-candidate
    /// bar-fetch OOM. Power users who have the memory headroom and explicitly
    /// accept the cost can RAISE the cap here (e.g. `max_window_days = 365`) to
    /// run longer windows. `regime_set` / `scenario_pool` windows are NOT
    /// affected and remain capped at `MAX_WINDOW_DAYS`. Must be >= 1 when set.
    #[serde(default)]
    pub max_window_days: Option<i64>,

    /// Per-cycle scenario rotation: pre-generate N market windows at session
    /// start and rotate through them — one per cycle — so each cycle evaluates
    /// on a different date range. On by default.
    #[serde(default)]
    pub scenario_rotation: ScenarioRotationConfig,
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

/// B19: one labeled (day, baseline-untouched) window pair in the round-robin
/// `scenario_pool`. Mirrors `RegimeWindow` but without a directional `side`
/// (the pool is sampled, not gated-across), and reuses the `NaiveDate`-backed
/// `DayWindow`/`BaselineUntouchedWindow` types so each pair synthesizes its
/// scenarios through the exact same builders as the top-level single pair.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioWindowPair {
    pub label: String,
    pub day: DayWindow,
    pub baseline: BaselineUntouchedWindow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GepaBenchmarkWindow {
    pub label: String,
    pub parent_strategy_id: String,
    pub day: DayWindow,
    pub baseline: BaselineUntouchedWindow,
}

// ── Scenario Rotation (per-cycle window diversity) ───────────────────────

/// Controls per-cycle scenario rotation: instead of using the same
/// `day_window`/`baseline_untouched_window` pair for every cycle in a session,
/// the optimizer pre-generates N market windows and rotates through them —
/// one per cycle — so each cycle trains and evaluates on a different date
/// range (bull, bear, chop, trend). Zero extra evals per cycle; just a
/// different pair of windows.
///
/// The spec lives at the linked design doc (latentwill/6079eee9).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioRotationConfig {
    /// Master switch. On by default so every session gets regime diversity out
    /// of the box. Existing configs without this section see the default:
    /// enabled, 14-day windows, 30-day stride, 10 windows.
    #[serde(default = "default_scenario_rotation_enabled")]
    pub enabled: bool,
    /// Span (days) of each day-window (in-sample training). Default 14.
    #[serde(default = "default_rotation_window_span")]
    pub day_window_span_days: i64,
    /// Span (days) of each untouched holdout window, immediately after the day
    /// window. Default 14.
    #[serde(default = "default_rotation_window_span")]
    pub untouched_window_span_days: i64,
    /// How many window pairs to pre-generate. Default 10 (one per cycle in a
    /// 10-cycle session).
    #[serde(default = "default_rotation_num_windows")]
    pub num_windows: usize,
    /// Optional explicit start of the date range to draw windows from. When
    /// unset, falls back to `AutoOptimizerConfig.day_window.start`. Format
    /// `"YYYY-MM-DD"`.
    pub date_range_start: Option<NaiveDate>,
    /// Optional explicit end of the date range. When unset, falls back to
    /// `AutoOptimizerConfig.baseline_untouched_window.end`. Format
    /// `"YYYY-MM-DD"`.
    pub date_range_end: Option<NaiveDate>,
    /// Stride (days) between successive window starts. A 30-day stride with
    /// 14-day spans gives windows that overlap by ~16 days — some overlap or
    /// gap is intentional for regime diversity. Default 30.
    #[serde(default = "default_rotation_stride_days")]
    pub stride_days: i64,
}

fn default_scenario_rotation_enabled() -> bool {
    true
}

fn default_rotation_window_span() -> i64 {
    14
}

fn default_rotation_num_windows() -> usize {
    10
}

fn default_rotation_stride_days() -> i64 {
    30
}

impl Default for ScenarioRotationConfig {
    fn default() -> Self {
        Self {
            enabled: default_scenario_rotation_enabled(),
            day_window_span_days: default_rotation_window_span(),
            untouched_window_span_days: default_rotation_window_span(),
            num_windows: default_rotation_num_windows(),
            date_range_start: None,
            date_range_end: None,
            stride_days: default_rotation_stride_days(),
        }
    }
}

impl Default for AutoOptimizerConfig {
    fn default() -> Self {
        Self {
            min_improvement: 0.05,
            holdout_min_improvement: default_holdout_min_improvement(),
            min_trade_retention_ratio: default_min_trade_retention_ratio(),
            min_realized_return_ratio: default_min_realized_return_ratio(),
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
            scenario_pool: vec![],
            baseline_direction: TradeDirection::Both,
            gepa_candidates: default_gepa_candidates(),
            gepa_generations: default_gepa_generations(),
            gepa_real_eval: false,
            gepa_real_eval_min_llm_score: default_gepa_real_eval_min_llm_score(),
            gepa_benchmark_pool: vec![],
            max_window_days: None,
            scenario_rotation: ScenarioRotationConfig::default(),
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
        let base_start: NaiveDate = rw.baseline.start.parse().with_context(|| {
            format!(
                "regime '{}': invalid baseline.start '{}'",
                rw.label, rw.baseline.start
            )
        })?;
        let base_end: NaiveDate = rw.baseline.end.parse().with_context(|| {
            format!(
                "regime '{}': invalid baseline.end '{}'",
                rw.label, rw.baseline.end
            )
        })?;

        // B22: cap each regime's day-window span (same OOM trap as the
        // top-level day_window). Label is named so the operator knows which
        // regime to shrink.
        let day_span = (day_end - day_start).num_days();
        if day_span > MAX_WINDOW_DAYS {
            bail!(
                "regime '{}': day window span ({} days, {} – {}) exceeds the {}-day cap; \
                 shrink this regime's day window \
                 (a window this large loads too many bars per candidate and can OOM the cycle)",
                rw.label,
                day_span,
                day_start,
                day_end,
                MAX_WINDOW_DAYS,
            );
        }

        // Overlap when: day_start < base_end AND base_start < day_end
        let overlaps = day_start < base_end && base_start < day_end;
        if overlaps {
            bail!(
                "regime '{}': day window ({} – {}) overlaps with baseline ({} – {}); \
                 they must be disjoint to keep train and held-out data separate",
                rw.label,
                day_start,
                day_end,
                base_start,
                base_end,
            );
        }
    }

    Ok(())
}

fn validate_gepa_benchmark_pool(pool: &[GepaBenchmarkWindow], max_window_days: i64) -> anyhow::Result<()> {
    if pool.len() > MAX_GEPA_BENCHMARK_POOL_SIZE {
        bail!(
            "gepa_benchmark_pool contains {} benchmarks, exceeding the {} benchmark cap",
            pool.len(),
            MAX_GEPA_BENCHMARK_POOL_SIZE
        );
    }

    let mut labels = std::collections::HashSet::new();
    let mut semantic_keys = std::collections::HashSet::new();
    for item in pool {
        let label = item.label.trim();
        if label.is_empty() {
            bail!("gepa_benchmark_pool contains an entry with an empty label");
        }
        if !labels.insert(label.to_owned()) {
            bail!(
                "duplicate gepa_benchmark_pool label '{}' — labels must be unique",
                item.label
            );
        }
        if item.parent_strategy_id.trim().is_empty() {
            bail!("gepa_benchmark_pool '{}' must set parent_strategy_id", item.label);
        }
        if item.day.start >= item.day.end {
            bail!(
                "gepa_benchmark_pool '{}' day window start {} must be before end {}",
                item.label,
                item.day.start,
                item.day.end
            );
        }
        if item.baseline.start >= item.baseline.end {
            bail!(
                "gepa_benchmark_pool '{}' baseline window start {} must be before end {}",
                item.label,
                item.baseline.start,
                item.baseline.end
            );
        }

        let semantic_key = (
            item.parent_strategy_id.trim().to_owned(),
            item.day.start,
            item.day.end,
            item.baseline.start,
            item.baseline.end,
        );
        if !semantic_keys.insert(semantic_key) {
            bail!(
                "duplicate gepa_benchmark_pool benchmark '{}' — parent_strategy_id/day/baseline windows must be unique",
                item.label
            );
        }

        let day_span = (item.day.end - item.day.start).num_days();
        if day_span > max_window_days {
            bail!(
                "gepa_benchmark_pool '{}' day window spans {} days, exceeding the {} day cap",
                item.label,
                day_span,
                max_window_days
            );
        }
        let base_span = (item.baseline.end - item.baseline.start).num_days();
        if base_span > max_window_days {
            bail!(
                "gepa_benchmark_pool '{}' baseline window spans {} days, exceeding the {} day cap",
                item.label,
                base_span,
                max_window_days
            );
        }
        let overlaps = item.day.start < item.baseline.end && item.baseline.start < item.day.end;
        if overlaps {
            bail!(
                "gepa_benchmark_pool '{}' day window overlaps baseline window",
                item.label
            );
        }
    }
    Ok(())
}

/// B19: validate a `scenario_pool` for structural correctness, mirroring
/// `validate_regime_set`:
///
/// 1. Unique `label`s (labels appear in observability logs; duplicates make the
///    round-robin trace ambiguous).
/// 2. Each pair's `day` and `baseline` ranges are well-ordered and disjoint
///    (overlap would mix train and held-out data, invalidating the per-candidate
///    gate comparison).
/// 3. Each `day` window span is within `MAX_WINDOW_DAYS` (same OOM trap as the
///    top-level day window — the whole window's bars load per candidate).
///
/// Returns `Ok(())` when the pool is empty (back-compat: empty = single-pair path).
pub fn validate_scenario_pool(pool: &[ScenarioWindowPair]) -> anyhow::Result<()> {
    let mut seen = std::collections::HashSet::new();
    for p in pool {
        if !seen.insert(p.label.as_str()) {
            bail!(
                "duplicate scenario_pool label '{}' — labels must be unique",
                p.label
            );
        }
    }

    for p in pool {
        if p.day.start >= p.day.end {
            bail!(
                "scenario_pool '{}': day window start ({}) must be before end ({})",
                p.label,
                p.day.start,
                p.day.end,
            );
        }
        if p.baseline.start >= p.baseline.end {
            bail!(
                "scenario_pool '{}': baseline window start ({}) must be before end ({})",
                p.label,
                p.baseline.start,
                p.baseline.end,
            );
        }

        let day_span = (p.day.end - p.day.start).num_days();
        if day_span > MAX_WINDOW_DAYS {
            bail!(
                "scenario_pool '{}': day window span ({} days, {} – {}) exceeds the {}-day cap; \
                 shrink this pair's day window \
                 (a window this large loads too many bars per candidate and can OOM the cycle)",
                p.label,
                day_span,
                p.day.start,
                p.day.end,
                MAX_WINDOW_DAYS,
            );
        }
        let baseline_span = (p.baseline.end - p.baseline.start).num_days();
        if baseline_span > MAX_WINDOW_DAYS {
            bail!(
                "scenario_pool '{}': baseline window span ({} days, {} – {}) exceeds the {}-day cap; \
                 shrink this pair's baseline window",
                p.label,
                baseline_span,
                p.baseline.start,
                p.baseline.end,
                MAX_WINDOW_DAYS,
            );
        }

        // Overlap when: day_start < base_end AND base_start < day_end.
        let overlaps = p.day.start < p.baseline.end && p.baseline.start < p.day.end;
        if overlaps {
            bail!(
                "scenario_pool '{}': day window ({} – {}) overlaps with baseline ({} – {}); \
                 they must be disjoint to keep train and held-out data separate",
                p.label,
                p.day.start,
                p.day.end,
                p.baseline.start,
                p.baseline.end,
            );
        }
    }

    Ok(())
}

/// Validate the scenario rotation config for structural correctness.
/// When `enabled = false` (or the config is default/absent), this is a no-op.
///
/// Checks:
/// 1. `day_window_span_days` and `untouched_window_span_days` must be positive
///    and ≤ `MAX_WINDOW_DAYS`.
/// 2. `num_windows` must be ≥ 1 when enabled.
/// 3. `stride_days` must be ≥ 1.
/// 4. When both `date_range_start` and `date_range_end` are set, they must be
///    well-ordered (start < end).
/// 5. When projection is possible (we have the fallback day_window), each
///    generated window's day span is ≤ `MAX_WINDOW_DAYS` (same OOM guard).
pub fn validate_scenario_rotation(
    rotation: &ScenarioRotationConfig,
    fallback_day: Option<&DayWindow>,
    _fallback_baseline: Option<&BaselineUntouchedWindow>,
) -> anyhow::Result<()> {
    if !rotation.enabled {
        return Ok(());
    }
    if rotation.day_window_span_days <= 0 {
        anyhow::bail!(
            "scenario_rotation.day_window_span_days must be > 0 (got {})",
            rotation.day_window_span_days
        );
    }
    if rotation.untouched_window_span_days <= 0 {
        anyhow::bail!(
            "scenario_rotation.untouched_window_span_days must be > 0 (got {})",
            rotation.untouched_window_span_days
        );
    }
    let max_span = MAX_WINDOW_DAYS;
    if rotation.day_window_span_days > max_span {
        anyhow::bail!(
            "scenario_rotation.day_window_span_days ({}) exceeds the {}-day cap; \
             shrink it or raise max_window_days",
            rotation.day_window_span_days,
            max_span,
        );
    }
    if rotation.untouched_window_span_days > max_span {
        anyhow::bail!(
            "scenario_rotation.untouched_window_span_days ({}) exceeds the {}-day cap; \
             shrink it or raise max_window_days",
            rotation.untouched_window_span_days,
            max_span,
        );
    }
    if rotation.num_windows < 1 {
        anyhow::bail!(
            "scenario_rotation.num_windows must be >= 1 (got {})",
            rotation.num_windows
        );
    }
    if rotation.stride_days <= 0 {
        anyhow::bail!(
            "scenario_rotation.stride_days must be > 0 (got {})",
            rotation.stride_days
        );
    }
    // When both explicit dates are set, they must be well-ordered.
    if let (Some(start), Some(end)) = (rotation.date_range_start, rotation.date_range_end) {
        if start >= end {
            anyhow::bail!(
                "scenario_rotation.date_range_start ({}) must be before date_range_end ({})",
                start,
                end,
            );
        }
    }
    // Project the max day-window span to check it won't exceed the cap.
    if let Some(day_window) = fallback_day {
        let range_start = rotation.date_range_start.unwrap_or(day_window.start);
        let range_end = rotation
            .date_range_end
            .unwrap_or(_fallback_baseline.map_or(day_window.end, |b| b.end));
        if range_start < range_end {
            // The last window starts at: range_start + (num_windows - 1) * stride_days
            let last_day_start = range_start
                + chrono::Duration::days((rotation.num_windows as i64 - 1) * rotation.stride_days);
            let last_day_end = last_day_start + chrono::Duration::days(rotation.day_window_span_days);
            // Clamp to range_end — if clamped empty, it's degenerate but won't
            // OOM (it just produces no scenarios). Only warn about a real overrun.
            let actual_end = last_day_end.min(range_end);
            let actual_span = (actual_end - last_day_start).num_days();
            if actual_span > max_span {
                anyhow::bail!(
                    "scenario_rotation: the last projected day-window span ({} days, {} – {}) \
                     exceeds the {}-day cap; reduce day_window_span_days, num_windows, \
                     or stride_days",
                    actual_span,
                    last_day_start,
                    actual_end,
                    max_span,
                );
            }
        }
    }
    Ok(())
}

impl AutoOptimizerConfig {
    pub fn from_path(path: &Path) -> anyhow::Result<Self> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("reading autooptimizer config at {}", path.display()))?;
        // U1/U15: embed the toml deserialization error text (field path +
        // line/col) DIRECTLY in the returned error message. `with_context`
        // alone would only surface in the error *chain*, which the CLI prints
        // as `{e}` (outermost frame only) — so the operator saw
        // "parsing autooptimizer config at <path>" with no field-level signal.
        // Inlining `{e}` here guarantees the offending field is visible no
        // matter how the caller formats the error.
        toml::from_str(&raw)
            .map_err(|e| anyhow::anyhow!("parsing autooptimizer config at {}: {e}", path.display()))
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

    /// The effective evaluation-window cap (days): the operator's
    /// `max_window_days` opt-in when set, else the safe [`MAX_WINDOW_DAYS`]
    /// default (xvision-71k). Applies to `day_window` /
    /// `baseline_untouched_window` only.
    pub fn effective_max_window_days(&self) -> i64 {
        self.max_window_days.unwrap_or(MAX_WINDOW_DAYS)
    }

    pub fn validate(&self) -> anyhow::Result<()> {
        if self.min_improvement <= 0.0 {
            bail!(
                "min_improvement must be greater than 0 (got {})",
                self.min_improvement
            );
        }
        if self.holdout_min_improvement <= 0.0 {
            bail!(
                "holdout_min_improvement must be greater than 0 (got {})",
                self.holdout_min_improvement
            );
        }
        if self.min_trade_retention_ratio <= 0.0 || self.min_trade_retention_ratio > 1.0 {
            bail!(
                "min_trade_retention_ratio must be in (0.0, 1.0] (got {})",
                self.min_trade_retention_ratio
            );
        }
        if self.min_realized_return_ratio < 0.0 || self.min_realized_return_ratio > 1.0 {
            bail!(
                "min_realized_return_ratio must be in [0.0, 1.0] (got {})",
                self.min_realized_return_ratio
            );
        }
        if let Some(cap) = self.max_window_days {
            if cap < 1 {
                bail!(
                    "max_window_days must be >= 1 when set (got {}); omit it to use the \
                     default {}-day cap",
                    cap,
                    MAX_WINDOW_DAYS,
                );
            }
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
        // B22: cap each evaluation window's span so a multi-month window cannot
        // load enough bars per candidate to OOM the container after the cycle
        // lock is taken. Applied here (right after the ordering checks, before
        // the lock/bars) so both the CLI and dashboard entrypoints get it for
        // free via their existing cfg.validate() call. xvision-71k: the cap is
        // the operator's opt-in `max_window_days` when set, else the safe
        // default — so a >120-day window now fails with an actionable message
        // pointing at the override instead of being a silent breaking change.
        let max_window_days = self.effective_max_window_days();
        let raise_hint = if self.max_window_days.is_none() {
            " (or raise the cap with `max_window_days` in autooptimizer.toml if you \
             have the memory headroom)"
        } else {
            ""
        };
        let baseline_span =
            (self.baseline_untouched_window.end - self.baseline_untouched_window.start).num_days();
        if baseline_span > max_window_days {
            bail!(
                "baseline_untouched_window span ({} days, {} – {}) exceeds the {}-day cap; \
                 shrink it via --baseline-start/--baseline-end or autooptimizer.toml{} \
                 (a window this large loads too many bars per candidate and can OOM the cycle)",
                baseline_span,
                self.baseline_untouched_window.start,
                self.baseline_untouched_window.end,
                max_window_days,
                raise_hint,
            );
        }
        let day_span = (self.day_window.end - self.day_window.start).num_days();
        if day_span > max_window_days {
            bail!(
                "day_window span ({} days, {} – {}) exceeds the {}-day cap; \
                 shrink it via --day-start/--day-end or autooptimizer.toml{} \
                 (a window this large loads too many bars per candidate and can OOM the cycle)",
                day_span,
                self.day_window.start,
                self.day_window.end,
                max_window_days,
                raise_hint,
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
        if !(0.0..=1.0).contains(&self.gepa_real_eval_min_llm_score) {
            bail!(
                "gepa_real_eval_min_llm_score must be between 0.0 and 1.0 inclusive; got {}",
                self.gepa_real_eval_min_llm_score
            );
        }
        if self.gepa_real_eval {
            bail!(
                "gepa_real_eval=true is not available until a production benchmark evaluator is wired; \
                 set gepa_real_eval=false to use the LLM scorer"
            );
        }
        validate_gepa_benchmark_pool(&self.gepa_benchmark_pool, self.effective_max_window_days())?;
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
        // B19: same structural validation for the round-robin scenario_pool.
        validate_scenario_pool(&self.scenario_pool)?;
        // Scenario rotation validation.
        validate_scenario_rotation(
            &self.scenario_rotation,
            Some(&self.day_window),
            Some(&self.baseline_untouched_window),
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_regime(
        label: &str,
        day_start: &str,
        day_end: &str,
        base_start: &str,
        base_end: &str,
    ) -> RegimeWindow {
        RegimeWindow {
            label: label.to_string(),
            side: RegimeSide::Bull,
            day: ScenarioWindow {
                start: day_start.to_string(),
                end: day_end.to_string(),
            },
            baseline: ScenarioWindow {
                start: base_start.to_string(),
                end: base_end.to_string(),
            },
        }
    }

    fn gepa_benchmark(label: &str) -> GepaBenchmarkWindow {
        GepaBenchmarkWindow {
            label: label.into(),
            parent_strategy_id: "parent-a".into(),
            day: DayWindow {
                start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            },
            baseline: BaselineUntouchedWindow {
                start: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 2, 1).unwrap(),
            },
        }
    }

    fn validate_gepa_benchmark_message(benchmark: GepaBenchmarkWindow) -> String {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.gepa_benchmark_pool = vec![benchmark];
        cfg.validate().unwrap_err().to_string()
    }

    #[test]
    fn experiments_per_cycle_defaults_to_five() {
        // The old hard-coded behavior was 1 experiment/cycle; the default is now 5.
        assert_eq!(AutoOptimizerConfig::default().experiments_per_cycle, 5);
        assert_eq!(default_experiments_per_cycle(), 5);
    }

    #[test]
    fn gepa_real_eval_defaults_to_disabled_with_empty_pool() {
        let cfg = AutoOptimizerConfig::default();
        assert!(!cfg.gepa_real_eval);
        assert_eq!(cfg.gepa_real_eval_min_llm_score, 0.30);
        assert!(cfg.gepa_benchmark_pool.is_empty());
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn gepa_real_eval_is_rejected_until_production_evaluator_exists() {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.gepa_real_eval = true;
        let err = cfg.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("gepa_real_eval=true")
                && msg.contains("production benchmark evaluator")
                && msg.contains("gepa_real_eval=false"),
            "real eval must fail validation before constructing an unusable scorer; got: {msg}"
        );
    }

    #[test]
    fn gepa_real_eval_rejects_fast_score_threshold_outside_unit_interval() {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.gepa_real_eval_min_llm_score = 1.1;
        let err = cfg.validate().unwrap_err();
        assert!(
            err.to_string().contains("gepa_real_eval_min_llm_score"),
            "threshold >1 must be rejected by name; got: {err}"
        );

        cfg.gepa_real_eval_min_llm_score = -0.01;
        let err = cfg.validate().unwrap_err();
        assert!(
            err.to_string().contains("gepa_real_eval_min_llm_score"),
            "threshold <0 must be rejected by name; got: {err}"
        );
    }

    #[test]
    fn gepa_benchmark_pool_rejects_overlong_day_window() {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.gepa_benchmark_pool = vec![GepaBenchmarkWindow {
            label: "too-wide".into(),
            parent_strategy_id: "parent-a".into(),
            day: DayWindow {
                start: NaiveDate::from_ymd_opt(2024, 1, 1).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            },
            baseline: BaselineUntouchedWindow {
                start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
                end: NaiveDate::from_ymd_opt(2025, 2, 1).unwrap(),
            },
        }];
        let err = cfg.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("gepa_benchmark_pool") && msg.contains("too-wide") && msg.contains("120"),
            "message must name benchmark pool, label, and day cap; got: {msg}"
        );
    }

    #[test]
    fn gepa_benchmark_pool_rejects_empty_or_duplicate_labels() {
        let mut blank = gepa_benchmark("blank");
        blank.label = "  ".into();
        let msg = validate_gepa_benchmark_message(blank);
        assert!(
            msg.contains("gepa_benchmark_pool") && msg.contains("empty label"),
            "empty label message should name field; got: {msg}"
        );

        let mut cfg = AutoOptimizerConfig::default();
        cfg.gepa_benchmark_pool = vec![gepa_benchmark("dup"), gepa_benchmark("dup")];
        let msg = cfg.validate().unwrap_err().to_string();
        assert!(
            msg.contains("gepa_benchmark_pool") && msg.contains("dup") && msg.contains("unique"),
            "duplicate label message should name field and label; got: {msg}"
        );
    }

    #[test]
    fn gepa_benchmark_pool_rejects_blank_parent_strategy_id() {
        let mut item = gepa_benchmark("missing-parent");
        item.parent_strategy_id = "  ".into();
        let msg = validate_gepa_benchmark_message(item);
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains("missing-parent")
                && msg.contains("parent_strategy_id"),
            "missing parent message should name field and label; got: {msg}"
        );
    }

    #[test]
    fn gepa_benchmark_pool_rejects_reversed_or_zero_length_windows() {
        let mut reversed_day = gepa_benchmark("reversed-day");
        reversed_day.day.start = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        reversed_day.day.end = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        let msg = validate_gepa_benchmark_message(reversed_day);
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains("reversed-day")
                && msg.contains("day window start"),
            "reversed day message should name field and label; got: {msg}"
        );

        let mut zero_day = gepa_benchmark("zero-day");
        zero_day.day.end = zero_day.day.start;
        let msg = validate_gepa_benchmark_message(zero_day);
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains("zero-day")
                && msg.contains("day window start"),
            "zero-length day message should name field and label; got: {msg}"
        );

        let mut reversed_baseline = gepa_benchmark("reversed-baseline");
        reversed_baseline.baseline.start = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        reversed_baseline.baseline.end = NaiveDate::from_ymd_opt(2025, 1, 15).unwrap();
        let msg = validate_gepa_benchmark_message(reversed_baseline);
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains("reversed-baseline")
                && msg.contains("baseline window start"),
            "reversed baseline message should name field and label; got: {msg}"
        );

        let mut zero_baseline = gepa_benchmark("zero-baseline");
        zero_baseline.baseline.end = zero_baseline.baseline.start;
        let msg = validate_gepa_benchmark_message(zero_baseline);
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains("zero-baseline")
                && msg.contains("baseline window start"),
            "zero-length baseline message should name field and label; got: {msg}"
        );
    }

    #[test]
    fn gepa_benchmark_pool_rejects_overlong_baseline_window_and_any_overlap_direction() {
        let mut overlong_baseline = gepa_benchmark("wide-baseline");
        overlong_baseline.day.start = NaiveDate::from_ymd_opt(2024, 1, 1).unwrap();
        overlong_baseline.day.end = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        overlong_baseline.baseline.start = NaiveDate::from_ymd_opt(2024, 2, 1).unwrap();
        overlong_baseline.baseline.end = NaiveDate::from_ymd_opt(2024, 7, 1).unwrap();
        let msg = validate_gepa_benchmark_message(overlong_baseline);
        assert!(
            msg.contains("gepa_benchmark_pool") && msg.contains("wide-baseline") && msg.contains("120"),
            "overlong baseline message should name field, label, and cap; got: {msg}"
        );

        let mut day_overlaps_baseline = gepa_benchmark("day-overlaps");
        day_overlaps_baseline.day.start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        day_overlaps_baseline.day.end = NaiveDate::from_ymd_opt(2025, 1, 20).unwrap();
        day_overlaps_baseline.baseline.start = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
        day_overlaps_baseline.baseline.end = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        let msg = validate_gepa_benchmark_message(day_overlaps_baseline);
        assert!(
            msg.contains("gepa_benchmark_pool") && msg.contains("day-overlaps") && msg.contains("overlaps"),
            "overlap message should name field and label; got: {msg}"
        );

        let mut baseline_contains_day = gepa_benchmark("baseline-overlaps");
        baseline_contains_day.day.start = NaiveDate::from_ymd_opt(2025, 1, 10).unwrap();
        baseline_contains_day.day.end = NaiveDate::from_ymd_opt(2025, 1, 20).unwrap();
        baseline_contains_day.baseline.start = NaiveDate::from_ymd_opt(2025, 1, 1).unwrap();
        baseline_contains_day.baseline.end = NaiveDate::from_ymd_opt(2025, 2, 1).unwrap();
        let msg = validate_gepa_benchmark_message(baseline_contains_day);
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains("baseline-overlaps")
                && msg.contains("overlaps"),
            "reverse overlap message should name field and label; got: {msg}"
        );
    }

    #[test]
    fn gepa_benchmark_pool_rejects_semantic_duplicates_and_excessive_count() {
        let mut duplicate = gepa_benchmark("duplicate-shape");
        duplicate.label = "same-window-different-label".into();
        let mut cfg = AutoOptimizerConfig::default();
        cfg.gepa_benchmark_pool = vec![gepa_benchmark("original"), duplicate];
        let msg = cfg.validate().unwrap_err().to_string();
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains("same-window-different-label")
                && msg.contains("parent_strategy_id/day/baseline"),
            "semantic duplicate message should name field and duplicate label; got: {msg}"
        );

        let mut cfg = AutoOptimizerConfig::default();
        cfg.gepa_benchmark_pool = (0..=MAX_GEPA_BENCHMARK_POOL_SIZE)
            .map(|i| {
                let mut item = gepa_benchmark(&format!("bench-{i}"));
                item.parent_strategy_id = format!("parent-{i}");
                item
            })
            .collect();
        let msg = cfg.validate().unwrap_err().to_string();
        assert!(
            msg.contains("gepa_benchmark_pool")
                && msg.contains(&(MAX_GEPA_BENCHMARK_POOL_SIZE + 1).to_string())
                && msg.contains(&MAX_GEPA_BENCHMARK_POOL_SIZE.to_string()),
            "pool count message should name field and cap; got: {msg}"
        );
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
        assert!(
            err.to_string().contains("duplicate regime label 'bull'"),
            "got: {err}"
        );
    }

    #[test]
    fn validate_regime_set_overlap_is_err() {
        // day 2024-01 to 2024-04, baseline 2024-03 to 2024-05 → overlaps in March
        let regimes = vec![make_regime(
            "bull",
            "2024-01-01",
            "2024-04-01",
            "2024-03-01",
            "2024-05-01",
        )];
        let err = validate_regime_set(&regimes).unwrap_err();
        assert!(err.to_string().contains("overlaps"), "got: {err}");
    }

    #[test]
    fn validate_regime_set_adjacent_windows_are_ok() {
        // day ends exactly where baseline starts — no overlap (open interval semantics)
        let regimes = vec![make_regime(
            "bull",
            "2024-01-01",
            "2024-03-01",
            "2024-03-01",
            "2024-04-01",
        )];
        assert!(validate_regime_set(&regimes).is_ok());
    }

    #[test]
    fn validate_rejects_overlong_day_window() {
        // B22: a ~20-month day window must be rejected before any cycle launches
        // (otherwise bars for the whole span load into memory per candidate and
        // OOM the container, stranding the lock).
        let mut cfg = AutoOptimizerConfig::default();
        cfg.day_window = DayWindow {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).expect("valid date"),
            end: NaiveDate::from_ymd_opt(2025, 9, 1).expect("valid date"),
        };
        let err = cfg.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("day_window") && msg.contains("120"),
            "message must name day_window and the cap; got: {msg}"
        );
        // xvision-71k: the default-cap rejection points the operator at the
        // opt-in override so the breaking change is discoverable.
        assert!(
            msg.contains("max_window_days"),
            "default-cap rejection must mention the max_window_days override; got: {msg}"
        );
    }

    // ── xvision-71k: opt-in max_window_days override ─────────────────────────

    #[test]
    fn validate_accepts_overlong_day_window_when_max_window_days_raised() {
        // A power user who raises the cap can run a longer day window.
        let mut cfg = AutoOptimizerConfig::default();
        cfg.day_window = DayWindow {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).expect("valid date"),
            end: NaiveDate::from_ymd_opt(2024, 9, 1).expect("valid date"), // ~244 days
        };
        cfg.baseline_untouched_window = BaselineUntouchedWindow {
            start: NaiveDate::from_ymd_opt(2024, 9, 1).expect("valid date"),
            end: NaiveDate::from_ymd_opt(2024, 10, 1).expect("valid date"),
        };
        cfg.max_window_days = Some(365);
        assert!(
            cfg.validate().is_ok(),
            "a 244-day window must pass when max_window_days is raised to 365"
        );
    }

    #[test]
    fn validate_still_rejects_when_window_exceeds_raised_cap() {
        // The override raises the cap but is still a cap: a window beyond it fails,
        // and the message names the raised value (not the default 120).
        let mut cfg = AutoOptimizerConfig::default();
        cfg.day_window = DayWindow {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).expect("valid date"),
            end: NaiveDate::from_ymd_opt(2025, 9, 1).expect("valid date"), // ~608 days
        };
        cfg.max_window_days = Some(200);
        let err = cfg.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("day_window") && msg.contains("200"),
            "message must name day_window and the raised 200-day cap; got: {msg}"
        );
        assert!(
            !msg.contains("max_window_days"),
            "when the cap is already raised, do not re-suggest the override; got: {msg}"
        );
    }

    #[test]
    fn validate_rejects_nonpositive_max_window_days() {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.max_window_days = Some(0);
        let err = cfg.validate().unwrap_err();
        assert!(
            err.to_string().contains("max_window_days must be >= 1"),
            "max_window_days=0 must be rejected; got: {err}"
        );
    }

    #[test]
    fn validate_accepts_three_month_day_window() {
        // ~90-day day window (the new default span) must stay Ok.
        let cfg = AutoOptimizerConfig::default();
        assert_eq!(
            (cfg.day_window.end - cfg.day_window.start).num_days(),
            90,
            "default day_window should span ~3 months"
        );
        assert!(cfg.validate().is_ok(), "a ~3-month day window must remain valid");
    }

    #[test]
    fn validate_rejects_overlong_baseline_window() {
        // B22: the held-out baseline window must also be span-capped.
        let mut cfg = AutoOptimizerConfig::default();
        cfg.baseline_untouched_window = BaselineUntouchedWindow {
            start: NaiveDate::from_ymd_opt(2024, 1, 1).expect("valid date"),
            end: NaiveDate::from_ymd_opt(2025, 9, 1).expect("valid date"),
        };
        let err = cfg.validate().unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("baseline_untouched_window") && msg.contains("120"),
            "message must name baseline_untouched_window and the cap; got: {msg}"
        );
    }

    #[test]
    fn validate_regime_set_rejects_overlong_window() {
        // B22: each regime's day window must be span-capped, with the label in
        // the message so the operator knows which regime to shrink.
        let regimes = vec![make_regime(
            "bull_long",
            "2024-01-01",
            "2025-09-01",
            "2025-09-01",
            "2025-10-01",
        )];
        let err = validate_regime_set(&regimes).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("bull_long") && msg.contains("120"),
            "message must name the regime label and the cap; got: {msg}"
        );
    }

    #[test]
    fn default_allowed_mutation_kinds_includes_filter() {
        let defaults = default_allowed_mutation_kinds();
        assert!(
            defaults.contains(&"filter".to_string()),
            "default allowed_mutation_kinds must include \"filter\"; got: {defaults:?}"
        );
        // Existing defaults must still be present.
        assert!(
            defaults.contains(&"prose".to_string()),
            "prose missing from defaults"
        );
        assert!(
            defaults.contains(&"param".to_string()),
            "param missing from defaults"
        );
        assert!(
            defaults.contains(&"tool".to_string()),
            "tool missing from defaults"
        );
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
        assert!(
            cfg.regime_set.is_empty(),
            "regime_set must default empty (back-compat)"
        );

        let cfg2: AutoOptimizerConfig = toml::from_str(
            r#"
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
        "#,
        )
        .unwrap();
        assert_eq!(cfg2.regime_set.len(), 1);
        assert_eq!(cfg2.regime_set[0].label, "bull");
        assert!(matches!(cfg2.regime_set[0].side, RegimeSide::Bull));
    }

    // ── B19: scenario_pool ────────────────────────────────────────────────

    #[test]
    fn scenario_pool_defaults_empty_and_parses_toml() {
        // Absence of the key ⇒ empty vec (back-compat: existing autooptimizer.toml
        // files keep the single-pair behavior).
        let cfg = AutoOptimizerConfig::default();
        assert!(
            cfg.scenario_pool.is_empty(),
            "scenario_pool must default empty (back-compat)"
        );

        // A config with two [[scenario_pool]] entries deserializes into a
        // Vec<ScenarioWindowPair>.
        let cfg2: AutoOptimizerConfig = toml::from_str(
            r#"
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

            [[scenario_pool]]
            label = "q1-2024"
            [scenario_pool.day]
            start = "2024-01-01"
            end   = "2024-03-01"
            [scenario_pool.baseline]
            start = "2024-03-01"
            end   = "2024-04-01"

            [[scenario_pool]]
            label = "q3-2024"
            [scenario_pool.day]
            start = "2024-07-01"
            end   = "2024-09-01"
            [scenario_pool.baseline]
            start = "2024-09-01"
            end   = "2024-10-01"
        "#,
        )
        .unwrap();
        assert_eq!(cfg2.scenario_pool.len(), 2);
        assert_eq!(cfg2.scenario_pool[0].label, "q1-2024");
        assert_eq!(cfg2.scenario_pool[1].label, "q3-2024");
        assert_eq!(
            cfg2.scenario_pool[0].day.start,
            NaiveDate::from_ymd_opt(2024, 1, 1).unwrap()
        );
        assert_eq!(
            cfg2.scenario_pool[1].baseline.end,
            NaiveDate::from_ymd_opt(2024, 10, 1).unwrap()
        );
        // The full config must also pass validation.
        assert!(cfg2.validate().is_ok(), "two disjoint pairs must validate");
    }

    fn make_pair(
        label: &str,
        day_start: &str,
        day_end: &str,
        base_start: &str,
        base_end: &str,
    ) -> ScenarioWindowPair {
        ScenarioWindowPair {
            label: label.to_string(),
            day: DayWindow {
                start: day_start.parse().unwrap(),
                end: day_end.parse().unwrap(),
            },
            baseline: BaselineUntouchedWindow {
                start: base_start.parse().unwrap(),
                end: base_end.parse().unwrap(),
            },
        }
    }

    #[test]
    fn validate_scenario_pool_empty_is_ok() {
        assert!(validate_scenario_pool(&[]).is_ok());
    }

    #[test]
    fn validate_scenario_pool_unique_disjoint_is_ok() {
        let pool = vec![
            make_pair("a", "2024-01-01", "2024-03-01", "2024-03-01", "2024-04-01"),
            make_pair("b", "2024-07-01", "2024-09-01", "2024-09-01", "2024-10-01"),
        ];
        assert!(validate_scenario_pool(&pool).is_ok());
    }

    #[test]
    fn validate_scenario_pool_duplicate_label_is_err() {
        let pool = vec![
            make_pair("dup", "2024-01-01", "2024-03-01", "2024-03-01", "2024-04-01"),
            make_pair("dup", "2024-07-01", "2024-09-01", "2024-09-01", "2024-10-01"),
        ];
        let err = validate_scenario_pool(&pool).unwrap_err();
        assert!(
            err.to_string().contains("duplicate scenario_pool label 'dup'"),
            "got: {err}"
        );
    }

    #[test]
    fn validate_scenario_pool_overlap_is_err() {
        // day 2024-01→2024-04 overlaps baseline 2024-03→2024-05 in March.
        let pool = vec![make_pair(
            "ovl",
            "2024-01-01",
            "2024-04-01",
            "2024-03-01",
            "2024-05-01",
        )];
        let err = validate_scenario_pool(&pool).unwrap_err();
        assert!(err.to_string().contains("overlaps"), "got: {err}");
    }

    #[test]
    fn validate_scenario_pool_overlong_day_window_is_err() {
        let pool = vec![make_pair(
            "toolong",
            "2024-01-01",
            "2025-09-01",
            "2025-09-01",
            "2025-10-01",
        )];
        let err = validate_scenario_pool(&pool).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("toolong") && msg.contains("120"),
            "message must name the pair label and the cap; got: {msg}"
        );
    }

    #[test]
    fn from_path_error_embeds_offending_field_name() {
        // U1/U15: a config with an unknown / mistyped field must surface the
        // toml field path (and line) DIRECTLY in the returned error message —
        // not only deep in the error chain — so an operator who runs
        // `xvn optimize run` and the CLI prints `{e}` still sees which field is
        // wrong. We assert on the embedded toml text, which names the field.
        use std::sync::atomic::{AtomicU64, Ordering};
        static SEQ: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "ar-config-test-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("autooptimizer.toml");
        // `day_window.start` is a date field; a table value is a type error,
        // and an unknown top-level key trips deny-unknown style messages — use
        // a clearly-wrong scalar type for a known field so toml names it.
        std::fs::write(
            &path,
            r#"
min_improvement = "not-a-number"

[day_window]
start = "2025-01-01"
end   = "2025-04-01"

[baseline_untouched_window]
start = "2025-04-01"
end   = "2025-05-01"

[mutator]
provider    = "test"
model       = "test-model"
max_retries = 2
"#,
        )
        .unwrap();

        let err = AutoOptimizerConfig::from_path(&path).unwrap_err();
        let msg = err.to_string();
        let _ = std::fs::remove_dir_all(&dir);
        assert!(
            msg.contains("parsing autooptimizer config at"),
            "message must name the config path; got: {msg}"
        );
        assert!(
            msg.contains("min_improvement"),
            "message must embed the offending field name from the toml error; got: {msg}"
        );
    }

    #[test]
    fn validate_rejects_zero_holdout_min_improvement() {
        let mut cfg = AutoOptimizerConfig::default();
        cfg.holdout_min_improvement = 0.0;
        let err = cfg
            .validate()
            .expect_err("validate should reject holdout_min_improvement = 0");
        assert!(
            err.to_string().contains("holdout_min_improvement"),
            "error should mention holdout_min_improvement, got: {err}",
        );
    }

    #[test]
    fn validate_config_runs_scenario_pool_validation() {
        // An invalid pool must fail the top-level cfg.validate().
        let mut cfg = AutoOptimizerConfig::default();
        cfg.scenario_pool = vec![make_pair(
            "ovl",
            "2024-01-01",
            "2024-04-01",
            "2024-03-01",
            "2024-05-01",
        )];
        assert!(
            cfg.validate().is_err(),
            "cfg.validate() must propagate scenario_pool validation errors"
        );
    }

    // ── Scenario rotation ────────────────────────────────────────────────

    #[test]
    fn scenario_rotation_defaults_enabled_with_sensible_defaults() {
        let r = ScenarioRotationConfig::default();
        assert!(r.enabled, "scenario_rotation must default to enabled");
        assert_eq!(r.day_window_span_days, 14);
        assert_eq!(r.untouched_window_span_days, 14);
        assert_eq!(r.num_windows, 10);
        assert_eq!(r.stride_days, 30);
        assert!(r.date_range_start.is_none());
        assert!(r.date_range_end.is_none());
    }

    #[test]
    fn scenario_rotation_disabled_passes_validation() {
        let mut r = ScenarioRotationConfig::default();
        r.enabled = false;
        r.day_window_span_days = 0; // would be rejected if enabled
        assert!(validate_scenario_rotation(&r, None, None).is_ok());
    }

    #[test]
    fn scenario_rotation_rejects_zero_day_window_span() {
        let mut r = ScenarioRotationConfig::default();
        r.day_window_span_days = 0;
        let err = validate_scenario_rotation(&r, None, None).unwrap_err();
        assert!(err.to_string().contains("day_window_span_days"), "got: {err}");
    }

    #[test]
    fn scenario_rotation_rejects_negative_stride() {
        let mut r = ScenarioRotationConfig::default();
        r.stride_days = -1;
        let err = validate_scenario_rotation(&r, None, None).unwrap_err();
        assert!(err.to_string().contains("stride_days"), "got: {err}");
    }

    #[test]
    fn scenario_rotation_rejects_zero_num_windows() {
        let mut r = ScenarioRotationConfig::default();
        r.num_windows = 0;
        let err = validate_scenario_rotation(&r, None, None).unwrap_err();
        assert!(err.to_string().contains("num_windows"), "got: {err}");
    }

    #[test]
    fn scenario_rotation_rejects_overlong_day_window_span() {
        let mut r = ScenarioRotationConfig::default();
        r.day_window_span_days = 200; // > MAX_WINDOW_DAYS
        let err = validate_scenario_rotation(&r, None, None).unwrap_err();
        assert!(err.to_string().contains("day_window_span_days"), "got: {err}");
    }

    #[test]
    fn scenario_rotation_rejects_bad_date_range() {
        let mut r = ScenarioRotationConfig::default();
        r.date_range_start = Some(NaiveDate::from_ymd_opt(2025, 6, 1).unwrap());
        r.date_range_end = Some(NaiveDate::from_ymd_opt(2025, 1, 1).unwrap());
        let err = validate_scenario_rotation(&r, None, None).unwrap_err();
        assert!(err.to_string().contains("date_range_start"), "got: {err}");
    }

    #[test]
    fn scenario_rotation_valid_default_passes() {
        let r = ScenarioRotationConfig::default();
        let dw = DayWindow {
            start: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 4, 1).unwrap(),
        };
        let bw = BaselineUntouchedWindow {
            start: NaiveDate::from_ymd_opt(2025, 4, 1).unwrap(),
            end: NaiveDate::from_ymd_opt(2025, 5, 1).unwrap(),
        };
        assert!(validate_scenario_rotation(&r, Some(&dw), Some(&bw)).is_ok());
    }

    #[test]
    fn scenario_rotation_primes_from_toml() {
        // A config with a custom scenario_rotation section deserializes correctly.
        let cfg: AutoOptimizerConfig = toml::from_str(
            r#"
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

            [scenario_rotation]
            enabled = true
            day_window_span_days = 21
            untouched_window_span_days = 7
            num_windows = 8
            stride_days = 45
        "#,
        )
        .unwrap();
        let r = &cfg.scenario_rotation;
        assert!(r.enabled);
        assert_eq!(r.day_window_span_days, 21);
        assert_eq!(r.untouched_window_span_days, 7);
        assert_eq!(r.num_windows, 8);
        assert_eq!(r.stride_days, 45);
        assert!(r.date_range_start.is_none());
        assert!(r.date_range_end.is_none());
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn scenario_rotation_absent_from_toml_defaults_enabled() {
        // Back-compat: a config WITHOUT `[scenario_rotation]` must deserialize
        // with the struct default (enabled=true, spans=14, etc.).
        let cfg: AutoOptimizerConfig = toml::from_str(
            r#"
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
        "#,
        )
        .unwrap();
        assert!(cfg.scenario_rotation.enabled);
        assert_eq!(cfg.scenario_rotation.day_window_span_days, 14);
        assert_eq!(cfg.scenario_rotation.num_windows, 10);
    }
}
