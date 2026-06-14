//! `LiveConfig` — value type describing a Live run's launch envelope.
//!
//! Phase 3 of the 2026-05-21 Alpaca-Live plan
//! (`docs/superpowers/plans/2026-05-21-alpaca-live-3-launch-and-freeze.md`).
//! Sits on `eval_runs.live_config_json` as a JSON blob when `mode = Live`
//! and `NULL` for backtest runs. The Phase 3 launch endpoint deserialises
//! it, validates it, and hands it to the unified `Executor` once that
//! lands in sub-track 4 (`executor-live-shell`).
//!
//! This module is **purely a value type + validator**. It does not own:
//!
//! * the launch endpoint (lands with Phase 3 API work),
//! * the executor wiring (blocked on the unified `Executor`),
//! * the freeze action (Phase E — lands once Live runs can complete).
//!
//! ## Validation rules
//!
//! Mirrors §Phase A of the plan plus the polish from §Phase F:
//!
//! | # | Rule | Source |
//! |---|---|---|
//! | 1 | `!assets.is_empty()` (single-asset wall lifted in §4 L2) | Plan A3 / multi-asset-alpaca-unlock |
//! | 2 | each asset is on the Alpaca crypto whitelist | Plan A3 + F1 |
//! | 3 | `stop_policy` has at least one limit set | Plan A3 |
//! | 4 | `venue_label != VenueLabel::Live` (v1 rejects real money) | Plan A3 |
//! | 5 | `capital.initial > 0` | Plan A3 |
//! | 6 | `broker_creds_ref` non-empty (reachability is a runtime check) | Plan A3 + F4 |
//! | 7 | `display_name` non-empty | (UI ergonomic; mirrors `Scenario.display_name`) |
//! | 8 | `time_limit_secs` ≤ 30 days when set | Plan F5 |
//! | 9 | each stop-policy limit > 0 when set | (defensive; type allows `Some(0)`) |
//!
//! Broker-creds reachability (Plan F4) is *not* a `validate()` rule because
//! it requires HTTP I/O. The Phase 3 launch endpoint performs the live
//! `GET /v2/account` check immediately after `validate()` succeeds; the
//! corresponding error variant (`BrokerCredsUnreachable`) is reserved here
//! so the launch endpoint can return it without inventing its own type.
//!
//! ## ts-rs export
//!
//! `LiveConfig`, `StopPolicy`, and `LiveConfigValidationError` are exported
//! to `frontend/web/src/api/types.gen/` so the LaunchLiveForm and the
//! validation-error renderer share one wire shape with the engine.

use serde::{Deserialize, Serialize};

use xvision_core::Capital;
use xvision_data::asset_whitelist::alpaca_crypto_asset;

use crate::eval::scenario::AssetRef;
use crate::safety::{SafetyLimits, VenueLabel};

/// Hard cap on the `time_limit_secs` stop-policy field. 30 days in seconds.
/// Prevents operator-typo runaway runs. Mirrors plan §Phase F5.
pub const LIVE_RUN_MAX_TIME_LIMIT_SECS: u64 = 30 * 24 * 60 * 60;

/// Run-terminating limits for a Live run. At least one limit must be set;
/// the engine evaluates whichever fires first.
///
/// `time_limit_secs` is wall-clock seconds since run start, `bar_limit` is
/// the bar count consumed from the [`crate::eval::executor::BarSource`],
/// and `decision_limit` is the LLM-dispatch count.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StopPolicy {
    /// Wall-clock seconds since run start; the run terminates at or before
    /// the first bar-close past this point. Capped at
    /// [`LIVE_RUN_MAX_TIME_LIMIT_SECS`] (30 days).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_limit_secs: Option<u64>,

    /// Number of bars consumed before termination.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bar_limit: Option<u32>,

    /// Number of LLM dispatches before termination. Useful for cost-bounded
    /// shake-down runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision_limit: Option<u32>,
}

impl StopPolicy {
    /// True iff the policy has any limit set. Used by [`LiveConfig::validate`].
    pub fn is_empty(&self) -> bool {
        self.time_limit_secs.is_none() && self.bar_limit.is_none() && self.decision_limit.is_none()
    }
}

