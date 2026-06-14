# Perps risk unification — converge Byreal/perps risk onto the engine veto path

**Date:** 2026-06-14
**Status:** Design (approved in brainstorming; pending spec review)
**Branch:** `feat/perps-risk-unification`

## Problem

The codebase has **two parallel risk systems**, and the perps risk rules sit in
the wrong one:

1. **The live risk gate** — the engine executor's inline "R3 risk-veto" block in
   `crates/xvision-engine/src/eval/executor/backtest.rs` (present in both the
   backtest decision loop ~L2012 and the live decision loop ~L3965). It reads
   `strategy.risk` (`crate::strategies::risk::RiskConfig`) and vetoes **new
   opens** on `daily_loss_kill_pct` and `max_concurrent_positions`, plus venue
   `broker_rules` (min order size, etc.). **Every spot + live run passes through
   this.**

2. **`xvision-risk::RiskLayer`** — a structured `RiskRule` engine
   (Approved/Modified/Vetoed). The two perps rules added for the Byreal/perps
   track live here:
   - `FundingCarryGuard` (#985) → `VetoReason::PunitiveFunding`
   - `LiquidationDistanceGuard` (#1000) → `VetoReason::NearLiquidation`

   But `RiskLayer` is only invoked by the **orphaned** `xvision-eval`
   `BacktestRunner` harness (no production caller — the `ab-compare` CLI verb
   that used it was already removed), the `xvn risk` CLI command, and tests. **It
   never gates a live trade.** The perps guards were built into a dead-end path.

The Byreal/perps work was pulled into the dead-end code and built there, so the
new perps risk rules are stranded off the path everything else uses.

## Goal

Make perps risk fire through the **same engine veto path** as everything else,
and retire the redundant `xvision-risk` system. Perps risk must be **active only
when perps are actually in play** (perps venue), with **zero behavior change** to
existing spot/backtest runs.

## Non-goals (explicit follow-on)

- **Live perps data plumbing.** The perps guards need data at the veto site:
  funding rate (`PerpsContext`, currently a `default()`/`None` stub on both
  backtest and live paths — the `xvision_data::perp_feed::fetch_perp_snapshot`
  poller is the unbuilt "LIVE PERPS ATTACH POINT") and per-position
  `liq_price` (populated by the byreal/Hyperliquid execution adapter into
  `xvision_core::OpenPosition`, but the engine's backtest `PortfolioBook` doesn't
  carry it). **This spec moves the rule logic onto the canonical path; it does
  not build the data feed.** The guards no-op (fail-safe) until that plumbing
  lands — which is the same fire-readiness as today, except now the rules live on
  the one veto path, so they activate the moment the data arrives. The data feed
  is a separate spec.
- **`max_leverage` enforcement.** Already plumbed: `strategy.risk.max_leverage`
  flows into the trader's seed/`risk_caps`, sits in `safety/limits.rs`, and
  drives `set_leverage()` at the venue. It is not a deterministic veto, but it is
  handled; no new leverage veto in this change.

## Background: venue / perp-ness

- The engine's scenario `Venue` enum (`eval/scenario.rs`) has a single variant,
  `Alpaca` (spot). **Backtest is spot-only.**
- Perps venues live in the **execution-adapter** layer: `byreal.rs` is the
  **Hyperliquid perps** adapter (native HL EIP-712 signing); Orderly is also
  perps (`PERP_*` symbols). `byreal_clmm.rs` is the Byreal **Solana CLMM LP** DEX
  (not directional perps — funding/liquidation guards do not apply).
- `BrokerSurface` (`execution/src/broker_surface.rs`) exposes `fn venue(&self) ->
  &str`. There is no perp/spot classifier today.

**Conclusion:** perps only ever occur on the **live** path with a perps broker
surface. The activation gate keys on that.

## Design

### 1. Activation gate — `BrokerSurface::is_perp_venue()`

Add to the `BrokerSurface` trait:

```rust
/// Whether this venue trades directional perpetual futures (funding +
/// liquidation apply). Default false; overridden true only on the
/// directional-perps adapters.
fn is_perp_venue(&self) -> bool { false }
```

Override `→ true` on every directional-perps `BrokerSurface` impl that already
exists in the codebase:
- `ByrealLiveSurface<A>` (`byreal.rs:625`, Hyperliquid perps — `ByrealPerpsApi`)
- `OrderlyLiveSurface<A>` (`broker_surface.rs:828`, venue `"orderly"`)
- `BybitPaperSurface<A>` (`bybit.rs:319`, Bybit `category=linear` perps)

Leave `→ false` (default) on the spot/test surfaces: `AlpacaPaperSurface`,
`AlpacaLiveSurface`, `MockBrokerSurface`, `DefaultsBroker`. (The Byreal CLMM LP
DEX is not a `BrokerSurface` impl and is unaffected.)

The perps guards run **only when `is_perp_venue()` is true**. Backtest and
live-spot are `false` → guards are completely inert.

### 2. Shared veto helper — `strategies::risk::perps`

New module `crates/xvision-engine/src/strategies/risk/perps.rs` with one pure
function (logic lifted verbatim from the two `xvision-risk` rules):

```rust
pub fn perps_entry_veto(
    cfg: &RiskConfig,
    is_perp_venue: bool,
    is_new_open: bool,     // true only for long_open / short_open; caller-computed
    direction: Direction,
    funding_rate_8h: Option<f64>,        // from PerpsContext (None ⇒ no-op)
    min_position_liq_distance_pct: Option<f64>, // min over open positions (None ⇒ no-op)
) -> Option<VetoReason>
```

The caller passes `is_new_open`/`direction` pre-computed (the R3 block already
gates on new-open and has the applied action in hand), so the helper stays a
pure decision function with no string parsing. Early-returns `None` when:
`!is_perp_venue`, not a new open (exits included), or the relevant datum is
absent. Otherwise:
- **Funding-carry:** a long pays `+funding`, a short pays `-funding`; if the pay
  rate exceeds `cfg.max_funding_pay_8h` ⇒ `Some(PunitiveFunding)`.
- **Liquidation-distance:** if any open position's liq distance %
  `< cfg.min_liq_distance_pct` ⇒ `Some(NearLiquidation)`.

Pure and unit-tested; the existing rule tests port over nearly unchanged.

### 3. Wiring into both veto blocks

In `backtest.rs`, both R3 veto sites call `perps_entry_veto(...)` alongside the
existing `daily_loss` / `max_concurrent_positions` checks. On a perps veto, the
open is rewritten to `hold` and a `risk_veto` supervisor note + engine event are
emitted (identical handling to the existing vetoes — reuse the same code path,
just add the reason strings `punitive_funding` / `near_liquidation`).

- **Backtest path:** `is_perp_venue = false` (Alpaca-only) → helper returns
  `None` → no behavior change.
- **Live path:** `is_perp_venue = broker.is_perp_venue()`; `funding_rate_8h` from
  the per-decision `PerpsContext` (the same struct already threaded into the
  trader input); `min_position_liq_distance_pct` from open positions once the
  data plumbing (follow-on) populates it — `None` until then ⇒ no-op.

### 4. MaxTotalExposure (ExposureCap) — general control

The one rule with no engine equivalent. Ported as a **general** veto (all
strategies, not perps-gated), in the R3 block next to `daily_loss` /
`max_concurrent_positions`:

- Add `max_total_exposure_pct: f64` to `strategies::risk::RiskConfig`
  (`#[serde(default)]`, **`0.0` = disabled**, mirroring `daily_loss_kill_pct`).
- New open vetoed when projected total exposure (sum of open-position notionals
  as % NAV) would exceed the cap ⇒ rewrite to `hold`, reason
  `max_total_exposure`. Reuses the existing veto-handling code path.
- **Behavior preservation:** default `0.0` (disabled) means configs/scenarios
  that don't set it are byte-identical. The three `RiskPreset` expansions carry a
  cap (mirroring the retired `xvision-risk` value, ~100% NAV). For spot (no
  leverage) total exposure stays ≤ NAV so the cap rarely binds; for leveraged
  perps it is a real control. **Implementation must verify** the
  `parity_pipeline_seed_byte_identical` and eval-baseline tests still pass with
  the preset values; if any preset backtest changes, set that preset's cap to
  `0.0` (disabled) instead. (Open decision for spec review: set presets to a real
  cap vs. ship disabled-by-default.)

### 5. Config + presets

Add to `strategies::risk::RiskConfig`:
- `max_funding_pay_8h: f64` (perps; `#[serde(default)]`)
- `min_liq_distance_pct: f64` (perps; `#[serde(default)]`)
- `max_total_exposure_pct: f64` (general; `#[serde(default)]`, 0.0 = disabled)

Set sensible perps defaults in `RiskPreset::{Conservative,Balanced,Aggressive}`
(inert on spot via the venue gate, so values only matter on perps venues).

### 6. Retirement

Delete:
- `crates/xvision-risk/` (entire crate — RiskLayer, all 10 rules, RiskConfig,
  Whitelist, RiskEvalContext, its tests)
- `crates/xvision-harness/` (entire crate — `apply_risk`; its only caller,
  `xvn run-setup`, was already removed upstream)
- `crates/xvision-eval/src/harness.rs` + `pub mod harness;` in `lib.rs`
  (the orphaned A/B `BacktestRunner`)
- `xvn risk` CLI command (`commands/risk.rs` + the `Risk` verb + `lib.rs` wiring)
- Dead `emit_risk_gate_started` / `emit_risk_gate_finished` in
  `engine/src/agent/observability.rs` (zero call sites) + the `RiskLayer`
  doc-comment references there and in `observability/src/types.rs`. **Leave the
  `SpanKind::RiskGate` enum variant** (DB/serialization safety).

Edit:
- Drop `xvision-risk` dep from `xvision-engine`, `xvision-eval`, `xvision-cli`
  Cargo.toml; drop `xvision-harness` from `xvision-cli`.
- Remove both crates from `[workspace] default-members` in root `Cargo.toml`.
- `engine/tests/risk_min_notional.rs`: drop the two `xvision_risk` imports + the
  `exact_min_notional_boundary_passes_risk_rule` unit test (the integration
  tests are already `#[ignore]`d and don't depend on `xvision_risk`).

Keep:
- `xvision_core::VetoReason` (incl. `PunitiveFunding`, `NearLiquidation`,
  `ExposureCap`/equivalent) and `xvision_core::OpenPosition.liq_price` /
  `.leverage`.
- `config/risk.toml` + `config/whitelist.toml` (xvision-core still reads
  risk.toml via `load_risk`).

### Audit dispositions (all 10 rules)

| Rule | Disposition |
|---|---|
| FundingCarryGuard | **Port** (venue-gated) |
| LiquidationDistanceGuard | **Port** (venue-gated) |
| MaxTotalExposure | **Port** (general, behavior-preserving default) |
| DailyLossCircuit | Drop — engine `daily_loss_kill_pct` veto covers it |
| MaxOpenPositions | Drop — engine `max_concurrent_positions` veto covers it |
| MinNotional | Drop — engine venue `broker_rules` covers it |
| AssetWhitelist | Drop — `manifest.asset_universe` enforces upstream |
| StopLossPresent | Drop — engine stops are mechanical (always present) |
| MaxPositionSize | Drop — rule only warns; engine sizes via `risk_pct_per_trade` |
| TakeProfitRR | Drop — `required=false` default; TP range validated in trader_output |

## Data flow

```
trader decision ─► guardrails::classify ─► R3 veto block ─────────────► fill/broker
                                            ├ daily_loss_kill (existing)
                                            ├ max_concurrent_positions (existing)
                                            ├ max_total_exposure (NEW, general)
                                            └ perps_entry_veto(...) (NEW, venue-gated)
                                                 ├ if !is_perp_venue ⇒ None
                                                 ├ funding-carry (needs PerpsContext.funding)
                                                 └ liq-distance (needs position liq_price)
```

## Testing

- **Unit:** port the `FundingCarryGuard` / `LiquidationDistanceGuard` rule tests
  to `strategies::risk::perps` tests (veto on punitive funding, no-op on
  favorable/absent funding, veto near liquidation, no-op for spot/no liq_price,
  exits always pass). Add `is_perp_venue=false ⇒ None` gate tests.
- **Exposure:** unit tests for the total-exposure veto (binds over cap, passes
  under, disabled at 0.0).
- **Regression:** `parity_pipeline_seed_byte_identical` + eval-baseline tests
  must stay green (proves zero spot behavior change).
- **Build:** `scripts/cargo build --workspace` + `cargo test` for
  `xvision-engine`, `xvision-eval`, `xvision-cli` after the crate removals.

## Risks

- **Workspace removal churn:** deleting two crates touches Cargo manifests +
  `default-members` + the Docker default-members build. Mitigation: full
  `--workspace` build/test in the worktree before PR.
- **MaxTotalExposure behavior change:** mitigated by disabled-by-default + the
  parity test gate (see §4).
- **Hidden `xvision-risk` consumers:** verified consumers are only eval-harness /
  CLI / tests; re-grep after deletion to confirm no dangling references.

## Follow-on (separate specs)

1. **Live perps data plumbing** — build `xvision_data::perp_feed` into
   `PerpsContext` at the live veto site; make engine position tracking
   liq-price-aware so `LiquidationDistanceGuard` has data. This is what makes the
   ported guards actually bite live.
2. (Optional) Deterministic `max_leverage` veto, if the seed/safety-limit/venue
   handling proves insufficient in live perps testing.
