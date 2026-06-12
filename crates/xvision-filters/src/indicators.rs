//! Incremental indicator math for the filter DSL indicator catalog.
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
//! * **RVOL** — `volume[t] / SMA(volume, period)`. Time-of-day aware: bars are
//!   bucketed by `hour*60+minute`; the SMA uses the last `period` bars in the
//!   same time slot. Warmup = `period` bars in the slot. Before the slot is warm
//!   the indicator falls back to a cross-bar rolling SMA over all bars pushed so
//!   far (same `period`), so a value is always available once `period` total bars
//!   have been seen regardless of time-of-day distribution.
//! * **VolumeZscore** — z-score of `volume[t]` relative to the last `period`
//!   volumes. Warmup = `period` bars.
//! * **ROC** — `100 * (close[t] - close[t-period]) / close[t-period]`.
//!   Warmup = `period + 1` bars (needs a bar `period` steps back).
//! * **Donchian / Highest / Lowest** — upper/lower are the max/min of the
//!   `period` bars BEFORE bar `t` (pre-push snapshot). This ensures
//!   `close crossed_above donchian_upper_N` is not structurally impossible.
//!   Warmup = `period + 1` bars (window must be full before the snapshot is
//!   taken). WilliamsR shares the `DonchianState` struct but uses the
//!   post-push window (current bar included); its warmup remains `period`.
//!
//! All warmups are inclusive: a `period=14` EMA produces its first value
//! on the 14th `push` call (1-based), i.e. after 14 closes have been
//! observed. This module is internally 1-based (counts of bars
//! consumed); the runtime translates to/from bar indices as needed.

use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Datelike, Timelike, Utc};

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
    pub volume: f64,
    pub timestamp: Option<DateTime<Utc>>,
    /// Perps funding rate (per-interval fraction). `None` for spot bars.
    pub funding_rate: Option<f64>,
    /// Open interest in USD. `None` for spot bars.
    pub open_interest: Option<f64>,
    /// Venue mark price. `None` for spot bars.
    pub mark_price: Option<f64>,
    /// Mark − index basis (fraction). `None` for spot bars.
    pub mark_index_basis: Option<f64>,
    /// Long/short account ratio. `None` for spot bars.
    pub long_short_ratio: Option<f64>,
}

impl Bar {
    pub fn new(open: f64, high: f64, low: f64, close: f64) -> Self {
        Self::with_volume(open, high, low, close, 0.0)
    }

    pub fn with_volume(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        Self {
            open,
            high,
            low,
            close,
            volume,
            timestamp: None,
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            mark_index_basis: None,
            long_short_ratio: None,
        }
    }

