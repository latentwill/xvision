# Byreal Solana Spot Trading — Implementation Plan (first slice)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an xvision agent (and an operator via a one-shot CLI) trade Solana spot — a curated SPL + xStocks set — through `@byreal-io/byreal-cli`, reusing the existing gated live-execution path (SafetyGate + `venue_label` + kill-switch) so spot is a new venue, not a second execution path.

**Architecture:** One integration surface — `ByrealSpotSurface`, a `BrokerSurface` over the `byreal-cli` subprocess (mirrors `byreal_clmm.rs`/`byreal.rs`). Built **C → A**: Phase C is a thin gated `xvn spot` one-shot swap (default `--dry-run`, no funds); Phase A wires a `byreal_spot` `LiveVenue` so an agent run flows through `Executor::live` and inherits `GatedBrokerSurface` automatically. Spot is Long/Flat only (no shorting, no leverage). Marks come from `byreal-cli` token price (poll-only).

**Tech Stack:** Rust (xvision-core / xvision-execution / xvision-engine / xvision-cli), `@byreal-io/byreal-cli` via `npx` subprocess, `garde`-validated TOML config, `tokio`, `sqlx` (kill-switch read), `garde`/`async-trait`.

**Source spec:** `docs/superpowers/specs/2026-06-15-byreal-solana-spot-trading-design.md`

---

## Pre-flight (do this FIRST, before any task)

The branch was already refreshed onto `origin/main` (merge commit present). Confirm the gated-path primitives exist before starting — if any are missing, STOP (the branch is stale again and must be re-merged):

```bash
cd /Users/edkennedy/Code/xvision/.worktrees/byreal-solana-spot
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
test -f crates/xvision-engine/src/eval/executor/gated_broker.rs || echo "MISSING gated_broker.rs — re-merge origin/main"
test -f crates/xvision-cli/src/commands/live_guard.rs || echo "MISSING live_guard.rs — re-merge origin/main"
git grep -q "fn broker_label_for" crates/xvision-engine/src/api/eval.rs && echo "broker_label_for OK" || echo "MISSING broker_label_for"
```

All build/test commands run from the worktree root with `CARGO_TARGET_DIR` exported and through the disk-guard wrapper:

```bash
scripts/cargo test -p xvision-execution
```

---

## File Structure

**Phase C (thin gated CLI — independently shippable as its own PR):**

| File | Responsibility | New/Modify |
|---|---|---|
| `docs/superpowers/specs/2026-06-15-byreal-spot-cli-grounding.md` | Pin the real `byreal-cli` swap/price/balance command surface + JSON shapes (mirrors the perps grounding spec) | Create |
| `crates/xvision-core/src/config.rs` | `SpotAssetKind`, `SpotAssetEntry`, `SpotAssetConfig`, `load_spot_assets()` (curated symbol↔mint↔kind↔decimals whitelist) | Modify |
| `config/byreal_spot_assets.toml` | Example curated-set config (SOL, JUP, AAPLx, NVDAx, USDC quote) | Create |
| `crates/xvision-execution/src/byreal_spot.rs` | `ByrealSpotApi` trait + `SubprocessByrealSpotApi` (CLI seam) + `ByrealSpotMode` + `SwapPreview` + `ByrealSpotSurface` (`BrokerSurface` impl) | Create |
| `crates/xvision-execution/src/lib.rs` | Export the new module's public types | Modify |
| `crates/xvision-cli/src/commands/spot.rs` | `xvn spot` handler (gated one-shot swap, dry-run default) | Create |
| `crates/xvision-cli/src/commands/mod.rs` | `pub mod spot;` | Modify |
| `crates/xvision-cli/src/lib.rs` | `Command::Spot { … }` clap variant + dispatch arm | Modify |

**Phase A (agent-driven gated live — builds on Phase C):**

| File | Responsibility | New/Modify |
|---|---|---|
| `crates/xvision-engine/src/api/eval.rs` | `LiveVenue::ByrealSpot` + `resolve_live_venue`/`broker_label_for`/`check_venue_label_network` arms + `build_live_executor` broker arm + poll-stream branch + inline tests | Modify |
| `crates/xvision-execution/src/byreal_spot.rs` | `ByrealSpotPriceFetcher` (`LivePollFetcher` over token price) | Modify |
| `crates/xvision-engine/tests/gated_byreal_spot_submit.rs` | Integration test: paused gate ⇒ 0 swaps; venue_label mismatch ⇒ blocked | Create |

**Naming (locked — match existing `ByrealLive` casing, NOT `ByRealSpot`):** module `byreal_spot`, `broker_creds_ref` = `"byreal_spot"`, env `BYREAL_SPOT_NETWORK`, types `ByrealSpotSurface`/`ByrealSpotApi`/`SubprocessByrealSpotApi`/`ByrealSpotPriceFetcher`, `LiveVenue::ByrealSpot`.

---

# PHASE C — thin gated CLI (`xvn spot`)

## Task C0: Ground the `byreal-cli` command surface

The spec flags the exact swap/price/balance flags and JSON shapes as **unconfirmed**. Pin them first (mirrors `docs/superpowers/specs/2026-06-13-byreal-perps-cli-grounding.md`) so C2's arg-builder is correct, not guessed. No production code in this task.

**Files:**
- Create: `docs/superpowers/specs/2026-06-15-byreal-spot-cli-grounding.md`

- [ ] **Step 1: Probe the CLI surface**

Run (network access required; this is the same package `byreal_clmm.rs` already uses):

```bash
npx -y @byreal-io/byreal-cli@latest --help
npx -y @byreal-io/byreal-cli@latest --version
npx -y @byreal-io/byreal-cli@latest catalog list -o json 2>/dev/null || true
npx -y @byreal-io/byreal-cli@latest swap --help 2>/dev/null || true
npx -y @byreal-io/byreal-cli@latest token --help 2>/dev/null || true
npx -y @byreal-io/byreal-cli@latest wallet --help 2>/dev/null || true
```

- [ ] **Step 2: Record findings in the grounding doc**

Capture, verbatim from the probe: (a) the exact `swap execute` flags (`--input-mint`/`--output-mint`/`--amount`/`--slippage`/`--dry-run`/`--confirm` — confirm names/positional-vs-flag), (b) the token-price query verb + flags + the `data` JSON shape (where the price number lives), (c) the wallet/balance query verb + the `data` shape, (d) the pinned `--version`, (e) whether `-o json` wraps output in the `{ success, data, error }` envelope (as CLMM assumes). If any flag differs from the spec's assumption, the doc is the source of truth for C2.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/specs/2026-06-15-byreal-spot-cli-grounding.md
git commit -m "docs(byreal-spot): ground byreal-cli swap/price/balance surface"
```

> If the probe cannot run (no network in the build env), record that explicitly in the doc, proceed with the spec's assumed flags in C2, and mark every assumed flag with a `// GROUNDING: assumed, verify against byreal-cli --version <pin>` comment so the manual smoke test (C5 Step 8) catches drift.

---

## Task C1: Curated-set config

**Files:**
- Modify: `crates/xvision-core/src/config.rs` (add after the `WhitelistConfig` block, ~line 384; add loader after `load_whitelist`, ~line 673)
- Create: `config/byreal_spot_assets.toml`
- Test: inline `#[cfg(test)]` in `config.rs`

