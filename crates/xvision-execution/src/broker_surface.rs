//! Unified broker surface — extracted from Plan 2c §Task 7 for the v1
//! eval engine plan to depend on without pulling in the deferred scheduler
//! / live daemon. Wraps the existing `alpaca` and `orderly` modules behind a
//! single trait + enum so callers (eval paper executor, future live daemon)
//! pick a broker at runtime.
//!
//! The trait is intentionally narrower than the existing `Executor` trait
//! (which takes a fully-formed `RiskDecision`). Callers that already have a
//! pipeline-driven decision should keep using `Executor::submit`. Callers
//! that just need to "place this order at the broker" — e.g. eval paper
//! mode, or a future deterministic test harness — use `BrokerSurface`.
//!
//! See `docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md#task-7-brokersurface-trait--dispatch`
//! for the original spec, and `team/briefings/broker-surface.md` for the v1
//! scope cut (Alpaca paper surface fully implemented; live surfaces stubbed).

use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use xvision_core::AssetSymbol;

use crate::alpaca::{AlpacaApi, ApacClientApi, OrderRequest as ApacOrderRequest, OrderSide as ApacSide};
use crate::orderly::{
    orderly_symbol_for, AlgoKind, Credentials as OrderlyCredentials, OrderSide as OrderlyOrderSide,
    OrderlyApi, OrderlyOrder, ReqwestOrderlyApi, ORDERLY_MAINNET_BASE,
};

/// Returns `true` when `asset` parses as an Alpaca crypto whitelist symbol
/// (`"BTC"`, `"BTC/USD"`, `"BTCUSD"`, etc.). Used by `AlpacaPaperSurface` and
/// downstream callers (the eval paper executor) to gate behaviour that
/// differs between Alpaca's crypto and (future) equities API surfaces:
///
/// - Crypto orders never use bracket / OCO / OTOCO classes — only simple
///   market or limit orders. Bracket take-profit / stop-loss legs are
///   silently dropped before submission.
/// - Crypto is long-only on Alpaca; opening a short position from flat is
///   not supported. Callers should refuse `short_open` for crypto assets
///   rather than round-tripping through the API.
///
/// Non-crypto symbols (e.g. a future `"AAPL"` equity) return `false` and
/// take the legacy bracket + bidirectional code path.
pub fn is_alpaca_crypto(asset: &str) -> bool {
    use xvision_core::asset_registry;
    match AssetSymbol::from_str(asset) {
        Ok(sym) => {
            if asset_registry::is_alpaca_crypto(sym) {
                return true;
            }
            // Handle compound forms like "BTCUSD" → try stripping "USDC"/"USD"
            // suffix so callers passing Alpaca-style pairs without a slash still
            // resolve correctly.
            let upper = asset.trim().to_ascii_uppercase();
            let base = upper.strip_suffix("USDC").or_else(|| upper.strip_suffix("USD"));
            if let Some(b) = base {
                if !b.is_empty() {
                    if let Ok(base_sym) = AssetSymbol::from_str(b) {
                        return asset_registry::is_alpaca_crypto(base_sym);
                    }
                }
            }
            false
        }
        Err(_) => false,
    }
}

// ── Public types ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrokerKind {
    AlpacaPaper,
    AlpacaLive,
    OrderlyLive,
    BybitPaper,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrderRequest {
    pub asset: String,
    pub side: Side,
    /// Base-asset units (e.g., 0.05 BTC). The broker impl converts to
    /// whatever unit its API requires (notional for Alpaca, base qty for
    /// Orderly).
    pub size: f64,
    /// Reference price chosen by the caller for base-size to notional
    /// conversion and bracket-leg derivation. Paper evals pass the current
    /// historical replay bar close here so execution is tied to the scenario,
    /// not to live quotes.
    pub reference_price_usd: f64,
    pub stop_loss_pct: Option<f32>,
    pub take_profit_pct: Option<f32>,
    /// Echoed to the broker as `client_order_id`. Brokers dedupe on this so
    /// duplicate retries collapse to a single fill.
    pub idempotency_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderConfirmation {
    pub broker_order_id: String,
    pub fill_price: Option<f64>,
    pub fill_size: f64,
    pub fee: Option<f64>,
}

/// Coarse classification of a broker-side failure. The shared
/// surface lets the eval executor + future live daemon pick the
/// same recover-vs-terminate boundary without re-encoding the broker
/// error string in each call site. Added by
/// `agent-error-feedback-self-healing`: recoverable variants are
/// round-tripped to the agent as a tool-result so the model can
/// self-heal (re-decide with smaller size, flat, close-first);
/// fatal variants continue to terminate the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrokerErrorClass {
    // Recoverable — the agent gets a structured tool-result and the
    // run continues.
    InsufficientFunds,
    RateLimited,
    PositionAlreadyOpen,
    MinOrderSize,
    MarketClosed,
    // Fatal — the run terminates with the existing error path.
    AuthFailed,
    NetworkUnreachable,
    UnsupportedAsset,
    /// Catch-all. Treated as fatal by `is_recoverable()` because
    /// blindly retrying on an un-known class can mask real provider
    /// outages or contract-shape regressions.
    Unknown,
}

impl BrokerErrorClass {
    /// Returns `true` when the eval executor should record the error
    /// and continue the run with a self-healing follow-up turn rather
    /// than terminating.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::InsufficientFunds
                | Self::RateLimited
                | Self::PositionAlreadyOpen
                | Self::MinOrderSize
                | Self::MarketClosed
        )
    }

    /// Compact snake-case tag for trace dock + decision-row error
    /// columns. Matches the wire shape from
    /// `xvision_observability::BrokerCallFinishedEvent.error_class`
    /// so downstream dashboards can group across the two surfaces
    /// without translation.
    pub fn as_tag(&self) -> &'static str {
        match self {
            Self::InsufficientFunds => "broker_insufficient_funds",
            Self::RateLimited => "broker_rate_limited",
            Self::PositionAlreadyOpen => "broker_position_already_open",
            Self::MinOrderSize => "broker_min_order_size",
            Self::MarketClosed => "broker_market_closed",
            Self::AuthFailed => "broker_auth",
            Self::NetworkUnreachable => "broker_network_unreachable",
            Self::UnsupportedAsset => "broker_unsupported",
            Self::Unknown => "broker_rejected",
        }
    }
}

/// Structured detail surfaced to the agent on a recoverable error.
/// `requested` / `available` / `asset` are best-effort extracted from
/// the broker message; callers should not assume all three are
/// populated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerErrorDetail {
    pub class: BrokerErrorClass,
    pub message: String,
    pub requested: Option<f64>,
    pub available: Option<f64>,
    pub asset: Option<String>,
}

