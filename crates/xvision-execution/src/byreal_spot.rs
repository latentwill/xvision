//! `ByrealSpotSurface` — a `BrokerSurface` over `@byreal-io/byreal-cli` for
//! Solana spot (curated SPL + xStocks). Buy = USDC→token swap, sell =
//! token→USDC swap. Spot is Long/Flat only: no shorting, no leverage. Mirrors
//! `byreal_clmm.rs` for the subprocess seam.
//!
//! Custody: the CLI manages the wallet keystore in `~/.config/byreal/keys/`;
//! this surface never reads or logs key material.
//!
//! CLI surface grounded in docs/superpowers/specs/2026-06-15-byreal-spot-cli-grounding.md
//! (byreal-cli v0.3.6).

use async_trait::async_trait;
use serde::Deserialize;

use crate::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest, Side};
use crate::executor::ExecutorError;
use xvision_core::config::SpotAssetConfig;

/// Whether a swap is a no-funds preview (`--dry-run`) or a real, confirmed
/// swap (`--confirm`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ByrealSpotMode {
    Preview,
    Live,
}

impl ByrealSpotMode {
    /// The byreal-cli flag this mode emits on `swap execute`.
    fn swap_flag(self) -> &'static str {
        match self {
            ByrealSpotMode::Preview => "--dry-run",
            ByrealSpotMode::Live => "--confirm",
        }
    }
}

/// Outcome of a `swap execute` (preview or live). Field names match the
/// byreal-cli v0.3.6 `data` payload (camelCase strings) — see grounding doc.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapResult {
    /// "dry-run" for a preview; an execution mode for a confirmed swap.
    #[serde(default)]
    pub mode: Option<String>,
    /// Stable Byreal order id (present on dry-run and confirm).
    #[serde(default)]
    pub order_id: Option<String>,
    /// Tx signature; empty on dry-run, populated on `--confirm`.
    #[serde(default)]
    pub transaction: Option<String>,
    /// Output received in display units (string, e.g. "0.140693755").
    #[serde(default)]
    pub ui_out_amount: Option<String>,
    /// Price-impact percent as a string (e.g. "0.0464"); NOT bps.
    #[serde(default)]
    pub price_impact_pct: Option<String>,
}

/// Mockable seam over the `byreal-cli` subprocess: swap, token price, balance.
#[async_trait]
pub trait ByrealSpotApi: Send + Sync {
    /// Execute (or preview) a swap of `amount` UI input-mint units into the
    /// output mint, with `slippage_bps` tolerance, in the given mode.
    async fn swap(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: f64,
        slippage_bps: u32,
        mode: ByrealSpotMode,
    ) -> Result<SwapResult, ExecutorError>;

    /// Latest token price in USD for a mint (marks + base-size calc).
    async fn token_price(&self, mint: &str) -> Result<f64, ExecutorError>;

    /// Wallet balance for a mint, in display units (token balance, or USDC for
    /// the quote mint). Requires a configured byreal-cli wallet keystore.
    async fn token_balance(&self, mint: &str) -> Result<f64, ExecutorError>;
}

/// `{ success, data, error }` envelope from `byreal-cli -o json` (the `meta`
/// field present in v0.3.6 output is ignored).
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    data: Option<T>,
    #[serde(default)]
    error: Option<String>,
}

/// `data` of `tokens list --search <mint> -o json`.
#[derive(Debug, Deserialize)]
struct TokensListData {
    tokens: Vec<TokenEntry>,
}
#[derive(Debug, Deserialize)]
struct TokenEntry {
    price_usd: f64,
}

/// `data` of `wallet balance -o json`. SHAPE UNCONFIRMED (needs a configured
/// keystore, absent during grounding) — assumed `{ tokens: [{ mint, uiAmount }] }`.
/// GROUNDING: verify field names at the first authenticated run and adjust.
#[derive(Debug, Deserialize)]
struct WalletBalanceData {
    #[serde(default)]
    tokens: Vec<WalletBalanceEntry>,
}
#[derive(Debug, Deserialize)]
struct WalletBalanceEntry {
    mint: String,
    #[serde(alias = "uiAmount", alias = "amount", default)]
    ui_amount: f64,
}

/// Production `ByrealSpotApi` that shells out to `npx -y @byreal-io/byreal-cli`.
pub struct SubprocessByrealSpotApi {
    base_args: Vec<String>,
}

impl SubprocessByrealSpotApi {
    /// Build from env. Reads `BYREAL_SPOT_NETWORK` (optional) → `--network`.
    /// The wallet keystore is owned by the CLI (no key env var).
    pub fn from_env() -> Self {
        let mut base_args = Vec::new();
        if let Ok(net) = std::env::var("BYREAL_SPOT_NETWORK") {
            if !net.is_empty() {
                base_args.push("--network".to_string());
                base_args.push(net);
            }
        }
        Self { base_args }
    }

