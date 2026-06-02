//! Unit tests for the `BarSource` / `Clock` / `FillSink` traits and their
//! Backtest concrete impls. Sub-track 1 of the 2026-05-21 executor
//! refactor (`team/contracts/executor-trait-extraction.md`).
//!
//! These tests pin the trait surface; the integration regression — that
//! `Executor` produces byte-identical metrics on real fixtures
//! after the rewire — is covered by the existing test suite
//! (`decisions_count.rs`, `eval_progress_backtest.rs`,
//! `api_eval_run.rs`, etc.), all of which exercise the executor's
//! per-bar loop end-to-end and continue to pass after the rewire.

use chrono::{Duration, TimeZone, Utc};
use xvision_core::market::Ohlcv;
use xvision_engine::eval::executor::traits::EvalOnly;
use xvision_engine::eval::executor::{
    BarSource, Clock, FillRequest, FillSink, InjectedBars, InstantClock, SimulatedFills,
};
use xvision_engine::eval::scenario::{FeeSource, SlippageModel};

// ---------------------------------------------------------------------------
// BarSource
// ---------------------------------------------------------------------------

fn make_bars(count: usize) -> Vec<Ohlcv> {
    let start = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
    (0..count)
        .map(|i| {
            let px = 50_000.0 + i as f64 * 100.0;
            Ohlcv {
                timestamp: start + Duration::hours(i as i64),
                open: px,
                high: px + 250.0,
                low: px - 250.0,
                close: px + 50.0,
                volume: 1_000.0 + i as f64,
            }
        })
        .collect()
}

#[tokio::test]
async fn injected_bars_yields_in_input_order_then_none() {
    let bars = make_bars(3);
    let mut src = InjectedBars::new(bars.clone());

    let b0 = src.next_bar().await.expect("first bar");
    assert_eq!(b0.timestamp, bars[0].timestamp);
    assert_eq!(b0.open, bars[0].open);

    let b1 = src.next_bar().await.expect("second bar");
    assert_eq!(b1.timestamp, bars[1].timestamp);

    let b2 = src.next_bar().await.expect("third bar");
    assert_eq!(b2.timestamp, bars[2].timestamp);

    assert!(src.next_bar().await.is_none(), "drained source returns None");
    assert!(
        src.next_bar().await.is_none(),
        "calling after None continues to return None"
    );
}

#[tokio::test]
async fn injected_bars_empty_source_returns_none_immediately() {
    let mut src = InjectedBars::new(Vec::new());
    assert!(src.next_bar().await.is_none());
}

#[test]
fn injected_bars_all_returns_full_slice() {
    let bars = make_bars(4);
    let src = InjectedBars::new(bars.clone());
    assert_eq!(src.all().len(), 4);
    assert_eq!(src.all()[0].timestamp, bars[0].timestamp);
    assert_eq!(src.all()[3].timestamp, bars[3].timestamp);
}