/// Map a broker error message to a typed [`BrokerErrorClass`]. Looks
/// at the full chain of `with_context` wrappers (callers pass
/// `format!("{e:#}")`) so it matches whether the offending phrase is
/// at the root cause or the outermost wrapper.
///
/// The classifier mirrors patterns documented in
/// `team/contracts/alpaca-paper-crypto-submit.md` and the operator
/// round-2/3 intakes. Adding a new pattern requires a test in
/// `broker_error_classifier_tests` below — the named set the
/// `agent-error-feedback-self-healing` contract calls out is covered.
pub fn classify_broker_error_message(msg: &str) -> BrokerErrorClass {
    let lower = msg.to_ascii_lowercase();
    if lower.contains("insufficient buying power")
        || lower.contains("insufficient balance")
        || lower.contains("insufficient_funds")
    {
        BrokerErrorClass::InsufficientFunds
    } else if lower.contains("rate limit") || lower.contains("rate_limited") {
        BrokerErrorClass::RateLimited
    } else if lower.contains("position already")
        || lower.contains("already open")
        || lower.contains("position_already_open")
    {
        BrokerErrorClass::PositionAlreadyOpen
    } else if lower.contains("min order size")
        || lower.contains("minimum order")
        || lower.contains("min_order_size")
        // Alpaca crypto-paper returns this exact phrase for the
        // $10 notional-minimum gate (round-4 live finding, 2026-05-18):
        //   "HTTP status 403 Forbidden: cost basis must be >=
        //    minimal amount of order 10".
        // It ALSO contains "Forbidden", so without this branch the
        // classifier falls through to AuthFailed (fatal) and the run
        // terminates — even though the right behaviour is to round-trip
        // the error to the agent as recoverable so it can re-decide
        // with a larger size.
        || lower.contains("minimal amount of order")
        || lower.contains("cost basis must be")
        || lower.contains("order amount is too small")
        || lower.contains("notional too small")
        || lower.contains("below minimum notional")
    {
        BrokerErrorClass::MinOrderSize
    } else if lower.contains("market closed")
        || lower.contains("market_closed")
        || lower.contains("outside market hours")
    {
        BrokerErrorClass::MarketClosed
    } else if lower.contains("unauthorized")
        || lower.contains("invalid_api_key")
        || lower.contains("auth_failed")
        || lower.contains("forbidden")
    {
        BrokerErrorClass::AuthFailed
    } else if lower.contains("network unreachable")
        || lower.contains("connection refused")
        || lower.contains("dns error")
        || lower.contains("timed out")
        || lower.contains("timeout")
    {
        BrokerErrorClass::NetworkUnreachable
    } else if lower.contains("not shortable")
        || lower.contains("not permitted")
        || (lower.contains("bracket") && lower.contains("not supported"))
        || lower.contains("unsupported asset")
    {
        BrokerErrorClass::UnsupportedAsset
    } else {
        BrokerErrorClass::Unknown
    }
}

/// Best-effort extraction of `(requested, available)` numbers from a
/// broker error message such as
/// `"insufficient balance for USD (requested: 2487.87, available: 1807.38)"`.
/// Returns `(None, None)` when no decimal pattern matches. Lives next
/// to the classifier so the executor can build a [`BrokerErrorDetail`]
/// without parsing the same string twice.
pub fn extract_requested_available(msg: &str) -> (Option<f64>, Option<f64>) {
    fn parse_after(label: &str, msg: &str) -> Option<f64> {
        let i = msg.to_ascii_lowercase().find(label)?;
        let rest = &msg[i + label.len()..];
        let trimmed = rest.trim_start_matches(|c: char| c == ':' || c.is_whitespace());
        // Capture decimal digits + dot + optional digits up to the
        // first non-numeric char. Avoids pulling in a regex crate.
        let mut end = 0usize;
        for (idx, c) in trimmed.char_indices() {
            if c.is_ascii_digit() || c == '.' {
                end = idx + c.len_utf8();
            } else {
                break;
            }
        }
        if end == 0 {
            return None;
        }
        trimmed[..end].parse::<f64>().ok()
    }
    (parse_after("requested", msg), parse_after("available", msg))
}

#[cfg(test)]
mod broker_error_classifier_tests {
    use super::*;

    #[test]
    fn insufficient_funds_variants() {
        assert_eq!(
            classify_broker_error_message(
                "alpaca create_order: rejected by venue: insufficient balance for USD"
            ),
            BrokerErrorClass::InsufficientFunds,
        );
        assert!(BrokerErrorClass::InsufficientFunds.is_recoverable());
    }

    #[test]
    fn auth_failed_is_fatal() {
        let class = classify_broker_error_message("alpaca create_order: 401 Unauthorized: invalid_api_key");
        assert_eq!(class, BrokerErrorClass::AuthFailed);
        assert!(!class.is_recoverable());
    }

    #[test]
    fn rate_limited_recoverable() {
        let class = classify_broker_error_message("HTTP 429: rate limit exceeded");
        assert_eq!(class, BrokerErrorClass::RateLimited);
        assert!(class.is_recoverable());
    }

    #[test]
    fn network_unreachable_fatal() {
        let class = classify_broker_error_message("connection refused after retries");
        assert_eq!(class, BrokerErrorClass::NetworkUnreachable);
        assert!(!class.is_recoverable());
    }

    #[test]
    fn unsupported_asset_fatal() {
        let class = classify_broker_error_message("alpaca: bracket not supported on crypto");
        assert_eq!(class, BrokerErrorClass::UnsupportedAsset);
        assert!(!class.is_recoverable());
    }

    #[test]
    fn position_already_open_recoverable() {
        let class = classify_broker_error_message("rejected: position already open on BTC/USD");
        assert_eq!(class, BrokerErrorClass::PositionAlreadyOpen);
        assert!(class.is_recoverable());
    }

    #[test]
    fn min_order_size_recoverable() {
        let class = classify_broker_error_message("rejected: minimum order size 0.001 BTC");
        assert_eq!(class, BrokerErrorClass::MinOrderSize);
        assert!(class.is_recoverable());
    }

    #[test]
    fn alpaca_cost_basis_minimum_classified_as_min_order_size_not_auth() {
        // Round-4 live finding (2026-05-18): Alpaca crypto-paper
        // rejects sub-$10 orders with:
        //   "HTTP status 403 Forbidden: cost basis must be >= minimal
        //    amount of order 10".
        // The message contains "Forbidden" which previously short-
        // circuited to AuthFailed (fatal). Operators saw `[broker_auth]`
        // in the run-error column and the run terminated instead of
        // round-tripping through agent-error-feedback-self-healing.
        let msg = "alpaca create_order: rejected by venue: HTTP status 403 Forbidden: \
                   cost basis must be >= minimal amount of order 10";
        let class = classify_broker_error_message(msg);
        assert_eq!(
            class,
            BrokerErrorClass::MinOrderSize,
            "Alpaca cost-basis-minimum must be MinOrderSize, not AuthFailed"
        );
        assert!(class.is_recoverable(), "MinOrderSize is recoverable");
    }

