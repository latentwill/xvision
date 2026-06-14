//! Filter v1 data model.
//!
//! Mirrors `docs/superpowers/specs/2026-05-21-filter-v1.md` §Data model
//! exactly. Field names are the source-of-truth for the JSON/TOML DSL,
//! ts-rs export, and downstream engine wiring (Stage 2+).
//!
//! ## Design notes for ambiguous edges
//!
//! * `IndicatorRef` is structurally `{ name, period, bar_offset }` but the
//!   DSL serialisation form is a **string** like `"ema_20"` or `"close"`.
//!   A custom `Serialize`/`Deserialize` impl round-trips through the
//!   string form so authors write `lhs = "ema_20"` rather than the more
//!   verbose struct shape. See `parse_indicator_dsl` and the matching
//!   `fmt` `Display` impl.
//!
//! * `Operand` serialises through `#[serde(untagged)]` (the three shapes —
//!   string, number, two-element array — are disjoint, so authors write
//!   `rhs = "ema_50"` / `rhs = 0.6` / `rhs = [50.0, 70.0]` directly).
//!   Deserialisation is **hand-rolled** via `OperandVisitor` rather than
//!   the derive: an untagged-derive failure surfaces as the unhelpful
//!   message "data did not match any variant of untagged enum Operand",
//!   which hides whether the input was a bad indicator DSL token, a
//!   non-numeric range element, or an entirely wrong shape. The visitor
//!   inspects the input type and produces a pointed error per shape
//!   (e.g. "invalid indicator DSL token 'foo_99'") so `parse.rs` can
//!   classify it into `ParseError::IndicatorDsl` deterministically and
//!   the eventual frontend (Stage 4) can render targeted help.
//!
//! * `bar_offset` on `IndicatorRef`: v1 has no DSL syntax for indexing a
//!   future bar (no `ema_20+1` token). The field is `#[serde(default,
//!   skip_serializing_if = "Option::is_none")]` so it never appears in
//!   real DSL output. It exists so the validator can reject an
//!   in-memory `IndicatorRef` constructed with `bar_offset: Some(n)` for
//!   `n > 0` (`E_FILTER_FUTURE_LEAK`). v1.5 plugins that expose future-
//!   indexing inherit the rule for free.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::errors::ParseError;

/// v1 default agent context template id. Exposed as a `pub const` so
/// tests can reference it without typo risk.
pub const DEFAULT_AGENT_CONTEXT_TEMPLATE: &str = "compact_trade_context_v1";

// ---------------------------------------------------------------------------
// ID newtypes
// ---------------------------------------------------------------------------

