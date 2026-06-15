//! Byreal Perps executor — Stage 3 perpetual-futures execution via the
//! `@byreal-io/byreal-perps-cli` subprocess. Trades route to Hyperliquid;
//! identity/reputation lives on Mantle (ERC-8004). The M0 probe
//! (`probes/m0-byreal/`) verified the CLI primitives this wraps:
//! `account.info`, `order.market`, `position.list`, `close-market`.
//!
//! Structure mirrors [`crate::orderly`]: an inner [`ByrealPerpsApi`] trait is
//! the mockable seam (subprocess in prod, in-memory in tests) and
//! [`ByrealPerpsExecutor`] implements the venue-agnostic [`Executor`] trait on
//! top of it. The CLI is invoked as
//! `npx -y @byreal-io/byreal-perps-cli@latest <verb> ... -o json`, returning the
//! `{ success, meta, data }` envelope the probe documented.

use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use uuid::Uuid;
use xvision_core::trading::{Action, Direction, OpenPosition};
use xvision_core::{AssetSymbol, PortfolioState, RiskDecision};

use crate::executor::{ExecutionReceipt, Executor, ExecutorError};

const VENUE: &str = "byreal";

/// Order side for the perps CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByrealSide {
    Buy,
    Sell,
}

/// Account snapshot (`account.info`).
#[derive(Debug, Clone, PartialEq)]
pub struct ByrealAccount {
    pub equity_usd: f64,
}

/// One open perps position (`position.list`). `qty_signed` is positive for a
/// long, negative for a short — matching Hyperliquid's signed size convention.
#[derive(Debug, Clone, PartialEq)]
pub struct ByrealPosition {
    pub symbol: String,
    pub qty_signed: f64,
    pub avg_open_price: f64,
    pub mark_price: f64,
    pub leverage: Option<f64>,
    pub liq_price: Option<f64>,
    pub funding_paid_usd: f64,
    pub unrealized_pnl_usd: f64,
}

/// Acknowledgement of a placed order (`order.market` / `close-market`).
#[derive(Debug, Clone, PartialEq)]
pub struct ByrealOrderAck {
    pub venue_order_id: String,
    pub avg_fill_price: f64,
    pub filled_qty: f64,
}

/// The mockable seam. Each method maps to one verified perps-CLI primitive.
#[async_trait]
pub trait ByrealPerpsApi: Send + Sync {
    async fn account_info(&self) -> Result<ByrealAccount, ExecutorError>;
    async fn mark_price(&self, symbol: &str) -> Result<f64, ExecutorError>;
    async fn position_list(&self) -> Result<Vec<ByrealPosition>, ExecutorError>;
    /// Place a market order. `tp_price`/`sl_price`, when set, attach native
    /// take-profit / stop-loss bracket legs to the entry (the perps CLI's
    /// `--tp`/`--sl` flags). `client_id` is retained for receipt/tracing and
    /// mock assertions; the perps CLI exposes no client-order-id, so it is not
    /// forwarded — venue-side idempotency is best-effort (see grounding spec).
    async fn order_market(
        &self,
        symbol: &str,
        side: ByrealSide,
        qty: f64,
        reduce_only: bool,
        tp_price: Option<f64>,
        sl_price: Option<f64>,
        client_id: &str,
    ) -> Result<ByrealOrderAck, ExecutorError>;
    async fn close_market(&self, symbol: &str) -> Result<ByrealOrderAck, ExecutorError>;
    /// Set leverage for a coin (`position leverage <coin> <leverage>`).
    async fn set_leverage(&self, symbol: &str, leverage: f64) -> Result<(), ExecutorError>;
}

/// Map an `AssetSymbol` to the Hyperliquid market the perps CLI expects.
/// Hyperliquid markets are the bare coin ticker (`BTC`, `ETH`).
fn byreal_symbol_for(asset: AssetSymbol) -> String {
    asset.as_str().to_uppercase()
}

/// Byreal Perps executor, generic over the API seam so tests inject a mock.
pub struct ByrealPerpsExecutor<A> {
    api: A,
}

impl<A: ByrealPerpsApi> ByrealPerpsExecutor<A> {
    pub fn new(api: A) -> Self {
        Self { api }
    }

    fn build_receipt(
        cycle_id: Uuid,
        asset: AssetSymbol,
        ack: &ByrealOrderAck,
        equity_usd: f64,
    ) -> ExecutionReceipt {
        let notional = ack.filled_qty.abs() * ack.avg_fill_price;
        let filled_size_bps = if equity_usd > 0.0 && ack.avg_fill_price > 0.0 {
            ((notional / equity_usd) * 10_000.0).round() as u32
        } else {
            0
        };
        ExecutionReceipt {
            cycle_id,
            venue: VENUE.to_string(),
            venue_order_id: ack.venue_order_id.clone(),
            asset,
            filled_size_bps,
            avg_fill_price: ack.avg_fill_price,
            fee_bps: 0,
            submitted_at: Utc::now(),
            filled_at: Some(Utc::now()),
            note: None,
        }
    }