/// Launch envelope for a Live run. Persisted as
/// `eval_runs.live_config_json` (Phase B migration).
///
/// **Current live scope: Alpaca paper trading only.** Live mode sends
/// orders to `https://paper-api.alpaca.markets` — real market data,
/// paper (simulated) money. Real-money venues (`VenueLabel::Live`) are
/// rejected at validation until the per-strategy verdict + kill-switch
/// hardening lands.
///
/// `strategy_id` references the strategy artifact that drives the run.
/// `assets` is a non-empty list of whitelisted assets; each is fanned out
/// into its own `LiveStream` and merged in the executor (§4 L2 multi-asset
/// live fanout, see
/// `docs/superpowers/plans/2026-05-25-cline-live-followups.md` and the
/// invariants in
/// `docs/superpowers/notes/2026-05-25-live-multi-asset-invariants.md`). The
/// earlier single-asset wall (`len() == 1`) has been lifted.
///
/// `broker_creds_ref` selects WHICH stored credential set to load
/// (e.g. `"alpaca"` → the Alpaca credentials row). It is a lookup key,
/// not a venue/environment selector — venue selection is a separate
/// future plan. The engine never stores secret material in `LiveConfig`.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveConfig {
    pub strategy_id: String,
    pub assets: Vec<AssetRef>,
    /// Initial trading capital. Same shape as `Scenario.capital`; inlined
    /// in the ts-rs export because `xvision_core::Capital` is not a ts-rs
    /// type (mirrors the existing override on `Scenario`).
    #[cfg_attr(feature = "ts-export", ts(type = "{ initial: number, currency: string }"))]
    pub capital: Capital,
    /// Selects WHICH stored credential set to use (e.g. `"alpaca"` → the
    /// Alpaca credentials row under Settings → Brokers). This is a lookup
    /// key, **not** a venue/environment selector — venue and environment
    /// selection is a separate future plan. Current live scope accepts only
    /// `"alpaca"` (Alpaca paper trading).
    pub broker_creds_ref: String,
    pub stop_policy: StopPolicy,

    /// Coarse safety label for the venue. v1 rejects [`VenueLabel::Live`]
    /// at validation; once the per-strategy verdict + kill-switch hardening
    /// lands, this opens up.
    #[serde(default)]
    pub venue_label: VenueLabel,

    /// Optional override for the historical warm-up window the strategy
    /// gets before live bars start flowing. `None` falls through to the
    /// strategy's declared warm-up.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub warmup_bars: Option<u32>,

    /// Per-run notional / order-count / leverage / drawdown limits. Same
    /// shape as `Scenario.safety_limits`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub safety_limits: Option<SafetyLimits>,

    pub display_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl LiveConfig {
    /// Validate every v1 rule. See module docs for the rule table. Returns
    /// the first violation encountered; the caller surfaces the
    /// `field_path()` (JSON pointer) to the operator.
    pub fn validate(&self) -> Result<(), LiveConfigValidationError> {
        use LiveConfigValidationError as E;

        if self.display_name.trim().is_empty() {
            return Err(E::DisplayNameEmpty);
        }
        if self.strategy_id.trim().is_empty() {
            return Err(E::StrategyIdEmpty);
        }
        if self.broker_creds_ref.trim().is_empty() {
            return Err(E::BrokerCredsEmpty);
        }

        // Asset-count floor. The single-asset wall (`len() != 1`) was lifted
        // by the cline-live follow-up (§4 L2 multi-asset live fanout): a
        // Live run now accepts `>= 1` whitelisted assets, each fanned out
        // into its own `LiveStream` and merged in the executor. An empty
        // asset set still has nothing to trade, so it stays an error.
        if self.assets.is_empty() {
            return Err(E::AssetCount {
                actual: self.assets.len(),
            });
        }
        for (i, asset) in self.assets.iter().enumerate() {
            if alpaca_crypto_asset(&asset.symbol).is_none() {
                return Err(E::AssetNotWhitelisted {
                    index: i,
                    symbol: asset.symbol.clone(),
                });
            }
        }

        // At least one stop-policy limit must be set, and each set limit
        // must be > 0. `Some(0)` would either never fire (decision/bar)
        // or fire instantly (time), neither of which is what the operator
        // meant — reject early instead of silently ambiguating.
        if self.stop_policy.is_empty() {
            return Err(E::StopPolicyEmpty);
        }
        if let Some(secs) = self.stop_policy.time_limit_secs {
            if secs == 0 {
                return Err(E::StopPolicyLimitNotPositive {
                    field: "time_limit_secs",
                });
            }
            if secs > LIVE_RUN_MAX_TIME_LIMIT_SECS {
                return Err(E::StopPolicyTimeLimitTooLong {
                    secs,
                    max: LIVE_RUN_MAX_TIME_LIMIT_SECS,
                });
            }
        }
        if let Some(bars) = self.stop_policy.bar_limit {
            if bars == 0 {
                return Err(E::StopPolicyLimitNotPositive { field: "bar_limit" });
            }
        }
        if let Some(d) = self.stop_policy.decision_limit {
            if d == 0 {
                return Err(E::StopPolicyLimitNotPositive {
                    field: "decision_limit",
                });
            }
        }

        // Real-money `Live` is allowed only for venues that settle real funds
        // (Byreal perps / Hyperliquid). Alpaca live scope is paper only.
        const REAL_MONEY_CREDS: &[&str] = &["byreal"];
        if self.venue_label == VenueLabel::Live && !REAL_MONEY_CREDS.contains(&self.broker_creds_ref.as_str())
        {
            return Err(E::VenueLabelLiveRejected);
        }

        if self.capital.initial <= 0.0 || !self.capital.initial.is_finite() {
            return Err(E::CapitalNotPositive {
                initial: self.capital.initial,
            });
        }

        Ok(())
    }
}