macro_rules! string_newtype {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
        #[cfg_attr(
            feature = "ts-export",
            ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
        )]
        #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub String);

        impl $name {
            pub fn new(value: impl Into<String>) -> Self {
                Self(value.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $name {
            fn from(value: String) -> Self {
                Self(value)
            }
        }

        impl From<&str> for $name {
            fn from(value: &str) -> Self {
                Self(value.to_string())
            }
        }
    };
}

string_newtype!(FilterId, "Filter primary key (ULID).");
string_newtype!(StrategyId, "Owning strategy id (ULID, FK).");
string_newtype!(Symbol, "Trading symbol (e.g. \"BTC/USD\").");
string_newtype!(
    Timeframe,
    "Bar timeframe as a string (e.g. \"1h\"). v1 keeps this stringly-typed; \
     formal Timeframe parsing lives in other crates."
);
string_newtype!(
    AgentContextTemplateId,
    "Reference to an LLM-context template. v1 ships \"compact_trade_context_v1\"."
);

// ---------------------------------------------------------------------------
// Simple enums
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FilterStatus {
    Draft,
    Active,
    Paused,
    Archived,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivationMode {
    EveryBar,
    FilterGated,
    /// Reserved for v1.5; `Strategy::validate()` rejects this with
    /// `E_FILTER_ACTIVATION_MODE_NOT_IMPL` (out of v1 scope here, but the
    /// wire variant is reserved so future builds parse cleanly).
    CompiledRules,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanCadence {
    /// v1 ships this single cadence; other cadences land in v1.5+.
    BarClose,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
/// Whether the trader agent is re-invoked while a position is open in the
/// filter's asset. Controls per-bar polling cost during a hold; it does NOT
/// change entry firing (a flat asset always fires on a fresh trip).
///
/// Serializes to snake_case tokens: `always`, `on_invalidation_or_target_only`,
/// `never`. The runtime default is [`WakeInPosition::OnInvalidationOrTargetOnly`]
/// (see `default_wake_in_position`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeInPosition {
    /// Wake the trader on every bar the condition tree is true while holding —
    /// the first true bar AND every sustained-true bar after it. Expensive: a
    /// level operator that stays true drives a trader-LLM call on every
    /// in-position bar. Opt-in only; almost never correct outside
    /// stop-management strategies.
    Always,
    /// Default. Wake only on a fresh trip — the bar the condition tree first
    /// becomes true again — so a new invalidation/target signal still lets the
    /// trader close, while the sustained-true bars in between are suppressed.
    /// The position is NOT re-evaluated every bar, so this is the cost-safe
    /// default. Pair with a distinct exit signal or
    /// `risk.stop_loss_atr_multiple`; otherwise an entry condition that stays
    /// true never re-wakes the trader to close.
    OnInvalidationOrTargetOnly,
    /// Never wake the trader while holding. Exits rely entirely on the
    /// deterministic risk gate (e.g. `risk.stop_loss_atr_multiple`). Produces
    /// the fewest decisions; use for hold-to-target mean-reversion strategies.
    Never,
}

// ---------------------------------------------------------------------------
// Indicator catalog + reference
// ---------------------------------------------------------------------------

/// The closed indicator catalog accepted by the filter DSL.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IndicatorName {
    Open,
    High,
    Low,
    Close,
    Volume,
    Ema,
    Sma,
    Wma,
    Rsi,
    Atr,
    AtrPct,
    Roc,
    Adx,
    DiPlus,
    DiMinus,
    MacdLine,
    MacdSignal,
    MacdHist,
    BbUpper,
    BbMiddle,
    BbLower,
    BbWidth,
    BbPercentB,
    DonchianUpper,
    DonchianMiddle,
    DonchianLower,
    StochK,
    StochD,
    Cci,
    Mfi,
    Obv,
    Vwap,
    VolumeSma,
    StochRsiK,
    StochRsiD,
    Rvol,
    RvolTod,
    VolumeZscore,
    Tenkan,
    Kijun,
    SenkouA,
    SenkouB,
    Chikou,
    CloudTop,
    CloudBottom,
    CloudThickness,
    PrevDayOpen,
    PrevDayHigh,
    PrevDayLow,
    PrevDayClose,
    PrevWeekHigh,
    PrevWeekLow,
    PrevWeekClose,
    PrevMonthOpen,
    PrevMonthHigh,
    PrevMonthLow,
    PrevMonthClose,
    PremarketHigh,
    PremarketLow,
    Highest,
    Lowest,
    OpeningRangeHigh,
    OpeningRangeLow,
    OpeningRangeMid,
    GapPct,
    GapUp,
    GapDown,
    KeltnerUpper,
    KeltnerMiddle,
    KeltnerLower,
    WilliamsR,
    // Perpetual-futures scalars (periodless; latest reading only). Backed by
    // the optional perps fields on `Bar`; `None` on spot bars.
    FundingRate,
    OpenInterest,
    MarkPrice,
    MarkIndexBasis,
    LongShortRatio,
    // WU5: Pine Script catalog parity indicators (native, no external crate)
    /// Hull Moving Average — period-parameterized. DSL: `hma_<period>`.
    Hma,
    /// Volume-Weighted Moving Average — period-parameterized. DSL: `vwma_<period>`.
    Vwma,
    /// SuperTrend (ATR-based trailing stop). The single `period` field packs
    /// both the ATR period and the band multiplier×10 as `atr_period * 1000 +
    /// mult_times_10`. DSL token: `supertrend_<atr_period>_<mult×10>`, e.g.
    /// `supertrend_10_30` (ATR period=10, multiplier=3.0). Emits the active
    /// SuperTrend band level; compare with `close` to derive trend direction.
    SuperTrend,
    /// Pivot high — highest high over a lookback window. The `period` field
    /// packs `left * 1000 + right` (left and right bar counts around the pivot).
    /// DSL token: `pivot_high_<left>_<right>`.
    PivotHigh,
    /// Pivot low — lowest low over the same packed lookback.
    /// DSL token: `pivot_low_<left>_<right>`.
    PivotLow,
}

impl IndicatorName {
    /// True when this indicator carries a single trailing-window `period`.
    /// Price/volume primitives, OBV, and default MACD components are
    /// periodless.
    pub fn has_period(&self) -> bool {
        !matches!(
            self,
            IndicatorName::Open
                | IndicatorName::High
                | IndicatorName::Low
                | IndicatorName::Close
                | IndicatorName::Volume
                | IndicatorName::Obv
                | IndicatorName::MacdLine
                | IndicatorName::MacdSignal
                | IndicatorName::MacdHist
                | IndicatorName::Tenkan
                | IndicatorName::Kijun
                | IndicatorName::SenkouA
                | IndicatorName::SenkouB
                | IndicatorName::Chikou
                | IndicatorName::CloudTop
                | IndicatorName::CloudBottom
                | IndicatorName::CloudThickness
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
                | IndicatorName::LongShortRatio // WU5 new indicators all carry a period (or packed period+param)
                                                // and are intentionally NOT listed here, so has_period() → true.
        )
    }

    /// DSL prefix written in the indicator token (`ema_20` → `"ema"`).
    pub fn dsl_prefix(&self) -> &'static str {
        match self {
            IndicatorName::Open => "open",
            IndicatorName::High => "high",
            IndicatorName::Low => "low",
            IndicatorName::Close => "close",
            IndicatorName::Volume => "volume",
            IndicatorName::Ema => "ema",
            IndicatorName::Sma => "sma",
            IndicatorName::Wma => "wma",
            IndicatorName::Rsi => "rsi",
            IndicatorName::Atr => "atr",
            IndicatorName::AtrPct => "atr_pct",
            IndicatorName::Roc => "roc",
            IndicatorName::Adx => "adx",
            IndicatorName::DiPlus => "di_plus",
            IndicatorName::DiMinus => "di_minus",
            IndicatorName::MacdLine => "macd_line",
            IndicatorName::MacdSignal => "macd_signal",
            IndicatorName::MacdHist => "macd_hist",
            IndicatorName::BbUpper => "bb_upper",
            IndicatorName::BbMiddle => "bb_middle",
            IndicatorName::BbLower => "bb_lower",
            IndicatorName::BbWidth => "bb_width",
            IndicatorName::BbPercentB => "bb_pct_b",
            IndicatorName::DonchianUpper => "donchian_upper",
            IndicatorName::DonchianMiddle => "donchian_middle",
            IndicatorName::DonchianLower => "donchian_lower",
            IndicatorName::StochK => "stoch_k",
            IndicatorName::StochD => "stoch_d",
            IndicatorName::Cci => "cci",
            IndicatorName::Mfi => "mfi",
            IndicatorName::Obv => "obv",
            IndicatorName::Vwap => "vwap",
            IndicatorName::VolumeSma => "volume_sma",
            IndicatorName::StochRsiK => "stoch_rsi_k",
            IndicatorName::StochRsiD => "stoch_rsi_d",
            IndicatorName::Rvol => "rvol",
            IndicatorName::RvolTod => "rvol_tod",
            IndicatorName::VolumeZscore => "volume_zscore",
            IndicatorName::Tenkan => "tenkan",
            IndicatorName::Kijun => "kijun",
            IndicatorName::SenkouA => "senkou_a",
            IndicatorName::SenkouB => "senkou_b",
            IndicatorName::Chikou => "chikou",
            IndicatorName::CloudTop => "cloud_top",
            IndicatorName::CloudBottom => "cloud_bottom",
            IndicatorName::CloudThickness => "cloud_thickness",
            IndicatorName::PrevDayOpen => "prev_day_open",
            IndicatorName::PrevDayHigh => "prev_day_high",
            IndicatorName::PrevDayLow => "prev_day_low",
            IndicatorName::PrevDayClose => "prev_day_close",
            IndicatorName::PrevWeekHigh => "prev_week_high",
            IndicatorName::PrevWeekLow => "prev_week_low",
            IndicatorName::PrevWeekClose => "prev_week_close",
            IndicatorName::PrevMonthOpen => "prev_month_open",
            IndicatorName::PrevMonthHigh => "prev_month_high",
            IndicatorName::PrevMonthLow => "prev_month_low",
            IndicatorName::PrevMonthClose => "prev_month_close",
            IndicatorName::PremarketHigh => "premarket_high",
            IndicatorName::PremarketLow => "premarket_low",
            IndicatorName::Highest => "highest",
            IndicatorName::Lowest => "lowest",
            IndicatorName::OpeningRangeHigh => "opening_range_high",
            IndicatorName::OpeningRangeLow => "opening_range_low",
            IndicatorName::OpeningRangeMid => "opening_range_mid",
            IndicatorName::GapPct => "gap_pct",
            IndicatorName::GapUp => "gap_up",
            IndicatorName::GapDown => "gap_down",
            IndicatorName::KeltnerUpper => "keltner_upper",
            IndicatorName::KeltnerMiddle => "keltner_middle",
            IndicatorName::KeltnerLower => "keltner_lower",
            IndicatorName::WilliamsR => "williams_r",
            IndicatorName::FundingRate => "funding_rate",
            IndicatorName::OpenInterest => "open_interest",
            IndicatorName::MarkPrice => "mark_price",
            IndicatorName::MarkIndexBasis => "mark_index_basis",
            IndicatorName::LongShortRatio => "long_short_ratio",
            // WU5 Pine catalog parity
            IndicatorName::Hma => "hma",
            IndicatorName::Vwma => "vwma",
            // SuperTrend/PivotHigh/PivotLow use multi-part DSL tokens with
            // their own custom parse_dsl / to_dsl logic; dsl_prefix() returns
            // the bare prefix used as the token stem.
            IndicatorName::SuperTrend => "supertrend",
            IndicatorName::PivotHigh => "pivot_high",
            IndicatorName::PivotLow => "pivot_low",
        }
    }

    /// Inclusive bounds for `period` per the spec's indicator catalog.
    /// `None` for periodless indicators.
    pub fn period_bounds(&self) -> Option<(u32, u32)> {
        match self {
            IndicatorName::Open
            | IndicatorName::High
            | IndicatorName::Low
            | IndicatorName::Close
            | IndicatorName::Volume
            | IndicatorName::Obv
            | IndicatorName::MacdLine
            | IndicatorName::MacdSignal
            | IndicatorName::MacdHist
            | IndicatorName::Tenkan
            | IndicatorName::Kijun
            | IndicatorName::SenkouA
            | IndicatorName::SenkouB
            | IndicatorName::Chikou
            | IndicatorName::CloudTop
            | IndicatorName::CloudBottom
            | IndicatorName::CloudThickness
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
            | IndicatorName::LongShortRatio => None,
            IndicatorName::Ema | IndicatorName::Sma | IndicatorName::Wma => Some((2, 500)),
            IndicatorName::Rsi
            | IndicatorName::Atr
            | IndicatorName::AtrPct
            | IndicatorName::Roc
            | IndicatorName::Adx
            | IndicatorName::DiPlus
            | IndicatorName::DiMinus
            | IndicatorName::BbUpper
            | IndicatorName::BbMiddle
            | IndicatorName::BbLower
            | IndicatorName::BbWidth
            | IndicatorName::BbPercentB
            | IndicatorName::DonchianUpper
            | IndicatorName::DonchianMiddle
            | IndicatorName::DonchianLower
            | IndicatorName::StochK
            | IndicatorName::StochD
            | IndicatorName::Cci
            | IndicatorName::Mfi
            | IndicatorName::Vwap
            | IndicatorName::VolumeSma
            | IndicatorName::StochRsiK
            | IndicatorName::StochRsiD
            | IndicatorName::Rvol
            | IndicatorName::RvolTod
            | IndicatorName::VolumeZscore
            | IndicatorName::Highest
            | IndicatorName::Lowest
            | IndicatorName::OpeningRangeHigh
            | IndicatorName::OpeningRangeLow
            | IndicatorName::OpeningRangeMid
            | IndicatorName::KeltnerUpper
            | IndicatorName::KeltnerMiddle
            | IndicatorName::KeltnerLower
            | IndicatorName::WilliamsR => Some((2, 200)),
            // WU5: HMA and VWMA use plain period (same range as EMA/SMA).
            IndicatorName::Hma | IndicatorName::Vwma => Some((2, 500)),
            // SuperTrend packs atr_period * 1000 + mult×10.
            // Effective range: atr_period ∈ [2, 200], mult×10 ∈ [1, 200].
            // Packed min = 2001, max = 200200.
            IndicatorName::SuperTrend => Some((2001, 200_200)),
            // PivotHigh/PivotLow pack left * 1000 + right.
            // Effective range: left ∈ [1, 100], right ∈ [1, 100].
            // Packed min = 1001, max = 100100.
            IndicatorName::PivotHigh | IndicatorName::PivotLow => Some((1001, 100_100)),
        }
    }
}

/// A reference to an indicator in a `Condition`. The DSL wire form is a
/// single string (`"ema_20"`, `"close"`) — see the module-level docs.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IndicatorRef {
    pub name: IndicatorName,
    /// `None` only when `name` is periodless. Validator enforces this.
    pub period: Option<u32>,
    /// Bar-relative offset for v1.5 plugins (positive = future). v1 has
    /// no DSL syntax for this; the validator rejects any `Some(n)` with
    /// `n > 0` (`E_FILTER_FUTURE_LEAK`). Skipped on serialize when `None`
    /// so DSL output stays clean.
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub bar_offset: Option<i32>,
}