    async fn run<T: for<'de> Deserialize<'de>>(
        &self,
        verb_args: &[String],
    ) -> Result<Option<T>, ExecutorError> {
        use tokio::process::Command;
        let mut args: Vec<String> = vec!["-y".into(), "@byreal-io/byreal-cli@latest".into()];
        args.extend(verb_args.iter().cloned());
        args.extend(self.base_args.iter().cloned());
        args.push("-o".into());
        args.push("json".into());

        let child = Command::new("npx")
            .args(&args)
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| ExecutorError::Io(format!("spawn npx byreal-cli: {e}")))?;

        let output = tokio::time::timeout(std::time::Duration::from_secs(120), child.wait_with_output())
            .await
            .map_err(|_| ExecutorError::Timeout("byreal-cli timed out after 120s".into()))?
            .map_err(|e| ExecutorError::Io(format!("byreal-cli process error: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(ExecutorError::Rejected(format!(
                "byreal-cli exited {}: {stderr}",
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
        Ok(env.data)
    }
}

#[async_trait]
impl ByrealSpotApi for SubprocessByrealSpotApi {
    async fn swap(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: f64,
        slippage_bps: u32,
        mode: ByrealSpotMode,
    ) -> Result<SwapResult, ExecutorError> {
        // swap execute --input-mint <m> --output-mint <m> --amount <ui>
        //   --slippage <bps> (--dry-run | --confirm). swap-mode defaults to "in"
        //   (amount = input). Decimals auto-resolved by the CLI.
        let args = vec![
            "swap".into(),
            "execute".into(),
            "--input-mint".into(),
            input_mint.into(),
            "--output-mint".into(),
            output_mint.into(),
            "--amount".into(),
            amount.to_string(),
            "--slippage".into(),
            slippage_bps.to_string(),
            mode.swap_flag().into(),
        ];
        self.run::<SwapResult>(&args)
            .await?
            .ok_or_else(|| ExecutorError::Internal("swap envelope missing data".into()))
    }

    async fn token_price(&self, mint: &str) -> Result<f64, ExecutorError> {
        // tokens list --search <mint> → data.tokens[0].price_usd
        let args = vec!["tokens".into(), "list".into(), "--search".into(), mint.into()];
        let data = self
            .run::<TokensListData>(&args)
            .await?
            .ok_or_else(|| ExecutorError::Internal("tokens list envelope missing data".into()))?;
        data.tokens
            .first()
            .map(|t| t.price_usd)
            .ok_or_else(|| ExecutorError::Rejected(format!("no byreal token for mint {mint}")))
    }

    async fn token_balance(&self, mint: &str) -> Result<f64, ExecutorError> {
        // wallet balance (no --mint filter); filter client-side by mint.
        // GROUNDING: JSON shape unconfirmed — verify at first authenticated run.
        let args = vec!["wallet".into(), "balance".into()];
        let data = self
            .run::<WalletBalanceData>(&args)
            .await?
            .ok_or_else(|| ExecutorError::Internal("wallet balance envelope missing data".into()))?;
        Ok(data
            .tokens
            .iter()
            .find(|e| e.mint == mint)
            .map(|e| e.ui_amount)
            .unwrap_or(0.0))
    }
}

// ── ByrealSpotSurface ─────────────────────────────────────────────────────────

/// Hard cap on slippage; byreal-cli warns above 200 bps, we refuse above it.
const MAX_SLIPPAGE_BPS: u32 = 200;

/// `BrokerSurface` over `byreal-cli` for curated Solana spot. Buy = USDC→token,
/// sell = token→USDC. Long/Flat only (no shorting). Holds the curated set for
/// symbol→mint resolution and the quote (USDC) mint.
pub struct ByrealSpotSurface<A = SubprocessByrealSpotApi> {
    api: A,
    assets: SpotAssetConfig,
    mode: ByrealSpotMode,
    slippage_bps: u32,
}

impl<A: ByrealSpotApi> ByrealSpotSurface<A> {
    /// Defaults to `Preview` (no funds) and a conservative 100 bps slippage.
    pub fn new(api: A, assets: SpotAssetConfig) -> Self {
        Self {
            api,
            assets,
            mode: ByrealSpotMode::Preview,
            slippage_bps: 100,
        }
    }
    pub fn with_mode(mut self, mode: ByrealSpotMode) -> Self {
        self.mode = mode;
        self
    }
    pub fn with_slippage_bps(mut self, bps: u32) -> Self {
        self.slippage_bps = bps;
        self
    }
}

#[async_trait]
impl<A: ByrealSpotApi + Send + Sync + 'static> BrokerSurface for ByrealSpotSurface<A> {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        if self.slippage_bps > MAX_SLIPPAGE_BPS {
            anyhow::bail!(
                "byreal_spot slippage {} bps exceeds the {} bps cap",
                self.slippage_bps,
                MAX_SLIPPAGE_BPS
            );
        }
        if !(req.size > 0.0) {
            anyhow::bail!("byreal_spot order size must be positive (got {})", req.size);
        }
        let entry = self
            .assets
            .resolve(&req.asset)
            .ok_or_else(|| anyhow::anyhow!("byreal_spot: '{}' is not in the curated set", req.asset))?;
        let usdc = self.assets.usdc_mint.as_str();

