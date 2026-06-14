//! Trading-domain types — TraderDecision, RiskDecision.
//!
//! All structs validate via `garde::Validate`. Parsing JSON only checks shape;
//! `decision.validate(&())` runs range/length checks at the boundary.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::asset_registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Buy,
    Sell,
    Flat,
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Long,
    Short,
    Flat,
}

/// Tradeable asset symbol — an interned `Copy` newtype.
///
/// Legacy variant names are preserved as associated constants so all existing
/// `AssetSymbol::Btc` call sites compile unchanged. `Eq`/`Hash`/`Ord` are
/// value-based (string content); serde emits a bare string scalar
/// (`"BTC"`, `"ETH"`, …). Wire format is identical to the old enum's
/// `rename_all = "UPPERCASE"` — no DB migration needed.
///
/// `FromStr` is permissive: validates format only (`[A-Z0-9_]+` after
/// trim → uppercase → base-before-`/`), not whitelist membership.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AssetSymbol(&'static str);

impl AssetSymbol {
    #[allow(non_upper_case_globals)]
    pub const Btc: AssetSymbol = AssetSymbol::from_static("BTC");
    #[allow(non_upper_case_globals)]
    pub const Eth: AssetSymbol = AssetSymbol::from_static("ETH");
    #[allow(non_upper_case_globals)]
    pub const Ltc: AssetSymbol = AssetSymbol::from_static("LTC");
    #[allow(non_upper_case_globals)]
    pub const Sol: AssetSymbol = AssetSymbol::from_static("SOL");
    #[allow(non_upper_case_globals)]
    pub const Avax: AssetSymbol = AssetSymbol::from_static("AVAX");
    #[allow(non_upper_case_globals)]
    pub const Link: AssetSymbol = AssetSymbol::from_static("LINK");
    #[allow(non_upper_case_globals)]
    pub const Aave: AssetSymbol = AssetSymbol::from_static("AAVE");
    #[allow(non_upper_case_globals)]
    pub const Uni: AssetSymbol = AssetSymbol::from_static("UNI");
    #[allow(non_upper_case_globals)]
    pub const Dot: AssetSymbol = AssetSymbol::from_static("DOT");
    #[allow(non_upper_case_globals)]
    pub const Doge: AssetSymbol = AssetSymbol::from_static("DOGE");
    #[allow(non_upper_case_globals)]
    pub const Shib: AssetSymbol = AssetSymbol::from_static("SHIB");
    #[allow(non_upper_case_globals)]
    pub const Matic: AssetSymbol = AssetSymbol::from_static("MATIC");
    #[allow(non_upper_case_globals)]
    pub const Bch: AssetSymbol = AssetSymbol::from_static("BCH");
    #[allow(non_upper_case_globals)]
    pub const Usdt: AssetSymbol = AssetSymbol::from_static("USDT");
    #[allow(non_upper_case_globals)]
    pub const Usdc: AssetSymbol = AssetSymbol::from_static("USDC");

    /// Construct an `AssetSymbol` from a `&'static str` at compile time.
    pub const fn from_static(s: &'static str) -> Self {
        Self(s)
    }

    /// Short upper-case ticker (`"BTC"`, `"ETH"`, …). Stable across the
    /// codebase — JSON, logs, prompt rendering, and report column headers
    /// all assume this form.
    pub fn as_str(self) -> &'static str {
        self.0
    }

    /// Alias for `as_str` (spec name from the asset-unlock plan).
    pub fn as_short(self) -> &'static str {
        self.0
    }

    /// Alpaca-style trading pair (`"BTC/USD"`, `"ETH/USD"`, …).
    pub fn as_alpaca_pair(self) -> String {
        format!("{}/USD", self.0)
    }
}

impl std::fmt::Debug for AssetSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AssetSymbol::{}", self.0)
    }
}

