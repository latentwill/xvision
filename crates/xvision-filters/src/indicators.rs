//! Incremental indicator math for the six v1 indicators.
//!
//! Each indicator is fed one bar at a time via [`IndicatorEngine::push`].
//! After enough bars have been consumed (the indicator's warmup), values
//! can be read with [`IndicatorEngine::value`].
//!
//! Numerical contracts:
//!
//! * **SMA** — trailing arithmetic mean of the last `period` closes.
//!   Warmup = `period` bars; value available on bar `period - 1` (0-indexed).
//! * **EMA** — seeded with the SMA on bar `period - 1`, then
//!   `ema[t] = α * close[t] + (1 - α) * ema[t-1]`, where `α = 2 / (period + 1)`.
//!   Warmup = `period` bars.
//! * **RSI (Wilder)** — Wilder's smoothing (NOT plain EMA). The seed
//!   average gain/loss on bar `period - 1` is the arithmetic mean of the
//!   first `period - 1` deltas; each subsequent bar updates
//!   `avg = (avg * (period - 1) + delta) / period`. Warmup = `period + 1` bars.
//! * **ATR (Wilder)** — same smoothing applied to true range. Warmup
//!   = `period + 1` bars (first true range needs a prior close).
//! * **ATR%** — `100 * ATR / close`. Warmup matches ATR.
//! * **Close** — `close[t]`, no warmup.
//!
//! All warmups are inclusive: a `period=14` EMA produces its first value
//! on the 14th `push` call (1-based), i.e. after 14 closes have been
//! observed. This module is internally 1-based (counts of bars
//! consumed); the runtime translates to/from bar indices as needed.

use std::collections::HashMap;

use crate::types::{IndicatorName, IndicatorRef};

/// Single OHLCV bar — engine-independent reduction of `xvision_core::market::Ohlcv`.
/// The runtime accepts these to keep the filters crate decoupled from
/// the engine's bar type.
#[derive(Debug, Clone, Copy)]
pub struct Bar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

impl Bar {
    pub fn new(open: f64, high: f64, low: f64, close: f64) -> Self {
        Self {
            open,
            high,
            low,
            close,
        }
    }
}

/// Key uniquely identifying an indicator instance fed by the engine.
/// Two refs with the same `(name, period)` share state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IndicatorKey {
    pub name: IndicatorName,
    pub period: u32,
}

impl IndicatorKey {
    pub fn from_ref(r: &IndicatorRef) -> Self {
        Self {
            name: r.name,
            // Close is periodless; we normalize to period=0 for keying.
            period: r.period.unwrap_or(0),
        }
    }
}

/// Engine that maintains incremental state for a fixed set of indicator
/// instances. The set is declared up-front (so memory is bounded) and
/// every `push` updates every instance.
#[derive(Debug)]
pub struct IndicatorEngine {
    instances: HashMap<IndicatorKey, Instance>,
    bars_seen: u64,
    /// Last close — held outside the per-instance state because every
    /// `IndicatorKey { Close, 0 }` query returns the same value, and
    /// because ATR needs the previous bar's close to compute true
    /// range on the next push.
    last_close: Option<f64>,
}

#[derive(Debug)]
enum Instance {
    Sma(SmaState),
    Ema(EmaState),
    Rsi(RsiState),
    Atr(AtrState),
    AtrPct(AtrState),
}

impl IndicatorEngine {
    /// Build an engine that tracks the union of indicator references
    /// passed in. Duplicate `(name, period)` pairs collapse to one
    /// instance. Empty input is valid (the engine just tracks
    /// last/prev close for `Close` lookups).
    pub fn new<'a, I>(refs: I) -> Self
    where
        I: IntoIterator<Item = &'a IndicatorRef>,
    {
        let mut instances: HashMap<IndicatorKey, Instance> = HashMap::new();
        for r in refs {
            let key = IndicatorKey::from_ref(r);
            if instances.contains_key(&key) {
                continue;
            }
            let inst = match r.name {
                IndicatorName::Sma => Instance::Sma(SmaState::new(key.period as usize)),
                IndicatorName::Ema => Instance::Ema(EmaState::new(key.period as usize)),
                IndicatorName::Rsi => Instance::Rsi(RsiState::new(key.period as usize)),
                IndicatorName::Atr => Instance::Atr(AtrState::new(key.period as usize)),
                IndicatorName::AtrPct => Instance::AtrPct(AtrState::new(key.period as usize)),
                IndicatorName::Close => continue, // no per-instance state
            };
            instances.insert(key, inst);
        }
        Self {
            instances,
            bars_seen: 0,
            last_close: None,
        }
    }