    #[test]
    fn alpaca_genuine_auth_403_still_fatal() {
        // Guardrail: a 403 that DOESN'T carry the min-notional phrase
        // (e.g. an expired key) must still classify as AuthFailed so
        // the run terminates with a clear error class.
        let msg = "alpaca create_order: HTTP status 403 Forbidden: invalid_api_key";
        let class = classify_broker_error_message(msg);
        assert_eq!(class, BrokerErrorClass::AuthFailed);
        assert!(!class.is_recoverable());
    }

    #[test]
    fn market_closed_recoverable() {
        let class = classify_broker_error_message("rejected: market closed");
        assert_eq!(class, BrokerErrorClass::MarketClosed);
        assert!(class.is_recoverable());
    }

    #[test]
    fn unknown_defaults_to_fatal() {
        let class = classify_broker_error_message("some weird new venue message");
        assert_eq!(class, BrokerErrorClass::Unknown);
        assert!(!class.is_recoverable());
    }

    #[test]
    fn extract_requested_available_from_operator_repro() {
        // Operator round-3 repro:
        //   broker_insufficient_funds … run_id=01KRWHY535HCYE14DFPWC7QEGG …
        //   "insufficient balance for USD (requested: 2487.87, available: 1807.38)"
        let msg = "alpaca create_order: rejected by venue: HTTP status 403 Forbidden: \
                   insufficient balance for USD (requested: 2487.87, available: 1807.38)";
        let (requested, available) = extract_requested_available(msg);
        assert_eq!(requested, Some(2487.87));
        assert_eq!(available, Some(1807.38));
    }

    #[test]
    fn extract_requested_available_returns_none_when_missing() {
        let msg = "alpaca create_order: 401 Unauthorized";
        let (requested, available) = extract_requested_available(msg);
        assert_eq!(requested, None);
        assert_eq!(available, None);
    }
}

// ── Trait ────────────────────────────────────────────────────────────────────

#[async_trait]
pub trait BrokerSurface: Send + Sync {
    /// Submit an order. Polls the broker until the order reaches a terminal
    /// state (filled / canceled / rejected) before returning.
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation>;

    /// Current position size in base-asset units (signed: positive = long,
    /// negative = short). Returns 0.0 when no position is open.
    async fn position(&self, asset: &str) -> anyhow::Result<f64>;

    /// Account equity in USD (cash + marked-to-market value of open positions).
    /// Use this for equity curves and metrics — NOT for sizing new orders.
    async fn balance(&self) -> anyhow::Result<f64>;

    /// USD available to fund a new order for `asset`. For Alpaca paper crypto
    /// this returns settled cash — the constraint Alpaca actually validates
    /// against when it rejects with 403 "insufficient balance for USD". For
    /// margin equities accounts it returns `buying_power`. Defaults to
    /// `balance` so non-cash-aware impls (mocks, stubs) keep compiling, but
    /// any sizing call site MUST use this instead of `balance`.
    async fn buying_power(&self, _asset: &str) -> anyhow::Result<f64> {
        self.balance().await
    }

    /// Human-readable venue identifier for this surface
    /// (e.g. `alpaca-paper`, `byreal`, `orderly`, `bybit`). Stamped onto
    /// the live trace (`broker_call_started.venue`, `order_signed.venue`,
    /// `venue_account_snapshot.venue`) so operators can tell paper from
    /// real fills and one venue from another at a glance. Defaults to the
    /// generic `"live"` so mocks/stubs keep compiling; concrete impls
    /// override with their real venue. Read-only; never carries secrets.
    fn venue(&self) -> &str {
        "live"
    }

    /// How this surface authenticates an order submit
    /// (e.g. `api-key`, `cli`, `ed25519`). Surfaced on the live trace's
    /// `order_signed` event as the `scheme` field so operators can see
    /// the signing path without ever exposing the key/secret/signature
    /// itself. Defaults to the generic `"broker"`; concrete impls
    /// override. Read-only; never carries secrets.
    fn signing_scheme(&self) -> &str {
        "broker"
    }

    /// Whether this surface trades directional perpetual futures, where
    /// funding and liquidation risk apply. Default `false` (spot); the
    /// directional-perps adapters (Hyperliquid/byreal, Orderly, Bybit linear)
    /// override to `true`. Gates the engine's perps risk vetoes so they
    /// stay inert on spot venues. Read-only.
    fn is_perp_venue(&self) -> bool {
        false
    }
}

// ── AlpacaPaperSurface ───────────────────────────────────────────────────────

/// Wraps the existing `alpaca::AlpacaApi` (paper environment) behind the
/// unified `BrokerSurface`. Used by eval paper mode and any caller that
/// wants Alpaca paper without going through the existing `RiskDecision`-
/// shaped `Executor` trait.
pub struct AlpacaPaperSurface {
    api: Arc<dyn AlpacaApi>,
}

impl AlpacaPaperSurface {
    /// Build from any `AlpacaApi` impl. Used by tests with mocks.
    pub fn with_api(api: Arc<dyn AlpacaApi>) -> Self {
        Self { api }
    }

    /// Build from environment variables (`APCA_API_KEY_ID`,
    /// `APCA_API_SECRET_KEY`, `APCA_API_BASE_URL`). Falls back to Alpaca
    /// paper-trading URL if `APCA_API_BASE_URL` is absent. Production entry
    /// point for eval paper mode.
    pub fn from_env() -> anyhow::Result<Self> {
        let api_info =
            apca::ApiInfo::from_env().map_err(|e| anyhow::anyhow!("alpaca ApiInfo::from_env: {e}"))?;
        let client = apca::Client::new(api_info);
        Ok(Self {
            api: Arc::new(ApacClientApi::new(client)),
        })
    }

    /// Build from explicit credentials. Useful for tests that hit the real
    /// paper API without relying on the process environment.
    pub fn from_credentials(key_id: &str, secret: &str, base_url: &str) -> anyhow::Result<Self> {
        let api_info = apca::ApiInfo::from_parts(base_url, key_id, secret)
            .map_err(|e| anyhow::anyhow!("alpaca ApiInfo::from_parts: {e}"))?;
        let client = apca::Client::new(api_info);
        Ok(Self {
            api: Arc::new(ApacClientApi::new(client)),
        })
    }
}

const ALPACA_FILL_POLL_MAX: u32 = 5;
const ALPACA_FILL_POLL_DELAY_MS: u64 = 200;

