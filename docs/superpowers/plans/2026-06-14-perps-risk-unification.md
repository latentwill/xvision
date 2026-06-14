# Perps Risk Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Move the two perps risk guards onto the engine's R3 veto path (venue-gated), port `MaxTotalExposure` as a general veto, and retire the orphaned `xvision-risk` system — with zero behavior change to existing spot/backtest runs.

**Architecture:** A new pure helper `strategies::risk::perps::perps_entry_veto` is called from both R3 veto blocks in `backtest.rs`. Perps guards activate only when a new `BrokerSurface::is_perp_venue()` gate is true (false everywhere today → inert). A separate general exposure-cap veto uses `book.open_legs()`. The `xvision-risk` / `xvision-harness` crates, the `xvision-eval` `BacktestRunner` harness, the `xvn risk` CLI command, and dead `emit_risk_gate_*` methods are deleted.

**Tech Stack:** Rust (workspace), `cargo` via `scripts/cargo`, `xvision_core::{VetoReason, Direction}`, `BrokerSurface` trait.

**Spec:** `docs/superpowers/specs/2026-06-14-perps-risk-unification-design.md`

**Worktree:** `.worktrees/perps-risk-unification` on branch `feat/perps-risk-unification`. All commands run from there. Set `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` once per shell.

---

## File Structure

| File | Responsibility | Change |
|---|---|---|
| `crates/xvision-execution/src/broker_surface.rs` | `BrokerSurface` trait + Alpaca/Orderly/Mock impls | Add `is_perp_venue()` default (`false`) + override `→ true` on `OrderlyLiveSurface`; add a perps test surface |
| `crates/xvision-execution/src/byreal.rs` | Hyperliquid perps adapter | Override `is_perp_venue() → true` on `ByrealLiveSurface<A>` |
| `crates/xvision-execution/src/bybit.rs` | Bybit linear-perps adapter | Override `is_perp_venue() → true` on `BybitPaperSurface<A>` |
| `crates/xvision-engine/src/eval/executor/real_broker_fills.rs` | live fill sink | Add `is_perp_venue()` passthrough to the wrapped broker |
| `crates/xvision-engine/src/strategies/risk.rs` | `RiskConfig` + presets | Add 3 config fields + preset defaults; declare `pub mod perps` |
| `crates/xvision-engine/src/strategies/risk/perps.rs` | **NEW** perps veto helper | `perps_entry_veto(...)` + unit tests |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | engine veto blocks | Wire perps veto + exposure veto into both R3 blocks (~L2012, ~L3965) |
| `crates/xvision-engine/src/agent/observability.rs` | obs emitter | Delete dead `emit_risk_gate_*` methods |
| `crates/xvision-engine/tests/risk_min_notional.rs` | engine test | Drop `xvision_risk` imports + the one unit test |
| `crates/xvision-risk/` | dead crate | **Delete** |
| `crates/xvision-harness/` | dead crate | **Delete** |
| `crates/xvision-eval/src/harness.rs` + `lib.rs` | A/B harness | **Delete** module + `pub mod harness;` |
| `crates/xvision-cli/src/commands/risk.rs` + `lib.rs` + `mod.rs` | `xvn risk` | **Delete** command + verb + wiring |
| `crates/xvision-{engine,eval,cli}/Cargo.toml`, root `Cargo.toml` | deps | Drop `xvision-risk`/`xvision-harness`; trim `default-members` |

> **Note on `strategies/risk.rs` → module dir:** adding `pub mod perps;` to a single-file module `risk.rs` requires either `risk/mod.rs` or the Rust 2018 `risk.rs` + `risk/perps.rs` sibling layout. This repo uses edition 2021, which supports `foo.rs` + `foo/` sibling dirs — so keep `risk.rs` and add `risk/perps.rs` alongside it. No rename of `risk.rs` needed.

---

## Task 1: `BrokerSurface::is_perp_venue()` gate

**Files:**
- Modify: `crates/xvision-execution/src/broker_surface.rs` (trait ~L421-465; `OrderlyLiveSurface<A>` impl at L828; add test surface in the `#[cfg(test)] mod tests`)
- Modify: `crates/xvision-execution/src/byreal.rs` (`ByrealLiveSurface<A>` impl at L625)
- Modify: `crates/xvision-execution/src/bybit.rs` (`BybitPaperSurface<A>` impl at L319)
- Modify: `crates/xvision-engine/src/eval/executor/real_broker_fills.rs`