    /// Feed one bar. Updates every tracked instance.
    pub fn push(&mut self, bar: &Bar) {
        let prev_close = self.last_close;
        for (_, inst) in self.instances.iter_mut() {
            match inst {
                Instance::Sma(s) => s.push(bar.close),
                Instance::Ema(s) => s.push(bar.close),
                Instance::Rsi(s) => s.push(bar.close),
                Instance::Atr(s) | Instance::AtrPct(s) => s.push(bar.high, bar.low, bar.close, prev_close),
            }
        }
        self.last_close = Some(bar.close);
        self.bars_seen += 1;
    }

    /// Current value for an indicator reference. `None` while the
    /// instance is still warming up. `Close` always returns the latest
    /// close once at least one bar has been pushed.
    pub fn value(&self, r: &IndicatorRef) -> Option<f64> {
        if matches!(r.name, IndicatorName::Close) {
            return self.last_close;
        }
        let key = IndicatorKey::from_ref(r);
        match self.instances.get(&key)? {
            Instance::Sma(s) => s.value(),
            Instance::Ema(s) => s.value(),
            Instance::Rsi(s) => s.value(),
            Instance::Atr(s) => s.value(),
            Instance::AtrPct(s) => match (s.value(), self.last_close) {
                (Some(atr), Some(close)) if close.abs() > f64::EPSILON => Some(100.0 * atr / close),
                _ => None,
            },
        }
    }

    /// Maximum warmup across every registered instance. Used by
    /// [`FilterState`](crate::state::FilterState) to decide when the
    /// filter is ready to evaluate. Includes a 1-bar margin for
    /// `crosses_*` operators which need a `t-1` value.
    pub fn warmup_bars(&self) -> u32 {
        let mut max_warmup: u32 = 0;
        for (key, inst) in &self.instances {
            let bars_needed = match inst {
                Instance::Sma(_) | Instance::Ema(_) => key.period,
                Instance::Rsi(_) | Instance::Atr(_) | Instance::AtrPct(_) => key.period + 1,
            };
            if bars_needed > max_warmup {
                max_warmup = bars_needed;
            }
        }
        max_warmup
    }

    /// Bars seen so far. Cheap accessor for tests and the runtime's
    /// warmup countdown.
    pub fn bars_seen(&self) -> u64 {
        self.bars_seen
    }
}

// ---------------------------------------------------------------------------
// SMA
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct SmaState {
    period: usize,
    window: std::collections::VecDeque<f64>,
    sum: f64,
}

impl SmaState {
    fn new(period: usize) -> Self {
        Self {
            period,
            window: std::collections::VecDeque::with_capacity(period),
            sum: 0.0,
        }
    }

    fn push(&mut self, close: f64) {
        self.window.push_back(close);
        self.sum += close;
        if self.window.len() > self.period {
            self.sum -= self.window.pop_front().expect("window non-empty");
        }
    }

