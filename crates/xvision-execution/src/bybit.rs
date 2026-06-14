use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Context;
use async_trait::async_trait;
use hmac::{Hmac, Mac};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use crate::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest, Side};

// ── Asset symbol mapping ─────────────────────────────────────────────────────

pub fn to_bybit_symbol(asset: &str) -> String {
    if asset.ends_with("USDT") {
        return asset.to_string();
    }
    let base = asset
        .strip_suffix("/USD")
        .or_else(|| asset.strip_suffix("USD"))
        .unwrap_or(asset);
    format!("{base}USDT")
}

// ── Wire types ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BybitOrderRequest {
    pub symbol: String,
    pub side: String,
    #[serde(rename = "orderType")]
    pub order_type: String,
    pub qty: String,
    pub category: String,
    #[serde(rename = "timeInForce")]
    pub time_in_force: String,
    #[serde(rename = "orderLinkId")]
    pub order_link_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BybitOrderResult {
    #[serde(rename = "orderId")]
    pub order_id: String,
    #[serde(rename = "orderLinkId")]
    pub order_link_id: String,
}

#[derive(Debug, Deserialize)]
struct BybitResponse<T> {
    #[serde(rename = "retCode")]
    ret_code: i64,
    #[serde(rename = "retMsg")]
    ret_msg: String,
    result: Option<T>,
}

#[derive(Debug, Deserialize)]
struct PositionList {
    list: Vec<PositionItem>,
}

#[derive(Debug, Deserialize)]
struct PositionItem {
    symbol: String,
    side: String,
    size: String,
}

#[derive(Debug, Deserialize)]
struct WalletBalanceList {
    list: Vec<WalletBalanceItem>,
}

#[derive(Debug, Deserialize)]
struct WalletBalanceItem {
    #[serde(rename = "totalEquity")]
    total_equity: String,
}

// ── BybitApi trait ────────────────────────────────────────────────────────────

#[async_trait]
pub trait BybitApi: Send + Sync {
    async fn place_order(&self, req: BybitOrderRequest) -> anyhow::Result<BybitOrderResult>;
    async fn positions(&self, symbol: &str) -> anyhow::Result<f64>;
    async fn wallet_balance(&self) -> anyhow::Result<f64>;
}

// ── Auth helpers ─────────────────────────────────────────────────────────────

const RECV_WINDOW: &str = "5000";

fn current_timestamp_ms() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}

fn hmac_sha256_hex(secret: &[u8], msg: &[u8]) -> String {
    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(secret).expect("HMAC accepts any key length");
    mac.update(msg);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect()
}

// ── BybitTestnetClient ────────────────────────────────────────────────────────

pub struct BybitTestnetClient {
    api_key: String,
    api_secret: String,
    client: Client,
    base_url: String,
}

impl BybitTestnetClient {
    pub fn from_env() -> anyhow::Result<Self> {
        let api_key = std::env::var("BYBIT_API_KEY").context("BYBIT_API_KEY not set")?;
        let api_secret = std::env::var("BYBIT_API_SECRET").context("BYBIT_API_SECRET not set")?;
        Ok(Self {
            api_key,
            api_secret,
            client: Client::new(),
            base_url: "https://api-testnet.bybit.com".to_string(),
        })
    }

    fn build_sign(&self, ts: &str, payload: &str) -> String {
        let msg = format!("{}{}{}{}", ts, self.api_key, RECV_WINDOW, payload);
        hmac_sha256_hex(self.api_secret.as_bytes(), msg.as_bytes())
    }

    async fn post_signed(&self, path: &str, body: &str) -> anyhow::Result<reqwest::Response> {
        let ts = current_timestamp_ms();
        let sign = self.build_sign(&ts, body);
        let url = format!("{}{}", self.base_url, path);
        self.client
            .post(&url)
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-TIMESTAMP", &ts)
            .header("X-BAPI-SIGN", &sign)
            .header("X-BAPI-RECV-WINDOW", RECV_WINDOW)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await
            .context("bybit POST request")
    }

    async fn get_signed(&self, path: &str, query: &str) -> anyhow::Result<reqwest::Response> {
        let ts = current_timestamp_ms();
        let sign = self.build_sign(&ts, query);
        let url = format!("{}{}?{}", self.base_url, path, query);
        self.client
            .get(&url)
            .header("X-BAPI-API-KEY", &self.api_key)
            .header("X-BAPI-TIMESTAMP", &ts)
            .header("X-BAPI-SIGN", &sign)
            .header("X-BAPI-RECV-WINDOW", RECV_WINDOW)
            .send()
            .await
            .context("bybit GET request")
    }
}

#[async_trait]
impl BybitApi for BybitTestnetClient {
    async fn place_order(&self, req: BybitOrderRequest) -> anyhow::Result<BybitOrderResult> {
        let body = serde_json::to_string(&req).context("serialize BybitOrderRequest")?;
        let resp = self.post_signed("/v5/order/create", &body).await?;
        let parsed: BybitResponse<BybitOrderResult> = resp.json().await.context("bybit place_order parse")?;
        if parsed.ret_code != 0 {
            anyhow::bail!("bybit place_order error {}: {}", parsed.ret_code, parsed.ret_msg);
        }
        parsed.result.context("bybit place_order: missing result")
    }