- [ ] **Step 1: Write the failing test** (in `broker_surface.rs` test module, near the existing `DefaultsBroker` at ~L1406)

```rust
#[tokio::test]
async fn default_surface_is_not_perp_venue() {
    let b = DefaultsBroker;
    assert!(!b.is_perp_venue(), "default BrokerSurface must be spot (is_perp_venue=false)");
}

struct PerpTestSurface;
#[async_trait::async_trait]
impl BrokerSurface for PerpTestSurface {
    async fn submit_order(&self, _r: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        unreachable!("not exercised")
    }
    async fn position(&self, _a: &str) -> anyhow::Result<f64> { Ok(0.0) }
    async fn balance(&self) -> anyhow::Result<f64> { Ok(0.0) }
    fn venue(&self) -> &str { "hyperliquid" }
    fn is_perp_venue(&self) -> bool { true }
}

#[tokio::test]
async fn perp_surface_reports_perp_venue() {
    assert!(PerpTestSurface.is_perp_venue());
}
```

Also add a test that a **real** perps adapter reports `true`, reusing the
existing Orderly mock-API test harness. The surrounding Orderly tests build a
`MockApi { .. }` struct literal (defined at ~L1095) and pass it to
`OrderlyLiveSurface::with_api(api)` (~L1210-1215). Mirror that exact pattern:

```rust
#[test]
fn orderly_live_surface_is_perp_venue() {
    // Build the surface exactly as the existing Orderly tests at ~L1210 do:
    // construct a `MockApi { .. }` and pass it to `with_api`.
    let api = MockApi { /* copy the field initializers from the L1210 test */ };
    let surface = OrderlyLiveSurface::with_api(api);
    assert!(surface.is_perp_venue(), "Orderly is a directional-perps venue");
}
```

> `MockApi` (broker_surface.rs:1095) is the in-scope Orderly mock — copy its
> field initializers verbatim from the test at ~L1210. The `DefaultsBroker`
> default-false test already covers the spot path. Mirror `DefaultsBroker`'s
> method signatures in `PerpTestSurface` (check L1406).

- [ ] **Step 2: Run test to verify it fails**

Run: `scripts/cargo test -p xvision-execution default_surface_is_not_perp_venue`
Expected: FAIL — `no method named is_perp_venue`.

- [ ] **Step 3: Add the trait method** (in `broker_surface.rs`, after the `venue()` default method ~L454)

```rust
    /// Whether this surface trades directional perpetual futures, where
    /// funding and liquidation risk apply. Default `false` (spot); the
    /// directional-perps adapters (Hyperliquid/byreal, Orderly perps)
    /// override to `true`. Gates the engine's perps risk vetoes so they
    /// stay inert on spot venues. Read-only.
    fn is_perp_venue(&self) -> bool {
        false
    }
```