impl std::fmt::Display for AssetSymbol {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl std::str::FromStr for AssetSymbol {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let upper = s.trim().to_ascii_uppercase();
        let base = upper.split('/').next().unwrap_or(&upper);
        if base.is_empty() {
            return Err("asset symbol must not be empty".into());
        }
        if !base.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(format!(
                "asset symbol '{base}' contains invalid characters (expected [A-Z0-9_]+)"
            ));
        }
        asset_registry::intern_symbol(base)
            .map(AssetSymbol)
            .ok_or_else(|| format!("asset registry cap exceeded; cannot intern '{base}'"))
    }
}

impl Serialize for AssetSymbol {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.0)
    }
}

impl<'de> Deserialize<'de> for AssetSymbol {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        use std::str::FromStr as _;
        let raw = String::deserialize(d)?;
        AssetSymbol::from_str(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Regime {
    Bull,
    Bear,
    Chop,
    HighVol,
    LowVol,
}

/// Stage 2 output: a concrete trade decision. Multiple arms (e.g.
/// trader_arm + baselines) each emit one of these against the same cached
/// briefing (Tier 1 fix #1) — arm identity is carried by the storage key
/// `(cycle_id, arm_name)`, not by a field on the decision itself.
///
/// **Deserialization runs the F-6 cross-field invariant automatically.**
/// `#[serde(try_from = "TraderDecisionRaw")]` parses into a private
/// shadow struct (with `#[serde(deny_unknown_fields)]`), then runs
/// `validate_cross_field` on the conversion. Every parse site — DB
/// store, CLI risk command, API boundary — picks up the
/// `take_profit_pct > stop_loss_pct` check without any caller change.
/// Direct struct construction (test fixtures, in-process risk-engine
/// outputs) skips the check — callers that need it can invoke
/// `validate_cross_field` explicitly.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
#[serde(try_from = "TraderDecisionRaw")]
pub struct TraderDecision {
    #[garde(skip)]
    pub cycle_id: Uuid,
    #[garde(skip)]
    pub action: Action,
    /// Position size in basis points of NAV (max 20% = 2000bps).
    #[garde(range(min = 0, max = 2000))]
    pub size_bps: u32,
    #[garde(skip)]
    pub direction: Direction,
    #[garde(range(min = 0.1, max = 20.0))]
    pub stop_loss_pct: f32,
    #[garde(range(min = 0.1, max = 50.0))]
    pub take_profit_pct: f32,
    #[garde(length(min = 10, max = 500))]
    pub trader_summary: String,
    /// F18 cascade complete: the trader names the asset every decision routes
    /// to. Risk reads `asset` directly (no separate param), executors route
    /// per-decision, and `BacktestConfig::instrument` was removed.
    #[garde(skip)]
    pub asset: AssetSymbol,

    // -- Trailing stop (ratchets SL toward current price as position profits) --
    #[garde(skip)]
    #[serde(default)]
    pub trailing_stop_pct: Option<f64>,
    // -- Break-even stop (move SL to entry once profit threshold is hit) --
    #[garde(skip)]
    #[serde(default)]
    pub breakeven_trigger_pct: Option<f64>,
    #[garde(skip)]
    #[serde(default)]
    pub breakeven_offset_pct: Option<f64>,
    // -- Fading SL (SL tightens toward entry over time) --
    #[garde(skip)]
    #[serde(default)]
    pub fade_sl_bars: Option<u32>,
    #[garde(skip)]
    #[serde(default)]
    pub fade_sl_start_pct: Option<f64>,
    #[garde(skip)]
    #[serde(default)]
    pub fade_sl_end_pct: Option<f64>,
    // -- Time-based exit (force-close after N bars) --
    #[garde(skip)]
    #[serde(default)]
    pub max_bars_held: Option<u32>,
    // -- ATR-based SL/TP (distance expressed as ATR multiples) --
    #[garde(skip)]
    #[serde(default)]
    pub sl_atr_mult: Option<f64>,
    #[garde(skip)]
    #[serde(default)]
    pub tp_atr_mult: Option<f64>,
    // -- Partial TP (close fraction at TP1, let remainder run to TP2) --
    #[garde(skip)]
    #[serde(default)]
    pub tp1_pct: Option<f64>,
    #[garde(skip)]
    #[serde(default)]
    pub tp1_close_fraction: Option<f64>,
    #[garde(skip)]
    #[serde(default)]
    pub tp2_pct: Option<f64>,
}

/// Shadow struct backing `TraderDecision`'s `try_from` deserialize
/// (F-6). Carries `#[serde(deny_unknown_fields)]` so typos in the
/// trader response or in stored JSON fail at parse time, and acts as
/// the seam where `validate_cross_field` runs before the typed value
/// is constructed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TraderDecisionRaw {
    cycle_id: Uuid,
    action: Action,
    size_bps: u32,
    direction: Direction,
    stop_loss_pct: f32,
    take_profit_pct: f32,
    trader_summary: String,
    asset: AssetSymbol,
    #[serde(default)]
    trailing_stop_pct: Option<f64>,
    #[serde(default)]
    breakeven_trigger_pct: Option<f64>,
    #[serde(default)]
    breakeven_offset_pct: Option<f64>,
    #[serde(default)]
    fade_sl_bars: Option<u32>,
    #[serde(default)]
    fade_sl_start_pct: Option<f64>,
    #[serde(default)]
    fade_sl_end_pct: Option<f64>,
    #[serde(default)]
    max_bars_held: Option<u32>,
    #[serde(default)]
    sl_atr_mult: Option<f64>,
    #[serde(default)]
    tp_atr_mult: Option<f64>,
    #[serde(default)]
    tp1_pct: Option<f64>,
    #[serde(default)]
    tp1_close_fraction: Option<f64>,
    #[serde(default)]
    tp2_pct: Option<f64>,
}

impl TryFrom<TraderDecisionRaw> for TraderDecision {
    type Error = String;