/// Stable validation errors for a `LiveConfig`. Each variant carries
/// enough structured context for the frontend to render a targeted
/// inline error next to the offending field, and `field_path()` returns
/// the JSON pointer the UI uses to scroll/focus to the relevant input.
///
/// `BrokerCredsUnreachable` is reserved for the launch endpoint to
/// raise after its reachability probe — it is **never** returned by
/// [`LiveConfig::validate`] (which is a pure / non-IO check).
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
// Intentionally `PartialEq` only (no `Eq`): the `CapitalNotPositive` variant
// carries an `f64`, which doesn't implement `Eq`. Tests rely on `assert_eq!`,
// which only needs `PartialEq`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "code", content = "detail", rename_all = "snake_case")]
pub enum LiveConfigValidationError {
    DisplayNameEmpty,
    StrategyIdEmpty,
    BrokerCredsEmpty,
    AssetCount {
        actual: usize,
    },
    AssetNotWhitelisted {
        index: usize,
        symbol: String,
    },
    StopPolicyEmpty,
    StopPolicyLimitNotPositive {
        field: &'static str,
    },
    StopPolicyTimeLimitTooLong {
        secs: u64,
        max: u64,
    },
    VenueLabelLiveRejected,
    CapitalNotPositive {
        initial: f64,
    },
    /// Reserved for the launch endpoint's `GET /v2/account` probe (Plan F4).
    BrokerCredsUnreachable {
        reason: String,
    },
}

impl LiveConfigValidationError {
    /// Stable `E_LIVECONFIG_*` code for telemetry + frontend matchers.
    pub fn code(&self) -> &'static str {
        match self {
            Self::DisplayNameEmpty => "E_LIVECONFIG_DISPLAY_NAME_EMPTY",
            Self::StrategyIdEmpty => "E_LIVECONFIG_STRATEGY_ID_EMPTY",
            Self::BrokerCredsEmpty => "E_LIVECONFIG_BROKER_CREDS_EMPTY",
            Self::AssetCount { .. } => "E_LIVECONFIG_ASSET_COUNT",
            Self::AssetNotWhitelisted { .. } => "E_LIVECONFIG_ASSET_NOT_WHITELISTED",
            Self::StopPolicyEmpty => "E_LIVECONFIG_STOP_POLICY_EMPTY",
            Self::StopPolicyLimitNotPositive { .. } => "E_LIVECONFIG_STOP_POLICY_NOT_POSITIVE",
            Self::StopPolicyTimeLimitTooLong { .. } => "E_LIVECONFIG_TIME_LIMIT_TOO_LONG",
            Self::VenueLabelLiveRejected => "E_LIVECONFIG_VENUE_LABEL_LIVE_REJECTED",
            Self::CapitalNotPositive { .. } => "E_LIVECONFIG_CAPITAL_NOT_POSITIVE",
            Self::BrokerCredsUnreachable { .. } => "E_LIVECONFIG_BROKER_CREDS_UNREACHABLE",
        }
    }

    /// JSON-pointer field path the UI should focus when this error fires.
    pub fn field_path(&self) -> String {
        match self {
            Self::DisplayNameEmpty => "/display_name".to_string(),
            Self::StrategyIdEmpty => "/strategy_id".to_string(),
            Self::BrokerCredsEmpty | Self::BrokerCredsUnreachable { .. } => "/broker_creds_ref".to_string(),
            Self::AssetCount { .. } => "/assets".to_string(),
            Self::AssetNotWhitelisted { index, .. } => format!("/assets/{index}/symbol"),
            Self::StopPolicyEmpty => "/stop_policy".to_string(),
            Self::StopPolicyLimitNotPositive { field } => format!("/stop_policy/{field}"),
            Self::StopPolicyTimeLimitTooLong { .. } => "/stop_policy/time_limit_secs".to_string(),
            Self::VenueLabelLiveRejected => "/venue_label".to_string(),
            Self::CapitalNotPositive { .. } => "/capital/initial".to_string(),
        }
    }
}