#[async_trait]
impl BrokerSurface for AlpacaPaperSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        // 1. Use the caller-selected reference price for size→notional
        //    conversion + bracket leg derivation. Paper evals source this
        //    from the historical replay bar so a flat Alpaca account does
        //    not force a live quote or a hard-coded BTC price.
        let reference_price = if req.reference_price_usd > 0.0 && req.reference_price_usd.is_finite() {
            req.reference_price_usd
        } else {
            anyhow::bail!(
                "alpaca paper order missing positive reference_price_usd for {}",
                req.asset
            );
        };

        // 2. Crypto pre-flight. Alpaca's crypto API only accepts simple
        //    market/limit orders and is long-only; selling from flat is
        //    rejected by the server and selling more than the open long
        //    would net into a short on fill. We refuse both cases here
        //    with a classifier-friendly message ("broker_unsupported")
        //    so the eval executor surfaces a clean class tag if it ever
        //    reaches the surface. The eval paper executor sizes
        //    crypto sells against the open long before this point in
        //    normal operation; this guard is the hard backstop.
        let asset_is_crypto = is_alpaca_crypto(&req.asset);
        if asset_is_crypto && matches!(req.side, Side::Sell) {
            let current_position = self
                .api
                .get_position(&req.asset)
                .await
                .map_err(|e| anyhow::anyhow!("alpaca get_position: {e}"))?
                .map(|p| if p.side == "long" { p.qty } else { -p.qty })
                .unwrap_or(0.0);
            if current_position <= 0.0 {
                anyhow::bail!(
                    "alpaca crypto broker_unsupported: short_open is not supported for {} (asset is not shortable on Alpaca crypto)",
                    req.asset
                );
            }
            if req.size > current_position {
                anyhow::bail!(
                    "alpaca crypto broker_unsupported: sell size {} exceeds open long position {} for {} (would net into a short, which Alpaca crypto does not support)",
                    req.size,
                    current_position,
                    req.asset
                );
            }
        }

        let notional = req.size * reference_price;
        tracing::debug!(
            target: "xvision::alpaca",
            asset = %req.asset,
            side = ?req.side,
            size = req.size,
            reference_price_usd = reference_price,
            reference_price_source = "eval_bar",
            notional,
            "alpaca paper order notional resolved"
        );

        // 3. Bracket legs — only for non-crypto. Alpaca's crypto API rejects
        //    `Class::Bracket` outright; submit a simple market order instead.
        //    Strategies that wire stop/tp percentages still see them at
        //    decision time, but they are not enforced server-side for
        //    crypto (a follow-up track can model client-side TP/SL
        //    tracking if needed).
        let (take_profit_price, stop_loss_price) = if asset_is_crypto {
            if req.take_profit_pct.is_some() || req.stop_loss_pct.is_some() {
                tracing::debug!(
                    target: "xvision::alpaca",
                    asset = %req.asset,
                    "alpaca crypto does not support bracket orders; dropping take_profit / stop_loss legs"
                );
            }
            (None, None)
        } else {
            match (req.take_profit_pct, req.stop_loss_pct) {
                (Some(tp_pct), Some(sl_pct)) => match req.side {
                    Side::Buy => (
                        Some(reference_price * (1.0 + tp_pct as f64 / 100.0)),
                        Some(reference_price * (1.0 - sl_pct as f64 / 100.0)),
                    ),
                    Side::Sell => (
                        Some(reference_price * (1.0 - tp_pct as f64 / 100.0)),
                        Some(reference_price * (1.0 + sl_pct as f64 / 100.0)),
                    ),
                },
                _ => (None, None),
            }
        };

        // 3. Submit via existing AlpacaApi.
        let apac_req = ApacOrderRequest {
            symbol: req.asset.clone(),
            notional,
            side: match req.side {
                Side::Buy => ApacSide::Buy,
                Side::Sell => ApacSide::Sell,
            },
            take_profit_price,
            stop_loss_price,
            client_order_id: req.idempotency_key.clone(),
        };

        let order = self
            .api
            .create_order(apac_req)
            .await
            .map_err(|e| anyhow::anyhow!("alpaca create_order: {e}"))?;
        let order_id = order.id.clone();

        // 4. Poll for terminal state.
        let filled = self.await_fill(&order_id).await?;

        Ok(OrderConfirmation {
            broker_order_id: filled.id,
            fill_price: filled.avg_fill_price,
            fill_size: filled.filled_qty,
            fee: None, // Alpaca paper has no fee model in v1
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let pos = self
            .api
            .get_position(asset)
            .await
            .map_err(|e| anyhow::anyhow!("alpaca get_position: {e}"))?;
        Ok(pos
            .map(|p| if p.side == "long" { p.qty } else { -p.qty })
            .unwrap_or(0.0))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        let acct = self
            .api
            .get_account()
            .await
            .map_err(|e| anyhow::anyhow!("alpaca get_account: {e}"))?;
        Ok(acct.equity)
    }

    async fn buying_power(&self, asset: &str) -> anyhow::Result<f64> {
        let acct = self
            .api
            .get_account()
            .await
            .map_err(|e| anyhow::anyhow!("alpaca get_account: {e}"))?;
        // Crypto symbols (e.g. "BTC/USD") are non-marginable: Alpaca validates
        // crypto buys against settled USD cash, not `buying_power` (which on a
        // margin account can include unsettled / margin allowance).
        // For equities, use `buying_power`.
        if is_crypto_symbol(asset) {
            Ok(acct.cash)
        } else {
            Ok(acct.buying_power)
        }
    }

    fn venue(&self) -> &str {
        "alpaca-paper"
    }

    fn signing_scheme(&self) -> &str {
        "api-key"
    }
}

/// Alpaca crypto symbols are pair-formatted (`BTC/USD`, `ETH/USD`). Equities
/// are bare tickers (`AAPL`). Used to pick the right buying-power field.
fn is_crypto_symbol(asset: &str) -> bool {
    asset.contains('/')
}

impl AlpacaPaperSurface {
    async fn await_fill(&self, order_id: &str) -> anyhow::Result<crate::alpaca::AlpacaOrder> {
        for _ in 0..ALPACA_FILL_POLL_MAX {
            let order = self
                .api
                .get_order(order_id)
                .await
                .map_err(|e| anyhow::anyhow!("alpaca get_order: {e}"))?;
            if order.is_terminal() {
                if order.is_rejected() {
                    return Err(anyhow::anyhow!("alpaca order {order_id} rejected"));
                }
                return Ok(order);
            }
            tokio::time::sleep(Duration::from_millis(ALPACA_FILL_POLL_DELAY_MS)).await;
        }
        Err(anyhow::anyhow!(
            "alpaca order {order_id} did not fill within {} polls",
            ALPACA_FILL_POLL_MAX
        ))
    }
}