impl IndicatorRef {
    /// Build a `close` reference.
    pub fn close() -> Self {
        Self {
            name: IndicatorName::Close,
            period: None,
            bar_offset: None,
        }
    }

    /// Build a periodic indicator reference.
    pub fn periodic(name: IndicatorName, period: u32) -> Self {
        Self {
            name,
            period: Some(period),
            bar_offset: None,
        }
    }

    /// Parse the DSL token form (`"ema_20"`, `"rsi_14"`, `"macd_line"`,
    /// `"bb_upper_20"`, `"close"`) into an `IndicatorRef`. Future-bar syntax (`+N`) is
    /// rejected here so the parser layer prevents `E_FILTER_FUTURE_LEAK`
    /// from reaching validation through DSL input.
    pub fn parse_dsl(token: &str) -> Result<Self, ParseError> {
        let path = format!("indicator '{}'", token);
        if token.contains('+') {
            return Err(ParseError::IndicatorDsl {
                path: path.clone(),
                token: token.to_string(),
            });
        }
        if let Some(name) = match token {
            "open" => Some(IndicatorName::Open),
            "high" => Some(IndicatorName::High),
            "low" => Some(IndicatorName::Low),
            "close" => Some(IndicatorName::Close),
            "volume" => Some(IndicatorName::Volume),
            "obv" => Some(IndicatorName::Obv),
            "tenkan" => Some(IndicatorName::Tenkan),
            "kijun" => Some(IndicatorName::Kijun),
            "senkou_a" => Some(IndicatorName::SenkouA),
            "senkou_b" => Some(IndicatorName::SenkouB),
            "chikou" => Some(IndicatorName::Chikou),
            "cloud_top" => Some(IndicatorName::CloudTop),
            "cloud_bottom" => Some(IndicatorName::CloudBottom),
            "cloud_thickness" => Some(IndicatorName::CloudThickness),
            "prev_day_open" => Some(IndicatorName::PrevDayOpen),
            "prev_day_high" => Some(IndicatorName::PrevDayHigh),
            "prev_day_low" => Some(IndicatorName::PrevDayLow),
            "prev_day_close" => Some(IndicatorName::PrevDayClose),
            "prev_week_high" => Some(IndicatorName::PrevWeekHigh),
            "prev_week_low" => Some(IndicatorName::PrevWeekLow),
            "prev_week_close" => Some(IndicatorName::PrevWeekClose),
            "prev_month_open" => Some(IndicatorName::PrevMonthOpen),
            "prev_month_high" => Some(IndicatorName::PrevMonthHigh),
            "prev_month_low" => Some(IndicatorName::PrevMonthLow),
            "prev_month_close" => Some(IndicatorName::PrevMonthClose),
            "premarket_high" => Some(IndicatorName::PremarketHigh),
            "premarket_low" => Some(IndicatorName::PremarketLow),
            "gap_pct" => Some(IndicatorName::GapPct),
            "gap_up" => Some(IndicatorName::GapUp),
            "gap_down" => Some(IndicatorName::GapDown),
            "funding_rate" => Some(IndicatorName::FundingRate),
            "open_interest" => Some(IndicatorName::OpenInterest),
            "mark_price" => Some(IndicatorName::MarkPrice),
            "mark_index_basis" => Some(IndicatorName::MarkIndexBasis),
            "long_short_ratio" => Some(IndicatorName::LongShortRatio),
            "macd" | "macd_line" | "macd_12_26_9" | "macd_line_12_26_9" => Some(IndicatorName::MacdLine),
            "macd_signal" | "macd_signal_12_26_9" => Some(IndicatorName::MacdSignal),
            "macd_hist" | "macd_hist_12_26_9" | "macd_histogram" | "macd_histogram_12_26_9" => {
                Some(IndicatorName::MacdHist)
            }
            _ => None,
        } {
            return Ok(Self {
                name,
                period: None,
                bar_offset: None,
            });
        }
        // WU5: three-part token parsers — `supertrend_<period>_<mult×10>`,
        // `pivot_high_<left>_<right>`, `pivot_low_<left>_<right>`.
        // Placed before the generic two-part prefix loop so longer matches win.
        if let Some(rest) = token.strip_prefix("supertrend_") {
            // rest = "<atr_period>_<mult_times_10>"
            let mut parts = rest.splitn(2, '_');
            let atr_period: u32 =
                parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| ParseError::IndicatorDsl {
                        path: path.clone(),
                        token: token.to_string(),
                    })?;
            let mult10: u32 =
                parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| ParseError::IndicatorDsl {
                        path: path.clone(),
                        token: token.to_string(),
                    })?;
            let packed = atr_period * 1000 + mult10;
            return Ok(Self::periodic(IndicatorName::SuperTrend, packed));
        }
        if let Some(rest) = token.strip_prefix("pivot_high_") {
            // rest = "<left>_<right>"
            let mut parts = rest.splitn(2, '_');
            let left: u32 =
                parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| ParseError::IndicatorDsl {
                        path: path.clone(),
                        token: token.to_string(),
                    })?;
            let right: u32 =
                parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| ParseError::IndicatorDsl {
                        path: path.clone(),
                        token: token.to_string(),
                    })?;
            let packed = left * 1000 + right;
            return Ok(Self::periodic(IndicatorName::PivotHigh, packed));
        }
        if let Some(rest) = token.strip_prefix("pivot_low_") {
            let mut parts = rest.splitn(2, '_');
            let left: u32 =
                parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| ParseError::IndicatorDsl {
                        path: path.clone(),
                        token: token.to_string(),
                    })?;
            let right: u32 =
                parts
                    .next()
                    .and_then(|s| s.parse().ok())
                    .ok_or_else(|| ParseError::IndicatorDsl {
                        path: path.clone(),
                        token: token.to_string(),
                    })?;
            let packed = left * 1000 + right;
            return Ok(Self::periodic(IndicatorName::PivotLow, packed));
        }
        // Try multi-part prefixes first (longest first) so `atr_pct_14`
        // is matched before `atr_pct` would be misread as `atr` + `pct_14`.
        let candidates: &[(&str, IndicatorName)] = &[
            ("donchian_middle", IndicatorName::DonchianMiddle),
            ("donchian_upper", IndicatorName::DonchianUpper),
            ("donchian_lower", IndicatorName::DonchianLower),
            ("keltner_middle", IndicatorName::KeltnerMiddle),
            ("keltner_upper", IndicatorName::KeltnerUpper),
            ("keltner_lower", IndicatorName::KeltnerLower),
            ("stoch_rsi_k", IndicatorName::StochRsiK),
            ("stoch_rsi_d", IndicatorName::StochRsiD),
            ("stoch_rsi", IndicatorName::StochRsiK),
            ("volume_sma", IndicatorName::VolumeSma),
            ("bb_pct_b", IndicatorName::BbPercentB),
            ("bb_percent_b", IndicatorName::BbPercentB),
            ("bb_middle", IndicatorName::BbMiddle),
            ("bb_upper", IndicatorName::BbUpper),
            ("bb_lower", IndicatorName::BbLower),
            ("bb_width", IndicatorName::BbWidth),
            ("atr_pct", IndicatorName::AtrPct),
            ("stoch_k", IndicatorName::StochK),
            ("stoch_d", IndicatorName::StochD),
            ("williams_r", IndicatorName::WilliamsR),
            ("opening_range_high", IndicatorName::OpeningRangeHigh),
            ("opening_range_low", IndicatorName::OpeningRangeLow),
            ("opening_range_mid", IndicatorName::OpeningRangeMid),
            ("volume_zscore", IndicatorName::VolumeZscore),
            ("rvol_tod", IndicatorName::RvolTod),
            ("di_plus", IndicatorName::DiPlus),
            ("di_minus", IndicatorName::DiMinus),
            ("highest", IndicatorName::Highest),
            ("lowest", IndicatorName::Lowest),
            ("rvol", IndicatorName::Rvol),
            ("vwap", IndicatorName::Vwap),
            ("wma", IndicatorName::Wma),
            ("ema", IndicatorName::Ema),
            ("sma", IndicatorName::Sma),
            ("rsi", IndicatorName::Rsi),
            ("atr", IndicatorName::Atr),
            ("adx", IndicatorName::Adx),
            ("roc", IndicatorName::Roc),
            ("cci", IndicatorName::Cci),
            ("mfi", IndicatorName::Mfi),
            // WU5 plain-period indicators
            ("vwma", IndicatorName::Vwma),
            ("hma", IndicatorName::Hma),
        ];
        for (prefix, name) in candidates {
            let needle = format!("{}_", prefix);
            if let Some(rest) = token.strip_prefix(&needle) {
                let period: u32 = rest.parse().map_err(|_| ParseError::IndicatorDsl {
                    path: path.clone(),
                    token: token.to_string(),
                })?;
                return Ok(Self::periodic(*name, period));
            }
        }
        Err(ParseError::IndicatorDsl {
            path,
            token: token.to_string(),
        })
    }

    /// Render to the DSL token form. Mirror of `parse_dsl`. Does not
    /// emit `bar_offset` because no DSL syntax exists for it in v1.
    pub fn to_dsl(&self) -> String {
        match (self.name, self.period) {
            (IndicatorName::Close, _) => "close".to_string(),
            // WU5: three-part packed tokens — unpack and re-emit.
            (IndicatorName::SuperTrend, Some(packed)) => {
                let atr_period = packed / 1000;
                let mult10 = packed % 1000;
                format!("supertrend_{}_{}", atr_period, mult10)
            }
            (IndicatorName::PivotHigh, Some(packed)) => {
                let left = packed / 1000;
                let right = packed % 1000;
                format!("pivot_high_{}_{}", left, right)
            }
            (IndicatorName::PivotLow, Some(packed)) => {
                let left = packed / 1000;
                let right = packed % 1000;
                format!("pivot_low_{}_{}", left, right)
            }
            (name, Some(p)) => format!("{}_{}", name.dsl_prefix(), p),
            (name, None) => name.dsl_prefix().to_string(),
        }
    }
}

