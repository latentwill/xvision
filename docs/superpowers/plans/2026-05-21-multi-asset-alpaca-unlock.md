# Multi-Asset — Alpaca crypto unlock (residual)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Drop the residual BTC-only wall on the Alpaca executor so any whitelisted Alpaca crypto pair (ETH, SOL, LTC, AVAX, …) can flow end-to-end through `xvn ab-compare`, scenarios, and the eval pipeline. The bars-cache layer + Alpaca historical fetcher already shipped (see `docs/superpowers/plans/2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md` Tasks 1–8 + 14). This plan covers the residual code-level wall removal: expanding `AssetSymbol`, lifting the `assets.len() != 1` scenario validator, generalizing the executor's symbol parser, and cascading the F18 `TraderDecision.asset` partial.

**Architecture:** Three thin layers. (a) The core domain enum `AssetSymbol` widens from BTC-only to the full Alpaca crypto whitelist already encoded in `xvision-data::asset_whitelist::ALPACA_CRYPTO_WHITELIST`. (b) The executor adapter (`crates/xvision-execution/src/alpaca.rs`) loses its hardcoded BTC parse and delegates to `AssetSymbol::from_str`. (c) `Scenario::validate_v1` drops the `asset.len() != 1` reject so multi-symbol windows parse — though for v1 of THIS plan the scenario surface still ships single-asset by convention; the wall removal is the prerequisite for F30 multi-asset. F18 partial `TraderDecision.asset` rides along so downstream sites can resolve the active asset without re-reading the scenario.

**Tech Stack:** Rust 2021, existing serde / chrono / sqlx machinery. No new crate dependencies. No SQLite schema migration — `bars_cache` keyed by asset already supports the wider whitelist; scenario rows already store asset as text.

**Reference spec:** `docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md` §§5–7, §13, §17 (the broader Custom-Scenario design). This plan implements the residual asset-unlock slice only.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-core/src/assets.rs` | Modify | Expand `AssetSymbol` enum from `Btc` only to the full Alpaca crypto whitelist (BTC, ETH, LTC, SOL, AVAX, LINK, AAVE, UNI, DOT, DOGE, SHIB, MATIC, BCH, USDT, USDC). Add `FromStr`, `Display`, `as_short`, `as_alpaca_pair` impls. |
| `crates/xvision-core/src/trading.rs` | Modify | Add `asset: Option<AssetSymbol>` field to `TraderDecision` (F18 partial; defaults to `None`, resolved downstream from the scenario's single asset). |
| `crates/xvision-execution/src/alpaca.rs` | Modify | Delete the BTC-only header comment. Replace the hardcoded BTC parse at line 46 with a delegation to `AssetSymbol::from_str`. Generalize `alpaca_symbol_for` to use `AssetSymbol::as_alpaca_pair`. |
| `crates/xvision-engine/src/eval/scenario.rs` | Modify | Drop the `if self.asset.len() != 1` reject at `validate_v1` (line 263). Replace with whitelist membership check via `xvision_data::asset_whitelist::is_alpaca_crypto_supported` on each asset. |
| `crates/xvision-engine/src/eval/scenario.rs` | Modify | Update `ScenarioValidationError` to add an `UnsupportedAsset(String)` variant (or reuse existing variant if there is a near match). Keep the v1 convention of `asset.len() == 1` documented but enforced at higher levels (CLI/UI), not the type validator. |
| `crates/xvision-engine/tests/scenario_multi_asset.rs` | Create | New integration tests asserting (a) `validate_v1` accepts an ETH-only scenario, (b) `validate_v1` accepts an array of whitelisted assets without rejecting, (c) `validate_v1` rejects an unknown asset (e.g. `XRP`). |
| `crates/xvision-execution/tests/alpaca_symbol_parser.rs` | Create | New integration tests covering BTC, ETH, SOL parse and round-trip; assert that `XRP` returns the expected error. |
| `crates/xvision-cli/src/commands/ab_compare.rs` | Modify | Replace any hardcoded `--asset BTC` defaulting with delegation to `AssetSymbol::from_str`; error message lists the v1 whitelist. |
| `crates/xvision-core/tests/asset_symbol.rs` | Create | Unit test that `AssetSymbol::from_str` covers every entry in `ALPACA_CRYPTO_WHITELIST`. |
| `docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md` | Already annotated | Status note added (see plan-level annotation). No further edits in this plan. |

---

## Phase 1 — Domain enum expansion (core)

**Files:** `crates/xvision-core/src/assets.rs`, `crates/xvision-core/tests/asset_symbol.rs`

- [ ] **Task 1.1: Write failing test for whitelist coverage**

```rust
// crates/xvision-core/tests/asset_symbol.rs
use std::str::FromStr;
use xvision_core::assets::AssetSymbol;

