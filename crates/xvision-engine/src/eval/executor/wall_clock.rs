//! `WallClock` — Live [`Clock`] impl. Reads `chrono::Utc::now()`
//! (or an injected `now_fn` for deterministic tests). `advance_to`
//! is a no-op — the wall clock takes no instruction.
//!
//! Sub-track 3 of the 2026-05-21 Alpaca-Live executor refactor
//! (see `team/contracts/live-bar-source-alpaca.md`). The companion
//! Backtest impl ([`crate::eval::executor::InstantClock`]) is the
//! advance-driven replay clock; this is the live counterpart.

use chrono::{DateTime, Utc};

use crate::eval::executor::traits::Clock;

/// Wall-clock [`Clock`]. `now()` returns the current UTC time (or
/// the injected `now_fn`'s output for tests); `advance_to` does
/// nothing.
pub struct WallClock {
    now_fn: Box<dyn Fn() -> DateTime<Utc> + Send + Sync>,
}

impl Default for WallClock {
    fn default() -> Self {
        Self::new()
    }
}

impl WallClock {
    /// Build a [`WallClock`] backed by `chrono::Utc::now()`.
    pub fn new() -> Self {
        Self::with_now_fn(Utc::now)
    }

    /// Build a [`WallClock`] with an injected clock source. Tests use
    /// this to feed deterministic timestamps.
    pub fn with_now_fn<F>(f: F) -> Self
    where
        F: Fn() -> DateTime<Utc> + Send + Sync + 'static,
    {
        Self { now_fn: Box::new(f) }
    }
}

impl Clock for WallClock {
    fn now(&self) -> DateTime<Utc> {
        (self.now_fn)()
    }

    /// Wall clock takes no instruction. The Live `Executor` calls
    /// this once per bar to match the Backtest path's shape, but the
    /// argument is ignored.
    fn advance_to(&mut self, _ts: DateTime<Utc>) {
        // intentionally empty
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicI64, Ordering};
    use std::sync::Arc;

    #[test]
    fn now_returns_injected_value_and_advance_to_is_noop() {
        let counter = Arc::new(AtomicI64::new(1_700_000_000));
        let counter_clone = counter.clone();
        let mut clock = WallClock::with_now_fn(move || {
            let secs = counter_clone.fetch_add(1, Ordering::SeqCst);
            DateTime::<Utc>::from_timestamp(secs, 0).expect("valid timestamp")
        });
        let t0 = clock.now();
        clock.advance_to(DateTime::<Utc>::from_timestamp(0, 0).unwrap());
        let t1 = clock.now();
        assert!(
            t1 > t0,
            "now_fn must keep advancing; advance_to must NOT reset it"
        );
        assert_eq!(t1.timestamp() - t0.timestamp(), 1);
    }

    #[test]
    fn default_uses_real_wall_clock_inside_utc_now_bracket() {
        let clock = WallClock::default();
        let before = Utc::now();
        let from_clock = clock.now();
        let after = Utc::now();
        assert!(
            from_clock >= before && from_clock <= after,
            "WallClock::default returned {from_clock}, outside [{before}, {after}]"
        );
    }
}