// ── AlpacaLiveSurface — stubbed for v1 ───────────────────────────────────────

/// Placeholder for Alpaca live trading. v1 test ships paper-only; this
/// type exists so `BrokerKind::AlpacaLive` can be matched without compile
/// errors. Activation requires a non-paper `APCA_API_BASE_URL` and explicit
/// operator opt-in — not in scope until post-v1.
pub struct AlpacaLiveSurface;

#[async_trait]
impl BrokerSurface for AlpacaLiveSurface {
    async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        Err(anyhow::anyhow!(
            "AlpacaLiveSurface is stubbed for v1. Use AlpacaPaperSurface for v1 test scope."
        ))
    }

    async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
        Err(anyhow::anyhow!("AlpacaLiveSurface stubbed"))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        Err(anyhow::anyhow!("AlpacaLiveSurface stubbed"))
    }
}

// ── OrderlyLiveSurface ───────────────────────────────────────────────────────

/// Orderly Network (perps) behind the unified `BrokerSurface`. Mirrors
/// `AlpacaPaperSurface` over `AlpacaApi`: thin order/position/balance calls
/// over the existing `OrderlyApi` trait, generic so tests inject a mock.
///
/// Order semantics:
/// - `req.size` is base-asset units, rounded to 6 dp before submission.
/// - Market entry with `client_order_id = req.idempotency_key` (Orderly
///   dedupes open orders on it).
/// - Best-effort SL/TP bracket via reduce-only algo orders derived from
///   `reference_price_usd` + `stop_loss_pct` / `take_profit_pct`. Bracket
///   failures never fail the entry — same policy as `OrderlyExecutor::submit`.
pub struct OrderlyLiveSurface<A: OrderlyApi = ReqwestOrderlyApi> {
    api: A,
}

impl OrderlyLiveSurface<ReqwestOrderlyApi> {
    /// Build from explicit credentials. `base_url` defaults to the Orderly
    /// mainnet EVM gateway when `None` — live-eval callers pass the testnet
    /// URL explicitly (the engine hard-requires it).
    pub fn connect(creds: OrderlyCredentials, base_url: Option<&str>) -> anyhow::Result<Self> {
        let url = base_url.unwrap_or(ORDERLY_MAINNET_BASE).to_string();
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| anyhow::anyhow!("orderly reqwest client: {e}"))?;
        Ok(Self {
            api: ReqwestOrderlyApi::new(http, url, creds),
        })
    }

    /// Build from environment variables (`ORDERLY_KEY`, `ORDERLY_SECRET`,
    /// `ORDERLY_ACCOUNT_ID`, optional `ORDERLY_BASE_URL`) — same contract as
    /// `OrderlyExecutor::from_env`.
    pub fn from_env() -> anyhow::Result<Self> {
        let key =
            std::env::var("ORDERLY_KEY").map_err(|_| anyhow::anyhow!("auth_failed: ORDERLY_KEY not set"))?;
        let secret = std::env::var("ORDERLY_SECRET")
            .map_err(|_| anyhow::anyhow!("auth_failed: ORDERLY_SECRET not set"))?;
        let account_id = std::env::var("ORDERLY_ACCOUNT_ID")
            .map_err(|_| anyhow::anyhow!("auth_failed: ORDERLY_ACCOUNT_ID not set"))?;
        let base_url = std::env::var("ORDERLY_BASE_URL").ok();
        Self::connect(
            OrderlyCredentials {
                orderly_key: key,
                orderly_secret: secret,
                orderly_account_id: account_id,
            },
            base_url.as_deref(),
        )
    }
}

impl<A: OrderlyApi> OrderlyLiveSurface<A> {
    /// Build from any `OrderlyApi` impl. Used by tests with mocks.
    pub fn with_api(api: A) -> Self {
        Self { api }
    }

    /// Resolve a free-text asset (`"BTC"`, `"BTC/USD"`, …) to its Orderly
    /// perp symbol. Failures carry "unsupported asset" so
    /// `classify_broker_error_message` lands on `UnsupportedAsset`.
    fn resolve_symbol(asset: &str) -> anyhow::Result<String> {
        let sym = AssetSymbol::from_str(asset)
            .map_err(|e| anyhow::anyhow!("orderly unsupported asset '{asset}': {e}"))?;
        orderly_symbol_for(sym).map_err(|e| anyhow::anyhow!("orderly unsupported asset '{asset}': {e}"))
    }

    async fn await_fill(&self, order_id: u64) -> anyhow::Result<OrderlyOrder> {
        const MAX_POLLS: u32 = 5;
        const POLL_DELAY_MS: u64 = 200;
        for _ in 0..MAX_POLLS {
            let order = self
                .api
                .get_order(order_id)
                .await
                .map_err(|e| anyhow::anyhow!("orderly get_order: {e}"))?;
            if order.is_terminal() {
                if order.is_unfilled_terminal() {
                    anyhow::bail!(
                        "orderly order {order_id} rejected: terminated unfilled ({}) — venue could not match it",
                        order.status
                    );
                }
                return Ok(order);
            }
            tokio::time::sleep(Duration::from_millis(POLL_DELAY_MS)).await;
        }
        anyhow::bail!("orderly order {order_id} did not fill within {MAX_POLLS} polls (timeout)")
    }
}

#[async_trait]
impl<A: OrderlyApi> BrokerSurface for OrderlyLiveSurface<A> {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let symbol = Self::resolve_symbol(&req.asset)?;

        // Align the base quantity to the market's step size (the venue
        // rejects misaligned quantities with -1104 "does not match the
        // step size"; caught live on testnet 2026-06-11) and enforce the
        // per-order minimum.
        let meta = self
            .api
            .get_symbol_meta(&symbol)
            .await
            .map_err(|e| anyhow::anyhow!("orderly get_symbol_meta: {e}"))?;
        let qty = crate::orderly::round_to_tick(req.size, meta.base_tick);
        if !(qty > 0.0) || qty < meta.base_min {
            anyhow::bail!(
                "orderly order amount is too small: size {} for {} (base_min {}, step {})",
                req.size,
                req.asset,
                meta.base_min,
                meta.base_tick
            );
        }

        let side = match req.side {
            Side::Buy => OrderlyOrderSide::Buy,
            Side::Sell => OrderlyOrderSide::Sell,
        };

        // Entry market order; client_order_id = idempotency key.
        let entry = self
            .api
            .create_order(&symbol, side, qty, Some(req.idempotency_key.clone()), None)
            .await
            .map_err(|e| anyhow::anyhow!("orderly create_order: {e}"))?;

        let filled = self.await_fill(entry.order_id).await?;
        let fill_price = filled.average_executed_price;
        let fill_qty = filled.executed_quantity.unwrap_or(qty);