impl fmt::Display for IndicatorRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_dsl())
    }
}

impl Serialize for IndicatorRef {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_dsl())
    }
}

impl<'de> Deserialize<'de> for IndicatorRef {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let token = String::deserialize(d)?;
        Self::parse_dsl(&token).map_err(serde::de::Error::custom)
    }
}

// ---------------------------------------------------------------------------
// Operator
// ---------------------------------------------------------------------------

/// Operator catalog. Static operators use their literal DSL tokens;
/// parameterized operators encode the parameter in the token
/// (`above_for_3`, `crossed_above_5`, `slope_gt_4`,
/// `within_pct_1.5`, `zscore_gt_20`).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Operator {
    Gt,
    Lt,
    Gte,
    Lte,
    Eq,
    CrossesAbove,
    CrossesBelow,
    Between,
    AboveFor(u32),
    BelowFor(u32),
    CrossedAbove(u32),
    CrossedBelow(u32),
    SlopeGt(u32),
    SlopeLt(u32),
    ZscoreGt(u32),
    ZscoreLt(u32),
    WithinPct(f64),
}

impl Operator {
    pub fn dsl_token(&self) -> String {
        match self {
            Operator::Gt => ">".to_string(),
            Operator::Lt => "<".to_string(),
            Operator::Gte => ">=".to_string(),
            Operator::Lte => "<=".to_string(),
            Operator::Eq => "==".to_string(),
            Operator::CrossesAbove => "crosses_above".to_string(),
            Operator::CrossesBelow => "crosses_below".to_string(),
            Operator::Between => "between".to_string(),
            Operator::AboveFor(n) => format!("above_for_{}", n),
            Operator::BelowFor(n) => format!("below_for_{}", n),
            Operator::CrossedAbove(n) => format!("crossed_above_{}", n),
            Operator::CrossedBelow(n) => format!("crossed_below_{}", n),
            Operator::SlopeGt(n) => format!("slope_gt_{}", n),
            Operator::SlopeLt(n) => format!("slope_lt_{}", n),
            Operator::ZscoreGt(n) => format!("zscore_gt_{}", n),
            Operator::ZscoreLt(n) => format!("zscore_lt_{}", n),
            Operator::WithinPct(pct) => format!("within_pct_{}", fmt_decimal_token(*pct)),
        }
    }