    fn try_from(raw: TraderDecisionRaw) -> Result<Self, Self::Error> {
        let decision = TraderDecision {
            cycle_id: raw.cycle_id,
            action: raw.action,
            size_bps: raw.size_bps,
            direction: raw.direction,
            stop_loss_pct: raw.stop_loss_pct,
            take_profit_pct: raw.take_profit_pct,
            trader_summary: raw.trader_summary,
            asset: raw.asset,
            trailing_stop_pct: raw.trailing_stop_pct,
            breakeven_trigger_pct: raw.breakeven_trigger_pct,
            breakeven_offset_pct: raw.breakeven_offset_pct,
            fade_sl_bars: raw.fade_sl_bars,
            fade_sl_start_pct: raw.fade_sl_start_pct,
            fade_sl_end_pct: raw.fade_sl_end_pct,
            max_bars_held: raw.max_bars_held,
            sl_atr_mult: raw.sl_atr_mult,
            tp_atr_mult: raw.tp_atr_mult,
            tp1_pct: raw.tp1_pct,
            tp1_close_fraction: raw.tp1_close_fraction,
            tp2_pct: raw.tp2_pct,
        };
        decision.validate_cross_field()?;
        Ok(decision)
    }
}

impl TraderDecision {
    /// Tuple keyed for divergence analysis (Tier 3 cleanup): the headline
    /// divergence rate operates on `(action, direction, size_bucket)` rather
    /// than `action` alone.
    pub fn divergence_key(&self) -> (Action, Direction, SizeBucket) {
        (self.action, self.direction, SizeBucket::from_bps(self.size_bps))
    }