        let (input_mint, output_mint, amount) = match req.side {
            Side::Buy => (usdc, entry.mint.as_str(), req.size * req.reference_price_usd),
            Side::Sell => {
                let amount = if self.mode == ByrealSpotMode::Live {
                    // Long/Flat: refuse a real sell with no position (no shorting).
                    let pos = self
                        .api
                        .token_balance(&entry.mint)
                        .await
                        .map_err(|e| anyhow::anyhow!("byreal_spot balance: {e}"))?;
                    if pos <= 0.0 {
                        anyhow::bail!(
                            "byreal_spot broker_unsupported: short_open is not supported (no {} position to sell)",
                            req.asset
                        );
                    }
                    req.size.min(pos)
                } else {
                    // Preview/forward-test: no wallet; simulate the sell as requested.
                    req.size
                };
                (entry.mint.as_str(), usdc, amount)
            }
        };

        let res = self
            .api
            .swap(input_mint, output_mint, amount, self.slippage_bps, self.mode)
            .await
            .map_err(|e| anyhow::anyhow!("byreal_spot swap: {e}"))?;

        let broker_order_id = res
            .order_id
            .or_else(|| res.transaction.filter(|s| !s.is_empty()))
            .unwrap_or_else(|| format!("preview-{}", req.idempotency_key));
        Ok(OrderConfirmation {
            broker_order_id,
            fill_price: (req.reference_price_usd > 0.0).then_some(req.reference_price_usd),
            fill_size: req.size,
            fee: None,
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let entry = self
            .assets
            .resolve(asset)
            .ok_or_else(|| anyhow::anyhow!("byreal_spot: '{asset}' is not in the curated set"))?;
        self.api
            .token_balance(&entry.mint)
            .await
            .map_err(|e| anyhow::anyhow!("byreal_spot position: {e}"))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        self.api
            .token_balance(&self.assets.usdc_mint)
            .await
            .map_err(|e| anyhow::anyhow!("byreal_spot balance: {e}"))
    }

    fn venue(&self) -> &str {
        "byreal_spot"
    }
    fn signing_scheme(&self) -> &str {
        "cli"
    }
    fn is_perp_venue(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use xvision_core::config::{SpotAssetConfig, SpotAssetEntry, SpotAssetKind};

    #[test]
    fn mode_maps_to_dry_run_or_confirm() {
        assert_eq!(ByrealSpotMode::Preview.swap_flag(), "--dry-run");
        assert_eq!(ByrealSpotMode::Live.swap_flag(), "--confirm");
    }

    // ── Mock ByrealSpotApi for ByrealSpotSurface tests ───────────────────────

    #[derive(Clone)]
    struct RecordedSwap {
        input_mint: String,
        output_mint: String,
        amount: f64,
        slippage_bps: u32,
        mode: ByrealSpotMode,
    }

    #[derive(Default, Clone)]
    struct MockSpotApi {
        price: f64,
        position: f64,
        swaps: Arc<Mutex<Vec<RecordedSwap>>>,
    }

    #[async_trait]
    impl ByrealSpotApi for MockSpotApi {
        async fn swap(
            &self,
            input_mint: &str,
            output_mint: &str,
            amount: f64,
            slippage_bps: u32,
            mode: ByrealSpotMode,
        ) -> Result<SwapResult, ExecutorError> {
            self.swaps.lock().unwrap().push(RecordedSwap {
                input_mint: input_mint.into(),
                output_mint: output_mint.into(),
                amount,
                slippage_bps,
                mode,
            });
            Ok(SwapResult {
                mode: Some("dry-run".into()),
                order_id: Some("mock-ord".into()),
                transaction: Some(String::new()),
                ui_out_amount: Some("1.0".into()),
                price_impact_pct: Some("0.01".into()),
            })
        }
        async fn token_price(&self, _mint: &str) -> Result<f64, ExecutorError> {
            Ok(self.price)
        }
        async fn token_balance(&self, _mint: &str) -> Result<f64, ExecutorError> {
            Ok(self.position)
        }
    }

    fn curated() -> SpotAssetConfig {
        SpotAssetConfig {
            usdc_mint: "USDC1111111111111111111111111111111111111111".into(),
            assets: vec![SpotAssetEntry {
                symbol: "SOL".into(),
                mint: "So11111111111111111111111111111111111111112".into(),
                kind: SpotAssetKind::Spl,
                decimals: 9,
            }],
        }
    }

    fn req(side: Side, size: f64) -> OrderRequest {
        OrderRequest {
            asset: "SOL".into(),
            side,
            size,
            reference_price_usd: 150.0,
            stop_loss_pct: None,
            take_profit_pct: None,
            idempotency_key: "cycle-1".into(),
        }
    }

    #[tokio::test]
    async fn surface_metadata_is_spot_not_perp() {
        let s = ByrealSpotSurface::new(
            MockSpotApi {
                price: 150.0,
                ..Default::default()
            },
            curated(),
        );
        assert_eq!(s.venue(), "byreal_spot");
        assert_eq!(s.signing_scheme(), "cli");
        assert!(!s.is_perp_venue());
    }

    #[tokio::test]
    async fn buy_swaps_usdc_into_token() {
        let api = MockSpotApi {
            price: 150.0,
            ..Default::default()
        };
        let swaps = api.swaps.clone();
        let s = ByrealSpotSurface::new(api, curated())
            .with_mode(ByrealSpotMode::Live)
            .with_slippage_bps(100);
        let conf = s.submit_order(req(Side::Buy, 2.0)).await.unwrap();
        let rec = swaps.lock().unwrap()[0].clone();
        assert_eq!(rec.input_mint, "USDC1111111111111111111111111111111111111111");
        assert_eq!(rec.output_mint, "So11111111111111111111111111111111111111112");
        assert_eq!(rec.amount, 300.0); // 2.0 * 150.0 USDC notional
        assert_eq!(rec.slippage_bps, 100);
        assert_eq!(rec.mode, ByrealSpotMode::Live);
        assert_eq!(conf.broker_order_id, "mock-ord");
    }

    #[tokio::test]
    async fn sell_with_position_swaps_token_into_usdc() {
        let api = MockSpotApi {
            price: 150.0,
            position: 5.0,
            ..Default::default()
        };
        let swaps = api.swaps.clone();
        let s = ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live);
        s.submit_order(req(Side::Sell, 5.0)).await.unwrap();
        let rec = swaps.lock().unwrap()[0].clone();
        assert_eq!(rec.input_mint, "So11111111111111111111111111111111111111112");
        assert_eq!(rec.output_mint, "USDC1111111111111111111111111111111111111111");
        assert_eq!(rec.amount, 5.0);
    }

    #[tokio::test]
    async fn live_sell_without_position_is_rejected_no_shorting() {
        let api = MockSpotApi {
            price: 150.0,
            position: 0.0,
            ..Default::default()
        };
        let swaps = api.swaps.clone();
        let s = ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live);
        let err = s.submit_order(req(Side::Sell, 1.0)).await.unwrap_err();
        assert!(
            err.to_string().contains("short_open is not supported"),
            "must refuse shorting; got: {err}"
        );
        assert_eq!(swaps.lock().unwrap().len(), 0, "no swap on a rejected short");
    }