    pub fn with_timestamp(
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        timestamp: DateTime<Utc>,
    ) -> Self {
        Self {
            open,
            high,
            low,
            close,
            volume,
            timestamp: Some(timestamp),
            funding_rate: None,
            open_interest: None,
            mark_price: None,
            mark_index_basis: None,
            long_short_ratio: None,
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
    last_open: Option<f64>,
    last_high: Option<f64>,
    last_low: Option<f64>,
    last_close: Option<f64>,
    last_volume: Option<f64>,
    last_funding_rate: Option<f64>,
    last_open_interest: Option<f64>,
    last_mark_price: Option<f64>,
    last_mark_index_basis: Option<f64>,
    last_long_short_ratio: Option<f64>,
    obv_value: f64,
    obv_started: bool,
    calendar: CalendarLevels,
}

#[derive(Debug)]
enum Instance {
    Sma(SmaState),
    Ema(EmaState),
    Wma(WindowState),
    Rsi(RsiState),
    Atr(AtrState),
    AtrPct(AtrState),
    Roc(RocState),
    Dmi(DmiState),
    Macd(MacdState),
    Bollinger(BollingerState),
    Donchian(DonchianState),
    Stoch(StochState),
    StochRsi(StochRsiState),
    Cci(CciState),
    Mfi(MfiState),
    Vwap(VwapState),
    VolumeSma(SmaState),
    Rvol(RvolState),
    VolumeZscore(VolumeZscoreState),
    Ichimoku(IchimokuState),
    OpeningRange(OpeningRangeState),
    Keltner(KeltnerState),
    WilliamsR(DonchianState),
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
                IndicatorName::Wma => Instance::Wma(WindowState::new(key.period as usize)),
                IndicatorName::Rsi => Instance::Rsi(RsiState::new(key.period as usize)),
                IndicatorName::Atr => Instance::Atr(AtrState::new(key.period as usize)),
                IndicatorName::AtrPct => Instance::AtrPct(AtrState::new(key.period as usize)),
                IndicatorName::Roc => Instance::Roc(RocState::new(key.period as usize)),
                IndicatorName::Adx | IndicatorName::DiPlus | IndicatorName::DiMinus => {
                    Instance::Dmi(DmiState::new(key.period as usize))
                }
                IndicatorName::MacdLine | IndicatorName::MacdSignal | IndicatorName::MacdHist => {
                    Instance::Macd(MacdState::default())
                }
                IndicatorName::BbUpper
                | IndicatorName::BbMiddle
                | IndicatorName::BbLower
                | IndicatorName::BbWidth
                | IndicatorName::BbPercentB => Instance::Bollinger(BollingerState::new(key.period as usize)),
                IndicatorName::DonchianUpper
                | IndicatorName::DonchianMiddle
                | IndicatorName::DonchianLower => Instance::Donchian(DonchianState::new(key.period as usize)),
                IndicatorName::StochK | IndicatorName::StochD => {
                    Instance::Stoch(StochState::new(key.period as usize))
                }
                IndicatorName::StochRsiK | IndicatorName::StochRsiD => {
                    Instance::StochRsi(StochRsiState::new(key.period as usize))
                }
                IndicatorName::Cci => Instance::Cci(CciState::new(key.period as usize)),
                IndicatorName::Mfi => Instance::Mfi(MfiState::new(key.period as usize)),
                IndicatorName::Vwap => Instance::Vwap(VwapState::new(key.period as usize)),
                IndicatorName::VolumeSma => Instance::VolumeSma(SmaState::new(key.period as usize)),
                IndicatorName::Rvol | IndicatorName::RvolTod => {
                    Instance::Rvol(RvolState::new(key.period as usize))
                }
                IndicatorName::VolumeZscore => {
                    Instance::VolumeZscore(VolumeZscoreState::new(key.period as usize))
                }
                IndicatorName::Tenkan
                | IndicatorName::Kijun
                | IndicatorName::SenkouA
                | IndicatorName::SenkouB
                | IndicatorName::Chikou
                | IndicatorName::CloudTop
                | IndicatorName::CloudBottom
                | IndicatorName::CloudThickness => Instance::Ichimoku(IchimokuState::new()),
                IndicatorName::Highest | IndicatorName::Lowest => {
                    Instance::Donchian(DonchianState::new(key.period as usize))
                }
                IndicatorName::OpeningRangeHigh
                | IndicatorName::OpeningRangeLow
                | IndicatorName::OpeningRangeMid => {
                    Instance::OpeningRange(OpeningRangeState::new(key.period))
                }
                IndicatorName::KeltnerUpper | IndicatorName::KeltnerMiddle | IndicatorName::KeltnerLower => {
                    Instance::Keltner(KeltnerState::new(key.period as usize))
                }
                IndicatorName::WilliamsR => Instance::WilliamsR(DonchianState::new(key.period as usize)),
                IndicatorName::Open
                | IndicatorName::High
                | IndicatorName::Low
                | IndicatorName::Close
                | IndicatorName::Volume
                | IndicatorName::Obv
                | IndicatorName::PrevDayOpen
                | IndicatorName::PrevDayHigh
                | IndicatorName::PrevDayLow
                | IndicatorName::PrevDayClose
                | IndicatorName::PrevWeekHigh
                | IndicatorName::PrevWeekLow
                | IndicatorName::PrevWeekClose
                | IndicatorName::PrevMonthOpen
                | IndicatorName::PrevMonthHigh
                | IndicatorName::PrevMonthLow
                | IndicatorName::PrevMonthClose
                | IndicatorName::PremarketHigh
                | IndicatorName::PremarketLow
                | IndicatorName::GapPct
                | IndicatorName::GapUp
                | IndicatorName::GapDown
                | IndicatorName::FundingRate
                | IndicatorName::OpenInterest
                | IndicatorName::MarkPrice
                | IndicatorName::MarkIndexBasis
                | IndicatorName::LongShortRatio => continue, // no per-instance state
            };
            instances.insert(key, inst);
        }
        Self {
            instances,
            bars_seen: 0,
            last_open: None,
            last_high: None,
            last_low: None,
            last_close: None,
            last_volume: None,
            last_funding_rate: None,
            last_open_interest: None,
            last_mark_price: None,
            last_mark_index_basis: None,
            last_long_short_ratio: None,
            obv_value: 0.0,
            obv_started: false,
            calendar: CalendarLevels::default(),
        }
    }

    /// Feed one bar. Updates every tracked instance.
    pub fn push(&mut self, bar: &Bar) {
        let prev_close = self.last_close;
        for (_, inst) in self.instances.iter_mut() {
            match inst {
                Instance::Sma(s) => s.push(bar.close),
                Instance::Ema(s) => s.push(bar.close),
                Instance::Wma(s) => s.push(bar.close),
                Instance::Rsi(s) => s.push(bar.close),
                Instance::Atr(s) | Instance::AtrPct(s) => s.push(bar.high, bar.low, bar.close, prev_close),
                Instance::Roc(s) => s.push(bar.close),
                Instance::Dmi(s) => s.push(bar.high, bar.low, bar.close),
                Instance::Macd(s) => s.push(bar.close),
                Instance::Bollinger(s) => s.push(bar.close),
                Instance::Donchian(s) => s.push(bar.high, bar.low),
                Instance::Stoch(s) => s.push(bar.high, bar.low, bar.close),
                Instance::StochRsi(s) => s.push(bar.close),
                Instance::Cci(s) => s.push(bar.high, bar.low, bar.close),
                Instance::Mfi(s) => s.push(bar.high, bar.low, bar.close, bar.volume),
                Instance::Vwap(s) => s.push(bar.high, bar.low, bar.close, bar.volume),
                Instance::VolumeSma(s) => s.push(bar.volume),
                Instance::Rvol(s) => s.push(bar.volume, bar.timestamp),
                Instance::VolumeZscore(s) => s.push(bar.volume),
                Instance::Ichimoku(s) => s.push(bar.high, bar.low, bar.close),
                Instance::OpeningRange(s) => s.push(bar),
                Instance::Keltner(s) => s.push(bar.high, bar.low, bar.close, prev_close),
                Instance::WilliamsR(s) => s.push(bar.high, bar.low),
            }
        }
        if let Some(prev) = prev_close {
            if bar.close > prev {
                self.obv_value += bar.volume;
            } else if bar.close < prev {
                self.obv_value -= bar.volume;
            }
        }
        self.obv_started = true;
        self.last_open = Some(bar.open);
        self.last_high = Some(bar.high);
        self.last_low = Some(bar.low);
        self.last_close = Some(bar.close);
        self.last_volume = Some(bar.volume);
        // Perps scalars: overwrite only when the bar carries a value so a
        // spot bar (all-None) does not erase the last-known perps reading.
        if bar.funding_rate.is_some() {
            self.last_funding_rate = bar.funding_rate;
        }
        if bar.open_interest.is_some() {
            self.last_open_interest = bar.open_interest;
        }
        if bar.mark_price.is_some() {
            self.last_mark_price = bar.mark_price;
        }
        if bar.mark_index_basis.is_some() {
            self.last_mark_index_basis = bar.mark_index_basis;
        }
        if bar.long_short_ratio.is_some() {
            self.last_long_short_ratio = bar.long_short_ratio;
        }
        self.calendar.push(bar);
        self.bars_seen += 1;
    }

    /// Current value for an indicator reference. `None` while the
    /// instance is still warming up. `Close` always returns the latest
    /// close once at least one bar has been pushed.
    pub fn value(&self, r: &IndicatorRef) -> Option<f64> {
        match r.name {
            IndicatorName::Open => return self.last_open,
            IndicatorName::High => return self.last_high,
            IndicatorName::Low => return self.last_low,
            IndicatorName::Close => return self.last_close,
            IndicatorName::Volume => return self.last_volume,
            IndicatorName::Obv => return self.obv_started.then_some(self.obv_value),
            IndicatorName::PrevDayOpen => return self.calendar.prev_day.map(|d| d.open),
            IndicatorName::PrevDayHigh => return self.calendar.prev_day.map(|d| d.high),
            IndicatorName::PrevDayLow => return self.calendar.prev_day.map(|d| d.low),
            IndicatorName::PrevDayClose => return self.calendar.prev_day.map(|d| d.close),
            IndicatorName::PrevWeekHigh => return self.calendar.prev_week.map(|w| w.high),
            IndicatorName::PrevWeekLow => return self.calendar.prev_week.map(|w| w.low),
            IndicatorName::PrevWeekClose => return self.calendar.prev_week.map(|w| w.close),
            IndicatorName::PrevMonthOpen => return self.calendar.prev_month.map(|m| m.open),
            IndicatorName::PrevMonthHigh => return self.calendar.prev_month.map(|m| m.high),
            IndicatorName::PrevMonthLow => return self.calendar.prev_month.map(|m| m.low),
            IndicatorName::PrevMonthClose => return self.calendar.prev_month.map(|m| m.close),
            IndicatorName::PremarketHigh => return self.calendar.premarket_high,
            IndicatorName::PremarketLow => return self.calendar.premarket_low,
            IndicatorName::GapPct => return self.calendar.gap_pct,
            IndicatorName::GapUp => return self.calendar.gap_pct.map(|v| if v > 0.0 { 1.0 } else { 0.0 }),
            IndicatorName::GapDown => return self.calendar.gap_pct.map(|v| if v < 0.0 { 1.0 } else { 0.0 }),
            IndicatorName::FundingRate => return self.last_funding_rate,
            IndicatorName::OpenInterest => return self.last_open_interest,
            IndicatorName::MarkPrice => return self.last_mark_price,
            IndicatorName::MarkIndexBasis => return self.last_mark_index_basis,
            IndicatorName::LongShortRatio => return self.last_long_short_ratio,
            _ => {}
        }
        let key = IndicatorKey::from_ref(r);
        match self.instances.get(&key)? {
            Instance::Sma(s) => s.value(),
            Instance::Ema(s) => s.value(),
            Instance::Wma(s) => s.wma(),
            Instance::Rsi(s) => s.value(),
            Instance::Atr(s) => s.value(),
            Instance::AtrPct(s) => match (s.value(), self.last_close) {
                (Some(atr), Some(close)) if close.abs() > f64::EPSILON => Some(100.0 * atr / close),
                _ => None,
            },
            Instance::Roc(s) => s.value(),
            Instance::Dmi(s) => match r.name {
                IndicatorName::Adx => s.adx(),
                IndicatorName::DiPlus => s.di_plus(),
                IndicatorName::DiMinus => s.di_minus(),
                _ => None,
            },
            Instance::Macd(s) => match r.name {
                IndicatorName::MacdLine => s.line(),
                IndicatorName::MacdSignal => s.signal(),
                IndicatorName::MacdHist => s.hist(),
                _ => None,
            },
            Instance::Bollinger(s) => match r.name {
                IndicatorName::BbUpper => s.upper(),
                IndicatorName::BbMiddle => s.middle(),
                IndicatorName::BbLower => s.lower(),
                IndicatorName::BbWidth => s.width(),
                IndicatorName::BbPercentB => s.percent_b(),
                _ => None,
            },
            Instance::Donchian(s) => match r.name {
                IndicatorName::DonchianUpper => s.upper(),
                IndicatorName::DonchianMiddle => s.middle(),
                IndicatorName::DonchianLower => s.lower(),
                IndicatorName::Highest => s.upper(),
                IndicatorName::Lowest => s.lower(),
                _ => None,
            },
            Instance::Stoch(s) => match r.name {
                IndicatorName::StochK => s.k(),
                IndicatorName::StochD => s.d(),
                _ => None,
            },
            Instance::StochRsi(s) => match r.name {
                IndicatorName::StochRsiK => s.k(),
                IndicatorName::StochRsiD => s.d(),
                _ => None,
            },
            Instance::Cci(s) => s.value(),
            Instance::Mfi(s) => s.value(),
            Instance::Vwap(s) => s.value(),
            Instance::VolumeSma(s) => s.value(),
            Instance::Rvol(s) => s.value(),
            Instance::VolumeZscore(s) => s.value(),
            Instance::Ichimoku(s) => match r.name {
                IndicatorName::Tenkan => s.tenkan(),
                IndicatorName::Kijun => s.kijun(),
                IndicatorName::SenkouA => s.senkou_a(),
                IndicatorName::SenkouB => s.senkou_b(),
                IndicatorName::Chikou => s.chikou(),
                IndicatorName::CloudTop => s.cloud_top(),
                IndicatorName::CloudBottom => s.cloud_bottom(),
                IndicatorName::CloudThickness => s.cloud_thickness(),
                _ => None,
            },
            Instance::OpeningRange(s) => match r.name {
                IndicatorName::OpeningRangeHigh => s.high(),
                IndicatorName::OpeningRangeLow => s.low(),
                IndicatorName::OpeningRangeMid => s.mid(),
                _ => None,
            },
            Instance::Keltner(s) => match r.name {
                IndicatorName::KeltnerUpper => s.upper(),
                IndicatorName::KeltnerMiddle => s.middle(),
                IndicatorName::KeltnerLower => s.lower(),
                _ => None,
            },
            Instance::WilliamsR(s) => match (s.current_upper(), s.current_lower(), self.last_close) {
                (Some(hh), Some(ll), Some(close)) if (hh - ll).abs() > f64::EPSILON => {
                    Some(-100.0 * (hh - close) / (hh - ll))
                }
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
                Instance::Sma(_)
                | Instance::Ema(_)
                | Instance::Wma(_)
                | Instance::Bollinger(_)
                | Instance::Vwap(_)
                | Instance::VolumeSma(_)
                | Instance::Rvol(_)
                | Instance::VolumeZscore(_)
                | Instance::WilliamsR(_) => key.period,
                // Donchian/Highest/Lowest snapshot the window BEFORE the
                // current bar is pushed (committed_upper/lower), so they
                // need one extra bar to produce their first valid value.
                Instance::Donchian(_) => key.period + 1,
                Instance::OpeningRange(_) => 0,
                Instance::Rsi(_) | Instance::Atr(_) | Instance::AtrPct(_) => key.period + 1,
                Instance::Dmi(_) => key.period * 2 + 1,
                Instance::Roc(_) => key.period + 1,
                Instance::Macd(_) => 35,
                Instance::Stoch(_) => key.period + 2,
                Instance::StochRsi(_) => key.period * 2 + 2,
                Instance::Cci(_) => key.period,
                Instance::Mfi(_) => key.period + 1,
                Instance::Ichimoku(_) => 52,
                Instance::Keltner(_) => key.period + 1,
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
// Shared rolling-window helpers and standard indicator components
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct WindowState {
    period: usize,
    window: VecDeque<f64>,
    sum: f64,
}

impl WindowState {
    fn new(period: usize) -> Self {
        Self {
            period,
            window: VecDeque::with_capacity(period),
            sum: 0.0,
        }
    }

    fn push(&mut self, value: f64) {
        self.window.push_back(value);
        self.sum += value;
        if self.window.len() > self.period {
            self.sum -= self.window.pop_front().expect("window non-empty");
        }
    }

    fn is_full(&self) -> bool {
        self.window.len() == self.period
    }

    fn mean(&self) -> Option<f64> {
        self.is_full().then_some(self.sum / self.period as f64)
    }

    fn stddev(&self) -> Option<f64> {
        let mean = self.mean()?;
        let var = self
            .window
            .iter()
            .map(|v| {
                let d = *v - mean;
                d * d
            })
            .sum::<f64>()
            / self.period as f64;
        Some(var.sqrt())
    }

    fn min(&self) -> Option<f64> {
        self.is_full()
            .then(|| self.window.iter().fold(f64::INFINITY, |a, b| a.min(*b)))
    }

    fn max(&self) -> Option<f64> {
        self.is_full()
            .then(|| self.window.iter().fold(f64::NEG_INFINITY, |a, b| a.max(*b)))
    }

    fn wma(&self) -> Option<f64> {
        if !self.is_full() {
            return None;
        }
        let (weighted, denom) =
            self.window
                .iter()
                .enumerate()
                .fold((0.0, 0.0), |(sum, weight_sum), (idx, value)| {
                    let weight = (idx + 1) as f64;
                    (sum + *value * weight, weight_sum + weight)
                });
        Some(weighted / denom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DateKey {
    year: i32,
    ordinal: u32,
}

impl DateKey {
    fn from_ts(ts: DateTime<Utc>) -> Self {
        Self {
            year: ts.year(),
            ordinal: ts.ordinal(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WeekKey {
    year: i32,
    week: u32,
}

impl WeekKey {
    fn from_ts(ts: DateTime<Utc>) -> Self {
        let week = ts.iso_week();
        Self {
            year: week.year(),
            week: week.week(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MonthKey {
    year: i32,
    month: u32,
}

impl MonthKey {
    fn from_ts(ts: DateTime<Utc>) -> Self {
        Self {
            year: ts.year(),
            month: ts.month(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct OhlcRange {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

impl OhlcRange {
    fn new(bar: &Bar) -> Self {
        Self {
            open: bar.open,
            high: bar.high,
            low: bar.low,
            close: bar.close,
        }
    }

    fn push(&mut self, bar: &Bar) {
        self.high = self.high.max(bar.high);
        self.low = self.low.min(bar.low);
        self.close = bar.close;
    }
}

#[derive(Debug, Default)]
struct CalendarLevels {
    current_day_key: Option<DateKey>,
    current_day: Option<OhlcRange>,
    prev_day: Option<OhlcRange>,
    current_week_key: Option<WeekKey>,
    current_week: Option<OhlcRange>,
    prev_week: Option<OhlcRange>,
    current_month_key: Option<MonthKey>,
    current_month: Option<OhlcRange>,
    prev_month: Option<OhlcRange>,
    premarket_day_key: Option<DateKey>,
    premarket_high: Option<f64>,
    premarket_low: Option<f64>,
    gap_pct: Option<f64>,
}

impl CalendarLevels {
    fn push(&mut self, bar: &Bar) {
        let Some(ts) = bar.timestamp else {
            return;
        };
        let day_key = DateKey::from_ts(ts);
        if self.current_day_key != Some(day_key) {
            self.prev_day = self.current_day;
            self.current_day_key = Some(day_key);
            self.current_day = Some(OhlcRange::new(bar));
            self.gap_pct = self.prev_day.and_then(|prev| {
                if prev.close.abs() > f64::EPSILON {
                    Some(100.0 * (bar.open - prev.close) / prev.close)
                } else {
                    None
                }
            });
            self.premarket_day_key = Some(day_key);
            self.premarket_high = None;
            self.premarket_low = None;
        } else if let Some(day) = self.current_day.as_mut() {
            day.push(bar);
        }

        let week_key = WeekKey::from_ts(ts);
        if self.current_week_key != Some(week_key) {
            self.prev_week = self.current_week;
            self.current_week_key = Some(week_key);
            self.current_week = Some(OhlcRange::new(bar));
        } else if let Some(week) = self.current_week.as_mut() {
            week.push(bar);
        }

        let month_key = MonthKey::from_ts(ts);
        if self.current_month_key != Some(month_key) {
            self.prev_month = self.current_month;
            self.current_month_key = Some(month_key);
            self.current_month = Some(OhlcRange::new(bar));
        } else if let Some(month) = self.current_month.as_mut() {
            month.push(bar);
        }

        if is_premarket_utc(ts) {
            self.premarket_high = Some(self.premarket_high.map_or(bar.high, |v| v.max(bar.high)));
            self.premarket_low = Some(self.premarket_low.map_or(bar.low, |v| v.min(bar.low)));
        }
    }
}

fn is_premarket_utc(ts: DateTime<Utc>) -> bool {
    let minutes = ts.hour() * 60 + ts.minute();
    // Equity premarket approximation in UTC. Kept deterministic and
    // timezone-free inside this engine-independent crate.
    (4 * 60..9 * 60 + 30).contains(&minutes)
}

#[derive(Debug)]
struct VolumeZscoreState {
    window: WindowState,
    current: Option<f64>,
}

impl VolumeZscoreState {
    fn new(period: usize) -> Self {
        Self {
            window: WindowState::new(period),
            current: None,
        }
    }

    fn push(&mut self, volume: f64) {
        self.window.push(volume);
        self.current = Some(volume);
    }

    fn value(&self) -> Option<f64> {
        let mean = self.window.mean()?;
        let stddev = self.window.stddev()?;
        if stddev <= f64::EPSILON {
            return Some(0.0);
        }
        Some((self.current? - mean) / stddev)
    }
}

#[derive(Debug)]
struct OpeningRangeState {
    minutes: u32,
    day_key: Option<DateKey>,
    start_ts: Option<DateTime<Utc>>,
    high: Option<f64>,
    low: Option<f64>,
    locked: bool,
}

impl OpeningRangeState {
    fn new(minutes: u32) -> Self {
        Self {
            minutes,
            day_key: None,
            start_ts: None,
            high: None,
            low: None,
            locked: false,
        }
    }

    fn push(&mut self, bar: &Bar) {
        let Some(ts) = bar.timestamp else {
            return;
        };
        let day_key = DateKey::from_ts(ts);
        if self.day_key != Some(day_key) {
            self.day_key = Some(day_key);
            self.start_ts = Some(ts);
            self.high = Some(bar.high);
            self.low = Some(bar.low);
            self.locked = false;
            return;
        }

        let Some(start) = self.start_ts else {
            return;
        };
        let elapsed = ts.signed_duration_since(start).num_minutes().max(0) as u32;
        if elapsed < self.minutes {
            self.high = Some(self.high.map_or(bar.high, |v| v.max(bar.high)));
            self.low = Some(self.low.map_or(bar.low, |v| v.min(bar.low)));
        } else {
            self.locked = true;
        }
    }

    fn high(&self) -> Option<f64> {
        self.locked.then_some(self.high?)
    }

    fn low(&self) -> Option<f64> {
        self.locked.then_some(self.low?)
    }

    fn mid(&self) -> Option<f64> {
        Some((self.high()? + self.low()?) / 2.0)
    }
}

#[derive(Debug)]
struct DmiState {
    period: usize,
    prev_high: Option<f64>,
    prev_low: Option<f64>,
    prev_close: Option<f64>,
    seed_tr: Vec<f64>,
    seed_plus_dm: Vec<f64>,
    seed_minus_dm: Vec<f64>,
    smoothed_tr: Option<f64>,
    smoothed_plus_dm: Option<f64>,
    smoothed_minus_dm: Option<f64>,
    seed_dx: Vec<f64>,
    adx: Option<f64>,
}

impl DmiState {
    fn new(period: usize) -> Self {
        Self {
            period,
            prev_high: None,
            prev_low: None,
            prev_close: None,
            seed_tr: Vec::with_capacity(period),
            seed_plus_dm: Vec::with_capacity(period),
            seed_minus_dm: Vec::with_capacity(period),
            smoothed_tr: None,
            smoothed_plus_dm: None,
            smoothed_minus_dm: None,
            seed_dx: Vec::with_capacity(period),
            adx: None,
        }
    }

    fn push(&mut self, high: f64, low: f64, close: f64) {
        let (Some(prev_high), Some(prev_low), Some(prev_close)) =
            (self.prev_high, self.prev_low, self.prev_close)
        else {
            self.prev_high = Some(high);
            self.prev_low = Some(low);
            self.prev_close = Some(close);
            return;
        };

        let up_move = high - prev_high;
        let down_move = prev_low - low;
        let plus_dm = if up_move > down_move && up_move > 0.0 {
            up_move
        } else {
            0.0
        };
        let minus_dm = if down_move > up_move && down_move > 0.0 {
            down_move
        } else {
            0.0
        };
        let tr = true_range(high, low, prev_close);

        match (self.smoothed_tr, self.smoothed_plus_dm, self.smoothed_minus_dm) {
            (Some(tr_s), Some(plus_s), Some(minus_s)) => {
                let p = self.period as f64;
                self.smoothed_tr = Some(tr_s - tr_s / p + tr);
                self.smoothed_plus_dm = Some(plus_s - plus_s / p + plus_dm);
                self.smoothed_minus_dm = Some(minus_s - minus_s / p + minus_dm);
                self.update_adx();
            }
            _ => {
                self.seed_tr.push(tr);
                self.seed_plus_dm.push(plus_dm);
                self.seed_minus_dm.push(minus_dm);
                if self.seed_tr.len() == self.period {
                    self.smoothed_tr = Some(self.seed_tr.iter().sum());
                    self.smoothed_plus_dm = Some(self.seed_plus_dm.iter().sum());
                    self.smoothed_minus_dm = Some(self.seed_minus_dm.iter().sum());
                    self.seed_tr.clear();
                    self.seed_plus_dm.clear();
                    self.seed_minus_dm.clear();
                    self.update_adx();
                }
            }
        }

        self.prev_high = Some(high);
        self.prev_low = Some(low);
        self.prev_close = Some(close);
    }

    fn update_adx(&mut self) {
        let Some(dx) = self.dx() else {
            return;
        };
        if self.adx.is_none() {
            self.seed_dx.push(dx);
            if self.seed_dx.len() == self.period {
                self.adx = Some(self.seed_dx.iter().sum::<f64>() / self.period as f64);
                self.seed_dx.clear();
            }
        } else {
            let p = self.period as f64;
            let prev = self.adx.unwrap();
            self.adx = Some((prev * (p - 1.0) + dx) / p);
        }
    }

    fn di_plus(&self) -> Option<f64> {
        let tr = self.smoothed_tr?;
        if tr.abs() <= f64::EPSILON {
            return Some(0.0);
        }
        Some(100.0 * self.smoothed_plus_dm? / tr)
    }

    fn di_minus(&self) -> Option<f64> {
        let tr = self.smoothed_tr?;
        if tr.abs() <= f64::EPSILON {
            return Some(0.0);
        }
        Some(100.0 * self.smoothed_minus_dm? / tr)
    }

    fn dx(&self) -> Option<f64> {
        let plus = self.di_plus()?;
        let minus = self.di_minus()?;
        let denom = plus + minus;
        if denom.abs() <= f64::EPSILON {
            return Some(0.0);
        }
        Some(100.0 * (plus - minus).abs() / denom)
    }

    fn adx(&self) -> Option<f64> {
        self.adx
    }
}

#[derive(Debug)]
struct RocState {
    period: usize,
    window: VecDeque<f64>,
    value: Option<f64>,
}

impl RocState {
    fn new(period: usize) -> Self {
        Self {
            period,
            window: VecDeque::with_capacity(period + 1),
            value: None,
        }
    }

    fn push(&mut self, close: f64) {
        self.window.push_back(close);
        if self.window.len() > self.period + 1 {
            self.window.pop_front();
        }
        if self.window.len() == self.period + 1 {
            let prior = self.window.front().copied().unwrap_or(0.0);
            self.value = if prior.abs() > f64::EPSILON {
                Some(100.0 * (close - prior) / prior)
            } else {
                None
            };
        }
    }

    fn value(&self) -> Option<f64> {
        self.value
    }
}

#[derive(Debug)]
struct MacdState {
    fast: EmaState,
    slow: EmaState,
    signal: EmaState,
    line: Option<f64>,
}

impl Default for MacdState {
    fn default() -> Self {
        Self {
            fast: EmaState::new(12),
            slow: EmaState::new(26),
            signal: EmaState::new(9),
            line: None,
        }
    }
}

impl MacdState {
    fn push(&mut self, close: f64) {
        self.fast.push(close);
        self.slow.push(close);
        self.line = match (self.fast.value(), self.slow.value()) {
            (Some(fast), Some(slow)) => {
                let line = fast - slow;
                self.signal.push(line);
                Some(line)
            }
            _ => None,
        };
    }

    fn line(&self) -> Option<f64> {
        self.line
    }

    fn signal(&self) -> Option<f64> {
        self.signal.value()
    }

    fn hist(&self) -> Option<f64> {
        Some(self.line()? - self.signal()?)
    }
}

#[derive(Debug)]
struct BollingerState {
    window: WindowState,
    last_close: Option<f64>,
}

impl BollingerState {
    fn new(period: usize) -> Self {
        Self {
            window: WindowState::new(period),
            last_close: None,
        }
    }

    fn push(&mut self, close: f64) {
        self.window.push(close);
        self.last_close = Some(close);
    }

    fn middle(&self) -> Option<f64> {
        self.window.mean()
    }

    fn upper(&self) -> Option<f64> {
        Some(self.middle()? + 2.0 * self.window.stddev()?)
    }

    fn lower(&self) -> Option<f64> {
        Some(self.middle()? - 2.0 * self.window.stddev()?)
    }

    fn width(&self) -> Option<f64> {
        let middle = self.middle()?;
        if middle.abs() <= f64::EPSILON {
            return None;
        }
        Some((self.upper()? - self.lower()?) / middle)
    }

    fn percent_b(&self) -> Option<f64> {
        let lower = self.lower()?;
        let upper = self.upper()?;
        let denom = upper - lower;
        if denom.abs() <= f64::EPSILON {
            return None;
        }
        Some((self.last_close? - lower) / denom)
    }
}

#[derive(Debug)]
struct DonchianState {
    highs: WindowState,
    lows: WindowState,
    // Snapshot of max/min taken BEFORE the most-recently pushed bar was
    // incorporated into the window.  upper()/lower() return these committed
    // values so that `close crossed_above donchian_upper_N` is not
    // structurally impossible (current high >= current close always, so a
    // post-push max can never be exceeded by the same bar's close).
    committed_upper: Option<f64>,
    committed_lower: Option<f64>,
}

impl DonchianState {
    fn new(period: usize) -> Self {
        Self {
            highs: WindowState::new(period),
            lows: WindowState::new(period),
            committed_upper: None,
            committed_lower: None,
        }
    }

    fn push(&mut self, high: f64, low: f64) {
        self.committed_upper = self.highs.max();
        self.committed_lower = self.lows.min();
        self.highs.push(high);
        self.lows.push(low);
    }

    /// Pre-push upper band — the breakout level in place before bar T arrived.
    fn upper(&self) -> Option<f64> {
        self.committed_upper
    }

    /// Pre-push lower band.
    fn lower(&self) -> Option<f64> {
        self.committed_lower
    }

    fn middle(&self) -> Option<f64> {
        Some((self.upper()? + self.lower()?) / 2.0)
    }

    /// Post-push upper band including the current bar's high.  WilliamsR
    /// conventionally includes the current bar in its lookback window.
    fn current_upper(&self) -> Option<f64> {
        self.highs.max()
    }

    /// Post-push lower band including the current bar's low.
    fn current_lower(&self) -> Option<f64> {
        self.lows.min()
    }
}

#[derive(Debug)]
struct StochState {
    highs: WindowState,
    lows: WindowState,
    d_sma: SmaState,
    k: Option<f64>,
}

impl StochState {
    fn new(period: usize) -> Self {
        Self {
            highs: WindowState::new(period),
            lows: WindowState::new(period),
            d_sma: SmaState::new(3),
            k: None,
        }
    }

    fn push(&mut self, high: f64, low: f64, close: f64) {
        self.highs.push(high);
        self.lows.push(low);
        self.k = match (self.highs.max(), self.lows.min()) {
            (Some(hh), Some(ll)) if (hh - ll).abs() > f64::EPSILON => Some(100.0 * (close - ll) / (hh - ll)),
            _ => None,
        };
        if let Some(k) = self.k {
            self.d_sma.push(k);
        }
    }

    fn k(&self) -> Option<f64> {
        self.k
    }

    fn d(&self) -> Option<f64> {
        self.d_sma.value()
    }
}

#[derive(Debug)]
struct StochRsiState {
    rsi: RsiState,
    rsi_window: WindowState,
    d_sma: SmaState,
    k: Option<f64>,
}

impl StochRsiState {
    fn new(period: usize) -> Self {
        Self {
            rsi: RsiState::new(period),
            rsi_window: WindowState::new(period),
            d_sma: SmaState::new(3),
            k: None,
        }
    }

    fn push(&mut self, close: f64) {
        self.rsi.push(close);
        if let Some(rsi) = self.rsi.value() {
            self.rsi_window.push(rsi);
            self.k = match (self.rsi_window.max(), self.rsi_window.min()) {
                (Some(max), Some(min)) if (max - min).abs() > f64::EPSILON => {
                    Some(100.0 * (rsi - min) / (max - min))
                }
                (Some(_), Some(_)) => Some(0.0),
                _ => None,
            };
            if let Some(k) = self.k {
                self.d_sma.push(k);
            }
        }
    }

    fn k(&self) -> Option<f64> {
        self.k
    }

    fn d(&self) -> Option<f64> {
        self.d_sma.value()
    }
}

#[derive(Debug)]
struct CciState {
    window: WindowState,
    current_tp: Option<f64>,
}

impl CciState {
    fn new(period: usize) -> Self {
        Self {
            window: WindowState::new(period),
            current_tp: None,
        }
    }

    fn push(&mut self, high: f64, low: f64, close: f64) {
        let tp = typical_price(high, low, close);
        self.window.push(tp);
        self.current_tp = Some(tp);
    }

    fn value(&self) -> Option<f64> {
        if !self.window.is_full() {
            return None;
        }
        let sma = self.window.mean()?;
        let mean_dev =
            self.window.window.iter().map(|v| (*v - sma).abs()).sum::<f64>() / self.window.period as f64;
        if mean_dev.abs() <= f64::EPSILON {
            return Some(0.0);
        }
        Some((self.current_tp? - sma) / (0.015 * mean_dev))
    }
}

#[derive(Debug)]
struct MfiState {
    period: usize,
    prev_tp: Option<f64>,
    pos: VecDeque<f64>,
    neg: VecDeque<f64>,
    pos_sum: f64,
    neg_sum: f64,
}

impl MfiState {
    fn new(period: usize) -> Self {
        Self {
            period,
            prev_tp: None,
            pos: VecDeque::with_capacity(period),
            neg: VecDeque::with_capacity(period),
            pos_sum: 0.0,
            neg_sum: 0.0,
        }
    }

    fn push(&mut self, high: f64, low: f64, close: f64, volume: f64) {
        let tp = typical_price(high, low, close);
        if let Some(prev) = self.prev_tp {
            let flow = tp * volume;
            let (pos, neg) = if tp > prev {
                (flow, 0.0)
            } else if tp < prev {
                (0.0, flow)
            } else {
                (0.0, 0.0)
            };
            self.pos.push_back(pos);
            self.neg.push_back(neg);
            self.pos_sum += pos;
            self.neg_sum += neg;
            if self.pos.len() > self.period {
                self.pos_sum -= self.pos.pop_front().unwrap_or(0.0);
                self.neg_sum -= self.neg.pop_front().unwrap_or(0.0);
            }
        }
        self.prev_tp = Some(tp);
    }

    fn value(&self) -> Option<f64> {
        if self.pos.len() < self.period {
            return None;
        }
        if self.neg_sum.abs() <= f64::EPSILON {
            return Some(100.0);
        }
        let ratio = self.pos_sum / self.neg_sum;
        Some(100.0 - 100.0 / (1.0 + ratio))
    }
}

#[derive(Debug)]
struct VwapState {
    period: usize,
    pv: VecDeque<f64>,
    vol: VecDeque<f64>,
    pv_sum: f64,
    vol_sum: f64,
}

impl VwapState {
    fn new(period: usize) -> Self {
        Self {
            period,
            pv: VecDeque::with_capacity(period),
            vol: VecDeque::with_capacity(period),
            pv_sum: 0.0,
            vol_sum: 0.0,
        }
    }

    fn push(&mut self, high: f64, low: f64, close: f64, volume: f64) {
        let pv = typical_price(high, low, close) * volume;
        self.pv.push_back(pv);
        self.vol.push_back(volume);
        self.pv_sum += pv;
        self.vol_sum += volume;
        if self.pv.len() > self.period {
            self.pv_sum -= self.pv.pop_front().unwrap_or(0.0);
            self.vol_sum -= self.vol.pop_front().unwrap_or(0.0);
        }
    }

    fn value(&self) -> Option<f64> {
        if self.pv.len() < self.period || self.vol_sum.abs() <= f64::EPSILON {
            return None;
        }
        Some(self.pv_sum / self.vol_sum)
    }
}

#[derive(Debug)]
struct RvolState {
    period: usize,
    by_slot: HashMap<u16, (VecDeque<f64>, f64)>,
    rolling: SmaState,
    value: Option<f64>,
}

impl RvolState {
    fn new(period: usize) -> Self {
        Self {
            period,
            by_slot: HashMap::new(),
            rolling: SmaState::new(period),
            value: None,
        }
    }

    fn push(&mut self, volume: f64, timestamp: Option<DateTime<Utc>>) {
        // Always keep rolling warm — used as fallback when a TOD slot hasn't
        // accumulated `period` bars yet, and as the primary path when timestamps
        // are absent.
        self.rolling.push(volume);
        let rolling_rvol = self
            .rolling
            .value()
            .and_then(|avg| (avg.abs() > f64::EPSILON).then_some(volume / avg));

        if let Some(ts) = timestamp {
            let slot = (ts.hour() * 60 + ts.minute()) as u16;
            let entry = self
                .by_slot
                .entry(slot)
                .or_insert_with(|| (VecDeque::with_capacity(self.period + 1), 0.0));
            let (window, sum) = entry;
            window.push_back(volume);
            *sum += volume;
            if window.len() > self.period {
                *sum -= window.pop_front().unwrap_or(0.0);
            }
            self.value = if window.len() == self.period && sum.abs() > f64::EPSILON {
                Some(volume / (*sum / self.period as f64))
            } else {
                // TOD slot not yet warm; fall back to rolling SMA.
                rolling_rvol
            };
            return;
        }

        self.value = rolling_rvol;
    }

    fn value(&self) -> Option<f64> {
        self.value
    }
}

#[derive(Debug)]
struct IchimokuState {
    tenkan: DonchianState,
    kijun: DonchianState,
    senkou_b: DonchianState,
    closes: VecDeque<f64>,
    close_lag: usize,
}

impl IchimokuState {
    fn new() -> Self {
        Self {
            tenkan: DonchianState::new(9),
            kijun: DonchianState::new(26),
            senkou_b: DonchianState::new(52),
            closes: VecDeque::with_capacity(27),
            close_lag: 26,
        }
    }

    fn push(&mut self, high: f64, low: f64, close: f64) {
        self.tenkan.push(high, low);
        self.kijun.push(high, low);
        self.senkou_b.push(high, low);
        self.closes.push_back(close);
        if self.closes.len() > self.close_lag + 1 {
            self.closes.pop_front();
        }
    }

    fn tenkan(&self) -> Option<f64> {
        self.tenkan.middle()
    }

    fn kijun(&self) -> Option<f64> {
        self.kijun.middle()
    }

    fn senkou_a(&self) -> Option<f64> {
        Some((self.tenkan()? + self.kijun()?) / 2.0)
    }

    fn senkou_b(&self) -> Option<f64> {
        self.senkou_b.middle()
    }

    fn chikou(&self) -> Option<f64> {
        (self.closes.len() == self.close_lag + 1)
            .then(|| self.closes.front().copied())
            .flatten()
    }

    fn cloud_top(&self) -> Option<f64> {
        Some(self.senkou_a()?.max(self.senkou_b()?))
    }

    fn cloud_bottom(&self) -> Option<f64> {
        Some(self.senkou_a()?.min(self.senkou_b()?))
    }

    fn cloud_thickness(&self) -> Option<f64> {
        Some((self.senkou_a()? - self.senkou_b()?).abs())
    }
}

#[derive(Debug)]
struct KeltnerState {
    middle: EmaState,
    atr: AtrState,
}

impl KeltnerState {
    fn new(period: usize) -> Self {
        Self {
            middle: EmaState::new(period),
            atr: AtrState::new(period),
        }
    }

    fn push(&mut self, high: f64, low: f64, close: f64, prev_close: Option<f64>) {
        self.middle.push(close);
        self.atr.push(high, low, close, prev_close);
    }

    fn middle(&self) -> Option<f64> {
        self.middle.value()
    }

    fn upper(&self) -> Option<f64> {
        Some(self.middle()? + 2.0 * self.atr.value()?)
    }

    fn lower(&self) -> Option<f64> {
        Some(self.middle()? - 2.0 * self.atr.value()?)
    }
}

fn typical_price(high: f64, low: f64, close: f64) -> f64 {
    (high + low + close) / 3.0
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{IndicatorName, IndicatorRef};
    use chrono::TimeZone;

    fn bar(o: f64, h: f64, l: f64, c: f64) -> Bar {
        Bar::new(o, h, l, c)
    }

    fn close_seq(closes: &[f64]) -> Vec<Bar> {
        // Synthesize OHLC where high = close + 0.5, low = close - 0.5 so
        // true range is well defined.
        closes.iter().map(|&c| bar(c, c + 0.5, c - 0.5, c)).collect()
    }

    fn periodless(name: IndicatorName) -> IndicatorRef {
        IndicatorRef {
            name,
            period: None,
            bar_offset: None,
        }
    }

    #[test]
    fn bar_carries_optional_perps_fields_default_none() {
        let b = Bar::new(1.0, 2.0, 0.5, 1.5);
        assert_eq!(b.funding_rate, None);
        assert_eq!(b.open_interest, None);
        assert_eq!(b.mark_price, None);
        assert_eq!(b.mark_index_basis, None);
        assert_eq!(b.long_short_ratio, None);
    }

    #[test]
    fn engine_reports_latest_perps_values() {
        let mut e = IndicatorEngine::new(std::iter::empty());
        let mut b = Bar::new(1.0, 2.0, 0.5, 1.5);
        b.funding_rate = Some(0.0001);
        b.open_interest = Some(5_000_000.0);
        b.mark_price = Some(1.52);
        b.mark_index_basis = Some(0.0005);
        b.long_short_ratio = Some(1.3);
        e.push(&b);
        assert_eq!(e.value(&periodless(IndicatorName::FundingRate)), Some(0.0001));
        assert_eq!(
            e.value(&periodless(IndicatorName::OpenInterest)),
            Some(5_000_000.0)
        );
        assert_eq!(e.value(&periodless(IndicatorName::MarkPrice)), Some(1.52));
        assert_eq!(e.value(&periodless(IndicatorName::MarkIndexBasis)), Some(0.0005));
        assert_eq!(e.value(&periodless(IndicatorName::LongShortRatio)), Some(1.3));
    }

    #[test]
    fn engine_perps_value_none_before_any_bar() {
        let e = IndicatorEngine::new(std::iter::empty());
        assert_eq!(e.value(&periodless(IndicatorName::FundingRate)), None);
    }

    #[test]
    fn perps_indicators_parse_from_dsl() {
        assert_eq!(
            IndicatorRef::parse_dsl("funding_rate").unwrap().name,
            IndicatorName::FundingRate
        );
        assert_eq!(
            IndicatorRef::parse_dsl("open_interest").unwrap().name,
            IndicatorName::OpenInterest
        );
        assert_eq!(
            IndicatorRef::parse_dsl("mark_price").unwrap().name,
            IndicatorName::MarkPrice
        );
        assert_eq!(
            IndicatorRef::parse_dsl("mark_index_basis").unwrap().name,
            IndicatorName::MarkIndexBasis
        );
        assert_eq!(
            IndicatorRef::parse_dsl("long_short_ratio").unwrap().name,
            IndicatorName::LongShortRatio
        );
    }

    #[test]
    fn perps_indicators_are_periodless() {
        assert!(!IndicatorName::FundingRate.has_period());
        assert!(!IndicatorName::OpenInterest.has_period());
        assert_eq!(IndicatorName::MarkPrice.period_bounds(), None);
        assert_eq!(IndicatorName::FundingRate.dsl_prefix(), "funding_rate");
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
    fn stoch_d_first_value_on_period_plus_2() {
        // StochD = 3-bar SMA of K. K is available once highs/lows window is
        // full (period bars). d_sma then needs 2 more K values → period + 2.
        let r = IndicatorRef::periodic(IndicatorName::StochD, 2);
        let mut e = IndicatorEngine::new([&r]);
        assert_eq!(e.warmup_bars(), 4);
        let bars = (0..3).map(|i| {
            let f = i as f64;
            Bar::with_volume(f, f + 1.0, f, f + 0.5, 1.0)
        });
        for b in bars {
            assert_eq!(e.value(&r), None);
            e.push(&b);
        }
        // 3rd push completes period(2)+2=4 — but we haven't pushed a 4th bar yet
        assert_eq!(e.value(&r), None);
        e.push(&Bar::with_volume(3.0, 4.0, 3.0, 3.5, 1.0));
        assert!(
            e.value(&r).is_some(),
            "StochD should have a value after period+2 bars"
        );
    }

    #[test]
    fn stochrsi_d_first_value_on_2x_period_plus_2() {
        // StochRsiD = 3-bar SMA of StochRsiK.
        // RSI warmup = period+1, rsi_window warmup = period more bars,
        // d_sma warmup = 2 more K values → 2*period + 2 total.
        let r = IndicatorRef::periodic(IndicatorName::StochRsiD, 2);
        let mut e = IndicatorEngine::new([&r]);
        assert_eq!(e.warmup_bars(), 6);
        for i in 0..5u32 {
            assert_eq!(e.value(&r), None, "should be None before bar {}", i + 1);
            e.push(&Bar::new(0.0, (i as f64) + 1.0, 0.0, (i as f64) + 1.0));
        }
        e.push(&Bar::new(0.0, 6.0, 0.0, 6.0));
        assert!(
            e.value(&r).is_some(),
            "StochRsiD should have a value after 2*period+2 bars"
        );
    }

    #[test]
    fn duplicate_refs_share_one_instance() {
        let r1 = IndicatorRef::periodic(IndicatorName::Sma, 5);
        let r2 = IndicatorRef::periodic(IndicatorName::Sma, 5);
        let e = IndicatorEngine::new([&r1, &r2]);
        assert_eq!(e.instances.len(), 1);
    }

    #[test]
    fn expanded_catalog_indicators_produce_values() {
        let refs = [
            IndicatorRef::periodic(IndicatorName::Wma, 5),
            IndicatorRef::periodic(IndicatorName::Roc, 5),
            IndicatorRef::periodic(IndicatorName::BbUpper, 20),
            IndicatorRef::periodic(IndicatorName::BbMiddle, 20),
            IndicatorRef::periodic(IndicatorName::BbLower, 20),
            IndicatorRef::periodic(IndicatorName::BbWidth, 20),
            IndicatorRef::periodic(IndicatorName::BbPercentB, 20),
            IndicatorRef::periodic(IndicatorName::DonchianUpper, 20),
            IndicatorRef::periodic(IndicatorName::DonchianMiddle, 20),
            IndicatorRef::periodic(IndicatorName::DonchianLower, 20),
            IndicatorRef::periodic(IndicatorName::StochK, 14),
            IndicatorRef::periodic(IndicatorName::StochD, 14),
            IndicatorRef::periodic(IndicatorName::Cci, 20),
            IndicatorRef::periodic(IndicatorName::Mfi, 14),
            IndicatorRef::periodic(IndicatorName::Vwap, 20),
            IndicatorRef::periodic(IndicatorName::VolumeSma, 20),
            IndicatorRef::periodic(IndicatorName::Adx, 14),
            IndicatorRef::periodic(IndicatorName::DiPlus, 14),
            IndicatorRef::periodic(IndicatorName::DiMinus, 14),
            IndicatorRef::periodic(IndicatorName::StochRsiK, 14),
            IndicatorRef::periodic(IndicatorName::StochRsiD, 14),
            IndicatorRef::periodic(IndicatorName::Rvol, 3),
            IndicatorRef::periodic(IndicatorName::RvolTod, 3),
            IndicatorRef::periodic(IndicatorName::VolumeZscore, 20),
            IndicatorRef::periodic(IndicatorName::Highest, 20),
            IndicatorRef::periodic(IndicatorName::Lowest, 20),
            IndicatorRef::periodic(IndicatorName::OpeningRangeHigh, 30),
            IndicatorRef::periodic(IndicatorName::OpeningRangeLow, 30),
            IndicatorRef::periodic(IndicatorName::OpeningRangeMid, 30),
            IndicatorRef::periodic(IndicatorName::KeltnerUpper, 20),
            IndicatorRef::periodic(IndicatorName::KeltnerMiddle, 20),
            IndicatorRef::periodic(IndicatorName::KeltnerLower, 20),
            IndicatorRef::periodic(IndicatorName::WilliamsR, 14),
            IndicatorRef {
                name: IndicatorName::Tenkan,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::Kijun,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::SenkouA,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::SenkouB,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::Chikou,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::CloudTop,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::CloudBottom,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::CloudThickness,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevDayOpen,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevDayHigh,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevDayLow,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevDayClose,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevWeekHigh,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevWeekLow,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevWeekClose,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PremarketHigh,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PremarketLow,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::GapPct,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::GapUp,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::GapDown,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::MacdLine,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::MacdSignal,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::MacdHist,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::Obv,
                period: None,
                bar_offset: None,
            },
        ];
        let mut e = IndicatorEngine::new(refs.iter());
        let start = Utc.with_ymd_and_hms(2026, 5, 4, 0, 0, 0).unwrap();
        for i in 1..=400 {
            let close = 100.0 + i as f64 + ((i % 7) as f64 - 3.0);
            e.push(&Bar::with_timestamp(
                close - 0.5,
                close + 2.0,
                close - 2.0,
                close,
                1_000.0 + i as f64,
                start + chrono::Duration::hours(i as i64),
            ));
        }

        for r in &refs {
            assert!(e.value(r).is_some(), "{} should have a value after warmup", r);
        }
    }

    #[test]
    fn previous_month_levels_roll_forward() {
        let refs = [
            IndicatorRef {
                name: IndicatorName::PrevMonthOpen,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevMonthHigh,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevMonthLow,
                period: None,
                bar_offset: None,
            },
            IndicatorRef {
                name: IndicatorName::PrevMonthClose,
                period: None,
                bar_offset: None,
            },
        ];
        let mut e = IndicatorEngine::new(refs.iter());
        let jan = Utc.with_ymd_and_hms(2026, 1, 31, 23, 0, 0).unwrap();
        e.push(&Bar::with_timestamp(10.0, 12.0, 9.0, 11.0, 100.0, jan));
        let feb = Utc.with_ymd_and_hms(2026, 2, 1, 0, 0, 0).unwrap();
        e.push(&Bar::with_timestamp(20.0, 21.0, 19.0, 20.5, 100.0, feb));

        assert_eq!(e.value(&refs[0]), Some(10.0));
        assert_eq!(e.value(&refs[1]), Some(12.0));
        assert_eq!(e.value(&refs[2]), Some(9.0));
        assert_eq!(e.value(&refs[3]), Some(11.0));
    }
}
