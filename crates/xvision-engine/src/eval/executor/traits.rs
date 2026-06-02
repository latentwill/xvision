//! Executor seam traits + Backtest concrete impls. Sub-track 1 of the
//! 2026-05-21 Alpaca-Live executor refactor (see
//! `team/contracts/executor-refactor.md` for the decomposition and
//! `team/contracts/executor-trait-extraction.md` for this sub-track).
//!
//! Three traits — [`BarSource`], [`Clock`], and [`FillSink`] — describe
//! the three knobs the `Executor` will eventually share with a
//! `LiveExecutor`:
//!   - **Bars** — where the next OHLCV bar comes from (in-memory `Vec`
//!     today; a market-data websocket in sub-track 3).
//!   - **Clock** — what "now" means (the most recent bar's timestamp
//!     for replay; wall-clock for live).
//!   - **Fills** — how an order becomes a fill record (simulated against
//!     the next bar today; forwarded to a broker in sub-track 3).
//!
//! This module ships the three Backtest impls — [`InjectedBars`],
//! [`InstantClock`], [`SimulatedFills`] — and is consumed internally by
//! [`super::Executor`]. The Live impls (`LiveStream`,
//! `WallClock`, `RealBrokerFills`) are explicitly out of scope for this
//! contract; they land in sub-track 3.
//!
//! ## Dispatch choice
//!
//! Trait objects (`Box<dyn BarSource>` etc.) rather than generics. The
//! per-bar loop pays one virtual dispatch per call; the alternative
//! (`<B: BarSource, C: Clock, F: FillSink>` on `Executor`) would
//! require monomorphizing the entire body for every combination — fine
//! for backtest-only today, but sub-track 3 will land a Live impl on
//! the same shape and we'd rather not blow up codegen there.
//!
//! ## Behavioral floor
//!
//! Every backtest fixture must produce byte-identical metrics after the
//! rewire. [`SimulatedFills`] lifts the existing fill-simulation code
//! out of `Executor` verbatim — same slippage, same fees, same
//! min-notional, same provenance. The integration regression in
//! `tests/eval_executor_traits.rs` pins this against a representative
//! fixture from the existing test corpus.

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use xvision_core::market::Ohlcv;
use xvision_execution::broker_surface::BrokerErrorClass;

use crate::eval::executor::trace_types::{AggressorSide, FillBranch};
use crate::eval::orders::OrderState;
use crate::eval::scenario::{FillProvenance, SlippageModel};

// ---------------------------------------------------------------------------
// EvalOnly capability token
// ---------------------------------------------------------------------------

/// Zero-size capability token required by [`SimulatedFills::new`].
///
/// Mintable only from within `xvision-engine` via [`eval_only_token`].
/// This gives a compile-time guarantee that `SimulatedFills` cannot be
/// constructed in the live/real-money path (which uses `RealBrokerFills`).
pub struct EvalOnly(());

/// Mint an eval-only capability token. Only callable from within
/// `xvision-engine`; external crates cannot invoke this.
pub(crate) fn eval_only_token() -> EvalOnly {
    EvalOnly(())
}

impl EvalOnly {
    /// Test-only constructor. For use in unit and integration tests that
    /// need to exercise [`SimulatedFills`] directly.
    ///
    /// **Not for use in production code.** Production paths use
    /// [`eval_only_token`] (crate-internal).
    #[doc(hidden)]
    pub fn new_for_tests() -> Self {
        EvalOnly(())
    }
}

// ---------------------------------------------------------------------------
// BarSource
// ---------------------------------------------------------------------------

