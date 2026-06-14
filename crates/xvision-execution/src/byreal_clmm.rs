//! Byreal CLMM LP action — Solana concentrated-liquidity positions via the
//! `@byreal-io/byreal-cli` subprocess (Stretch 4).
//!
//! Distinct from [`crate::byreal`] (Hyperliquid *perps*): this drives the
//! Byreal **CLMM DEX on Solana** to open → rebalance → close a liquidity
//! position, surfacing each step in the run trace. Structure mirrors the perps
//! module: an inner [`ByrealClmmApi`] trait is the mockable seam (subprocess in
//! prod, in-memory in tests) and [`ClmmLpAction`] drives the lifecycle on top.
//!
//! Command surface verified against `@byreal-io/byreal-cli` `positions`:
//!   open  → `positions open --pool <addr> --price-lower <p> --price-upper <p>
//!            --amount-usd <usd> [--slippage <bps>] --confirm`  (returns NFT mint)
//!   close → `positions close --nft-mint <mint> --confirm`
//! A CLMM "rebalance" has no single command — it is a close of the old position
//! followed by an open at the new range. See the grounding spec.

use async_trait::async_trait;
use serde::Deserialize;

use crate::executor::ExecutorError;

/// Parameters to open a CLMM liquidity position.
#[derive(Debug, Clone, PartialEq)]
pub struct OpenLpRequest {
    /// Pool address (Solana).
    pub pool: String,
    /// Lower price bound of the concentrated range.
    pub price_lower: f64,
    /// Upper price bound of the concentrated range.
    pub price_upper: f64,
    /// Investment size in USD (`--amount-usd`; the CLI auto-splits the pair).
    pub amount_usd: f64,
    /// Optional slippage tolerance in basis points.
    pub slippage_bps: Option<u32>,
}

/// An opened CLMM position, identified by its NFT mint.
#[derive(Debug, Clone, PartialEq)]
pub struct ClmmPosition {
    pub nft_mint: String,
}

/// The mockable seam over the CLMM CLI. Each method maps to one `positions`
/// subcommand.
#[async_trait]
pub trait ByrealClmmApi: Send + Sync {
    /// `positions open …` — open a new position, returning its NFT mint.
    async fn open_position(&self, req: &OpenLpRequest) -> Result<ClmmPosition, ExecutorError>;
    /// `positions close --nft-mint <mint> --confirm` — remove all liquidity.
    async fn close_position(&self, nft_mint: &str) -> Result<(), ExecutorError>;
}

// ── CLI argument construction (pure; unit-tested against @byreal-io/byreal-cli) ──

fn open_position_args(req: &OpenLpRequest) -> Vec<String> {
    let mut a = vec![
        "positions".to_string(),
        "open".to_string(),
        "--pool".to_string(),
        req.pool.clone(),
        "--price-lower".to_string(),
        format!("{}", req.price_lower),
        "--price-upper".to_string(),
        format!("{}", req.price_upper),
        "--amount-usd".to_string(),
        format!("{}", req.amount_usd),
    ];
    if let Some(bps) = req.slippage_bps {
        a.push("--slippage".to_string());
        a.push(format!("{bps}"));
    }
    // `--confirm` actually executes (vs the CLI's dry-run default).
    a.push("--confirm".to_string());
    a
}

fn close_position_args(nft_mint: &str) -> Vec<String> {
    vec![
        "positions".to_string(),
        "close".to_string(),
        "--nft-mint".to_string(),
        nft_mint.to_string(),
        "--confirm".to_string(),
    ]
}

// ── Subprocess implementation ───────────────────────────────────────────────

/// The `{ success, data, error }` envelope the CLI emits under `-o json`.
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    data: Option<T>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenData {
    nft_mint: String,
}

/// Production [`ByrealClmmApi`] shelling out to `@byreal-io/byreal-cli`.
pub struct SubprocessByrealClmmApi {
    /// Extra args injected before the verb (e.g. `--network`). The CLI reads
    /// its own Solana wallet/keypair config per its contract.
    base_args: Vec<String>,
}