#[tokio::test]
async fn injected_bars_remaining_tracks_cursor() {
    let bars = make_bars(3);
    let mut src = InjectedBars::new(bars);
    assert_eq!(src.remaining().len(), 3);
    let _ = src.next_bar().await;
    assert_eq!(src.remaining().len(), 2);
    let _ = src.next_bar().await;
    let _ = src.next_bar().await;
    assert_eq!(src.remaining().len(), 0);
    let _ = src.next_bar().await;
    assert_eq!(src.remaining().len(), 0, "remaining stays at 0 after drain");
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

#[test]
fn instant_clock_starts_at_epoch_before_first_advance() {
    let clock = InstantClock::new();
    let epoch = Utc.with_ymd_and_hms(1970, 1, 1, 0, 0, 0).unwrap();
    assert_eq!(clock.now(), epoch);
}

#[test]
fn instant_clock_now_returns_most_recent_advance_target() {
    let mut clock = InstantClock::new();
    let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
    let t2 = Utc.with_ymd_and_hms(2026, 1, 1, 13, 0, 0).unwrap();
    clock.advance_to(t1);
    assert_eq!(clock.now(), t1);
    clock.advance_to(t2);
    assert_eq!(clock.now(), t2);
}

// ---------------------------------------------------------------------------
// FillSink
// ---------------------------------------------------------------------------

/// Hand-picked golden-value request matching the test fixture in
/// `backtest.rs`'s `args()` helper. Pre-refactor `simulate_fill` on
/// this input produces a long_open from flat at fill_price ≈ 60_060
/// (60_000 * (1 + 10/10000)).
fn golden_request(pos: f64, action: &str) -> FillRequest {
    FillRequest {
        pos,
        entry: 50_000.0,
        action: action.into(),
        next_open: 60_000.0,
        bar_volume: 1_000.0,
        slip_bps: 10.0,
        spread_bps: 0.0,
        taker_bps: 25.0,
        maker_bps: 10.0,
        equity: 10_000.0,
        risk_pct: 0.02,
        slippage_model: SlippageModel::Linear { bps: 10 },
        fee_source: FeeSource::Default,
        asset: "BTC/USD".into(),
        bar_ts: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        bar_open: 60_000.0,
        bar_high: 61_000.0,
        bar_low: 59_000.0,
        bar_close: 60_500.0,
        decision_to_fill_ms: 0,
        bar_duration_ms: 3_600_000,
    }
}

#[tokio::test]
async fn simulated_fills_long_open_from_flat_matches_pre_refactor() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink.submit(golden_request(0.0, "long_open")).await;
    let fp = rec.fill_price.expect("filled");
    // 60_000 * (1 + 10/10000) = 60_060
    assert!((fp - 60_060.0).abs() < 1e-6, "fill_price = {fp}");
    assert!(rec.new_pos > 0.0);
    assert!(rec.fill_size.is_some());
    let fee = rec.fee.expect("fee");
    // Pure open: no realized leg from a prior position, but the open
    // leg's fee is still booked. Pre-refactor `simulate_fill` returns
    // `realized - fee`, so `realized_pnl == -fee` for a pure open.
    assert!(
        (rec.realized_pnl + fee).abs() < 1e-9,
        "realized_pnl ({}) should equal -fee ({})",
        rec.realized_pnl,
        fee
    );
    assert!(
        rec.volume_cap_hit.is_none(),
        "linear slippage does not bind volume cap"
    );
}

#[tokio::test]
async fn simulated_fills_noop_when_already_aligned() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    // flat when flat — no-op.
    let r1 = sink.submit(golden_request(0.0, "flat")).await;
    assert_eq!(r1.new_pos, 0.0);
    assert!(r1.fill_price.is_none());
    assert_eq!(r1.realized_pnl, 0.0);

    // long when already long — no-op.
    let r2 = sink.submit(golden_request(0.001, "long_open")).await;
    assert_eq!(r2.new_pos, 0.001);
    assert!(r2.fill_price.is_none());
}

#[tokio::test]
async fn simulated_fills_flat_closes_long_and_books_realized() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink.submit(golden_request(0.001, "flat")).await;
    assert_eq!(rec.new_pos, 0.0);
    let fp = rec.fill_price.expect("close-leg fills");
    // long close: fill at next_open * (1 - slip) = 60_000 * 0.999 = 59_940
    assert!((fp - 59_940.0).abs() < 1e-6, "fill_price = {fp}");
    // realized leg = pos * (close - entry) - fee
    //              = 0.001 * (59_940 - 50_000) = 9.94
    // fee = 0.001 * 59_940 * 25/10000 = 0.14985
    // realized_pnl ≈ 9.79
    assert!(
        rec.realized_pnl > 9.0 && rec.realized_pnl < 10.0,
        "realized_pnl out of band: {}",
        rec.realized_pnl
    );
}

#[tokio::test]
async fn simulated_fills_short_open_from_long_reverses() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink.submit(golden_request(0.001, "short_open")).await;
    assert!(rec.new_pos < 0.0);
    assert!(rec.fill_price.is_some());
    // closing long at gain produces positive realized PnL.
    assert!(rec.realized_pnl > 0.0);
}

/// Volume-cap regression — pin that the `volume_cap_hit` provenance
/// tuple matches the pre-refactor `SimulateFillResult.volume_cap_hit`
/// shape under `VolumeShare` slippage.
#[tokio::test]
async fn simulated_fills_volume_share_cap_emits_provenance_tuple() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let mut req = golden_request(0.0, "long_open");
    // Force the cap to bind: tiny bar volume relative to the order size.
    req.bar_volume = 0.0001;
    req.slippage_model = SlippageModel::VolumeShare {
        price_impact: 0.1,
        volume_limit: 0.05,
    };
    let rec = sink.submit(req).await;
    let (_req_qty, bar_vol, cap_qty, fill_share) = rec.volume_cap_hit.expect("cap should have bound");
    assert_eq!(bar_vol, 0.0001);
    assert!((fill_share - 0.05).abs() < 1e-9);
    assert!((cap_qty - 0.05 * 0.0001).abs() < 1e-9);
}