    pub fn parse_dsl(token: &str) -> Result<Self, ParseError> {
        let normalized = token.trim().to_ascii_lowercase();
        let op = match normalized.as_str() {
            ">" | "gt" | "above" => Some(Operator::Gt),
            "<" | "lt" | "below" => Some(Operator::Lt),
            ">=" | "gte" | "above_or_equal" | "at_or_above" => Some(Operator::Gte),
            "<=" | "lte" | "below_or_equal" | "at_or_below" => Some(Operator::Lte),
            "==" | "eq" | "equals" => Some(Operator::Eq),
            "crosses_above" | "crosses_over" => Some(Operator::CrossesAbove),
            "crosses_below" | "crosses_under" => Some(Operator::CrossesBelow),
            "between" => Some(Operator::Between),
            _ => None,
        };
        if let Some(op) = op {
            return Ok(op);
        }
        if let Some(n) = parse_u32_suffix(&normalized, "above_for_") {
            return Ok(Operator::AboveFor(n));
        }
        if let Some(n) = parse_u32_suffix(&normalized, "below_for_") {
            return Ok(Operator::BelowFor(n));
        }
        if let Some(n) = parse_u32_suffix(&normalized, "crossed_above_") {
            return Ok(Operator::CrossedAbove(n));
        }
        if let Some(n) = parse_u32_suffix(&normalized, "crossed_below_") {
            return Ok(Operator::CrossedBelow(n));
        }
        if let Some(n) = parse_u32_suffix(&normalized, "slope_gt_") {
            return Ok(Operator::SlopeGt(n));
        }
        if let Some(n) = parse_u32_suffix(&normalized, "slope_lt_") {
            return Ok(Operator::SlopeLt(n));
        }
        if let Some(n) = parse_u32_suffix(&normalized, "zscore_gt_") {
            return Ok(Operator::ZscoreGt(n));
        }
        if let Some(n) = parse_u32_suffix(&normalized, "zscore_lt_") {
            return Ok(Operator::ZscoreLt(n));
        }
        if let Some(pct) = parse_f64_suffix(&normalized, "within_pct_") {
            return Ok(Operator::WithinPct(pct));
        }
        Err(ParseError::UnknownOperator {
            path: "/conditions/all/0/op".to_string(),
            token: token.to_string(),
        })
    }
}