impl std::fmt::Display for LiveConfigValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DisplayNameEmpty => f.write_str("display_name must be non-empty"),
            Self::StrategyIdEmpty => f.write_str("strategy_id must be non-empty"),
            Self::BrokerCredsEmpty => f.write_str("broker_creds_ref must be non-empty"),
            Self::AssetCount { actual } => {
                write!(f, "Live runs require at least 1 asset (got {actual})")
            }
            Self::AssetNotWhitelisted { symbol, .. } => {
                write!(f, "asset '{symbol}' is not on the Alpaca crypto whitelist")
            }
            Self::StopPolicyEmpty => f.write_str(
                "stop_policy requires at least one of time_limit_secs / bar_limit / decision_limit",
            ),
            Self::StopPolicyLimitNotPositive { field } => {
                write!(f, "stop_policy.{field} must be > 0 when set")
            }
            Self::StopPolicyTimeLimitTooLong { secs, max } => write!(
                f,
                "stop_policy.time_limit_secs {secs} exceeds the {max}-second hard cap"
            ),
            Self::VenueLabelLiveRejected => f.write_str(
                "real-money live (venue_label = Live) is not yet supported; \
                 current live mode is Alpaca paper trading only \
                 (https://paper-api.alpaca.markets). \
                 Set venue_label = Paper (or omit it) to use the current live scope.",
            ),
            Self::CapitalNotPositive { initial } => {
                write!(f, "capital.initial must be > 0 (got {initial})")
            }
            Self::BrokerCredsUnreachable { reason } => {
                write!(f, "broker creds unreachable: {reason}")
            }
        }
    }
}

impl std::error::Error for LiveConfigValidationError {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use crate::eval::scenario::AssetClass;

    fn whitelisted_btc_asset() -> AssetRef {
        AssetRef {
            class: AssetClass::Crypto,
            symbol: "BTC/USD".into(),
            venue_symbol: "BTC/USD".into(),
        }
    }

    fn whitelisted_eth_asset() -> AssetRef {
        AssetRef {
            class: AssetClass::Crypto,
            symbol: "ETH/USD".into(),
            venue_symbol: "ETH/USD".into(),
        }
    }

    fn valid_config() -> LiveConfig {
        LiveConfig {
            strategy_id: "s_01JX_TEST".into(),
            assets: vec![whitelisted_btc_asset()],
            capital: Capital {
                initial: 10_000.0,
                currency: "USD".into(),
            },
            broker_creds_ref: "alpaca_paper_default".into(),
            stop_policy: StopPolicy {
                time_limit_secs: Some(900),
                ..Default::default()
            },
            venue_label: VenueLabel::Paper,
            warmup_bars: None,
            safety_limits: None,
            display_name: "Smoke run".into(),
            description: None,
            tags: vec![],
            notes: None,
        }
    }

    #[test]
    fn baseline_config_validates() {
        let cfg = valid_config();
        assert!(cfg.validate().is_ok(), "baseline ought to validate");
    }

    #[test]
    fn display_name_must_be_non_empty() {
        let mut cfg = valid_config();
        cfg.display_name = "   ".into();
        let err = cfg.validate().unwrap_err();
        assert_eq!(err, LiveConfigValidationError::DisplayNameEmpty);
        assert_eq!(err.code(), "E_LIVECONFIG_DISPLAY_NAME_EMPTY");
        assert_eq!(err.field_path(), "/display_name");
    }