    fn no_position_receipt(asset: AssetSymbol) -> ExecutionReceipt {
        ExecutionReceipt {
            cycle_id: Uuid::nil(),
            venue: VENUE.to_string(),
            venue_order_id: String::new(),
            asset,
            filled_size_bps: 0,
            avg_fill_price: 0.0,
            fee_bps: 0,
            submitted_at: Utc::now(),
            filled_at: None,
            note: Some("no open position".to_string()),
        }
    }
}

#[async_trait]
impl<A: ByrealPerpsApi + 'static> Executor for ByrealPerpsExecutor<A> {
    async fn submit(&self, decision: &RiskDecision) -> Result<ExecutionReceipt, ExecutorError> {
        // 1. Bail on vetoed decisions.
        let td = match decision {
            RiskDecision::Vetoed { .. } => {
                return Err(ExecutorError::NotActionable("decision was vetoed".to_string()));
            }
            RiskDecision::Approved { decision: td, .. } => td,
            RiskDecision::Modified { modified: td, .. } => td,
        };

        let symbol = byreal_symbol_for(td.asset);

        // 2. Flat is not a submit; Close routes to close_position.
        match td.action {
            Action::Flat => {
                return Err(ExecutorError::NotActionable(
                    "flat decision is not a submit".to_string(),
                ));
            }
            Action::Close => return self.close_position(td.asset).await,
            Action::Buy | Action::Sell => {}
        }

        // 3. Size from notional: (size_bps/10_000) * equity / mark.
        let account = self.api.account_info().await?;
        let mark = self.api.mark_price(&symbol).await?;
        if mark <= 0.0 {
            return Err(ExecutorError::Internal(format!(
                "non-positive mark price for {symbol}"
            )));
        }
        let notional_usd = (td.size_bps as f64 / 10_000.0) * account.equity_usd;
        let qty = notional_usd / mark;
        if qty <= 0.0 {
            return Err(ExecutorError::Rejected(format!(
                "computed non-positive qty for {symbol} (notional ${notional_usd:.2} @ mark {mark})"
            )));
        }

        let side = match td.action {
            Action::Buy => ByrealSide::Buy,
            _ => ByrealSide::Sell,
        };

        // 4. Place the market order with native TP/SL brackets derived from the
        //    decision's stop/target percentages against the current mark.
        let (tp_price, sl_price) = bracket_prices(
            side,
            mark,
            Some(td.take_profit_pct as f64),
            Some(td.stop_loss_pct as f64),
        );
        let ack = self
            .api
            .order_market(
                &symbol,
                side,
                qty,
                false,
                tp_price,
                sl_price,
                &td.cycle_id.to_string(),
            )
            .await?;

        Ok(Self::build_receipt(
            td.cycle_id,
            td.asset,
            &ack,
            account.equity_usd,
        ))
    }

    async fn close_position(&self, asset: AssetSymbol) -> Result<ExecutionReceipt, ExecutorError> {
        let symbol = byreal_symbol_for(asset);
        let positions = self.api.position_list().await?;
        let Some(pos) = positions.iter().find(|p| p.symbol == symbol) else {
            return Ok(Self::no_position_receipt(asset));
        };
        if pos.qty_signed == 0.0 {
            return Ok(Self::no_position_receipt(asset));
        }
        let ack = self.api.close_market(&symbol).await?;
        let account = self.api.account_info().await?;
        Ok(Self::build_receipt(Uuid::nil(), asset, &ack, account.equity_usd))
    }

    async fn portfolio(&self) -> Result<PortfolioState, ExecutorError> {
        let (account, positions) = tokio::try_join!(self.api.account_info(), self.api.position_list())?;
        let equity = account.equity_usd;
        let mut open_positions = BTreeMap::new();
        for pos in &positions {
            if pos.qty_signed == 0.0 {
                continue;
            }
            let Ok(asset) = pos.symbol.parse::<AssetSymbol>() else {
                continue;
            };
            let direction = if pos.qty_signed > 0.0 {
                Direction::Long
            } else {
                Direction::Short
            };
            let notional = pos.qty_signed.abs() * pos.mark_price;
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
                    entry_price: pos.avg_open_price,
                    mark_price: pos.mark_price,
                    stop_loss_pct: 2.0,
                    take_profit_pct: 5.0,
                    opened_at: Utc::now(),
                    // Perps risk data straight from the venue — feeds the
                    // LiquidationDistanceGuard risk rule.
                    leverage: pos.leverage,
                    liq_price: pos.liq_price,
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

// ── perps-CLI argument construction (pure; unit-tested against v0.3.7) ───────
//
// These build the exact argv each verb maps to. They are split out from the
// subprocess `run` so the command surface is unit-testable without spawning the
// CLI. Verified against `@byreal-io/byreal-perps-cli@0.3.7 catalog`; see
// docs/superpowers/specs/2026-06-13-byreal-perps-cli-grounding.md.

fn order_market_args(
    side: ByrealSide,
    qty: f64,
    coin: &str,
    reduce_only: bool,
    tp_price: Option<f64>,
    sl_price: Option<f64>,
) -> Vec<String> {
    // Positional: `order market <side> <size> <coin>` (NOT --symbol/--side/--qty).
    let side_s = match side {
        ByrealSide::Buy => "buy",
        ByrealSide::Sell => "sell",
    };
    let mut a = vec![
        "order".to_string(),
        "market".to_string(),
        side_s.to_string(),
        format!("{qty}"),
        coin.to_string(),
    ];
    if reduce_only {
        a.push("--reduce-only".to_string());
    }
    if let Some(tp) = tp_price {
        a.push("--tp".to_string());
        a.push(format!("{tp}"));
    }
    if let Some(sl) = sl_price {
        a.push("--sl".to_string());
        a.push(format!("{sl}"));
    }
    a
}

/// Single-coin quote: `signal detail <coin>` (NOT `signal scan`, which is a
/// market-wide scan with no coin argument).
fn mark_price_args(coin: &str) -> Vec<String> {
    vec!["signal".to_string(), "detail".to_string(), coin.to_string()]
}

/// Close at market: `position close-market <coin>` (NOT top-level `close-market`).
fn close_market_args(coin: &str) -> Vec<String> {
    vec![
        "position".to_string(),
        "close-market".to_string(),
        coin.to_string(),
    ]
}

/// Set leverage: `position leverage <coin> <leverage>`.
fn set_leverage_args(coin: &str, leverage: f64) -> Vec<String> {
    vec![
        "position".to_string(),
        "leverage".to_string(),
        coin.to_string(),
        format!("{leverage}"),
    ]
}

/// Compute `(tp_price, sl_price)` bracket trigger prices for a perps entry from
/// a reference price, side, and tp/sl percentages — direction-aware, mirroring
/// [`crate::orderly::OrderlyLiveSurface`]: a long takes profit above and stops
/// below; a short is inverted. A `None`/non-positive percentage (or a
/// non-positive reference price) yields `None` for that leg.
fn bracket_prices(
    side: ByrealSide,
    reference_price: f64,
    tp_pct: Option<f64>,
    sl_pct: Option<f64>,
) -> (Option<f64>, Option<f64>) {
    if !(reference_price > 0.0) {
        return (None, None);
    }
    let tp = tp_pct.filter(|p| *p > 0.0).map(|p| match side {
        ByrealSide::Buy => reference_price * (1.0 + p / 100.0),
        ByrealSide::Sell => reference_price * (1.0 - p / 100.0),
    });
    let sl = sl_pct.filter(|p| *p > 0.0).map(|p| match side {
        ByrealSide::Buy => reference_price * (1.0 - p / 100.0),
        ByrealSide::Sell => reference_price * (1.0 + p / 100.0),
    });
    (tp, sl)
}

// ── Subprocess API implementation ──────────────────────────────────────────

/// The `{ success, meta, data }` envelope every perps-CLI command returns
/// (documented by the M0 probe).
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    // `Option<T>` is already treated as optional by serde (missing → None);
    // no `#[serde(default)]` so we avoid an unwanted `T: Default` bound.
    data: Option<T>,
    #[serde(default)]
    error: Option<String>,
}

/// Production [`ByrealPerpsApi`] that shells out to the perps CLI.
pub struct SubprocessByrealApi {
    /// Extra args injected before the verb (e.g. `--network`, `--account`).
    base_args: Vec<String>,
}

impl SubprocessByrealApi {
    /// Build from environment. `BYREAL_NETWORK` (default `mainnet`) and
    /// `BYREAL_ACCOUNT` are forwarded to the CLI; the signing key is read by
    /// the CLI itself from `BYREAL_PRIVATE_KEY` per its own contract.
    pub fn from_env() -> Result<Self, ExecutorError> {
        let mut base_args = Vec::new();
        if let Ok(net) = std::env::var("BYREAL_NETWORK") {
            base_args.push("--network".to_string());
            base_args.push(net);
        }
        if let Ok(acct) = std::env::var("BYREAL_ACCOUNT") {
            base_args.push("--account".to_string());
            base_args.push(acct);
        }
        Ok(Self { base_args })
    }

    async fn run<T: for<'de> Deserialize<'de>>(&self, verb_args: &[&str]) -> Result<T, ExecutorError> {
        use tokio::process::Command;
        let mut args: Vec<String> = vec!["-y".into(), "@byreal-io/byreal-perps-cli@latest".into()];
        for a in verb_args {
            args.push((*a).to_string());
        }
        args.extend(self.base_args.iter().cloned());
        args.push("-o".into());
        args.push("json".into());

        let child = Command::new("npx")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ExecutorError::Io(format!("spawn npx byreal-perps-cli: {e}")))?;

        let output = tokio::time::timeout(std::time::Duration::from_secs(60), child.wait_with_output())
            .await
            .map_err(|_| ExecutorError::Timeout("byreal-perps-cli timed out after 60s".to_string()))?
            .map_err(|e| ExecutorError::Io(format!("byreal-perps-cli process error: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ExecutorError::Rejected(format!(
                "byreal-perps-cli exited {}: {stderr}",
                output.status.code().unwrap_or(-1)
            )));
        }

        let env: Envelope<T> = serde_json::from_slice(&output.stdout)
            .map_err(|e| ExecutorError::Internal(format!("malformed CLI JSON: {e}")))?;
        if !env.success {
            return Err(ExecutorError::Rejected(
                env.error
                    .unwrap_or_else(|| "CLI returned success=false".to_string()),
            ));
        }
        env.data
            .ok_or_else(|| ExecutorError::Internal("CLI envelope missing data".to_string()))
    }
}