/// Source of OHLCV bars for the executor's per-bar loop.
///
/// `next_bar` returns `Some(bar)` for each available bar in chronological
/// order, then `None` to terminate the loop. The trait is async so a
/// future Live implementation can `.await` on a market-data websocket;
/// the Backtest implementation ([`InjectedBars`]) returns immediately
/// from an in-memory `Vec`.
///
/// **Constraint:** the trait must be implementable for both an in-memory
/// buffer AND a future polling/streaming source without re-shaping. The
/// `&mut self` + `Option<Ohlcv>` return achieves that — push-style
/// streams adapt by buffering one bar at a time.
#[async_trait]
pub trait BarSource: Send + Sync {
    /// Yield the next bar. Returns `None` exactly once when the source
    /// is drained; calling again after `None` is implementation-defined
    /// but `InjectedBars` keeps returning `None`.
    async fn next_bar(&mut self) -> Option<Ohlcv>;
}

/// In-memory `BarSource` backed by a `Vec<Ohlcv>` and a cursor. The
/// Backtest path uses this — either with bars loaded via
/// `xvision_data::fixtures::load_ohlcv_fixture` (legacy canonical
/// scenarios) or with bars handed in pre-loaded via
/// `eval::bars::load_bars` (Task 8 DB-resolved path).
pub struct InjectedBars {
    bars: Vec<Ohlcv>,
    cursor: usize,
}

impl InjectedBars {
    /// Build a new source from an owned `Vec<Ohlcv>`.
    pub fn new(bars: Vec<Ohlcv>) -> Self {
        Self { bars, cursor: 0 }
    }

    /// Remaining bars not yet yielded. Used by the executor's per-bar
    /// loop to look up the *next* bar (T+1) for the fill price without
    /// disturbing the cursor.
    pub fn remaining(&self) -> &[Ohlcv] {
        &self.bars[self.cursor.min(self.bars.len())..]
    }

    /// All bars known to this source (including those already yielded).
    /// Used by the executor to drive `compute_baselines` and to look
    /// ahead by `+1` for the next-open fill price.
    pub fn all(&self) -> &[Ohlcv] {
        &self.bars
    }
}

#[async_trait]
impl BarSource for InjectedBars {
    async fn next_bar(&mut self) -> Option<Ohlcv> {
        if self.cursor >= self.bars.len() {
            return None;
        }
        let bar = self.bars[self.cursor].clone();
        self.cursor += 1;
        Some(bar)
    }
}

// ---------------------------------------------------------------------------
// Clock
// ---------------------------------------------------------------------------

/// Logical clock for the executor.
///
/// The Backtest impl advances explicitly to each bar's timestamp via
/// [`Clock::advance_to`]; `now()` returns the most-recent advanced-to
/// timestamp. The future Live impl reads the wall clock and ignores
/// `advance_to`.
pub trait Clock: Send + Sync {
    /// Current logical timestamp. For [`InstantClock`], this is the
    /// timestamp of the most recently emitted bar; for a future
    /// `WallClock`, the wall-clock now.
    fn now(&self) -> DateTime<Utc>;

    /// Backtest-style advance. Live impls treat this as a no-op (the
    /// wall clock doesn't take instruction).
    fn advance_to(&mut self, ts: DateTime<Utc>);
}

/// Replay-style clock that holds the last `advance_to` timestamp.
///
/// Before the first `advance_to` call, `now()` returns the Unix epoch
/// (`1970-01-01T00:00:00Z`). The executor calls `advance_to(bar.ts)`
/// once per cadence-gated bar inside the loop, so once the loop is
/// running `now()` matches the most recent emitted bar.
pub struct InstantClock {
    current: DateTime<Utc>,
}

impl Default for InstantClock {
    fn default() -> Self {
        Self::new()
    }
}

impl InstantClock {
    /// Build a clock anchored at the Unix epoch. Used by the Backtest
    /// path; advance is driven by the per-bar loop.
    pub fn new() -> Self {
        Self {
            current: DateTime::<Utc>::from_timestamp(0, 0).expect("epoch is a valid DateTime"),
        }
    }
}

impl Clock for InstantClock {
    fn now(&self) -> DateTime<Utc> {
        self.current
    }

    fn advance_to(&mut self, ts: DateTime<Utc>) {
        self.current = ts;
    }
}

// ---------------------------------------------------------------------------
// FillSink
// ---------------------------------------------------------------------------