    /// Cross-field invariants not expressible via field-level garde:
    /// take-profit must exceed stop-loss for any directional position
    /// (Buy/Sell). `Flat`/`Close` skip the check — they're position
    /// exits with no forward risk asymmetry to enforce. Symmetric for
    /// long and short since both pcts are stored as positive
    /// magnitudes.
    ///
    /// Callers that need full validation should invoke `validate(&())`
    /// (field-level ranges + lengths via garde) AND
    /// `validate_cross_field()` (this method). The eval boundary
    /// already gates on the former; F-6 adds the latter to the
    /// pre-persist seam in `StrategyStore::save` (for trader-decision
    /// fixtures embedded in a strategy) and to any future risk-gate
    /// audit that wants the cross-field discipline.
    pub fn validate_cross_field(&self) -> Result<(), String> {
        if matches!(self.action, Action::Flat | Action::Close) {
            return Ok(());
        }
        if self.take_profit_pct <= self.stop_loss_pct {
            return Err(format!(
                "take_profit_pct ({:.2}) must exceed stop_loss_pct ({:.2}) for action {:?}",
                self.take_profit_pct, self.stop_loss_pct, self.action,
            ));
        }
        Ok(())
    }
}

/// One open position. Direction is `Long` or `Short` (never `Flat`).
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct OpenPosition {
    #[garde(skip)]
    pub asset: AssetSymbol,
    #[garde(skip)]
    pub direction: Direction,
    /// Notional position size in basis points of NAV at entry.
    #[garde(range(min = 1, max = 2000))]
    pub size_bps: u32,
    #[garde(range(min = 0.0))]
    pub entry_price: f64,
    #[garde(range(min = 0.0))]
    pub mark_price: f64,
    #[garde(range(min = 0.1, max = 20.0))]
    pub stop_loss_pct: f32,
    #[garde(range(min = 0.1, max = 50.0))]
    pub take_profit_pct: f32,
    #[garde(skip)]
    pub opened_at: DateTime<Utc>,
    /// Account leverage on this position, for perps venues. `None` for spot
    /// (and any venue that does not report it). Populated by the perps executor
    /// from the venue; consumed by the `LiquidationDistanceGuard` risk rule.
    #[serde(default)]
    #[garde(skip)]
    pub leverage: Option<f64>,
    /// Venue-reported liquidation price for this perps position. `None` for spot
    /// (no liquidation). When set, `LiquidationDistanceGuard` vetoes new entries
    /// while this position sits within the configured distance of liquidation.
    #[serde(default)]
    #[garde(skip)]
    pub liq_price: Option<f64>,
}

/// Snapshot of the trading account at decision time. The Trader uses this to
/// reason about exposure (e.g. "I'm already long BTC at 1500 bps; consider
/// closing before sizing up another position"). Risk rules read it too.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct PortfolioState {
    #[garde(range(min = 0.0))]
    pub equity_usd: f64,
    /// Realized PnL today (UTC day) — informs daily-loss circuit-breaker logic.
    #[garde(skip)]
    pub realized_pnl_today_usd: f64,
    /// Day index since strategy start; used by risk rules for windowed limits.
    #[garde(skip)]
    pub day_index: u32,
    /// Open positions keyed by asset for stable iteration order.
    #[garde(skip)]
    pub open_positions: BTreeMap<AssetSymbol, OpenPosition>,
    #[garde(skip)]
    pub as_of: DateTime<Utc>,
}

impl PortfolioState {
    /// Sum of open exposure in basis points across all positions.
    pub fn total_exposure_bps(&self) -> u32 {
        self.open_positions.values().map(|p| p.size_bps).sum()
    }

    /// Flat = no open positions.
    pub fn is_flat(&self) -> bool {
        self.open_positions.is_empty()
    }
}

/// Coarse size bucketing for divergence analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SizeBucket {
    Zero,
    Small,  // 1–500 bps
    Medium, // 501–1500 bps
    Large,  // 1501–2000 bps
}

impl SizeBucket {
    pub fn from_bps(bps: u32) -> Self {
        match bps {
            0 => Self::Zero,
            1..=500 => Self::Small,
            501..=1500 => Self::Medium,
            _ => Self::Large,
        }
    }
}

