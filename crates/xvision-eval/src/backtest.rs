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

use std::collections::{BTreeMap, VecDeque};
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

/// A market order accepted by `submit()` but not yet filled. Filled at the
/// NEXT bar's open so a decision formed on bar `i` never trades bar `i`'s
/// own close (no same-bar lookahead).
#[derive(Debug, Clone)]
pub struct PendingOrder {
    pub cycle_id: Uuid,
    pub asset: AssetSymbol,
    pub action: Action,
    pub size_bps: u32,
    pub stop_loss_pct: f32,
    pub take_profit_pct: f32,
}

/// Realized PnL for one simulator day (indexed by `day_index`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DailyPnl {
    pub day_index: u32,
    pub realised_usd: f64,
}

/// Returned by `tick()` describing fills and day-rollover info.
#[derive(Debug, Clone)]
pub struct TickReport {
    /// Receipts for queued market orders filled at this bar's open.
    pub market_filled_receipts: Vec<ExecutionReceipt>,
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
    /// Notional USD committed at entry, per asset. Frozen at fill time so
    /// exit PnL and fees never drift with later equity changes.
    pub entry_notional_usd: BTreeMap<AssetSymbol, f64>,
    /// Market orders accepted but not yet filled; filled at next bar open.
    pub pending_orders: Vec<PendingOrder>,

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
            entry_notional_usd: BTreeMap::new(),
            pending_orders: Vec::new(),
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

    /// Realize a close at `fill_px` (already includes any gap/slippage
    /// adjustment chosen by the caller) and update PnL / equity.
    ///
    /// Notional is the USD amount FROZEN at entry (`entry_notional_usd`), so
    /// exit fees and unit counts never drift when equity moves between entry
    /// and exit. Returns realised net PnL (gross PnL minus exit fee).
    fn realize_close(&mut self, pos: &OpenPosition, config: &BacktestConfig, fill_px: f64) -> f64 {
        let notional = match self.entry_notional_usd.get(&pos.asset) {
            Some(n) => *n,
            // Defensive fallback (state desync): size like a fresh entry off
            // the same NAV basis entries use, not raw stored cash equity.
            None => self.marked_equity() * pos.size_bps as f64 / 10_000.0,
        };
        let exit_fee = notional * (config.fee_bps as f64 / 2.0) / 10_000.0;

        let units = notional / pos.entry_price;
        let direction_sign = match pos.direction {
            Direction::Long => 1.0,
            Direction::Short => -1.0,
            Direction::Flat => 0.0,
        };
        let pnl_gross = direction_sign * (fill_px - pos.entry_price) * units;
        let realised_net = pnl_gross - exit_fee;

        self.portfolio.equity_usd += realised_net;
        self.current_day_pnl += realised_net;
        self.portfolio.realized_pnl_today_usd += realised_net;
        self.entry_notional_usd.remove(&pos.asset);

        realised_net
    }

    /// Unrealized PnL across all open positions, marked to current prices.
    ///
    /// A position missing its frozen entry notional falls back to fresh-entry
    /// sizing off CASH equity. Never call `marked_equity()` here for the
    /// fallback — it would recurse through this function.
    pub fn unrealized_pnl(&self) -> f64 {
        let eq = self.portfolio.equity_usd;
        self.portfolio
            .open_positions
            .values()
            .map(|pos| {
                let notional = self
                    .entry_notional_usd
                    .get(&pos.asset)
                    .copied()
                    .unwrap_or_else(|| eq * pos.size_bps as f64 / 10_000.0);
                let units = notional / pos.entry_price;
                let sign = match pos.direction {
                    Direction::Long => 1.0,
                    Direction::Short => -1.0,
                    Direction::Flat => 0.0,
                };
                sign * (pos.mark_price - pos.entry_price) * units
            })
            .sum()
    }

    /// NAV including unrealized PnL: what sizing and risk rules should see.
    pub fn marked_equity(&self) -> f64 {
        self.portfolio.equity_usd + self.unrealized_pnl()
    }