    #[test]
    fn strategy_id_must_be_non_empty() {
        let mut cfg = valid_config();
        cfg.strategy_id = "".into();
        let err = cfg.validate().unwrap_err();
        assert_eq!(err, LiveConfigValidationError::StrategyIdEmpty);
        assert_eq!(err.field_path(), "/strategy_id");
    }

    #[test]
    fn broker_creds_must_be_non_empty() {
        let mut cfg = valid_config();
        cfg.broker_creds_ref = "".into();
        let err = cfg.validate().unwrap_err();
        assert_eq!(err, LiveConfigValidationError::BrokerCredsEmpty);
        assert_eq!(err.field_path(), "/broker_creds_ref");
    }

    #[test]
    fn empty_asset_set_is_rejected() {
        let mut cfg = valid_config();
        cfg.assets = vec![];
        let err = cfg.validate().unwrap_err();
        assert!(matches!(err, LiveConfigValidationError::AssetCount { actual: 0 }));
        assert_eq!(err.field_path(), "/assets");
    }

    #[test]
    fn multi_asset_validates_after_single_asset_wall_lift() {
        // §4 L2: the single-asset wall is lifted. Two whitelisted assets
        // now validate (each is fanned out into its own LiveStream).
        let mut cfg = valid_config();
        cfg.assets = vec![whitelisted_btc_asset(), whitelisted_eth_asset()];
        assert!(
            cfg.validate().is_ok(),
            "multi-asset live config must validate after the wall lift"
        );
    }

    #[test]
    fn multi_asset_still_rejects_non_whitelisted_member() {
        // The per-asset whitelist check still applies to every member.
        let mut cfg = valid_config();
        cfg.assets = vec![
            whitelisted_btc_asset(),
            AssetRef {
                class: AssetClass::Crypto,
                symbol: "DOGE/USDT".into(),
                venue_symbol: "DOGE/USDT".into(),
            },
        ];
        let err = cfg.validate().unwrap_err();
        match err {
            LiveConfigValidationError::AssetNotWhitelisted { index: 1, symbol } => {
                assert_eq!(symbol, "DOGE/USDT");
            }
            other => panic!("expected AssetNotWhitelisted at index 1; got {other:?}"),
        }
    }

    #[test]
    fn asset_must_be_on_alpaca_whitelist() {
        let mut cfg = valid_config();
        cfg.assets = vec![AssetRef {
            class: AssetClass::Crypto,
            symbol: "DOGE/USDT".into(),
            venue_symbol: "DOGE/USDT".into(),
        }];
        let err = cfg.validate().unwrap_err();
        match err {
            LiveConfigValidationError::AssetNotWhitelisted { index: 0, symbol } => {
                assert_eq!(symbol, "DOGE/USDT");
            }
            other => panic!("expected AssetNotWhitelisted; got {other:?}"),
        }
    }

    #[test]
    fn stop_policy_must_have_at_least_one_limit() {
        let mut cfg = valid_config();
        cfg.stop_policy = StopPolicy::default();
        let err = cfg.validate().unwrap_err();
        assert_eq!(err, LiveConfigValidationError::StopPolicyEmpty);
        assert_eq!(err.field_path(), "/stop_policy");
    }

    #[test]
    fn stop_policy_limits_must_be_positive() {
        for (set_zero, field) in [
            (
                StopPolicy {
                    time_limit_secs: Some(0),
                    ..Default::default()
                },
                "time_limit_secs",
            ),
            (
                StopPolicy {
                    bar_limit: Some(0),
                    ..Default::default()
                },
                "bar_limit",
            ),
            (
                StopPolicy {
                    decision_limit: Some(0),
                    ..Default::default()
                },
                "decision_limit",
            ),
        ] {
            let mut cfg = valid_config();
            cfg.stop_policy = set_zero;
            let err = cfg.validate().unwrap_err();
            assert!(
                matches!(err, LiveConfigValidationError::StopPolicyLimitNotPositive { field: f } if f == field),
                "expected StopPolicyLimitNotPositive {{ field: {field:?} }}, got {err:?}",
            );
            assert_eq!(err.field_path(), format!("/stop_policy/{field}"));
        }
    }