- [ ] **Step 1: Write the failing test**

Add to the `#[cfg(test)] mod tests` block in `crates/xvision-core/src/config.rs`:

```rust
#[test]
fn loads_byreal_spot_assets_and_resolves_mint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("byreal_spot_assets.toml");
    std::fs::write(
        &path,
        r#"
usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"

[[assets]]
symbol = "SOL"
mint = "So11111111111111111111111111111111111111112"
kind = "spl"
decimals = 9

[[assets]]
symbol = "AAPLx"
mint = "XsbEhLAtcf6HdfpFZ5xEMdqW8nfAvcsP5bdudRLJzJp"
kind = "xstock"
decimals = 8
"#,
    )
    .unwrap();

    let cfg = load_spot_assets(&path).expect("valid config loads");
    assert_eq!(cfg.usdc_mint, "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
    let sol = cfg.resolve("SOL").expect("SOL is curated");
    assert_eq!(sol.mint, "So11111111111111111111111111111111111111112");
    assert_eq!(sol.decimals, 9);
    assert_eq!(sol.kind, SpotAssetKind::Spl);
    // Symbol resolution is case-insensitive on the ticker.
    assert!(cfg.resolve("aaplx").is_some());
    // Unknown symbol is refused (whitelist semantics).
    assert!(cfg.resolve("DOGE").is_none());
}

#[test]
fn rejects_spot_assets_with_empty_mint() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.toml");
    std::fs::write(
        &path,
        "usdc_mint = \"EPjF...\"\n[[assets]]\nsymbol = \"SOL\"\nmint = \"\"\nkind = \"spl\"\ndecimals = 9\n",
    )
    .unwrap();
    assert!(matches!(
        load_spot_assets(&path),
        Err(ConfigError::Validation { .. })
    ));
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `scripts/cargo test -p xvision-core loads_byreal_spot_assets`
Expected: FAIL — `load_spot_assets`, `SpotAssetKind` not found.

- [ ] **Step 3: Add the config types + loader**

In `crates/xvision-core/src/config.rs`, after the `WhitelistConfig` impl block (~line 384):

```rust
// --- byreal spot curated set ------------------------------------------------

/// Token category for a curated Solana-spot asset. `xstock` (Backed Finance
/// tokenized equities) is a plain SPL token; the tag drives display only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SpotAssetKind {
    Spl,
    Xstock,
}

/// One operator-curated Solana-spot asset.
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct SpotAssetEntry {
    /// Ticker symbol used by the agent and `xvn spot` (e.g. "SOL", "AAPLx").
    #[garde(length(min = 1, max = 32))]
    pub symbol: String,
    /// SPL mint address (base58). xStocks are plain SPL mints.
    #[garde(length(min = 32, max = 64))]
    pub mint: String,
    #[garde(skip)]
    pub kind: SpotAssetKind,
    /// On-chain decimals for the mint (used for base-unit ⇄ display conversion).
    #[garde(range(min = 0, max = 18))]
    pub decimals: u8,
}

/// Curated whitelist mapping `symbol → { mint, kind, decimals }`, plus the
/// USDC mint used as the quote leg of every buy/sell swap. Operator-defined;
/// out-of-set symbols are refused (this is a whitelist, not a hint list).
#[derive(Debug, Clone, PartialEq, Validate, Serialize, Deserialize)]
pub struct SpotAssetConfig {
    /// USDC SPL mint used as the quote asset for buys (USDC→token) and sells
    /// (token→USDC).
    #[garde(length(min = 32, max = 64))]
    pub usdc_mint: String,
    #[garde(dive)]
    pub assets: Vec<SpotAssetEntry>,
}

impl SpotAssetConfig {
    /// Case-insensitive ticker lookup. Returns `None` for symbols not in the
    /// curated set (whitelist semantics).
    pub fn resolve(&self, symbol: &str) -> Option<&SpotAssetEntry> {
        self.assets
            .iter()
            .find(|a| a.symbol.eq_ignore_ascii_case(symbol))
    }
}
```

And after `load_whitelist` (~line 673):

```rust
/// Load the curated Byreal-spot asset set from a TOML file (see
/// `config/byreal_spot_assets.toml`). Validated via `garde`.
pub fn load_spot_assets(path: &Path) -> Result<SpotAssetConfig, ConfigError> {
    read_toml(path)
}

