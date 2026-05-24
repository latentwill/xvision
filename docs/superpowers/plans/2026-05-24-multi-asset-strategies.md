# Multi-Asset Strategies Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make a strategy trade its declared `asset_universe` against an asset-free scenario — fanning the agent pipeline out per asset with a shared NAV — and respond to multi-asset across CLI and dashboard.

**Architecture:** Asset moves off the `Scenario` (it described a symbol, not a market) onto the `Strategy.asset_universe` (already exists). Pipeline *shape* (fan-out, signal scope, capital pooling) becomes `Strategy`/`PublicManifest` config with v1 defaults, so the harness branches on data instead of hardcoding loop shape — keeping future prompt-optimization hypotheses reachable. v1 implements the `PerAsset` + `Pooled` arms only; other arms parse/validate but return a clear not-implemented error.

**Tech Stack:** Rust 2021 (xvision-core / xvision-engine / xvision-cli), serde, sqlx/SQLite (no migration — scenarios are `body_json`, strategies are filesystem JSON), ts-rs (type export), Vite + React + TanStack Query + Radix + Tailwind (dashboard).

**Spec:** `docs/superpowers/specs/2026-05-24-multi-asset-strategies-design.md`

**Worktree:** `/Users/edkennedy/Code/xvision/.worktrees/multi-asset` (branch `feat/multi-asset`, off clean `main`). Run all commands from this dir. Before any cargo from this worktree, set a per-worktree target dir to avoid colliding with the main checkout:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-multi-asset"
```

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/agent/dispatch_capability.rs` | Modify | Add `SignalScope` enum; add `scope` field to `FilterSignal`. |
| `crates/xvision-engine/src/agent/signal_cache.rs` | Modify | Add `scope` to `SignalCacheKey`; update constructor + tests. |
| `crates/xvision-engine/src/strategies/manifest.rs` | Modify | Add `execution_mode: ExecutionMode`, `capital_mode: CapitalMode` to `PublicManifest`. |
| `crates/xvision-engine/src/strategies/exec_mode.rs` | Create | `ExecutionMode` + `CapitalMode` enums + defaults. |
| `crates/xvision-engine/src/strategies/mod.rs` | Modify | `pub mod exec_mode;` + re-export; populate the new manifest fields in any constructor. |
| `crates/xvision-engine/src/eval/scenario.rs` | Modify | Remove `Scenario.asset`; keep `asset_class`/`quote_currency`. |
| `crates/xvision-engine/src/api/scenario.rs` | Modify | Drop `asset` from `CreateScenarioRequest`/`ScenarioMutations`; drop the `len()==1` gate; per-asset cache-key deferral. |
| `crates/xvision-engine/src/eval/executor/asset_set.rs` | Create | `active_assets(strategy, run_subset) -> Result<Vec<AssetSymbol>>`. |
| `crates/xvision-engine/src/eval/executor/book.rs` | Create | `PortfolioBook` — per-asset positions + shared realized PnL + `equity()`. |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | Modify | Branch on `execution_mode`; per-asset fan-out; per-asset seed; asset-scoped signals; `PortfolioBook` accounting. |
| `crates/xvision-engine/src/eval/executor/paper.rs` | Modify | Same fan-out treatment as backtest. |
| `crates/xvision-engine/src/api/eval.rs` | Modify | `load_bars_for_scenario` parameterized by asset; thread universe/subset. |
| `crates/xvision-cli/src/commands/strategy.rs` | Modify | `--assets` (plural) → `asset_universe`; `--execution-mode`; keep `--asset` as 1-elem alias. |
| `crates/xvision-cli/src/commands/scenario.rs` | Modify | Drop `--asset` from `create`; asset-free request. |
| `crates/xvision-cli/src/commands/eval/mod.rs` | Modify | Add optional `--assets` subset to `RunArgs`. |
| `crates/xvision-cli/src/commands/eval/compare_format.rs` | Modify | Per-asset rollup rows. |
| `frontend/web/src/components/scenario/ScenarioForm.tsx` | Modify | Remove asset picker. |
| `frontend/web/src/routes/authoring.tsx` | Modify | Editable `asset_universe` multi-select. |
| `frontend/web/src/routes/eval-runs-detail.tsx` | Modify | Per-asset rollup view. |
| `crates/xvision-engine/src/api/...` (StrategySummary) | Modify | Expose `asset_universe` + `execution_mode`. |

---

## Phase A — Data model & types (no behavior change)

### Task A1: `SignalScope` enum + `FilterSignal.scope`

**Files:**
- Modify: `crates/xvision-engine/src/agent/dispatch_capability.rs` (after `FilterSignal`, lines ~68–81)

- [ ] **Step 1: Write the failing test** — append to the `#[cfg(test)] mod tests` in `dispatch_capability.rs`:

```rust
#[test]
fn signal_scope_round_trips_each_variant() {
    use xvision_core::trading::AssetSymbol;
    for scope in [
        SignalScope::Global,
        SignalScope::Asset(AssetSymbol::Btc),
        SignalScope::Pair(AssetSymbol::Btc, AssetSymbol::Eth),
        SignalScope::Custom("vol_basket".into()),
    ] {
        let s = serde_json::to_string(&scope).unwrap();
        let back: SignalScope = serde_json::from_str(&s).unwrap();
        assert_eq!(scope, back);
    }
}

#[test]
fn filter_signal_defaults_scope_to_global_when_absent() {
    // Legacy FilterSignal JSON (pre-multi-asset) has no `scope` key.
    let json = serde_json::json!({
        "name": "regime", "payload": {"regime":"trend"},
        "granularity": "bar", "ts": "2026-05-24T00:00:00Z"
    });
    let sig: FilterSignal = serde_json::from_value(json).unwrap();
    assert_eq!(sig.scope, SignalScope::Global);
}
```

- [ ] **Step 2: Run, expect FAIL** (`SignalScope` undefined, `scope` field missing)

```bash
cargo test -p xvision-engine --lib agent::dispatch_capability::tests::signal_scope 2>&1 | tail -20
```

- [ ] **Step 3: Add the enum + field.** In `dispatch_capability.rs`, add near the top (after imports):

```rust
use xvision_core::trading::AssetSymbol;

/// Scope at which a `FilterSignal` is meaningful. First-class so cross-asset
/// and global signals are not second-class "synthetic asset name" hacks.
/// In v1's `PerAsset` fan-out the dispatcher tags signals `Asset(current)`;
/// the other variants exist so future filters emit them with no key migration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SignalScope {
    Global,
    Asset(AssetSymbol),
    Pair(AssetSymbol, AssetSymbol),
    Custom(String),
}

impl Default for SignalScope {
    fn default() -> Self {
        SignalScope::Global
    }
}
```

