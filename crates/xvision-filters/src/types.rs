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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WakeInPosition {
    Always,
    OnInvalidationOrTargetOnly,
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
            | IndicatorName::GapDown => None,
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

/// Logical tree above leaf `Condition`s. v1 is a flat `All` or `Any`; the
/// spec's TOML example uses `[[filter.conditions.all]]`. Serde
/// `rename_all = "snake_case"` produces that tag.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionTree {
    All(Vec<Condition>),
    Any(Vec<Condition>),
}

impl ConditionTree {
    pub fn variant_name(&self) -> &'static str {
        match self {
            ConditionTree::All(_) => "all",
            ConditionTree::Any(_) => "any",
        }
    }

    pub fn conditions(&self) -> &[Condition] {
        match self {
            ConditionTree::All(v) | ConditionTree::Any(v) => v,
        }
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