    async fn positions(&self, symbol: &str) -> anyhow::Result<f64> {
        let query = format!("category=linear&symbol={}", symbol);
        let resp = self.get_signed("/v5/position/list", &query).await?;
        let parsed: BybitResponse<PositionList> = resp.json().await.context("bybit positions parse")?;
        if parsed.ret_code != 0 {
            anyhow::bail!("bybit positions error {}: {}", parsed.ret_code, parsed.ret_msg);
        }
        let list = parsed.result.map(|r| r.list).unwrap_or_default();
        let size = list
            .iter()
            .find(|p| p.symbol == symbol)
            .and_then(|p| {
                let s: f64 = p.size.parse().ok()?;
                Some(if p.side == "Buy" { s } else { -s })
            })
            .unwrap_or(0.0);
        Ok(size)
    }

    async fn wallet_balance(&self) -> anyhow::Result<f64> {
        let query = "accountType=UNIFIED";
        let resp = self.get_signed("/v5/account/wallet-balance", query).await?;
        let parsed: BybitResponse<WalletBalanceList> =
            resp.json().await.context("bybit wallet_balance parse")?;
        if parsed.ret_code != 0 {
            anyhow::bail!(
                "bybit wallet_balance error {}: {}",
                parsed.ret_code,
                parsed.ret_msg
            );
        }
        let equity = parsed
            .result
            .and_then(|r| r.list.into_iter().next())
            .and_then(|item| item.total_equity.parse::<f64>().ok())
            .unwrap_or(0.0);
        Ok(equity)
    }
}

// ── MockBybitClient ───────────────────────────────────────────────────────────

pub struct MockBybitClient {
    place_order_log: Mutex<Vec<BybitOrderRequest>>,
    positions_log: Mutex<Vec<String>>,
    wallet_balance_count: Mutex<u32>,
    scripted_position: f64,
    scripted_balance: f64,
}

impl Default for MockBybitClient {
    fn default() -> Self {
        Self::new()
    }
}

impl MockBybitClient {
    pub fn new() -> Self {
        Self {
            place_order_log: Mutex::new(Vec::new()),
            positions_log: Mutex::new(Vec::new()),
            wallet_balance_count: Mutex::new(0),
            scripted_position: 0.0,
            scripted_balance: 100_000.0,
        }
    }

    pub fn with_position(mut self, pos: f64) -> Self {
        self.scripted_position = pos;
        self
    }

    pub fn with_balance(mut self, balance: f64) -> Self {
        self.scripted_balance = balance;
        self
    }

    pub fn place_order_calls(&self) -> Vec<BybitOrderRequest> {
        self.place_order_log.lock().unwrap().clone()
    }

    pub fn positions_calls(&self) -> Vec<String> {
        self.positions_log.lock().unwrap().clone()
    }

    pub fn wallet_balance_call_count(&self) -> u32 {
        *self.wallet_balance_count.lock().unwrap()
    }
}

#[async_trait]
impl BybitApi for MockBybitClient {
    async fn place_order(&self, req: BybitOrderRequest) -> anyhow::Result<BybitOrderResult> {
        let result = BybitOrderResult {
            order_id: format!("mock-{}", req.order_link_id),
            order_link_id: req.order_link_id.clone(),
        };
        self.place_order_log.lock().unwrap().push(req);
        Ok(result)
    }

    async fn positions(&self, symbol: &str) -> anyhow::Result<f64> {
        self.positions_log.lock().unwrap().push(symbol.to_string());
        Ok(self.scripted_position)
    }

    async fn wallet_balance(&self) -> anyhow::Result<f64> {
        *self.wallet_balance_count.lock().unwrap() += 1;
        Ok(self.scripted_balance)
    }
}

// ── BybitPaperSurface ─────────────────────────────────────────────────────────

pub struct BybitPaperSurface<A: BybitApi> {
    api: Arc<A>,
}

impl<A: BybitApi> BybitPaperSurface<A> {
    pub fn with_api(api: Arc<A>) -> Self {
        Self { api }
    }
}

impl BybitPaperSurface<BybitTestnetClient> {
    pub fn from_env() -> anyhow::Result<Self> {
        let client = BybitTestnetClient::from_env()?;
        Ok(Self {
            api: Arc::new(client),
        })
    }
}

#[async_trait]
impl<A: BybitApi + 'static> BrokerSurface for BybitPaperSurface<A> {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let symbol = to_bybit_symbol(&req.asset);
        let side = match req.side {
            Side::Buy => "Buy",
            Side::Sell => "Sell",
        };
        let bybit_req = BybitOrderRequest {
            symbol,
            side: side.to_string(),
            order_type: "Market".to_string(),
            qty: format!("{:.8}", req.size),
            category: "linear".to_string(),
            time_in_force: "IOC".to_string(),
            order_link_id: req.idempotency_key.clone(),
        };
        let result = self
            .api
            .place_order(bybit_req)
            .await
            .context("bybit place_order")?;
        Ok(OrderConfirmation {
            broker_order_id: result.order_id,
            fill_price: Some(req.reference_price_usd),
            fill_size: req.size,
            fee: None,
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let symbol = to_bybit_symbol(asset);
        self.api.positions(&symbol).await.context("bybit positions")
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        self.api.wallet_balance().await.context("bybit wallet_balance")
    }

    fn venue(&self) -> &str {
        "bybit"
    }

    fn signing_scheme(&self) -> &str {
        "api-key"
    }

    fn is_perp_venue(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod venue_identity_tests {
    use super::*;

    #[test]
    fn paper_surface_reports_bybit_api_key_identity() {
        // WS-4: bybit signs with an HMAC API key/secret, so the trace
        // must stamp venue=bybit / scheme=api-key.
        let surface = BybitPaperSurface::with_api(Arc::new(MockBybitClient::new()));
        assert_eq!(surface.venue(), "bybit");
        assert_eq!(surface.signing_scheme(), "api-key");
    }
}