Add the field to `FilterSignal` (after `ts`):

```rust
    /// Scope this signal applies to. Defaults to `Global` for back-compat
    /// with pre-multi-asset signal JSON.
    #[serde(default)]
    pub scope: SignalScope,
```

- [ ] **Step 4: Fix any `FilterSignal { … }` literals** the compiler flags (they now need `scope`). For existing producers default to `SignalScope::Global` (behavior unchanged); the per-asset dispatcher sets `Asset(..)` in Task B4.

```bash
cargo build -p xvision-engine 2>&1 | tail -20
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib agent::dispatch_capability::tests 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/agent/dispatch_capability.rs
git commit -m "feat(engine): SignalScope enum + FilterSignal.scope (default Global)"
```

### Task A2: `scope` on `SignalCacheKey`

**Files:**
- Modify: `crates/xvision-engine/src/agent/signal_cache.rs:42-54`

- [ ] **Step 1: Update the failing test.** In `signal_cache.rs` tests, replace the two `SignalCacheKey::new("sid", "regime_filter")` style calls and add:

```rust
#[test]
fn keys_differ_by_scope() {
    use crate::agent::dispatch_capability::SignalScope;
    use xvision_core::trading::AssetSymbol;
    let mut c = SignalCache::new();
    let ts = Utc.with_ymd_and_hms(2026, 5, 22, 9, 30, 0).unwrap();
    let btc = SignalCacheKey::new("sid", "regime", SignalScope::Asset(AssetSymbol::Btc));
    let eth = SignalCacheKey::new("sid", "regime", SignalScope::Asset(AssetSymbol::Eth));
    c.insert(btc.clone(), signal_at("regime", ts));
    c.insert(eth.clone(), signal_at("regime", ts));
    assert_eq!(c.len(), 2, "same role, different asset scope must not collide");
    assert!(c.get(&btc).is_some());
    assert!(c.get(&eth).is_some());
}
```

- [ ] **Step 2: Run, expect FAIL** (`new` takes 2 args)

```bash
cargo test -p xvision-engine --lib agent::signal_cache 2>&1 | tail -20
```

- [ ] **Step 3: Add the field.** Replace the struct + constructor:

```rust
use crate::agent::dispatch_capability::{FilterSignal, SignalScope};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignalCacheKey {
    pub strategy_id: String,
    pub role: String,
    pub scope: SignalScope,
}

impl SignalCacheKey {
    pub fn new(strategy_id: impl Into<String>, role: impl Into<String>, scope: SignalScope) -> Self {
        Self { strategy_id: strategy_id.into(), role: role.into(), scope }
    }
}
```

- [ ] **Step 4: Patch existing call sites.** Update the two existing tests' `SignalCacheKey::new(...)` to pass `SignalScope::Global`, and any non-test caller in `filter_dispatch.rs` / `pipeline.rs` to pass `SignalScope::Global` for now (Task B4 switches the executor call sites to `Asset(current)`):

```bash
cargo build -p xvision-engine 2>&1 | tail -20   # surfaces every call site
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib agent::signal_cache 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/agent/signal_cache.rs crates/xvision-engine/src/agent/filter_dispatch.rs crates/xvision-engine/src/agent/pipeline.rs
git commit -m "feat(engine): SignalCacheKey gains scope (callers default Global)"
```

### Task A3: `ExecutionMode` + `CapitalMode` enums

**Files:**
- Create: `crates/xvision-engine/src/strategies/exec_mode.rs`
- Modify: `crates/xvision-engine/src/strategies/mod.rs` (add `pub mod exec_mode;`)

- [ ] **Step 1: Write the failing test** in the new file:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_mode_default_is_per_asset() {
        assert_eq!(ExecutionMode::default(), ExecutionMode::PerAsset);
    }

    #[test]
    fn capital_mode_default_is_pooled() {
        assert_eq!(CapitalMode::default(), CapitalMode::Pooled);
    }

    #[test]
    fn execution_mode_serializes_snake_case() {
        assert_eq!(serde_json::to_value(ExecutionMode::PerAsset).unwrap(), serde_json::json!("per_asset"));
        assert_eq!(serde_json::to_value(ExecutionMode::Portfolio).unwrap(), serde_json::json!("portfolio"));
        let c: ExecutionMode = serde_json::from_value(serde_json::json!({"custom": "rotate"})).unwrap();
        assert_eq!(c, ExecutionMode::Custom("rotate".into()));
    }
}
```

- [ ] **Step 2: Run, expect FAIL** (module empty)

```bash
cargo test -p xvision-engine --lib strategies::exec_mode 2>&1 | tail -20
```

- [ ] **Step 3: Implement the enums** (above the test module):

```rust
//! Strategy-level pipeline-shape config. Per the multi-asset design spec,
//! pipeline shape is Strategy data with a default, not a harness invariant —
//! so prompt-optimization can vary it without engine edits. v1 implements
//! only the default arms; other arms parse + validate but the executor
//! returns a clear not-implemented error.

use serde::{Deserialize, Serialize};

/// How the harness drives a multi-asset universe per bar.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    /// v1: run the pipeline once per active asset each bar.
    #[default]
    PerAsset,
    /// Reserved: one cycle sees all assets; trader reasons as a book.
    Portfolio,
    /// Open hatch for optimizer-authored modes.
    Custom(String),
}

/// How capital is shared across assets in a run.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CapitalMode {
    /// v1: one capital pool, per-asset positions, shared equity.
    #[default]
    Pooled,
    /// Reserved: segregated per-asset sub-portfolios.
    PerAsset,
}
```

- [ ] **Step 4: Wire the module** in `strategies/mod.rs`: add `pub mod exec_mode;` and `pub use exec_mode::{CapitalMode, ExecutionMode};`.

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib strategies::exec_mode 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/strategies/exec_mode.rs crates/xvision-engine/src/strategies/mod.rs
git commit -m "feat(engine): ExecutionMode + CapitalMode strategy config enums"
```

### Task A4: Add the new fields to `PublicManifest`

**Files:**
- Modify: `crates/xvision-engine/src/strategies/manifest.rs:5-35`