#[test]
fn asset_symbol_covers_alpaca_crypto_whitelist() {
    for sym in &["BTC", "ETH", "LTC", "SOL", "AVAX", "LINK", "AAVE", "UNI",
                 "DOT", "DOGE", "SHIB", "MATIC", "BCH", "USDT", "USDC"] {
        assert!(AssetSymbol::from_str(sym).is_ok(), "missing variant: {sym}");
    }
}

#[test]
fn asset_symbol_rejects_unknown() {
    assert!(AssetSymbol::from_str("XRP").is_err());
    assert!(AssetSymbol::from_str("DOGEUSDT").is_err());
}
```

- [ ] **Task 1.2: Run, expect FAIL**

```bash
cargo test -p xvision-core asset_symbol
```

- [ ] **Task 1.3: Expand the enum + impls per the spec snippet in the M1 plan (Task 9 step 3)**

The variants, `FromStr`, `as_short`, `as_alpaca_pair`, and `Display` impls are spelled out in `docs/superpowers/plans/2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md` Task 9 step 3. Copy that block verbatim and place it in `crates/xvision-core/src/assets.rs`.

- [ ] **Task 1.4: Run test, expect PASS**

```bash
cargo test -p xvision-core asset_symbol
```

- [ ] **Task 1.5: `cargo build --workspace` to surface downstream match-exhaustiveness errors**

Expected: a handful of exhaustive-match errors in `xvision-execution`, `xvision-eval`, report renderers, fee-schedule lookups. Patch each by mapping the new variants to a sensible default (fee schedule unchanged for now; report renderer prints the symbol).

- [ ] **Task 1.6: Commit**

```bash
git add crates/xvision-core/src/assets.rs crates/xvision-core/tests/asset_symbol.rs
git commit -m "feat(xvision-core): AssetSymbol covers full Alpaca crypto whitelist"
```

---

## Phase 2 — F18 partial: `TraderDecision.asset`

**Files:** `crates/xvision-core/src/trading.rs`, all callers across the workspace.

- [ ] **Task 2.1: Add the field**

```rust
pub struct TraderDecision {
    // ... existing fields ...
    pub asset: Option<AssetSymbol>,   // F18 partial: defaults to None; resolved from scenario.asset[0] downstream
}
```

- [ ] **Task 2.2: `cargo build --workspace`**

Expected: missing-field + pattern-match errors at construction sites in `xvision-engine`, `xvision-eval`, baselines, and tests.

- [ ] **Task 2.3: Patch each caller**

For fresh constructions, set `asset: None`. Downstream consumers (risk, executor, report renderer) fall through to the scenario's single asset via `decision.asset.unwrap_or(scenario.asset[0])`. F18 proper cascades the full resolution later (covered by F30 multi-asset; out of scope here).

- [ ] **Task 2.4: Run workspace tests**

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Task 2.5: Commit**

```bash
git add -p
git commit -m "feat(core): TraderDecision.asset partial field (F18; resolves from scenario downstream)"
```

---

## Phase 3 — Drop BTC-only wall in `xvision-execution`

**Files:** `crates/xvision-execution/src/alpaca.rs`, `crates/xvision-execution/tests/alpaca_symbol_parser.rs`

- [ ] **Task 3.1: Write failing tests covering ETH/SOL parse**

```rust
// crates/xvision-execution/tests/alpaca_symbol_parser.rs
use xvision_execution::alpaca::{parse_alpaca_asset, alpaca_symbol_for};
use xvision_core::assets::AssetSymbol;