impl fmt::Display for Operator {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.dsl_token())
    }
}

impl Serialize for Operator {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.dsl_token())
    }
}

impl<'de> Deserialize<'de> for Operator {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let token = String::deserialize(d)?;
        Self::parse_dsl(&token).map_err(serde::de::Error::custom)
    }
}

fn parse_u32_suffix(token: &str, prefix: &str) -> Option<u32> {
    let raw = token.strip_prefix(prefix)?;
    let n: u32 = raw.parse().ok()?;
    (n > 0).then_some(n)
}

fn parse_f64_suffix(token: &str, prefix: &str) -> Option<f64> {
    let raw = token.strip_prefix(prefix)?;
    let pct: f64 = raw.parse().ok()?;
    (pct.is_finite() && pct >= 0.0).then_some(pct)
}

fn fmt_decimal_token(value: f64) -> String {
    let mut s = value.to_string();
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    s
}

// ---------------------------------------------------------------------------
// Operand
// ---------------------------------------------------------------------------

/// One side of a `Condition`. Discriminates by JSON/TOML type — see
/// module docs. `Serialize` is derived as `#[serde(untagged)]`;
/// `Deserialize` is hand-rolled via `OperandVisitor` so failures surface
/// per-shape (string vs number vs sequence) instead of the opaque
/// untagged-derive message.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum Operand {
    /// String DSL form: `"ema_20"`, `"close"`, etc.
    Indicator(IndicatorRef),
    /// Numeric literal: `0.6`, `50.0`, …
    Numeric(f64),
    /// Two-element ascending range: `[50.0, 70.0]`. Only valid with
    /// `Operator::Between`.
    Range(f64, f64),
}

