//! Phase 6.3 — Orderly Network perpetuals executor (Mantle EVM gateway).
//!
//! Routes per `TraderDecision.asset` (post-F18 cascade, expanded
//! 2026-05-22 per `docs/superpowers/plans/2026-05-22-orderly-multi-asset-expansion.md`).
//! Supported markets: BTC, ETH, SOL, AVAX, DOGE, LINK — the Orderly
//! perp inventory on `https://api-evm.orderly.org`. Decisions targeting
//! any other `AssetSymbol` are rejected at the executor boundary with
//! `ExecutorError::NotActionable` naming the asset.
//!
//! # SDK dep conflict — why we use raw reqwest
//!
//! `orderly-connector-rs 0.4.15` pins `zeroize = "=1.3.0"` via its `solana-sdk`
//! transitive dependency. The workspace already uses `reqwest = 0.13` (in
//! several crates) which pulls `rustls 0.23` requiring
//! `zeroize >= 1.7`. These are irreconcilable: Cargo's resolver cannot
//! simultaneously satisfy `=1.3.0` and `>=1.7`.
//!
//! Resolution for v1: implement the five required Orderly REST endpoints as
//! direct signed `reqwest` HTTP calls. The signing scheme is identical to what
//! `orderly-connector-rs` uses (Ed25519 over `${ts}${METHOD}${path}${body}`,
//! base64-encoded, secret decoded from base58). Upgrading or forking
//! `orderly-connector-rs` to drop the Solana dependency is tracked as F19 in
//! FOLLOWUPS.md.
//!
//! # TLS
//! `reqwest 0.13` uses `rustls` via `rustls-tls` by default. No OpenSSL / native-tls.
//! `apca 0.30` uses `hyper-tls`. Both co-exist in the binary (different link units).
//! Strategy note for `decisions/strategy-choices.md` #3: a future dep audit
//! should consolidate to a single TLS stack; rustls is preferred.
//!
//! # Architecture
//! `OrderlyApi` is a thin async trait the executor calls. The real impl
//! (`ReqwestOrderlyApi`) makes HTTP calls; `MockOrderlyApi` in tests returns
//! hard-wired fixtures via mockito HTTP mocks. Because we control the HTTP
//! layer directly, every test assertion on request bodies and response parsing
//! can be driven from a mockito `Server`.
//!
//! # Idempotency
//! `client_order_id = td.cycle_id.to_string()` always (max 36 chars). Orderly
//! deduplicates open orders on `client_order_id`.
//!
//! # Onboarding
//! `from_env` reads `ORDERLY_KEY`, `ORDERLY_SECRET`, `ORDERLY_ACCOUNT_ID`, and
//! optionally `ORDERLY_BASE_URL`. Brokered onboarding (`xvn setup
//! --orderly-onboard`) is tracked as F5 in FOLLOWUPS.md.

use std::collections::BTreeMap;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use base64::Engine as _;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;

use xvision_core::{Action, AssetSymbol, Direction, OpenPosition, PortfolioState, RiskDecision};

use crate::executor::{ExecutionReceipt, Executor, ExecutorError};

// ── Constants ────────────────────────────────────────────────────────────────

pub(crate) const ORDERLY_MAINNET_BASE: &str = "https://api-evm.orderly.org";

/// Map an `AssetSymbol` to its Orderly perp symbol. Uses the process-global
/// asset registry when loaded, with a `"PERP_{TICKER}_USDC"` fallback for
/// unregistered symbols. Returns `NotActionable` only when the registry is
/// loaded and explicitly marks the asset as having no Orderly symbol.
pub fn orderly_symbol_for(asset: AssetSymbol) -> Result<String, ExecutorError> {
    xvision_core::asset_registry::orderly_symbol(asset).ok_or_else(|| {
        ExecutorError::NotActionable(format!(
            "Orderly does not list {} on its Mantle EVM gateway",
            asset.as_str()
        ))
    })
}

/// Inverse helper: map an Orderly market string back to its
/// `AssetSymbol`. Returns `None` only for strings that don't match the
/// `PERP_{BASE}_USDC` pattern (e.g. empty string, non-PERP prefix).
/// When the registry is loaded, a registry lookup is attempted first.
pub fn asset_symbol_from_orderly(symbol: &str) -> Option<AssetSymbol> {
    xvision_core::asset_registry::symbol_from_orderly(symbol)
}

// ── Credentials ──────────────────────────────────────────────────────────────

/// Authentication credentials for Orderly Network REST API.
///
/// `orderly_key` is the base64-encoded Ed25519 public key string provided by
/// Orderly. `orderly_secret` is the base58-encoded Ed25519 private key (may
/// include an `"ed25519:"` prefix). `orderly_account_id` is the account UUID.
#[derive(Clone)]
pub struct Credentials {
    pub orderly_key: String,
    pub orderly_secret: String,
    pub orderly_account_id: String,
}

// ── Signing ──────────────────────────────────────────────────────────────────

/// Sign `message` with the Ed25519 key derived from `secret_b58` (base58).
/// Returns the signature as a standard base64 string.
fn sign_message(secret_b58: &str, message: &str) -> Result<String, ExecutorError> {
    use ed25519_dalek::{Signer, SigningKey};

    // Strip optional "ed25519:" prefix.
    let raw = if let Some(stripped) = secret_b58.strip_prefix("ed25519:") {
        stripped
    } else {
        secret_b58
    };

    let key_bytes = bs58::decode(raw)
        .into_vec()
        .map_err(|e| ExecutorError::Auth(format!("orderly secret base58 decode: {e}")))?;

    if key_bytes.len() != 32 {
        return Err(ExecutorError::Auth(format!(
            "orderly secret: expected 32 bytes, got {}",
            key_bytes.len()
        )));
    }

    let signing_key = SigningKey::from_bytes(
        key_bytes
            .as_slice()
            .try_into()
            .map_err(|_| ExecutorError::Internal("key slice conversion failed".into()))?,
    );

    let sig = signing_key.sign(message.as_bytes());
    Ok(base64::engine::general_purpose::STANDARD.encode(sig.to_bytes()))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

/// Compose a venue `client_order_id` from a short prefix and a key,
/// stripping hyphens and truncating to Orderly's 36-char limit
/// (full prefixed UUIDs overflow it: "tp-" + 36 = 39 — the venue
/// rejects with -1005; caught live on testnet 2026-06-11).
pub(crate) fn venue_client_id(prefix: &str, key: &str) -> String {
    let compact: String = key.chars().filter(|c| *c != '-').collect();
    let mut id = format!("{prefix}{compact}");
    id.truncate(36);
    id
}

/// Round `qty` DOWN to a multiple of `tick`, defusing f64 noise (e.g.
/// 0.00048 / 0.00001 = 47.999999…) by nudging the ratio before flooring.
/// Rounding down (never up) keeps the order within the decided notional.
pub(crate) fn round_to_tick(qty: f64, tick: f64) -> f64 {
    if tick <= 0.0 {
        return qty;
    }
    let steps = (qty / tick + 1e-9).floor();
    steps * tick
}

// ── Plain-data types ─────────────────────────────────────────────────────────

/// Orderly order record returned by the API abstraction.
#[derive(Debug, Clone)]
pub struct OrderlyOrder {
    pub order_id: u64,
    pub client_order_id: Option<String>,
    /// Normalised status string (uppercase), e.g. `"FILLED"`, `"REJECTED"`.
    pub status: String,
    pub executed_quantity: Option<f64>,
    pub average_executed_price: Option<f64>,
}

impl OrderlyOrder {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status.as_str(),
            "FILLED" | "CANCELLED" | "REJECTED" | "EXPIRED"
        )
    }
    pub fn is_rejected(&self) -> bool {
        self.status == "REJECTED"
    }

    /// Terminal without any fill: REJECTED, or CANCELLED/EXPIRED with zero
    /// executed quantity. Orderly cancels market orders it cannot match
    /// (e.g. an empty testnet book) — callers must surface that as a
    /// failure, not fabricate a receipt from fallback prices (live
    /// testnet finding 2026-06-11: a CANCELLED order produced a
    /// "filled" ExecutionReceipt priced at the mark).
    pub fn is_unfilled_terminal(&self) -> bool {
        if self.status == "REJECTED" {
            return true;
        }
        matches!(self.status.as_str(), "CANCELLED" | "EXPIRED")
            && self.executed_quantity.unwrap_or(0.0) <= 0.0
    }
}

