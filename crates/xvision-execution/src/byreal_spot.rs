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

use crate::executor::ExecutorError;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_maps_to_dry_run_or_confirm() {
        assert_eq!(ByrealSpotMode::Preview.swap_flag(), "--dry-run");
        assert_eq!(ByrealSpotMode::Live.swap_flag(), "--confirm");
    }
}