#[test]
fn parse_accepts_btc_eth_sol() {
    assert_eq!(parse_alpaca_asset("BTC"), Some(AssetSymbol::Btc));
    assert_eq!(parse_alpaca_asset("ETH/USD"), Some(AssetSymbol::Eth));
    assert_eq!(parse_alpaca_asset("SOL"), Some(AssetSymbol::Sol));
}

#[test]
fn alpaca_symbol_for_returns_pair_form() {
    assert_eq!(alpaca_symbol_for(AssetSymbol::Eth), "ETH/USD");
    assert_eq!(alpaca_symbol_for(AssetSymbol::Sol), "SOL/USD");
}

#[test]
fn parse_rejects_unknown() {
    assert_eq!(parse_alpaca_asset("XRP"), None);
}
```

- [ ] **Task 3.2: Run, expect FAIL**

```bash
cargo test -p xvision-execution alpaca_symbol_parser
```

- [ ] **Task 3.3: Delete the BTC-only header comment**

Open `crates/xvision-execution/src/alpaca.rs`. Delete the line `//! v1 scope: BTC-only via Alpaca's crypto endpoint (BTC/USD).` (line 3 per the M1 plan reference).

- [ ] **Task 3.4: Generalize the parser**

Replace the hardcoded BTC match around line 46 with delegation:

```rust
pub fn parse_alpaca_asset(s: &str) -> Option<AssetSymbol> {
    s.parse().ok()
}
```

- [ ] **Task 3.5: Generalize `alpaca_symbol_for`**

Replace `symbol_for_btc: &'static str` (or equivalent BTC-only constant) with:

```rust
pub fn alpaca_symbol_for(asset: AssetSymbol) -> String {
    asset.as_alpaca_pair()
}
```

Update all call sites accordingly.

- [ ] **Task 3.6: Run tests, expect PASS**

```bash
cargo test -p xvision-execution
```

If a pre-existing test asserted BTC-only behaviour, update the assertion to verify "the configured asset is used" instead of "BTC is used."

- [ ] **Task 3.7: Commit**

```bash
git add crates/xvision-execution/src/alpaca.rs crates/xvision-execution/tests/alpaca_symbol_parser.rs
git commit -m "feat(xvision-execution): drop BTC-only wall in Alpaca executor"
```

---

## Phase 4 — Scenario validator: drop `asset.len() != 1` reject

**Files:** `crates/xvision-engine/src/eval/scenario.rs`, `crates/xvision-engine/tests/scenario_multi_asset.rs`

- [ ] **Task 4.1: Write failing test exercising the validator**

```rust
// crates/xvision-engine/tests/scenario_multi_asset.rs
use xvision_engine::eval::scenario::{Scenario, ScenarioValidationError};
// (fixture helper imports as needed)

#[test]
fn validate_v1_accepts_eth_only_scenario() {
    let s = Scenario::test_fixture_eth_only();   // small helper that builds a one-asset ETH scenario
    assert!(s.validate_v1().is_ok());
}

#[test]
fn validate_v1_accepts_two_whitelisted_assets() {
    let s = Scenario::test_fixture_eth_and_sol();
    assert!(s.validate_v1().is_ok());
}

#[test]
fn validate_v1_rejects_unknown_asset() {
    let s = Scenario::test_fixture_with_unknown_asset("XRP");
    assert!(matches!(s.validate_v1(), Err(ScenarioValidationError::UnsupportedAsset(_))));
}
```

- [ ] **Task 4.2: Run, expect FAIL**

```bash
cargo test -p xvision-engine scenario_multi_asset
```