/// Account equity snapshot.
#[derive(Debug, Clone)]
pub struct OrderlyAccount {
    pub usdc_holding: f64,
    pub unrealized_pnl: f64,
}

impl OrderlyAccount {
    pub fn equity(&self) -> f64 {
        self.usdc_holding + self.unrealized_pnl
    }
}

/// Position snapshot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct OrderlyPosition {
    pub symbol: String,
    /// Positive = long, negative = short (BTC units).
    pub position_qty: f64,
    pub average_open_price: f64,
    pub mark_price: f64,
    pub unsettled_pnl: f64,
}

/// Account + positions snapshot for operator-facing venue status surfaces
/// (dashboard live page, CLI). Serializable so the engine API can pass it
/// through to HTTP handlers unchanged.
#[derive(Debug, Clone, serde::Serialize)]
pub struct VenueSnapshot {
    pub equity_usd: f64,
    pub usdc_holding: f64,
    pub unrealized_pnl: f64,
    pub positions: Vec<OrderlyPosition>,
}

/// Order side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// Algo order type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlgoKind {
    StopMarket,
    TakeProfitMarket,
}

// ── Internal trait ───────────────────────────────────────────────────────────

/// Minimal wire contract the executor needs from Orderly REST.
/// `pub` so the default type parameter on `OrderlyExecutor` is sound.
#[async_trait]
pub trait OrderlyApi: Send + Sync {
    async fn create_order(
        &self,
        symbol: &str,
        side: OrderSide,
        quantity: f64,
        client_order_id: Option<String>,
        reduce_only: Option<bool>,
    ) -> Result<OrderlyOrder, ExecutorError>;

    #[allow(clippy::too_many_arguments)]
    async fn create_algo_order(
        &self,
        symbol: &str,
        algo_type: AlgoKind,
        side: OrderSide,
        quantity: f64,
        trigger_price: f64,
        client_order_id: Option<String>,
        reduce_only: Option<bool>,
    ) -> Result<u64, ExecutorError>;

    async fn get_order(&self, order_id: u64) -> Result<OrderlyOrder, ExecutorError>;
    async fn get_account(&self) -> Result<OrderlyAccount, ExecutorError>;
    async fn get_positions(&self) -> Result<Vec<OrderlyPosition>, ExecutorError>;

    /// Mark price from the PUBLIC futures endpoint
    /// (`GET /v1/public/futures/{symbol}`, no auth). Used to convert a
    /// notional decision into base quantity when the account holds no
    /// position on the market yet.
    async fn get_mark_price(&self, symbol: &str) -> Result<f64, ExecutorError>;

    /// Per-market order constraints from the PUBLIC info endpoint
    /// (`GET /v1/public/info/{symbol}`, no auth). Quantities must be a
    /// multiple of `base_tick` and at least `base_min` or the venue
    /// rejects with -1104 ("does not match the step size").
    async fn get_symbol_meta(&self, symbol: &str) -> Result<OrderlySymbolMeta, ExecutorError>;
}

/// Order-quantity constraints for one market. See [`OrderlyApi::get_symbol_meta`].
#[derive(Debug, Clone, Copy, Deserialize)]
pub struct OrderlySymbolMeta {
    /// Quantity step (e.g. `0.00001` BTC) — submitted qty must be a multiple.
    pub base_tick: f64,
    /// Minimum base quantity per order.
    pub base_min: f64,
}

// ── Real implementation ───────────────────────────────────────────────────────

/// HTTP implementation backed by direct `reqwest` calls to the Orderly REST API.
pub struct ReqwestOrderlyApi {
    http: reqwest::Client,
    base_url: String,
    creds: Credentials,
}

impl ReqwestOrderlyApi {
    pub fn new(http: reqwest::Client, base_url: String, creds: Credentials) -> Self {
        Self {
            http,
            base_url,
            creds,
        }
    }

    /// Build and send a signed private request.
    async fn signed_request(
        &self,
        method: reqwest::Method,
        path: &str,
        body_json: Option<&str>,
    ) -> Result<reqwest::Response, ExecutorError> {
        let ts = now_ms();
        let body_str = body_json.unwrap_or("");
        let message = format!("{}{}{}{}", ts, method.as_str(), path, body_str);
        let sig = sign_message(&self.creds.orderly_secret, &message)?;

        let url = format!("{}{}", self.base_url, path);
        let mut rb = self
            .http
            .request(method.clone(), &url)
            .header("orderly-timestamp", ts.to_string())
            .header("orderly-key", &self.creds.orderly_key)
            .header("orderly-signature", sig)
            .header("orderly-account-id", &self.creds.orderly_account_id);

        if let Some(body) = body_json {
            rb = rb
                .header("content-type", "application/json")
                .body(body.to_string());
        }

        rb.send().await.map_err(|e| {
            if e.is_timeout() {
                ExecutorError::Timeout(e.to_string())
            } else {
                ExecutorError::Network(e.to_string())
            }
        })
    }

    /// Parse the response; map HTTP error status codes to `ExecutorError`.
    async fn parse_response<T: for<'de> Deserialize<'de>>(
        resp: reqwest::Response,
    ) -> Result<T, ExecutorError> {
        let status = resp.status();
        if status.is_success() {
            resp.json::<T>()
                .await
                .map_err(|e| ExecutorError::Internal(format!("orderly response parse: {e}")))
        } else {
            let code = status.as_u16();
            let body = resp.text().await.unwrap_or_default();
            if code == 401 || code == 403 {
                Err(ExecutorError::Auth(format!("HTTP {code}: {body}")))
            } else if (400..500).contains(&code) {
                Err(ExecutorError::Rejected(format!("HTTP {code}: {body}")))
            } else {
                Err(ExecutorError::Network(format!("HTTP {code}: {body}")))
            }
        }
    }
}

// ── JSON response shapes (minimal — only fields we need) ────────────────────

#[derive(Deserialize)]
struct OkEnvelope<T> {
    #[allow(dead_code)]
    success: bool,
    data: T,
}

#[derive(Deserialize)]
struct CreateOrderData {
    order_id: u64,
}

#[derive(Deserialize)]
struct OrderData {
    order_id: u64,
    #[serde(default)]
    client_order_id: Option<String>,
    status: String,
    /// The venue's `GET /v1/order/{id}` payload carries BOTH `executed`
    /// and `total_executed_quantity` (other surfaces use
    /// `executed_quantity`) — kept as separate fields (serde aliases
    /// would error on the duplicate) and coalesced in [`order_from_data`].
    #[serde(default)]
    executed_quantity: Option<f64>,
    #[serde(default)]
    total_executed_quantity: Option<f64>,
    #[serde(default)]
    executed: Option<f64>,
    #[serde(default)]
    average_executed_price: Option<f64>,
}

#[derive(Deserialize)]
struct GetOrderWrapper {
    #[serde(flatten)]
    order: OrderData,
}

#[derive(Deserialize)]
struct HoldingEntry {
    token: String,
    holding: f64,
}

#[derive(Deserialize)]
struct HoldingData {
    holding: Vec<HoldingEntry>,
}

#[derive(Deserialize)]
struct PositionEntry {
    symbol: String,
    position_qty: f64,
    average_open_price: f64,
    mark_price: f64,
    unsettled_pnl: f64,
}

#[derive(Deserialize)]
struct PositionsData {
    rows: Vec<PositionEntry>,
}

#[derive(Deserialize)]
struct AlgoOrderData {
    algo_order_id: String,
}

#[derive(Deserialize)]
struct MarkPriceData {
    mark_price: f64,
}

fn order_from_data(d: OrderData) -> OrderlyOrder {
    OrderlyOrder {
        order_id: d.order_id,
        client_order_id: d.client_order_id,
        status: d.status.to_uppercase(),
        executed_quantity: d.executed_quantity.or(d.total_executed_quantity).or(d.executed),
        average_executed_price: d.average_executed_price,
    }
}

#[async_trait]
impl OrderlyApi for ReqwestOrderlyApi {
    async fn create_order(
        &self,
        symbol: &str,
        side: OrderSide,
        quantity: f64,
        client_order_id: Option<String>,
        reduce_only: Option<bool>,
    ) -> Result<OrderlyOrder, ExecutorError> {
        let side_str = match side {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        };

        let mut body = serde_json::json!({
            "symbol": symbol,
            "order_type": "MARKET",
            "side": side_str,
            "order_quantity": quantity,
        });

        if let Some(id) = &client_order_id {
            body["client_order_id"] = serde_json::Value::String(id.clone());
        }
        if let Some(ro) = reduce_only {
            body["reduce_only"] = serde_json::Value::Bool(ro);
        }

        let body_str =
            serde_json::to_string(&body).map_err(|e| ExecutorError::Internal(format!("json: {e}")))?;

        let resp = self
            .signed_request(reqwest::Method::POST, "/v1/order", Some(&body_str))
            .await?;

        let env: OkEnvelope<CreateOrderData> = Self::parse_response(resp).await?;
        let order_id = env.data.order_id;

        // Fetch full order details immediately after creation.
        let order = self.get_order(order_id).await?;
        Ok(order)
    }