/// Reason a risk rule modified or vetoed a decision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VetoReason {
    PositionTooLarge,
    ExposureCap,
    AssetNotWhitelisted,
    DailyLossCircuitBreaker,
    MaxOpenPositions,
    StopLossMissing,
    StopLossTooWide,
    TakeProfitMissing,
    /// Order notional (size × reference price) is below the venue's
    /// configured deterministic minimum (`config/risk.toml`
    /// `[venues.<id>].min_notional_usd`). Fired by the `MinNotional`
    /// risk rule and by the pre-submit gate in `paper-mode-executor-deleted` so the
    /// broker never sees a known-bad order. Operator-visible as a
    /// clean risk veto instead of an opaque broker rejection.
    BelowVenueMinNotional,
    /// The perp funding rate the position would pay at entry exceeds the
    /// configured punitive threshold (`[perps].max_funding_pay_8h`). Fired by
    /// the `FundingCarryGuard` rule so an entry is never opened into funding
    /// that erodes the edge. Exits (Flat/Close) are never blocked. Favorable
    /// (carry) funding passes through unchanged. Operator-visible as a clean
    /// risk veto.
    PunitiveFunding,
    /// An open perps position sits within the configured distance
    /// (`[perps].min_liq_distance_pct`) of its liquidation price. Fired by the
    /// `LiquidationDistanceGuard` rule to block *new* entries while existing
    /// risk is near liquidation. Exits are never blocked; spot positions (no
    /// liquidation price) never trigger it. Operator-visible as a clean veto.
    NearLiquidation,
    Custom(String),
}

/// Risk-layer output: a `TraderDecision` is approved as-is, modified into a
/// reduced version, or fully vetoed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "verdict", rename_all = "snake_case", deny_unknown_fields)]
pub enum RiskDecision {
    Approved {
        decision: TraderDecision,
        #[serde(default)]
        warnings: Vec<String>,
    },
    Modified {
        original: TraderDecision,
        modified: TraderDecision,
        reason: VetoReason,
        #[serde(default)]
        warnings: Vec<String>,
    },
    Vetoed {
        original: TraderDecision,
        reason: VetoReason,
    },
}

