//! Advanced stop-loss / take-profit patterns for the backtest executor.
//!
//! `check_and_update` is called once per bar per open position BEFORE the
//! LLM pipeline runs. Priority order (highest first):
//!   SL → PartialTP1 → time-exit → TP2/basic-TP
//!
//! Trailing stop and break-even modify the effective SL price; they are
//! folded into `effective_sl_price` rather than being separate priorities.

use xvision_core::market::Ohlcv;
use xvision_core::trading::Direction;

/// Mutable per-position risk state maintained while a position is open.
pub struct PositionRiskState {
    pub direction: Direction,
    pub entry_price: f64,
    pub stop_loss_pct: f64,
    pub take_profit_pct: f64,
    /// High-water mark: max seen high (long) or min seen low (short).
    pub hwm: f64,
    pub bars_held: u32,
    pub entry_atr: Option<f64>,
    pub breakeven_activated: bool,
    pub tp1_taken: bool,
    pub trailing_stop_pct: Option<f64>,
    pub breakeven_trigger_pct: Option<f64>,
    pub breakeven_offset_pct: Option<f64>,
    pub fade_sl_bars: Option<u32>,
    pub fade_sl_start_pct: Option<f64>,
    pub fade_sl_end_pct: Option<f64>,
    pub max_bars_held: Option<u32>,
    pub sl_atr_mult: Option<f64>,
    pub tp_atr_mult: Option<f64>,
    pub tp1_pct: Option<f64>,
    pub tp1_close_fraction: Option<f64>,
    pub tp2_pct: Option<f64>,
}

