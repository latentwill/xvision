//! `LiveSessionTracker` — in-memory per-session execution-layer state that
//! makes a live deployment's `drawdown_pct` and `daily_loss_limit_remaining_usd`
//! computable from execution truth (CT5, Epic s78 Wave 3, §6).
//!
//! These are deliberately **NOT a persisted snapshot table**. The live loop
//! already holds a `peak_equity` high-water mark and a `daily_realized_at_day_start`
//! UTC-day baseline; this type promotes those loop-local variables into a small
//! cohesive holder so the drawdown + daily-loss formulas are unit-testable in
//! isolation and re-usable from the metrics emission. A session's drawdown is
//! per-session: resetting to `initial` each loop start is acceptable, and the
//! poll path reconstructs drawdown from the persisted equity curve.
//!
//! HONESTY MANDATE (§8.1): when the session has no equity sample / no peak yet,
//! `drawdown_pct()` returns `None` (rendered "—"), never a fabricated `0`.
//! Likewise `daily_loss_limit_remaining_usd()` returns `None` when no kill
//! policy or no day baseline has been seen.

use chrono::NaiveDate;

/// In-memory per-session peak-equity + day-start-baseline tracker. Owned by the
/// live loop (`run_inner_live`) for the duration of one session.
#[derive(Debug, Clone)]
pub struct LiveSessionTracker {
    /// Starting capital for the session. Used as the daily-loss-limit base.
    starting_capital: f64,
    /// High-water mark of session equity. `None` until the first equity sample.
    peak_equity: Option<f64>,
    /// UTC day the daily-loss accumulator was last rolled. `None` ⇒ not yet seen.
    daily_loss_day: Option<NaiveDate>,
    /// The book's realized-PnL snapshot taken at the start of the current UTC
    /// day. `realized_today = book.realized() - daily_realized_at_day_start`.
    daily_realized_at_day_start: f64,
}

impl LiveSessionTracker {
    /// Construct a fresh tracker for a session with the given starting capital.
    /// No peak and no day baseline yet — `drawdown_pct()` / the daily-loss
    /// buffer return `None` until the first equity sample / day roll.
    pub fn new(starting_capital: f64) -> Self {
        Self {
            starting_capital,
            peak_equity: None,
            daily_loss_day: None,
            daily_realized_at_day_start: 0.0,
        }
    }

    /// Construct a tracker with a pre-seeded peak (test/poll-fallback seam) so
    /// the drawdown formula can be exercised against a known high-water mark.
    pub fn with_peak(starting_capital: f64, peak_equity: f64) -> Self {
        Self {
            starting_capital,
            peak_equity: Some(peak_equity),
            daily_loss_day: None,
            daily_realized_at_day_start: 0.0,
        }
    }

    /// Record a new equity sample, advancing the high-water mark when it rises.
    /// Call once per post-tick equity sample in the live loop.
    pub fn observe_equity(&mut self, equity: f64) {
        self.peak_equity = Some(match self.peak_equity {
            Some(p) if p >= equity => p,
            _ => equity,
        });
    }

    /// Current session high-water mark, if any equity has been observed.
    pub fn peak_equity(&self) -> Option<f64> {
        self.peak_equity
    }

    /// `(peak_equity - current_equity) / peak_equity * 100`, clamped at 0.
    /// `None` when no peak has been observed yet (session not started / no
    /// equity sample) or the peak is non-positive — the honest "no data" case,
    /// never a fabricated `0`.
    pub fn drawdown_pct(&self, current_equity: f64) -> Option<f64> {
        match self.peak_equity {
            Some(peak) if peak > 0.0 => Some(((peak - current_equity) / peak * 100.0).max(0.0)),
            _ => None,
        }
    }

    /// Roll the daily-loss day baseline when a new UTC day is seen. On a day
    /// boundary the baseline becomes the book's current realized PnL, so
    /// `realized_today` resets to 0 for the new day. Idempotent within a day.
    pub fn roll_day(&mut self, bar_day: NaiveDate, realized_now: f64) {
        if self.daily_loss_day != Some(bar_day) {
            self.daily_loss_day = Some(bar_day);
            self.daily_realized_at_day_start = realized_now;
        }
    }

    /// `realized_today = realized_now - daily_realized_at_day_start`. `None`
    /// when no day baseline has been rolled yet.
    pub fn realized_today(&self, realized_now: f64) -> Option<f64> {
        self.daily_loss_day
            .map(|_| realized_now - self.daily_realized_at_day_start)
    }