/// Default location of the curated spot-asset config under `xvn_home`.
pub fn spot_assets_path(xvn_home: &Path) -> PathBuf {
    xvn_home.join("config").join("byreal_spot_assets.toml")
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `scripts/cargo test -p xvision-core loads_byreal_spot_assets rejects_spot_assets`
Expected: PASS (both tests).

- [ ] **Step 5: Create the example config**

`config/byreal_spot_assets.toml`:

```toml
# Curated Byreal Solana-spot universe (first slice).
# symbol → { mint, kind, decimals }. xStocks (Backed Finance) are plain SPL
# mints; `kind = "xstock"` is a display tag only. Out-of-set symbols are refused.
# Mints below are placeholders — confirm each against an explorer before live use.
usdc_mint = "EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v"

[[assets]]
symbol = "SOL"
mint = "So11111111111111111111111111111111111111112"
kind = "spl"
decimals = 9

[[assets]]
symbol = "JUP"
mint = "JUPyiwrYJFskUPiHa7hkeR8VUtAeFoSYbKedZNsDvCN"
kind = "spl"
decimals = 6

[[assets]]
symbol = "AAPLx"
mint = "PLACEHOLDER_AAPLx_MINT_confirm_on_explorer"
kind = "xstock"
decimals = 8

[[assets]]
symbol = "NVDAx"
mint = "PLACEHOLDER_NVDAx_MINT_confirm_on_explorer"
kind = "xstock"
decimals = 8
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-core/src/config.rs config/byreal_spot_assets.toml
git commit -m "feat(byreal-spot): curated symbol→mint config (SpotAssetConfig)"
```

---

## Task C2: `byreal-cli` subprocess seam (`ByrealSpotApi` + `SubprocessByrealSpotApi`)

Mirrors `byreal_clmm.rs` exactly (same `@byreal-io/byreal-cli@latest` package, 120s timeout, `{ success, data, error }` envelope).

**Files:**
- Create: `crates/xvision-execution/src/byreal_spot.rs`
- Test: inline `#[cfg(test)]` in the same file

- [ ] **Step 1: Write the failing test (mode → flag mapping)**

Create `crates/xvision-execution/src/byreal_spot.rs` with only the types + a unit test that pins the Preview/Live → flag decision (the part we can test without spawning a subprocess):

```rust
//! `ByrealSpotSurface` — a `BrokerSurface` over `@byreal-io/byreal-cli` for
//! Solana spot (curated SPL + xStocks). Buy = USDC→token swap, sell =
//! token→USDC swap. Spot is Long/Flat only: no shorting, no leverage. Mirrors
//! `byreal_clmm.rs` for the subprocess seam.
//!
//! Custody: the CLI manages the wallet keystore in `~/.config/byreal/keys/`.
//! This surface never reads or logs key material.

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

/// Outcome of a swap (preview or live). Field names mirror the byreal-cli
/// `swap execute` JSON `data` payload — CONFIRM against the C0 grounding doc.
#[derive(Debug, Clone, Deserialize)]
pub struct SwapResult {
    /// Transaction signature for a confirmed swap; `None`/empty for a dry-run.
    #[serde(default)]
    pub signature: Option<String>,
    /// Output-asset amount received (in output-mint display units).
    #[serde(default)]
    pub out_amount: f64,
    /// Effective price impact in basis points, if reported.
    #[serde(default)]
    pub price_impact_bps: Option<f64>,
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
```

- [ ] **Step 2: Register the module so it compiles**

In `crates/xvision-execution/src/lib.rs`, add `pub mod byreal_spot;` after `pub mod byreal_clmm;`.

Run: `scripts/cargo test -p xvision-execution mode_maps_to_dry_run`
Expected: PASS (trivial), confirming the module compiles and is wired.

- [ ] **Step 3: Add the `ByrealSpotApi` trait + subprocess impl**

Append to `crates/xvision-execution/src/byreal_spot.rs` (template lifted verbatim from `byreal_clmm.rs::run`; adjust verb args per the C0 grounding doc):

```rust
/// Mockable seam over the `byreal-cli` subprocess: swap, token price, balance.
#[async_trait]
pub trait ByrealSpotApi: Send + Sync {
    /// Execute (or preview) a swap of `amount` input-mint units into the
    /// output mint, with `slippage_bps` tolerance, in the given mode.
    async fn swap(
        &self,
        input_mint: &str,
        output_mint: &str,
        amount: f64,
        slippage_bps: u32,
        mode: ByrealSpotMode,
    ) -> Result<SwapResult, ExecutorError>;

    /// Latest token price in USD for a mint (used for marks + base-size calc).
    async fn token_price(&self, mint: &str) -> Result<f64, ExecutorError>;

    /// Wallet balance for a mint, in display units (token balance, or USDC for
    /// the quote mint).
    async fn token_balance(&self, mint: &str) -> Result<f64, ExecutorError>;
}

/// `{ success, data, error }` envelope emitted by `byreal-cli -o json`.
#[derive(Debug, Deserialize)]
struct Envelope<T> {
    success: bool,
    data: Option<T>,
    #[serde(default)]
    error: Option<String>,
}

/// Production `ByrealSpotApi` that shells out to `npx -y @byreal-io/byreal-cli`.
pub struct SubprocessByrealSpotApi {
    base_args: Vec<String>,
}

impl SubprocessByrealSpotApi {
    /// Build from env. Reads `BYREAL_SPOT_NETWORK` (optional) → `--network`.
    /// The wallet keystore is managed by the CLI itself (no key env var).
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

        let output =
            tokio::time::timeout(std::time::Duration::from_secs(120), child.wait_with_output())
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
                env.error.unwrap_or_else(|| "CLI returned success=false".into()),
            ));
        }
        Ok(env.data)
    }
}

/// Price payload for `token price <mint>` — CONFIRM field name against C0.
#[derive(Debug, Deserialize)]
struct PricePayload {
    #[serde(alias = "priceUsd", alias = "price")]
    price: f64,
}

/// Balance payload for the wallet balance query — CONFIRM against C0.
#[derive(Debug, Deserialize)]
struct BalancePayload {
    #[serde(alias = "uiAmount", alias = "amount")]
    amount: f64,
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
        // GROUNDING: verb/flags per C0 grounding doc. Assumed shape:
        //   swap execute --input-mint <m> --output-mint <m> --amount <n>
        //     --slippage <bps> (--dry-run | --confirm)
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
        // GROUNDING: verb per C0. Assumed: `token price <mint>`.
        let args = vec!["token".into(), "price".into(), mint.into()];
        let p = self
            .run::<PricePayload>(&args)
            .await?
            .ok_or_else(|| ExecutorError::Internal("price envelope missing data".into()))?;
        Ok(p.price)
    }

    async fn token_balance(&self, mint: &str) -> Result<f64, ExecutorError> {
        // GROUNDING: verb per C0. Assumed: `wallet balance --mint <mint>`.
        let args = vec![
            "wallet".into(),
            "balance".into(),
            "--mint".into(),
            mint.into(),
        ];
        let b = self
            .run::<BalancePayload>(&args)
            .await?
            .ok_or_else(|| ExecutorError::Internal("balance envelope missing data".into()))?;
        Ok(b.amount)
    }
}
```

- [ ] **Step 4: Verify it compiles**

Run: `scripts/cargo build -p xvision-execution`
Expected: builds clean (the subprocess impl has no automated test — it is exercised via the manual smoke in C5 and grounded in C0).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-execution/src/byreal_spot.rs crates/xvision-execution/src/lib.rs
git commit -m "feat(byreal-spot): byreal-cli subprocess seam (ByrealSpotApi)"
```

---

## Task C3: `ByrealSpotSurface` (`BrokerSurface` impl) + mock tests

**Files:**
- Modify: `crates/xvision-execution/src/byreal_spot.rs`
- Test: inline `#[cfg(test)]` (mock `ByrealSpotApi`, mirrors `MockByrealApi` in `byreal.rs`)

- [ ] **Step 1: Write the failing tests**

Add to the `#[cfg(test)] mod tests` block in `byreal_spot.rs`:

```rust
use std::sync::{Arc, Mutex};
use xvision_core::config::{SpotAssetConfig, SpotAssetEntry, SpotAssetKind};

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
            signature: Some("mock-sig".into()),
            out_amount: amount / self.price.max(1.0),
            price_impact_bps: Some(5.0),
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
        usdc_mint: "USDC_MINT_11111111111111111111111111111111".into(),
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

// 1. metadata
#[tokio::test]
async fn surface_metadata_is_spot_not_perp() {
    let s = ByrealSpotSurface::new(MockSpotApi { price: 150.0, ..Default::default() }, curated());
    assert_eq!(s.venue(), "byreal_spot");
    assert_eq!(s.signing_scheme(), "cli");
    assert!(!s.is_perp_venue());
}

// 2. buy = USDC→token swap, amount = notional, Live uses --confirm
async fn buy_swaps_usdc_into_token() {
    let api = MockSpotApi { price: 150.0, ..Default::default() };
    let swaps = api.swaps.clone();
    let s = ByrealSpotSurface::new(api, curated())
        .with_mode(ByrealSpotMode::Live)
        .with_slippage_bps(100);
    let conf = s.submit_order(req(Side::Buy, 2.0)).await.unwrap();
    let rec = swaps.lock().unwrap()[0].clone();
    assert_eq!(rec.input_mint, "USDC_MINT_11111111111111111111111111111111");
    assert_eq!(rec.output_mint, "So11111111111111111111111111111111111111112");
    assert_eq!(rec.amount, 300.0); // 2.0 * 150.0 reference price, in USDC
    assert_eq!(rec.slippage_bps, 100);
    assert_eq!(rec.mode, ByrealSpotMode::Live);
    assert!(conf.broker_order_id.contains("mock-sig"));
}
#[tokio::test]
async fn buy_swaps_usdc_into_token_t() { buy_swaps_usdc_into_token().await }

// 3. sell with an open position = token→USDC swap, amount = base size
#[tokio::test]
async fn sell_swaps_token_into_usdc() {
    let api = MockSpotApi { price: 150.0, position: 5.0, ..Default::default() };
    let swaps = api.swaps.clone();
    let s = ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live);
    s.submit_order(req(Side::Sell, 5.0)).await.unwrap();
    let rec = swaps.lock().unwrap()[0].clone();
    assert_eq!(rec.input_mint, "So11111111111111111111111111111111111111112");
    assert_eq!(rec.output_mint, "USDC_MINT_11111111111111111111111111111111");
    assert_eq!(rec.amount, 5.0); // base units
}

// 4. sell with no position is refused (Long/Flat only — no shorting)
#[tokio::test]
async fn sell_without_position_is_rejected_no_shorting() {
    let api = MockSpotApi { price: 150.0, position: 0.0, ..Default::default() };
    let swaps = api.swaps.clone();
    let s = ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live);
    let err = s.submit_order(req(Side::Sell, 1.0)).await.unwrap_err();
    assert!(
        err.to_string().contains("short_open is not supported"),
        "spot must refuse shorting; got: {err}"
    );
    assert_eq!(swaps.lock().unwrap().len(), 0, "no swap on a rejected short");
}

// 5. unknown symbol is refused (whitelist)
#[tokio::test]
async fn unknown_symbol_is_rejected() {
    let s = ByrealSpotSurface::new(MockSpotApi { price: 1.0, ..Default::default() }, curated());
    let mut r = req(Side::Buy, 1.0);
    r.asset = "DOGE".into();
    assert!(s.submit_order(r).await.is_err());
}

// 6. Preview mode emits --dry-run
#[tokio::test]
async fn preview_mode_uses_dry_run() {
    let api = MockSpotApi { price: 150.0, ..Default::default() };
    let swaps = api.swaps.clone();
    let s = ByrealSpotSurface::new(api, curated()); // default = Preview
    s.submit_order(req(Side::Buy, 1.0)).await.unwrap();
    assert_eq!(swaps.lock().unwrap()[0].mode, ByrealSpotMode::Preview);
}

// 7. slippage over the configured cap is refused
#[tokio::test]
async fn slippage_over_cap_is_refused() {
    let s = ByrealSpotSurface::new(MockSpotApi { price: 150.0, ..Default::default() }, curated())
        .with_slippage_bps(500); // 500 > 200 cap
    assert!(s.submit_order(req(Side::Buy, 1.0)).await.is_err());
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `scripts/cargo test -p xvision-execution byreal_spot`
Expected: FAIL — `ByrealSpotSurface` not defined.

- [ ] **Step 3: Implement `ByrealSpotSurface`**

Add to `byreal_spot.rs` (before the test module):

```rust
/// Hard cap on slippage; byreal-cli warns above 200 bps, we refuse above it.
const MAX_SLIPPAGE_BPS: u32 = 200;

/// `BrokerSurface` over `byreal-cli` for curated Solana spot. Buy = USDC→token,
/// sell = token→USDC. Long/Flat only. Holds the curated set for symbol→mint
/// resolution and the quote (USDC) mint.
pub struct ByrealSpotSurface<A = SubprocessByrealSpotApi> {
    api: A,
    assets: SpotAssetConfig,
    mode: ByrealSpotMode,
    slippage_bps: u32,
}

impl<A: ByrealSpotApi> ByrealSpotSurface<A> {
    /// Defaults to `Preview` mode (no funds) and a conservative 100 bps slippage.
    pub fn new(api: A, assets: SpotAssetConfig) -> Self {
        Self { api, assets, mode: ByrealSpotMode::Preview, slippage_bps: 100 }
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
impl<A: ByrealSpotApi + 'static> BrokerSurface for ByrealSpotSurface<A> {
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
            // Buy: spend USDC notional (size base-units × reference price) to get token.
            Side::Buy => (usdc, entry.mint.as_str(), req.size * req.reference_price_usd),
            // Sell: spend `size` base-units of the token to get USDC. Long/Flat
            // only — refuse a sell with no position (no shorting).
            Side::Sell => {
                let pos = self
                    .api
                    .token_balance(&entry.mint)
                    .await
                    .map_err(|e| anyhow::anyhow!("byreal_spot balance: {e}"))?;
                if pos <= 0.0 {
                    anyhow::bail!(
                        "byreal_spot broker_unsupported: short_open is not supported \
                         (no {} position to sell)",
                        req.asset
                    );
                }
                (entry.mint.as_str(), usdc, req.size.min(pos))
            }
        };

        let res = self
            .api
            .swap(input_mint, output_mint, amount, self.slippage_bps, self.mode)
            .await
            .map_err(|e| anyhow::anyhow!("byreal_spot swap: {e}"))?;

        Ok(OrderConfirmation {
            broker_order_id: res.signature.unwrap_or_else(|| format!("preview-{}", req.idempotency_key)),
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `scripts/cargo test -p xvision-execution byreal_spot`
Expected: PASS (all 8 tests).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-execution/src/byreal_spot.rs
git commit -m "feat(byreal-spot): ByrealSpotSurface (buy/sell→swap, Long/Flat, slippage cap)"
```

---

## Task C4: Export the public types

**Files:**
- Modify: `crates/xvision-execution/src/lib.rs`

- [ ] **Step 1: Add the re-export**

After the `pub use byreal::{…};` block in `crates/xvision-execution/src/lib.rs`, add:

```rust
pub use byreal_spot::{
    ByrealSpotApi, ByrealSpotMode, ByrealSpotSurface, SubprocessByrealSpotApi, SwapResult,
};
```

- [ ] **Step 2: Verify the workspace still builds**

Run: `scripts/cargo build -p xvision-execution`
Expected: clean build.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-execution/src/lib.rs
git commit -m "feat(byreal-spot): export ByrealSpot* from xvision-execution"
```

---

## Task C5: `xvn spot` CLI command (gated one-shot swap)

Mirrors `xvn close-position` (`commands/venue.rs` + `lib.rs`): threads `--i-understand-real-money` and `--xvn-home`, reuses `check_not_paused`. Default is `--dry-run` Preview (no funds); `--i-understand-real-money` flips to Live (`--confirm`) and runs the kill-switch check first.

**Files:**
- Create: `crates/xvision-cli/src/commands/spot.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs`, `crates/xvision-cli/src/lib.rs`
- Test: inline `#[cfg(test)]` in `spot.rs` (pure mode-selection helper)

- [ ] **Step 1: Write the failing test (mode selection)**

Create `crates/xvision-cli/src/commands/spot.rs`:

```rust
//! `xvn spot` — gated one-shot Solana-spot swap via `byreal-cli`.
//!
//! Default is a no-funds `--dry-run` preview. `--i-understand-real-money`
//! flips to a real `--confirm` swap, but ONLY after the global kill-switch
//! (`check_not_paused`) passes. Symbol resolves to a mint via the curated
//! `byreal_spot_assets.toml` under `xvn_home`.

use std::path::PathBuf;

use anyhow::{Context, Result};
use xvision_core::config::{load_spot_assets, spot_assets_path};
use xvision_execution::{
    BrokerSurface, ByrealSpotApi, ByrealSpotMode, ByrealSpotSurface, SubprocessByrealSpotApi,
};
use xvision_execution::broker_surface::{OrderRequest, Side};

use crate::commands::live_guard::check_not_paused;

/// Map the ack flag to a swap mode. Pure + unit-tested.
fn mode_for(i_understand_real_money: bool) -> ByrealSpotMode {
    if i_understand_real_money {
        ByrealSpotMode::Live
    } else {
        ByrealSpotMode::Preview
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_ack_is_preview_ack_is_live() {
        assert_eq!(mode_for(false), ByrealSpotMode::Preview);
        assert_eq!(mode_for(true), ByrealSpotMode::Live);
    }
}
```

- [ ] **Step 2: Run the test (fails until module is registered)**

In `crates/xvision-cli/src/commands/mod.rs`, add `pub mod spot;` (alphabetical, near `pub mod strategy;`).

Run: `scripts/cargo test -p xvision-cli no_ack_is_preview`
Expected: PASS (after registration), confirming wiring.

- [ ] **Step 3: Add the `run` handler**

Append to `spot.rs`:

```rust
#[derive(Debug, Clone, Copy)]
pub enum SpotSide {
    Buy,
    Sell,
}

impl std::str::FromStr for SpotSide {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "buy" => Ok(SpotSide::Buy),
            "sell" => Ok(SpotSide::Sell),
            other => Err(format!("unknown side '{other}'; want buy|sell")),
        }
    }
}

/// `xvn spot --buy|--sell <symbol> --amount <usd> [--slippage <bps>]
///   [--i-understand-real-money] [--xvn-home <path>]`.
///
/// `amount` is USD notional for a buy, and USD-equivalent for a sell (converted
/// to base units via the live token price).
#[allow(clippy::too_many_arguments)]
pub async fn run(
    side: SpotSide,
    symbol: String,
    amount_usd: f64,
    slippage_bps: u32,
    i_understand_real_money: bool,
    xvn_home: PathBuf,
) -> Result<()> {
    let mode = mode_for(i_understand_real_money);

    // Kill-switch: only gate a real (Live) swap; a dry-run moves no funds.
    // Live spot is real money, so fail-closed if the DB is missing.
    if mode == ByrealSpotMode::Live {
        check_not_paused(&xvn_home, true).await?;
    }

    let cfg_path = spot_assets_path(&xvn_home);
    let assets = load_spot_assets(&cfg_path)
        .with_context(|| format!("load curated spot set at {}", cfg_path.display()))?;
    let entry = assets
        .resolve(&symbol)
        .ok_or_else(|| anyhow::anyhow!("'{symbol}' is not in the curated spot set ({})", cfg_path.display()))?
        .clone();

    let api = SubprocessByrealSpotApi::from_env();
    // Live token price → base size = USD / price.
    let price = api
        .token_price(&entry.mint)
        .await
        .map_err(|e| anyhow::anyhow!("byreal_spot price for {symbol}: {e}"))?;
    anyhow::ensure!(price > 0.0, "byreal_spot returned non-positive price for {symbol}");
    let size = amount_usd / price;

    let surface = ByrealSpotSurface::new(api, assets)
        .with_mode(mode)
        .with_slippage_bps(slippage_bps);

    let order = OrderRequest {
        asset: symbol.clone(),
        side: match side {
            SpotSide::Buy => Side::Buy,
            SpotSide::Sell => Side::Sell,
        },
        size,
        reference_price_usd: price,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: format!("xvn-spot-{symbol}"),
    };

    let label = if mode == ByrealSpotMode::Live { "LIVE (real funds)" } else { "preview (dry-run)" };
    println!("→ {label}: {side:?} {symbol} ~${amount_usd} ({size:.6} @ ${price:.4}, slippage {slippage_bps}bps)");

    let conf = surface.submit_order(order).await?;
    println!("\n--- swap result ---\n{}", serde_json::to_string_pretty(&conf)?);
    Ok(())
}
```

- [ ] **Step 4: Add the clap variant**

In `crates/xvision-cli/src/lib.rs`, add a `Command::Spot` variant (model on `ClosePosition` at lines 163–180). Note `Action`-derived doc comment + `i_understand_real_money` + `xvn_home: Option<PathBuf>`:

```rust
    /// One-shot gated Solana-spot swap via byreal-cli (curated SPL + xStocks).
    /// Defaults to a no-funds `--dry-run` preview; `--i-understand-real-money`
    /// executes a real swap (kill-switch checked first).
    Spot {
        #[arg(long, value_parser = clap::value_parser!(commands::spot::SpotSide))]
        side: commands::spot::SpotSide,
        /// Curated ticker (e.g. SOL, JUP, AAPLx). Resolved via byreal_spot_assets.toml.
        #[arg(long)]
        symbol: String,
        /// USD notional to swap.
        #[arg(long)]
        amount: f64,
        /// Max slippage in basis points (capped at 200).
        #[arg(long, default_value_t = 100)]
        slippage: u32,
        #[arg(long, default_value_t = false)]
        i_understand_real_money: bool,
        #[arg(long)]
        xvn_home: Option<PathBuf>,
    },
```

Add `impl clap::ValueEnum`-free parsing: `SpotSide` already implements `FromStr`, so use `value_parser = clap::value_parser!(...)` won't work for a bare `FromStr`. Instead declare the arg as:

```rust
        #[arg(long)]
        side: String,
```

and parse inside the dispatch arm (mirrors how `close-position` takes `asset: String`). Update the variant accordingly (drop the `value_parser`).

- [ ] **Step 5: Add the dispatch arm**

In `Cli::run()` (after the `Command::ClosePosition { … }` arm, ~line 338):

```rust
            Command::Spot {
                side,
                symbol,
                amount,
                slippage,
                i_understand_real_money,
                xvn_home,
            } => {
                let home = commands::home::resolve_xvn_home(xvn_home).map_err(crate::exit::CliError::from)?;
                let side: commands::spot::SpotSide =
                    side.parse().map_err(|e: String| crate::exit::CliError::from(anyhow::anyhow!(e)))?;
                commands::spot::run(side, symbol, amount, slippage, i_understand_real_money, home)
                    .await
                    .map_err(crate::exit::CliError::from)?;
            }
```

(Match the exact error-mapping idiom used by the neighboring `ClosePosition` arm — copy its `.map_err(...)` shape verbatim if it differs.)

- [ ] **Step 6: Run all CLI tests + build**

Run: `scripts/cargo build -p xvision-cli && scripts/cargo test -p xvision-cli spot`
Expected: builds; `no_ack_is_preview_ack_is_live` passes.

- [ ] **Step 7: Verify the gate wiring by inspection**

Confirm in `spot.rs::run` that `check_not_paused(&xvn_home, true)` is called **before** `surface.submit_order` whenever `mode == Live`, and that the default (no flag) path never calls it and emits `--dry-run`. Confirm a paused DB blocks a `--i-understand-real-money` run:

```bash
# (manual, optional) with a paused xvn.db present:
cargo run -p xvision-cli -- spot --side buy --symbol SOL --amount 10 --i-understand-real-money
# expect: "system is paused via the safety kill-switch" and no swap.
```

- [ ] **Step 8: Manual smoke (no funds) — validates C0 grounding end-to-end**

```bash
cargo run -p xvision-cli -- spot --side buy --symbol SOL --amount 10
# expect: a dry-run preview JSON (routing/price), zero funds moved.
cargo run -p xvision-cli -- spot --side buy --symbol AAPLx --amount 10   # one xStock
```

If the JSON shape differs from `SwapResult`/`PricePayload`, correct those structs (and the C0 doc) — this is the grounding feedback loop. Record the smoke output in the C0 grounding doc.

- [ ] **Step 9: Commit**

```bash
git add crates/xvision-cli/src/commands/spot.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/src/lib.rs
git commit -m "feat(byreal-spot): xvn spot — gated one-shot swap (dry-run default)"
```

> **Phase C is independently shippable here.** Consider opening a PR for Phase C before starting Phase A, or continue on the same branch.

---

# PHASE A — agent-driven gated live (`byreal_spot` LiveVenue)

Phase A makes a `byreal_spot` run flow through `Executor::live`, inheriting `GatedBrokerSurface` automatically (it wraps **every** broker in `build_live_executor`). We add: a `LiveVenue` variant, three label/network-helper arms, a poll-only price stream, and an integration test.

## Task A1: `LiveVenue::ByrealSpot` + resolve/label/network arms

The three helpers (`resolve_live_venue`, `broker_label_for`, `check_venue_label_network`) and `build_live_executor` all live in `crates/xvision-engine/src/api/eval.rs`. We thread a new `byreal_spot_network: Option<&str>` through the three helpers (analogous to `degen_network`/`hl_network`).

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs`
- Test: inline `#[cfg(test)] mod broker_label_for_tests` (~line 5995) + a new `resolve_live_venue` test

- [ ] **Step 1: Write the failing tests**

In `mod broker_label_for_tests` add:

```rust
#[test]
fn byreal_spot_unset_maps_to_live() {
    assert_eq!(
        broker_label_for(LiveVenue::ByrealSpot, None, None, None, None),
        VenueLabel::Live,
        "ByrealSpot + unset network → Live (fail-safe)"
    );
}
#[test]
fn byreal_spot_testnet_maps_to_testnet() {
    assert_eq!(
        broker_label_for(LiveVenue::ByrealSpot, None, None, None, Some("testnet")),
        VenueLabel::Testnet
    );
}
#[test]
fn resolve_byreal_spot_ok() {
    assert_eq!(
        resolve_live_venue("byreal_spot", None, None, None, None, None).unwrap(),
        LiveVenue::ByrealSpot
    );
}
```

(The `broker_label_for` / `resolve_live_venue` calls now take one extra trailing `Option<&str>` — the tests encode the new arity. Update existing `broker_label_for(...)` / `resolve_live_venue(...)` test calls in this module to pass the extra `None` for `byreal_spot_network`.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `scripts/cargo test -p xvision-engine broker_label_for_tests`
Expected: FAIL — `LiveVenue::ByrealSpot` missing + arity mismatch.

- [ ] **Step 3: Add the enum variant**

In the `LiveVenue` enum (~line 3496), after `ByrealLive`:

```rust
    /// Byreal Solana spot (curated SPL + xStocks) via `byreal-cli`. Spot is
    /// Long/Flat only; marks come from byreal-cli token price (poll-only, no
    /// Alpaca data). Mainnet gated by `venue_label`=Live + the SafetyGate,
    /// like ByrealLive.
    ByrealSpot,
```

- [ ] **Step 4: Add the `resolve_live_venue` arm + new param**

Add `byreal_spot_network: Option<&str>` as the last parameter of `resolve_live_venue` (prefix `_` — like `_byreal_network`, gating is via venue_label). Add the arm after `"byreal"`:

```rust
        "byreal_spot" => {
            // Solana spot via byreal-cli. Testnet/mainnet split carried by the
            // run's venue_label + SafetyGate (mirrors "byreal"), not refused here.
            Ok(LiveVenue::ByrealSpot)
        }
```

Extend the `other =>` error message's supported-venues list with `"byreal_spot"`.

- [ ] **Step 5: Add the `broker_label_for` arm + new param**

Add `byreal_spot_network: Option<&str>` as the last param of `broker_label_for`. Add the arm:

```rust
        LiveVenue::ByrealSpot => label_from_network(byreal_spot_network),
```

- [ ] **Step 6: Add the `check_venue_label_network` arm**

In the `env_var` match inside `check_venue_label_network`, add:

```rust
        LiveVenue::ByrealSpot => "BYREAL_SPOT_NETWORK",
```

- [ ] **Step 7: Update the three call sites in `build_live_executor`**

After `let byreal_network = std::env::var("BYREAL_NETWORK").ok();` (~line 3751) add:

```rust
    let byreal_spot_network = std::env::var("BYREAL_SPOT_NETWORK").ok();
```

Then pass `byreal_spot_network.as_deref()` as the new trailing arg to `resolve_live_venue(...)` and to `broker_label_for(...)`. (`check_venue_label_network` takes no network string — it only adds the match arm in Step 6.)

- [ ] **Step 8: Run the tests to verify they pass**

Run: `scripts/cargo test -p xvision-engine broker_label_for_tests resolve_byreal_spot`
Expected: PASS. Also run `scripts/cargo build -p xvision-engine` to confirm the arity change compiles across all call sites.

- [ ] **Step 9: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(byreal-spot): LiveVenue::ByrealSpot + resolve/label/network arms"
```

---

## Task A2: `ByrealSpotPriceFetcher` (poll-only marks)

The live loop needs a bar stream. Spot has no OHLCV history from byreal-cli, so the fetcher returns a single synthetic bar (o=h=l=c=latest price) per poll. This satisfies `LivePollFetcher` for `LiveStream::new_poll_only`.

**Files:**
- Modify: `crates/xvision-execution/src/byreal_spot.rs`
- Test: inline `#[cfg(test)]`

> First confirm the trait name + signature to implement. Inspect `crates/xvision-data/src/alpaca_live_poll.rs` for the `LivePollFetcher` trait (`async fn fetch_window(&self, asset, granularity, start, end) -> Result<Vec<MarketBar>, AlpacaPollError>`) and `MarketBar` fields (`timestamp, open, high, low, close, volume`). Match them exactly. The fetcher resolves the asset symbol → mint via the curated `SpotAssetConfig` it holds.

- [ ] **Step 1: Write the failing test**

```rust
#[tokio::test]
async fn price_fetcher_returns_one_synthetic_bar() {
    use xvision_data::alpaca::BarGranularity;
    let api = MockSpotApi { price: 150.0, ..Default::default() };
    let fetcher = ByrealSpotPriceFetcher::new(api, curated());
    let now = chrono::Utc::now();
    let bars = fetcher
        .fetch_window("SOL", BarGranularity::Minute1, now, now)
        .await
        .unwrap();
    assert_eq!(bars.len(), 1);
    assert_eq!(bars[0].close, 150.0);
    assert_eq!(bars[0].open, 150.0);
    assert_eq!(bars[0].high, 150.0);
    assert_eq!(bars[0].low, 150.0);
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `scripts/cargo test -p xvision-execution price_fetcher_returns_one`
Expected: FAIL — type missing (and `xvision-data` may need adding to `[dependencies]` / `[dev-dependencies]` of `xvision-execution` — check `crates/xvision-execution/Cargo.toml`; if absent, add `xvision-data = { path = "../xvision-data" }`).

- [ ] **Step 3: Implement the fetcher**

```rust
use xvision_data::alpaca::{BarGranularity, MarketBar};
use xvision_data::alpaca_live_poll::{AlpacaPollError, LivePollFetcher};

/// `LivePollFetcher` that turns the latest byreal-cli token price into a single
/// synthetic OHLCV bar (o=h=l=c=price). No history — v1 forward-test marks.
pub struct ByrealSpotPriceFetcher<A = SubprocessByrealSpotApi> {
    api: A,
    assets: SpotAssetConfig,
}

impl<A: ByrealSpotApi> ByrealSpotPriceFetcher<A> {
    pub fn new(api: A, assets: SpotAssetConfig) -> Self {
        Self { api, assets }
    }
}

#[async_trait]
impl<A: ByrealSpotApi + 'static> LivePollFetcher for ByrealSpotPriceFetcher<A> {
    async fn fetch_window(
        &self,
        asset: &str,
        _granularity: BarGranularity,
        _start: chrono::DateTime<chrono::Utc>,
        end: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<MarketBar>, AlpacaPollError> {
        let entry = self
            .assets
            .resolve(asset)
            .ok_or_else(|| AlpacaPollError::Other(format!("byreal_spot: '{asset}' not curated")))?;
        let price = self
            .api
            .token_price(&entry.mint)
            .await
            .map_err(|e| AlpacaPollError::Other(format!("byreal_spot price: {e}")))?;
        Ok(vec![MarketBar {
            timestamp: end,
            open: price,
            high: price,
            low: price,
            close: price,
            volume: 0.0,
        }])
    }
}
```

> Confirm `AlpacaPollError` has an `Other(String)` (or equivalent free-form) variant; if not, use the closest existing variant and adjust. Check `crates/xvision-data/src/alpaca_live_poll.rs`.

- [ ] **Step 4: Run to verify it passes**

Run: `scripts/cargo test -p xvision-execution price_fetcher_returns_one`
Expected: PASS.

- [ ] **Step 5: Export + commit**

Add `ByrealSpotPriceFetcher` to the `pub use byreal_spot::{…}` re-export in `lib.rs`.

```bash
git add crates/xvision-execution/src/byreal_spot.rs crates/xvision-execution/src/lib.rs crates/xvision-execution/Cargo.toml
git commit -m "feat(byreal-spot): poll-only price fetcher (synthetic bar from token price)"
```

---

## Task A3: Wire `ByrealSpot` into `build_live_executor`

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs`

- [ ] **Step 1: Add the broker-construction arm**

In the `match venue { … }` broker block (~line 3855), after the `LiveVenue::ByrealLive => …` arm:

```rust
            LiveVenue::ByrealSpot => {
                let cfg_path = xvision_core::config::spot_assets_path(&ctx.xvn_home);
                let assets = xvision_core::config::load_spot_assets(&cfg_path).map_err(|e| {
                    ApiError::Validation(format!(
                        "byreal_spot requires a curated set at {}: {e}",
                        cfg_path.display()
                    ))
                })?;
                // venue_label decides the swap mode: Live → real --confirm,
                // Testnet/Paper → --dry-run preview (forward-test).
                let mode = if cfg.venue_label == VenueLabel::Live {
                    xvision_execution::ByrealSpotMode::Live
                } else {
                    xvision_execution::ByrealSpotMode::Preview
                };
                Arc::new(
                    xvision_execution::ByrealSpotSurface::new(
                        xvision_execution::SubprocessByrealSpotApi::from_env(),
                        assets,
                    )
                    .with_mode(mode),
                )
            }
```

- [ ] **Step 2: Exclude ByrealSpot from Alpaca data**

Update the `uses_alpaca_data` line (~line 3776):

```rust
    let uses_alpaca_data = venue != LiveVenue::DegenArena
        && venue != LiveVenue::Hyperliquid
        && venue != LiveVenue::ByrealSpot;
```

- [ ] **Step 3: Add the poll-only stream branch for ByrealSpot**

The current `else` branch builds an HL fetcher. Make the non-Alpaca branch select by venue. At the top of the `else` block (~line 3970), special-case ByrealSpot before the HL logic:

```rust
        } else if venue == LiveVenue::ByrealSpot {
            // Solana spot: poll byreal-cli token price → single synthetic bar.
            // No warmup history available in v1 (forward-test marks).
            let cfg_path = xvision_core::config::spot_assets_path(&ctx.xvn_home);
            let assets = xvision_core::config::load_spot_assets(&cfg_path)
                .map_err(|e| ApiError::Validation(format!("byreal_spot curated set: {e}")))?;
            let fetcher = xvision_execution::ByrealSpotPriceFetcher::new(
                xvision_execution::SubprocessByrealSpotApi::from_env(),
                assets,
            );
            let poll = AlpacaLivePoll::new(fetcher, asset.clone(), granularity);
            crate::eval::executor::LiveStream::new_poll_only(Vec::new(), poll)
        } else {
            // … existing HL (DegenArena / Hyperliquid) branch unchanged …
```

> Confirm `AlpacaLivePoll::new` accepts the `ByrealSpotPriceFetcher` (it is generic over `LivePollFetcher`). If `AlpacaLivePoll::new` needs an `Arc`, wrap accordingly (the gated_live_submit test wraps the fetcher in `Arc::new(...)`). Match the call shape used by the HL branch.

- [ ] **Step 4: Build + run the existing live tests**

Run: `scripts/cargo build -p xvision-engine && scripts/cargo test -p xvision-engine eval_executor_live_loop gated_live_submit`
Expected: build clean; pre-existing live tests still pass (no regression).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(byreal-spot): wire ByrealSpot into build_live_executor (poll-only marks)"
```

---

## Task A4: Integration test — gate inheritance for byreal_spot

Proves the spec's Phase-A safety claims using the `gated_live_submit.rs` pattern: a paused gate ⇒ zero swaps reach the inner broker, and a `venue_label` mismatch ⇒ blocked. We assert at the `GatedBrokerSurface` layer wrapping a `ByrealSpotSurface` over a recording mock API (no subprocess, no funds).

**Files:**
- Create: `crates/xvision-engine/tests/gated_byreal_spot_submit.rs`

- [ ] **Step 1: Write the test**

Model on `crates/xvision-engine/tests/gated_live_submit.rs`. Build a `ByrealSpotSurface` over an in-test recording `ByrealSpotApi` mock (count swaps), wrap it in `GatedBrokerSurface`, and assert:

```rust
//! byreal_spot inherits the SafetyGate via GatedBrokerSurface:
//! (a) paused gate ⇒ ZERO swaps reach the inner ByrealSpotSurface;
//! (b) Paper-run + Live-broker label mismatch ⇒ blocked, ZERO swaps;
//! (c) allow_all + matching labels ⇒ the swap reaches the inner surface.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use xvision_core::config::{SpotAssetConfig, SpotAssetEntry, SpotAssetKind};
use xvision_execution::broker_surface::{BrokerSurface, OrderRequest, Side};
use xvision_execution::byreal_spot::{ByrealSpotApi, ByrealSpotMode, ByrealSpotSurface, SwapResult};
use xvision_execution::executor::ExecutorError;
use xvision_engine::eval::executor::GatedBrokerSurface;
use xvision_engine::safety::{AuthContext, SafetyGate, SafetyManager, VenueLabel};

#[derive(Default, Clone)]
struct CountingSpotApi {
    swaps: Arc<Mutex<u32>>,
}

#[async_trait]
impl ByrealSpotApi for CountingSpotApi {
    async fn swap(&self, _i: &str, _o: &str, _a: f64, _s: u32, _m: ByrealSpotMode) -> Result<SwapResult, ExecutorError> {
        *self.swaps.lock().unwrap() += 1;
        Ok(SwapResult { signature: Some("sig".into()), out_amount: 1.0, price_impact_bps: None })
    }
    async fn token_price(&self, _m: &str) -> Result<f64, ExecutorError> { Ok(150.0) }
    async fn token_balance(&self, _m: &str) -> Result<f64, ExecutorError> { Ok(10.0) }
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

fn buy() -> OrderRequest {
    OrderRequest {
        asset: "SOL".into(),
        side: Side::Buy,
        size: 1.0,
        reference_price_usd: 150.0,
        stop_loss_pct: None,
        take_profit_pct: None,
        idempotency_key: "k1".into(),
    }
}

async fn paused_gate() -> SafetyGate {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/030_safety_state_and_audit.sql")).execute(&pool).await.unwrap();
    let mgr = SafetyManager::new(pool);
    mgr.bootstrap(false).await.unwrap();
    mgr.pause(Some("test".into()), &AuthContext::system()).await.unwrap();
    SafetyGate::new(mgr)
}

#[tokio::test]
async fn paused_gate_blocks_spot_swaps() {
    let api = CountingSpotApi::default();
    let swaps = api.swaps.clone();
    let inner: Arc<dyn BrokerSurface> = Arc::new(ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live));
    let gated = GatedBrokerSurface::new(inner, paused_gate().await, VenueLabel::Paper, VenueLabel::Paper, AuthContext::system());
    assert!(gated.submit_order(buy()).await.is_err());
    assert_eq!(*swaps.lock().unwrap(), 0, "paused gate must block all swaps");
}

#[tokio::test]
async fn venue_label_mismatch_blocks_spot_swaps() {
    use sqlx::sqlite::SqlitePoolOptions;
    let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::query(include_str!("../migrations/030_safety_state_and_audit.sql")).execute(&pool).await.unwrap();
    let mgr = SafetyManager::new(pool);
    mgr.bootstrap(false).await.unwrap();
    let gate = SafetyGate::new(mgr); // not paused; mismatch is the only denial
    let api = CountingSpotApi::default();
    let swaps = api.swaps.clone();
    let inner: Arc<dyn BrokerSurface> = Arc::new(ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Live));
    // Paper run + Live broker label ⇒ VenueLabelMismatch.
    let gated = GatedBrokerSurface::new(inner, gate, VenueLabel::Paper, VenueLabel::Live, AuthContext::system());
    assert!(gated.submit_order(buy()).await.is_err());
    assert_eq!(*swaps.lock().unwrap(), 0, "venue-label mismatch must block the swap");
}

#[tokio::test]
async fn allow_all_gate_lets_spot_swap_through() {
    let api = CountingSpotApi::default();
    let swaps = api.swaps.clone();
    let inner: Arc<dyn BrokerSurface> = Arc::new(ByrealSpotSurface::new(api, curated()).with_mode(ByrealSpotMode::Preview));
    let gated = GatedBrokerSurface::new(inner, SafetyGate::allow_all(), VenueLabel::Paper, VenueLabel::Paper, AuthContext::system());
    gated.submit_order(buy()).await.unwrap();
    assert_eq!(*swaps.lock().unwrap(), 1, "allow_all gate must let the swap through");
}
```

> Confirm `byreal_spot`'s test-visibility: the test imports `xvision_execution::byreal_spot::{…}`. Ensure those types are `pub` (the trait, `ByrealSpotMode`, `SwapResult`, `ByrealSpotSurface`) — they are per C2/C3. If the module path isn't public, import via the crate root re-exports instead (`xvision_execution::{ByrealSpotSurface, ByrealSpotMode, …}`).

- [ ] **Step 2: Run the test**

Run: `scripts/cargo test -p xvision-engine --test gated_byreal_spot_submit`
Expected: PASS (all 3).

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-engine/tests/gated_byreal_spot_submit.rs
git commit -m "test(byreal-spot): gate inheritance — paused/mismatch block swaps"
```

---

## Task A5: Final verification + docs

**Files:**
- Modify: `docs/superpowers/specs/2026-06-15-byreal-solana-spot-trading-design.md` (status line)
- Optionally: `MANUAL.md` / dashboard wiki entry for `xvn spot` (operator-surface)

- [ ] **Step 1: Workspace build + full test of touched crates**

```bash
scripts/cargo build --workspace
scripts/cargo test -p xvision-core -p xvision-execution -p xvision-engine -p xvision-cli
```

Expected: clean build; all new tests green; no regression in existing live tests. (Per the baseline-rot memory, `git stash` + re-run any red test to confirm it's pre-existing, not introduced here.)

- [ ] **Step 2: Format only changed files**

Per the cargo-fmt memory, format only the files this plan touched (NOT a workspace `cargo fmt`):

```bash
rustfmt --edition 2021 \
  crates/xvision-core/src/config.rs \
  crates/xvision-execution/src/byreal_spot.rs \
  crates/xvision-execution/src/lib.rs \
  crates/xvision-cli/src/commands/spot.rs \
  crates/xvision-cli/src/lib.rs \
  crates/xvision-engine/src/api/eval.rs \
  crates/xvision-engine/tests/gated_byreal_spot_submit.rs
```

- [ ] **Step 3: Flip the spec status**

Change the design spec's status line from "Ready for an implementation plan" to "Implemented (Phase C + A) — see plan 2026-06-15-byreal-solana-spot-trading.md".

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "docs(byreal-spot): mark design implemented; operator notes for xvn spot"
```

- [ ] **Step 5: Finish the branch**

Use `superpowers:finishing-a-development-branch` to decide merge/PR. Phase C and Phase A can be one PR or two (Phase C is independently shippable). No real funds moved by any automated test; the only real-money path requires `--i-understand-real-money` (CLI) or `venue_label=Live` + a configured keystore (agent run).

---

## Self-review notes (author)

- **Spec coverage:** §3.1 ByrealSpotSurface → C2/C3; §3.2 Phase C `xvn spot` → C5; §3.3 Phase A LiveVenue wiring → A1/A3; §3.4 curated config → C1; §5 safety (dry-run default, kill-switch, slippage cap, Long/Flat) → C3/C5/A3; §7 testing (mock surface, dry-run default, paused/mismatch gate) → C3/C5/A4. RFQ/xStocks "free" → no task needed (xStocks are plain SPL entries in C1; RFQ is auto-routed inside the swap). Out-of-scope items (§8) intentionally have no tasks.
- **Type consistency:** `ByrealSpotMode` (Preview/Live), `SwapResult`, `SpotAssetConfig`/`SpotAssetEntry`/`SpotAssetKind`, `ByrealSpotSurface::new(api, assets).with_mode().with_slippage_bps()`, `ByrealSpotApi::{swap, token_price, token_balance}` are used consistently across C2–A4.
- **Open confirmations (flagged inline, not placeholders):** exact byreal-cli flags + JSON field names (C0 grounds them; structs use `#[serde(alias)]` to tolerate variants); `LivePollFetcher`/`AlpacaPollError`/`MarketBar` exact shapes (A2 Step 0 confirms against `xvision-data`); `AlpacaLivePoll::new` Arc-vs-owned (A3 Step 3 matches the HL branch). These are real "verify the neighbor's signature" steps, not vague TODOs.
- **Gate inheritance:** verified in current code — `build_live_executor` wraps **every** broker (incl. overrides) in `GatedBrokerSurface`, so ByrealSpot needs no explicit gate call; A4 proves it.