impl Operand {
    /// Discriminator name used in error messages.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Operand::Indicator(_) => "indicator",
            Operand::Numeric(_) => "numeric",
            Operand::Range(_, _) => "range",
        }
    }
}

impl<'de> Deserialize<'de> for Operand {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(OperandVisitor)
    }
}

struct OperandVisitor;

impl<'de> serde::de::Visitor<'de> for OperandVisitor {
    type Value = Operand;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(
            "an indicator DSL string (e.g. \"ema_20\", \"close\"), \
             a numeric literal, \
             or a two-element numeric range array (e.g. [50.0, 70.0])",
        )
    }

    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(Operand::Numeric(v as f64))
    }

    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(Operand::Numeric(v as f64))
    }

    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(Operand::Numeric(v))
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        match IndicatorRef::parse_dsl(v) {
            Ok(ind) => Ok(Operand::Indicator(ind)),
            Err(ParseError::IndicatorDsl { token, .. }) => {
                Err(E::custom(format!("invalid indicator DSL token '{}'", token)))
            }
            Err(other) => Err(E::custom(other.to_string())),
        }
    }

    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
        self.visit_str(&v)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        let lo: f64 = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::custom("range operand: expected 2 numeric elements, got 0"))?;
        let hi: f64 = seq
            .next_element()?
            .ok_or_else(|| serde::de::Error::custom("range operand: expected 2 numeric elements, got 1"))?;
        if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
            return Err(serde::de::Error::custom(
                "range operand: expected 2 numeric elements, got more",
            ));
        }
        Ok(Operand::Range(lo, hi))
    }
}

// ---------------------------------------------------------------------------
// Condition + ConditionTree
// ---------------------------------------------------------------------------

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Condition {
    pub lhs: Operand,
    pub op: Operator,
    pub rhs: Operand,
}

/// Inner condition group — non-recursive. Holds only flat `Condition`s.
/// Depth > 1 is structurally impossible: `ConditionGroup` contains
/// `Vec<Condition>`, not `Vec<ConditionItem>`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionGroup {
    All(Vec<Condition>),
    Any(Vec<Condition>),
}

impl ConditionGroup {
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::All(_) => "all",
            Self::Any(_) => "any",
        }
    }

    pub fn conditions(&self) -> &[Condition] {
        match self {
            Self::All(v) | Self::Any(v) => v,
        }
    }

    pub fn conditions_mut(&mut self) -> &mut Vec<Condition> {
        match self {
            Self::All(v) | Self::Any(v) => v,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.conditions().is_empty()
    }

    pub fn combine(&self, results: &[bool]) -> bool {
        match self {
            Self::All(_) => results.iter().all(|&r| r),
            Self::Any(_) => results.iter().any(|&r| r),
        }
    }
}

/// A top-level item in a `ConditionTree`: either a flat `Leaf` condition
/// or a nested `ConditionGroup` (max 1 level deep by type construction).
///
/// Serializes as untagged (Leaf: `{lhs,op,rhs}`, Group: `{all:[...]}` /
/// `{any:[...]}`). Deserialization uses a custom visitor that peeks at the
/// first map key to choose the variant — this avoids swallowing Operand/
/// Operator parse errors the way `#[serde(untagged)]` would.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionItem {
    Leaf(Condition),
    Group(ConditionGroup),
}

impl Serialize for ConditionItem {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            ConditionItem::Leaf(c) => c.serialize(serializer),
            ConditionItem::Group(g) => g.serialize(serializer),
        }
    }
}

impl<'de> Deserialize<'de> for ConditionItem {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(ConditionItemVisitor)
    }
}

struct ConditionItemVisitor;