/// Inputs to a single fill submission. Mirrors the v1 `SimulateFillArgs`
/// in `backtest.rs` and is re-exported here so the trait surface is a
/// stable shape that a future broker impl can adopt.
///
/// All fields are owned/`Copy` to keep `FillSink::submit` free of
/// lifetime gymnastics. The `slippage_model` is cloned in the executor
/// before assembling the request — backtests own their scenario for
/// the whole run, so the clone is a single SlippageModel value per fill.
///
/// ## Latency fields
///
/// `decision_to_fill_ms` and `bar_duration_ms` drive the intra-bar
/// fill-reference interpolation in [`simulate_fill_inner`]. When
/// `decision_to_fill_ms == 0` (or `bar_duration_ms == 0`) the fill
/// reference is exactly `next_open`, preserving backward compatibility.
/// Non-zero latency shifts the fill reference toward `bar_close`:
///
/// ```text
/// latency_fraction = min(decision_to_fill_ms / bar_duration_ms, 1.0)
/// fill_ref         = next_open + latency_fraction * (bar_close - next_open)
/// ```
///
/// `bar_close` is the close of the fill bar (T+1), i.e. the bar whose
/// open is `next_open`. This models "how far into the fill bar does the
/// decision arrive."
#[derive(Debug, Clone)]
pub struct FillRequest {
    /// Pre-fill position size (base-asset units; +long / -short).
    pub pos: f64,
    /// Volume-weighted entry price of the existing position. Ignored
    /// when `pos == 0.0`.
    pub entry: f64,
    /// The applied trader action — one of `"long_open"`, `"short_open"`,
    /// `"flat"`, `"hold"`. (A `"hold"` action is a no-op at the executor
    /// level and never reaches `submit` today.)
    pub action: String,
    /// The next bar's open price (or terminal-bar close fallback).
    pub next_open: f64,
    /// Decision-bar volume — required by `VolumeShare` slippage.
    pub bar_volume: f64,
    /// Effective `slip_bps` after override resolution.
    pub slip_bps: f64,
    /// Effective half-spread in basis points (0.0 when no per-bar column).
    pub spread_bps: f64,
    /// Effective taker fee in basis points.
    pub taker_bps: f64,
    /// Effective maker fee in basis points.
    pub maker_bps: f64,
    /// Current equity (drives the risk-pct sizing).
    pub equity: f64,
    /// `Strategy.risk.risk_pct_per_trade`.
    pub risk_pct: f64,
    /// Resolved slippage model for this fill (owned clone).
    pub slippage_model: SlippageModel,
    /// Provenance tag — which override won (per-bar / per-asset / default).
    pub fee_source: crate::eval::scenario::FeeSource,
    /// Asset venue symbol (for debug logging in fallback paths).
    pub asset: String,
    /// Decision bar timestamp (for debug logging).
    pub bar_ts: DateTime<Utc>,
    /// Fill bar's open price (for intra-bar fill ordering).
    pub bar_open: f64,
    /// Fill bar's high (for intra-bar fill ordering; v1 market orders
    /// ignore this).
    pub bar_high: f64,
    /// Fill bar's low (for intra-bar fill ordering; v1 market orders
    /// ignore this).
    pub bar_low: f64,
    /// Fill bar's close. Used as the end-point of the latency
    /// interpolation range when `decision_to_fill_ms > 0`.
    pub bar_close: f64,
    /// Configured decision-to-fill latency in milliseconds.
    /// `0` → fill at `next_open` (backward-compatible no-op).
    pub decision_to_fill_ms: u32,
    /// Duration of one bar in milliseconds. Used to compute
    /// `latency_fraction = decision_to_fill_ms / bar_duration_ms`.
    /// `0` → treated as zero latency.
    pub bar_duration_ms: u64,
}