    async fn create_algo_order(
        &self,
        symbol: &str,
        algo_type: AlgoKind,
        side: OrderSide,
        quantity: f64,
        trigger_price: f64,
        client_order_id: Option<String>,
        reduce_only: Option<bool>,
    ) -> Result<u64, ExecutorError> {
        let type_str = match algo_type {
            AlgoKind::StopMarket => "STOP_MARKET",
            AlgoKind::TakeProfitMarket => "TAKE_PROFIT_MARKET",
        };
        let side_str = match side {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        };

        let mut body = serde_json::json!({
            "symbol": symbol,
            "order_type": type_str,
            "side": side_str,
            "quantity": quantity,
            "trigger_price": trigger_price,
        });

        if let Some(id) = &client_order_id {
            body["client_order_id"] = serde_json::Value::String(id.clone());
        }
        if let Some(ro) = reduce_only {
            body["reduce_only"] = serde_json::Value::Bool(ro);
        }

        let body_str =
            serde_json::to_string(&body).map_err(|e| ExecutorError::Internal(format!("json: {e}")))?;

        let resp = self
            .signed_request(reqwest::Method::POST, "/v1/algo-order", Some(&body_str))
            .await?;

        let env: OkEnvelope<AlgoOrderData> = Self::parse_response(resp).await?;
        let id = env.data.algo_order_id.parse::<u64>().unwrap_or(0);
        Ok(id)
    }

    async fn get_order(&self, order_id: u64) -> Result<OrderlyOrder, ExecutorError> {
        let path = format!("/v1/order/{}", order_id);
        let resp = self.signed_request(reqwest::Method::GET, &path, None).await?;

        let env: OkEnvelope<GetOrderWrapper> = Self::parse_response(resp).await?;
        Ok(order_from_data(env.data.order))
    }

    async fn get_account(&self) -> Result<OrderlyAccount, ExecutorError> {
        // Fetch holdings and positions concurrently.
        let (holding_resp, pos_resp) = tokio::try_join!(
            self.signed_request(reqwest::Method::GET, "/v1/client/holding", None),
            self.signed_request(reqwest::Method::GET, "/v1/positions", None),
        )?;

        let holding_env: OkEnvelope<HoldingData> = Self::parse_response(holding_resp).await?;
        let pos_env: OkEnvelope<PositionsData> = Self::parse_response(pos_resp).await?;

        let usdc_holding = holding_env
            .data
            .holding
            .iter()
            .find(|h| h.token.to_uppercase() == "USDC")
            .map(|h| h.holding)
            .unwrap_or(0.0);

        let unrealized_pnl: f64 = pos_env.data.rows.iter().map(|p| p.unsettled_pnl).sum();

        Ok(OrderlyAccount {
            usdc_holding,
            unrealized_pnl,
        })
    }

    async fn get_positions(&self) -> Result<Vec<OrderlyPosition>, ExecutorError> {
        let resp = self
            .signed_request(reqwest::Method::GET, "/v1/positions", None)
            .await?;

        let env: OkEnvelope<PositionsData> = Self::parse_response(resp).await?;
        Ok(env
            .data
            .rows
            .into_iter()
            .map(|p| OrderlyPosition {
                symbol: p.symbol,
                position_qty: p.position_qty,
                average_open_price: p.average_open_price,
                mark_price: p.mark_price,
                unsettled_pnl: p.unsettled_pnl,
            })
            .collect())
    }

    async fn get_mark_price(&self, symbol: &str) -> Result<f64, ExecutorError> {
        // Public endpoint — unsigned request.
        let url = format!("{}/v1/public/futures/{}", self.base_url, symbol);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            if e.is_timeout() {
                ExecutorError::Timeout(e.to_string())
            } else {
                ExecutorError::Network(e.to_string())
            }
        })?;
        let env: OkEnvelope<MarkPriceData> = Self::parse_response(resp).await?;
        if env.data.mark_price <= 0.0 {
            return Err(ExecutorError::Internal(format!(
                "orderly public futures {symbol}: non-positive mark price {}",
                env.data.mark_price
            )));
        }
        Ok(env.data.mark_price)
    }

    async fn get_symbol_meta(&self, symbol: &str) -> Result<OrderlySymbolMeta, ExecutorError> {
        // Public endpoint — unsigned request.
        let url = format!("{}/v1/public/info/{}", self.base_url, symbol);
        let resp = self.http.get(&url).send().await.map_err(|e| {
            if e.is_timeout() {
                ExecutorError::Timeout(e.to_string())
            } else {
                ExecutorError::Network(e.to_string())
            }
        })?;
        let env: OkEnvelope<OrderlySymbolMeta> = Self::parse_response(resp).await?;
        if env.data.base_tick <= 0.0 {
            return Err(ExecutorError::Internal(format!(
                "orderly public info {symbol}: non-positive base_tick {}",
                env.data.base_tick
            )));
        }
        Ok(env.data)
    }
}

// ── OrderlyExecutor ──────────────────────────────────────────────────────────

/// Orderly Network perpetuals executor. Routes per `TraderDecision.asset`
/// across the Mantle EVM gateway's perp markets via the asset registry
/// with a `PERP_{TICKER}_USDC` fallback.
///
/// Generic over `OrderlyApi` so tests can inject a mock.
pub struct OrderlyExecutor<A = ReqwestOrderlyApi> {
    api: A,
}

impl OrderlyExecutor<ReqwestOrderlyApi> {
    /// Build from explicit credentials.
    ///
    /// `base_url` defaults to `https://api-evm.orderly.org` when `None`.
    pub fn connect(creds: Credentials, base_url: Option<&str>) -> Result<Self, ExecutorError> {
        let url = base_url.unwrap_or(ORDERLY_MAINNET_BASE).to_string();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| ExecutorError::Internal(format!("reqwest client: {e}")))?;
        Ok(Self {
            api: ReqwestOrderlyApi::new(http, url, creds),
        })
    }

    /// Build from environment variables.
    ///
    /// Reads `ORDERLY_KEY`, `ORDERLY_SECRET`, `ORDERLY_ACCOUNT_ID`, and
    /// optionally `ORDERLY_BASE_URL` (defaults to mainnet api-evm).
    ///
    /// Brokered onboarding (`xvn setup --orderly-onboard`) is tracked as F5
    /// in FOLLOWUPS.md.
    pub fn from_env() -> Result<Self, ExecutorError> {
        let key = std::env::var("ORDERLY_KEY")
            .map_err(|_| ExecutorError::Auth("ORDERLY_KEY not set".to_string()))?;
        let secret = std::env::var("ORDERLY_SECRET")
            .map_err(|_| ExecutorError::Auth("ORDERLY_SECRET not set".to_string()))?;
        let account_id = std::env::var("ORDERLY_ACCOUNT_ID")
            .map_err(|_| ExecutorError::Auth("ORDERLY_ACCOUNT_ID not set".to_string()))?;
        let base_url = std::env::var("ORDERLY_BASE_URL").ok();

        let creds = Credentials {
            orderly_key: key,
            orderly_secret: secret,
            orderly_account_id: account_id,
        };

        Self::connect(creds, base_url.as_deref())
    }
}

impl<A: OrderlyApi> OrderlyExecutor<A> {
    #[cfg(test)]
    pub(crate) fn with_api(api: A) -> Self {
        Self { api }
    }

    /// Read-only account + positions snapshot for status surfaces (dashboard
    /// live page, CLI). Fetches account and positions concurrently; no
    /// orders are placed.
    pub async fn venue_snapshot(&self) -> Result<VenueSnapshot, ExecutorError> {
        let (account, positions) = tokio::try_join!(self.api.get_account(), self.api.get_positions())?;
        Ok(VenueSnapshot {
            equity_usd: account.equity(),
            usdc_holding: account.usdc_holding,
            unrealized_pnl: account.unrealized_pnl,
            positions,
        })
    }