        // Best-effort SL/TP bracket via reduce-only algo orders. Trigger
        // prices derive from the caller's reference price (the live bar
        // close), matching the AlpacaPaperSurface bracket derivation; the
        // fill price is preferred when the venue reported one.
        let anchor = fill_price
            .filter(|p| *p > 0.0 && p.is_finite())
            .unwrap_or(req.reference_price_usd);
        if anchor > 0.0 && anchor.is_finite() {
            let close_side = match side {
                OrderlyOrderSide::Buy => OrderlyOrderSide::Sell,
                OrderlyOrderSide::Sell => OrderlyOrderSide::Buy,
            };
            let dir = match side {
                OrderlyOrderSide::Buy => 1.0,
                OrderlyOrderSide::Sell => -1.0,
            };
            if let Some(tp_pct) = req.take_profit_pct {
                let trigger = anchor * (1.0 + dir * tp_pct as f64 / 100.0);
                if let Err(e) = self
                    .api
                    .create_algo_order(
                        &symbol,
                        AlgoKind::TakeProfitMarket,
                        close_side,
                        fill_qty,
                        trigger,
                        Some(crate::orderly::venue_client_id("tp-", &req.idempotency_key)),
                        Some(true),
                    )
                    .await
                {
                    tracing::warn!(
                        target: "xvision::orderly",
                        asset = %req.asset,
                        "orderly take-profit algo order failed (entry stands): {e}"
                    );
                }
            }
            if let Some(sl_pct) = req.stop_loss_pct {
                let trigger = anchor * (1.0 - dir * sl_pct as f64 / 100.0);
                if let Err(e) = self
                    .api
                    .create_algo_order(
                        &symbol,
                        AlgoKind::StopMarket,
                        close_side,
                        fill_qty,
                        trigger,
                        Some(crate::orderly::venue_client_id("sl-", &req.idempotency_key)),
                        Some(true),
                    )
                    .await
                {
                    tracing::warn!(
                        target: "xvision::orderly",
                        asset = %req.asset,
                        "orderly stop-loss algo order failed (entry stands): {e}"
                    );
                }
            }
        }

        Ok(OrderConfirmation {
            broker_order_id: filled.order_id.to_string(),
            fill_price,
            fill_size: fill_qty,
            fee: None,
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let symbol = Self::resolve_symbol(asset)?;
        let positions = self
            .api
            .get_positions()
            .await
            .map_err(|e| anyhow::anyhow!("orderly get_positions: {e}"))?;
        Ok(positions
            .iter()
            .find(|p| p.symbol == symbol)
            .map(|p| p.position_qty)
            .unwrap_or(0.0))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        let account = self
            .api
            .get_account()
            .await
            .map_err(|e| anyhow::anyhow!("orderly get_account: {e}"))?;
        Ok(account.equity())
    }

    fn venue(&self) -> &str {
        "orderly"
    }

    fn signing_scheme(&self) -> &str {
        "ed25519"
    }

    fn is_perp_venue(&self) -> bool {
        true
    }
}

// ── MockBrokerSurface ────────────────────────────────────────────────────────

/// Deterministic in-memory `BrokerSurface` for downstream tests
/// (eval engine, wizard, etc.). Records every submission, fills at a fake
/// constant price (`70_000.0` if no override), tracks per-asset position,
/// and never hits the network.
///
/// Public so downstream crates can use it without re-implementing it.
pub struct MockBrokerSurface {
    state: Mutex<MockState>,
    fill_price: f64,
}

#[derive(Default)]
struct MockState {
    balance: f64,
    submitted: Vec<OrderRequest>,
    positions: std::collections::HashMap<String, f64>,
}

impl MockBrokerSurface {
    /// New mock seeded with `balance` USD and zero positions. Default fill
    /// price is 70_000.0; override with [`with_fill_price`].
    pub fn new(balance: f64) -> Self {
        Self {
            state: Mutex::new(MockState {
                balance,
                ..Default::default()
            }),
            fill_price: 70_000.0,
        }
    }

    pub fn with_fill_price(mut self, fill_price: f64) -> Self {
        self.fill_price = fill_price;
        self
    }

    /// Returns a clone of every order ever submitted to this mock.
    pub fn submitted(&self) -> Vec<OrderRequest> {
        self.state.lock().unwrap().submitted.clone()
    }
}

#[async_trait]
impl BrokerSurface for MockBrokerSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let mut s = self.state.lock().unwrap();
        s.submitted.push(req.clone());

        let signed = match req.side {
            Side::Buy => req.size,
            Side::Sell => -req.size,
        };
        *s.positions.entry(req.asset.clone()).or_insert(0.0) += signed;

