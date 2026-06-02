//! Latency model tests: fill price shifts with decision_to_fill_ms.
//! Covers M3 (latency half) of the 2026-06-02 synthetic-eval-fill-path spec.

use chrono::{TimeZone, Utc};
use xvision_engine::eval::executor::traits::{EvalOnly, FillRequest, FillSink, SimulatedFills};
use xvision_engine::eval::scenario::{FeeSource, SlippageModel};

fn ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

fn req_with_latency(
    next_open: f64,
    bar_close: f64,
    decision_to_fill_ms: u32,
    bar_duration_ms: u64,
) -> FillRequest {
    FillRequest {
        pos: 0.0,
        entry: 0.0,
        action: "long_open".into(),
        next_open,
        bar_volume: 10_000.0,
        slip_bps: 0.0, // zero slip so fill_price == fill_ref exactly
        spread_bps: 0.0,
        taker_bps: 0.0, // zero fees so math is clean
        maker_bps: 0.0,
        equity: 100_000.0,
        risk_pct: 0.01,
        slippage_model: SlippageModel::None,
        fee_source: FeeSource::Default,
        asset: "BTC/USD".into(),
        bar_ts: ts(),
        bar_open: next_open,
        bar_high: next_open.max(bar_close) * 1.01,
        bar_low: next_open.min(bar_close) * 0.99,
        bar_close,
        decision_to_fill_ms,
        bar_duration_ms,
    }
}

#[tokio::test]
async fn zero_latency_fills_at_next_open() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink.submit(req_with_latency(100.0, 102.0, 0, 3_600_000)).await;

    let fp = rec.fill_price.expect("filled");
    assert!(
        (fp - 100.0).abs() < 1e-10,
        "zero latency must fill at next_open=100, got {fp}"
    );
}

#[tokio::test]
async fn half_bar_latency_shifts_fill_toward_bar_close() {
    // fill_ref = 100 + 0.5 * (102 - 100) = 101
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink
        .submit(req_with_latency(100.0, 102.0, 1_800_000, 3_600_000))
        .await;

    let fp = rec.fill_price.expect("filled");
    assert!(
        (fp - 101.0).abs() < 1e-10,
        "half-bar latency must give fill_ref=101, got {fp}"
    );
}

#[tokio::test]
async fn full_bar_latency_fills_at_bar_close() {
    // fill_ref = 100 + 1.0 * (102 - 100) = 102
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink
        .submit(req_with_latency(100.0, 102.0, 3_600_000, 3_600_000))
        .await;

    let fp = rec.fill_price.expect("filled");
    assert!(
        (fp - 102.0).abs() < 1e-10,
        "full-bar latency must fill at bar_close=102, got {fp}"
    );
}

#[tokio::test]
async fn latency_exceeding_bar_duration_is_capped_at_bar_close() {
    // 2x bar duration → fraction clamped to 1.0 → fill at bar_close
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink
        .submit(req_with_latency(100.0, 102.0, 7_200_000, 3_600_000))
        .await;

    let fp = rec.fill_price.expect("filled");
    assert!(
        (fp - 102.0).abs() < 1e-10,
        "oversized latency must clamp to bar_close=102, got {fp}"
    );
}

#[tokio::test]
async fn nonzero_latency_shifts_long_fill_up_when_close_above_open() {
    let mut s_zero = SimulatedFills::new(EvalOnly::new_for_tests());
    let r_zero = s_zero.submit(req_with_latency(100.0, 105.0, 0, 3_600_000)).await;

    let mut s_latency = SimulatedFills::new(EvalOnly::new_for_tests());
    let r_latency = s_latency
        .submit(req_with_latency(100.0, 105.0, 900_000, 3_600_000))
        .await;

    assert!(
        r_latency.fill_price.unwrap() > r_zero.fill_price.unwrap(),
        "latency on rising bar must push long fill price up: {:.6} vs {:.6}",
        r_latency.fill_price.unwrap(),
        r_zero.fill_price.unwrap(),
    );
}

#[tokio::test]
async fn latency_deterministic_same_inputs_same_result() {
    let req = req_with_latency(100.0, 104.0, 900_000, 3_600_000);

    let mut s1 = SimulatedFills::new(EvalOnly::new_for_tests());
    let r1 = s1.submit(req.clone()).await;

    let mut s2 = SimulatedFills::new(EvalOnly::new_for_tests());
    let r2 = s2.submit(req.clone()).await;

    assert_eq!(r1.fill_price, r2.fill_price, "fill_price must be deterministic");
    assert_eq!(
        r1.realized_pnl, r2.realized_pnl,
        "realized_pnl must be deterministic"
    );
}

#[test]
fn zero_bar_duration_ms_falls_back_to_next_open() {
    // When bar_duration_ms = 0, latency fraction is 0 regardless of
    // decision_to_fill_ms → fill_ref = next_open.
    let req = FillRequest {
        pos: 0.0,
        entry: 0.0,
        action: "long_open".into(),
        next_open: 100.0,
        bar_volume: 1_000.0,
        slip_bps: 0.0,
        spread_bps: 0.0,
        taker_bps: 0.0,
        maker_bps: 0.0,
        equity: 100_000.0,
        risk_pct: 0.01,
        slippage_model: SlippageModel::None,
        fee_source: FeeSource::Default,
        asset: "BTC/USD".into(),
        bar_ts: Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap(),
        bar_open: 100.0,
        bar_high: 105.0,
        bar_low: 95.0,
        bar_close: 105.0,
        decision_to_fill_ms: 1_000,
        bar_duration_ms: 0, // zero → no latency
    };
    // Use simulate_fill_inner via SimulatedFills submit.
    // We just verify it compiles and runs without panic.
    // The fill_price must equal next_open (100.0) since bar_duration_ms=0.
    let rt = tokio::runtime::Runtime::new().unwrap();
    let fp = rt.block_on(async {
        let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
        let rec = sink.submit(req).await;
        rec.fill_price.unwrap()
    });
    assert!(
        (fp - 100.0).abs() < 1e-10,
        "bar_duration_ms=0 must give fill at next_open=100, got {fp}"
    );
}