    async fn await_fill(&self, order_id: u64) -> Result<OrderlyOrder, ExecutorError> {
        const MAX_POLLS: u32 = 5;
        const POLL_DELAY_MS: u64 = 200;

        for _ in 0..MAX_POLLS {
            let order = self.api.get_order(order_id).await?;
            if order.is_terminal() {
                if order.is_unfilled_terminal() {
                    return Err(ExecutorError::Rejected(format!(
                        "order {order_id} rejected: terminated unfilled ({}) — venue could not match it",
                        order.status
                    )));
                }
                return Ok(order);
            }
            tokio::time::sleep(tokio::time::Duration::from_millis(POLL_DELAY_MS)).await;
        }

        Err(ExecutorError::Timeout(format!(
            "order {order_id} did not fill within {MAX_POLLS} polls"
        )))
    }

    fn build_receipt(
        cycle_id: Uuid,
        asset: AssetSymbol,
        order: &OrderlyOrder,
        equity_usd: f64,
    ) -> ExecutionReceipt {
        let qty = order.executed_quantity.unwrap_or(0.0);
        let price = order.average_executed_price.unwrap_or(0.0);
        let notional = qty * price;
        let filled_size_bps = if equity_usd > 0.0 {
            ((notional / equity_usd) * 10_000.0).round() as u32
        } else {
            0
        };

        ExecutionReceipt {
            cycle_id,
            venue: "orderly".to_string(),
            venue_order_id: order.order_id.to_string(),
            asset,
            filled_size_bps,
            avg_fill_price: price,
            fee_bps: 0,
            submitted_at: Utc::now(),
            filled_at: Some(Utc::now()),
            note: None,
        }
    }
}

