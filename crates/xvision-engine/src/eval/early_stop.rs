//! Eval early-stop policy — short-circuits the outer per-bar loop when the
//! trader has been emitting consecutive low-conviction `flat`/`hold`
//! decisions and the portfolio hasn't changed. F-9 of the
//! `eval-traces-2026-05-19` wave.
//!
//! ## Why
//!
//! Audit run `01KS03Z0BRCTDM1MX8BRRGMQP5` produced 20 consecutive `flat`
//! decisions with conviction ≤ 0.2 and burned ~460k input tokens. A
//! degenerate model that's settled on "do nothing" should not keep paying
//! the LLM tax. After K consecutive low-conviction flats with no
//! portfolio change, the executor inherits the next M decisions
//! (writes them as `action=flat`, `conviction=0.0`,
//! `justification="inherited from early-stop policy"`) instead of
//! invoking the model.
//!
//! ## Reset triggers
//!
//! The skip-eligibility streak resets on ANY of:
//!   1. A non-`flat`/`hold` action (e.g. `long_open`, `short_open`).
//!   2. A portfolio state change — a position opens, closes, or changes
//!      size; a new asset enters the active set.
//!   3. (Caller-side) starting a fresh run.
//!
//! ## Configuration
//!
//! `EarlyStopConfig::default()` is `window=8, skip_count=4,
//! conviction_threshold=0.2`. `from_env_or_default()` overrides each
//! field from these environment variables when present and parseable:
//!
//!   - `XVN_EARLY_STOP_WINDOW`       (usize) — streak length to trigger
//!   - `XVN_EARLY_STOP_SKIP`         (u32)   — bars to skip per trigger
//!   - `XVN_EARLY_STOP_CONVICTION`   (f64)   — max conviction counted as "low"
//!
//! Out-of-range / unparseable values fall back to the default for that
//! field; we do not panic on a typo'd env var.

use std::env;

/// Policy knobs. All fields are pub so callers can tune mid-run or build
/// a fixture-specific config in tests.
#[derive(Debug, Clone, PartialEq)]
pub struct EarlyStopConfig {
    /// Number of consecutive low-conviction flats/holds that must precede
    /// a skip. Default 8.
    pub window: usize,
    /// Number of decisions to inherit (skip) once the policy fires.
    /// Default 4.
    pub skip_count: u32,
    /// Maximum conviction (inclusive) that counts as "low". A single
    /// decision in the window with conviction strictly above this floor
    /// disqualifies the streak. Default 0.2.
    pub conviction_threshold: f64,
}

impl Default for EarlyStopConfig {
    fn default() -> Self {
        Self {
            window: 8,
            skip_count: 4,
            conviction_threshold: 0.2,
        }
    }
}

impl EarlyStopConfig {
    /// Build a config from defaults, overriding any field whose env var
    /// is set and parses cleanly. Unparseable / out-of-range values fall
    /// back to the default for that field.
    pub fn from_env_or_default() -> Self {
        let mut cfg = Self::default();
        if let Ok(raw) = env::var("XVN_EARLY_STOP_WINDOW") {
            if let Ok(parsed) = raw.parse::<usize>() {
                if parsed > 0 {
                    cfg.window = parsed;
                }
            }
        }
        if let Ok(raw) = env::var("XVN_EARLY_STOP_SKIP") {
            if let Ok(parsed) = raw.parse::<u32>() {
                cfg.skip_count = parsed;
            }
        }
        if let Ok(raw) = env::var("XVN_EARLY_STOP_CONVICTION") {
            if let Ok(parsed) = raw.parse::<f64>() {
                if parsed.is_finite() && parsed >= 0.0 {
                    cfg.conviction_threshold = parsed;
                }
            }
        }
        cfg
    }
}

/// What the executor should do when the policy fires.
#[derive(Debug, Clone, PartialEq)]
pub struct SkipPlan {
    /// How many of the next decisions to inherit (write as flat,
    /// conviction=0.0, no model call).
    pub skip_count: u32,
    /// Human-readable rationale, suitable for a `supervisor_notes` row.
    pub reason: String,
}

/// Action enum the policy operates on. Mirrors the v1 trader-output
/// schema (`long_open`, `short_open`, `flat`, `hold`, `close`). The
/// executor passes whatever action string it observed; we classify here
/// rather than at every call site.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Flat,
    Hold,
    /// Any other action (`long_open`, `short_open`, `close`, …) — counts
    /// as a forward-progress event that resets the streak.
    Other,
}

impl Action {
    /// Classify an action string into the policy's three-way enum. The
    /// comparison is ASCII-case-insensitive and trims whitespace, so
    /// `" Flat "` and `"FLAT"` both land as `Flat`.
    pub fn classify(action: &str) -> Self {
        let trimmed = action.trim();
        if trimmed.eq_ignore_ascii_case("flat") {
            Action::Flat
        } else if trimmed.eq_ignore_ascii_case("hold") {
            Action::Hold
        } else {
            Action::Other
        }
    }

    fn is_skippable(self) -> bool {
        matches!(self, Action::Flat | Action::Hold)
    }
}

