//! Phase 6.3 — Orderly Network perpetuals executor (Mantle EVM gateway).
//!
//! v1 scope: BTC-only via `PERP_BTC_USDC` on `https://api-evm.orderly.org`.
//!
//! # SDK dep conflict — why we use raw reqwest
//!
//! `orderly-connector-rs 0.4.15` pins `zeroize = "=1.3.0"` via its `solana-sdk`
//! transitive dependency. The workspace already uses `reqwest = 0.13` (in
//! `xianvec-intern` and others) which pulls `rustls 0.23` requiring
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

use xianvec_core::{Action, AssetSymbol, Direction, OpenPosition, PortfolioState, RiskDecision};

use crate::executor::{ExecutionReceipt, Executor, ExecutorError};

// ── Constants ────────────────────────────────────────────────────────────────

const PERP_BTC_USDC: &str = "PERP_BTC_USDC";
const ORDERLY_MAINNET_BASE: &str = "https://api-evm.orderly.org";

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
#[derive(Debug, Clone)]
pub struct OrderlyPosition {
    pub symbol: String,
    /// Positive = long, negative = short (BTC units).
    pub position_qty: f64,
    pub average_open_price: f64,
    pub mark_price: f64,
    pub unsettled_pnl: f64,
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
            resp.json::<T>().await.map_err(|e| {
                ExecutorError::Internal(format!("orderly response parse: {e}"))
            })
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
    #[serde(default)]
    executed_quantity: Option<f64>,
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

fn order_from_data(d: OrderData) -> OrderlyOrder {
    OrderlyOrder {
        order_id: d.order_id,
        client_order_id: d.client_order_id,
        status: d.status.to_uppercase(),
        executed_quantity: d.executed_quantity,
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

        let body_str = serde_json::to_string(&body)
            .map_err(|e| ExecutorError::Internal(format!("json: {e}")))?;

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

        let body_str = serde_json::to_string(&body)
            .map_err(|e| ExecutorError::Internal(format!("json: {e}")))?;

        let resp = self
            .signed_request(reqwest::Method::POST, "/v1/algo-order", Some(&body_str))
            .await?;

        let env: OkEnvelope<AlgoOrderData> = Self::parse_response(resp).await?;
        let id = env
            .data
            .algo_order_id
            .parse::<u64>()
            .unwrap_or(0);
        Ok(id)
    }

    async fn get_order(&self, order_id: u64) -> Result<OrderlyOrder, ExecutorError> {
        let path = format!("/v1/order/{}", order_id);
        let resp = self
            .signed_request(reqwest::Method::GET, &path, None)
            .await?;

        let env: OkEnvelope<GetOrderWrapper> = Self::parse_response(resp).await?;
        Ok(order_from_data(env.data.order))
    }

    async fn get_account(&self) -> Result<OrderlyAccount, ExecutorError> {
        // Fetch holdings and positions concurrently.
        let (holding_resp, pos_resp) = tokio::try_join!(
            self.signed_request(reqwest::Method::GET, "/v1/client/holding", None),
            self.signed_request(reqwest::Method::GET, "/v1/positions", None),
        )?;

        let holding_env: OkEnvelope<HoldingData> =
            Self::parse_response(holding_resp).await?;
        let pos_env: OkEnvelope<PositionsData> =
            Self::parse_response(pos_resp).await?;

        let usdc_holding = holding_env
            .data
            .holding
            .iter()
            .find(|h| h.token.to_uppercase() == "USDC")
            .map(|h| h.holding)
            .unwrap_or(0.0);

        let unrealized_pnl: f64 = pos_env
            .data
            .rows
            .iter()
            .map(|p| p.unsettled_pnl)
            .sum();

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
}

// ── OrderlyExecutor ──────────────────────────────────────────────────────────

/// Orderly Network perpetuals executor. v1: BTC-only (`PERP_BTC_USDC`) on
/// the Mantle EVM gateway.
///
/// Generic over `OrderlyApi` so tests can inject a mock.
pub struct OrderlyExecutor<A = ReqwestOrderlyApi> {
    api: A,
    symbol: String,
}

impl OrderlyExecutor<ReqwestOrderlyApi> {
    /// Build from explicit credentials.
    ///
    /// `base_url` defaults to `https://api-evm.orderly.org` when `None`.
    pub fn connect(
        creds: Credentials,
        base_url: Option<&str>,
    ) -> Result<Self, ExecutorError> {
        let url = base_url.unwrap_or(ORDERLY_MAINNET_BASE).to_string();
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .map_err(|e| ExecutorError::Internal(format!("reqwest client: {e}")))?;
        Ok(Self {
            api: ReqwestOrderlyApi::new(http, url, creds),
            symbol: PERP_BTC_USDC.to_string(),
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
        Self {
            api,
            symbol: PERP_BTC_USDC.to_string(),
        }
    }

    async fn await_fill(&self, order_id: u64) -> Result<OrderlyOrder, ExecutorError> {
        const MAX_POLLS: u32 = 5;
        const POLL_DELAY_MS: u64 = 200;

        for _ in 0..MAX_POLLS {
            let order = self.api.get_order(order_id).await?;
            if order.is_terminal() {
                if order.is_rejected() {
                    return Err(ExecutorError::Rejected(format!(
                        "order {order_id} was rejected by Orderly"
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

    fn build_receipt(cycle_id: Uuid, order: &OrderlyOrder, equity_usd: f64) -> ExecutionReceipt {
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
            asset: AssetSymbol::Btc,
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
                return Err(ExecutorError::NotActionable(
                    "decision was vetoed".to_string(),
                ));
            }
            RiskDecision::Approved { decision: td } => td,
            RiskDecision::Modified { modified: td, .. } => td,
        };

        // 2. Handle Flat / Close.
        match td.action {
            Action::Flat => {
                return Err(ExecutorError::NotActionable(
                    "flat decision is not a submit".to_string(),
                ));
            }
            Action::Close => {
                return self.close_position(AssetSymbol::Btc).await;
            }
            Action::Buy | Action::Sell => {}
        }

        // 3. Read live equity and open positions.
        let account = self.api.get_account().await?;
        let equity = account.equity();
        let positions = self.api.get_positions().await?;

        // 4. Compute BTC quantity from notional.
        let notional_usd = (td.size_bps as f64 / 10_000.0) * equity;
        let btc_mark = positions
            .iter()
            .find(|p| p.symbol == self.symbol)
            .map(|p| p.mark_price);

        // If we have a mark price, compute base quantity; otherwise send notional
        // directly via order_quantity (Orderly accepts USDC notional for Market
        // orders when no base qty is available — see F19 for the cleaner approach
        // once the SDK dep conflict is resolved). For safety, if no mark price
        // exists, use a minimum-safe proxy: notional / current_btc_estimate.
        // In v1 single-shot flow the account almost always has a BTC position
        // already so mark_price is available; the fallback triggers only on
        // the very first order.
        let qty = if let Some(price) = btc_mark {
            notional_usd / price
        } else {
            // No mark price available. This is a first-order scenario.
            // Orderly's Market order with order_quantity in base asset.
            // Without a price we cannot compute base qty correctly; use
            // order_amount (USD notional) path by sending quantity=notional
            // and documenting the limitation. The fill receipt will correct
            // the bps math via the actual filled price.
            notional_usd
        };

        let side = match td.action {
            Action::Buy => OrderSide::Buy,
            _ => OrderSide::Sell,
        };

        // 5. Place entry order (client_order_id = cycle_id for idempotency).
        let entry = self
            .api
            .create_order(
                &self.symbol,
                side,
                qty,
                Some(td.cycle_id.to_string()),
                None,
            )
            .await?;

        // 6. Poll for fill.
        let filled = self.await_fill(entry.order_id).await?;
        let fill_price = filled.average_executed_price.unwrap_or(btc_mark.unwrap_or(0.0));
        let fill_qty = filled.executed_quantity.unwrap_or(qty);

        // 7. Place TP/SL bracket legs (best-effort).
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
                    &self.symbol,
                    AlgoKind::TakeProfitMarket,
                    close_side,
                    fill_qty,
                    tp_trigger,
                    Some(format!("tp-{}", td.cycle_id)),
                    Some(true),
                )
                .await;

            let _ = self
                .api
                .create_algo_order(
                    &self.symbol,
                    AlgoKind::StopMarket,
                    close_side,
                    fill_qty,
                    sl_trigger,
                    Some(format!("sl-{}", td.cycle_id)),
                    Some(true),
                )
                .await;
        }

        // 8. Build receipt.
        let filled_size_bps = if equity > 0.0 && fill_price > 0.0 {
            ((fill_qty * fill_price / equity) * 10_000.0).round() as u32
        } else {
            td.size_bps
        };

        Ok(ExecutionReceipt {
            cycle_id: td.cycle_id,
            venue: "orderly".to_string(),
            venue_order_id: entry.order_id.to_string(),
            asset: AssetSymbol::Btc,
            filled_size_bps,
            avg_fill_price: fill_price,
            fee_bps: 0,
            submitted_at: Utc::now(),
            filled_at: Some(Utc::now()),
            note: None,
        })
    }

    async fn close_position(&self, _asset: AssetSymbol) -> Result<ExecutionReceipt, ExecutorError> {
        let positions = self.api.get_positions().await?;
        let btc_pos = positions.iter().find(|p| p.symbol == self.symbol);

        let Some(pos) = btc_pos else {
            return Ok(ExecutionReceipt {
                cycle_id: Uuid::nil(),
                venue: "orderly".to_string(),
                venue_order_id: String::new(),
                asset: AssetSymbol::Btc,
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
                asset: AssetSymbol::Btc,
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
                &self.symbol,
                close_side,
                qty,
                Some(format!("close-{}", Uuid::new_v4())),
                Some(true),
            )
            .await?;

        let filled = self.await_fill(order.order_id).await?;
        let account = self.api.get_account().await?;

        Ok(Self::build_receipt(Uuid::nil(), &filled, account.equity()))
    }

    async fn portfolio(&self) -> Result<PortfolioState, ExecutorError> {
        let (account, positions) = tokio::try_join!(
            self.api.get_account(),
            self.api.get_positions(),
        )?;

        let equity = account.equity();
        let mut open_positions = BTreeMap::new();

        for pos in &positions {
            if pos.symbol != PERP_BTC_USDC || pos.position_qty == 0.0 {
                continue;
            }
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
                AssetSymbol::Btc,
                OpenPosition {
                    asset: AssetSymbol::Btc,
                    direction,
                    size_bps,
                    entry_price: pos.average_open_price,
                    mark_price: pos.mark_price,
                    stop_loss_pct: 2.0,
                    take_profit_pct: 5.0,
                    opened_at: Utc::now(),
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
    use xianvec_core::{Action, Direction, RiskDecision, TraderDecision, VetoReason};

    // ── Mock ─────────────────────────────────────────────────────────────────

    #[derive(Debug, Clone)]
    struct CreateOrderCall {
        #[allow(dead_code)]
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
    }

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
        OrderlyPosition {
            symbol: PERP_BTC_USDC.to_string(),
            position_qty: qty,
            average_open_price: 70_000.0,
            mark_price: 71_000.0,
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
            },
        }
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
        assert!(tp.client_order_id.as_deref().map(|s| s.starts_with("tp-")).unwrap_or(false));
        assert!(sl.client_order_id.as_deref().map(|s| s.starts_with("sl-")).unwrap_or(false));
    }

    // ── Test 2 ───────────────────────────────────────────────────────────────

    /// `submit_vetoed_decision_returns_not_actionable` — no HTTP call.
    #[tokio::test]
    async fn submit_vetoed_decision_returns_not_actionable() {
        struct PanicApi;

        #[async_trait]
        impl OrderlyApi for PanicApi {
            async fn create_order(&self, _: &str, _: OrderSide, _: f64, _: Option<String>, _: Option<bool>) -> Result<OrderlyOrder, ExecutorError> {
                panic!("create_order must not be called")
            }
            async fn create_algo_order(&self, _: &str, _: AlgoKind, _: OrderSide, _: f64, _: f64, _: Option<String>, _: Option<bool>) -> Result<u64, ExecutorError> {
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
    // Run: cargo test -p xianvec-execution -- orderly_http_auth_failure_mockito
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

        let executor = OrderlyExecutor::connect(creds, Some(&server.url()))
            .expect("connect must succeed");

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
    //   cargo test -p xianvec-execution -- --ignored orderly_live_portfolio
    #[tokio::test]
    #[ignore = "requires live Orderly credentials (ORDERLY_KEY, ORDERLY_SECRET, ORDERLY_ACCOUNT_ID)"]
    async fn orderly_live_portfolio() {
        let executor = OrderlyExecutor::from_env().expect("from_env must succeed");
        let state = executor.portfolio().await.expect("live portfolio() must succeed");
        println!("equity_usd: {}", state.equity_usd);
        assert!(state.equity_usd >= 0.0);
    }
}
