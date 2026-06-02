//! Foundation tests: EvalOnly token + synthetic short fills through
//! SimulatedFills. Covers M1 of the 2026-06-02 synthetic-eval-fill-path spec.

use chrono::{TimeZone, Utc};
use xvision_engine::eval::executor::traits::{EvalOnly, FillRequest, FillSink, SimulatedFills};
use xvision_engine::eval::scenario::{FeeSource, SlippageModel};

fn ts() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap()
}

fn base_req(pos: f64, entry: f64, action: &str, next_open: f64) -> FillRequest {
    FillRequest {
        pos,
        entry,
        action: action.into(),
        next_open,
        bar_volume: 10_000.0,
        slip_bps: 5.0,
        spread_bps: 0.0,
        taker_bps: 10.0,
        maker_bps: 5.0,
        equity: 100_000.0,
        risk_pct: 0.01,
        slippage_model: SlippageModel::None,
        fee_source: FeeSource::Default,
        asset: "BTC/USD".into(),
        bar_ts: ts(),
        bar_open: next_open,
        bar_high: next_open * 1.01,
        bar_low: next_open * 0.99,
        bar_close: next_open,
        decision_to_fill_ms: 0,
        bar_duration_ms: 3_600_000,
    }
}

#[tokio::test]
async fn short_open_from_flat_creates_negative_position() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink.submit(base_req(0.0, 0.0, "short_open", 50_000.0)).await;

    assert!(
        rec.new_pos < 0.0,
        "short_open must create negative position, got {}",
        rec.new_pos
    );
    assert!(rec.fill_price.is_some(), "fill price must be set");
    assert_eq!(
        rec.realized_pnl + rec.fee.unwrap_or(0.0),
        0.0,
        "pure open: no realized leg"
    );
}

#[tokio::test]
async fn short_flat_realizes_correct_pnl() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());

    // Open short at 50_000, no slippage.
    let open = sink.submit(base_req(0.0, 0.0, "short_open", 50_000.0)).await;
    let open_pos = open.new_pos;
    let open_entry = open.new_entry;
    assert!(open_pos < 0.0);

    // Close (flat) at 45_000 — price fell, short profits.
    let close_req = base_req(open_pos, open_entry, "flat", 45_000.0);
    let close = sink.submit(close_req).await;

    assert_eq!(close.new_pos, 0.0, "position must be flat after close");
    let fp = close.fill_price.expect("fill price set on close");
    // With SlippageModel::None the fill price is exactly next_open.
    // realized = pos * (fill_price - entry)
    //          = open_pos * (45_000 - open_entry)
    // open_pos < 0, 45_000 < open_entry  =>  realized > 0
    let gross = open_pos * (fp - open_entry);
    let fee = close.fee.unwrap_or(0.0);
    let expected_net = gross - fee;
    assert!(
        (close.realized_pnl - expected_net).abs() < 1e-6,
        "realized_pnl {} != expected {expected_net}",
        close.realized_pnl,
    );
    assert!(close.realized_pnl > 0.0, "short on falling price must profit");
}

#[tokio::test]
async fn short_flat_realizes_loss_when_price_rises() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());

    let open = sink.submit(base_req(0.0, 0.0, "short_open", 50_000.0)).await;
    // Price rose: short is a loss.
    let close = sink
        .submit(base_req(open.new_pos, open.new_entry, "flat", 55_000.0))
        .await;

    assert!(close.realized_pnl < 0.0, "short on rising price must lose");
}

#[tokio::test]
async fn short_open_noop_when_already_short() {
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());

    let open = sink.submit(base_req(0.0, 0.0, "short_open", 50_000.0)).await;
    let pos_after_open = open.new_pos;

    // Already short — short_open is a no-op.
    let noop = sink
        .submit(base_req(pos_after_open, open.new_entry, "short_open", 49_000.0))
        .await;
    assert_eq!(noop.new_pos, pos_after_open);
    assert!(noop.fill_price.is_none(), "no-op must not produce a fill");
}

#[tokio::test]
async fn same_request_produces_byte_identical_results() {
    let req = base_req(0.0, 0.0, "short_open", 50_000.0);

    let mut s1 = SimulatedFills::new(EvalOnly::new_for_tests());
    let r1 = s1.submit(req.clone()).await;

    let mut s2 = SimulatedFills::new(EvalOnly::new_for_tests());
    let r2 = s2.submit(req.clone()).await;

    assert_eq!(r1.new_pos, r2.new_pos);
    assert_eq!(r1.new_entry, r2.new_entry);
    assert_eq!(r1.fill_price, r2.fill_price);
    assert_eq!(r1.fill_size, r2.fill_size);
    assert_eq!(r1.fee, r2.fee);
    assert_eq!(r1.realized_pnl, r2.realized_pnl);
}

#[tokio::test]
async fn look_ahead_guard_zero_latency_uses_next_open() {
    // With zero latency, fill price derives solely from next_open.
    let req = base_req(0.0, 0.0, "long_open", 100.0);
    let mut sink = SimulatedFills::new(EvalOnly::new_for_tests());
    let rec = sink.submit(req).await;

    // SlippageModel::None, spread 0 → fill_price == next_open.
    assert!(
        (rec.fill_price.unwrap() - 100.0).abs() < 1e-10,
        "zero latency must fill at next_open=100, got {}",
        rec.fill_price.unwrap(),
    );
}