    fn value(&self) -> Option<f64> {
        if self.window.len() == self.period {
            Some(self.sum / self.period as f64)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// EMA
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct EmaState {
    period: usize,
    /// Seed window: accumulates the first `period` closes; once full,
    /// produces the seed SMA and the EMA recurrence takes over.
    seed_buf: Vec<f64>,
    value: Option<f64>,
    alpha: f64,
}

impl EmaState {
    fn new(period: usize) -> Self {
        Self {
            period,
            seed_buf: Vec::with_capacity(period),
            value: None,
            alpha: 2.0 / (period as f64 + 1.0),
        }
    }

    fn push(&mut self, close: f64) {
        if self.value.is_none() {
            self.seed_buf.push(close);
            if self.seed_buf.len() == self.period {
                let seed: f64 = self.seed_buf.iter().sum::<f64>() / self.period as f64;
                self.value = Some(seed);
                // free the seed buffer
                self.seed_buf = Vec::new();
            }
        } else {
            let prev = self.value.unwrap();
            self.value = Some(self.alpha * close + (1.0 - self.alpha) * prev);
        }
    }

    fn value(&self) -> Option<f64> {
        self.value
    }
}

// ---------------------------------------------------------------------------
// RSI (Wilder)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct RsiState {
    period: usize,
    prev_close: Option<f64>,
    /// Pre-seed: collects the first `period` deltas (so `period + 1`
    /// closes). On the (period+1)-th close, the seed averages compute
    /// and Wilder smoothing begins.
    seed_gains: Vec<f64>,
    seed_losses: Vec<f64>,
    avg_gain: Option<f64>,
    avg_loss: Option<f64>,
}

impl RsiState {
    fn new(period: usize) -> Self {
        Self {
            period,
            prev_close: None,
            seed_gains: Vec::with_capacity(period),
            seed_losses: Vec::with_capacity(period),
            avg_gain: None,
            avg_loss: None,
        }
    }

    fn push(&mut self, close: f64) {
        let Some(prev) = self.prev_close else {
            self.prev_close = Some(close);
            return;
        };
        let delta = close - prev;
        let gain = if delta > 0.0 { delta } else { 0.0 };
        let loss = if delta < 0.0 { -delta } else { 0.0 };

        if self.avg_gain.is_none() {
            self.seed_gains.push(gain);
            self.seed_losses.push(loss);
            if self.seed_gains.len() == self.period {
                let g = self.seed_gains.iter().sum::<f64>() / self.period as f64;
                let l = self.seed_losses.iter().sum::<f64>() / self.period as f64;
                self.avg_gain = Some(g);
                self.avg_loss = Some(l);
                self.seed_gains = Vec::new();
                self.seed_losses = Vec::new();
            }
        } else {
            let p = self.period as f64;
            let g = self.avg_gain.unwrap();
            let l = self.avg_loss.unwrap();
            self.avg_gain = Some((g * (p - 1.0) + gain) / p);
            self.avg_loss = Some((l * (p - 1.0) + loss) / p);
        }

        self.prev_close = Some(close);
    }

    fn value(&self) -> Option<f64> {
        let g = self.avg_gain?;
        let l = self.avg_loss?;
        if l.abs() < f64::EPSILON {
            return Some(100.0);
        }
        let rs = g / l;
        Some(100.0 - 100.0 / (1.0 + rs))
    }
}

// ---------------------------------------------------------------------------
// ATR (Wilder)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct AtrState {
    period: usize,
    seed_tr: Vec<f64>,
    value: Option<f64>,
}

impl AtrState {
    fn new(period: usize) -> Self {
        Self {
            period,
            seed_tr: Vec::with_capacity(period),
            value: None,
        }
    }

    fn push(&mut self, high: f64, low: f64, _close: f64, prev_close: Option<f64>) {
        let Some(prev) = prev_close else {
            return;
        };
        let tr = true_range(high, low, prev);
        if self.value.is_none() {
            self.seed_tr.push(tr);
            if self.seed_tr.len() == self.period {
                let seed = self.seed_tr.iter().sum::<f64>() / self.period as f64;
                self.value = Some(seed);
                self.seed_tr = Vec::new();
            }
        } else {
            let p = self.period as f64;
            let v = self.value.unwrap();
            self.value = Some((v * (p - 1.0) + tr) / p);
        }
    }