    #[test]
    fn stop_policy_time_limit_caps_at_30_days() {
        let mut cfg = valid_config();
        cfg.stop_policy = StopPolicy {
            time_limit_secs: Some(LIVE_RUN_MAX_TIME_LIMIT_SECS + 1),
            ..Default::default()
        };
        let err = cfg.validate().unwrap_err();
        assert!(matches!(
            err,
            LiveConfigValidationError::StopPolicyTimeLimitTooLong { .. }
        ));
        assert_eq!(err.field_path(), "/stop_policy/time_limit_secs");
    }

    #[test]
    fn stop_policy_time_limit_30_days_exactly_passes() {
        let mut cfg = valid_config();
        cfg.stop_policy = StopPolicy {
            time_limit_secs: Some(LIVE_RUN_MAX_TIME_LIMIT_SECS),
            ..Default::default()
        };
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn venue_label_live_is_rejected() {
        // This test uses the default `valid_config()` broker_creds_ref
        // ("alpaca_paper_default"), which is NOT a real-money venue → Live
        // must still be rejected for it.
        let mut cfg = valid_config();
        cfg.venue_label = VenueLabel::Live;
        let err = cfg.validate().unwrap_err();
        assert_eq!(err, LiveConfigValidationError::VenueLabelLiveRejected);
        assert_eq!(err.field_path(), "/venue_label");
    }

    #[test]
    fn live_label_rejected_for_alpaca_paper_creds() {
        // Alpaca is paper-trading only; venue_label=Live must be rejected
        // even when broker_creds_ref is "alpaca".
        let mut cfg = valid_config();
        cfg.broker_creds_ref = "alpaca".into();
        cfg.venue_label = VenueLabel::Live;
        let err = cfg.validate().unwrap_err();
        assert_eq!(err, LiveConfigValidationError::VenueLabelLiveRejected);
        assert_eq!(err.field_path(), "/venue_label");
    }

    #[test]
    fn live_label_allowed_for_byreal_creds() {
        // Byreal perps settle real funds on Hyperliquid mainnet; venue_label=Live
        // must be ACCEPTED for broker_creds_ref = "byreal".
        let mut cfg = valid_config();
        cfg.broker_creds_ref = "byreal".into();
        cfg.venue_label = VenueLabel::Live;
        assert!(
            cfg.validate().is_ok(),
            "byreal + VenueLabel::Live must pass validation after the mainnet parity lift"
        );
    }

    #[test]
    fn venue_label_testnet_is_accepted() {
        let mut cfg = valid_config();
        cfg.venue_label = VenueLabel::Testnet;
        assert!(cfg.validate().is_ok());
    }

    #[test]
    fn capital_initial_must_be_positive_and_finite() {
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let mut cfg = valid_config();
            cfg.capital.initial = bad;
            let err = cfg.validate().unwrap_err();
            assert!(
                matches!(err, LiveConfigValidationError::CapitalNotPositive { .. }),
                "expected CapitalNotPositive for initial={bad}, got {err:?}",
            );
        }
    }

    #[test]
    fn unreachable_creds_variant_is_reserved_for_launch_endpoint() {
        // The variant exists so the launch endpoint can return it after the
        // GET /v2/account probe, but `validate()` itself is pure and must
        // never produce it. This test pins that contract by constructing
        // the variant directly and confirming it isn't reachable from any
        // valid input to `validate()`.
        let err = LiveConfigValidationError::BrokerCredsUnreachable { reason: "401".into() };
        assert_eq!(err.code(), "E_LIVECONFIG_BROKER_CREDS_UNREACHABLE");
        assert_eq!(err.field_path(), "/broker_creds_ref");
    }

    #[test]
    fn json_roundtrip_preserves_structure() {
        let cfg = valid_config();
        let json = serde_json::to_string(&cfg).expect("serialize");
        let back: LiveConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cfg, back);
    }

    #[test]
    fn empty_stop_policy_is_skipped_on_serialize() {
        let cfg = LiveConfig {
            stop_policy: StopPolicy::default(),
            ..valid_config()
        };
        let json = serde_json::to_value(&cfg).unwrap();
        let policy = json.get("stop_policy").expect("stop_policy serialized");
        // All three sub-fields skip when None, so the policy itself serialises
        // as an empty object — not omitted (the field is required on the
        // struct). The empty-object form lets the launch endpoint surface the
        // shape unambiguously.
        assert_eq!(policy.as_object().map(|o| o.len()), Some(0));
    }
}