- [ ] **Task 4.3: Edit `validate_v1` (currently at line ~262)**

Delete the `if self.asset.len() != 1 { ... }` block. Replace with a whitelist check per asset:

```rust
for asset in &self.asset {
    if !xvision_data::asset_whitelist::is_alpaca_crypto_supported(asset.as_short()) {
        return Err(ScenarioValidationError::UnsupportedAsset(asset.as_short().to_string()));
    }
}
```

- [ ] **Task 4.4: Add the `UnsupportedAsset` variant to `ScenarioValidationError`**

```rust
#[derive(Debug, thiserror::Error)]
pub enum ScenarioValidationError {
    // ... existing variants ...
    #[error("asset '{0}' is not in the Alpaca crypto whitelist")]
    UnsupportedAsset(String),
}
```

- [ ] **Task 4.5: Run tests, expect PASS**

```bash
cargo test -p xvision-engine scenario_multi_asset
cargo test --workspace
```

Workspace sweep catches any caller that pattern-matched on the deleted error variant or relied on the `len() != 1` rejection (CLI error messages, validation report renderers).

- [ ] **Task 4.6: Commit**

```bash
git add crates/xvision-engine/src/eval/scenario.rs crates/xvision-engine/tests/scenario_multi_asset.rs
git commit -m "feat(xvision-engine): drop scenario asset.len()!=1 wall; whitelist-check assets"
```

---

## Phase 5 — CLI wiring & docs cleanup

**Files:** `crates/xvision-cli/src/commands/ab_compare.rs`, `docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md` (already annotated; verify only).

- [ ] **Task 5.1: Replace hardcoded BTC defaulting**

In `ab_compare.rs`, replace any `--asset` default of `"BTC"` or `parse_asset_btc_only` call with delegation to `AssetSymbol::from_str`. Update the `--asset` CLI help text to list the v1 whitelist explicitly.

- [ ] **Task 5.2: Smoke test against a known-good ETH window**

```bash
cargo run --bin xvn -- ab-compare --asset ETH --from 2024-02-03 --to 2024-02-10 --granularity 1h --arms buy_and_hold --output /tmp/eth.json
cargo run --bin xvn -- ab-compare --asset XRP --from 2024-02-03 --to 2024-02-10 --granularity 1h --arms buy_and_hold --output /tmp/xrp.json
```

Expected: ETH passes end-to-end through cache + fetcher + executor. XRP errors with a friendly "asset XRP not in the Alpaca crypto whitelist" message.

- [ ] **Task 5.3: Final workspace sweep**

```bash
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

- [ ] **Task 5.4: Commit & merge**

```bash
git add -p
git commit -m "feat(cli): ab-compare --asset accepts full Alpaca crypto whitelist"
```

---

## Acceptance

- Every entry in `xvision_data::asset_whitelist::ALPACA_CRYPTO_WHITELIST` parses through `AssetSymbol::from_str`.
- `crates/xvision-execution/src/alpaca.rs` no longer carries the BTC-only header comment, and its parser delegates to the core enum.
- `Scenario::validate_v1` accepts ETH-only and multi-whitelisted-asset scenarios; rejects unknown assets with `UnsupportedAsset`.
- `xvn ab-compare --asset ETH --from … --to …` runs end-to-end against the bars-cache + Alpaca fetcher already shipped in the M1 plan.
- `cargo test --workspace` passes; `cargo clippy --workspace -- -D warnings` clean.

## Out of scope (deferred to F30 multi-asset)

- Routing a single Live or Backtest run across multiple simultaneous assets (the validator wall is dropped but the CLI / scenario authoring surface still ships single-asset by convention in v1).
- `TraderDecision.asset` full cascade: routing per-decision assets through risk and the executor when the decision's asset differs from the scenario's primary asset. This plan adds the field; the cascade is F18 proper, tracked under F30.
- Equities / options / futures asset classes. The validator now whitelist-checks; non-crypto asset classes remain out of v1.