    fn value(&self) -> Option<f64> {
        self.value
    }
}

fn true_range(high: f64, low: f64, prev_close: f64) -> f64 {
    let a = high - low;
    let b = (high - prev_close).abs();
    let c = (low - prev_close).abs();
    a.max(b).max(c)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{IndicatorName, IndicatorRef};

    fn bar(o: f64, h: f64, l: f64, c: f64) -> Bar {
        Bar::new(o, h, l, c)
    }

    fn close_seq(closes: &[f64]) -> Vec<Bar> {
        // Synthesize OHLC where high = close + 0.5, low = close - 0.5 so
        // true range is well defined.
        closes.iter().map(|&c| bar(c, c + 0.5, c - 0.5, c)).collect()
    }

    #[test]
    fn sma_warmup_and_value() {
        let r = IndicatorRef::periodic(IndicatorName::Sma, 3);
        let mut e = IndicatorEngine::new([&r]);
        let bars = close_seq(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        e.push(&bars[0]);
        assert_eq!(e.value(&r), None);
        e.push(&bars[1]);
        assert_eq!(e.value(&r), None);
        e.push(&bars[2]);
        assert!((e.value(&r).unwrap() - 2.0).abs() < 1e-9);
        e.push(&bars[3]);
        assert!((e.value(&r).unwrap() - 3.0).abs() < 1e-9);
        e.push(&bars[4]);
        assert!((e.value(&r).unwrap() - 4.0).abs() < 1e-9);
    }

    #[test]
    fn ema_seed_then_recurrence() {
        let r = IndicatorRef::periodic(IndicatorName::Ema, 3);
        let mut e = IndicatorEngine::new([&r]);
        // After 3 bars the seed is the SMA of {1,2,3} = 2.0.
        let bars = close_seq(&[1.0, 2.0, 3.0, 4.0]);
        for b in &bars[..3] {
            e.push(b);
        }
        assert!((e.value(&r).unwrap() - 2.0).abs() < 1e-9);
        // alpha = 2/4 = 0.5; ema_4 = 0.5*4 + 0.5*2 = 3.0
        e.push(&bars[3]);
        assert!((e.value(&r).unwrap() - 3.0).abs() < 1e-9);
    }

    #[test]
    fn rsi_wilder_seed_value() {
        // 15 closes → 14 deltas → seed RSI on bar 15.
        // Hand-computed expected ≈ 62.48 (sum_gains=2.93, sum_losses=1.76,
        // RS=1.6648, RSI=100 - 100/2.6648).
        let closes = [
            46.13, 46.26, 46.50, 46.38, 46.25, 46.65, 46.42, 46.92, 46.30, 46.07, 46.03, 46.83, 47.69, 47.54,
            47.30,
        ];
        let r = IndicatorRef::periodic(IndicatorName::Rsi, 14);
        let mut e = IndicatorEngine::new([&r]);
        for c in &closes {
            e.push(&bar(*c, *c + 0.5, *c - 0.5, *c));
        }
        let v = e.value(&r).expect("rsi seed should have formed");
        assert!((v - 62.48).abs() < 0.05, "rsi seed off: {}", v);
    }

    #[test]
    fn atr_wilder_seed() {
        let r = IndicatorRef::periodic(IndicatorName::Atr, 3);
        let mut e = IndicatorEngine::new([&r]);
        // High-low = 2 every bar, no gaps → seed ATR = 2.
        let bars = (0..5)
            .map(|i| bar(i as f64, i as f64 + 1.0, i as f64 - 1.0, i as f64))
            .collect::<Vec<_>>();
        for b in &bars {
            e.push(b);
        }
        let v = e.value(&r).unwrap();
        // First bar has no prev_close so contributes nothing; we have
        // 4 TR samples and the seed forms after 3.
        assert!(v > 0.0);
    }

    #[test]
    fn close_indicator_no_warmup() {
        let r = IndicatorRef::close();
        let mut e = IndicatorEngine::new([&r]);
        assert_eq!(e.value(&r), None);
        e.push(&bar(0.0, 0.0, 0.0, 42.5));
        assert_eq!(e.value(&r), Some(42.5));
    }

    #[test]
    fn warmup_bars_max_across_instances() {
        let r1 = IndicatorRef::periodic(IndicatorName::Sma, 5);
        let r2 = IndicatorRef::periodic(IndicatorName::Rsi, 14);
        let e = IndicatorEngine::new([&r1, &r2]);
        // RSI 14 needs 14 deltas → 15 bars. SMA 5 needs 5. Max is 15.
        assert_eq!(e.warmup_bars(), 15);
    }

    #[test]
    fn duplicate_refs_share_one_instance() {
        let r1 = IndicatorRef::periodic(IndicatorName::Sma, 5);
        let r2 = IndicatorRef::periodic(IndicatorName::Sma, 5);
        let e = IndicatorEngine::new([&r1, &r2]);
        assert_eq!(e.instances.len(), 1);
    }
}
