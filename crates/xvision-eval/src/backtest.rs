//! Phase 6.4 — in-process backtest simulator.
//!
//! `BacktestExecutor` implements the `BacktestExecutor` trait from `xvision-execution`
//! so Phase 8's harness can swap it in transparently. The sim is driven forward
//! in time by the harness via `tick(next_bar)`, which advances the clock, marks
//! open positions, and fires stop/target orders when bars cross the levels.
//!
//! ## Tier 1 fix #3 compliance
//! NAV, open positions, daily PnL window, loss streak, and 14-bar Wilder ATR
//! are all tracked inside `BacktestState`.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use xvision_core::{Action, AssetSymbol, Direction, OpenPosition, PortfolioState, RiskDecision};
use xvision_execution::{ExecutionReceipt, Executor, ExecutorError};

// ---------------------------------------------------------------------------
// Public value types
// ---------------------------------------------------------------------------

/// One OHLCV bar fed to the simulator via `tick()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarketBar {
    pub timestamp: DateTime<Utc>,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Realized PnL for one simulator day (indexed by `day_index`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyPnl {
    pub day_index: u32,
    pub realised_usd: f64,
}

/// Returned by `tick()` describing auto-fills and day-rollover info.
#[derive(Debug, Clone)]
pub struct TickReport {
    /// Receipts for any stop-loss or take-profit orders that auto-fired during
    /// this bar (zero or more).
    pub auto_filled_receipts: Vec<ExecutionReceipt>,
    /// True when the bar's timestamp crossed a UTC midnight boundary.
    pub day_rollover: bool,
    /// Realized PnL for the day that just closed (populated only when
    /// `day_rollover` is true, otherwise 0.0).
    pub day_pnl: f64,
}

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Static configuration for one backtest run. F18 cascade: `instrument` is
/// removed — `submit()` routes per `TraderDecision.asset`. The runner is
/// asset-agnostic; multiple positions keyed by asset coexist in the
/// portfolio.
#[derive(Debug, Clone)]
pub struct BacktestConfig {
    /// Starting equity in USD.
    pub initial_equity_usd: f64,
    /// Round-trip fee in basis points (entry + exit combined).
    /// Default: 10 bps (5 entry + 5 exit) — conservative for crypto perps.
    pub fee_bps: u32,
    /// Slippage expressed as a fraction of ATR per market order.
    /// Default: 0.10 — move price by 10% of ATR against the taker on entry/exit.
    pub slippage_atr_frac: f64,
    /// Rolling window size for `realised_pnl_history`. Default: 30 days.
    pub max_history_days: usize,
}

impl Default for BacktestConfig {
    fn default() -> Self {
        Self {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.10,
            max_history_days: 30,
        }
    }
}

// ---------------------------------------------------------------------------
// Mutable state
// ---------------------------------------------------------------------------

/// All mutable state for one backtest run. Wrapped in `Arc<Mutex<…>>` inside
/// `BacktestExecutor` because the `BacktestExecutor` trait methods take `&self`.
#[derive(Debug)]
pub struct BacktestState {
    /// Current portfolio snapshot (equity, open positions, etc.).
    pub portfolio: PortfolioState,
    /// Most recent bar; advanced by `tick()`.
    pub current_bar: MarketBar,
    /// Rolling ring of realized daily PnL. Capped at `BacktestConfig::max_history_days`.
    pub realised_pnl_history: VecDeque<DailyPnl>,
    /// Number of consecutive days with negative realized PnL.
    pub loss_streak: u32,
    /// Current 14-bar Wilder ATR estimate. During warmup (< 14 bars seen) this
    /// is the simple average of true ranges seen so far — good-enough for fill
    /// price modelling even on the first bars.
    pub recent_atr: f64,
    /// Days elapsed since strategy start.
    pub day_index: u32,
    /// Simulator's current time (equals the timestamp of the current bar).
    pub now: DateTime<Utc>,
    /// Chronological log of all fills (entries, exits, auto-fills).
    pub fills_log: Vec<ExecutionReceipt>,

    // --- private bookkeeping ---
    /// Running sum of true ranges for Wilder ATR warmup (first 13 bars).
    atr_warmup_sum: f64,
    /// How many bars have been processed (gates Wilder vs simple-avg ATR).
    bar_count: u32,
    /// Previous bar's close, needed for true-range calculation.
    prev_close: f64,
    /// Realized PnL accumulated in the current (not yet rolled) UTC day.
    current_day_pnl: f64,
    /// Monotonically increasing fill sequence number, per day bucket.
    fill_seq: u32,
}

impl BacktestState {
    fn new(config: &BacktestConfig, opening_bar: MarketBar) -> Self {
        let now = opening_bar.timestamp;
        let portfolio = PortfolioState {
            equity_usd: config.initial_equity_usd,
            realized_pnl_today_usd: 0.0,
            day_index: 0,
            open_positions: std::collections::BTreeMap::new(),
            as_of: now,
        };
        // Seed ATR with the opening bar's range as a single-sample estimate.
        let initial_range = opening_bar.high - opening_bar.low;
        Self {
            portfolio,
            current_bar: opening_bar.clone(),
            realised_pnl_history: VecDeque::new(),
            loss_streak: 0,
            recent_atr: initial_range.max(1.0), // guard div-by-zero when close≈open
            day_index: 0,
            now,
            fills_log: Vec::new(),
            atr_warmup_sum: initial_range,
            bar_count: 1,
            prev_close: opening_bar.close,
            current_day_pnl: 0.0,
            fill_seq: 0,
        }
    }

    /// Update Wilder ATR with the new bar.
    /// For the first 14 bars: simple average of true ranges (standard warmup).
    /// From bar 15 onward: `ATR = ATR_prev * (13/14) + TR * (1/14)`.
    fn update_atr(&mut self, bar: &MarketBar) {
        let tr = {
            let hl = bar.high - bar.low;
            let hc = (bar.high - self.prev_close).abs();
            let lc = (bar.low - self.prev_close).abs();
            hl.max(hc).max(lc)
        };
        const PERIOD: u32 = 14;
        if self.bar_count < PERIOD {
            self.atr_warmup_sum += tr;
            self.bar_count += 1;
            self.recent_atr = self.atr_warmup_sum / self.bar_count as f64;
        } else {
            self.bar_count += 1;
            // Wilder smoothing: ATR_new = ATR_prev × (N-1)/N + TR × 1/N
            self.recent_atr = self.recent_atr * (PERIOD as f64 - 1.0) / PERIOD as f64 + tr / PERIOD as f64;
        }
        self.prev_close = bar.close;
    }

    /// Compute the fill price for a market order.
    /// `slippage_dir`: +1.0 for buys (pay up), -1.0 for sells (get less).
    fn fill_price(&self, close: f64, slippage_dir: f64, slippage_atr_frac: f64) -> f64 {
        close * (1.0 + slippage_dir * slippage_atr_frac * self.recent_atr / close)
    }

    /// Deduct entry-leg fees from equity.
    fn apply_entry_fee(&mut self, notional: f64, fee_bps: u32) {
        let fee = notional * (fee_bps as f64 / 2.0) / 10_000.0;
        self.portfolio.equity_usd -= fee;
    }

    /// Next venue_order_id tag.
    fn next_order_id(&mut self) -> String {
        let id = format!("bt-{}-{}", self.day_index, self.fill_seq);
        self.fill_seq += 1;
        id
    }

    /// Realize a close and update PnL / equity.
    /// Returns `(fill_price, realised_pnl_usd)`.
    fn realize_close(
        &mut self,
        pos: &OpenPosition,
        config: &BacktestConfig,
        override_price: Option<f64>,
    ) -> (f64, f64) {
        let slippage_dir = match pos.direction {
            Direction::Long => -1.0, // exit long: sell, price moves against us
            Direction::Short => 1.0, // exit short: buy, price moves against us
            Direction::Flat => 0.0,
        };
        let fill_px = override_price.unwrap_or_else(|| {
            self.fill_price(self.current_bar.close, slippage_dir, config.slippage_atr_frac)
        });

        // Notional at entry for fee calculation (conservative: use entry size × fill price)
        let notional = self.portfolio.equity_usd * pos.size_bps as f64 / 10_000.0;
        let exit_fee = notional * (config.fee_bps as f64 / 2.0) / 10_000.0;
        self.portfolio.equity_usd -= exit_fee;

        // PnL = direction_sign × (exit - entry) × notional / entry
        // Expressed in USD: size is a fraction of NAV at entry, but we track
        // it in bps, so: units = notional / entry_price; pnl = units × (exit - entry)
        let units = notional / pos.entry_price;
        let direction_sign = match pos.direction {
            Direction::Long => 1.0,
            Direction::Short => -1.0,
            Direction::Flat => 0.0,
        };
        let realised_pnl = direction_sign * (fill_px - pos.entry_price) * units - exit_fee;

        self.portfolio.equity_usd += realised_pnl + exit_fee; // equity already had exit_fee removed; add pnl
                                                              // Correct: equity change = pnl (fees already deducted above)
                                                              // Actually let's be explicit:
                                                              //   equity_usd -= exit_fee  (done above)
                                                              //   equity_usd += pnl_gross (where pnl_gross = direction_sign * (fill_px - entry) * units)
                                                              // We'll undo the double-count here:
        self.portfolio.equity_usd -= realised_pnl + exit_fee; // undo double-add
        let pnl_gross = direction_sign * (fill_px - pos.entry_price) * units;
        self.portfolio.equity_usd += pnl_gross;

        let realised_net = pnl_gross - exit_fee;
        self.current_day_pnl += realised_net;
        self.portfolio.realized_pnl_today_usd += realised_net;

        (fill_px, realised_net)
    }
}

// ---------------------------------------------------------------------------
// BacktestExecutor
// ---------------------------------------------------------------------------

/// Stateful in-process backtest simulator. Implements `BacktestExecutor` so it can be
/// swapped in wherever a live executor is expected (Phase 8 harness).
pub struct BacktestExecutor {
    state: Arc<Mutex<BacktestState>>,
    config: BacktestConfig,
}

impl BacktestExecutor {
    /// Create a new simulator from `config` and the first OHLCV bar.
    pub fn new(config: BacktestConfig, opening_bar: MarketBar) -> Self {
        let state = BacktestState::new(&config, opening_bar);
        Self {
            state: Arc::new(Mutex::new(state)),
            config,
        }
    }

    /// Advance the simulator to the next OHLCV bar.
    ///
    /// 1. Checks all open positions against the new bar's high/low for
    ///    stop-loss and take-profit triggers; auto-fills any that fire.
    /// 2. Marks remaining positions to `next.close`.
    /// 3. Updates Wilder ATR.
    /// 4. Advances the clock; rolls the day when `next.timestamp` crosses UTC midnight.
    pub fn tick(&self, next: MarketBar) -> Result<TickReport, ExecutorError> {
        let mut st = self
            .state
            .lock()
            .map_err(|_| ExecutorError::Internal("mutex poisoned".into()))?;

        let mut auto_fills: Vec<ExecutionReceipt> = Vec::new();
        let prev_day = date_of(&st.now);
        let next_day = date_of(&next.timestamp);
        let day_rollover = next_day != prev_day;

        // --- stop / take-profit scanning ---
        let assets: Vec<AssetSymbol> = st.portfolio.open_positions.keys().cloned().collect();
        for asset in assets {
            let pos = match st.portfolio.open_positions.get(&asset) {
                Some(p) => p.clone(),
                None => continue,
            };

            // Determine the stop/target trigger price
            let (stop_px, target_px) = sl_tp_prices(&pos);

            let (triggered_at, is_tp) = match pos.direction {
                Direction::Long => {
                    // TP fires when high >= target; SL fires when low <= stop
                    let tp_hit = next.high >= target_px;
                    let sl_hit = next.low <= stop_px;
                    if tp_hit && sl_hit {
                        // Both hit — use whichever is worse (stop), conservative
                        (Some(stop_px), false)
                    } else if tp_hit {
                        (Some(target_px), true)
                    } else if sl_hit {
                        (Some(stop_px), false)
                    } else {
                        (None, false)
                    }
                }
                Direction::Short => {
                    // TP fires when low <= target; SL fires when high >= stop
                    let tp_hit = next.low <= target_px;
                    let sl_hit = next.high >= stop_px;
                    if sl_hit && tp_hit {
                        (Some(stop_px), false)
                    } else if tp_hit {
                        (Some(target_px), true)
                    } else if sl_hit {
                        (Some(stop_px), false)
                    } else {
                        (None, false)
                    }
                }
                Direction::Flat => (None, false),
            };

            if let Some(fill_at_px) = triggered_at {
                // Auto-fill at the level price (conservative: not worse than the level)
                let (fill_px, _realised) = st.realize_close(&pos, &self.config, Some(fill_at_px));
                st.portfolio.open_positions.remove(&asset);

                let order_id = st.next_order_id();
                let note = if is_tp {
                    "take-profit auto-fill"
                } else {
                    "stop-loss auto-fill"
                };
                let receipt = ExecutionReceipt {
                    cycle_id: Uuid::nil(), // auto-fills have no originating cycle_id
                    venue: "backtest".into(),
                    venue_order_id: order_id,
                    asset,
                    filled_size_bps: pos.size_bps,
                    avg_fill_price: fill_px,
                    fee_bps: self.config.fee_bps / 2,
                    submitted_at: next.timestamp,
                    filled_at: Some(next.timestamp),
                    note: Some(note.into()),
                };
                st.fills_log.push(receipt.clone());
                auto_fills.push(receipt);
            }
        }

        // --- mark open positions to new close ---
        for pos in st.portfolio.open_positions.values_mut() {
            pos.mark_price = next.close;
        }

        // --- update ATR ---
        st.update_atr(&next);

        // --- advance clock ---
        st.now = next.timestamp;
        st.current_bar = next;
        st.portfolio.as_of = st.now;

        // --- day rollover ---
        let mut day_pnl = 0.0;
        if day_rollover {
            day_pnl = st.current_day_pnl;
            let entry = DailyPnl {
                day_index: st.day_index,
                realised_usd: day_pnl,
            };
            st.realised_pnl_history.push_back(entry);
            while st.realised_pnl_history.len() > self.config.max_history_days {
                st.realised_pnl_history.pop_front();
            }
            if day_pnl < 0.0 {
                st.loss_streak += 1;
            } else {
                st.loss_streak = 0;
            }
            st.current_day_pnl = 0.0;
            st.portfolio.realized_pnl_today_usd = 0.0;
            st.day_index += 1;
            st.portfolio.day_index = st.day_index;
            st.fill_seq = 0;
        }

        Ok(TickReport {
            auto_filled_receipts: auto_fills,
            day_rollover,
            day_pnl,
        })
    }

    /// Current portfolio snapshot (lock-free copy).
    pub fn portfolio_snapshot(&self) -> PortfolioState {
        // Best-effort: if lock is poisoned return an empty portfolio rather than panic.
        match self.state.lock() {
            Ok(st) => st.portfolio.clone(),
            Err(poisoned) => poisoned.into_inner().portfolio.clone(),
        }
    }

    /// Clone of all fills logged so far.
    pub fn fills_log(&self) -> Vec<ExecutionReceipt> {
        match self.state.lock() {
            Ok(st) => st.fills_log.clone(),
            Err(poisoned) => poisoned.into_inner().fills_log.clone(),
        }
    }
}

#[async_trait]
impl Executor for BacktestExecutor {
    async fn submit(&self, decision: &RiskDecision) -> Result<ExecutionReceipt, ExecutorError> {
        let td = match decision.effective() {
            Some(d) => d.clone(),
            None => {
                return Err(ExecutorError::NotActionable(
                    "vetoed decision forwarded to executor".into(),
                ))
            }
        };

        let asset = td.asset;

        match td.action {
            Action::Flat => {
                return Err(ExecutorError::NotActionable(
                    "Flat action: caller should call close_position".into(),
                ))
            }
            Action::Close => return self.close_position(asset).await,
            Action::Buy | Action::Sell => {}
        }

        let mut st = self
            .state
            .lock()
            .map_err(|_| ExecutorError::Internal("mutex poisoned".into()))?;

        let slippage_dir = if td.action == Action::Buy { 1.0 } else { -1.0 };
        let fill_px = st.fill_price(st.current_bar.close, slippage_dir, self.config.slippage_atr_frac);

        let notional = st.portfolio.equity_usd * td.size_bps as f64 / 10_000.0;
        st.apply_entry_fee(notional, self.config.fee_bps);

        let direction = match td.action {
            Action::Buy => Direction::Long,
            Action::Sell => Direction::Short,
            _ => unreachable!(),
        };

        let now = st.now;
        let day_index = st.day_index;
        let fill_seq = st.fill_seq;
        st.fill_seq += 1;

        // Upsize existing position in same asset+direction, or open new.
        let open_pos = st
            .portfolio
            .open_positions
            .entry(asset)
            .or_insert_with(|| OpenPosition {
                asset,
                direction,
                size_bps: 0,
                entry_price: fill_px,
                mark_price: fill_px,
                stop_loss_pct: td.stop_loss_pct,
                take_profit_pct: td.take_profit_pct,
                opened_at: now,
                leverage: None,
                liq_price: None,
            });

        if open_pos.direction == direction {
            // Weighted average entry price when upsizing
            let old_notional = open_pos.size_bps as f64;
            let new_notional = old_notional + td.size_bps as f64;
            if new_notional > 0.0 {
                open_pos.entry_price =
                    (open_pos.entry_price * old_notional + fill_px * td.size_bps as f64) / new_notional;
            }
            open_pos.size_bps = open_pos.size_bps.saturating_add(td.size_bps).min(2000);
            // Keep the tighter SL / larger TP when upsizing
            if td.stop_loss_pct < open_pos.stop_loss_pct {
                open_pos.stop_loss_pct = td.stop_loss_pct;
            }
            if td.take_profit_pct > open_pos.take_profit_pct {
                open_pos.take_profit_pct = td.take_profit_pct;
            }
        } else {
            // Opposite direction — treat as a close + flip (overwrite)
            *open_pos = OpenPosition {
                asset,
                direction,
                size_bps: td.size_bps,
                entry_price: fill_px,
                mark_price: fill_px,
                stop_loss_pct: td.stop_loss_pct,
                take_profit_pct: td.take_profit_pct,
                opened_at: now,
                leverage: None,
                liq_price: None,
            };
        }
        open_pos.mark_price = fill_px;

        let receipt = ExecutionReceipt {
            cycle_id: td.cycle_id,
            venue: "backtest".into(),
            venue_order_id: format!("bt-{}-{}", day_index, fill_seq),
            asset,
            filled_size_bps: td.size_bps,
            avg_fill_price: fill_px,
            fee_bps: self.config.fee_bps / 2,
            submitted_at: now,
            filled_at: Some(now),
            note: None,
        };
        st.fills_log.push(receipt.clone());
        Ok(receipt)
    }

    async fn close_position(&self, asset: AssetSymbol) -> Result<ExecutionReceipt, ExecutorError> {
        let mut st = self
            .state
            .lock()
            .map_err(|_| ExecutorError::Internal("mutex poisoned".into()))?;

        let pos = match st.portfolio.open_positions.get(&asset) {
            Some(p) => p.clone(),
            None => {
                // Zero-fill receipt — no state mutation
                let now = st.now;
                let order_id = st.next_order_id();
                let receipt = ExecutionReceipt {
                    cycle_id: Uuid::nil(),
                    venue: "backtest".into(),
                    venue_order_id: order_id,
                    asset,
                    filled_size_bps: 0,
                    avg_fill_price: 0.0,
                    fee_bps: 0,
                    submitted_at: now,
                    filled_at: Some(now),
                    note: Some("no open position".into()),
                };
                return Ok(receipt);
            }
        };

        let (fill_px, _realised) = st.realize_close(&pos, &self.config, None);
        st.portfolio.open_positions.remove(&asset);

        let now = st.now;
        let order_id = st.next_order_id();
        let receipt = ExecutionReceipt {
            cycle_id: Uuid::nil(),
            venue: "backtest".into(),
            venue_order_id: order_id,
            asset,
            filled_size_bps: pos.size_bps,
            avg_fill_price: fill_px,
            fee_bps: self.config.fee_bps / 2,
            submitted_at: now,
            filled_at: Some(now),
            note: None,
        };
        st.fills_log.push(receipt.clone());
        Ok(receipt)
    }

    async fn portfolio(&self) -> Result<PortfolioState, ExecutorError> {
        let st = self
            .state
            .lock()
            .map_err(|_| ExecutorError::Internal("mutex poisoned".into()))?;
        Ok(st.portfolio.clone())
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Compute stop-loss and take-profit absolute price levels for a position.
fn sl_tp_prices(pos: &OpenPosition) -> (f64, f64) {
    match pos.direction {
        Direction::Long => {
            let stop = pos.entry_price * (1.0 - pos.stop_loss_pct as f64 / 100.0);
            let target = pos.entry_price * (1.0 + pos.take_profit_pct as f64 / 100.0);
            (stop, target)
        }
        Direction::Short => {
            // For short: stop is above entry, target is below entry
            let stop = pos.entry_price * (1.0 + pos.stop_loss_pct as f64 / 100.0);
            let target = pos.entry_price * (1.0 - pos.take_profit_pct as f64 / 100.0);
            (stop, target)
        }
        Direction::Flat => (f64::NEG_INFINITY, f64::INFINITY),
    }
}

/// Extract the UTC date from a timestamp (used for day-rollover detection).
fn date_of(dt: &DateTime<Utc>) -> chrono::NaiveDate {
    dt.date_naive()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use uuid::Uuid;
    use xvision_core::{Action, AssetSymbol, Direction, RiskDecision, TraderDecision, VetoReason};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn bar(ts_secs: i64, open: f64, high: f64, low: f64, close: f64) -> MarketBar {
        MarketBar {
            timestamp: Utc.timestamp_opt(ts_secs, 0).single().unwrap(),
            open,
            high,
            low,
            close,
            volume: 1_000.0,
        }
    }

    fn decision(action: Action, size_bps: u32, direction: Direction, sl: f32, tp: f32) -> RiskDecision {
        RiskDecision::Approved {
            decision: TraderDecision {
                cycle_id: Uuid::new_v4(),
                action,
                size_bps,
                direction,
                stop_loss_pct: sl,
                take_profit_pct: tp,
                trader_summary: "test fixture decision for unit test".into(),
                asset: AssetSymbol::Btc,
                trailing_stop_pct: None,
                breakeven_trigger_pct: None,
                breakeven_offset_pct: None,
                fade_sl_bars: None,
                fade_sl_start_pct: None,
                fade_sl_end_pct: None,
                max_bars_held: None,
                sl_atr_mult: None,
                tp_atr_mult: None,
                tp1_pct: None,
                tp1_close_fraction: None,
                tp2_pct: None,
            },
            warnings: vec![],
        }
    }

    fn default_exec(opening_close: f64) -> BacktestExecutor {
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.10,
            max_history_days: 30,
        };
        // Opening bar: ATR seed = high-low = 500 (100 bps of 50000)
        let ob = bar(
            0,
            opening_close,
            opening_close + 500.0,
            opening_close - 500.0,
            opening_close,
        );
        BacktestExecutor::new(cfg, ob)
    }

    // -----------------------------------------------------------------------
    // Scenario 1: Buy → tick through take-profit
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn submit_buy_then_tick_through_take_profit() {
        // Opening bar: close 50000, ATR seed = high-low = 500
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.10,
            max_history_days: 30,
        };
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        // Buy 1000 bps at 50000; TP +5% = 52500
        let d = decision(Action::Buy, 1_000, Direction::Long, 2.0, 5.0);
        let receipt = exec.submit(&d).await.expect("submit must succeed");
        assert_eq!(receipt.venue, "backtest");
        // entry price should be slipped up from 50000
        assert!(receipt.avg_fill_price > 50_000.0, "buy must pay up");

        // Bar 2: high = 52600 → crosses TP at 52500
        // TP = entry_price * 1.05; entry_price ≈ 50100 (slipped), so TP ≈ 52605
        // Use a bar where high clearly blows through 52500+
        let bar2 = bar(86_400, 51_000.0, 53_000.0, 50_900.0, 52_500.0);
        let report = exec.tick(bar2).expect("tick must succeed");

        assert!(
            !report.auto_filled_receipts.is_empty(),
            "take-profit should have auto-fired"
        );
        let auto = &report.auto_filled_receipts[0];
        assert!(
            auto.note.as_deref().unwrap_or("").contains("take-profit"),
            "receipt note should say take-profit"
        );
        // Position should be closed
        let pf = exec.portfolio_snapshot();
        assert!(pf.is_flat(), "portfolio must be flat after TP fires");
    }

    // -----------------------------------------------------------------------
    // Scenario 2: Buy → tick through stop-loss
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn submit_buy_then_tick_through_stop_loss() {
        let exec = default_exec(50_000.0);

        // Buy 1000 bps; SL -2% = 49000
        let d = decision(Action::Buy, 1_000, Direction::Long, 2.0, 10.0);
        exec.submit(&d).await.expect("submit must succeed");

        // Bar where low dips to 48500 — below SL at 49000 (approx)
        let bar2 = bar(86_400, 50_000.0, 50_100.0, 48_500.0, 49_200.0);
        let report = exec.tick(bar2).expect("tick must succeed");

        assert!(
            !report.auto_filled_receipts.is_empty(),
            "stop-loss should have auto-fired"
        );
        let auto = &report.auto_filled_receipts[0];
        assert!(
            auto.note.as_deref().unwrap_or("").contains("stop-loss"),
            "receipt note should say stop-loss"
        );
        let pf = exec.portfolio_snapshot();
        assert!(pf.is_flat(), "portfolio must be flat after SL fires");
    }

    // -----------------------------------------------------------------------
    // Scenario 3: Vetoed decision returns NotActionable
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn vetoed_decision_returns_not_actionable() {
        let exec = default_exec(50_000.0);
        let equity_before = exec.portfolio_snapshot().equity_usd;

        let vetoed = RiskDecision::Vetoed {
            original: TraderDecision {
                cycle_id: Uuid::new_v4(),
                action: Action::Buy,
                size_bps: 500,
                direction: Direction::Long,
                stop_loss_pct: 2.0,
                take_profit_pct: 5.0,
                trader_summary: "test vetoed decision fixture for test".into(),
                asset: AssetSymbol::Btc,
                trailing_stop_pct: None,
                breakeven_trigger_pct: None,
                breakeven_offset_pct: None,
                fade_sl_bars: None,
                fade_sl_start_pct: None,
                fade_sl_end_pct: None,
                max_bars_held: None,
                sl_atr_mult: None,
                tp_atr_mult: None,
                tp1_pct: None,
                tp1_close_fraction: None,
                tp2_pct: None,
            },
            reason: VetoReason::DailyLossCircuitBreaker,
        };

        let result = exec.submit(&vetoed).await;
        assert!(
            matches!(result, Err(ExecutorError::NotActionable(_))),
            "vetoed decision must return NotActionable"
        );

        // State must be unchanged
        let pf = exec.portfolio_snapshot();
        assert!(pf.is_flat(), "state must not mutate on veto");
        assert_eq!(pf.equity_usd, equity_before, "equity must not change on veto");
    }

    // -----------------------------------------------------------------------
    // Scenario 4: Close with no holdings returns zero-fill
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn close_position_no_holdings_returns_zero_fill() {
        let exec = default_exec(50_000.0);
        let receipt = exec
            .close_position(AssetSymbol::Btc)
            .await
            .expect("close must not error");
        assert_eq!(receipt.filled_size_bps, 0, "zero fill for empty position");
        assert_eq!(receipt.avg_fill_price, 0.0);
        assert_eq!(receipt.note.as_deref(), Some("no open position"));
    }

    // -----------------------------------------------------------------------
    // Scenario 5: Loss streak increments on consecutive negative day rollovers
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn loss_streak_increments_on_negative_day_rollover() {
        // Drive 3 consecutive losing days. Each day:
        //   1. Submit a long buy.
        //   2. Tick to a bar that stays within SL/TP (no auto-fill) but closes lower.
        //   3. Call close_position — realises a loss because close < entry.
        //   4. Tick to a midnight-crossing bar → day_rollover fires, loss streak bumps.
        //
        // Using SL=40% and TP=40% so the bars in steps 2/4 never trigger auto-fills.

        // Opening bar: day 0 baseline, ts=0
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.0, // zero slippage so we can reason about exact prices
            max_history_days: 30,
        };
        // Opening bar: ts=0, close=50000, ATR seed = 1000
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        for day in 0..3u32 {
            // Each "day" gets 3 ticks of timestamps:
            //   - intra_ts: same UTC day, just a few seconds later (no rollover)
            //   - mid_ts: crosses into next UTC day (rollover)
            // day 0: base unix day 1 (86400)
            // day 1: base unix day 2, etc.
            let day_start_sec = (day as i64 + 1) * 86_400;
            let intra_ts = day_start_sec - 3600; // still in the same UTC day as day_start-1
            let midnight_ts = day_start_sec + 1; // just crossed UTC midnight

            // 1. Submit a long buy. Current close is 50000 on day 0, or whatever
            //    the previous day closed at. Use wide SL/TP so we never auto-fill.
            let d = decision(Action::Buy, 200, Direction::Long, 40.0, 40.0);
            exec.submit(&d).await.expect("submit ok");

            // 2. Tick to a bar that closes *lower* than entry — stays inside SL/TP.
            //    Intra-day bar (same UTC day): no rollover.
            let entry_close = exec
                .portfolio_snapshot()
                .open_positions
                .get(&AssetSymbol::Btc)
                .map(|p| p.entry_price)
                .unwrap_or(50_000.0);
            let lower_close = entry_close * 0.99; // -1% — well inside 40% SL
            let intra_bar = bar(
                intra_ts,
                lower_close,
                lower_close * 1.001,
                lower_close * 0.999,
                lower_close,
            );
            let report_intra = exec.tick(intra_bar).expect("tick ok");
            assert!(
                report_intra.auto_filled_receipts.is_empty(),
                "day {day}: no auto-fill on intra-day bar"
            );

            // 3. Manually close the position — realises a loss (close < entry, slippage=0).
            let close_receipt = exec.close_position(AssetSymbol::Btc).await.expect("close ok");
            assert!(close_receipt.filled_size_bps > 0, "day {day}: close should fill");
            // fill price should be <= entry_price (slippage=0, sell fills at close = lower_close)
            assert!(
                close_receipt.avg_fill_price <= entry_close,
                "day {day}: fill price {:.0} should be <= entry {:.0}",
                close_receipt.avg_fill_price,
                entry_close
            );

            // 4. Tick to a bar crossing midnight → triggers day_rollover.
            let midnight_bar = bar(
                midnight_ts,
                lower_close,
                lower_close * 1.001,
                lower_close * 0.999,
                lower_close,
            );
            let report_midnight = exec.tick(midnight_bar).expect("tick ok");
            assert!(
                report_midnight.day_rollover,
                "day {day}: midnight tick must trigger day_rollover"
            );
            assert!(
                report_midnight.day_pnl < 0.0,
                "day {day}: day_pnl {:.4} must be negative",
                report_midnight.day_pnl
            );
        }

        let st = exec.state.lock().unwrap();
        assert_eq!(st.loss_streak, 3, "loss streak must be 3 after 3 losing days");
    }

    // -----------------------------------------------------------------------
    // Scenario 6: Slippage moves price against the taker
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn slippage_moves_price_against_taker() {
        // Buy at close 50000, ATR 1000, slippage_atr_frac = 0.1
        // Expected fill = 50000 × (1 + 0.1 × 1000/50000) = 50000 × 1.002 = 50100
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.10,
            max_history_days: 30,
        };
        // Opening bar: high-low = 1000 (seeds ATR = 1000)
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        // Verify ATR seed
        {
            let st = exec.state.lock().unwrap();
            assert_eq!(st.recent_atr, 1000.0, "ATR should be seeded at 1000");
        }

        let d = decision(Action::Buy, 1_000, Direction::Long, 2.0, 5.0);
        let receipt = exec.submit(&d).await.expect("submit ok");

        let expected_fill = 50_000.0 * (1.0 + 0.10 * 1000.0 / 50_000.0);
        assert_eq!(expected_fill, 50_100.0);
        assert!(
            (receipt.avg_fill_price - expected_fill).abs() < 1e-6,
            "fill price {:.6} should equal {expected_fill:.6}",
            receipt.avg_fill_price
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 7: Fees applied round-trip
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn fees_applied_round_trip() {
        // Buy 1000 USD notional (size_bps such that equity × bps / 10000 ≈ 1000)
        // equity = 100_000, size_bps = 100 → notional = 1000 USD
        // fee_bps = 10 → entry fee = 1000 × 5/10000 = 0.5 USD
        //                exit fee  = 1000 × 5/10000 = 0.5 USD  (approx; notional at exit ≈ same)
        // Total fees ≈ 1 USD
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.0, // zero slippage to isolate fee effect
            max_history_days: 30,
        };
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        let equity_before = exec.portfolio_snapshot().equity_usd;

        // Buy 100 bps = 1000 USD notional
        let d = decision(Action::Buy, 100, Direction::Long, 2.0, 5.0);
        exec.submit(&d).await.expect("submit ok");

        // Close at the same price (slippage = 0, so fill_px = close = 50000)
        exec.close_position(AssetSymbol::Btc).await.expect("close ok");

        let equity_after = exec.portfolio_snapshot().equity_usd;
        let equity_drop = equity_before - equity_after;

        // Notional = 1000 USD. Round-trip fee = 1000 × 10bps / 10000 = 1.0 USD
        // (entry 0.5 + exit 0.5; exit notional is slightly different due to slippage=0
        //  but with slippage=0 the price is the same so fees are symmetric)
        // Allow ±0.05 for tiny floating point drift.
        let expected_fee = 1000.0 * 10.0 / 10_000.0;
        assert!(
            (equity_drop - expected_fee).abs() < 0.05,
            "equity drop {equity_drop:.6} should ≈ expected fee {expected_fee:.6}"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 8: No-trade init carries zero per-asset PnL / cost
    // -----------------------------------------------------------------------

    /// Regression guard: a freshly constructed `BacktestExecutor` must report
    /// equity exactly equal to `initial_equity_usd` with zero realized PnL,
    /// no open positions, and no fills logged. No entry-leg fee or
    /// half-spread may be charged at t=0 before any `submit()` has been
    /// called. This locks in the invariant that per-asset PnL/cost begins
    /// at $0.00 for every asset until an actual fill lands.
    #[tokio::test]
    async fn init_state_has_zero_per_asset_pnl_and_cost() {
        let exec = default_exec(50_000.0);
        let pf = exec.portfolio_snapshot();

        assert_eq!(
            pf.equity_usd, 100_000.0,
            "init: equity must equal initial_equity_usd; no cost may be charged before any fill"
        );
        assert_eq!(
            pf.realized_pnl_today_usd, 0.0,
            "init: realized PnL today must start at 0.0"
        );
        assert!(
            pf.is_flat(),
            "init: portfolio must carry no open positions for any asset"
        );
        assert!(
            pf.open_positions.is_empty(),
            "init: per-asset position map must be empty"
        );
        assert!(
            exec.fills_log().is_empty(),
            "init: fills_log must be empty before any submit()"
        );

        for asset in [AssetSymbol::Btc, AssetSymbol::Eth] {
            assert!(
                pf.open_positions.get(&asset).is_none(),
                "init: asset {asset:?} must have no open position (per-asset cost must start at $0)"
            );
        }
    }
}