/// Decide whether the next decision should be skipped (inherited). Pure
/// function — no I/O, no clock, no state outside the inputs.
///
/// Returns `Some(SkipPlan)` only when ALL of these hold:
///
///   1. `recent_actions.len() >= cfg.window`,
///   2. The last `cfg.window` actions are all `Flat` or `Hold`,
///   3. `recent_convictions.len() >= cfg.window` and every conviction
///      in the last `cfg.window` slice is `<= cfg.conviction_threshold`,
///   4. `portfolio_unchanged == true`.
///
/// Returns `None` otherwise. Caller is responsible for resetting the
/// counter when forward progress happens — but condition (2) is also
/// sufficient to prevent a false trigger if the caller forgets.
pub fn should_skip_next_decision(
    recent_actions: &[Action],
    recent_convictions: &[f64],
    portfolio_unchanged: bool,
    cfg: &EarlyStopConfig,
) -> Option<SkipPlan> {
    if cfg.window == 0 || cfg.skip_count == 0 {
        return None;
    }
    if !portfolio_unchanged {
        return None;
    }
    if recent_actions.len() < cfg.window || recent_convictions.len() < cfg.window {
        return None;
    }

    let action_tail = &recent_actions[recent_actions.len() - cfg.window..];
    if !action_tail.iter().all(|a| a.is_skippable()) {
        return None;
    }

    let conviction_tail = &recent_convictions[recent_convictions.len() - cfg.window..];
    if !conviction_tail
        .iter()
        .all(|c| c.is_finite() && *c <= cfg.conviction_threshold)
    {
        return None;
    }

    Some(SkipPlan {
        skip_count: cfg.skip_count,
        reason: format!(
            "early-stop: {} low-conviction flats; skipping {} bars",
            cfg.window, cfg.skip_count
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flats(n: usize) -> Vec<Action> {
        vec![Action::Flat; n]
    }

    fn convictions(n: usize, c: f64) -> Vec<f64> {
        vec![c; n]
    }

    #[test]
    fn fires_on_eight_flats_low_conviction_no_state_change() {
        let cfg = EarlyStopConfig::default();
        let plan =
            should_skip_next_decision(&flats(8), &convictions(8, 0.1), true, &cfg).expect("should fire");
        assert_eq!(plan.skip_count, 4);
        assert!(plan.reason.contains("8"));
        assert!(plan.reason.contains("4"));
    }

    #[test]
    fn seven_flats_below_window_no_fire() {
        let cfg = EarlyStopConfig::default();
        assert!(should_skip_next_decision(&flats(7), &convictions(7, 0.1), true, &cfg).is_none());
    }

    #[test]
    fn eight_flats_with_one_conviction_above_threshold_no_fire() {
        let cfg = EarlyStopConfig::default();
        let mut conv = convictions(8, 0.1);
        // Spike one of the convictions just above the threshold.
        conv[3] = 0.21;
        assert!(should_skip_next_decision(&flats(8), &conv, true, &cfg).is_none());
    }

    #[test]
    fn eight_flats_but_portfolio_changed_no_fire() {
        let cfg = EarlyStopConfig::default();
        assert!(should_skip_next_decision(&flats(8), &convictions(8, 0.1), false, &cfg).is_none());
    }

    #[test]
    fn eight_holds_is_eligible() {
        let cfg = EarlyStopConfig::default();
        let plan = should_skip_next_decision(&vec![Action::Hold; 8], &convictions(8, 0.05), true, &cfg);
        assert!(plan.is_some());
    }

    #[test]
    fn mixed_flat_and_hold_is_eligible() {
        let cfg = EarlyStopConfig::default();
        let mut acts = flats(8);
        acts[2] = Action::Hold;
        acts[5] = Action::Hold;
        assert!(should_skip_next_decision(&acts, &convictions(8, 0.1), true, &cfg).is_some());
    }

    #[test]
    fn single_non_skippable_action_disqualifies() {
        let cfg = EarlyStopConfig::default();
        let mut acts = flats(8);
        acts[4] = Action::Other;
        assert!(should_skip_next_decision(&acts, &convictions(8, 0.1), true, &cfg).is_none());
    }

    #[test]
    fn nan_conviction_disqualifies() {
        let cfg = EarlyStopConfig::default();
        let mut conv = convictions(8, 0.1);
        conv[0] = f64::NAN;
        assert!(should_skip_next_decision(&flats(8), &conv, true, &cfg).is_none());
    }

    #[test]
    fn classify_action_strings() {
        assert_eq!(Action::classify("flat"), Action::Flat);
        assert_eq!(Action::classify("FLAT"), Action::Flat);
        assert_eq!(Action::classify(" hold "), Action::Hold);
        assert_eq!(Action::classify("long_open"), Action::Other);
        assert_eq!(Action::classify("short_open"), Action::Other);
        assert_eq!(Action::classify("close"), Action::Other);
    }

    #[test]
    fn longer_history_uses_only_tail() {
        // 12 entries, last 8 all flat+low — should fire even though
        // earlier entries include `Other`.
        let cfg = EarlyStopConfig::default();
        let mut acts = flats(12);
        acts[0] = Action::Other;
        acts[1] = Action::Other;
        let conv = convictions(12, 0.1);
        assert!(should_skip_next_decision(&acts, &conv, true, &cfg).is_some());
    }

    #[test]
    fn zero_window_or_skip_disables_policy() {
        let mut cfg = EarlyStopConfig::default();
        cfg.window = 0;
        assert!(should_skip_next_decision(&flats(8), &convictions(8, 0.0), true, &cfg).is_none());
        cfg = EarlyStopConfig::default();
        cfg.skip_count = 0;
        assert!(should_skip_next_decision(&flats(8), &convictions(8, 0.0), true, &cfg).is_none());
    }

    #[test]
    fn default_config_values() {
        let cfg = EarlyStopConfig::default();
        assert_eq!(cfg.window, 8);
        assert_eq!(cfg.skip_count, 4);
        assert!((cfg.conviction_threshold - 0.2).abs() < 1e-12);
    }
}