- [ ] **Step 3b: Override `→ true` on the directional-perps adapters.** Add this identical method inside three existing `impl BrokerSurface` blocks (place it next to each impl's `venue()` method):

```rust
    fn is_perp_venue(&self) -> bool {
        true
    }
```

  - `impl<A: OrderlyApi> BrokerSurface for OrderlyLiveSurface<A>` — `broker_surface.rs:828` (venue `"orderly"`, `PERP_*` symbols)
  - `impl<A: ByrealPerpsApi + 'static> BrokerSurface for ByrealLiveSurface<A>` — `byreal.rs:625` (Hyperliquid perps)
  - `impl<A: BybitApi + 'static> BrokerSurface for BybitPaperSurface<A>` — `bybit.rs:319` (Bybit `category=linear` perps)

  Leave `AlpacaPaperSurface` / `AlpacaLiveSurface` / `MockBrokerSurface` / `DefaultsBroker` on the default `false`. (Without these overrides the gate is permanently inert on real perps venues — the guards would be dead code.)

- [ ] **Step 4: Run test to verify it passes**

Run: `scripts/cargo test -p xvision-execution is_not_perp_venue perp_surface_reports orderly_live_surface_is_perp_venue`
Expected: PASS (all three).

- [ ] **Step 5: Add passthrough on `RealBrokerFills`** (`real_broker_fills.rs`; it wraps `broker: Arc<dyn BrokerSurface>` and already calls `self.broker.venue()` ~L154)

```rust
    /// Whether the wrapped live broker is a directional-perps venue.
    /// Threaded into the engine's R3 veto so perps guards activate only
    /// on perps venues. Spot brokers (Alpaca) return false.
    pub fn is_perp_venue(&self) -> bool {
        self.broker.is_perp_venue()
    }
```

- [ ] **Step 6: Build + commit**

Run: `scripts/cargo build -p xvision-execution -p xvision-engine`
Expected: clean build.

```bash
git add crates/xvision-execution/src/broker_surface.rs crates/xvision-execution/src/byreal.rs crates/xvision-execution/src/bybit.rs crates/xvision-engine/src/eval/executor/real_broker_fills.rs
git commit -m "feat(risk): add BrokerSurface::is_perp_venue() gate (true on Orderly/byreal/Bybit) + RealBrokerFills passthrough"
```

---

## Task 2: `perps_entry_veto` helper + config fields + presets

**Files:**
- Create: `crates/xvision-engine/src/strategies/risk/perps.rs`
- Modify: `crates/xvision-engine/src/strategies/risk.rs` (add fields L8-16, presets L29-53, `pub mod perps;`)
- Modify (struct-literal fix, Step 2b): `crates/xvision-engine/tests/{parity_pipeline_seed_byte_identical.rs,eval_causal_input_sanitization.rs,pine_import_map.rs,eval_exit_enforcement.rs}`

- [ ] **Step 1: Add config fields** (`risk.rs`, inside `pub struct RiskConfig`, after `max_position_pct_nav`)

```rust
    /// Maximum perp funding rate (8h, same units as
    /// `PerpsContext.funding_rate`) an entry may *pay* before it is vetoed.
    /// A long pays `+funding`, a short pays `-funding`. Perps-venue only.
    /// `0.0` disables. Default 0.0 so spot configs are unaffected.
    #[serde(default)]
    pub max_funding_pay_8h: f64,
    /// Minimum distance (percent of mark) an open perps position's
    /// liquidation price must keep before new entries are vetoed.
    /// Perps-venue only. `0.0` disables. Default 0.0.
    #[serde(default)]
    pub min_liq_distance_pct: f64,
    /// Maximum total open exposure (sum of position notionals as percent of
    /// NAV) a new open may push the book to. General control (spot + perps).
    /// `0.0` disables. Default 0.0 so existing behavior is unchanged.
    #[serde(default)]
    pub max_total_exposure_pct: f64,
```

- [ ] **Step 2: Set preset defaults** (`risk.rs`, each `RiskPreset::*` arm — add the three fields). Use perps-meaningful values; exposure cap stays generous so spot never binds:

```rust
            // Conservative:
                max_funding_pay_8h: 0.01,
                min_liq_distance_pct: 8.0,
                max_total_exposure_pct: 100.0,
            // Balanced:
                max_funding_pay_8h: 0.02,
                min_liq_distance_pct: 5.0,
                max_total_exposure_pct: 150.0,
            // Aggressive:
                max_funding_pay_8h: 0.05,
                min_liq_distance_pct: 3.0,
                max_total_exposure_pct: 250.0,
```

> **Spec §4 open decision:** if the parity test in Task 6 shows any preset backtest changed, set that preset's `max_total_exposure_pct` to `0.0` (disabled) instead. Spot exposure ≤ NAV so 100–250% should never bind; the test confirms it.

- [ ] **Step 2b: Fix the exhaustive `RiskConfig` struct literals.** `RiskConfig` does **not** derive `Default`, so adding three fields breaks every exhaustive `RiskConfig { .. }` literal — the workspace won't compile until each is updated. There are exactly **4** such sites (all in tests; the `autooptimizer_*` tests build `RiskConfig` via `serde_json` and are covered by `#[serde(default)]`, so they need no change). Append these three lines (`= 0.0`, i.e. disabled → zero behavior change) to each literal:

```rust
        max_funding_pay_8h: 0.0,
        min_liq_distance_pct: 0.0,
        max_total_exposure_pct: 0.0,
```

Sites:
- `crates/xvision-engine/tests/parity_pipeline_seed_byte_identical.rs:39` (the parity gate itself — must compile)
- `crates/xvision-engine/tests/eval_causal_input_sanitization.rs:188`
- `crates/xvision-engine/tests/pine_import_map.rs:469`
- `crates/xvision-engine/tests/eval_exit_enforcement.rs:369`

Verify the list is complete before building: `grep -rn "RiskConfig {" crates/xvision-engine --include='*.rs' | grep -v 'strategies/risk.rs'` should show only these 4 (plus the `fn distinctive_risk()` wrappers). Build the engine tests to confirm: `scripts/cargo test -p xvision-engine --no-run`.

- [ ] **Step 3: Declare the module** (`risk.rs`, top, after `use serde...`)

```rust
pub mod perps;
```

- [ ] **Step 4: Write the helper with failing tests** (`strategies/risk/perps.rs`)

```rust
//! Perps entry vetoes for the engine R3 risk path. Pure; venue-gated.
//!
//! Funding-carry and liquidation-distance checks, lifted from the retired
//! `xvision-risk` `FundingCarryGuard` / `LiquidationDistanceGuard` rules.
//! Both fail-safe to no-op when the relevant datum is absent (spot/backtest)
//! and never fire unless `is_perp_venue` is true.

use xvision_core::trading::{Direction, VetoReason};

use super::RiskConfig;

/// Decide whether a NEW open should be vetoed on perps risk grounds.
/// Returns `None` (allow) when not a perps venue, not a new open, or when
/// the gating data is absent.
///
/// - `is_perp_venue`: from `BrokerSurface::is_perp_venue()` (false on spot).
/// - `is_new_open`: true only for `long_open` / `short_open`.
/// - `funding_rate_8h`: `PerpsContext.funding_rate` (None ⇒ funding check skipped).
/// - `min_position_liq_distance_pct`: smallest liq-distance % across open
///   positions (None ⇒ liquidation check skipped; populated by the follow-on
///   data-plumbing track).
pub fn perps_entry_veto(
    cfg: &RiskConfig,
    is_perp_venue: bool,
    is_new_open: bool,
    direction: Direction,
    funding_rate_8h: Option<f64>,
    min_position_liq_distance_pct: Option<f64>,
) -> Option<VetoReason> {
    if !is_perp_venue || !is_new_open {
        return None;
    }
    // Funding-carry: a long pays +funding, a short pays -funding.
    if cfg.max_funding_pay_8h > 0.0 {
        if let Some(funding) = funding_rate_8h {
            let pay_rate = match direction {
                Direction::Long => funding,
                Direction::Short => -funding,
                Direction::Flat => return None,
            };
            if pay_rate > cfg.max_funding_pay_8h {
                return Some(VetoReason::PunitiveFunding);
            }
        }
    }
    // Liquidation-distance: any open position within the configured % of liq.
    if cfg.min_liq_distance_pct > 0.0 {
        if let Some(dist) = min_position_liq_distance_pct {
            if dist < cfg.min_liq_distance_pct {
                return Some(VetoReason::NearLiquidation);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::risk::RiskPreset;

    fn cfg() -> RiskConfig {
        let mut c = RiskPreset::Balanced.expand();
        c.max_funding_pay_8h = 0.01;
        c.min_liq_distance_pct = 5.0;
        c
    }

    #[test]
    fn no_op_on_spot_venue() {
        assert_eq!(
            perps_entry_veto(&cfg(), false, true, Direction::Long, Some(0.5), Some(1.0)),
            None
        );
    }

    #[test]
    fn no_op_when_not_new_open() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, false, Direction::Long, Some(0.5), Some(1.0)),
            None
        );
    }

    #[test]
    fn veto_long_paying_punitive_funding() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, Some(0.05), None),
            Some(VetoReason::PunitiveFunding)
        );
    }

    #[test]
    fn short_receives_funding_passes() {
        // Short pays -funding; +0.05 funding ⇒ short receives ⇒ pass.
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Short, Some(0.05), None),
            None
        );
    }

    #[test]
    fn absent_funding_is_no_op() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, None, None),
            None
        );
    }

    #[test]
    fn veto_near_liquidation() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, None, Some(2.0)),
            Some(VetoReason::NearLiquidation)
        );
    }

    #[test]
    fn liq_distance_above_threshold_passes() {
        assert_eq!(
            perps_entry_veto(&cfg(), true, true, Direction::Long, None, Some(9.0)),
            None
        );
    }
}
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `scripts/cargo test -p xvision-engine strategies::risk::perps`
Expected: PASS (7 tests). (They compile against the real `RiskConfig`/`Direction`/`VetoReason`, so a green run also proves the config fields landed.)

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/strategies/risk.rs crates/xvision-engine/src/strategies/risk/perps.rs
git commit -m "feat(risk): perps_entry_veto helper + RiskConfig perps/exposure fields + presets"
```

---

## Task 3: Wire `perps_entry_veto` into both R3 veto blocks

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` (backtest block ~L2028-2089; live block ~L3982-4032)

The existing blocks compute `daily_loss_breached || max_positions_breached` then, on breach, write a note + `risk_veto` engine event and rewrite to `"hold"`. Extend each to also consult `perps_entry_veto`, reusing the same note/event/rewrite path with a perps reason string.

- [ ] **Step 1: Compute `is_perp_venue` once per loop.**
  - **Backtest loop** (constant — backtest is spot-only). Just before the R3 block (~L2028), no broker exists: use `let is_perp_venue = false;`.
  - **Live loop** (~L3982): derive from the live runtime's broker. The locked `LiveRuntime` guard is bound as `runtime` and exposes `runtime.fill_sink` (a `RealBrokerFills`, passed as `&mut runtime.fill_sink` into `decide_one_live`). Read the gate once, **above** the asset/decision loop (~L3428, where `runtime` is first available), into a local `bool`:

```rust
    // Perps risk gate: only directional-perps venues run the funding /
    // liquidation guards. Read once; the broker venue is fixed per run.
    let is_perp_venue = runtime.fill_sink.is_perp_venue();
```

  > Confirm the guard binding name by searching `live_runtime` / `fill_sink` in the live-loop method (verified to be `runtime` at the time of writing). The bool is `Copy`, so threading it into `decide_one_live` (add a `bool` param) or computing it inside that fn from the `&mut RealBrokerFills` it already receives both work — prefer adding the param so the value is read exactly once.

- [ ] **Step 2: Extend the breach condition** — in **both** blocks, after the existing `let ... = max_positions_breached ...;` line and before `if daily_loss_breached || max_positions_breached {`, insert:

```rust
                        let direction = if applied_action == "short_open" {
                            xvision_core::trading::Direction::Short
                        } else {
                            xvision_core::trading::Direction::Long
                        };
                        // Liquidation-distance data is not yet plumbed into the
                        // engine book (follow-on track); pass None ⇒ that check
                        // no-ops. Funding comes from the per-decision perps ctx.
                        let perps_veto = crate::strategies::risk::perps::perps_entry_veto(
                            &strategy.risk,
                            is_perp_venue,
                            true, // is_new_open: this branch only runs for new opens
                            direction,
                            perps_funding_rate, // Option<f64>; see Step 3
                            None,
                        );
```

- [ ] **Step 3: Source the funding rate.**
  - **Backtest loop:** `let perps_funding_rate: Option<f64> = None;` (backtest passes `PerpsContext::default()`).
  - **Live loop:** the live `PerpsContext` is currently an inline `PerpsContext::default()` literal at the `DecisionSeedInput { ... perps: PerpsContext::default() ... }` construction (~L3895). First extract it to a named binding so both the seed and the veto read the same value:

```rust
    let perps_ctx = PerpsContext::default(); // follow-on track populates funding/OI here
    // ... then use `perps: perps_ctx,` in DecisionSeedInput, and:
    let perps_funding_rate = perps_ctx.funding_rate; // Option<f64>; None until the feed lands ⇒ no-op
```

- [ ] **Step 4: Fold the perps veto into the breach handling** — change the breach gate and reason selection in **both** blocks:

```rust
                        let breach_reason: Option<&str> = if daily_loss_breached {
                            Some("daily_loss_kill")
                        } else if max_positions_breached {
                            Some("max_concurrent_positions")
                        } else {
                            match perps_veto {
                                Some(xvision_core::trading::VetoReason::PunitiveFunding) => Some("punitive_funding"),
                                Some(xvision_core::trading::VetoReason::NearLiquidation) => Some("near_liquidation"),
                                _ => None,
                            }
                        };

                        if let Some(reason) = breach_reason {
                            // ... existing note + emit_engine_event("risk_veto", ...) + rewrite to "hold",
                            //     using `reason` in place of the old `reason` binding.
                        } else {
                            applied_action
                        }
```

  > Replace the existing `let reason = if daily_loss_breached {...} else {...};` and `if daily_loss_breached || max_positions_breached {` with the `breach_reason`/`if let Some(reason)` form above. Keep the note text, `record_supervisor_note`, `emit_engine_event`, and (live block only) `risk_vetoed = true;` exactly as they are.

- [ ] **Step 5: Build**

Run: `scripts/cargo build -p xvision-engine`
Expected: clean build.

- [ ] **Step 6: Regression — existing eval tests still green**

Run: `scripts/cargo test -p xvision-engine --test eval_executor_live_loop --test eval_run_mtm`
Expected: PASS (perps veto is inert: `is_perp_venue=false` in backtest, `None` funding/liq).

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/backtest.rs
git commit -m "feat(risk): wire venue-gated perps_entry_veto into both R3 veto blocks"
```

---

## Task 4: `MaxTotalExposure` general veto

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` (both R3 blocks — same two sites as Task 3)

Exposure = Σ(|position| × mark) over open legs, as a percent of NAV. A new open's projected notional ≈ `estimated_qty × next_bar_open` (the same estimate the broker-rule check uses). Veto when `max_total_exposure_pct > 0.0` and projected exceeds it.

- [ ] **Step 1: Add a pure helper to `perps.rs`** (despite the file name, this is the engine-risk math module). After `perps_entry_veto`:

```rust
/// Whether opening `new_notional_usd` more would push total open exposure
/// past `max_total_exposure_pct` of `nav_usd`. `0.0` cap (or non-positive
/// nav) disables. `existing_notional_usd` is Σ(|position| × mark) over open
/// legs.
pub fn exceeds_total_exposure(
    max_total_exposure_pct: f64,
    nav_usd: f64,
    existing_notional_usd: f64,
    new_notional_usd: f64,
) -> bool {
    if max_total_exposure_pct <= 0.0 || nav_usd <= 0.0 {
        return false;
    }
    let projected_pct = ((existing_notional_usd + new_notional_usd) / nav_usd) * 100.0;
    projected_pct > max_total_exposure_pct
}
```

- [ ] **Step 2: Unit tests** (append to `perps.rs` `mod tests`)

```rust
    #[test]
    fn exposure_disabled_at_zero_cap() {
        assert!(!exceeds_total_exposure(0.0, 1000.0, 5000.0, 5000.0));
    }
    #[test]
    fn exposure_under_cap_passes() {
        assert!(!exceeds_total_exposure(150.0, 1000.0, 500.0, 500.0)); // 100% ≤ 150%
    }
    #[test]
    fn exposure_over_cap_vetoes() {
        assert!(exceeds_total_exposure(100.0, 1000.0, 800.0, 800.0)); // 160% > 100%
    }
```

- [ ] **Step 3: Run helper tests**

Run: `scripts/cargo test -p xvision-engine strategies::risk::perps`
Expected: PASS (10 tests total).

- [ ] **Step 4: Wire into both R3 blocks** — extend `breach_reason` (Task 3 Step 4) with an exposure branch. Just before the `breach_reason` computation, compute:

```rust
                        let exposure_breached = {
                            let cap = strategy.risk.max_total_exposure_pct;
                            if cap > 0.0 {
                                let existing: f64 = book
                                    .open_legs()
                                    .iter()
                                    .map(|(_, pos, _entry, mark)| pos.abs() * mark)
                                    .sum();
                                let new_notional = {
                                    let usd_at_risk = equity * strategy.risk.risk_pct_per_trade;
                                    // qty ≈ usd_at_risk / next_bar_open; notional ≈ qty × next_bar_open
                                    usd_at_risk.max(0.0)
                                };
                                crate::strategies::risk::perps::exceeds_total_exposure(
                                    cap, equity, existing, new_notional,
                                )
                            } else {
                                false
                            }
                        };
```

  > `equity` is in scope at both veto sites (used by the broker-rule estimate). `book.open_legs()` returns `(AssetSymbol, position, entry_price, last_mark)`. The new-open notional reuses the existing `usd_at_risk = equity * risk_pct_per_trade` estimate already present a few lines below for the broker-rule check — keep it consistent.

  Then add to the `breach_reason` chain (after `max_positions_breached`, before the `perps_veto` match):

```rust
                        } else if exposure_breached {
                            Some("max_total_exposure")
                        } else {
```

- [ ] **Step 5: Build + regression**

Run: `scripts/cargo build -p xvision-engine && scripts/cargo test -p xvision-engine --test eval_executor_live_loop --test eval_run_mtm`
Expected: clean build; PASS (default cap on these tests' configs must not change fills — if a test config uses a preset with a cap and a fill disappears, set that preset's `max_total_exposure_pct` to `0.0` per Task 2 Step 2 note).

- [ ] **Step 6: Parity gate**

Run: `scripts/cargo test -p xvision-engine --test parity_pipeline_seed_byte_identical`
Expected: PASS (proves seed/exposure changes did not alter the deterministic seed).

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/strategies/risk/perps.rs crates/xvision-engine/src/eval/executor/backtest.rs
git commit -m "feat(risk): port MaxTotalExposure as a general engine veto (disabled-by-default)"
```

---

## Task 5: Retire `xvision-risk`, `xvision-harness`, the eval harness, and `xvn risk`

**Files:** multiple deletions + Cargo edits (see File Structure).

- [ ] **Step 1: Delete the dead `xvision-eval` harness**

```bash
git rm crates/xvision-eval/src/harness.rs
```
Edit `crates/xvision-eval/src/lib.rs`: remove the line `pub mod harness;` (L9).
Edit `crates/xvision-eval/Cargo.toml`: remove the `xvision-risk = { path = "../xvision-risk" }` line (L17).

- [ ] **Step 2: Delete the `xvn risk` command**

```bash
git rm crates/xvision-cli/src/commands/risk.rs
```
Edit `crates/xvision-cli/src/lib.rs`: remove the `/// Risk layer evaluation...` doc + the `Risk(commands::risk::RiskCmd),` enum variant (~L189-190) and the `Command::Risk(cmd) => commands::risk::run(cmd)...` match arm (~L328).
Edit `crates/xvision-cli/src/commands/mod.rs`: remove the `pub mod risk;` declaration.
Edit `crates/xvision-cli/Cargo.toml`: remove `xvision-risk` (L27) and `xvision-harness` (L29) deps.

- [ ] **Step 3: Delete the dead observability methods** (`crates/xvision-engine/src/agent/observability.rs`)
Delete `emit_risk_gate_started` and `emit_risk_gate_finished` (the methods at ~L1379 and ~L1403 — verified zero call sites). Update the `RiskLayer::evaluate` references in the surrounding doc comments to read "the engine R3 risk veto". In `crates/xvision-observability/src/types.rs:145`, update the `BacktestRunner`/`RiskLayer` doc comment to reference the engine veto. (Leave the `SpanKind::RiskGate` variant.)

- [ ] **Step 4: Fix the engine test** (`crates/xvision-engine/tests/risk_min_notional.rs`)
Remove the imports `use xvision_risk::rules::MinNotional;` (L41) and `use xvision_risk::{RiskEvalContext, RiskRule, RuleVerdict};` (L42), and delete the `exact_min_notional_boundary_passes_risk_rule` test fn (the only user of those imports). Leave the `#[ignore]`d integration tests untouched.

- [ ] **Step 4b: Update stale doc references to the deleted surfaces**
  - `crates/xvision-core/src/config.rs` (~L396, L405, L412): the `RiskPerpsGuards` struct + `RiskConfig.perps` field are the **xvision-core mirror** of the `risk.toml [perps]` section (per the "risk.toml parsed by two crates" convention). **Keep the struct and field** so existing `risk.toml` files still deserialize, but update the doc comments that say "consumed by the xvision-risk crate's own `PerpsGuards`" — that crate is gone. New wording: note that `risk.toml [perps]` is now a **vestigial global mirror**; the live perps vetoes read the per-strategy `strategy.risk.{max_funding_pay_8h,min_liq_distance_pct}` instead (consistent with how `daily_loss_kill_pct` / `max_concurrent_positions` are sourced). Flag the vestigial mirror for a future cleanup; do not remove it here.
  - `docs/cli-non-surfaced.md` (L66, L106): remove/replace the `xvn risk show-config` / `xvn risk evaluate` references — that command is deleted. Point readers at the engine R3 veto (`strategy.risk`) instead.

- [ ] **Step 5: Delete the two crates + workspace wiring**

```bash
git rm -r crates/xvision-risk crates/xvision-harness
```
Edit `crates/xvision-engine/Cargo.toml`: remove `xvision-risk = { path = "../xvision-risk" }` (L23).
Edit root `Cargo.toml`: remove `"crates/xvision-risk",` and `"crates/xvision-harness",` from `[workspace] default-members`. (The `members = ["crates/*", ...]` glob no longer matches the deleted dirs — nothing to change there.)

- [ ] **Step 6: Re-grep for dangling references**

Run (scan source AND docs):
```bash
grep -rn "xvision_risk\|xvision-risk\|xvision_harness\|xvision-harness\|RiskLayer\|commands::risk\|harness::BacktestRunner\|xvn risk" crates/ docs/ --include='*.rs' --include='*.toml' --include='*.md' | grep -v '/target/'
```
Expected: **no** hits except (a) this plan/spec's own prose under `docs/superpowers/`, and (b) the kept `RiskPerpsGuards` struct in `xvision-core/config.rs` with its now-corrected doc wording. Fix any other source/doc hit (esp. stray comment refs in `api/eval.rs`, `autooptimizer/mutator.rs`, `xvision-observability/types.rs`).

- [ ] **Step 7: Build the whole workspace**

Run: `scripts/cargo build --workspace`
Expected: clean build (Cargo.lock regenerates; the two crates drop out).

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "refactor(risk): retire xvision-risk + xvision-harness + eval BacktestRunner + xvn risk

The perps guards (FundingCarryGuard, LiquidationDistanceGuard) now live on the
engine R3 veto path; the structured RiskLayer + its only callers (orphaned
eval harness, xvn risk CLI, run-setup already removed) are deleted. VetoReason
+ OpenPosition.liq_price stay in xvision-core; config/risk.toml stays
(xvision-core reads it)."
```

---

## Task 6: Workspace verification

- [ ] **Step 1: Full build**

Run: `scripts/cargo build --workspace`
Expected: clean.

- [ ] **Step 2: Targeted test suites**

Run: `scripts/cargo test -p xvision-engine -p xvision-eval -p xvision-cli -p xvision-execution`
Expected: PASS. Investigate any failure against the baseline (some pre-existing `#[ignore]`/baseline-rot tests are unrelated — confirm via `git stash` comparison only if a red test looks pre-existing).

- [ ] **Step 3: Confirm live risk path intact**

Run: `scripts/cargo test -p xvision-engine --test eval_executor_live_loop`
Expected: PASS — the engine's `daily_loss_kill` / `max_concurrent_positions` vetoes (the live risk path) are unchanged.

- [ ] **Step 4: Final review + push**

```bash
git status
git log --oneline feat/perps-risk-unification ^main
```
Review the full diff for out-of-scope/unreported changes (scan for `D ` deletions beyond the planned crates), then hand back for PR.

---

## Self-Review

- **Spec coverage:** venue gate incl. `→ true` overrides on the three real perps adapters — Orderly/byreal/Bybit (T1.3b), helper+config+presets (T2), both-block wiring (T3), MaxTotalExposure general veto with disabled-by-default (T4), full retirement incl. Cargo/default-members/test fix/dead obs methods + stale-doc cleanup (T5), build/test verification incl. parity gate (T4.6, T6). `max_leverage` correctly out of scope. ✓
- **Gate iteration 1 fixes applied:** (1) added `is_perp_venue()=true` overrides on `OrderlyLiveSurface`/`ByrealLiveSurface`/`BybitPaperSurface` so the gate is not permanently inert (T1.3b + test T1.1); (2) helper signature `is_new_open: bool, direction: Direction` now matches the spec (spec §2 updated); (3) folded stale-doc cleanup for `xvision-core/config.rs RiskPerpsGuards` + `docs/cli-non-surfaced.md` into T5.4b + broadened T5.6 re-grep to `docs/`; (4) corrected the live-loop bindings to the verified `runtime.fill_sink` / named `perps_ctx` (T3.1, T3.3).
- **Gate iteration 2 fix applied:** `RiskConfig` has no `Default` derive, so adding 3 fields breaks 4 exhaustive `RiskConfig { .. }` test literals (incl. the parity gate). T2.2b now updates all 4 sites with the new fields `= 0.0` (disabled → zero behavior change); the `autooptimizer_*` JSON-built configs are unaffected via `#[serde(default)]`. Also corrected the Orderly test mock to the in-scope `MockApi { .. }` pattern (T1.1).
- **Placeholders:** none — every code step shows code; the "confirm the binding name" notes (T3.1/T3.3) are verified-name guidance with explicit fallbacks.
- **Type consistency:** `perps_entry_veto` / `exceeds_total_exposure` signatures, `VetoReason::{PunitiveFunding,NearLiquidation}`, `Direction::{Long,Short,Flat}`, `book.open_legs()` 4-tuple, `is_perp_venue()` — all consistent across tasks and with the spec.