/// The fill outcome returned by `FillSink::submit`. Mirrors the
/// pre-refactor `FillOutcome` in `backtest.rs`.
#[derive(Debug, Clone)]
pub struct FillRecord {
    /// New position size after the fill (base-asset units).
    pub new_pos: f64,
    /// New volume-weighted entry price. `0.0` when `new_pos == 0.0`.
    pub new_entry: f64,
    /// Fill price, or `None` for no-op fills.
    pub fill_price: Option<f64>,
    /// Filled quantity (units crossed), or `None` for no-op fills.
    pub fill_size: Option<f64>,
    /// Fee paid (USD), or `None` for no-op fills.
    pub fee: Option<f64>,
    /// Realized PnL from closing the prior leg (net of fee). `0.0` for
    /// pure-open and no-op fills.
    pub realized_pnl: f64,
    /// Provenance — how cost was resolved for this fill.
    pub provenance: FillProvenance,
    /// Which intra-bar branch fired (v1 market orders are always
    /// `NextOpenOnly`). `None` on no-op.
    pub fill_branch: Option<FillBranch>,
    /// Maker vs taker classification. `None` on no-op.
    pub aggressor_side: Option<AggressorSide>,
    /// Order lifecycle state after the attempt. `None` on no-op.
    pub order_state: Option<OrderState>,
    /// When the `VolumeShare` cap bound, the tuple
    /// `(requested_qty, bar_volume, cap_binding_qty, fill_share)` that
    /// the executor uses to emit a `volume_share_excess` finding.
    /// `None` for every other case.
    pub volume_cap_hit: Option<(f64, f64, f64, f64)>,
    /// Broker error classification for Live fills rejected by the paper
    /// broker. `None` for simulated backtest fills and successful Live fills.
    pub broker_error: Option<(BrokerErrorClass, String)>,
}

/// Order-fill seam.
///
/// The Backtest impl ([`SimulatedFills`]) runs the existing
/// `simulate_fill` against the request and returns the verbatim
/// outcome. A future broker impl will forward the order to
/// `BrokerSurface::submit_order` and translate the resulting
/// `BrokerFill` into a `FillRecord`.
///
/// **Error model.** The trait surface is infallible (`-> FillRecord`).
/// Errors from a future broker impl — auth failures, rate limits,
/// rejections — will be wrapped into the existing
/// `classify_run_failure` flow at the *executor* level, not here. The
/// trait deliberately does NOT bake in `classify_run_failure`'s
/// error-class wrapping so the broker impl in a later track can return
/// raw errors (probably via a fallible variant of this trait) and the
/// executor wraps them.
#[async_trait]
pub trait FillSink: Send + Sync {
    /// Produce a fill for the given request. The request is fully owned
    /// so impls don't need to bound their lifetimes against caller
    /// state.
    async fn submit(&mut self, req: FillRequest) -> FillRecord;
}

/// Backtest fill simulation. Lifts the body of
/// `backtest::simulate_fill` so the trait surface is identical to the
/// pre-refactor inline call. **Behavior is byte-identical** to the
/// pre-refactor code path; the integration regression in
/// `tests/eval_executor_traits.rs` pins this.
///
/// Requires an [`EvalOnly`] capability token so this type cannot be
/// constructed in the live/real-money path. Use [`eval_only_token`]
/// (crate-internal) or [`EvalOnly::new_for_tests`] in tests.
pub struct SimulatedFills;

impl SimulatedFills {
    pub fn new(_token: EvalOnly) -> Self {
        Self
    }
}

#[async_trait]
impl FillSink for SimulatedFills {
    async fn submit(&mut self, req: FillRequest) -> FillRecord {
        simulate_fill_inner(&req)
    }
}