#[derive(Debug, Deserialize)]
struct AccountData {
    equity_usd: f64,
}

#[derive(Debug, Deserialize)]
struct MarkData {
    mark_price: f64,
}

#[derive(Debug, Deserialize)]
struct PositionData {
    symbol: String,
    qty_signed: f64,
    avg_open_price: f64,
    mark_price: f64,
    #[serde(default)]
    leverage: Option<f64>,
    #[serde(default)]
    liq_price: Option<f64>,
    #[serde(default)]
    funding_paid_usd: f64,
    #[serde(default)]
    unrealized_pnl_usd: f64,
}

#[derive(Debug, Deserialize)]
struct OrderData {
    venue_order_id: String,
    avg_fill_price: f64,
    filled_qty: f64,
}

#[async_trait]
impl ByrealPerpsApi for SubprocessByrealApi {
    async fn account_info(&self) -> Result<ByrealAccount, ExecutorError> {
        let d: AccountData = self.run(&["account", "info"]).await?;
        Ok(ByrealAccount {
            equity_usd: d.equity_usd,
        })
    }

    async fn mark_price(&self, symbol: &str) -> Result<f64, ExecutorError> {
        let args = mark_price_args(symbol);
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let d: MarkData = self.run(&argv).await?;
        Ok(d.mark_price)
    }