impl RiskDecision {
    /// The decision the executor should act on (None for veto).
    pub fn effective(&self) -> Option<&TraderDecision> {
        match self {
            Self::Approved { decision, .. }
            | Self::Modified {
                modified: decision, ..
            } => Some(decision),
            Self::Vetoed { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn fixture_open_position() -> OpenPosition {
        OpenPosition {
            asset: AssetSymbol::Btc,
            direction: Direction::Long,
            size_bps: 800,
            entry_price: 70_000.0,
            mark_price: 70_500.0,
            stop_loss_pct: 2.0,
            take_profit_pct: 5.0,
            opened_at: Utc.timestamp_opt(1_699_900_000, 0).single().unwrap(),
            leverage: None,
            liq_price: None,
        }
    }

    fn fixture_portfolio() -> PortfolioState {
        PortfolioState {
            equity_usd: 100_000.0,
            realized_pnl_today_usd: -250.0,
            day_index: 7,
            open_positions: BTreeMap::from([(AssetSymbol::Btc, fixture_open_position())]),
            as_of: Utc.timestamp_opt(1_700_000_000, 0).single().unwrap(),
        }
    }

    fn fixture_decision() -> TraderDecision {
        TraderDecision {
            cycle_id: Uuid::nil(),
            action: Action::Buy,
            size_bps: 1000,
            direction: Direction::Long,
            stop_loss_pct: 2.5,
            take_profit_pct: 5.0,
            trader_summary: "Long entry on confirmed range break with 2:1 R:R.".into(),
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
        }
    }

    #[test]
    fn decision_validates() {
        fixture_decision().validate().expect("fixture must pass");
    }

    #[test]
    fn decision_rejects_oversize_position() {
        let mut d = fixture_decision();
        d.size_bps = 2500;
        d.validate().expect_err("size_bps > 2000 must fail");
    }

    #[test]
    fn decision_rejects_zero_stop_loss() {
        let mut d = fixture_decision();
        d.stop_loss_pct = 0.0;
        d.validate()
            .expect_err("stop_loss_pct < 0.1 must fail (Tier-3 risk hygiene)");
    }

    #[test]
    fn decision_rejects_short_summary() {
        let mut d = fixture_decision();
        d.trader_summary = "ok".into();
        d.validate().expect_err("trader_summary < 10 chars must fail");
    }

    #[test]
    fn decision_round_trips_json() {
        let d = fixture_decision();
        let s = serde_json::to_string(&d).unwrap();
        let back: TraderDecision = serde_json::from_str(&s).unwrap();
        assert_eq!(d, back);
    }

    #[test]
    fn risk_decision_round_trips_json_for_each_variant() {
        let d = fixture_decision();
        let cases = vec![
            RiskDecision::Approved {
                decision: d.clone(),
                warnings: vec![],
            },
            RiskDecision::Modified {
                original: d.clone(),
                modified: TraderDecision {
                    size_bps: 500,
                    ..d.clone()
                },
                reason: VetoReason::PositionTooLarge,
                warnings: vec![],
            },
            RiskDecision::Vetoed {
                original: d.clone(),
                reason: VetoReason::DailyLossCircuitBreaker,
            },
        ];
        for r in cases {
            let s = serde_json::to_string(&r).unwrap();
            let back: RiskDecision = serde_json::from_str(&s).unwrap();
            assert_eq!(r, back);
        }
    }

    #[test]
    fn risk_decision_effective_skips_veto() {
        let d = fixture_decision();
        assert!(RiskDecision::Approved {
            decision: d.clone(),
            warnings: vec![]
        }
        .effective()
        .is_some());
        assert!(RiskDecision::Vetoed {
            original: d,
            reason: VetoReason::AssetNotWhitelisted
        }
        .effective()
        .is_none());
    }

    #[test]
    fn divergence_key_groups_size_into_buckets() {
        let mut d = fixture_decision();
        d.size_bps = 0;
        assert_eq!(d.divergence_key().2, SizeBucket::Zero);
        d.size_bps = 250;
        assert_eq!(d.divergence_key().2, SizeBucket::Small);
        d.size_bps = 1000;
        assert_eq!(d.divergence_key().2, SizeBucket::Medium);
        d.size_bps = 1900;
        assert_eq!(d.divergence_key().2, SizeBucket::Large);
    }

    #[test]
    fn portfolio_validates_and_round_trips() {
        let p = fixture_portfolio();
        p.validate().expect("fixture must pass");
        let s = serde_json::to_string(&p).unwrap();
        let back: PortfolioState = serde_json::from_str(&s).unwrap();
        assert_eq!(p, back);
    }

    #[test]
    fn portfolio_total_exposure_sums_open_positions() {
        let p = fixture_portfolio();
        assert_eq!(p.total_exposure_bps(), 800);
        assert!(!p.is_flat());

        let mut empty = p.clone();
        empty.open_positions.clear();
        assert_eq!(empty.total_exposure_bps(), 0);
        assert!(empty.is_flat());
    }

    #[test]
    fn open_position_rejects_oversize() {
        let mut op = fixture_open_position();
        op.size_bps = 2500;
        op.validate().expect_err("size_bps > 2000 must fail");
    }

    // ── F-6: deny_unknown_fields + cross-field invariants ───────────

    #[test]
    fn decision_rejects_unknown_field() {
        let valid = serde_json::to_value(fixture_decision()).unwrap();
        let mut object = valid.as_object().unwrap().clone();
        object.insert("extra".into(), serde_json::json!(1));
        let err = serde_json::from_value::<TraderDecision>(serde_json::Value::Object(object))
            .expect_err("deny_unknown_fields must reject `extra`");
        assert!(err.to_string().contains("unknown field"));
        assert!(err.to_string().contains("extra"));
    }

    #[test]
    fn risk_decision_rejects_unknown_field() {
        let approved = RiskDecision::Approved {
            decision: fixture_decision(),
            warnings: vec![],
        };
        let valid = serde_json::to_value(&approved).unwrap();
        let mut object = valid.as_object().unwrap().clone();
        object.insert("snuck_in".into(), serde_json::json!(true));
        let err = serde_json::from_value::<RiskDecision>(serde_json::Value::Object(object))
            .expect_err("deny_unknown_fields must reject `snuck_in`");
        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn decision_cross_field_accepts_tp_above_sl_for_buy() {
        let d = fixture_decision();
        assert!(matches!(d.action, Action::Buy));
        assert!(d.take_profit_pct > d.stop_loss_pct);
        d.validate_cross_field()
            .expect("fixture has TP > SL so cross-field passes");
    }

    #[test]
    fn decision_cross_field_rejects_tp_below_sl_for_buy() {
        let mut d = fixture_decision();
        d.stop_loss_pct = 5.0;
        d.take_profit_pct = 3.0; // TP < SL
        let err = d
            .validate_cross_field()
            .expect_err("TP <= SL on a Buy action must fail");
        assert!(err.contains("take_profit_pct"));
        assert!(err.contains("stop_loss_pct"));
    }

    #[test]
    fn decision_cross_field_rejects_tp_equal_sl_for_sell() {
        let mut d = fixture_decision();
        d.action = Action::Sell;
        d.direction = Direction::Short;
        d.stop_loss_pct = 3.0;
        d.take_profit_pct = 3.0; // TP == SL
        let err = d
            .validate_cross_field()
            .expect_err("TP == SL on a Sell action must fail (strict >)");
        assert!(err.contains("take_profit_pct"));
    }

    #[test]
    fn decision_cross_field_skips_flat_action() {
        let mut d = fixture_decision();
        d.action = Action::Flat;
        d.stop_loss_pct = 5.0;
        d.take_profit_pct = 1.0; // would fail for Buy/Sell
        d.validate_cross_field()
            .expect("Flat action skips the directional TP/SL invariant");
    }

    #[test]
    fn decision_cross_field_skips_close_action() {
        let mut d = fixture_decision();
        d.action = Action::Close;
        d.stop_loss_pct = 5.0;
        d.take_profit_pct = 1.0;
        d.validate_cross_field()
            .expect("Close action is an exit; no forward TP/SL asymmetry to enforce");
    }

    #[test]
    fn decision_deserialize_runs_cross_field_check() {
        // PR #302 review P1: the try_from shadow must enforce
        // validate_cross_field on every parse path (DB store, CLI,
        // API), not just when callers happen to call it explicitly.
        let bad = serde_json::json!({
            "cycle_id": Uuid::nil(),
            "action": "buy",
            "size_bps": 1000,
            "direction": "long",
            "stop_loss_pct": 5.0,
            "take_profit_pct": 3.0,
            "trader_summary": "Long entry on confirmed range break with 2:1 R:R.",
            "asset": "BTC",
        });
        let err = serde_json::from_value::<TraderDecision>(bad)
            .expect_err("Buy with TP<=SL must fail deserialization");
        let msg = err.to_string();
        assert!(msg.contains("take_profit_pct"), "{msg}");
        assert!(msg.contains("stop_loss_pct"), "{msg}");
    }

    #[test]
    fn decision_deserialize_passes_for_flat_with_inverted_tp_sl() {
        // Flat actions skip the cross-field rule even via deserialize.
        let raw = serde_json::json!({
            "cycle_id": Uuid::nil(),
            "action": "flat",
            "size_bps": 0,
            "direction": "flat",
            "stop_loss_pct": 5.0,
            "take_profit_pct": 1.0,
            "trader_summary": "Flat — no directional signal.",
            "asset": "BTC",
        });
        let d: TraderDecision = serde_json::from_value(raw)
            .expect("Flat action must skip the TP/SL cross-field rule at parse time");
        assert_eq!(d.action, Action::Flat);
    }

    #[test]
    fn risk_decision_deserialize_propagates_inner_cross_field_failure() {
        // Round-tripping a RiskDecision::Approved with a bad inner
        // TraderDecision must also fail — the inner TraderDecision's
        // try_from runs on the embedded decode.
        let bad = serde_json::json!({
            "verdict": "approved",
            "decision": {
                "cycle_id": Uuid::nil(),
                "action": "sell",
                "size_bps": 1000,
                "direction": "short",
                "stop_loss_pct": 4.0,
                "take_profit_pct": 4.0,
                "trader_summary": "Short on RSI overbought with 1:1 R:R.",
                "asset": "BTC",
            }
        });
        let err = serde_json::from_value::<RiskDecision>(bad)
            .expect_err("inner TraderDecision with TP==SL must reject");
        assert!(err.to_string().contains("take_profit_pct"));
    }

    #[test]
    fn asset_symbol_covers_alpaca_crypto_whitelist() {
        use std::str::FromStr;
        for sym in &[
            "BTC", "ETH", "LTC", "SOL", "AVAX", "LINK", "AAVE", "UNI", "DOT", "DOGE", "SHIB", "MATIC", "BCH",
            "USDT", "USDC",
        ] {
            assert!(AssetSymbol::from_str(sym).is_ok(), "missing variant: {sym}");
        }
    }

    #[test]
    fn asset_symbol_accepts_pair_concatenated_lowercase_and_trimmed_forms() {
        use std::str::FromStr;

        assert_eq!(AssetSymbol::from_str("eth/usd").unwrap(), AssetSymbol::Eth);
        // "SOLUSD" is format-valid but parses as "SOLUSD" (not "SOL") in the
        // new permissive newtype design — the slash-splitting only strips after
        // a "/" character.
        assert_eq!(
            AssetSymbol::from_str(" SOLUSD ").unwrap(),
            AssetSymbol::from_static("SOLUSD"),
        );
        assert_eq!(AssetSymbol::from_str("link").unwrap(), AssetSymbol::Link);
    }

    // ── New tests for the interned Copy newtype (W1) ──────────────────────────

    #[test]
    fn asset_symbol_newtype_eq_is_value_based() {
        use std::str::FromStr;
        // const Btc equals an interned "BTC"
        let interned = AssetSymbol::from_str("BTC").unwrap();
        assert_eq!(interned, AssetSymbol::Btc);
    }

    #[test]
    fn asset_symbol_serde_round_trips_as_scalar_string() {
        let sym = AssetSymbol::Btc;
        let json = serde_json::to_string(&sym).unwrap();
        assert_eq!(json, r#""BTC""#);
        let back: AssetSymbol = serde_json::from_str(&json).unwrap();
        assert_eq!(back, sym);
    }

    #[test]
    fn asset_symbol_from_str_permissive_accepts_new_ticker() {
        use std::str::FromStr;
        // HYPE is not in the old enum but must succeed format validation
        let hype = AssetSymbol::from_str("HYPE").unwrap();
        assert_eq!(hype.as_str(), "HYPE");
    }

    #[test]
    fn asset_symbol_from_str_strips_slash_pair() {
        use std::str::FromStr;
        // "ETH/USD" → "ETH"
        let eth = AssetSymbol::from_str("ETH/USD").unwrap();
        assert_eq!(eth, AssetSymbol::Eth);
    }

    #[test]
    fn asset_symbol_from_str_rejects_invalid_format() {
        use std::str::FromStr;
        assert!(AssetSymbol::from_str("").is_err());
        assert!(AssetSymbol::from_str("BTC$").is_err());
        assert!(AssetSymbol::from_str("btc!").is_err());
    }

    #[test]
    fn asset_symbol_serde_btc_map_key() {
        // BTreeMap<AssetSymbol, i32> must serialize/deserialize with string keys
        let mut m = std::collections::BTreeMap::new();
        m.insert(AssetSymbol::Btc, 42i32);
        let json = serde_json::to_string(&m).unwrap();
        assert_eq!(json, r#"{"BTC":42}"#);
        let back: std::collections::BTreeMap<AssetSymbol, i32> = serde_json::from_str(&json).unwrap();
        assert_eq!(back[&AssetSymbol::Btc], 42);
    }

    proptest::proptest! {
        #[test]
        fn size_bucket_total(bps in 0u32..=2000) {
            let _ = SizeBucket::from_bps(bps); // never panics
        }

        #[test]
        fn decision_size_bps_validation_is_total(bps in 0u32..=5000) {
            let mut d = fixture_decision();
            d.size_bps = bps;
            let result = d.validate();
            if bps <= 2000 {
                proptest::prop_assert!(result.is_ok());
            } else {
                proptest::prop_assert!(result.is_err());
            }
        }
    }
}