    #[tokio::test]
    async fn preview_sell_skips_balance_check() {
        // Preview/forward-test has no wallet; a sell must still preview (no balance gate).
        let api = MockSpotApi {
            price: 150.0,
            position: 0.0,
            ..Default::default()
        };
        let swaps = api.swaps.clone();
        let s = ByrealSpotSurface::new(api, curated()); // default = Preview
        s.submit_order(req(Side::Sell, 1.0)).await.unwrap();
        let rec = swaps.lock().unwrap()[0].clone();
        assert_eq!(rec.mode, ByrealSpotMode::Preview);
        assert_eq!(rec.amount, 1.0);
    }

    #[tokio::test]
    async fn unknown_symbol_is_rejected() {
        let s = ByrealSpotSurface::new(
            MockSpotApi {
                price: 1.0,
                ..Default::default()
            },
            curated(),
        );
        let mut r = req(Side::Buy, 1.0);
        r.asset = "DOGE".into();
        assert!(s.submit_order(r).await.is_err());
    }

    #[tokio::test]
    async fn preview_mode_uses_dry_run() {
        let api = MockSpotApi {
            price: 150.0,
            ..Default::default()
        };
        let swaps = api.swaps.clone();
        let s = ByrealSpotSurface::new(api, curated()); // default Preview
        s.submit_order(req(Side::Buy, 1.0)).await.unwrap();
        assert_eq!(swaps.lock().unwrap()[0].mode, ByrealSpotMode::Preview);
    }

    #[tokio::test]
    async fn slippage_over_cap_is_refused() {
        let s = ByrealSpotSurface::new(
            MockSpotApi {
                price: 150.0,
                ..Default::default()
            },
            curated(),
        )
        .with_slippage_bps(500); // 500 > 200 cap
        assert!(s.submit_order(req(Side::Buy, 1.0)).await.is_err());
    }
}