    async fn position_list(&self) -> Result<Vec<ByrealPosition>, ExecutorError> {
        let d: Vec<PositionData> = self.run(&["position", "list"]).await?;
        Ok(d.into_iter()
            .map(|p| ByrealPosition {
                symbol: p.symbol,
                qty_signed: p.qty_signed,
                avg_open_price: p.avg_open_price,
                mark_price: p.mark_price,
                leverage: p.leverage,
                liq_price: p.liq_price,
                funding_paid_usd: p.funding_paid_usd,
                unrealized_pnl_usd: p.unrealized_pnl_usd,
            })
            .collect())
    }

    async fn order_market(
        &self,
        symbol: &str,
        side: ByrealSide,
        qty: f64,
        reduce_only: bool,
        tp_price: Option<f64>,
        sl_price: Option<f64>,
        client_id: &str,
    ) -> Result<ByrealOrderAck, ExecutorError> {
        // The perps CLI exposes no client-order-id on `order market`; the param
        // is retained on the trait for the receipt/mock path but is not
        // forwarded. Venue-side idempotency is best-effort (grounding spec).
        let _ = client_id;
        let args = order_market_args(side, qty, symbol, reduce_only, tp_price, sl_price);
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let d: OrderData = self.run(&argv).await?;
        Ok(ByrealOrderAck {
            venue_order_id: d.venue_order_id,
            avg_fill_price: d.avg_fill_price,
            filled_qty: d.filled_qty,
        })
    }

    async fn close_market(&self, symbol: &str) -> Result<ByrealOrderAck, ExecutorError> {
        let args = close_market_args(symbol);
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        let d: OrderData = self.run(&argv).await?;
        Ok(ByrealOrderAck {
            venue_order_id: d.venue_order_id,
            avg_fill_price: d.avg_fill_price,
            filled_qty: d.filled_qty,
        })
    }

    async fn set_leverage(&self, symbol: &str, leverage: f64) -> Result<(), ExecutorError> {
        let args = set_leverage_args(symbol, leverage);
        let argv: Vec<&str> = args.iter().map(String::as_str).collect();
        // Only success matters; the command's data payload is not consumed.
        let _: serde_json::Value = self.run(&argv).await?;
        Ok(())
    }
}