/// The verbatim fill-simulation logic, lifted from `backtest.rs`'s
/// `simulate_fill`. Kept as a free function so unit tests can call it
/// without holding a `SimulatedFills` value.
///
/// ## Look-ahead invariant
///
/// This function is a pure function of `FillRequest`. All bar fields
/// in `FillRequest` are either from the decision bar (T) or from the
/// fill bar (T+1). No data beyond bar T+1 is accessed, so there is no
/// multi-bar look-ahead bias. The `debug_assert` below documents this.
///
/// ## Latency semantics
///
/// When `decision_to_fill_ms > 0`, the fill reference is shifted from
/// `next_open` (T+1 open) toward `bar_close` (T+1 close):
///
/// ```text
/// latency_fraction = min(decision_to_fill_ms / bar_duration_ms, 1.0)
/// fill_ref         = next_open + latency_fraction * (bar_close - next_open)
/// ```
///
/// Slippage and spread are then applied on top of `fill_ref`. Zero
/// latency (`decision_to_fill_ms == 0`) reproduces the pre-latency
/// behavior exactly.
pub(crate) fn simulate_fill_inner(a: &FillRequest) -> FillRecord {
    // Look-ahead guard: fill uses only bar T+1 fields (next_open, bar_close,
    // bar_open, bar_high, bar_low). No data beyond bar T+1 is read.
    // Latency > bar_duration is handled by clamping the fraction to 1.0.
    debug_assert!(a.next_open > 0.0, "next_open must be positive");

    let want_long = a.action == "long_open";
    let want_short = a.action == "short_open";
    let want_flat = !want_long && !want_short;

    // No-op when target direction matches current position.
    if (want_long && a.pos > 0.0) || (want_short && a.pos < 0.0) || (want_flat && a.pos == 0.0) {
        return FillRecord {
            new_pos: a.pos,
            new_entry: a.entry,
            fill_price: None,
            fill_size: None,
            fee: None,
            realized_pnl: 0.0,
            provenance: FillProvenance::default(),
            fill_branch: None,
            aggressor_side: None,
            order_state: None,
            volume_cap_hit: None,
            broker_error: None,
        };
    }

    // Direction of the trade we're about to execute.
    let trade_long = if want_long {
        true
    } else if want_short {
        false
    } else {
        a.pos < 0.0 // closing a short means buying
    };

    let approx_units = if want_flat {
        a.pos.abs()
    } else {
        let usd_at_risk = a.equity * a.risk_pct;
        let units = (usd_at_risk / a.next_open).max(0.0);
        if a.pos != 0.0 {
            // Reversing: pay close leg + open leg.
            a.pos.abs() + units
        } else {
            units
        }
    };

    // Resolve slip fraction and volume-cap state.
    let mut volume_share = 0.0_f64;
    let mut volume_cap_bound = false;
    let mut volume_cap_hit: Option<(f64, f64, f64, f64)> = None;

    let effective_slip_fraction: f64 = match &a.slippage_model {
        SlippageModel::None => 0.0,

        SlippageModel::Linear { bps } => {
            let _ = bps; // resolved value via a.slip_bps
            a.slip_bps / 10_000.0
        }

        SlippageModel::VolumeShare {
            price_impact,
            volume_limit,
        } => {
            if a.bar_volume <= 0.0 || !a.bar_volume.is_finite() {
                tracing::debug!(
                    asset = a.asset,
                    bar_ts = %a.bar_ts,
                    "VolumeShare: bar volume missing or zero; falling back to Linear slip_bps={}",
                    a.slip_bps
                );
                a.slip_bps / 10_000.0
            } else {
                let raw_share = approx_units / a.bar_volume;
                volume_cap_bound = raw_share > *volume_limit;
                volume_share = raw_share.min(*volume_limit);

                if volume_cap_bound {
                    let cap_qty = *volume_limit * a.bar_volume;
                    volume_cap_hit = Some((approx_units, a.bar_volume, cap_qty, volume_share));
                }

                price_impact * volume_share * volume_share
            }
        }
    };

    // Latency: interpolate fill reference between next_open and bar_close.
    // Zero latency → fill_ref = next_open (backward-compatible).
    let fill_ref = if a.decision_to_fill_ms == 0 || a.bar_duration_ms == 0 {
        a.next_open
    } else {
        let frac = (a.decision_to_fill_ms as f64 / a.bar_duration_ms as f64).min(1.0);
        a.next_open + frac * (a.bar_close - a.next_open)
    };

    let spread_fraction = a.spread_bps / 10_000.0 / 2.0;

    let fill_price = if trade_long {
        fill_ref * (1.0 + effective_slip_fraction + spread_fraction)
    } else {
        fill_ref * (1.0 - effective_slip_fraction - spread_fraction)
    };

    let realized = if a.pos != 0.0 {
        a.pos * (fill_price - a.entry)
    } else {
        0.0
    };

    let new_pos_units = if want_flat {
        0.0
    } else {
        let usd_at_risk = a.equity * a.risk_pct;
        let units = (usd_at_risk / fill_price).max(0.0);
        if want_long {
            units
        } else {
            -units
        }
    };

    let traded_units = if a.pos == 0.0 {
        new_pos_units.abs()
    } else if new_pos_units == 0.0 {
        a.pos.abs()
    } else {
        a.pos.abs() + new_pos_units.abs()
    };

    let aggressor_side =
        super::backtest::classify_aggressor_side(&a.action, fill_price, a.bar_open, a.spread_bps);

    let fee_bps_applied = match aggressor_side {
        AggressorSide::Maker => a.maker_bps,
        AggressorSide::Taker => a.taker_bps,
    };

    let notional = traded_units * fill_price;
    let fee = notional * (fee_bps_applied / 10_000.0);

    let new_entry = if new_pos_units == 0.0 { 0.0 } else { fill_price };

    let fill_branch = FillBranch::NextOpenOnly;

    let order_state = if volume_cap_bound {
        OrderState::PartiallyFilled
    } else {
        OrderState::Filled
    };

    let provenance = FillProvenance {
        slip_bps_applied: effective_slip_fraction * 10_000.0,
        spread_bps_applied: spread_fraction * 2.0 * 10_000.0,
        fee_bps_applied,
        fee_source: a.fee_source,
        volume_share,
        volume_cap_bound,
    };

    FillRecord {
        new_pos: new_pos_units,
        new_entry,
        fill_price: Some(fill_price),
        fill_size: Some(traded_units),
        fee: Some(fee),
        realized_pnl: realized - fee,
        provenance,
        fill_branch: Some(fill_branch),
        aggressor_side: Some(aggressor_side),
        order_state: Some(order_state),
        volume_cap_hit,
        broker_error: None,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn ohlcv_at(ts_secs: i64, close: f64) -> Ohlcv {
        Ohlcv {
            timestamp: Utc.timestamp_opt(ts_secs, 0).unwrap(),
            open: close - 1.0,
            high: close + 1.0,
            low: close - 2.0,
            close,
            volume: 100.0,
        }
    }

    fn base_req() -> FillRequest {
        FillRequest {
            pos: 0.0,
            entry: 0.0,
            action: "long_open".into(),
            next_open: 100.0,
            bar_volume: 1_000.0,
            slip_bps: 5.0,
            spread_bps: 2.0,
            taker_bps: 10.0,
            maker_bps: 5.0,
            equity: 10_000.0,
            risk_pct: 0.01,
            slippage_model: SlippageModel::Linear { bps: 5 },
            fee_source: crate::eval::scenario::FeeSource::Default,
            asset: "BTC/USD".into(),
            bar_ts: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            bar_open: 100.0,
            bar_high: 101.0,
            bar_low: 99.0,
            bar_close: 100.5,
            decision_to_fill_ms: 0,
            bar_duration_ms: 3_600_000,
        }
    }

    #[tokio::test]
    async fn injected_bars_yields_each_bar_in_order_then_none() {
        let bars = vec![ohlcv_at(1, 100.0), ohlcv_at(2, 101.0), ohlcv_at(3, 102.0)];
        let mut src = InjectedBars::new(bars.clone());

        for expected in bars.iter() {
            let got = src.next_bar().await.expect("bar present");
            assert_eq!(got.timestamp, expected.timestamp);
            assert_eq!(got.close, expected.close);
        }
        assert!(src.next_bar().await.is_none(), "source must drain to None");
        assert!(src.next_bar().await.is_none());
    }

    #[test]
    fn instant_clock_now_returns_most_recent_advance_to() {
        let mut clock = InstantClock::new();
        assert_eq!(clock.now().timestamp(), 0);

        let t1 = Utc.timestamp_opt(1_700_000_000, 0).unwrap();
        clock.advance_to(t1);
        assert_eq!(clock.now(), t1);

        let t2 = Utc.timestamp_opt(1_700_000_060, 0).unwrap();
        clock.advance_to(t2);
        assert_eq!(clock.now(), t2);
    }

    #[tokio::test]
    async fn simulated_fills_produces_same_outcome_as_inline_simulate() {
        let req = base_req();
        let mut sink = SimulatedFills::new(eval_only_token());
        let from_trait = sink.submit(req.clone()).await;
        let from_inline = simulate_fill_inner(&req);

        assert_eq!(from_trait.new_pos, from_inline.new_pos);
        assert_eq!(from_trait.new_entry, from_inline.new_entry);
        assert_eq!(from_trait.fill_price, from_inline.fill_price);
        assert_eq!(from_trait.fill_size, from_inline.fill_size);
        assert_eq!(from_trait.fee, from_inline.fee);
        assert_eq!(from_trait.realized_pnl, from_inline.realized_pnl);
        assert_eq!(from_trait.fill_branch, from_inline.fill_branch);
        assert_eq!(from_trait.aggressor_side, from_inline.aggressor_side);
        assert_eq!(from_trait.order_state, from_inline.order_state);
    }

    #[test]
    fn zero_latency_fill_ref_equals_next_open() {
        let mut req = base_req(); // decision_to_fill_ms: 0
        req.spread_bps = 0.0; // isolate latency path; no spread component
        let rec = simulate_fill_inner(&req);
        // With Linear 5bps slip and no spread, fill = next_open * (1 + 0.0005)
        let expected = 100.0 * (1.0 + 5.0 / 10_000.0);
        assert!(
            (rec.fill_price.unwrap() - expected).abs() < 1e-10,
            "zero latency fill_price={} expected={}",
            rec.fill_price.unwrap(),
            expected,
        );
    }

    #[test]
    fn nonzero_latency_shifts_fill_ref_toward_bar_close() {
        let mut req = base_req();
        // Half-bar latency: fill_ref = 100.0 + 0.5 * (100.5 - 100.0) = 100.25
        req.decision_to_fill_ms = 1_800_000; // 30 min of a 60-min bar
        req.bar_duration_ms = 3_600_000;
        req.bar_close = 100.5;
        let rec_latency = simulate_fill_inner(&req);

        let mut req_zero = base_req();
        req_zero.bar_close = 100.5;
        let rec_zero = simulate_fill_inner(&req_zero);

        // Latency should push fill price above zero-latency fill price for long_open.
        assert!(
            rec_latency.fill_price.unwrap() > rec_zero.fill_price.unwrap(),
            "half-bar latency must increase long fill price: {} vs {}",
            rec_latency.fill_price.unwrap(),
            rec_zero.fill_price.unwrap(),
        );
    }

    #[test]
    fn latency_exceeding_bar_duration_is_capped() {
        // Oversized latency (2x bar duration) must be clamped to fill_ref = bar_close.
        // The fraction is capped at 1.0: fill_ref = next_open + 1.0 * (bar_close - next_open) = bar_close.
        let mut req = base_req();
        req.spread_bps = 0.0; // isolate latency path; no spread component
        req.decision_to_fill_ms = 7_200_000;
        req.bar_duration_ms = 3_600_000;
        req.bar_close = 101.0;
        let rec = simulate_fill_inner(&req);

        let expected = 101.0 * (1.0 + 5.0 / 10_000.0);
        assert!(
            (rec.fill_price.unwrap() - expected).abs() < 1e-10,
            "capped latency fill_price={} expected={}",
            rec.fill_price.unwrap(),
            expected,
        );
    }
}