    /// Exact headroom (USD) before the enforced daily-loss kill fires:
    /// `(kill_pct * starting_capital) + realized_today`, where `realized_today`
    /// is negative on a losing day so the buffer shrinks toward 0.
    ///
    /// `None` when there is no kill policy (`kill_pct <= 0`) or no day baseline
    /// has been rolled yet (§6.2) — never a fabricated `0`.
    pub fn daily_loss_limit_remaining_usd(&self, kill_pct: f64, realized_now: f64) -> Option<f64> {
        if kill_pct <= 0.0 {
            return None;
        }
        self.realized_today(realized_now)
            .map(|realized_today| (kill_pct * self.starting_capital) + realized_today)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    #[test]
    fn fresh_tracker_has_no_peak_and_null_drawdown() {
        // HONESTY: no equity sample yet ⇒ drawdown is None (not 0).
        let t = LiveSessionTracker::new(10_000.0);
        assert_eq!(t.peak_equity(), None);
        assert_eq!(t.drawdown_pct(10_000.0), None);
    }

    #[test]
    fn drawdown_computes_from_seeded_peak() {
        // Seed a peak of 12_000; current equity 10_800 ⇒ 10% drawdown.
        let t = LiveSessionTracker::with_peak(10_000.0, 12_000.0);
        assert_eq!(t.drawdown_pct(10_800.0), Some(10.0));
    }

    #[test]
    fn peak_advances_only_upward_and_drives_drawdown() {
        let mut t = LiveSessionTracker::new(10_000.0);
        t.observe_equity(10_000.0);
        assert_eq!(t.drawdown_pct(10_000.0), Some(0.0));
        t.observe_equity(11_000.0); // new high-water mark
        assert_eq!(t.peak_equity(), Some(11_000.0));
        // A dip to 9_900 against the 11_000 peak is a 10% drawdown.
        assert_eq!(t.drawdown_pct(9_900.0), Some(10.0));
        // A lower equity does NOT lower the peak.
        t.observe_equity(9_900.0);
        assert_eq!(t.peak_equity(), Some(11_000.0));
    }

    #[test]
    fn drawdown_clamps_at_zero_when_equity_exceeds_peak() {
        // Equity above peak (transient before observe) ⇒ clamp at 0, not negative.
        let t = LiveSessionTracker::with_peak(10_000.0, 10_000.0);
        assert_eq!(t.drawdown_pct(10_500.0), Some(0.0));
    }

    #[test]
    fn drawdown_none_when_peak_non_positive() {
        let t = LiveSessionTracker::with_peak(0.0, 0.0);
        assert_eq!(t.drawdown_pct(0.0), None);
    }

    #[test]
    fn daily_loss_buffer_none_before_day_roll() {
        // No day baseline yet ⇒ buffer is None (not the full envelope).
        let t = LiveSessionTracker::new(10_000.0);
        assert_eq!(t.daily_loss_limit_remaining_usd(0.05, 0.0), None);
        assert_eq!(t.realized_today(0.0), None);
    }

    #[test]
    fn daily_loss_buffer_none_when_no_kill_policy() {
        let mut t = LiveSessionTracker::new(10_000.0);
        t.roll_day(day("2026-06-13"), 0.0);
        assert_eq!(t.daily_loss_limit_remaining_usd(0.0, 0.0), None);
    }

    #[test]
    fn daily_loss_buffer_computes_from_seeded_day_start() {
        // kill_pct 5% of 10_000 = 500 envelope. Day starts at realized 0.
        let mut t = LiveSessionTracker::new(10_000.0);
        t.roll_day(day("2026-06-13"), 0.0);
        // No loss yet ⇒ full 500 buffer.
        assert_eq!(t.daily_loss_limit_remaining_usd(0.05, 0.0), Some(500.0));
        // Realized -120 today ⇒ realized_today = -120 ⇒ buffer 380.
        assert_eq!(t.realized_today(-120.0), Some(-120.0));
        assert_eq!(t.daily_loss_limit_remaining_usd(0.05, -120.0), Some(380.0));
    }

    #[test]
    fn day_roll_rebaselines_realized_today() {
        // Day 1 ends with realized -200; day 2 starts ⇒ realized_today resets.
        let mut t = LiveSessionTracker::new(10_000.0);
        t.roll_day(day("2026-06-13"), 0.0);
        assert_eq!(t.realized_today(-200.0), Some(-200.0));
        // New UTC day: baseline rebaselines to the -200 carried into day 2.
        t.roll_day(day("2026-06-14"), -200.0);
        // Same realized total ⇒ no loss *today* yet.
        assert_eq!(t.realized_today(-200.0), Some(0.0));
        assert_eq!(t.daily_loss_limit_remaining_usd(0.05, -200.0), Some(500.0));
        // Roll within the same day is idempotent (baseline unchanged).
        t.roll_day(day("2026-06-14"), -250.0);
        assert_eq!(t.realized_today(-250.0), Some(-50.0));
    }
}
