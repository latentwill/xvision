//! TOML loader for `config/risk.toml`.

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::Path;

use crate::RiskError;

#[derive(Debug, Clone, Deserialize)]
pub struct Limits {
    pub max_position_pct_nav: f64,
    pub max_total_exposure_pct: f64,
    pub max_open_positions: usize,
    pub max_daily_loss_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stops {
    pub stop_loss_required: bool,
    pub stop_loss_min_pct: f64,
    pub stop_loss_max_pct: f64,
    pub take_profit_required: bool,
    pub take_profit_min_rr: f64,
}

/// Per-venue deterministic constraints (broker min-notional, etc.).
///
/// The `MinNotional` rule reads `min_notional_usd` to pre-submit-veto
/// orders the broker would otherwise reject for `cost basis must be >=
/// minimal amount of order N`. Default 0.0 = no-op (matches the
/// pre-rule behavior on venues with no recorded minimum).
///
/// Reasonable values (verify against the venue docs before relying on
/// them in production):
/// - `paper` (Alpaca paper crypto): `10.0` — Alpaca paper enforces a
///   $10 minimum cost basis on crypto market orders; the rejection
///   message is `"cost basis must be >= minimal amount of order 10"`,
///   which is what PR #314 already classifies on the post-submit path.
/// - `live` (Alpaca live crypto): `1.0` — Alpaca live crypto has a
///   $1 minimum order size on most pairs per the public docs at
///   <https://docs.alpaca.markets/docs/crypto-trading> ("Order
///   minimums"). Keep `1.0` as the conservative default; bump per
///   asset only if a venue surface adds per-symbol overrides.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct VenueLimits {
    /// Minimum order notional in USD (`size × reference_price`). The
    /// risk rule vetoes when notional is strictly less than this
    /// value. `0.0` (the default) disables the rule for the venue.
    #[serde(default)]
    pub min_notional_usd: f64,
}

/// Perps-specific risk guards. Every field has a safe default so an existing
/// `config/risk.toml` with no `[perps]` section keeps working. Guards only bite
/// on perps venues that report the relevant signal (e.g. byreal positions carry
/// leverage/liq price); spot venues leave it `None`, so the guards no-op.
///
/// NOTE: PR #985 (funding carry guard) introduces this same `PerpsGuards`
/// struct with a `max_funding_pay_8h` field. When both land, combine the two
/// field sets into one `PerpsGuards` — a trivial merge.
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct PerpsGuards {
    /// Minimum distance (percent of mark) an open perps position's liquidation
    /// price must keep from its mark before `LiquidationDistanceGuard` vetoes
    /// *new* entries — don't pile on risk while a position is near liquidation.
    /// Default `5.0`. Only bites when a position reports a `liq_price` (perps).
    pub min_liq_distance_pct: f64,
}

impl Default for PerpsGuards {
    fn default() -> Self {
        Self {
            min_liq_distance_pct: 5.0,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    pub limits: Limits,
    pub stops: Stops,
    /// Per-venue deterministic constraints. Indexed by the venue id
    /// the executor identifies itself with (today: `"paper"` and
    /// `"live"`). Absent venues use `VenueLimits::default()` — no
    /// constraints applied, matching pre-rule behavior.
    #[serde(default)]
    pub venues: BTreeMap<String, VenueLimits>,
    /// Perps-specific guards. Absent `[perps]` section ⇒ `PerpsGuards::default()`.
    #[serde(default)]
    pub perps: PerpsGuards,
}

impl RiskConfig {
    /// Return the configured limits for `venue_id`, falling back to the
    /// default (all-zero) when the venue is unconfigured. The
    /// `MinNotional` rule treats the zero default as a no-op, so an
    /// unknown venue inherits today's pass-everything behavior.
    pub fn venue_limits(&self, venue_id: &str) -> VenueLimits {
        self.venues.get(venue_id).cloned().unwrap_or_default()
    }

    pub fn from_path(path: &Path) -> Result<Self, RiskError> {
        let raw = std::fs::read_to_string(path)
            .map_err(|e| RiskError::Config(format!("cannot read {}: {e}", path.display())))?;
        let cfg: RiskConfig = toml::from_str(&raw)
            .map_err(|e| RiskError::Config(format!("parse error in {}: {e}", path.display())))?;
        cfg.validate()?;
        Ok(cfg)
    }

    fn validate(&self) -> Result<(), RiskError> {
        let l = &self.limits;
        let s = &self.stops;

        if !(l.max_position_pct_nav > 0.0 && l.max_position_pct_nav < 100.0) {
            return Err(RiskError::Config(
                "max_position_pct_nav must be in (0, 100)".into(),
            ));
        }
        if !(l.max_total_exposure_pct > 0.0 && l.max_total_exposure_pct <= 500.0) {
            return Err(RiskError::Config(
                "max_total_exposure_pct must be in (0, 500]".into(),
            ));
        }
        if l.max_open_positions == 0 {
            return Err(RiskError::Config("max_open_positions must be > 0".into()));
        }
        if !(l.max_daily_loss_pct > 0.0 && l.max_daily_loss_pct <= 100.0) {
            return Err(RiskError::Config("max_daily_loss_pct must be in (0, 100]".into()));
        }
        if s.stop_loss_min_pct <= 0.0 {
            return Err(RiskError::Config("stop_loss_min_pct must be > 0".into()));
        }
        if s.stop_loss_max_pct <= s.stop_loss_min_pct {
            return Err(RiskError::Config(
                "stop_loss_max_pct must be > stop_loss_min_pct".into(),
            ));
        }
        if s.take_profit_min_rr <= 0.0 {
            return Err(RiskError::Config("take_profit_min_rr must be > 0".into()));
        }
        for (venue, limits) in &self.venues {
            if !limits.min_notional_usd.is_finite() || limits.min_notional_usd < 0.0 {
                return Err(RiskError::Config(format!(
                    "venues.{venue}.min_notional_usd must be a finite non-negative number"
                )));
            }
        }
        if !self.perps.min_liq_distance_pct.is_finite() || self.perps.min_liq_distance_pct < 0.0 {
            return Err(RiskError::Config(
                "perps.min_liq_distance_pct must be a finite non-negative number".into(),
            ));
        }
        Ok(())
    }
}