// ── ByrealLiveSurface (BrokerSurface) ──────────────────────────────────────
//
// The live-eval engine drives execution through `BrokerSurface`, NOT the
// `Executor` trait. This adapter exposes the same `ByrealPerpsApi` seam to the
// live path so a live run can execute on Byreal (routing to Hyperliquid) while
// Alpaca supplies the market-data stream — exactly mirroring `OrderlyLiveSurface`.

use crate::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest, Side};

/// Map a venue asset string (`"BTC"`, `"BTC/USD"`) to the bare Hyperliquid coin
/// ticker the perps CLI expects.
fn byreal_symbol_for_str(asset: &str) -> anyhow::Result<String> {
    let sym: AssetSymbol = asset
        .parse()
        .map_err(|e| anyhow::anyhow!("byreal asset '{asset}': {e}"))?;
    Ok(byreal_symbol_for(sym))
}

/// Parse the optional `BYREAL_LEVERAGE` env var into a positive leverage.
/// Absent / unparseable / non-positive ⇒ `None` (leave account leverage as-is).
fn parse_env_leverage() -> Option<f64> {
    std::env::var("BYREAL_LEVERAGE")
        .ok()
        .and_then(|s| s.trim().parse::<f64>().ok())
        .filter(|l| *l > 0.0)
}

/// `BrokerSurface` over the Byreal perps CLI for live-eval runs.
pub struct ByrealLiveSurface<A = SubprocessByrealApi> {
    api: A,
    /// Leverage applied (via `position leverage <coin> <lev>`) before each
    /// entry. Sourced from `BYREAL_LEVERAGE`. `None` leaves the account's
    /// existing leverage untouched (the perps CLI's default).
    leverage: Option<f64>,
}

impl ByrealLiveSurface<SubprocessByrealApi> {
    /// Build from environment (`BYREAL_NETWORK`, `BYREAL_ACCOUNT`,
    /// `BYREAL_LEVERAGE`; the CLI reads `BYREAL_PRIVATE_KEY` itself).
    pub fn from_env() -> Result<Self, ExecutorError> {
        Ok(Self {
            api: SubprocessByrealApi::from_env()?,
            leverage: parse_env_leverage(),
        })
    }
}

impl<A: ByrealPerpsApi> ByrealLiveSurface<A> {
    pub fn new(api: A) -> Self {
        Self { api, leverage: None }
    }

    /// Set the leverage applied before each entry (`position leverage`).
    pub fn with_leverage(mut self, leverage: Option<f64>) -> Self {
        self.leverage = leverage;
        self
    }
}