- [ ] **Step 1: Write the failing test** in `manifest.rs` (add a test module if absent):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_without_modes_defaults_per_asset_pooled() {
        // Legacy strategy JSON omits the new fields → defaults.
        let json = serde_json::json!({
            "id":"s1","display_name":"d","plain_summary":"",
            "creator":"@x","template":"custom","regime_fit":[],
            "asset_universe":["BTC/USD"],"decision_cadence_minutes":60,
            "attested_with":[],"required_tools":[],
            "risk_preset_or_config":"balanced"
        });
        let m: PublicManifest = serde_json::from_value(json).unwrap();
        assert_eq!(m.execution_mode, crate::strategies::ExecutionMode::PerAsset);
        assert_eq!(m.capital_mode, crate::strategies::CapitalMode::Pooled);
    }
}
```

- [ ] **Step 2: Run, expect FAIL** (no such fields)

```bash
cargo test -p xvision-engine --lib strategies::manifest 2>&1 | tail -20
```

- [ ] **Step 3: Add the fields** to `PublicManifest` (after `color`):

```rust
    /// How the harness drives the asset universe. Defaults to `PerAsset`
    /// so pre-multi-asset strategy JSON parses unchanged.
    #[serde(default)]
    pub execution_mode: crate::strategies::ExecutionMode,
    /// How capital is shared across assets. Defaults to `Pooled`.
    #[serde(default)]
    pub capital_mode: crate::strategies::CapitalMode,
```

- [ ] **Step 4: Patch the one literal** in `crates/xvision-cli/src/commands/strategy.rs:736-770` (the `PublicManifest { … }` construction) — add `execution_mode: Default::default(), capital_mode: Default::default(),`. Run a build to find any other literal:

```bash
cargo build -p xvision-engine -p xvision-cli 2>&1 | tail -20
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib strategies::manifest 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/strategies/manifest.rs crates/xvision-cli/src/commands/strategy.rs
git commit -m "feat(engine): PublicManifest.execution_mode + capital_mode (defaults)"
```

### Task A5: Remove `asset` from `Scenario`

**Files:**
- Modify: `crates/xvision-engine/src/eval/scenario.rs:42` (struct) + `canonical_scenarios()`

- [ ] **Step 1: Write the failing test** in `scenario.rs`:

```rust
#[test]
fn scenario_drops_legacy_asset_key_on_parse() {
    // A pre-multi-asset body_json carried `asset: [...]`. With the field
    // removed and no deny_unknown_fields, the key is ignored on parse.
    let mut v = serde_json::to_value(canonical_scenarios()[0].clone()).unwrap();
    v.as_object_mut().unwrap().insert(
        "asset".into(),
        serde_json::json!([{"class":"crypto","symbol":"BTC","venue_symbol":"BTC/USD"}]),
    );
    let back: Scenario = serde_json::from_value(v).expect("legacy asset key must be ignored");
    assert_eq!(back.asset_class, AssetClass::Crypto);
}
```

- [ ] **Step 2: Confirm `Scenario` is NOT `#[serde(deny_unknown_fields)]`.** Read the derive on the struct (line ~31). If it has `deny_unknown_fields`, this test would fail to parse the legacy key — in that case the test instead asserts the field is simply gone (construct without `asset`). Run:

```bash
grep -n "deny_unknown_fields" crates/xvision-engine/src/eval/scenario.rs
```

- [ ] **Step 3: Remove the field.** Delete `pub asset: Vec<AssetRef>,` (line 42). Keep `asset_class` and `quote_currency`.