#[async_trait]
impl<A: OrderlyApi + 'static> Executor for OrderlyExecutor<A> {
    async fn submit(&self, decision: &RiskDecision) -> Result<ExecutionReceipt, ExecutorError> {
        // 1. Bail on vetoed decisions immediately.
        let td = match decision {
            RiskDecision::Vetoed { .. } => {
                return Err(ExecutorError::NotActionable("decision was vetoed".to_string()));
            }
            RiskDecision::Approved { decision: td, .. } => td,
            RiskDecision::Modified { modified: td, .. } => td,
        };

        // 2. Resolve the Orderly perp symbol for this asset. Decisions
        //    targeting an asset Orderly doesn't list are rejected at the
        //    executor boundary as a defense-in-depth gate (the risk
        //    layer's whitelist is the primary check).
        let symbol = orderly_symbol_for(td.asset)?;

        // 3. Handle Flat / Close.
        match td.action {
            Action::Flat => {
                return Err(ExecutorError::NotActionable(
                    "flat decision is not a submit".to_string(),
                ));
            }
            Action::Close => {
                return self.close_position(td.asset).await;
            }
            Action::Buy | Action::Sell => {}
        }

        // 4. Read live equity and open positions.
        let account = self.api.get_account().await?;
        let equity = account.equity();
        let positions = self.api.get_positions().await?;

        // 5. Compute base-asset quantity from notional.
        let notional_usd = (td.size_bps as f64 / 10_000.0) * equity;
        let mark_price = positions
            .iter()
            .find(|p| p.symbol == symbol)
            .map(|p| p.mark_price);

        // With no open position on this market (the first-order scenario),
        // fall back to the PUBLIC futures mark price. The previous fallback
        // sent the USD notional as `order_quantity`, but Orderly interprets
        // `order_quantity` as BASE units — a $50 BTC decision became a
        // 50-BTC order (caught live on testnet 2026-06-11; the venue's
        // max-quantity cap rejected it with code -1104).
        let mark_price = match mark_price {
            Some(p) => p,
            None => self.api.get_mark_price(&symbol).await?,
        };

        // Align to the market's step size (venue rejects misaligned
        // quantities with -1104; caught live on testnet 2026-06-11) and
        // enforce the per-order minimum.
        let meta = self.api.get_symbol_meta(&symbol).await?;
        let qty = round_to_tick(notional_usd / mark_price, meta.base_tick);
        if qty < meta.base_min {
            return Err(ExecutorError::Rejected(format!(
                "min order size: {} {} below base_min {} (notional ${:.2} @ mark {})",
                qty, symbol, meta.base_min, notional_usd, mark_price
            )));
        }

        let side = match td.action {
            Action::Buy => OrderSide::Buy,
            _ => OrderSide::Sell,
        };

        // 6. Place entry order (client_order_id = cycle_id for idempotency).
        let entry = self
            .api
            .create_order(&symbol, side, qty, Some(td.cycle_id.to_string()), None)
            .await?;

        // 7. Poll for fill.
        let filled = self.await_fill(entry.order_id).await?;
        let fill_price = filled.average_executed_price.unwrap_or(mark_price);
        let fill_qty = filled.executed_quantity.unwrap_or(qty);

        // 8. Place TP/SL bracket legs (best-effort).
        let close_side = match side {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        };
        let (tp_trigger, sl_trigger) = match side {
            OrderSide::Buy => (
                fill_price * (1.0 + td.take_profit_pct as f64 / 100.0),
                fill_price * (1.0 - td.stop_loss_pct as f64 / 100.0),
            ),
            OrderSide::Sell => (
                fill_price * (1.0 - td.take_profit_pct as f64 / 100.0),
                fill_price * (1.0 + td.stop_loss_pct as f64 / 100.0),
            ),
        };

        if fill_price > 0.0 {
            let _ = self
                .api
                .create_algo_order(
                    &symbol,
                    AlgoKind::TakeProfitMarket,
                    close_side,
                    fill_qty,
                    tp_trigger,
                    Some(venue_client_id("tp-", &td.cycle_id.to_string())),
                    Some(true),
                )
                .await;

            let _ = self
                .api
                .create_algo_order(
                    &symbol,
                    AlgoKind::StopMarket,
                    close_side,
                    fill_qty,
                    sl_trigger,
                    Some(venue_client_id("sl-", &td.cycle_id.to_string())),
                    Some(true),
                )
                .await;
        }

        // 9. Build receipt.
        let filled_size_bps = if equity > 0.0 && fill_price > 0.0 {
            ((fill_qty * fill_price / equity) * 10_000.0).round() as u32
        } else {
            td.size_bps
        };

        Ok(ExecutionReceipt {
            cycle_id: td.cycle_id,
            venue: "orderly".to_string(),
            venue_order_id: entry.order_id.to_string(),
            asset: td.asset,
            filled_size_bps,
            avg_fill_price: fill_price,
            fee_bps: 0,
            submitted_at: Utc::now(),
            filled_at: Some(Utc::now()),
            note: None,
        })
    }

    async fn close_position(&self, asset: AssetSymbol) -> Result<ExecutionReceipt, ExecutorError> {
        let symbol = orderly_symbol_for(asset)?;
        let positions = self.api.get_positions().await?;
        let target_pos = positions.iter().find(|p| p.symbol == symbol);

        let Some(pos) = target_pos else {
            return Ok(ExecutionReceipt {
                cycle_id: Uuid::nil(),
                venue: "orderly".to_string(),
                venue_order_id: String::new(),
                asset,
                filled_size_bps: 0,
                avg_fill_price: 0.0,
                fee_bps: 0,
                submitted_at: Utc::now(),
                filled_at: None,
                note: Some("no open position".to_string()),
            });
        };

        if pos.position_qty == 0.0 {
            return Ok(ExecutionReceipt {
                cycle_id: Uuid::nil(),
                venue: "orderly".to_string(),
                venue_order_id: String::new(),
                asset,
                filled_size_bps: 0,
                avg_fill_price: 0.0,
                fee_bps: 0,
                submitted_at: Utc::now(),
                filled_at: None,
                note: Some("no open position".to_string()),
            });
        }

        let qty = pos.position_qty.abs();
        let close_side = if pos.position_qty > 0.0 {
            OrderSide::Sell
        } else {
            OrderSide::Buy
        };

        let order = self
            .api
            .create_order(
                &symbol,
                close_side,
                qty,
                Some(venue_client_id("cl-", &Uuid::new_v4().to_string())),
                Some(true),
            )
            .await?;

        let filled = self.await_fill(order.order_id).await?;
        let account = self.api.get_account().await?;

        Ok(Self::build_receipt(Uuid::nil(), asset, &filled, account.equity()))
    }

    async fn portfolio(&self) -> Result<PortfolioState, ExecutorError> {
        let (account, positions) = tokio::try_join!(self.api.get_account(), self.api.get_positions(),)?;

        let equity = account.equity();
        let mut open_positions = BTreeMap::new();

        for pos in &positions {
            if pos.position_qty == 0.0 {
                continue;
            }
            // Filter out any positions on symbols we don't recognise —
            // Orderly may list markets we haven't added to
            // ORDERLY_SUPPORTED yet. Operators see them on the venue UI
            // but the executor refuses to surface them via the
            // PortfolioState contract until they're explicitly enabled.
            let Some(asset) = asset_symbol_from_orderly(&pos.symbol) else {
                continue;
            };
            let direction = if pos.position_qty > 0.0 {
                Direction::Long
            } else {
                Direction::Short
            };
            let notional = pos.position_qty.abs() * pos.mark_price;
            let size_bps = if equity > 0.0 {
                ((notional / equity) * 10_000.0).round().clamp(1.0, 2000.0) as u32
            } else {
                1
            };

            open_positions.insert(
                asset,
                OpenPosition {
                    asset,
                    direction,
                    size_bps,
                    entry_price: pos.average_open_price,
                    mark_price: pos.mark_price,
                    stop_loss_pct: 2.0,
                    take_profit_pct: 5.0,
                    opened_at: Utc::now(),
                    // Orderly leverage/liq-price readback is a follow-up; the
                    // LiquidationDistanceGuard's grounded source is byreal.
                    leverage: None,
                    liq_price: None,
                },
            );
        }

        Ok(PortfolioState {
            equity_usd: equity,
            realized_pnl_today_usd: 0.0,
            day_index: 0,
            open_positions,
            as_of: Utc::now(),
        })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────
//
// Mocking strategy: `MockOrderlyApi` implements `OrderlyApi` with in-process
// fixture data — no HTTP calls. The five required test scenarios are all driven
// through this trait. Additionally, `ReqwestOrderlyApi` tests use a mockito
// HTTP server to validate the signing headers and JSON request bodies.

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    use uuid::Uuid;
    use xvision_core::{Action, Direction, RiskDecision, TraderDecision, VetoReason};

    // ── Mock ─────────────────────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    struct CreateOrderCall {
        symbol: String,
        side: OrderSide,
        #[allow(dead_code)]
        quantity: f64,
        client_order_id: Option<String>,
    }

    #[derive(Debug, Clone)]
    struct AlgoOrderCall {
        algo_type: AlgoKind,
        side: OrderSide,
        trigger_price: f64,
        client_order_id: Option<String>,
    }

    struct MockOrderlyApi {
        create_order_result: OrderlyOrder,
        get_order_result: OrderlyOrder,
        account: OrderlyAccount,
        positions: Vec<OrderlyPosition>,
        create_order_err: Option<ExecutorError>,
        captured_create: Arc<Mutex<Option<CreateOrderCall>>>,
        captured_algo: Arc<Mutex<Vec<AlgoOrderCall>>>,
    }

    impl MockOrderlyApi {
        fn new(
            account: OrderlyAccount,
            positions: Vec<OrderlyPosition>,
            create: OrderlyOrder,
            get: OrderlyOrder,
        ) -> Self {
            Self {
                create_order_result: create,
                get_order_result: get,
                account,
                positions,
                create_order_err: None,
                captured_create: Arc::new(Mutex::new(None)),
                captured_algo: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn with_err(mut self, err: ExecutorError) -> Self {
            self.create_order_err = Some(err);
            self
        }
    }

    #[async_trait]
    impl OrderlyApi for MockOrderlyApi {
        async fn create_order(
            &self,
            symbol: &str,
            side: OrderSide,
            quantity: f64,
            client_order_id: Option<String>,
            _reduce_only: Option<bool>,
        ) -> Result<OrderlyOrder, ExecutorError> {
            if let Some(ref e) = self.create_order_err {
                return Err(match e {
                    ExecutorError::Auth(s) => ExecutorError::Auth(s.clone()),
                    ExecutorError::Rejected(s) => ExecutorError::Rejected(s.clone()),
                    ExecutorError::Network(s) => ExecutorError::Network(s.clone()),
                    other => ExecutorError::Internal(other.to_string()),
                });
            }
            *self.captured_create.lock().unwrap() = Some(CreateOrderCall {
                symbol: symbol.to_string(),
                side,
                quantity,
                client_order_id,
            });
            Ok(self.create_order_result.clone())
        }

        async fn create_algo_order(
            &self,
            _symbol: &str,
            algo_type: AlgoKind,
            side: OrderSide,
            _quantity: f64,
            trigger_price: f64,
            client_order_id: Option<String>,
            _reduce_only: Option<bool>,
        ) -> Result<u64, ExecutorError> {
            self.captured_algo.lock().unwrap().push(AlgoOrderCall {
                algo_type,
                side,
                trigger_price,
                client_order_id,
            });
            Ok(999)
        }

        async fn get_order(&self, _order_id: u64) -> Result<OrderlyOrder, ExecutorError> {
            Ok(self.get_order_result.clone())
        }

        async fn get_account(&self) -> Result<OrderlyAccount, ExecutorError> {
            Ok(self.account.clone())
        }

        async fn get_positions(&self) -> Result<Vec<OrderlyPosition>, ExecutorError> {
            Ok(self.positions.clone())
        }

        async fn get_mark_price(&self, _symbol: &str) -> Result<f64, ExecutorError> {
            Ok(MOCK_PUBLIC_MARK_PRICE)
        }

        async fn get_symbol_meta(&self, _symbol: &str) -> Result<OrderlySymbolMeta, ExecutorError> {
            Ok(OrderlySymbolMeta {
                base_tick: 0.00001,
                base_min: 0.00001,
            })
        }
    }

    /// Mark price the mock "public futures" endpoint serves — distinct from
    /// the position fixtures' 71_000 mark so tests can tell which source the
    /// executor used.
    const MOCK_PUBLIC_MARK_PRICE: f64 = 50_000.0;

    // ── Fixtures ─────────────────────────────────────────────────────────────

    fn fixture_account() -> OrderlyAccount {
        OrderlyAccount {
            usdc_holding: 100_000.0,
            unrealized_pnl: 0.0,
        }
    }

    fn fixture_filled_order(order_id: u64, client_id: Option<&str>) -> OrderlyOrder {
        OrderlyOrder {
            order_id,
            client_order_id: client_id.map(str::to_string),
            status: "FILLED".to_string(),
            executed_quantity: Some(0.1),
            average_executed_price: Some(70_000.0),
        }
    }

    fn fixture_btc_position(qty: f64) -> OrderlyPosition {
        fixture_position("PERP_BTC_USDC", qty, 70_000.0, 71_000.0)
    }

    fn fixture_position(symbol: &str, qty: f64, entry: f64, mark: f64) -> OrderlyPosition {
        OrderlyPosition {
            symbol: symbol.to_string(),
            position_qty: qty,
            average_open_price: entry,
            mark_price: mark,
            unsettled_pnl: 100.0,
        }
    }

    fn fixture_buy_decision(cycle_id: Uuid) -> RiskDecision {
        RiskDecision::Approved {
            decision: TraderDecision {
                cycle_id,
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
            },
            warnings: vec![],
        }
    }

    /// First order on a market (no open position to read a mark price
    /// from) must price the base quantity off the PUBLIC futures mark
    /// price — NOT submit the USD notional as base `order_quantity`.
    /// Regression for the live testnet finding (2026-06-11): a $50 BTC
    /// decision was submitted as a 50-BTC order and rejected with -1104.
    #[tokio::test]
    async fn submit_first_order_prices_qty_off_public_mark_price() {
        let cycle_id = Uuid::new_v4();
        let filled = fixture_filled_order(42, Some(&cycle_id.to_string()));

        // NO positions — the mark must come from the public endpoint.
        let api = MockOrderlyApi::new(fixture_account(), vec![], filled.clone(), filled);
        let captured_create = Arc::clone(&api.captured_create);

        let executor = OrderlyExecutor::with_api(api);
        executor
            .submit(&fixture_buy_decision(cycle_id))
            .await
            .expect("submit must succeed");

        let call = captured_create.lock().unwrap().clone().unwrap();
        // equity 100_000 × 1000 bps = 10_000 notional / 50_000 public mark.
        let expected_qty = 10_000.0 / MOCK_PUBLIC_MARK_PRICE;
        assert!(
            (call.quantity - expected_qty).abs() < 1e-9,
            "qty must be notional/mark ({expected_qty}), got {}",
            call.quantity
        );
    }

    /// A market order the venue CANCELLED without any fill (empty book,
    /// price protection) must surface as `Rejected` — not as a fabricated
    /// receipt priced at the mark (live testnet finding 2026-06-11).
    #[tokio::test]
    async fn submit_cancelled_unfilled_order_is_rejected() {
        let cycle_id = Uuid::new_v4();
        let created = fixture_filled_order(43, Some(&cycle_id.to_string()));
        let cancelled = OrderlyOrder {
            order_id: 43,
            client_order_id: Some(cycle_id.to_string()),
            status: "CANCELLED".to_string(),
            executed_quantity: Some(0.0),
            average_executed_price: None,
        };

        let api = MockOrderlyApi::new(
            fixture_account(),
            vec![fixture_btc_position(0.1)],
            created,
            cancelled,
        );
        let executor = OrderlyExecutor::with_api(api);

        let err = executor
            .submit(&fixture_buy_decision(cycle_id))
            .await
            .expect_err("cancelled-unfilled must be an error");
        assert!(
            matches!(err, ExecutorError::Rejected(_)),
            "expected Rejected, got {err:?}"
        );
    }

    #[test]
    fn venue_client_id_fits_orderly_limit() {
        let id = venue_client_id("tp-", "550e8400-e29b-41d4-a716-446655440000");

        assert_eq!(id, "tp-550e8400e29b41d4a716446655440000");
        assert!(id.len() <= 36);
    }

    // ── Test 1 ───────────────────────────────────────────────────────────────

    /// `submit_buy_with_bracket_constructs_correct_orders`:
    /// - `client_order_id` == `cycle_id` (idempotency)
    /// - TP and SL algo orders are placed
    /// - TP trigger > entry, SL trigger < entry (long direction)
    /// - Closing side is Sell (opposite of Buy entry)
    #[tokio::test]
    async fn submit_buy_with_bracket_constructs_correct_orders() {
        let cycle_id = Uuid::new_v4();
        let filled = fixture_filled_order(42, Some(&cycle_id.to_string()));

        let api = MockOrderlyApi::new(
            fixture_account(),
            vec![fixture_btc_position(0.1)],
            filled.clone(),
            filled,
        );
        let captured_create = Arc::clone(&api.captured_create);
        let captured_algo = Arc::clone(&api.captured_algo);

        let executor = OrderlyExecutor::with_api(api);
        let receipt = executor
            .submit(&fixture_buy_decision(cycle_id))
            .await
            .expect("submit must succeed");

        // Receipt basics.
        assert_eq!(receipt.cycle_id, cycle_id);
        assert_eq!(receipt.venue, "orderly");

        // client_order_id == cycle_id.
        let call = captured_create.lock().unwrap().clone().unwrap();
        assert_eq!(
            call.client_order_id.as_deref(),
            Some(cycle_id.to_string().as_str()),
        );
        assert_eq!(call.side, OrderSide::Buy);

        // Two algo orders placed.
        let algos = captured_algo.lock().unwrap().clone();
        assert_eq!(algos.len(), 2, "TP + SL algo orders must be placed");

        let tp = algos
            .iter()
            .find(|a| matches!(a.algo_type, AlgoKind::TakeProfitMarket))
            .expect("TP order must exist");
        let sl = algos
            .iter()
            .find(|a| matches!(a.algo_type, AlgoKind::StopMarket))
            .expect("SL order must exist");

        // For long entry at 70_000:
        let fill_price = 70_000.0_f64;
        assert!(tp.trigger_price > fill_price, "TP above fill for long");
        assert!(sl.trigger_price < fill_price, "SL below fill for long");
        assert_eq!(tp.side, OrderSide::Sell, "close side is Sell");
        assert_eq!(sl.side, OrderSide::Sell, "close side is Sell");

        // TP/SL client ids start with expected prefixes.
        assert!(tp
            .client_order_id
            .as_deref()
            .map(|s| s.starts_with("tp-"))
            .unwrap_or(false));
        assert!(sl
            .client_order_id
            .as_deref()
            .map(|s| s.starts_with("sl-"))
            .unwrap_or(false));
    }

    // ── Test 2 ───────────────────────────────────────────────────────────────

    /// `submit_vetoed_decision_returns_not_actionable` — no HTTP call.
    #[tokio::test]
    async fn submit_vetoed_decision_returns_not_actionable() {
        struct PanicApi;

        #[async_trait]
        impl OrderlyApi for PanicApi {
            async fn create_order(
                &self,
                _: &str,
                _: OrderSide,
                _: f64,
                _: Option<String>,
                _: Option<bool>,
            ) -> Result<OrderlyOrder, ExecutorError> {
                panic!("create_order must not be called")
            }
            async fn create_algo_order(
                &self,
                _: &str,
                _: AlgoKind,
                _: OrderSide,
                _: f64,
                _: f64,
                _: Option<String>,
                _: Option<bool>,
            ) -> Result<u64, ExecutorError> {
                panic!("create_algo_order must not be called")
            }
            async fn get_order(&self, _: u64) -> Result<OrderlyOrder, ExecutorError> {
                panic!("get_order must not be called")
            }
            async fn get_account(&self) -> Result<OrderlyAccount, ExecutorError> {
                panic!("get_account must not be called")
            }
            async fn get_positions(&self) -> Result<Vec<OrderlyPosition>, ExecutorError> {
                panic!("get_positions must not be called")
            }
            async fn get_mark_price(&self, _: &str) -> Result<f64, ExecutorError> {
                panic!("get_mark_price must not be called")
            }
            async fn get_symbol_meta(&self, _: &str) -> Result<OrderlySymbolMeta, ExecutorError> {
                panic!("get_symbol_meta must not be called")
            }
        }

        let executor = OrderlyExecutor::with_api(PanicApi);
        let decision = RiskDecision::Vetoed {
            original: TraderDecision {
                cycle_id: Uuid::nil(),
                action: Action::Buy,
                size_bps: 500,
                direction: Direction::Long,
                stop_loss_pct: 2.0,
                take_profit_pct: 4.0,
                trader_summary: "Vetoed test decision — should not reach executor.".into(),
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

        let err = executor
            .submit(&decision)
            .await
            .expect_err("vetoed decision must return Err");
        assert!(
            matches!(err, ExecutorError::NotActionable(_)),
            "expected NotActionable, got {err:?}"
        );
    }

    // ── Test 3 ───────────────────────────────────────────────────────────────

    /// `close_position_no_holdings_returns_zero_fill`
    #[tokio::test]
    async fn close_position_no_holdings_returns_zero_fill() {
        let filler = fixture_filled_order(1, None);
        let api = MockOrderlyApi::new(fixture_account(), vec![], filler.clone(), filler);
        let executor = OrderlyExecutor::with_api(api);

        let receipt = executor
            .close_position(AssetSymbol::Btc)
            .await
            .expect("close_position must succeed even with no holdings");

        assert_eq!(receipt.filled_size_bps, 0);
        assert_eq!(receipt.note.as_deref(), Some("no open position"));
    }

    // ── Test 4 ───────────────────────────────────────────────────────────────

    /// `portfolio_maps_orderly_positions_to_open_positions`
    #[tokio::test]
    async fn portfolio_maps_orderly_positions_to_open_positions() {
        let filler = fixture_filled_order(1, None);
        let api = MockOrderlyApi::new(
            fixture_account(),
            vec![fixture_btc_position(0.5)],
            filler.clone(),
            filler,
        );
        let executor = OrderlyExecutor::with_api(api);

        let state = executor.portfolio().await.expect("portfolio() must succeed");

        let btc = state
            .open_positions
            .get(&AssetSymbol::Btc)
            .expect("BTC perp must appear in PortfolioState");

        assert_eq!(btc.direction, Direction::Long);
        assert!(btc.size_bps > 0);
        assert_eq!(btc.entry_price, 70_000.0);
        assert_eq!(btc.mark_price, 71_000.0);
    }

    // ── Test 5 ───────────────────────────────────────────────────────────────

    /// `auth_failure_maps_to_executor_error_auth`
    #[tokio::test]
    async fn auth_failure_maps_to_executor_error_auth() {
        let filler = fixture_filled_order(1, None);
        let api = MockOrderlyApi::new(
            fixture_account(),
            vec![fixture_btc_position(0.1)],
            filler.clone(),
            filler,
        )
        .with_err(ExecutorError::Auth("403 forbidden".to_string()));

        let executor = OrderlyExecutor::with_api(api);
        let err = executor
            .submit(&fixture_buy_decision(Uuid::new_v4()))
            .await
            .expect_err("auth failure must propagate");

        assert!(
            matches!(err, ExecutorError::Auth(_)),
            "expected Auth error, got {err:?}"
        );
    }

    // ── Signing unit test ─────────────────────────────────────────────────────

    /// Verify that `sign_message` produces a valid base64-encoded Ed25519 sig.
    #[test]
    fn sign_message_produces_valid_base64_signature() {
        // Throwaway ed25519 private key (32 zero bytes encoded in base58).
        let secret_b58 = bs58::encode(vec![0u8; 32]).into_string();
        let sig = sign_message(&secret_b58, "test message").expect("sign must succeed");
        assert!(!sig.is_empty());
        // Verify it's valid base64 (86+ chars for a 64-byte sig).
        base64::engine::general_purpose::STANDARD
            .decode(&sig)
            .expect("signature must be valid base64");
    }

    /// Signing with the `ed25519:` prefix must produce the same output.
    #[test]
    fn sign_message_prefix_is_stripped() {
        let raw = bs58::encode(vec![1u8; 32]).into_string();
        let with_prefix = format!("ed25519:{}", raw);
        let msg = "1700000000000POST/v1/order{}";
        let sig_plain = sign_message(&raw, msg).unwrap();
        let sig_prefixed = sign_message(&with_prefix, msg).unwrap();
        assert_eq!(sig_plain, sig_prefixed);
    }

    // ── Mockito HTTP integration test ─────────────────────────────────────────
    //
    // Uses a real mockito server to validate that a 403 response from
    // POST /v1/order maps to `ExecutorError::Auth`. All upstream endpoints
    // (holding, positions) return minimal 200 fixtures so the executor reaches
    // the create_order call.
    //
    // Run: cargo test -p xvision-execution -- orderly_http_auth_failure_mockito
    #[tokio::test]
    async fn orderly_http_auth_failure_mockito() {
        use mockito::Server;

        let mut server = Server::new_async().await;

        // Stub GET /v1/client/holding — returns empty USDC holding.
        let _holding = server
            .mock("GET", "/v1/client/holding")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success":true,"data":{"holding":[{"token":"USDC","holding":100000.0,"frozen":0.0,"updated_time":0}]}}"#)
            .expect_at_least(1)
            .create_async()
            .await;

        // Stub GET /v1/positions — returns no open positions.
        let _positions = server
            .mock("GET", "/v1/positions")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success":true,"data":{"rows":[]}}"#)
            .expect_at_least(1)
            .create_async()
            .await;

        // Stub GET /v1/public/futures/PERP_BTC_USDC — the no-position path
        // now reads the public mark price before placing the entry order.
        let _futures = server
            .mock("GET", "/v1/public/futures/PERP_BTC_USDC")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success":true,"data":{"mark_price":70000.0}}"#)
            .create_async()
            .await;

        // Stub GET /v1/public/info/PERP_BTC_USDC — step-size metadata.
        let _info = server
            .mock("GET", "/v1/public/info/PERP_BTC_USDC")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"success":true,"data":{"base_tick":0.00001,"base_min":0.00001}}"#)
            .create_async()
            .await;

        // Stub POST /v1/order — returns 403.
        let _order = server
            .mock("POST", "/v1/order")
            .with_status(403)
            .with_header("content-type", "application/json")
            .with_body(r#"{"code":10001,"message":"auth failed"}"#)
            .create_async()
            .await;

        // Use a throwaway ed25519 key (32 zero bytes).
        let secret_b58 = bs58::encode(vec![0u8; 32]).into_string();
        let creds = Credentials {
            orderly_key: "ed25519:test_key".into(),
            orderly_secret: secret_b58,
            orderly_account_id: "0xdeadbeef".into(),
        };

        let executor = OrderlyExecutor::connect(creds, Some(&server.url())).expect("connect must succeed");

        let err = executor
            .submit(&fixture_buy_decision(Uuid::new_v4()))
            .await
            .expect_err("403 must propagate as Err");

        assert!(
            matches!(err, ExecutorError::Auth(_)),
            "expected Auth, got {err:?}"
        );
    }

    // ── Live API test (ignored) ───────────────────────────────────────────────
    //
    // Run manually:
    //   ORDERLY_KEY=xxx ORDERLY_SECRET=yyy ORDERLY_ACCOUNT_ID=zzz \
    //   cargo test -p xvision-execution -- --ignored orderly_live_portfolio
    #[tokio::test]
    #[ignore = "requires live Orderly credentials (ORDERLY_KEY, ORDERLY_SECRET, ORDERLY_ACCOUNT_ID)"]
    async fn orderly_live_portfolio() {
        let executor = OrderlyExecutor::from_env().expect("from_env must succeed");
        let state = executor.portfolio().await.expect("live portfolio() must succeed");
        println!("equity_usd: {}", state.equity_usd);
        assert!(state.equity_usd >= 0.0);
    }

    // ── Multi-asset expansion (2026-05-22) ────────────────────────────────────

    #[test]
    fn orderly_symbol_mapping_round_trips_legacy_symbols() {
        // Verify that the core legacy symbols round-trip correctly via the
        // fallback (registry not loaded in unit tests).
        let cases: &[(AssetSymbol, &str)] = &[
            (AssetSymbol::Btc, "PERP_BTC_USDC"),
            (AssetSymbol::Eth, "PERP_ETH_USDC"),
            (AssetSymbol::Sol, "PERP_SOL_USDC"),
            (AssetSymbol::Avax, "PERP_AVAX_USDC"),
            (AssetSymbol::Doge, "PERP_DOGE_USDC"),
            (AssetSymbol::Link, "PERP_LINK_USDC"),
        ];
        for (asset, sym) in cases {
            assert_eq!(
                orderly_symbol_for(*asset).unwrap_or_else(|_| "ERR".to_string()),
                *sym,
                "forward mapping for {asset:?}",
            );
            assert_eq!(
                asset_symbol_from_orderly(sym),
                Some(*asset),
                "inverse mapping for {sym}",
            );
        }
    }

    #[test]
    fn orderly_symbol_for_uses_fallback_for_unregistered_symbol() {
        // Unregistered symbol (registry not loaded) → fallback generates PERP_{SYM}_USDC.
        let sym = AssetSymbol::from_static("TESTCOIN");
        assert_eq!(orderly_symbol_for(sym).unwrap(), "PERP_TESTCOIN_USDC");
    }

    #[test]
    fn orderly_symbol_for_resolves_new_symbols_via_fallback() {
        // HYPE is not in the legacy list but the fallback generates a symbol.
        assert_eq!(
            orderly_symbol_for(AssetSymbol::from_static("HYPE")).unwrap(),
            "PERP_HYPE_USDC"
        );
        // Legacy symbols still work unchanged.
        assert_eq!(orderly_symbol_for(AssetSymbol::Btc).unwrap(), "PERP_BTC_USDC");
        assert_eq!(orderly_symbol_for(AssetSymbol::Eth).unwrap(), "PERP_ETH_USDC");
        assert_eq!(orderly_symbol_for(AssetSymbol::Sol).unwrap(), "PERP_SOL_USDC");
    }

    #[test]
    fn asset_symbol_from_orderly_parses_new_symbols() {
        let hype = asset_symbol_from_orderly("PERP_HYPE_USDC").unwrap();
        assert_eq!(hype.as_str(), "HYPE");
        let btc = asset_symbol_from_orderly("PERP_BTC_USDC").unwrap();
        assert_eq!(btc, AssetSymbol::Btc);
    }

    #[test]
    fn asset_symbol_from_orderly_returns_none_for_malformed_strings() {
        // Empty string — no pattern match.
        assert_eq!(asset_symbol_from_orderly(""), None);
        // Wrong format — not a PERP_*_USDC string.
        assert_eq!(asset_symbol_from_orderly("BTC/USD"), None);
        // Empty base — PERP__USDC is rejected.
        assert_eq!(asset_symbol_from_orderly("PERP__USDC"), None);
    }

    /// `submit_routes_per_decision_asset`: when the decision names ETH, the
    /// captured entry-order request must target `PERP_ETH_USDC`, not the
    /// previously-hardcoded `PERP_BTC_USDC`. Mirrors Alpaca's
    /// `submit_honors_trader_decision_asset` test.
    #[tokio::test]
    async fn submit_routes_per_decision_asset_to_orderly_eth() {
        let cycle_id = Uuid::new_v4();
        let entry_order = fixture_create_order_result(7777);
        let get_order = fixture_filled_order(7777, Some(&cycle_id.to_string()));

        let api = MockOrderlyApi::new(
            fixture_account(),
            vec![fixture_position("PERP_ETH_USDC", 0.0, 3_500.0, 3_500.0)],
            entry_order,
            get_order,
        );
        let captured = api.captured_create.clone();

        let executor = OrderlyExecutor::with_api(api);
        let decision = RiskDecision::Approved {
            decision: TraderDecision {
                cycle_id,
                action: Action::Buy,
                size_bps: 1000,
                direction: Direction::Long,
                stop_loss_pct: 2.5,
                take_profit_pct: 5.0,
                trader_summary: "Long ETH 1000bps confirming range break with 2:1 R:R.".into(),
                asset: AssetSymbol::Eth,
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
        };

        let receipt = executor.submit(&decision).await.expect("ETH submit must succeed");
        assert_eq!(receipt.asset, AssetSymbol::Eth);

        let req = captured
            .lock()
            .unwrap()
            .clone()
            .expect("a create_order must have been recorded");
        assert_eq!(
            req.symbol, "PERP_ETH_USDC",
            "entry order must route to the asset named on the decision"
        );
    }

    /// With the registry fallback, `orderly_symbol_for` generates a
    /// `PERP_{SYM}_USDC` string for any asset when the registry is not loaded.
    /// This replaces the old "rejects unsupported asset" test — that behaviour
    /// is now gated by the registry (W4 scope: a registry entry with
    /// `orderly_symbol = None` will still reject).
    #[tokio::test]
    async fn submit_uses_fallback_symbol_for_unregistered_asset() {
        let cycle_id = Uuid::new_v4();
        let entry_order = fixture_create_order_result(9999);
        let get_order = fixture_filled_order(9999, Some(&cycle_id.to_string()));

        let api = MockOrderlyApi::new(
            fixture_account(),
            // provide a SHIB position so mark price is known
            vec![fixture_position("PERP_SHIB_USDC", 0.0, 0.000015, 0.000015)],
            entry_order,
            get_order,
        );
        let captured = api.captured_create.clone();

        let executor = OrderlyExecutor::with_api(api);
        let decision = RiskDecision::Approved {
            decision: TraderDecision {
                cycle_id,
                action: Action::Buy,
                size_bps: 500,
                direction: Direction::Long,
                stop_loss_pct: 2.0,
                take_profit_pct: 5.0,
                trader_summary: "SHIB long via registry fallback path.".into(),
                asset: AssetSymbol::Shib,
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
        };

        let receipt = executor
            .submit(&decision)
            .await
            .expect("fallback path must succeed when registry not loaded");
        assert_eq!(receipt.asset, AssetSymbol::Shib);

        let req = captured
            .lock()
            .unwrap()
            .clone()
            .expect("create_order must have been called");
        assert_eq!(
            req.symbol, "PERP_SHIB_USDC",
            "fallback must generate PERP_SHIB_USDC"
        );
    }

    /// `close_position(ETH)` must look up the ETH position by
    /// `PERP_ETH_USDC` and submit the close order against the same
    /// symbol. Before the multi-asset expansion this was hardcoded to
    /// the BTC market regardless of the requested asset.
    #[tokio::test]
    async fn close_position_routes_per_asset() {
        let api = MockOrderlyApi::new(
            fixture_account(),
            vec![fixture_position("PERP_ETH_USDC", 1.5, 3_500.0, 3_550.0)],
            fixture_create_order_result(8888),
            fixture_filled_order(8888, None),
        );
        let captured = api.captured_create.clone();

        let executor = OrderlyExecutor::with_api(api);
        let receipt = executor
            .close_position(AssetSymbol::Eth)
            .await
            .expect("close_position(ETH) must succeed");

        assert_eq!(receipt.asset, AssetSymbol::Eth);
        let req = captured
            .lock()
            .unwrap()
            .clone()
            .expect("close must call create_order");
        assert_eq!(req.symbol, "PERP_ETH_USDC");
        // ETH position was long → close side is Sell.
        assert!(matches!(req.side, OrderSide::Sell));
    }

    /// Closing an asset Orderly doesn't list returns `NotActionable`
    /// With the registry fallback, `close_position` generates a PERP symbol
    /// for any asset and proceeds to call `get_positions`. When there is no
    /// open position it returns a zero-fill receipt rather than an error.
    /// This replaces the old "rejects unsupported close" test — the rejection
    /// now requires the registry to mark `orderly_symbol = None` (W4 scope).
    #[tokio::test]
    async fn close_position_uses_fallback_and_returns_zero_fill_when_no_position() {
        let filler = fixture_filled_order(1, None);
        let api = MockOrderlyApi::new(fixture_account(), vec![], filler.clone(), filler);
        let executor = OrderlyExecutor::with_api(api);

        let receipt = executor
            .close_position(AssetSymbol::Shib)
            .await
            .expect("close_position(SHIB) must succeed via fallback when no position is open");
        assert_eq!(receipt.filled_size_bps, 0);
        assert_eq!(receipt.note.as_deref(), Some("no open position"));
    }

    /// `portfolio()` must map an ETH position back to `AssetSymbol::Eth`
    /// via the inverse helper, not silently drop it because it isn't BTC.
    /// Markets with unparseable symbol strings must be filtered out.
    #[tokio::test]
    async fn portfolio_maps_eth_position_to_asset_symbol_eth() {
        let api = MockOrderlyApi::new(
            fixture_account(),
            vec![
                fixture_position("PERP_ETH_USDC", 2.0, 3_500.0, 3_550.0),
                fixture_position("PERP_BTC_USDC", 0.05, 70_000.0, 71_000.0),
                // Well-formed PERP_*_USDC → XRP is now parsed by the fallback
                fixture_position("PERP_XRP_USDC", 100.0, 2.0, 2.1),
                // Malformed symbol — must be filtered, not blow up the mapping
                fixture_position("SPOT_ETH_USDC", 0.1, 3_500.0, 3_550.0),
            ],
            fixture_create_order_result(1),
            fixture_filled_order(1, None),
        );
        let executor = OrderlyExecutor::with_api(api);
        let state = executor.portfolio().await.expect("portfolio must succeed");

        assert!(state.open_positions.contains_key(&AssetSymbol::Eth));
        assert!(state.open_positions.contains_key(&AssetSymbol::Btc));
        assert!(state
            .open_positions
            .contains_key(&AssetSymbol::from_static("XRP")));
        assert_eq!(
            state.open_positions.len(),
            3,
            "PERP_*_USDC markets resolve via fallback; only truly malformed symbols are filtered"
        );
    }

    // ── venue_snapshot ────────────────────────────────────────────────────────

    /// `venue_snapshot()` must combine equity (holding + uPnL) with the raw
    /// position rows and serialize cleanly.
    #[tokio::test]
    async fn venue_snapshot_combines_account_and_positions() {
        let filler = fixture_filled_order(1, None);
        let api = MockOrderlyApi::new(
            OrderlyAccount {
                usdc_holding: 1_000.0,
                unrealized_pnl: 25.5,
            },
            vec![fixture_btc_position(0.5)],
            filler.clone(),
            filler,
        );
        let executor = OrderlyExecutor::with_api(api);

        let snap = executor.venue_snapshot().await.expect("snapshot must succeed");

        assert_eq!(snap.usdc_holding, 1_000.0);
        assert_eq!(snap.unrealized_pnl, 25.5);
        assert_eq!(snap.equity_usd, 1_025.5);
        assert_eq!(snap.positions.len(), 1);
        assert_eq!(snap.positions[0].symbol, "PERP_BTC_USDC");
        assert_eq!(snap.positions[0].position_qty, 0.5);

        let json = serde_json::to_value(&snap).expect("VenueSnapshot must serialize");
        assert_eq!(json["equity_usd"], 1_025.5);
        assert_eq!(json["positions"][0]["symbol"], "PERP_BTC_USDC");
        assert_eq!(json["positions"][0]["mark_price"], 71_000.0);
    }

    // Helper used by the new multi-asset tests above. Returns a generic
    // `OrderlyOrder` pretending to be the just-created order; tests
    // pair it with `fixture_filled_order` returned by `get_order` to
    // complete the await_fill loop in `submit`.
    fn fixture_create_order_result(id: u64) -> OrderlyOrder {
        OrderlyOrder {
            order_id: id,
            client_order_id: None,
            status: "NEW".to_string(),
            executed_quantity: None,
            average_executed_price: None,
        }
    }
}