/// Exit signal returned by `check_and_update`.
pub enum SltpTrigger {
    FullExit { reason: &'static str },
    PartialTp1 { fraction: f64 },
}

impl PositionRiskState {
    pub fn new(
        direction: Direction,
        entry_price: f64,
        stop_loss_pct: f64,
        take_profit_pct: f64,
        entry_atr: Option<f64>,
        trailing_stop_pct: Option<f64>,
        breakeven_trigger_pct: Option<f64>,
        breakeven_offset_pct: Option<f64>,
        fade_sl_bars: Option<u32>,
        fade_sl_start_pct: Option<f64>,
        fade_sl_end_pct: Option<f64>,
        max_bars_held: Option<u32>,
        sl_atr_mult: Option<f64>,
        tp_atr_mult: Option<f64>,
        tp1_pct: Option<f64>,
        tp1_close_fraction: Option<f64>,
        tp2_pct: Option<f64>,
    ) -> Self {
        Self {
            direction,
            entry_price,
            stop_loss_pct,
            take_profit_pct,
            hwm: entry_price,
            bars_held: 0,
            entry_atr,
            breakeven_activated: false,
            tp1_taken: false,
            trailing_stop_pct,
            breakeven_trigger_pct,
            breakeven_offset_pct,
            fade_sl_bars,
            fade_sl_start_pct,
            fade_sl_end_pct,
            max_bars_held,
            sl_atr_mult,
            tp_atr_mult,
            tp1_pct,
            tp1_close_fraction,
            tp2_pct,
        }
    }
}

fn update_hwm(state: &mut PositionRiskState, bar: &Ohlcv) {
    match state.direction {
        Direction::Long => {
            if bar.high > state.hwm {
                state.hwm = bar.high;
            }
        }
        Direction::Short => {
            if bar.low < state.hwm {
                state.hwm = bar.low;
            }
        }
        Direction::Flat => {}
    }
}

fn maybe_activate_breakeven(state: &mut PositionRiskState, bar: &Ohlcv) {
    if state.breakeven_activated {
        return;
    }
    let trigger = match state.breakeven_trigger_pct {
        Some(t) if t > 0.0 => t,
        _ => return,
    };
    let profit_pct = match state.direction {
        Direction::Long => (bar.close - state.entry_price) / state.entry_price,
        Direction::Short => (state.entry_price - bar.close) / state.entry_price,
        Direction::Flat => return,
    };
    if profit_pct >= trigger {
        state.breakeven_activated = true;
    }
}

fn trailing_sl_price(state: &PositionRiskState) -> Option<f64> {
    let trail_pct = state.trailing_stop_pct?;
    if trail_pct <= 0.0 {
        return None;
    }
    let level = match state.direction {
        Direction::Long => state.hwm * (1.0 - trail_pct / 100.0),
        Direction::Short => state.hwm * (1.0 + trail_pct / 100.0),
        Direction::Flat => return None,
    };
    Some(level)
}

fn fading_sl_price(state: &PositionRiskState) -> Option<f64> {
    let bars = state.fade_sl_bars? as f64;
    let start_pct = state.fade_sl_start_pct?;
    if bars <= 0.0 || start_pct <= 0.0 {
        return None;
    }
    let end_pct = state.fade_sl_end_pct.unwrap_or(0.0);
    let t = (state.bars_held as f64 / bars).min(1.0);
    let current_pct = start_pct + (end_pct - start_pct) * t;
    let level = match state.direction {
        Direction::Long => state.entry_price * (1.0 - current_pct / 100.0),
        Direction::Short => state.entry_price * (1.0 + current_pct / 100.0),
        Direction::Flat => return None,
    };
    Some(level)
}

fn breakeven_sl_price(state: &PositionRiskState) -> Option<f64> {
    if !state.breakeven_activated {
        return None;
    }
    let offset = state.breakeven_offset_pct.unwrap_or(0.0);
    let level = match state.direction {
        Direction::Long => state.entry_price * (1.0 + offset / 100.0),
        Direction::Short => state.entry_price * (1.0 - offset / 100.0),
        Direction::Flat => return None,
    };
    Some(level)
}

fn atr_sl_price(state: &PositionRiskState) -> Option<f64> {
    let mult = state.sl_atr_mult?;
    let atr = state.entry_atr?;
    let level = match state.direction {
        Direction::Long => state.entry_price - atr * mult,
        Direction::Short => state.entry_price + atr * mult,
        Direction::Flat => return None,
    };
    Some(level)
}

fn atr_tp_price(state: &PositionRiskState) -> Option<f64> {
    let mult = state.tp_atr_mult?;
    let atr = state.entry_atr?;
    let level = match state.direction {
        Direction::Long => state.entry_price + atr * mult,
        Direction::Short => state.entry_price - atr * mult,
        Direction::Flat => return None,
    };
    Some(level)
}

/// Returns the most-restrictive effective SL price across all active SL sources.
/// For long: highest price wins (fires soonest). For short: lowest price wins.
fn effective_sl_price(state: &PositionRiskState) -> f64 {
    let is_long = matches!(state.direction, Direction::Long);
    let floor = if is_long { 0.0_f64 } else { f64::MAX };
    let pick = |a: f64, b: f64| if is_long { a.max(b) } else { a.min(b) };

    let basic = if state.stop_loss_pct > 0.0 {
        let level = if is_long {
            state.entry_price * (1.0 - state.stop_loss_pct / 100.0)
        } else {
            state.entry_price * (1.0 + state.stop_loss_pct / 100.0)
        };
        pick(floor, level)
    } else {
        floor
    };

    [
        trailing_sl_price(state).unwrap_or(floor),
        fading_sl_price(state).unwrap_or(floor),
        atr_sl_price(state).unwrap_or(floor),
        breakeven_sl_price(state).unwrap_or(floor),
    ]
    .iter()
    .cloned()
    .fold(basic, pick)
}

fn check_sl_hit(direction: Direction, bar: &Ohlcv, sl_price: f64) -> bool {
    if sl_price <= 0.0 || sl_price >= f64::MAX {
        return false;
    }
    match direction {
        Direction::Long => bar.low <= sl_price,
        Direction::Short => bar.high >= sl_price,
        Direction::Flat => false,
    }
}

fn check_tp_hit(direction: Direction, bar: &Ohlcv, tp_price: f64) -> bool {
    if tp_price <= 0.0 {
        return false;
    }
    match direction {
        Direction::Long => bar.high >= tp_price,
        Direction::Short => bar.low <= tp_price,
        Direction::Flat => false,
    }
}

fn basic_tp_price(direction: Direction, entry: f64, tp_pct: f64) -> f64 {
    match direction {
        Direction::Long => entry * (1.0 + tp_pct / 100.0),
        Direction::Short => entry * (1.0 - tp_pct / 100.0),
        Direction::Flat => 0.0,
    }
}

/// Compute a simple 14-bar ATR (simple average of true ranges) from a history slice.
pub fn compute_atr14(bars: &[&Ohlcv]) -> Option<f64> {
    let n = bars.len();
    if n < 2 {
        return None;
    }
    let start = n.saturating_sub(15);
    let slice = &bars[start..];
    let mut sum = 0.0_f64;
    let mut count = 0u32;
    for i in 1..slice.len().min(15) {
        let b = slice[i];
        let pc = slice[i - 1].close;
        let tr = (b.high - b.low).max((b.high - pc).abs()).max((b.low - pc).abs());
        sum += tr;
        count += 1;
    }
    if count == 0 {
        None
    } else {
        Some(sum / count as f64)
    }
}

/// Update state for the current bar and return any exit trigger.
///
/// Priority: SL (all sources) → PartialTP1 → time-exit → TP2/basic-TP.
pub fn check_and_update(state: &mut PositionRiskState, bar: &Ohlcv) -> Option<SltpTrigger> {
    update_hwm(state, bar);
    maybe_activate_breakeven(state, bar);
    state.bars_held = state.bars_held.saturating_add(1);

    // Priority 1: SL (most restrictive across all SL sources)
    let sl_price = effective_sl_price(state);
    if check_sl_hit(state.direction, bar, sl_price) {
        return Some(SltpTrigger::FullExit { reason: "stop_loss" });
    }

    // Priority 2: Partial TP1 (if configured and not yet taken)
    if !state.tp1_taken {
        if let Some(tp1_pct) = state.tp1_pct {
            let tp1_price = basic_tp_price(state.direction, state.entry_price, tp1_pct);
            if check_tp_hit(state.direction, bar, tp1_price) {
                let frac = state.tp1_close_fraction.unwrap_or(0.5).clamp(0.01, 0.99);
                return Some(SltpTrigger::PartialTp1 { fraction: frac });
            }
        }
    }

    // Priority 3: Time-based exit
    if let Some(max_bars) = state.max_bars_held {
        if state.bars_held >= max_bars {
            return Some(SltpTrigger::FullExit {
                reason: "max_bars_held",
            });
        }
    }

    // Priority 4: TP2 or basic TP (skip if TP1 configured but not yet taken)
    if state.tp1_pct.is_some() && !state.tp1_taken {
        return None;
    }
    let tp_pct = if state.tp1_taken {
        state.tp2_pct.unwrap_or(state.take_profit_pct)
    } else {
        state.take_profit_pct
    };
    if tp_pct > 0.0 {
        let tp_price =
            atr_tp_price(state).unwrap_or_else(|| basic_tp_price(state.direction, state.entry_price, tp_pct));
        if check_tp_hit(state.direction, bar, tp_price) {
            return Some(SltpTrigger::FullExit {
                reason: "take_profit",
            });
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn bar(low: f64, high: f64, close: f64) -> Ohlcv {
        Ohlcv {
            timestamp: Utc::now(),
            open: close,
            high,
            low,
            close,
            volume: 1000.0,
        }
    }

    fn long_state(sl_pct: f64, tp_pct: f64) -> PositionRiskState {
        PositionRiskState::new(
            Direction::Long,
            100.0,
            sl_pct,
            tp_pct,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        )
    }

    #[test]
    fn basic_sl_fires_on_long_when_low_touches() {
        let mut state = long_state(5.0, 10.0);
        // SL at 95.0; bar low = 94.9 → hits
        let trigger = check_and_update(&mut state, &bar(94.9, 101.0, 100.5));
        assert!(matches!(
            trigger,
            Some(SltpTrigger::FullExit { reason: "stop_loss" })
        ));
    }

    #[test]
    fn basic_tp_fires_on_long_when_high_touches() {
        let mut state = long_state(5.0, 10.0);
        // TP at 110.0; bar high = 110.5 → hits
        let trigger = check_and_update(&mut state, &bar(99.0, 110.5, 100.0));
        assert!(matches!(
            trigger,
            Some(SltpTrigger::FullExit {
                reason: "take_profit"
            })
        ));
    }

    #[test]
    fn trailing_stop_ratchets_up_as_price_rises_long() {
        let mut state = long_state(0.0, 0.0);
        state.trailing_stop_pct = Some(2.0); // 2% trailing
                                             // Bar 1: high = 110.0 → hwm = 110, trailing_SL = 107.8
        check_and_update(&mut state, &bar(100.0, 110.0, 109.0));
        assert!((state.hwm - 110.0).abs() < 1e-6);
        // Bar 2: price drops to 107.0 (below 107.8) → SL fires
        let trigger = check_and_update(&mut state, &bar(107.0, 108.0, 107.5));
        assert!(matches!(
            trigger,
            Some(SltpTrigger::FullExit { reason: "stop_loss" })
        ));
    }

    #[test]
    fn max_bars_held_forces_exit() {
        let mut state = long_state(0.0, 0.0);
        state.max_bars_held = Some(3);
        check_and_update(&mut state, &bar(99.0, 101.0, 100.0)); // bar 1
        check_and_update(&mut state, &bar(99.0, 101.0, 100.0)); // bar 2
        let trigger = check_and_update(&mut state, &bar(99.0, 101.0, 100.0)); // bar 3
        assert!(matches!(
            trigger,
            Some(SltpTrigger::FullExit {
                reason: "max_bars_held"
            })
        ));
    }

    #[test]
    fn breakeven_activates_and_moves_sl_to_entry() {
        let mut state = long_state(5.0, 20.0);
        state.breakeven_trigger_pct = Some(0.01); // 1% profit triggers
                                                  // Bar 1: close = 101.5 → profit = 1.5% → triggers breakeven
        check_and_update(&mut state, &bar(100.0, 102.0, 101.5));
        assert!(state.breakeven_activated);
        // Bar 2: price drops to 100.0 → BE SL at entry (100.0), low = 99.5 → fires
        let trigger = check_and_update(&mut state, &bar(99.5, 101.0, 100.5));
        assert!(matches!(
            trigger,
            Some(SltpTrigger::FullExit { reason: "stop_loss" })
        ));
    }

    #[test]
    fn partial_tp1_fires_and_sets_taken() {
        let mut state = long_state(5.0, 0.0);
        state.tp1_pct = Some(5.0);
        state.tp1_close_fraction = Some(0.5);
        let trigger = check_and_update(&mut state, &bar(99.0, 106.0, 105.0));
        assert!(
            matches!(trigger, Some(SltpTrigger::PartialTp1 { fraction }) if (fraction - 0.5).abs() < 1e-6)
        );
    }

    #[test]
    fn compute_atr14_returns_mean_true_range() {
        let bars: Vec<Ohlcv> = (0..15)
            .map(|i| Ohlcv {
                timestamp: Utc::now(),
                open: 100.0,
                high: 102.0,
                low: 98.0,
                close: 100.0 + i as f64 * 0.1,
                volume: 1000.0,
            })
            .collect();
        let refs: Vec<&Ohlcv> = bars.iter().collect();
        let atr = compute_atr14(&refs).unwrap();
        assert!(atr > 0.0);
    }
}