- [ ] **Step 4: Fix `canonical_scenarios()`** and every `Scenario { … }` literal the compiler flags — drop the `asset:` line. (`AssetRef` type stays; it's still used by the API request/CLI.)

```bash
cargo build -p xvision-engine 2>&1 | tail -30
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib eval::scenario 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/eval/scenario.rs
git commit -m "feat(engine): scenarios are asset-free (drop Scenario.asset)"
```

### Task A6: Asset-free scenario API (drop gate + per-asset cache key)

**Files:**
- Modify: `crates/xvision-engine/src/api/scenario.rs` — `CreateScenarioRequest` (42), `ScenarioMutations` (110), `create()` (125), `validate_request()` (the `len()==1` + symbol-whitelist gate)

- [ ] **Step 1: Write the failing test** (in `api/scenario.rs` tests, or a new `tests/` integration test):

```rust
#[tokio::test]
async fn create_scenario_without_asset_succeeds() {
    let ctx = ApiContext::test_in_memory().await; // existing test helper
    let req = sample_create_request(); // helper below: no `asset` field
    let sc = create(&ctx, req).await.expect("asset-free scenario must create");
    assert_eq!(sc.asset_class, AssetClass::Crypto);
}
```

(Add a `sample_create_request()` helper mirroring the existing test fixture minus `asset`.)

- [ ] **Step 2: Run, expect FAIL** (`asset` still required on the request)

```bash
cargo test -p xvision-engine --lib api::scenario 2>&1 | tail -20
```

- [ ] **Step 3: Edit the request + mutations + create + validate.**
  - `CreateScenarioRequest`: delete `pub asset: Vec<AssetRef>,` (line 42).
  - `ScenarioMutations`: delete `pub asset: Option<Vec<AssetRef>>,` (line 110).
  - `create()`: the `bar_cache_policy.cache_key` (lines 129-135) **can no longer key on an asset** — scenarios are asset-free. Replace the asset-specific key with a window+granularity+source key (asset is appended at bar-load time in Task B2):

```rust
    let cache_key = engine_bars::compute_scenario_cache_key(
        req.granularity, req.time_window.start, req.time_window.end, "alpaca-historical-v1",
    );
```

   Add `compute_scenario_cache_key` to `eval/bars.rs` (drop the `asset` arg from the existing `compute_cache_key`, or add a sibling that omits it). Remove the `asset:` line from the `Scenario { … }` literal in `create()`.
  - `validate_request()`: delete the `asset.len() == 1` check and the per-symbol whitelist check (symbol validation moves to strategy-universe validation, Task B1). Keep `asset_class == Crypto`, `quote_currency`, granularity, time-window, replay-mode checks.
  - Update the module docstring (lines 1-14): remove the `asset.len() == 1` and `asset symbol must be in the whitelist` bullets.

- [ ] **Step 4: Build + fix call sites** (the CLI scenario create + any test fixture building the request):

```bash
cargo build -p xvision-engine 2>&1 | tail -30
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib api::scenario 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/scenario.rs crates/xvision-engine/src/eval/bars.rs
git commit -m "feat(engine): asset-free scenario API (drop len==1 gate, per-asset cache key at load)"
```

### Task A7: Phase A workspace gate

- [ ] **Step 1:** `cargo test -p xvision-engine 2>&1 | tail -30` — expect PASS (some downstream executor tests may need the `asset`-free fixtures; if a test still constructs `Scenario { asset: … }` fix it to drop the field). Commit any fixture fixes:

```bash
git add -A && git commit -m "test(engine): phase A — asset-free scenario fixtures"
```

---

## Phase B — Harness `PerAsset` fan-out + `Pooled` NAV + asset-scoped signals

### Task B1: `active_assets` resolver + symbol/asset-class validation

**Files:**
- Create: `crates/xvision-engine/src/eval/executor/asset_set.rs`
- Modify: `crates/xvision-engine/src/eval/executor/mod.rs` (`pub mod asset_set;`)

- [ ] **Step 1: Write the failing test** in `asset_set.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::trading::AssetSymbol;

    #[test]
    fn resolves_full_universe_when_no_subset() {
        let got = active_assets(&["BTC/USD".into(), "ETH/USD".into()], None).unwrap();
        assert_eq!(got, vec![AssetSymbol::Btc, AssetSymbol::Eth]);
    }

    #[test]
    fn subset_must_be_subset_of_universe() {
        let err = active_assets(&["BTC/USD".into()], Some(&[AssetSymbol::Eth])).unwrap_err();
        assert!(err.to_string().contains("not in the strategy universe"));
    }

    #[test]
    fn rejects_unparseable_universe_symbol() {
        let err = active_assets(&["NOTACOIN".into()], None).unwrap_err();
        assert!(err.to_string().contains("NOTACOIN"));
    }
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --lib eval::executor::asset_set 2>&1 | tail -20
```

- [ ] **Step 3: Implement** `asset_set.rs`:

```rust
//! Resolve the active asset set for a run from the strategy universe and an
//! optional per-run subset. v1 is pure/static — a future cross-asset selector
//! agent would be consulted here, but the resolver signature stays the same.

use anyhow::{anyhow, Result};
use std::str::FromStr;
use xvision_core::trading::AssetSymbol;

/// `universe` is `Strategy.manifest.asset_universe` (e.g. `["BTC/USD","ETH/USD"]`).
/// `subset` is an optional `--assets` narrowing; every entry must be in the universe.
pub fn active_assets(universe: &[String], subset: Option<&[AssetSymbol]>) -> Result<Vec<AssetSymbol>> {
    if universe.is_empty() {
        return Err(anyhow!("strategy asset_universe is empty"));
    }
    let parsed: Vec<AssetSymbol> = universe
        .iter()
        .map(|s| AssetSymbol::from_str(s).map_err(|e| anyhow!("{e}")))
        .collect::<Result<_>>()?;
    match subset {
        None => Ok(parsed),
        Some(sub) => {
            for a in sub {
                if !parsed.contains(a) {
                    return Err(anyhow!("asset {a} is not in the strategy universe"));
                }
            }
            Ok(parsed.into_iter().filter(|a| sub.contains(a)).collect())
        }
    }
}
```

- [ ] **Step 4: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib eval::executor::asset_set 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/asset_set.rs crates/xvision-engine/src/eval/executor/mod.rs
git commit -m "feat(engine): active_assets resolver (universe + optional subset)"
```

### Task B2: `load_bars_for_scenario` parameterized by asset

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` (`load_bars_for_scenario` ~2041)

- [ ] **Step 1: Write the failing test** (integration test in `crates/xvision-engine/tests/` or inline) asserting the loader takes an explicit asset and computes the per-asset cache key:

```rust
#[tokio::test]
async fn load_bars_uses_explicit_asset_not_scenario() {
    let ctx = ApiContext::test_with_mock_alpaca().await; // existing helper
    let scenario = canonical_scenarios()[0].clone(); // asset-free now
    let bars = load_bars_for_scenario(&ctx, &scenario, AssetSymbol::Eth).await.unwrap();
    assert!(!bars.is_empty());
}
```

- [ ] **Step 2: Run, expect FAIL** (signature mismatch — function takes no asset)

```bash
cargo test -p xvision-engine --test '*' load_bars_uses_explicit_asset 2>&1 | tail -20
```

- [ ] **Step 3: Change the signature.** Replace the `scenario.asset.first()…venue_symbol` extraction with the passed `asset: AssetSymbol`, and compute the cache key from `asset.as_alpaca_pair()` + scenario granularity/window:

```rust
pub async fn load_bars_for_scenario(
    ctx: &ApiContext,
    scenario: &Scenario,
    asset: AssetSymbol,
) -> ApiResult<Vec<xvision_data::alpaca::MarketBar>> {
    let venue_symbol = asset.as_alpaca_pair();
    let cache_key = engine_bars::compute_cache_key(
        &venue_symbol, scenario.granularity,
        scenario.time_window.start, scenario.time_window.end, "alpaca-historical-v1",
    );
    // … existing load_bars body, using venue_symbol + cache_key …
}
```

- [ ] **Step 4: Build + fix the single existing caller** (it will pass a resolved asset — temporarily `AssetSymbol::Btc` until B4 wires the loop):

```bash
cargo build -p xvision-engine 2>&1 | tail -20
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine --test '*' load_bars_uses_explicit_asset 2>&1 | tail -20
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(engine): load_bars_for_scenario takes explicit asset"
```

### Task B3: `PortfolioBook` accounting type

**Files:**
- Create: `crates/xvision-engine/src/eval/executor/book.rs`
- Modify: `crates/xvision-engine/src/eval/executor/mod.rs` (`pub mod book;`)

- [ ] **Step 1: Write the failing test** in `book.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::trading::AssetSymbol::{Btc, Eth};

    #[test]
    fn equity_sums_pooled_realized_plus_per_asset_marks() {
        let mut b = PortfolioBook::new(100_000.0);
        // open +1 BTC unit @ 50k, +2 ETH units @ 2k
        b.set_position(Btc, 1.0, 50_000.0);
        b.set_position(Eth, 2.0, 2_000.0);
        b.add_realized(500.0);
        // marks: BTC 51k (+1k), ETH 2.1k (+200) → unrealized 1k + 400
        let marks = std::collections::BTreeMap::from([(Btc, 51_000.0), (Eth, 2_100.0)]);
        assert_eq!(b.equity(&marks), 100_000.0 + 500.0 + 1_000.0 + 400.0);
    }

    #[test]
    fn flat_book_equity_is_initial_plus_realized() {
        let mut b = PortfolioBook::new(100_000.0);
        b.add_realized(-250.0);
        assert_eq!(b.equity(&std::collections::BTreeMap::new()), 99_750.0);
    }
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --lib eval::executor::book 2>&1 | tail -20
```

- [ ] **Step 3: Implement** `book.rs`:

```rust
//! Pooled multi-asset portfolio accounting for the eval executors.
//! One capital pool, per-asset positions, shared realized PnL.
//! equity = initial + realized + Σ position[a] * (mark[a] - entry[a]).

use std::collections::BTreeMap;
use xvision_core::trading::AssetSymbol;

#[derive(Debug, Clone, Copy)]
struct Leg { position: f64, entry_price: f64 } // +long / -short units

#[derive(Debug, Clone)]
pub struct PortfolioBook {
    initial: f64,
    realized: f64,
    legs: BTreeMap<AssetSymbol, Leg>,
}

impl PortfolioBook {
    pub fn new(initial: f64) -> Self {
        Self { initial, realized: 0.0, legs: BTreeMap::new() }
    }
    pub fn position(&self, a: AssetSymbol) -> f64 { self.legs.get(&a).map_or(0.0, |l| l.position) }
    pub fn entry_price(&self, a: AssetSymbol) -> f64 { self.legs.get(&a).map_or(0.0, |l| l.entry_price) }
    pub fn set_position(&mut self, a: AssetSymbol, position: f64, entry_price: f64) {
        if position == 0.0 { self.legs.remove(&a); }
        else { self.legs.insert(a, Leg { position, entry_price }); }
    }
    pub fn add_realized(&mut self, pnl: f64) { self.realized += pnl; }
    pub fn realized(&self) -> f64 { self.realized }
    /// Mark-to-market equity. `marks[a]` is the price to value asset `a` at;
    /// assets absent from `marks` contribute zero unrealized (treated flat-mark).
    pub fn equity(&self, marks: &BTreeMap<AssetSymbol, f64>) -> f64 {
        let unrealized: f64 = self.legs.iter()
            .map(|(a, l)| marks.get(a).map_or(0.0, |m| l.position * (m - l.entry_price)))
            .sum();
        self.initial + self.realized + unrealized
    }
}
```

- [ ] **Step 4: Run, expect PASS**

```bash
cargo test -p xvision-engine --lib eval::executor::book 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/book.rs crates/xvision-engine/src/eval/executor/mod.rs
git commit -m "feat(engine): PortfolioBook pooled multi-asset accounting"
```

### Task B4: Per-asset fan-out in the backtest executor

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` — `run_inner` (375), asset extraction (387-391), accounting init (508-510), equity lines (636/824/1483), seed `"asset"` (696, 713, 871, 1084, 1186, 1253, 1444, 1625), signal-cache call sites.

This task restructures the single-asset replay into a per-asset fan-out under a shared `PortfolioBook`, branching on `execution_mode`. Read the current `run_inner` end-to-end before editing.

- [ ] **Step 1: Write the failing integration test** `crates/xvision-engine/tests/multi_asset_backtest.rs`:

```rust
use xvision_engine::eval::executor::backtest::BacktestExecutor;
// (use the existing test harness pattern from tests/ for building a Strategy + Scenario + injected bars)

#[tokio::test]
async fn backtest_fans_out_over_universe_with_shared_nav() {
    // Strategy with asset_universe ["BTC/USD","ETH/USD"], execution_mode PerAsset.
    // Inject 3 aligned bars for each asset via the executor's bar-injection hook.
    // A deterministic stub dispatch returns long_open on bar 1 for each asset.
    let summary = run_two_asset_backtest().await; // helper built in this test file
    // Both assets produced decisions:
    let assets: std::collections::BTreeSet<_> =
        summary.decisions.iter().map(|d| d.asset.clone()).collect();
    assert!(assets.contains("BTC"));
    assert!(assets.contains("ETH"));
    // One pooled equity curve (not two independent ones):
    assert_eq!(summary.equity_curve_is_pooled(), true);
}

#[tokio::test]
async fn portfolio_mode_returns_not_implemented() {
    let err = run_backtest_with_mode("portfolio").await.unwrap_err();
    assert!(err.to_string().contains("not yet implemented"));
    assert!(err.to_string().contains("portfolio"));
}
```

(Build the helpers from the existing single-asset executor test setup — copy the closest existing `tests/` fixture and extend it to two assets + the injected-bars hook.)

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --test multi_asset_backtest 2>&1 | tail -30
```

- [ ] **Step 3: Branch on execution_mode + reject unimplemented arms.** At the top of `run_inner` (after the cadence check ~399), before the asset extraction:

```rust
    use crate::strategies::{CapitalMode, ExecutionMode};
    match &strategy.manifest.execution_mode {
        ExecutionMode::PerAsset => {}
        ExecutionMode::Portfolio => anyhow::bail!("execution_mode `portfolio` not yet implemented"),
        ExecutionMode::Custom(name) => anyhow::bail!("execution_mode `custom:{name}` not yet implemented"),
    }
    if strategy.manifest.capital_mode != CapitalMode::Pooled {
        anyhow::bail!("capital_mode `per_asset` not yet implemented");
    }
```

- [ ] **Step 4: Resolve the active asset set + load per-asset bars.** Replace the `scenario.asset.first()` block (387-391) with:

```rust
    let active = crate::eval::executor::asset_set::active_assets(
        &strategy.manifest.asset_universe, self.asset_subset.as_deref(),
    )?;
    // per-asset bars, aligned by timestamp into a BTreeMap<DateTime<Utc>, BTreeMap<AssetSymbol, Ohlcv>>
    let timeline = self.load_aligned_timeline(scenario, &active).await?;
```

  Add an `asset_subset: Option<Vec<AssetSymbol>>` field to `BacktestExecutor` (defaulting `None`; set by the CLI/run wiring in Task C3) and a `load_aligned_timeline` method that calls the (asset-parameterized) loader per asset and outer-joins on timestamp. For the injected-bars test path, accept a `BTreeMap<AssetSymbol, Vec<Ohlcv>>` injection.

- [ ] **Step 5: Replace scalar accounting with `PortfolioBook`.** Replace `let mut position`/`entry_price`/`realized_total` (508-510) with `let mut book = PortfolioBook::new(initial);`. Replace each equity line (636/824/1483) `equity = initial + realized_total + position * (next_bar_open - entry_price)` with a per-asset marks map built from each active asset's `next_bar_open` and `book.equity(&marks)`.

- [ ] **Step 6: Drive the per-(timestamp, asset) loop.** Wrap the existing per-bar body in `for (ts, per_asset_bars) in timeline { for (asset, bar) in per_asset_bars { … } }`. Inside, for each asset:
  - set the seed `"asset"` field (696/713/871/1084/1186/1253/1444/1625) to that asset's `venue_symbol`, and add `"active_assets": active.iter().map(|a| a.as_short()).collect::<Vec<_>>()`.
  - pass `SignalScope::Asset(asset)` into every `SignalCacheKey::new(strategy_id, role, …)` call so signals are per-asset (the A2 default-`Global` call sites in the executor become `Asset(asset)`).
  - read/update the position via `book.position(asset)` / `book.set_position(asset, …)` instead of the scalar; book realized via `book.add_realized(fill.realized_pnl)`.
  - emit the decision with `decision.asset = asset` and a `cycle_id` derived from `(decision_idx, asset)`.

- [ ] **Step 7: Build, then run, expect PASS**

```bash
cargo build -p xvision-engine 2>&1 | tail -30
cargo test -p xvision-engine --test multi_asset_backtest 2>&1 | tail -30
```

- [ ] **Step 8: Run the existing single-asset executor tests — expect PASS unchanged** (a 1-element universe collapses to today's behavior). Triage any diff:

```bash
cargo test -p xvision-engine --lib eval::executor::backtest 2>&1 | tail -30
```

- [ ] **Step 9: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/backtest.rs crates/xvision-engine/tests/multi_asset_backtest.rs
git commit -m "feat(engine): per-asset fan-out + pooled NAV in backtest executor"
```

### Task B5: Mirror the fan-out in the paper executor

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/paper.rs` (asset extraction ~606, accounting, seed, signal keys)

- [ ] **Step 1: Write the failing test** — same shape as B4's `backtest_fans_out_…` but against `PaperExecutor` with a mock broker; assert both assets get decisions and `broker.position(&asset)` is queried per asset.

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --test '*' paper_fans_out 2>&1 | tail -20
```

- [ ] **Step 3: Apply the same edits as B4** to `paper.rs`: execution_mode branch, `active_assets`, aligned timeline, `PortfolioBook`, per-asset seed + `active_assets`, `SignalScope::Asset(asset)` keys, per-asset broker position queries.

- [ ] **Step 4: Build + run, expect PASS; then run existing paper tests unchanged**

```bash
cargo build -p xvision-engine 2>&1 | tail -20
cargo test -p xvision-engine --test '*' paper 2>&1 | tail -30
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/paper.rs crates/xvision-engine/tests/*paper*
git commit -m "feat(engine): per-asset fan-out + pooled NAV in paper executor"
```

### Task B6: Multi-filter scope regression test

**Files:**
- Create/extend: `crates/xvision-engine/tests/multi_asset_filter_scope.rs`

- [ ] **Step 1: Write the test** — a strategy with two Filters (`regime`, `vol`) + one Trader over `[BTC,ETH]`; assert the signal cache holds 4 entries (2 roles × 2 assets), that BTC's trader briefing surfaces `filter_signals["regime"]` scoped to BTC, and an edge predicate on `regime` gates per-asset:

```rust
#[tokio::test]
async fn two_filters_two_assets_produce_four_scoped_signals() {
    let (summary, cache_len) = run_two_filter_two_asset_backtest().await;
    assert_eq!(cache_len, 4, "2 roles x 2 assets must not collide");
    // BTC trader saw a BTC-scoped regime signal, ETH trader saw ETH-scoped:
    assert_eq!(summary.briefing_signal_asset("regime", "BTC"), Some("BTC".to_string()));
    assert_eq!(summary.briefing_signal_asset("regime", "ETH"), Some("ETH".to_string()));
}
```

- [ ] **Step 2: Run, expect FAIL → implement briefing/edge resolution.** The briefing builder and edge-predicate evaluator must select signals whose `scope` is `Asset(current_asset)` or `Global` and present them keyed by role. Implement that resolution in `filter_dispatch.rs` / `pipeline.rs` where the `filter_signals` map is assembled for the downstream briefing.

```bash
cargo test -p xvision-engine --test multi_asset_filter_scope 2>&1 | tail -30
```

- [ ] **Step 3: Build + run, expect PASS; full engine suite**

```bash
cargo test -p xvision-engine 2>&1 | tail -30
```

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-engine/src/agent/filter_dispatch.rs crates/xvision-engine/src/agent/pipeline.rs crates/xvision-engine/tests/multi_asset_filter_scope.rs
git commit -m "feat(engine): asset-scoped filter-signal resolution in briefings"
```

---

## Phase C — CLI

### Task C1: `xvn strategy new --assets`

**Files:**
- Modify: `crates/xvision-cli/src/commands/strategy.rs` (Args 56-90, construction 736-770)

- [ ] **Step 1: Write the failing test** in `crates/xvision-cli/tests/strategy_cli.rs`:

```rust
#[test]
fn strategy_new_assets_populates_universe() {
    let out = run_xvn(&["strategy","new","--name","Multi","--provider","anthropic",
        "--model","claude-sonnet-4-6","--role","trader",
        "--assets","BTC,ETH,SOL","--timeframe","1h","--prompt", PROMPT_PATH, "--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["manifest"]["asset_universe"], serde_json::json!(["BTC/USD","ETH/USD","SOL/USD"]));
}
```

- [ ] **Step 2: Run, expect FAIL** (`--assets` unknown)

```bash
cargo test -p xvision-cli --test strategy_cli strategy_new_assets 2>&1 | tail -20
```

- [ ] **Step 3: Add the flag + construction.** In the `New` Args, add:

```rust
    /// Comma-separated assets the strategy trades, e.g. `BTC,ETH,SOL`.
    /// Populates `asset_universe`. Supersedes `--asset` (kept as a 1-elem alias).
    #[arg(long, value_delimiter = ',')]
    assets: Vec<String>,
    /// How the harness drives the universe. `per-asset` (default) | `portfolio`.
    #[arg(long, default_value = "per-asset")]
    execution_mode: String,
```

  Build the universe (each bare symbol → `SYM/USD` via `AssetSymbol::from_str(..).as_alpaca_pair()`), preferring `--assets`, falling back to `--asset` (1-elem), erroring if both empty in atomic mode. Set `asset_universe` and `execution_mode` (parse `"per-asset"`→`PerAsset`, `"portfolio"`→`Portfolio`).

- [ ] **Step 4: Run, expect PASS**

```bash
cargo test -p xvision-cli --test strategy_cli 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/strategy.rs crates/xvision-cli/tests/strategy_cli.rs
git commit -m "feat(cli): strategy new --assets (multi-asset universe) + --execution-mode"
```

### Task C2: `xvn scenario create` is asset-free

**Files:**
- Modify: `crates/xvision-cli/src/commands/scenario.rs` (CreateArgs 114-164, request 484-488)

- [ ] **Step 1: Write the failing test** in `crates/xvision-cli/tests/` (scenario create test): asset-free create succeeds and the persisted scenario has no asset:

```rust
#[test]
fn scenario_create_has_no_asset_flag() {
    let out = run_xvn(&["scenario","create","--name","Win","--from","2024-02-01",
        "--to","2024-02-10","--granularity","1h","--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(v.get("asset").is_none(), "scenarios are asset-free");
}
```

- [ ] **Step 2: Run, expect FAIL** (today `--asset` is required)

```bash
cargo test -p xvision-cli scenario_create_has_no_asset 2>&1 | tail -20
```

- [ ] **Step 3: Edit.** Remove `pub asset: String,` from `CreateArgs`; remove the `asset_ref_from_sym` usage + `asset: vec![...]` from the `CreateScenarioRequest` build (484-488). Keep asset-class defaulting to Crypto. Remove the now-unused `asset_ref_from_sym` helper if nothing else uses it.

- [ ] **Step 4: Build + run, expect PASS**

```bash
cargo build -p xvision-cli 2>&1 | tail -20
cargo test -p xvision-cli scenario 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/scenario.rs
git commit -m "feat(cli): scenario create is asset-free"
```

### Task C3: `xvn eval run --assets` subset + executor wiring

**Files:**
- Modify: `crates/xvision-cli/src/commands/eval/mod.rs` (RunArgs 105-166), eval run dispatch (sets `BacktestExecutor.asset_subset`)

- [ ] **Step 1: Write the failing test** — `eval run --assets ETH` on a `[BTC,ETH]` strategy runs only ETH:

```rust
#[test]
fn eval_run_assets_subsets_universe() {
    let out = run_xvn(&["eval","run","--strategy", SID, "--scenario", SCID,
        "--mode","backtest","--assets","ETH","--json"]);
    let v: serde_json::Value = serde_json::from_str(&out).unwrap();
    let assets: std::collections::BTreeSet<&str> =
        v["decisions"].as_array().unwrap().iter().map(|d| d["asset"].as_str().unwrap()).collect();
    assert_eq!(assets, std::collections::BTreeSet::from(["ETH"]));
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-cli eval_run_assets_subsets 2>&1 | tail -20
```

- [ ] **Step 3: Add the flag + wire it.** In `RunArgs`:

```rust
    /// Optional subset of the strategy's universe to trade this run
    /// (comma-separated, e.g. `ETH,SOL`). Must be ⊆ the strategy universe.
    #[arg(long, value_delimiter = ',')]
    pub assets: Vec<String>,
```

  In the run dispatch, parse `assets` into `Vec<AssetSymbol>` and set it on the executor (`BacktestExecutor.asset_subset` / `PaperExecutor.asset_subset`) before `run`.

- [ ] **Step 4: Build + run, expect PASS**

```bash
cargo build -p xvision-cli 2>&1 | tail -20
cargo test -p xvision-cli eval 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/eval/mod.rs
git commit -m "feat(cli): eval run --assets subset override"
```

### Task C4: Per-asset rollup in compare report

**Files:**
- Modify: `crates/xvision-cli/src/commands/eval/compare_format.rs` (render 16-29, `markdown_row` 119-201)

- [ ] **Step 1: Write the failing test** in `crates/xvision-cli/tests/eval_compare_report.rs`: a 2-asset run's report includes a per-asset breakdown section keyed by asset symbol:

```rust
#[test]
fn compare_report_has_per_asset_rollup() {
    let md = render_markdown(&two_asset_report(), "Multi");
    assert!(md.contains("### Per-asset"), "expected a per-asset rollup section");
    assert!(md.contains("| BTC |"));
    assert!(md.contains("| ETH |"));
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-cli --test eval_compare_report compare_report_has_per_asset 2>&1 | tail -20
```

- [ ] **Step 3: Add a per-asset rollup section.** After the existing per-scenario table loop in `render_markdown`, append a `### Per-asset` table that groups the run's decisions by `asset` (the per-decision asset is already on the decision rows) and sums per-asset realized PnL / trade count. Add a `per_asset_rows(report)` helper that aggregates by asset symbol.

- [ ] **Step 4: Build + run, expect PASS**

```bash
cargo test -p xvision-cli --test eval_compare_report 2>&1 | tail -20
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-cli/src/commands/eval/compare_format.rs crates/xvision-cli/tests/eval_compare_report.rs
git commit -m "feat(cli): per-asset rollup section in eval compare report"
```

---

## Phase D — Frontend

> Build the SPA after edits: `pnpm -C frontend/web build` (regenerates `crates/xvision-dashboard/static/`). Regenerate TS types from Rust where a struct gained `#[derive(ts_rs::TS)]`: the engine test-suite emits them (`cargo test -p xvision-engine export_bindings` or the repo's existing ts-export task). No popups (workspace rule) — everything inline.

### Task D1: Expose `asset_universe` + `execution_mode` on `StrategySummary`

**Files:**
- Modify: the Rust `StrategySummary` (search: `grep -rn "struct StrategySummary" crates/xvision-engine/src`), and its mapping from `Strategy`.

- [ ] **Step 1: Write the failing test** (engine) asserting the summary carries the universe:

```rust
#[test]
fn strategy_summary_exposes_asset_universe() {
    let s = sample_strategy_with_universe(&["BTC/USD","ETH/USD"]);
    let sum = StrategySummary::from(&s);
    assert_eq!(sum.asset_universe, vec!["BTC/USD".to_string(), "ETH/USD".to_string()]);
    assert_eq!(sum.execution_mode, "per_asset");
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine strategy_summary_exposes_asset_universe 2>&1 | tail -20
```

- [ ] **Step 3: Add fields** `asset_universe: Vec<String>` and `execution_mode: String` to `StrategySummary` and populate them in the `From<&Strategy>` mapping.

- [ ] **Step 4: Regenerate TS types + run, expect PASS**

```bash
cargo test -p xvision-engine 2>&1 | tail -20   # emits StrategySummary.ts
git diff --stat frontend/web/src/api/types.gen/
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src frontend/web/src/api/types.gen/StrategySummary.ts
git commit -m "feat(api): StrategySummary exposes asset_universe + execution_mode"
```

### Task D2: Remove the asset picker from `ScenarioForm`

**Files:**
- Modify: `frontend/web/src/components/scenario/ScenarioForm.tsx` (state 100-102, submit 191-200, the picker JSX ~255)

- [ ] **Step 1: Edit.** Remove the `const [asset, setAsset] = useState(...)` hook (100-102), the asset `<select>` JSX (~255), and the `asset: [...]` line in the `CreateScenarioRequest` body (191-200). Keep `ASSET_CLASS`/`QUOTE_CURRENCY`. `ALPACA_ASSETS` moves to Task D3 (strategy authoring) — if no longer referenced here, export it from a shared module `frontend/web/src/lib/assets.ts` instead of deleting.

- [ ] **Step 2: Typecheck + build**

```bash
pnpm -C frontend/web exec tsc --noEmit 2>&1 | tail -20
pnpm -C frontend/web build 2>&1 | tail -5
```

- [ ] **Step 3: Verify in browser.** Start the app (`cargo run -p xvision-cli -- serve` or the project's dev-server skill), open the scenario create form, confirm there is no asset picker and a scenario still creates. (UI correctness must be eyeballed — type/build passing is necessary, not sufficient.)

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/components/scenario/ScenarioForm.tsx frontend/web/src/lib/assets.ts
git commit -m "feat(web): scenario form is asset-free"
```

### Task D3: Editable `asset_universe` multi-select in strategy authoring

**Files:**
- Modify: `frontend/web/src/routes/authoring.tsx` (read-only block 825-837)
- Create: `frontend/web/src/lib/assets.ts` (shared `ALPACA_ASSETS` const)

- [ ] **Step 1: Move the const.** Create `lib/assets.ts` exporting `ALPACA_ASSETS` (the 15-symbol list) and a `toVenuePair(sym: string)` helper returning `${sym}/USD`.

- [ ] **Step 2: Replace the read-only badges** (825-837) with a multi-select chip editor: render each `m.asset_universe` entry as a removable chip, plus an "add" control listing `ALPACA_ASSETS` not already selected. On change, PATCH the strategy via the existing `StrategyMetadataPatch` mutation (it already accepts `asset_universe: Array<string> | null`). Persist as venue pairs (`BTC/USD`).

- [ ] **Step 3: Typecheck + build**

```bash
pnpm -C frontend/web exec tsc --noEmit 2>&1 | tail -20
pnpm -C frontend/web build 2>&1 | tail -5
```

- [ ] **Step 4: Verify in browser.** Open a strategy in authoring, add/remove assets, reload, confirm persistence.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/routes/authoring.tsx frontend/web/src/lib/assets.ts
git commit -m "feat(web): editable asset_universe multi-select in strategy authoring"
```

### Task D4: Per-asset rollup in eval results

**Files:**
- Modify: `frontend/web/src/routes/eval-runs-detail.tsx` (decision rows already render `r.asset` at 848)

- [ ] **Step 1: Add a per-asset summary panel.** Above the decisions table, group `decisions` by `asset` and render a small table: asset, # decisions, # trades, realized PnL. Use the existing `DecisionRowDto.asset` field; no new API needed. Inline-expand (no popup).

- [ ] **Step 2: Typecheck + build**

```bash
pnpm -C frontend/web exec tsc --noEmit 2>&1 | tail -20
pnpm -C frontend/web build 2>&1 | tail -5
```

- [ ] **Step 3: Verify in browser.** Open a multi-asset run, confirm the per-asset panel matches the per-decision rows.

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/routes/eval-runs-detail.tsx
git commit -m "feat(web): per-asset rollup panel on eval run detail"
```

---

## Phase E — Evidence & acceptance

### Task E1: No-regression baseline

- [ ] **Step 1:** Full workspace test:

```bash
cargo test --workspace 2>&1 | tail -40
```

Expected: PASS. A 1-element-universe strategy must produce identical results to pre-change (the per-asset loop collapses to one asset). Triage any diff against the single-asset fixtures.

- [ ] **Step 2: Commit** any fixture reconciliations:

```bash
git add -A && git commit -m "test: phase E no-regression reconciliation"
```

### Task E2: Capture evidence artifacts

- [ ] **Step 1: CLI multi-asset run.** With Alpaca paper creds sourced, run end-to-end and save the transcript to `docs/superpowers/evidence/2026-05-24-multi-asset/cli-run.txt`:

```bash
cargo run -p xvision-cli -- strategy new --name "Tri" --provider anthropic --model claude-sonnet-4-6 --role trader --assets BTC,ETH,SOL --timeframe 1h --prompt <prompt.md> --json
cargo run -p xvision-cli -- scenario create --name "Feb24" --from 2024-02-01 --to 2024-02-10 --granularity 1h --json
cargo run -p xvision-cli -- eval run --strategy <id> --scenario <id> --mode backtest --json
```

- [ ] **Step 2: Decision-trace evidence.** Save a snippet of the run's decisions showing `asset` varying across BTC/ETH/SOL under one pooled equity curve to `…/decision-trace.json`.

- [ ] **Step 3: UI screenshot.** Capture the asset-free scenario form, the editable universe multi-select, and the per-asset results panel into `…/ui.png`.

- [ ] **Step 4: Commit evidence**

```bash
git add -f docs/superpowers/evidence/2026-05-24-multi-asset/
git commit -m "docs(evidence): multi-asset end-to-end CLI + trace + UI"
```

### Task E3: Spec/plan close-out

- [ ] **Step 1:** Tick the spec's acceptance list; note the two decisions-with-defaults as resolved (universe_bars gated; `capital_mode` shipped).
- [ ] **Step 2:** Open the PR (or hand back for review) summarizing the five phases and linking the evidence.

---

## Self-review notes

- **Spec coverage:** Decisions 1–8 → Tasks A5/A6 (asset-free scenario), A4 (asset_universe source of truth; already existed), A3+A4+B4 (execution_mode), A1+A2+B4+B6 (SignalScope first-class + multi-filter), B4 step 6 (briefing `asset`/`active_assets`; `universe_bars` gated = omitted in PerAsset), B1 (active-set resolver, no named stage), B3+B4 (capital_mode/Pooled + risk via existing RiskConfig), B4 (minimal asset injection — existing `"asset"` field). Surfaces → C1–C4 (CLI), D1–D4 (frontend). Testing/evidence → B-tests + E1–E3.
- **Out-of-scope respected:** Portfolio/Custom + capital PerAsset return not-implemented (B4 step 3); no selector agent; Pair/Global producers not built; Live wall untouched.
- **Type consistency:** `SignalScope` (A1) used by `SignalCacheKey` (A2) and executor call sites (B4/B5); `ExecutionMode`/`CapitalMode` (A3) used by `PublicManifest` (A4) and the executor branch (B4); `PortfolioBook` API (`set_position`/`position`/`add_realized`/`equity`) consistent between B3 definition and B4/B5 use; `active_assets` signature consistent between B1 and B4/B5; `asset_subset` executor field introduced B4, set by CLI C3.
