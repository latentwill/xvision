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
    async fn order_market(
        &self,
        symbol: &str,
        side: ByrealSide,
        qty: f64,
        reduce_only: bool,
        client_id: &str,
    ) -> Result<ByrealOrderAck, ExecutorError>;
    async fn close_market(&self, symbol: &str) -> Result<ByrealOrderAck, ExecutorError>;
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

        // 4. Place the market order (client_id = cycle_id for idempotency).
        let ack = self
            .api
            .order_market(&symbol, side, qty, false, &td.cycle_id.to_string())
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
        let d: MarkData = self.run(&["signal", "scan", "--symbol", symbol]).await?;
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
        client_id: &str,
    ) -> Result<ByrealOrderAck, ExecutorError> {
        let side_s = match side {
            ByrealSide::Buy => "buy",
            ByrealSide::Sell => "sell",
        };
        let qty_s = format!("{qty}");
        let mut verb: Vec<&str> = vec![
            "order",
            "market",
            "--symbol",
            symbol,
            "--side",
            side_s,
            "--qty",
            &qty_s,
            "--client-id",
            client_id,
        ];
        if reduce_only {
            verb.push("--reduce-only");
        }
        let d: OrderData = self.run(&verb).await?;
        Ok(ByrealOrderAck {
            venue_order_id: d.venue_order_id,
            avg_fill_price: d.avg_fill_price,
            filled_qty: d.filled_qty,
        })
    }

    async fn close_market(&self, symbol: &str) -> Result<ByrealOrderAck, ExecutorError> {
        let d: OrderData = self.run(&["close-market", "--symbol", symbol]).await?;
        Ok(ByrealOrderAck {
            venue_order_id: d.venue_order_id,
            avg_fill_price: d.avg_fill_price,
            filled_qty: d.filled_qty,
        })
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

/// `BrokerSurface` over the Byreal perps CLI for live-eval runs.
pub struct ByrealLiveSurface<A = SubprocessByrealApi> {
    api: A,
}

impl ByrealLiveSurface<SubprocessByrealApi> {
    /// Build from environment (`BYREAL_NETWORK`, `BYREAL_ACCOUNT`; the CLI reads
    /// `BYREAL_PRIVATE_KEY` itself).
    pub fn from_env() -> Result<Self, ExecutorError> {
        Ok(Self {
            api: SubprocessByrealApi::from_env()?,
        })
    }
}

impl<A: ByrealPerpsApi> ByrealLiveSurface<A> {
    pub fn new(api: A) -> Self {
        Self { api }
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
        // NOTE: SL/TP brackets are not yet supported through the perps-CLI seam
        // (OrderlyLiveSurface places reduce-only algo legs; the byreal CLI
        // adapter does not expose that yet). The entry order stands on its own;
        // bracket support is a follow-up.
        let ack = self
            .api
            .order_market(&symbol, side, req.size, false, &req.idempotency_key)
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
    #[derive(Default, Clone)]
    struct MockByrealApi {
        equity_usd: f64,
        mark: f64,
        positions: Vec<ByrealPosition>,
        order_ack: Option<ByrealOrderAck>,
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
            _reduce_only: bool,
            _client_id: &str,
        ) -> Result<ByrealOrderAck, ExecutorError> {
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
    async fn live_surface_rejects_nonpositive_size() {
        let surface = ByrealLiveSurface::new(MockByrealApi::default());
        assert!(surface
            .submit_order(order_req("BTC", Side::Buy, 0.0))
            .await
            .is_err());
    }
}