        Ok(OrderConfirmation {
            broker_order_id: format!("mock-{}", req.idempotency_key),
            fill_price: Some(req.reference_price_usd),
            fill_size: req.size,
            fee: None,
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let s = self.state.lock().unwrap();
        Ok(s.positions.get(asset).copied().unwrap_or(0.0))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        Ok(self.state.lock().unwrap().balance)
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn is_alpaca_crypto_accepts_whitelist_forms() {
        for ok in [
            "BTC", "BTC/USD", "BTCUSD", "btc", "btc/usd", "ETH/USD", "SOL/USD", "DOGE", "USDC/USD",
        ] {
            assert!(is_alpaca_crypto(ok), "{ok} must be classified as crypto");
        }
    }

    #[test]
    fn is_alpaca_crypto_rejects_equities_and_unknown_symbols() {
        for not_ok in ["", "AAPL", "TSLA", "SPY", "XRP", "XRP/USD"] {
            assert!(
                !is_alpaca_crypto(not_ok),
                "{not_ok} must NOT be classified as crypto"
            );
        }
    }
}

#[cfg(test)]
mod orderly_live_surface_tests {
    use super::*;
    use crate::executor::ExecutorError;
    use crate::orderly::{OrderlyAccount, OrderlyPosition, OrderlySymbolMeta};

    // ── Local mock OrderlyApi ────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    struct CreateCall {
        symbol: String,
        side: OrderlyOrderSide,
        quantity: f64,
        client_order_id: Option<String>,
    }

    #[derive(Debug, Clone)]
    struct AlgoCall {
        algo_type: AlgoKind,
        side: OrderlyOrderSide,
        quantity: f64,
        trigger_price: f64,
        client_order_id: Option<String>,
        reduce_only: Option<bool>,
    }

    #[derive(Default)]
    struct MockApi {
        account: Option<OrderlyAccount>,
        positions: Vec<OrderlyPosition>,
        create_result: Option<OrderlyOrder>,
        get_result: Option<OrderlyOrder>,
        create_err: Option<String>,
        algo_err: Option<String>,
        created: Mutex<Vec<CreateCall>>,
        algos: Mutex<Vec<AlgoCall>>,
    }

    #[async_trait]
    impl OrderlyApi for MockApi {
        async fn create_order(
            &self,
            symbol: &str,
            side: OrderlyOrderSide,
            quantity: f64,
            client_order_id: Option<String>,
            _reduce_only: Option<bool>,
        ) -> Result<OrderlyOrder, ExecutorError> {
            if let Some(msg) = &self.create_err {
                return Err(ExecutorError::Rejected(msg.clone()));
            }
            self.created.lock().unwrap().push(CreateCall {
                symbol: symbol.to_string(),
                side,
                quantity,
                client_order_id,
            });
            Ok(self.create_result.clone().expect("create_result fixture"))
        }

        async fn create_algo_order(
            &self,
            _symbol: &str,
            algo_type: AlgoKind,
            side: OrderlyOrderSide,
            quantity: f64,
            trigger_price: f64,
            client_order_id: Option<String>,
            reduce_only: Option<bool>,
        ) -> Result<u64, ExecutorError> {
            if let Some(msg) = &self.algo_err {
                return Err(ExecutorError::Rejected(msg.clone()));
            }
            self.algos.lock().unwrap().push(AlgoCall {
                algo_type,
                side,
                quantity,
                trigger_price,
                client_order_id,
                reduce_only,
            });
            Ok(1)
        }

        async fn get_order(&self, _order_id: u64) -> Result<OrderlyOrder, ExecutorError> {
            Ok(self.get_result.clone().expect("get_result fixture"))
        }

        async fn get_account(&self) -> Result<OrderlyAccount, ExecutorError> {
            Ok(self.account.clone().expect("account fixture"))
        }

        async fn get_positions(&self) -> Result<Vec<OrderlyPosition>, ExecutorError> {
            Ok(self.positions.clone())
        }

        async fn get_mark_price(&self, _symbol: &str) -> Result<f64, ExecutorError> {
            Ok(50_000.0)
        }

        async fn get_symbol_meta(&self, _symbol: &str) -> Result<OrderlySymbolMeta, ExecutorError> {
            Ok(OrderlySymbolMeta {
                base_tick: 0.00001,
                base_min: 0.00001,
            })
        }
    }

    fn filled_order(id: u64, qty: f64, price: f64) -> OrderlyOrder {
        OrderlyOrder {
            order_id: id,
            client_order_id: None,
            status: "FILLED".into(),
            executed_quantity: Some(qty),
            average_executed_price: Some(price),
        }
    }

    fn btc_pos(qty: f64) -> OrderlyPosition {
        OrderlyPosition {
            symbol: "PERP_BTC_USDC".into(),
            position_qty: qty,
            average_open_price: 70_000.0,
            mark_price: 71_000.0,
            unsettled_pnl: 50.0,
        }
    }

    fn buy_req(size: f64) -> OrderRequest {
        OrderRequest {
            asset: "BTC/USD".into(),
            side: Side::Buy,
            size,
            reference_price_usd: 70_000.0,
            stop_loss_pct: Some(2.0),
            take_profit_pct: Some(5.0),
            idempotency_key: "cycle-abc".into(),
        }
    }

    #[tokio::test]
    async fn submit_order_places_market_entry_with_idempotency_key_and_brackets() {
        let api = MockApi {
            create_result: Some(filled_order(42, 0.05, 70_100.0)),
            get_result: Some(filled_order(42, 0.05, 70_100.0)),
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);

        let conf = surface
            .submit_order(buy_req(0.05))
            .await
            .expect("submit must succeed");

        assert_eq!(conf.broker_order_id, "42");
        assert_eq!(conf.fill_price, Some(70_100.0));
        assert_eq!(conf.fill_size, 0.05);
        assert_eq!(conf.fee, None);

        let created = surface.api.created.lock().unwrap().clone();
        assert_eq!(created.len(), 1);
        assert_eq!(created[0].symbol, "PERP_BTC_USDC");
        assert_eq!(created[0].side, OrderlyOrderSide::Buy);
        assert_eq!(created[0].quantity, 0.05);
        assert_eq!(created[0].client_order_id.as_deref(), Some("cycle-abc"));

        // Brackets: reduce-only TP above fill, SL below fill, close side Sell.
        let algos = surface.api.algos.lock().unwrap().clone();
        assert_eq!(algos.len(), 2, "TP + SL algo orders must be placed");
        let tp = algos
            .iter()
            .find(|a| matches!(a.algo_type, AlgoKind::TakeProfitMarket))
            .expect("TP algo order");
        let sl = algos
            .iter()
            .find(|a| matches!(a.algo_type, AlgoKind::StopMarket))
            .expect("SL algo order");
        assert!(tp.trigger_price > 70_100.0, "TP above fill for long");
        assert!(sl.trigger_price < 70_100.0, "SL below fill for long");
        for leg in [tp, sl] {
            assert_eq!(leg.side, OrderlyOrderSide::Sell);
            assert_eq!(leg.reduce_only, Some(true));
            assert_eq!(leg.quantity, 0.05);
        }
        assert_eq!(tp.client_order_id.as_deref(), Some("tp-cycleabc"));
        assert_eq!(sl.client_order_id.as_deref(), Some("sl-cycleabc"));
    }

    #[tokio::test]
    async fn submit_order_bracket_failure_does_not_fail_entry() {
        let api = MockApi {
            create_result: Some(filled_order(7, 0.1, 70_000.0)),
            get_result: Some(filled_order(7, 0.1, 70_000.0)),
            algo_err: Some("algo not supported".into()),
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);

        let conf = surface
            .submit_order(buy_req(0.1))
            .await
            .expect("entry must survive bracket failure");
        assert_eq!(conf.broker_order_id, "7");
    }

    #[tokio::test]
    async fn submit_order_aligns_size_to_market_step() {
        let api = MockApi {
            create_result: Some(filled_order(9, 0.12345, 70_000.0)),
            get_result: Some(filled_order(9, 0.12345, 70_000.0)),
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);

        surface
            .submit_order(buy_req(0.123456789))
            .await
            .expect("submit must succeed");

        // Mock symbol meta serves base_tick = 0.00001 — qty must be
        // rounded DOWN onto the step grid (venue rejects misaligned
        // quantities with -1104), never up past the decided size.
        let created = surface.api.created.lock().unwrap().clone();
        assert!((created[0].quantity - 0.12345).abs() < 1e-12);
    }

    #[tokio::test]
    async fn submit_order_rejected_status_maps_to_error() {
        let mut rejected = filled_order(11, 0.0, 0.0);
        rejected.status = "REJECTED".into();
        let api = MockApi {
            create_result: Some(rejected.clone()),
            get_result: Some(rejected),
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);

        let err = surface
            .submit_order(buy_req(0.05))
            .await
            .expect_err("rejected order must error");
        assert!(err.to_string().contains("rejected"), "got: {err:#}");
    }

    #[tokio::test]
    async fn submit_order_venue_rejection_text_classifies_recoverable() {
        // The venue body text must flow through anyhow so
        // classify_broker_error_message lands on the right class.
        let api = MockApi {
            create_err: Some("insufficient balance for USDC".into()),
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);

        let err = surface
            .submit_order(buy_req(0.05))
            .await
            .expect_err("create_order error must propagate");
        let class = classify_broker_error_message(&format!("{err:#}"));
        assert_eq!(class, BrokerErrorClass::InsufficientFunds);
    }

    #[tokio::test]
    async fn submit_order_unparseable_asset_classifies_unsupported() {
        let surface = OrderlyLiveSurface::with_api(MockApi::default());
        let mut req = buy_req(0.05);
        req.asset = "not a symbol!!".into();

        let err = surface
            .submit_order(req)
            .await
            .expect_err("bad asset must error before any venue call");
        let class = classify_broker_error_message(&format!("{err:#}"));
        assert_eq!(class, BrokerErrorClass::UnsupportedAsset);
        assert!(surface.api.created.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn surface_reports_orderly_ed25519_venue_identity() {
        // WS-4: orderly signs every request with an ed25519 keypair, so
        // the trace must stamp venue=orderly / scheme=ed25519 — not the
        // generic "live"/"broker" defaults.
        let surface = OrderlyLiveSurface::with_api(MockApi::default());
        assert_eq!(surface.venue(), "orderly");
        assert_eq!(surface.signing_scheme(), "ed25519");
    }

    #[tokio::test]
    async fn submit_order_zero_size_is_min_order_size() {
        let surface = OrderlyLiveSurface::with_api(MockApi::default());
        let err = surface
            .submit_order(buy_req(0.0))
            .await
            .expect_err("zero size must error");
        let class = classify_broker_error_message(&format!("{err:#}"));
        assert_eq!(class, BrokerErrorClass::MinOrderSize);
    }

    #[tokio::test]
    async fn position_returns_signed_qty_or_zero() {
        let api = MockApi {
            positions: vec![btc_pos(-0.25)],
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);

        assert_eq!(surface.position("BTC/USD").await.unwrap(), -0.25);
        assert_eq!(surface.position("ETH/USD").await.unwrap(), 0.0);
    }

    #[tokio::test]
    async fn balance_returns_account_equity() {
        let api = MockApi {
            account: Some(OrderlyAccount {
                usdc_holding: 1_000.0,
                unrealized_pnl: -50.0,
            }),
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);
        assert_eq!(surface.balance().await.unwrap(), 950.0);
    }

    #[test]
    fn orderly_live_surface_is_perp_venue() {
        let api = MockApi {
            create_result: Some(filled_order(42, 0.05, 70_100.0)),
            get_result: Some(filled_order(42, 0.05, 70_100.0)),
            ..Default::default()
        };
        let surface = OrderlyLiveSurface::with_api(api);
        assert!(surface.is_perp_venue(), "Orderly is a directional-perps venue");
    }
}

// ── WS-4 venue identity (venue / signing_scheme) ─────────────────────────────

#[cfg(test)]
mod venue_identity_tests {
    use super::*;
    use crate::alpaca::{AlpacaAccount, AlpacaOrder, AlpacaPosition};
    use crate::executor::ExecutorError;

    /// A bare `BrokerSurface` impl that overrides nothing must keep
    /// compiling and inherit the conservative defaults. This pins the
    /// "defaults keep all mocks compiling" contract for WS-4.
    struct DefaultsBroker;

    #[async_trait]
    impl BrokerSurface for DefaultsBroker {
        async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
            anyhow::bail!("unused")
        }
        async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
            Ok(0.0)
        }
        async fn balance(&self) -> anyhow::Result<f64> {
            Ok(0.0)
        }
    }

    struct StubAlpacaApi;

    #[async_trait]
    impl AlpacaApi for StubAlpacaApi {
        async fn create_order(&self, _req: ApacOrderRequest) -> Result<AlpacaOrder, ExecutorError> {
            Err(ExecutorError::Internal("unused stub".into()))
        }

        async fn get_order(&self, _order_id: &str) -> Result<AlpacaOrder, ExecutorError> {
            Err(ExecutorError::Internal("unused stub".into()))
        }

        async fn get_account(&self) -> Result<AlpacaAccount, ExecutorError> {
            Err(ExecutorError::Internal("unused stub".into()))
        }

        async fn list_positions(&self) -> Result<Vec<AlpacaPosition>, ExecutorError> {
            Err(ExecutorError::Internal("unused stub".into()))
        }

        async fn get_position(&self, _symbol: &str) -> Result<Option<AlpacaPosition>, ExecutorError> {
            Err(ExecutorError::Internal("unused stub".into()))
        }
    }

    #[test]
    fn trait_defaults_are_generic_broker() {
        let b = DefaultsBroker;
        assert_eq!(b.venue(), "live");
        assert_eq!(b.signing_scheme(), "broker");
    }

    #[test]
    fn alpaca_paper_overrides_venue_and_scheme() {
        let surface = AlpacaPaperSurface::with_api(Arc::new(StubAlpacaApi));
        assert_eq!(surface.venue(), "alpaca-paper");
        assert_eq!(surface.signing_scheme(), "api-key");
    }

    #[test]
    fn mock_broker_surface_uses_defaults() {
        let m = MockBrokerSurface::new(1_000.0);
        assert_eq!(m.venue(), "live");
        assert_eq!(m.signing_scheme(), "broker");
    }

    // ── WS-perps-risk: is_perp_venue gate ────────────────────────────────────

    #[test]
    fn default_surface_is_not_perp_venue() {
        let b = DefaultsBroker;
        assert!(
            !b.is_perp_venue(),
            "default BrokerSurface must be spot (is_perp_venue=false)"
        );
    }

    struct PerpTestSurface;

    #[async_trait]
    impl BrokerSurface for PerpTestSurface {
        async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
            anyhow::bail!("not exercised")
        }
        async fn position(&self, _asset: &str) -> anyhow::Result<f64> {
            Ok(0.0)
        }
        async fn balance(&self) -> anyhow::Result<f64> {
            Ok(0.0)
        }
        fn venue(&self) -> &str {
            "hyperliquid"
        }
        fn is_perp_venue(&self) -> bool {
            true
        }
    }

    #[test]
    fn perp_surface_reports_perp_venue() {
        assert!(PerpTestSurface.is_perp_venue());
    }
}