impl<'de> serde::de::Visitor<'de> for ConditionItemVisitor {
    type Value = ConditionItem;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a condition leaf {lhs, op, rhs} or a group {all: [...]} / {any: [...]}")
    }

    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        // Collect all key-value pairs as raw serde_json::Value so we can
        // inspect the keys and then re-deserialize to the right type.
        use serde::de::Error as _;

        let mut raw: std::collections::BTreeMap<String, serde_json::Value> =
            std::collections::BTreeMap::new();
        while let Some((k, v)) = map.next_entry::<String, serde_json::Value>()? {
            raw.insert(k, v);
        }

        // Discriminate by key set.
        let is_group = raw.contains_key("all") || raw.contains_key("any");

        let json_obj = serde_json::Value::Object(
            raw.into_iter()
                .map(|(k, v)| (k, v))
                .collect::<serde_json::Map<String, serde_json::Value>>(),
        );

        if is_group {
            let group: ConditionGroup =
                serde_json::from_value(json_obj).map_err(|e| A::Error::custom(e.to_string()))?;
            Ok(ConditionItem::Group(group))
        } else {
            // It looks like a Leaf — propagate any Condition parse error
            // verbatim (preserving OperandVisitor messages).
            let cond: Condition =
                serde_json::from_value(json_obj).map_err(|e| A::Error::custom(e.to_string()))?;
            Ok(ConditionItem::Leaf(cond))
        }
    }
}

/// Logical tree above leaf `Condition`s. Serde `rename_all = "snake_case"`
/// produces the `all`/`any` tag expected by the DSL (`[[filter.conditions.all]]`).
///
/// Items may be flat `Leaf` conditions or one level of nested `Group`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionTree {
    All(Vec<ConditionItem>),
    Any(Vec<ConditionItem>),
}

impl ConditionTree {
    pub fn variant_name(&self) -> &'static str {
        match self {
            Self::All(_) => "all",
            Self::Any(_) => "any",
        }
    }

    pub fn items(&self) -> &[ConditionItem] {
        match self {
            Self::All(v) | Self::Any(v) => v,
        }
    }

    pub fn items_mut(&mut self) -> &mut Vec<ConditionItem> {
        match self {
            Self::All(v) | Self::Any(v) => v,
        }
    }

    /// Total number of leaf `Condition`s, counting into nested groups.
    pub fn leaf_count(&self) -> usize {
        self.items()
            .iter()
            .map(|i| match i {
                ConditionItem::Leaf(_) => 1,
                ConditionItem::Group(g) => g.conditions().len(),
            })
            .sum()
    }

    /// All leaf `Condition`s in depth-first order.
    pub fn leaves_dfs(&self) -> Vec<&Condition> {
        self.items()
            .iter()
            .flat_map(|i| match i {
                ConditionItem::Leaf(c) => vec![c],
                ConditionItem::Group(g) => g.conditions().iter().collect(),
            })
            .collect()
    }

    /// All leaf `Condition`s in depth-first order, mutably.
    ///
    /// Collected into a `Vec` — a lazy iterator is not feasible with
    /// heterogeneous mutable refs across enum arms.
    pub fn leaves_dfs_mut(&mut self) -> Vec<&mut Condition> {
        let mut out = Vec::new();
        for item in self.items_mut().iter_mut() {
            match item {
                ConditionItem::Leaf(c) => out.push(c),
                ConditionItem::Group(g) => out.extend(g.conditions_mut().iter_mut()),
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// LLM fire metadata
// ---------------------------------------------------------------------------

/// Optional author-facing metadata attached to a filter trip. This does
/// not change whether a filter fires; it tells downstream agent surfaces
/// why the gate fired and which compact indicator values should be
/// surfaced with the trigger.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterFire {
    pub reason: String,
    pub priority: f64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub context: Vec<IndicatorRef>,
}

// ---------------------------------------------------------------------------
// Filter (top-level)
// ---------------------------------------------------------------------------

/// Top-level Filter entity. JSON parses this struct directly; TOML wraps
/// it under `[filter]` (handled in `parse.rs`).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Filter {
    pub id: FilterId,
    pub strategy_id: StrategyId,
    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default = "default_status")]
    pub status: FilterStatus,
    pub asset_scope: Vec<Symbol>,
    pub timeframe: Timeframe,
    #[serde(default = "default_scan_cadence")]
    pub scan_cadence: ScanCadence,
    pub conditions: ConditionTree,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fire: Option<FilterFire>,
    #[serde(default)]
    pub cooldown_bars: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_wakeups_per_day: Option<u32>,
    #[serde(default = "default_wake_in_position")]
    pub wake_when_in_position: WakeInPosition,
    #[serde(default = "default_agent_context_template")]
    pub agent_context_template: AgentContextTemplateId,
}

fn default_status() -> FilterStatus {
    FilterStatus::Draft
}

fn default_scan_cadence() -> ScanCadence {
    ScanCadence::BarClose
}

fn default_wake_in_position() -> WakeInPosition {
    // Sane default: while a position is open, only re-wake the trader on a
    // FRESH filter trip (a new invalidation / target-style signal), not on
    // every sustained-true bar. `Always` (the previous default) drove a
    // redundant trader-LLM call on EVERY in-position bar — the per-bar
    // polling cost bug. The trader can still close on a fresh trip, and the
    // deterministic SL/TP enforces exits regardless. `Always` stays available
    // as an explicit opt-in.
    WakeInPosition::OnInvalidationOrTargetOnly
}

fn default_agent_context_template() -> AgentContextTemplateId {
    AgentContextTemplateId::new(DEFAULT_AGENT_CONTEXT_TEMPLATE)
}