#[async_trait]
impl<A: ByrealPerpsApi + 'static> BrokerSurface for ByrealLiveSurface<A> {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let symbol = byreal_symbol_for_str(&req.asset)?;
        if !(req.size > 0.0) {
            anyhow::bail!(
                "byreal order size must be positive for {} (got {})",
                req.asset,
                req.size
            );
        }
        let side = match req.side {
            Side::Buy => ByrealSide::Buy,
            Side::Sell => ByrealSide::Sell,
        };
        // Apply configured leverage (BYREAL_LEVERAGE) before the entry so the
        // position opens at the intended leverage. `None` ⇒ leave as-is.
        if let Some(lev) = self.leverage {
            self.api
                .set_leverage(&symbol, lev)
                .await
                .map_err(|e| anyhow::anyhow!("byreal set_leverage: {e}"))?;
        }
        // Native TP/SL brackets: the perps CLI's `order market --tp/--sl` attach
        // reduce-only bracket legs to the entry. Derive trigger prices from the
        // caller's stop/target percentages against the reference price (mirrors
        // OrderlyLiveSurface). Percentages left unset ⇒ no leg.
        let (tp_price, sl_price) = bracket_prices(
            side,
            req.reference_price_usd,
            req.take_profit_pct.map(|p| p as f64),
            req.stop_loss_pct.map(|p| p as f64),
        );
        let ack = self
            .api
            .order_market(
                &symbol,
                side,
                req.size,
                false,
                tp_price,
                sl_price,
                &req.idempotency_key,
            )
            .await
            .map_err(|e| anyhow::anyhow!("byreal order_market: {e}"))?;
        Ok(OrderConfirmation {
            broker_order_id: ack.venue_order_id,
            fill_price: (ack.avg_fill_price > 0.0).then_some(ack.avg_fill_price),
            fill_size: if ack.filled_qty > 0.0 {
                ack.filled_qty
            } else {
                req.size
            },
            fee: None,
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let symbol = byreal_symbol_for_str(asset)?;
        let positions = self
            .api
            .position_list()
            .await
            .map_err(|e| anyhow::anyhow!("byreal position_list: {e}"))?;
        Ok(positions
            .iter()
            .find(|p| p.symbol == symbol)
            .map(|p| p.qty_signed)
            .unwrap_or(0.0))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        let acct = self
            .api
            .account_info()
            .await
            .map_err(|e| anyhow::anyhow!("byreal account_info: {e}"))?;
        Ok(acct.equity_usd)
    }

    fn venue(&self) -> &str {
        "byreal"
    }

    fn signing_scheme(&self) -> &str {
        "cli"
    }

    fn is_perp_venue(&self) -> bool {
        true
    }
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::trading::{TraderDecision, VetoReason};

    fn buy_decision(asset: AssetSymbol, size_bps: u32) -> TraderDecision {
        TraderDecision {
            cycle_id: Uuid::nil(),
            action: Action::Buy,
            size_bps,
            direction: Direction::Long,
            stop_loss_pct: 2.0,
            take_profit_pct: 4.0,
            trader_summary: "Byreal perps test decision — long entry.".into(),
            asset,
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

    fn approved(td: TraderDecision) -> RiskDecision {
        RiskDecision::Approved {
            decision: td,
            warnings: vec![],
        }
    }

    fn vetoed() -> RiskDecision {
        RiskDecision::Vetoed {
            original: buy_decision(AssetSymbol::Btc, 500),
            reason: VetoReason::DailyLossCircuitBreaker,
        }
    }

    /// In-memory [`ByrealPerpsApi`] for deterministic executor tests.
    /// One `order_market` call captured by the mock, for bracket assertions.
    #[derive(Clone, Debug, PartialEq)]
    struct RecordedOrder {
        qty: f64,
        reduce_only: bool,
        tp_price: Option<f64>,
        sl_price: Option<f64>,
    }

    #[derive(Default, Clone)]
    struct MockByrealApi {
        equity_usd: f64,
        mark: f64,
        positions: Vec<ByrealPosition>,
        order_ack: Option<ByrealOrderAck>,
        // Shared (Arc) so a cloned mock still observes recorded calls.
        orders: std::sync::Arc<std::sync::Mutex<Vec<RecordedOrder>>>,
        leverage_calls: std::sync::Arc<std::sync::Mutex<Vec<(String, f64)>>>,
    }

    #[async_trait]
    impl ByrealPerpsApi for MockByrealApi {
        async fn account_info(&self) -> Result<ByrealAccount, ExecutorError> {
            Ok(ByrealAccount {
                equity_usd: self.equity_usd,
            })
        }
        async fn mark_price(&self, _symbol: &str) -> Result<f64, ExecutorError> {
            Ok(self.mark)
        }
        async fn position_list(&self) -> Result<Vec<ByrealPosition>, ExecutorError> {
            Ok(self.positions.clone())
        }
        async fn order_market(
            &self,
            _symbol: &str,
            _side: ByrealSide,
            qty: f64,
            reduce_only: bool,
            tp_price: Option<f64>,
            sl_price: Option<f64>,
            _client_id: &str,
        ) -> Result<ByrealOrderAck, ExecutorError> {
            self.orders.lock().unwrap().push(RecordedOrder {
                qty,
                reduce_only,
                tp_price,
                sl_price,
            });
            Ok(self.order_ack.clone().unwrap_or(ByrealOrderAck {
                venue_order_id: "mock-ord".into(),
                avg_fill_price: self.mark,
                filled_qty: qty,
            }))
        }
        async fn close_market(&self, _symbol: &str) -> Result<ByrealOrderAck, ExecutorError> {
            Ok(self.order_ack.clone().unwrap_or(ByrealOrderAck {
                venue_order_id: "mock-close".into(),
                avg_fill_price: self.mark,
                filled_qty: 0.0,
            }))
        }
        async fn set_leverage(&self, symbol: &str, leverage: f64) -> Result<(), ExecutorError> {
            self.leverage_calls
                .lock()
                .unwrap()
                .push((symbol.to_string(), leverage));
            Ok(())
        }
    }

    fn long_position(symbol: &str, qty: f64) -> ByrealPosition {
        ByrealPosition {
            symbol: symbol.to_string(),
            qty_signed: qty,
            avg_open_price: 60_000.0,
            mark_price: 61_000.0,
            leverage: Some(3.0),
            liq_price: Some(45_000.0),
            funding_paid_usd: 1.2,
            unrealized_pnl_usd: 10.0,
        }
    }

    #[tokio::test]
    async fn submit_vetoed_is_not_actionable() {
        let exec = ByrealPerpsExecutor::new(MockByrealApi::default());
        let d = vetoed();
        let err = exec.submit(&d).await.unwrap_err();
        assert!(matches!(err, ExecutorError::NotActionable(_)));
    }

    #[tokio::test]
    async fn submit_market_returns_byreal_receipt() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 60_000.0,
            order_ack: Some(ByrealOrderAck {
                venue_order_id: "ord-123".into(),
                avg_fill_price: 60_000.0,
                filled_qty: 0.01,
            }),
            ..Default::default()
        };
        let exec = ByrealPerpsExecutor::new(api);
        let d = approved(buy_decision(AssetSymbol::Btc, 500));
        let r = exec.submit(&d).await.unwrap();
        assert_eq!(r.venue, "byreal");
        assert_eq!(r.venue_order_id, "ord-123");
        assert!(r.avg_fill_price > 0.0);
    }

    #[tokio::test]
    async fn close_position_with_open_long_submits_close() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 61_000.0,
            positions: vec![long_position("BTC", 0.01)],
            ..Default::default()
        };
        let exec = ByrealPerpsExecutor::new(api);
        let r = exec.close_position(AssetSymbol::Btc).await.unwrap();
        assert_eq!(r.venue, "byreal");
    }

    #[tokio::test]
    async fn close_position_without_position_is_noop() {
        let exec = ByrealPerpsExecutor::new(MockByrealApi {
            mark: 1.0,
            ..Default::default()
        });
        let r = exec.close_position(AssetSymbol::Btc).await.unwrap();
        assert_eq!(r.note.as_deref(), Some("no open position"));
        assert_eq!(r.filled_size_bps, 0);
    }

    #[tokio::test]
    async fn portfolio_maps_signed_qty_to_direction() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 61_000.0,
            positions: vec![long_position("BTC", 0.01)],
            ..Default::default()
        };
        let exec = ByrealPerpsExecutor::new(api);
        let pf = exec.portfolio().await.unwrap();
        let pos = pf.open_positions.get(&AssetSymbol::Btc).expect("btc position");
        assert_eq!(pos.direction, Direction::Long);
    }

    // ── ByrealLiveSurface (BrokerSurface) ──────────────────────────────────
    use crate::broker_surface::{BrokerSurface, OrderRequest, Side};

    fn order_req(asset: &str, side: Side, size: f64) -> OrderRequest {
        OrderRequest {
            asset: asset.into(),
            side,
            size,
            reference_price_usd: 60_000.0,
            stop_loss_pct: Some(2.0),
            take_profit_pct: Some(4.0),
            idempotency_key: "cycle-xyz".into(),
        }
    }

    #[tokio::test]
    async fn live_surface_submit_order_returns_confirmation() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 60_000.0,
            order_ack: Some(ByrealOrderAck {
                venue_order_id: "ord-789".into(),
                avg_fill_price: 60_000.0,
                filled_qty: 0.01,
            }),
            ..Default::default()
        };
        let surface = ByrealLiveSurface::new(api);
        let conf = surface
            .submit_order(order_req("BTC/USD", Side::Buy, 0.01))
            .await
            .unwrap();
        assert_eq!(conf.broker_order_id, "ord-789");
        assert_eq!(conf.fill_price, Some(60_000.0));
        assert_eq!(conf.fill_size, 0.01);
    }

    #[tokio::test]
    async fn live_surface_position_and_balance() {
        let api = MockByrealApi {
            equity_usd: 12_345.0,
            mark: 61_000.0,
            positions: vec![long_position("BTC", 0.02)],
            ..Default::default()
        };
        let surface = ByrealLiveSurface::new(api);
        assert_eq!(surface.position("BTC/USD").await.unwrap(), 0.02);
        assert_eq!(surface.position("ETH/USD").await.unwrap(), 0.0);
        assert_eq!(surface.balance().await.unwrap(), 12_345.0);
    }

    #[tokio::test]
    async fn live_surface_reports_byreal_venue_identity() {
        // WS-4: the live order path stamps the broker's real venue +
        // signing scheme onto the trace (broker_call_started / order_signed)
        // instead of the hardcoded "live"/"broker" placeholders.
        let surface = ByrealLiveSurface::new(MockByrealApi::default());
        assert_eq!(surface.venue(), "byreal");
        assert_eq!(surface.signing_scheme(), "cli");
    }

    #[tokio::test]
    async fn live_surface_rejects_nonpositive_size() {
        let surface = ByrealLiveSurface::new(MockByrealApi::default());
        assert!(surface
            .submit_order(order_req("BTC", Side::Buy, 0.0))
            .await
            .is_err());
    }

    // ── perps-CLI command grounding + native TP/SL brackets (S3) ────────────

    fn argv(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    #[test]
    fn order_market_args_are_positional_not_flagged() {
        // Real CLI: `order market <side> <size> <coin>` — NOT --symbol/--side/--qty/--client-id.
        let a = order_market_args(ByrealSide::Buy, 0.01, "BTC", false, None, None);
        assert_eq!(argv(&a), vec!["order", "market", "buy", "0.01", "BTC"]);
        assert!(!a
            .iter()
            .any(|x| x == "--symbol" || x == "--side" || x == "--qty" || x == "--client-id"));
    }

    #[test]
    fn order_market_args_append_reduce_only_then_brackets() {
        let a = order_market_args(ByrealSide::Sell, 2.5, "ETH", true, Some(1800.0), Some(2200.0));
        assert_eq!(
            argv(&a),
            vec![
                "order",
                "market",
                "sell",
                "2.5",
                "ETH",
                "--reduce-only",
                "--tp",
                "1800",
                "--sl",
                "2200"
            ]
        );
    }

    #[test]
    fn mark_price_uses_signal_detail_not_scan() {
        assert_eq!(argv(&mark_price_args("BTC")), vec!["signal", "detail", "BTC"]);
    }

    #[test]
    fn close_market_uses_position_subcommand() {
        assert_eq!(
            argv(&close_market_args("BTC")),
            vec!["position", "close-market", "BTC"]
        );
    }

    #[test]
    fn set_leverage_args_shape() {
        assert_eq!(
            argv(&set_leverage_args("BTC", 5.0)),
            vec!["position", "leverage", "BTC", "5"]
        );
    }

    #[test]
    fn bracket_prices_are_direction_aware() {
        let approx = |got: Option<f64>, want: f64| {
            let g = got.expect("expected a bracket price");
            assert!((g - want).abs() < 1e-9, "got {g}, want {want}");
        };
        // Long: TP above entry, SL below.
        let (tp, sl) = bracket_prices(ByrealSide::Buy, 100.0, Some(10.0), Some(5.0));
        approx(tp, 110.0);
        approx(sl, 95.0);
        // Short: inverted.
        let (tp, sl) = bracket_prices(ByrealSide::Sell, 100.0, Some(10.0), Some(5.0));
        approx(tp, 90.0);
        approx(sl, 105.0);
        // Missing pct or non-positive reference ⇒ no leg.
        assert_eq!(bracket_prices(ByrealSide::Buy, 100.0, None, None), (None, None));
        assert_eq!(
            bracket_prices(ByrealSide::Buy, 0.0, Some(10.0), Some(5.0)),
            (None, None)
        );
    }

    #[tokio::test]
    async fn live_surface_attaches_tp_sl_brackets_to_entry() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 60_000.0,
            ..Default::default()
        };
        let orders = api.orders.clone();
        let surface = ByrealLiveSurface::new(api);
        // order_req: ref 60_000, tp 4%, sl 2%, Buy ⇒ tp 62_400, sl 58_800.
        surface
            .submit_order(order_req("BTC", Side::Buy, 0.01))
            .await
            .unwrap();
        let recorded = orders.lock().unwrap();
        assert_eq!(recorded.len(), 1, "exactly one entry order");
        let o = &recorded[0];
        assert!(
            (o.tp_price.unwrap() - 62_400.0).abs() < 1e-6,
            "tp {:?}",
            o.tp_price
        );
        assert!(
            (o.sl_price.unwrap() - 58_800.0).abs() < 1e-6,
            "sl {:?}",
            o.sl_price
        );
        assert!(!o.reduce_only, "the entry leg is not reduce-only");
    }

    #[tokio::test]
    async fn executor_submit_attaches_brackets() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 60_000.0,
            ..Default::default()
        };
        let orders = api.orders.clone();
        let exec = ByrealPerpsExecutor::new(api);
        // buy_decision: sl 2%, tp 4% ⇒ against mark 60_000, tp 62_400, sl 58_800.
        exec.submit(&approved(buy_decision(AssetSymbol::Btc, 500)))
            .await
            .unwrap();
        let recorded = orders.lock().unwrap();
        assert_eq!(recorded.len(), 1);
        assert!((recorded[0].tp_price.unwrap() - 62_400.0).abs() < 1e-6);
        assert!((recorded[0].sl_price.unwrap() - 58_800.0).abs() < 1e-6);
    }

    #[tokio::test]
    async fn live_surface_sets_leverage_before_entry_when_configured() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 60_000.0,
            ..Default::default()
        };
        let lev_calls = api.leverage_calls.clone();
        let surface = ByrealLiveSurface::new(api).with_leverage(Some(5.0));
        surface
            .submit_order(order_req("BTC", Side::Buy, 0.01))
            .await
            .unwrap();
        let calls = lev_calls.lock().unwrap();
        assert_eq!(calls.len(), 1, "leverage should be set once before the entry");
        assert_eq!(calls[0].0, "BTC");
        assert_eq!(calls[0].1, 5.0);
    }

    #[tokio::test]
    async fn live_surface_leaves_leverage_untouched_when_unset() {
        let api = MockByrealApi {
            equity_usd: 10_000.0,
            mark: 60_000.0,
            ..Default::default()
        };
        let lev_calls = api.leverage_calls.clone();
        // No `with_leverage` ⇒ leverage stays None ⇒ no set_leverage call.
        let surface = ByrealLiveSurface::new(api);
        surface
            .submit_order(order_req("BTC", Side::Buy, 0.01))
            .await
            .unwrap();
        assert!(
            lev_calls.lock().unwrap().is_empty(),
            "no configured leverage ⇒ account leverage left untouched"
        );
    }
}