    /// Fill every queued order at this bar's open (plus adverse slippage on
    /// market legs). Handles Buy/Sell opens, upsizes, flips, and queued
    /// closes. Everything fills one bar after its decision bar, so a
    /// decision never acts on its own decision bar's close. Returns one
    /// receipt per fill, in queue order.
    fn execute_pending_orders(
        &mut self,
        config: &BacktestConfig,
        bar_open: f64,
        ts: DateTime<Utc>,
    ) -> Vec<ExecutionReceipt> {
        let queued = std::mem::take(&mut self.pending_orders);
        let mut receipts = Vec::with_capacity(queued.len());
        for p in queued {
            // Queued close: exit at this bar's open with exit-side slippage.
            if p.action == Action::Close {
                let filled = match self.portfolio.open_positions.get(&p.asset) {
                    Some(pos) => {
                        let pos = pos.clone();
                        let slip_dir = match pos.direction {
                            Direction::Long => -1.0,
                            Direction::Short => 1.0,
                            Direction::Flat => 0.0,
                        };
                        let fill_px = self.fill_price(bar_open, slip_dir, config.slippage_atr_frac);
                        self.realize_close(&pos, config, fill_px);
                        self.portfolio.open_positions.remove(&p.asset);
                        (pos.size_bps, fill_px, config.fee_bps / 2)
                    }
                    None => (0, 0.0, 0),
                };
                receipts.push(ExecutionReceipt {
                    cycle_id: p.cycle_id,
                    venue: "backtest".into(),
                    venue_order_id: self.next_order_id(),
                    asset: p.asset,
                    filled_size_bps: filled.0,
                    avg_fill_price: filled.1,
                    fee_bps: filled.2,
                    submitted_at: ts,
                    filled_at: Some(ts),
                    note: Some("close filled at next bar open".into()),
                });
                continue;
            }

            let direction = match p.action {
                Action::Buy => Direction::Long,
                _ => Direction::Short,
            };
            let slip_dir = if p.action == Action::Buy { 1.0 } else { -1.0 };
            let fill_px = self.fill_price(bar_open, slip_dir, config.slippage_atr_frac);
            let notional = self.marked_equity() * p.size_bps as f64 / 10_000.0;

            match self.portfolio.open_positions.get(&p.asset) {
                Some(existing) if existing.direction == direction => {
                    // Upsize: cap the accepted slice at 2000 bps so reported
                    // size_bps and committed notional never diverge. Fees
                    // charge only on the accepted slice.
                    let old_entry = existing.entry_price;
                    let old_notional = self.entry_notional_usd.get(&p.asset).copied().unwrap_or(0.0);
                    let accepted_bps = 2000u32.saturating_sub(existing.size_bps).min(p.size_bps);
                    if accepted_bps > 0 {
                        let accepted_notional = notional * accepted_bps as f64 / p.size_bps as f64;
                        self.apply_entry_fee(accepted_notional, config.fee_bps);
                        let total_notional = old_notional + accepted_notional;
                        let avg_px = if total_notional > 0.0 {
                            (old_entry * old_notional + fill_px * accepted_notional) / total_notional
                        } else {
                            fill_px
                        };
                        let pos = self.portfolio.open_positions.get_mut(&p.asset).unwrap();
                        pos.entry_price = avg_px;
                        pos.size_bps += accepted_bps;
                        if p.stop_loss_pct < pos.stop_loss_pct {
                            pos.stop_loss_pct = p.stop_loss_pct;
                        }
                        if p.take_profit_pct > pos.take_profit_pct {
                            pos.take_profit_pct = p.take_profit_pct;
                        }
                        pos.mark_price = fill_px;
                        *self.entry_notional_usd.entry(p.asset.clone()).or_insert(0.0) += accepted_notional;
                    }
                    let order_id = self.next_order_id();
                    receipts.push(ExecutionReceipt {
                        cycle_id: p.cycle_id,
                        venue: "backtest".into(),
                        venue_order_id: order_id,
                        asset: p.asset,
                        filled_size_bps: accepted_bps,
                        avg_fill_price: fill_px,
                        fee_bps: if accepted_bps > 0 { config.fee_bps / 2 } else { 0 },
                        submitted_at: ts,
                        filled_at: Some(ts),
                        note: None,
                    });
                }
                existing => {
                    // Flip or fresh open: realize the old leg first so its PnL
                    // and exit fee land in equity instead of vanishing.
                    if let Some(old) = existing.cloned() {
                        let old_dir = old.direction;
                        let exit_slip_dir = match old_dir {
                            Direction::Long => -1.0,
                            Direction::Short => 1.0,
                            Direction::Flat => 0.0,
                        };
                        let exit_px = self.fill_price(bar_open, exit_slip_dir, config.slippage_atr_frac);
                        self.realize_close(&old, config, exit_px);
                        self.portfolio.open_positions.remove(&p.asset);
                    }
                    self.apply_entry_fee(notional, config.fee_bps);
                    self.portfolio.open_positions.insert(
                        p.asset.clone(),
                        OpenPosition {
                            asset: p.asset.clone(),
                            direction,
                            size_bps: p.size_bps,
                            entry_price: fill_px,
                            mark_price: fill_px,
                            stop_loss_pct: p.stop_loss_pct,
                            take_profit_pct: p.take_profit_pct,
                            opened_at: ts,
                            leverage: None,
                            liq_price: None,
                        },
                    );
                    self.entry_notional_usd.insert(p.asset.clone(), notional);

                    let order_id = self.next_order_id();
                    receipts.push(ExecutionReceipt {
                        cycle_id: p.cycle_id,
                        venue: "backtest".into(),
                        venue_order_id: order_id,
                        asset: p.asset,
                        filled_size_bps: p.size_bps,
                        avg_fill_price: fill_px,
                        fee_bps: config.fee_bps / 2,
                        submitted_at: ts,
                        filled_at: Some(ts),
                        note: None,
                    });
                }
            }
        }
        receipts
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
    /// 1. Rolls the UTC day when `next.timestamp` crosses midnight — BEFORE
    ///    any fills, so fills stamped with the new day book into the new day.
    /// 2. Fills every order queued by `submit()`/`close_position()` at
    ///    `next.open` (plus adverse slippage). A decision formed on bar `i`
    ///    trades bar `i+1` — never its own decision bar.
    /// 3. Checks all open positions (including just-filled ones) against the
    ///    new bar's high/low for stop-loss and take-profit triggers. Stop
    ///    fills respect gaps through the level and always pay adverse
    ///    slippage; take-profit fills capture favorable gaps to the open.
    /// 4. Marks remaining positions to `next.close`.
    /// 5. Updates Wilder ATR and advances the clock.
    pub fn tick(&self, next: MarketBar) -> Result<TickReport, ExecutorError> {
        let mut st = self
            .state
            .lock()
            .map_err(|_| ExecutorError::Internal("mutex poisoned".into()))?;

        let prev_day = date_of(&st.now);
        let next_day = date_of(&next.timestamp);
        let day_rollover = next_day != prev_day;

        // --- day rollover FIRST ---
        // Orders queued yesterday fill at this bar's open carrying today's
        // timestamp; flushing the old bucket before they execute books those
        // fills into the NEW day instead of leaking them into yesterday.
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

        // --- queued market orders fill at this bar's open ---
        let market_fills = st.execute_pending_orders(&self.config, next.open, next.timestamp);
        st.fills_log.extend(market_fills.iter().cloned());

        // --- stop / take-profit scanning ---
        let mut auto_fills: Vec<ExecutionReceipt> = Vec::new();
        let assets: Vec<AssetSymbol> = st.portfolio.open_positions.keys().cloned().collect();
        for asset in assets {
            let pos = match st.portfolio.open_positions.get(&asset) {
                Some(p) => p.clone(),
                None => continue,
            };

            let (stop_px, target_px) = sl_tp_prices(&pos);

            // Final exit price given trigger kind and this bar's open, with
            // gaps and adverse slippage priced in. `None` = no trigger.
            let trigger: Option<(f64, bool)> = match pos.direction {
                Direction::Long => {
                    let tp_hit = next.high >= target_px;
                    let sl_hit = next.low <= stop_px;
                    if sl_hit {
                        // Gap-through-stop fills at the (worse) open; a normal
                        // touch fills at the stop level; stops are market
                        // orders so adverse slippage applies on top.
                        let base = if next.open <= stop_px { next.open } else { stop_px };
                        Some((base - self.config.slippage_atr_frac * st.recent_atr, false))
                    } else if tp_hit {
                        // Favorable gap above target fills at the better open.
                        let base = if next.open >= target_px {
                            next.open
                        } else {
                            target_px
                        };
                        Some((base, true))
                    } else {
                        None
                    }
                }
                Direction::Short => {
                    let tp_hit = next.low <= target_px;
                    let sl_hit = next.high >= stop_px;
                    if sl_hit {
                        let base = if next.open >= stop_px { next.open } else { stop_px };
                        Some((base + self.config.slippage_atr_frac * st.recent_atr, false))
                    } else if tp_hit {
                        let base = if next.open <= target_px {
                            next.open
                        } else {
                            target_px
                        };
                        Some((base, true))
                    } else {
                        None
                    }
                }
                Direction::Flat => None,
            };

            if let Some((fill_px, is_tp)) = trigger {
                st.realize_close(&pos, &self.config, fill_px);
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

        Ok(TickReport {
            market_filled_receipts: market_fills,
            auto_filled_receipts: auto_fills,
            day_rollover,
            day_pnl,
        })
    }

    /// Current portfolio snapshot (lock-free copy).
    ///
    /// The STORED equity stays on the cash basis; only the RETURNED copy is
    /// marked (cash + unrealized PnL). Marking live state here would make a
    /// later `realize_close()` book the same unrealized gain a second time.
    pub fn portfolio_snapshot(&self) -> PortfolioState {
        // Best-effort: if lock is poisoned return the portfolio rather than panic.
        match self.state.lock() {
            Ok(st) => {
                let mut pf = st.portfolio.clone();
                pf.equity_usd = st.marked_equity();
                pf
            }
            Err(poisoned) => {
                let st = poisoned.into_inner();
                let mut pf = st.portfolio.clone();
                pf.equity_usd = st.marked_equity();
                pf
            }
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

        // Queue the order; it fills at the NEXT bar's open in tick(). A
        // decision formed from bar i's data must never fill at bar i's close.
        let now = st.now;
        st.pending_orders.push(PendingOrder {
            cycle_id: td.cycle_id,
            asset,
            action: td.action,
            size_bps: td.size_bps,
            stop_loss_pct: td.stop_loss_pct,
            take_profit_pct: td.take_profit_pct,
        });

        Ok(ExecutionReceipt {
            cycle_id: td.cycle_id,
            venue: "backtest".into(),
            venue_order_id: format!("pending-{}", td.cycle_id),
            asset,
            filled_size_bps: 0,
            avg_fill_price: 0.0,
            fee_bps: 0,
            submitted_at: now,
            filled_at: None,
            note: Some("queued: fills at next bar open".into()),
        })
    }

    async fn close_position(&self, asset: AssetSymbol) -> Result<ExecutionReceipt, ExecutorError> {
        let mut st = self
            .state
            .lock()
            .map_err(|_| ExecutorError::Internal("mutex poisoned".into()))?;

        // A close cancels anything still queued for this asset — otherwise a
        // stale buy/sell could reopen exposure one bar after the strategy
        // went flat.
        st.pending_orders.retain(|p| p.asset != asset);

        if !st.portfolio.open_positions.contains_key(&asset) {
            // Zero-fill receipt — no state mutation
            let now = st.now;
            let order_id = st.next_order_id();
            return Ok(ExecutionReceipt {
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
            });
        }

        let pos_size_bps = st.portfolio.open_positions[&asset].size_bps;

        // Queue the exit; it fills at the NEXT bar's open under the same
        // no-lookahead rule as entries — a decision formed from bar i's data
        // must never trade bar i's own close.
        let now = st.now;
        st.pending_orders.push(PendingOrder {
            cycle_id: Uuid::nil(),
            asset: asset.clone(),
            action: Action::Close,
            size_bps: pos_size_bps,
            stop_loss_pct: 0.0,
            take_profit_pct: 0.0,
        });

        Ok(ExecutionReceipt {
            cycle_id: Uuid::nil(),
            venue: "backtest".into(),
            venue_order_id: format!("pending-close-{}", asset),
            asset,
            filled_size_bps: 0,
            avg_fill_price: 0.0,
            fee_bps: 0,
            submitted_at: now,
            filled_at: None,
            note: Some("queued: closes at next bar open".into()),
        })
    }

    async fn portfolio(&self) -> Result<PortfolioState, ExecutorError> {
        let st = self
            .state
            .lock()
            .map_err(|_| ExecutorError::Internal("mutex poisoned".into()))?;
        let mut pf = st.portfolio.clone();
        pf.equity_usd = st.marked_equity();
        Ok(pf)
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
        // Opening bar: close 50000, ATR seed = high-low = 1000
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.10,
            max_history_days: 30,
        };
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        // Buy 1000 bps; queued on bar 0, must fill at bar 1's OPEN.
        let d = decision(Action::Buy, 1_000, Direction::Long, 2.0, 5.0);
        let receipt = exec.submit(&d).await.expect("submit must succeed");
        assert_eq!(receipt.venue, "backtest");
        assert!(receipt.filled_at.is_none(), "order must be queued, not filled");
        assert!(
            receipt.note.as_deref().unwrap_or("").contains("next bar open"),
            "queued receipt must document next-open fill timing"
        );

        // Bar 1: pending buy fills at open 51_000 + adverse slip (0.1 × ATR).
        let bar1 = bar(3_600, 51_000.0, 51_200.0, 50_800.0, 51_000.0);
        let report1 = exec.tick(bar1).expect("tick must succeed");
        assert_eq!(report1.market_filled_receipts.len(), 1, "buy must fill");
        let entry_receipt = &report1.market_filled_receipts[0];
        assert!(
            entry_receipt.avg_fill_price > 51_000.0,
            "buy must pay up over the next open"
        );
        assert!(report1.auto_filled_receipts.is_empty());

        // Entry ≈ 51_100 → TP ≈ 53_655. Bar 2 high blows through it.
        let bar2 = bar(86_400, 52_000.0, 54_500.0, 51_900.0, 53_000.0);
        let report2 = exec.tick(bar2).expect("tick must succeed");

        assert!(
            !report2.auto_filled_receipts.is_empty(),
            "take-profit should have auto-fired"
        );
        let auto = &report2.auto_filled_receipts[0];
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
        //   2. Tick to an intra-day bar: the buy fills at that bar's open;
        //      the bar closes lower but stays inside SL/TP.
        //   3. Call close_position — queues the exit (no same-bar lookahead).
        //   4. Tick a second intra-day bar: the close fills at its open,
        //      realising a loss on the SAME UTC day.
        //   5. Tick a midnight-crossing bar → day_rollover fires; the loss
        //      flushes into that day's bucket and the loss streak bumps.
        //
        // Using SL=40% and TP=40% so no bar ever triggers auto-fills.

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
            //   - intra_ts / intra2_ts: same UTC day, no rollover
            //   - mid_ts: crosses into next UTC day (rollover)
            // day 0: base unix day 1 (86400)
            // day 1: base unix day 2, etc.
            let day_start_sec = (day as i64 + 1) * 86_400;
            let intra_ts = day_start_sec - 3600; // still in the same UTC day
            let intra2_ts = day_start_sec - 1800; // also same UTC day
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

            // 3. Queue the manual close (fills next bar open — slippage=0).
            let close_receipt = exec.close_position(AssetSymbol::Btc).await.expect("close ok");
            assert!(
                close_receipt.filled_at.is_none(),
                "day {day}: close must be queued, not filled same-bar"
            );

            // 4. Second intra-day bar fills the queued close at its open.
            let intra2_bar = bar(
                intra2_ts,
                lower_close,
                lower_close * 1.001,
                lower_close * 0.999,
                lower_close,
            );
            let report_intra2 = exec.tick(intra2_bar).expect("tick ok");
            assert_eq!(
                report_intra2.market_filled_receipts.len(),
                1,
                "day {day}: queued close must fill"
            );
            let close_fill = &report_intra2.market_filled_receipts[0];
            assert!(close_fill.filled_size_bps > 0, "day {day}: close should fill");
            // Fill price <= entry (slippage=0, sell fills at open = lower_close).
            assert!(
                close_fill.avg_fill_price <= entry_close,
                "day {day}: fill price {:.0} should be <= entry {:.0}",
                close_fill.avg_fill_price,
                entry_close
            );

            // 5. Tick to a bar crossing midnight → triggers day_rollover.
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
        // Pending buy fills at the NEXT bar's open 50000, ATR 1000,
        // slippage_atr_frac = 0.1 → fill = 50000 × (1 + 0.1 × 1000/50000) = 50100
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
        exec.submit(&d).await.expect("submit ok");

        // Next bar opens at 50_000; SL/TP stay untouched within this bar.
        let bar1 = bar(3_600, 50_000.0, 50_200.0, 49_800.0, 50_000.0);
        let report = exec.tick(bar1).expect("tick ok");

        let expected_fill = 50_000.0 * (1.0 + 0.10 * 1000.0 / 50_000.0);
        assert_eq!(expected_fill, 50_100.0);
        assert_eq!(report.market_filled_receipts.len(), 1);
        assert!(
            (report.market_filled_receipts[0].avg_fill_price - expected_fill).abs() < 1e-6,
            "fill price {:.6} should equal {expected_fill:.6}",
            report.market_filled_receipts[0].avg_fill_price
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

        // Buy 100 bps = 1000 USD notional; fills at next bar open.
        let d = decision(Action::Buy, 100, Direction::Long, 2.0, 5.0);
        exec.submit(&d).await.expect("submit ok");

        // Fill bar: same prices (slippage = 0), SL/TP untouched.
        let bar1 = bar(3_600, 50_000.0, 50_200.0, 49_800.0, 50_000.0);
        exec.tick(bar1).expect("tick ok");

        // Queue the close, then tick a second bar so the exit fills at that
        // bar's open (slippage = 0 → fill_px = 50000 again).
        exec.close_position(AssetSymbol::Btc).await.expect("close ok");
        let bar2 = bar(7_200, 50_000.0, 50_200.0, 49_800.0, 50_000.0);
        exec.tick(bar2).expect("tick ok");

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
    // -----------------------------------------------------------------------
    // Scenario 9: Snapshot marking must not mutate cash equity (P0 guard)
    // -----------------------------------------------------------------------

    /// Calling `portfolio_snapshot()` / `portfolio()` must never write the
    /// marked NAV back into stored cash equity. If it did, a later close
    /// would book the same unrealized gain twice.
    #[tokio::test]
    async fn snapshot_does_not_mutate_cash_equity() {
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 0,
            slippage_atr_frac: 0.0,
            max_history_days: 30,
        };
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        // Buy 1000 bps = 10_000 USD notional at bar1 open (50000).
        let d = decision(Action::Buy, 1_000, Direction::Long, 2.0, 5.0);
        exec.submit(&d).await.expect("submit ok");
        let bar1 = bar(3_600, 50_000.0, 50_200.0, 49_800.0, 60_000.0);
        exec.tick(bar1).expect("tick ok");

        // Position: 0.2 BTC entered at 50000, marked to 60000 → +2000 unrealized.
        let snap1 = exec.portfolio_snapshot().equity_usd;
        assert!(
            (snap1 - 102_000.0).abs() < 1e-6,
            "marked snapshot should be 102000, got {snap1}"
        );

        // Close via queued order filling at bar2 open (60000).
        exec.close_position(AssetSymbol::Btc).await.expect("close ok");
        let bar2 = bar(7_200, 60_000.0, 60_200.0, 59_800.0, 60_000.0);
        exec.tick(bar2).expect("tick ok");

        // Cash 100000 + realized 2000 — NOT 104000 (double-counted).
        let final_eq = exec.portfolio_snapshot().equity_usd;
        assert!(
            (final_eq - 102_000.0).abs() < 1e-6,
            "final equity must be exactly cash + realized once; got {final_eq} (double count?)"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 10: close_position cancels queued orders for the asset
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn close_cancels_pending_orders_for_asset() {
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 10,
            slippage_atr_frac: 0.0,
            max_history_days: 30,
        };
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        // Queue a buy but close before it can fill.
        let d = decision(Action::Buy, 1_000, Direction::Long, 2.0, 5.0);
        exec.submit(&d).await.expect("submit ok");
        exec.close_position(AssetSymbol::Btc).await.expect("close ok");

        // Next bar: the stale buy must NOT fill; no position may open.
        let bar1 = bar(3_600, 50_000.0, 50_200.0, 49_800.0, 50_000.0);
        let report = exec.tick(bar1).expect("tick ok");
        assert!(
            report.market_filled_receipts.is_empty(),
            "queued buy must be cancelled by close_position"
        );
        let pf = exec.portfolio_snapshot();
        assert!(pf.is_flat(), "no position may exist after close cancels pendings");
        assert!(
            (pf.equity_usd - 100_000.0).abs() < 1e-9,
            "equity must be untouched when only a cancelled pending order existed"
        );
    }

    // -----------------------------------------------------------------------
    // Scenario 11: manual exits defer to next-bar open (no same-bar lookahead)
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn close_fills_at_next_bar_open_not_decision_close() {
        let cfg = BacktestConfig {
            initial_equity_usd: 100_000.0,
            fee_bps: 0,
            slippage_atr_frac: 0.0,
            max_history_days: 30,
        };
        let opening = bar(0, 50_000.0, 50_500.0, 49_500.0, 50_000.0);
        let exec = BacktestExecutor::new(cfg, opening);

        // Entry fills at bar1 open.
        let d = decision(Action::Buy, 1_000, Direction::Long, 40.0, 40.0);
        exec.submit(&d).await.expect("submit ok");
        let bar1 = bar(3_600, 50_000.0, 55_900.0, 49_800.0, 55_000.0);
        exec.tick(bar1).expect("tick ok");

        // Decision forms on bar1 (close 55000) — exit must fill at BAR2 OPEN
        // (53000), never at bar1's own close.
        exec.close_position(AssetSymbol::Btc).await.expect("close ok");
        let bar2 = bar(7_200, 53_000.0, 53_100.0, 52_900.0, 53_000.0);
        let report = exec.tick(bar2).expect("tick ok");

        assert_eq!(report.market_filled_receipts.len(), 1, "exit must fill");
        let exit = &report.market_filled_receipts[0];
        assert!(
            (exit.avg_fill_price - 53_000.0).abs() < 1e-6,
            "exit must fill at next bar open 53000, got {}",
            exit.avg_fill_price
        );
    }
}