impl SubprocessByrealClmmApi {
    /// Build from environment. `BYREAL_CLMM_NETWORK` (e.g. `mainnet`/`devnet`),
    /// if set, is forwarded as `--network`.
    pub fn from_env() -> Self {
        let mut base_args = Vec::new();
        if let Ok(net) = std::env::var("BYREAL_CLMM_NETWORK") {
            base_args.push("--network".to_string());
            base_args.push(net);
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
            .map_err(|_| ExecutorError::Timeout("byreal-cli timed out after 120s".to_string()))?
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
impl ByrealClmmApi for SubprocessByrealClmmApi {
    async fn open_position(&self, req: &OpenLpRequest) -> Result<ClmmPosition, ExecutorError> {
        let data: OpenData = self
            .run(&open_position_args(req))
            .await?
            .ok_or_else(|| ExecutorError::Internal("open: CLI envelope missing data".into()))?;
        Ok(ClmmPosition {
            nft_mint: data.nft_mint,
        })
    }

    async fn close_position(&self, nft_mint: &str) -> Result<(), ExecutorError> {
        // The close payload is not consumed; only success matters.
        let _: Option<serde_json::Value> = self.run(&close_position_args(nft_mint)).await?;
        Ok(())
    }
}

// ── Lifecycle action (open → rebalance → close), surfaced in the trace ──────

/// One recorded step of a CLMM LP lifecycle, for the run trace / result.
#[derive(Debug, Clone, PartialEq)]
pub enum ClmmStep {
    Opened { nft_mint: String },
    Rebalanced { from: String, to: String },
    Closed { nft_mint: String },
}

/// Outcome of a full open → rebalance → close lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub struct ClmmLifecycle {
    pub steps: Vec<ClmmStep>,
}

/// Drives a CLMM LP position through its lifecycle on top of a
/// [`ByrealClmmApi`], emitting a `tracing` event per step so the run trace
/// records open / rebalance / close.
pub struct ClmmLpAction<A: ByrealClmmApi> {
    api: A,
}

impl<A: ByrealClmmApi> ClmmLpAction<A> {
    pub fn new(api: A) -> Self {
        Self { api }
    }

    /// Open a position at the given range/size.
    pub async fn open(&self, req: &OpenLpRequest) -> Result<ClmmPosition, ExecutorError> {
        let pos = self.api.open_position(req).await?;
        tracing::info!(
            target: "xvision::byreal_clmm",
            step = "open",
            pool = %req.pool,
            nft_mint = %pos.nft_mint,
            "clmm: opened LP position"
        );
        Ok(pos)
    }

    /// Close a position (remove all liquidity).
    pub async fn close(&self, nft_mint: &str) -> Result<(), ExecutorError> {
        self.api.close_position(nft_mint).await?;
        tracing::info!(
            target: "xvision::byreal_clmm",
            step = "close",
            nft_mint = %nft_mint,
            "clmm: closed LP position"
        );
        Ok(())
    }

    /// Rebalance: a CLMM range change is a close of the current position
    /// followed by an open at the new range. Returns the new position.
    pub async fn rebalance(
        &self,
        current_mint: &str,
        new_range: &OpenLpRequest,
    ) -> Result<ClmmPosition, ExecutorError> {
        self.api.close_position(current_mint).await?;
        let pos = self.api.open_position(new_range).await?;
        tracing::info!(
            target: "xvision::byreal_clmm",
            step = "rebalance",
            from = %current_mint,
            to = %pos.nft_mint,
            "clmm: rebalanced LP position to new range"
        );
        Ok(pos)
    }

    /// Run a full open → rebalance → close lifecycle, recording each step.
    /// Intended for the verifiability demo / smoke; production callers compose
    /// the primitive `open`/`rebalance`/`close` ops directly.
    pub async fn run_lifecycle(
        &self,
        open_req: &OpenLpRequest,
        rebalance_to: &OpenLpRequest,
    ) -> Result<ClmmLifecycle, ExecutorError> {
        let mut steps = Vec::new();
        let opened = self.open(open_req).await?;
        steps.push(ClmmStep::Opened {
            nft_mint: opened.nft_mint.clone(),
        });

        let rebalanced = self.rebalance(&opened.nft_mint, rebalance_to).await?;
        steps.push(ClmmStep::Rebalanced {
            from: opened.nft_mint,
            to: rebalanced.nft_mint.clone(),
        });

        self.close(&rebalanced.nft_mint).await?;
        steps.push(ClmmStep::Closed {
            nft_mint: rebalanced.nft_mint,
        });

        Ok(ClmmLifecycle { steps })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    fn open_req(pool: &str) -> OpenLpRequest {
        OpenLpRequest {
            pool: pool.to_string(),
            price_lower: 0.95,
            price_upper: 1.05,
            amount_usd: 50.0,
            slippage_bps: Some(50),
        }
    }

    fn argv(v: &[String]) -> Vec<&str> {
        v.iter().map(String::as_str).collect()
    }

    #[test]
    fn open_args_match_real_cli() {
        let a = open_position_args(&open_req("POOL1"));
        assert_eq!(
            argv(&a),
            vec![
                "positions",
                "open",
                "--pool",
                "POOL1",
                "--price-lower",
                "0.95",
                "--price-upper",
                "1.05",
                "--amount-usd",
                "50",
                "--slippage",
                "50",
                "--confirm"
            ]
        );
    }

    #[test]
    fn open_args_omit_slippage_when_unset() {
        let mut req = open_req("POOL1");
        req.slippage_bps = None;
        let a = open_position_args(&req);
        assert!(!a.iter().any(|x| x == "--slippage"));
        assert_eq!(a.last().map(String::as_str), Some("--confirm"));
    }

    #[test]
    fn close_args_match_real_cli() {
        assert_eq!(
            argv(&close_position_args("MINT9")),
            vec!["positions", "close", "--nft-mint", "MINT9", "--confirm"]
        );
    }

    /// Records every API call so we can assert the lifecycle ordering.
    #[derive(Default, Clone)]
    struct MockClmmApi {
        calls: Arc<Mutex<Vec<String>>>,
        next_mint: Arc<Mutex<u32>>,
    }

    #[async_trait]
    impl ByrealClmmApi for MockClmmApi {
        async fn open_position(&self, req: &OpenLpRequest) -> Result<ClmmPosition, ExecutorError> {
            let mut n = self.next_mint.lock().unwrap();
            *n += 1;
            let nft_mint = format!("mint-{n}");
            self.calls
                .lock()
                .unwrap()
                .push(format!("open:{}:{}", req.pool, nft_mint));
            Ok(ClmmPosition { nft_mint })
        }
        async fn close_position(&self, nft_mint: &str) -> Result<(), ExecutorError> {
            self.calls.lock().unwrap().push(format!("close:{nft_mint}"));
            Ok(())
        }
    }

    #[tokio::test]
    async fn lifecycle_opens_rebalances_closes_in_order() {
        let api = MockClmmApi::default();
        let calls = api.calls.clone();
        let action = ClmmLpAction::new(api);

        let result = action
            .run_lifecycle(&open_req("POOLX"), &open_req("POOLX"))
            .await
            .unwrap();

        // Lifecycle records open → rebalance → close.
        assert!(matches!(result.steps[0], ClmmStep::Opened { .. }));
        assert!(matches!(result.steps[1], ClmmStep::Rebalanced { .. }));
        assert!(matches!(result.steps[2], ClmmStep::Closed { .. }));

        // Underlying API call order: open(1) → [rebalance = close(1) + open(2)] → close(2).
        let calls = calls.lock().unwrap();
        assert_eq!(
            *calls,
            vec![
                "open:POOLX:mint-1".to_string(),
                "close:mint-1".to_string(),
                "open:POOLX:mint-2".to_string(),
                "close:mint-2".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn open_then_close_primitives() {
        let api = MockClmmApi::default();
        let action = ClmmLpAction::new(api);
        let pos = action.open(&open_req("P")).await.unwrap();
        assert_eq!(pos.nft_mint, "mint-1");
        action.close(&pos.nft_mint).await.unwrap();
    }
}
