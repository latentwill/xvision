//! Unit tests for [`xvision_engine::eval::executor::wall_clock::WallClock`].
//!
//! Pins the injected `now_fn` seam and the no-op `advance_to`.

use chrono::{DateTime, TimeZone, Utc};

use xvision_engine::eval::executor::traits::Clock;
use xvision_engine::eval::executor::WallClock;

#[test]
fn injected_now_fn_is_used_and_advance_to_is_noop() {
    let fixed = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
    let mut clock = WallClock::with_now_fn(move || fixed);
    assert_eq!(clock.now(), fixed);
    // advance_to must NOT change `now()`'s output — the wall clock
    // takes no instruction.
    let unrelated = Utc.timestamp_opt(1_500_000_000, 0).unwrap();
    clock.advance_to(unrelated);
    assert_eq!(clock.now(), fixed);
}

#[test]
fn default_constructor_follows_utc_now() {
    let clock = WallClock::default();
    let before = Utc::now() - chrono::Duration::seconds(1);
    let from_clock: DateTime<Utc> = clock.now();
    let after = Utc::now() + chrono::Duration::seconds(1);
    assert!(
        from_clock >= before && from_clock <= after,
        "from_clock={from_clock}, before={before}, after={after}"
    );
}
