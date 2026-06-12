# Byreal Perps + Perps-Aware Single-Stage Agent — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Activate Byreal as a live perpetual-futures executor and thread real perps data (funding, OI, mark/basis, leverage, liquidation, funding paid) through the filter, single-stage agent, and positions surfaces — with the Byreal capability highlighted in the UI.

**Architecture:** Retire the legacy two-stage Intern→Trader model; fold the Intern's skill-injection + onchain/perps rendering into a single-stage agent context builder (`agents/context.rs`). Add a `ByrealPerpsExecutor` implementing the existing venue-agnostic `Executor` trait via a mockable subprocess seam to `@byreal-io/byreal-perps-cli` (routes to Hyperliquid). A live-only public perps feed populates the already-existing `OnchainPanel` schema and new optional `Bar` fields; the filter `IndicatorName` catalog and positions structs gain perps fields. Backtest stays spot-only (perps deferred, fields default to `None`).

**Tech Stack:** Rust (tokio, reqwest, serde) workspace; SQLx migrations; React + TanStack Query + Tailwind frontend. Build through `scripts/cargo` (disk guard). Spec: `docs/superpowers/specs/2026-06-12-byreal-perps-single-stage-agent-design.md`.

---

## Execution status — 2026-06-12 (PR #1, branch `feat/byreal-perps-single-stage`)

Phase 1 executed inline. **Three highest-value, well-isolated pieces landed and
green**; two surfaces deferred after they proved more under-plumbed/risky than
this plan assumed. Findings recorded below so the follow-up doesn't re-discover
them.

**DONE (committed, all tests passing):**

- ✅ **Tasks 1+2 — filter perps indicators (dimension D).** 5 periodless
  `IndicatorName` variants (`FundingRate`, `OpenInterest`, `MarkPrice`,
  `MarkIndexBasis`, `LongShortRatio`) backed by 5 optional `Bar` fields, wired
  through `IndicatorEngine` push/value + DSL parser. 5 tests.
  - *Finding:* `Bar::new` takes **4 args**, not 6 (plan was wrong). The enum is
    closed with exhaustive matches only in `xvision-filters` — full workspace
    build confirms no downstream match broke.
- ✅ **Tasks 5+6 — `ByrealPerpsExecutor` (dimension B, headline).** Venue-agnostic
  `Executor` impl over a mockable `ByrealPerpsApi` subprocess seam
  (`npx @byreal-io/byreal-perps-cli`, routes to Hyperliquid), `venue="byreal"`.
  `submit`/`close_position`/`portfolio`, 5 mock-seam tests.
  - *Finding:* `RiskDecision` has **no `approved()`/`vetoed()` constructors** — it
    is a struct-variant enum; `Approved`/`Modified` carry a `warnings: Vec<String>`
    field. Patched 3 **pre-existing-broken** Orderly test fixtures (1214/1747/1807)
    that predate `warnings` and were blocking the execution test binary.
  - *Finding:* `AssetSymbol` has no `from_ticker`; use `str::parse::<AssetSymbol>()`.
- ✅ **Task 7 — Hyperliquid public perps feed (dimension C, partial).**
  `parse_perp_snapshot` + `apply_to_onchain` (populates `OnchainPanel`) +
  `fetch_perp_snapshot` against the public no-auth HL info endpoint. 3 tests.
  - *Finding (blocks full wiring):* there is **no live (non-test) `MarketSnapshot`
    build site** — every `OnchainPanel::default()` is a test fixture. The live
    agent path uses `build_decision_seed` (`backtest.rs`), so the feed's live
    call-site is `DecisionSeedInput` perps fields = **inside Task 4**. The clean
    wiring API (`apply_to_onchain`) is shipped; the call-site is deferred with A.

**DEFERRED (with the real reason):**

- ⬜ **Task 8 — capability badge (F).** *Recon was wrong:* `StrategyListItem`
  (`frontend/web/src/api/strategies.ts`) has **no `capabilities`/`skills` field**
  in this checkout. The badge needs a **backend API change** to surface those on
  the list item first — not the isolated frontend task originally scoped.
- ⬜ **Tasks 3+4 — fold the Intern (A).** The invasive single-stage refactor
  (5,000-line `backtest.rs` + `dispatch_capability.rs` + eval baselines).
  Deprioritized for risk/budget. The feed's live call-site (above) and any
  perps-in-agent-context work gate on Task 4's `DecisionSeedInput` change.

**PHASE 2 / STRETCH — not started.** Tasks 9–11 (positions perps fields) and all
stretch items remain. Note: `ByrealPosition` (Task 5) already carries `leverage`,
`liq_price`, `funding_paid_usd`, `unrealized_pnl_usd`, so Task 9's executor-side
parsing is partly pre-built.

**Recommended follow-up order:** Task 4 (`DecisionSeedInput` perps fields →
unblocks feed live-wiring + agent context) → Task 3 (retire Intern stage) →
Task 8 backend field + badge → Task 9 positions.

---

## Pre-flight (do once, before Task 1)

- [ ] Create an isolated worktree (REQUIRED — never branch in the main checkout):

```bash
cd /Users/edkennedy/Code/xvision
git worktree add .worktrees/byreal-perps -b feat/byreal-perps-single-stage
cd .worktrees/byreal-perps
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
cp ../../docs/superpowers/specs/2026-06-12-byreal-perps-single-stage-agent-design.md docs/superpowers/specs/ 2>/dev/null || true
git add docs/superpowers/specs/2026-06-12-byreal-perps-single-stage-agent-design.md docs/superpowers/plans/2026-06-12-byreal-perps-single-stage-agent.md
git commit -m "docs: byreal perps spec + plan"
```

- [ ] Confirm baseline is green before changing anything:

```bash
scripts/cargo build --workspace
scripts/cargo test -p xvision-filters -p xvision-core -p xvision-execution
```
Expected: PASS. If red, stop and report — do not build on a broken baseline.

---

# PHASE 1 — MUST-DO (cut line: A + B + C + F)

## Task 1: Perps market data model on `Bar` (Section C, data layer)

Adds optional perps scalars to the filter `Bar` so the live feed and filter engine can carry them. Backtest leaves them `None`.

**Files:**
- Modify: `crates/xvision-filters/src/indicators.rs` (`Bar` struct at :54; `IndicatorEngine` push/value)
- Test: `crates/xvision-filters/src/indicators.rs` (inline `#[cfg(test)]`)

- [ ] **Step 1: Read the anchors.** Read `crates/xvision-filters/src/indicators.rs:40-90` (the `Bar` struct + constructor) and `:260-340` (`IndicatorEngine::push` and `value`). Note the existing field style and how `last_*` cached fields are stored.

- [ ] **Step 2: Write the failing test.** Add to the `#[cfg(test)]` module in `indicators.rs`:

```rust
#[test]
fn bar_carries_optional_perps_fields_default_none() {
    let bar = Bar::new(1.0, 2.0, 0.5, 1.5, 100.0, 0);
    assert_eq!(bar.funding_rate, None);
    assert_eq!(bar.open_interest, None);
    assert_eq!(bar.mark_price, None);
}

#[test]
fn engine_reports_latest_perps_values() {
    let mut eng = IndicatorEngine::new(8);
    let mut bar = Bar::new(1.0, 2.0, 0.5, 1.5, 100.0, 0);
    bar.funding_rate = Some(0.0001);
    bar.open_interest = Some(5_000_000.0);
    bar.mark_price = Some(1.52);
    eng.push(&bar);
    assert_eq!(eng.value(&IndicatorName::FundingRate, None), Some(0.0001));
    assert_eq!(eng.value(&IndicatorName::OpenInterest, None), Some(5_000_000.0));
    assert_eq!(eng.value(&IndicatorName::MarkPrice, None), Some(1.52));
}
```

> Note: `IndicatorName::FundingRate` etc. are added in Task 2. To keep tasks independently committable, do Task 2's enum addition first OR co-commit Tasks 1+2. Recommended: implement Task 2 enum variants, then return here. (Both edit different files; one commit covering both is acceptable.)

- [ ] **Step 3: Run test to verify it fails.** `scripts/cargo test -p xvision-filters bar_carries_optional_perps` → FAIL (no field `funding_rate`).

- [ ] **Step 4: Implement.** Add to `Bar` (after `volume`/`timestamp`):

```rust
    /// Perps funding rate (per-interval fraction). None for spot bars.
    pub funding_rate: Option<f64>,
    /// Open interest in USD. None for spot bars.
    pub open_interest: Option<f64>,
    /// Venue mark price. None for spot bars.
    pub mark_price: Option<f64>,
```
In `Bar::new`, default the three to `None`. In `IndicatorEngine`, add cached fields `last_funding_rate/last_open_interest/last_mark_price: Option<f64>`, set them in `push` from `bar.funding_rate` etc. (only overwrite when `Some`), and return them from `value` for the new `IndicatorName` arms (added Task 2).

- [ ] **Step 5: Run tests.** `scripts/cargo test -p xvision-filters indicators::` → PASS.

- [ ] **Step 6: Commit.** `git commit -am "feat(filters): optional perps fields on Bar + IndicatorEngine"`

---

## Task 2: Perps `IndicatorName` variants + DSL (Section D)

**Files:**
- Modify: `crates/xvision-filters/src/types.rs` (`IndicatorName` enum :177; method arms :251-445; `parse_dsl` :491)
- Test: inline `#[cfg(test)]` in `types.rs`

- [ ] **Step 1: Read the anchors.** Read `types.rs:177-260` (enum + serde naming), `:251-445` (`has_period`, `dsl_prefix`, `period_bounds`), and `:480-540` (`parse_dsl`). Mirror the pattern used by a non-windowed field like `Close`.

- [ ] **Step 2: Write the failing test:**

```rust
#[test]
fn perps_indicators_parse_from_dsl() {
    assert_eq!(parse_dsl("funding_rate").unwrap().0, IndicatorName::FundingRate);
    assert_eq!(parse_dsl("open_interest").unwrap().0, IndicatorName::OpenInterest);
    assert_eq!(parse_dsl("mark_price").unwrap().0, IndicatorName::MarkPrice);
    assert_eq!(parse_dsl("mark_index_basis").unwrap().0, IndicatorName::MarkIndexBasis);
    assert_eq!(parse_dsl("long_short_ratio").unwrap().0, IndicatorName::LongShortRatio);
}

#[test]
fn perps_indicators_are_non_windowed() {
    assert!(!IndicatorName::FundingRate.has_period());
    assert!(!IndicatorName::OpenInterest.has_period());
}
```

- [ ] **Step 3: Run → FAIL** (`no variant FundingRate`). `scripts/cargo test -p xvision-filters perps_indicators`

- [ ] **Step 4: Implement.** Add variants `FundingRate, OpenInterest, MarkPrice, MarkIndexBasis, LongShortRatio` to `IndicatorName`. In each method: `has_period` → `false`; `dsl_prefix` → the snake tokens above; `period_bounds` → `None`/default (match the `Close` arm). In `parse_dsl`, route the five tokens to the variants. In `IndicatorEngine::value` (Task 1 file), return the matching `last_*` field.

- [ ] **Step 5: Run → PASS.** `scripts/cargo test -p xvision-filters` (full crate, ensure no match-exhaustiveness breaks).

- [ ] **Step 6: Commit.** `git commit -am "feat(filters): perps IndicatorName variants + DSL tokens"`

---

## Task 3: Single-stage agent context builder — fold the Intern (Section A, core)

Create the shared context builder that absorbs the Intern's two helpers (skill-catalog injection + `OnchainPanel` rendering).

**Files:**
- Create: `crates/xvision-engine/src/agents/context.rs`
- Modify: `crates/xvision-engine/src/agents/mod.rs` (add `pub mod context;`)
- Reference (read, do not yet delete): `crates/xvision-intern/src/prompt.rs:32,68,132`
- Test: inline `#[cfg(test)]` in `context.rs`

- [ ] **Step 1: Read** `crates/xvision-intern/src/prompt.rs:32-160` (the whole `build_intern_prompt`), and `crates/xvision-core/src/market.rs:60-95` (`OnchainPanel`, `SkillRef`). Identify the exact rendering of the `# Loaded skill catalogs` block (:68) and the onchain panel block (:132).

- [ ] **Step 2: Write the failing test** in `context.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use xvision_core::market::{OnchainPanel, SkillRef};

    fn byreal_skill() -> SkillRef {
        SkillRef { catalog: "byreal".into(), name: "perp-risk-shapes".into(),
                   summary: "funding/OI risk shapes for perps".into() }
    }

    #[test]
    fn context_injects_skill_catalog_section() {
        let panel = OnchainPanel::default();
        let out = render_agent_context(&[byreal_skill()], &panel);
        assert!(out.contains("Loaded skill catalogs"));
        assert!(out.contains("byreal"));
        assert!(out.contains("perp-risk-shapes"));
    }

    #[test]
    fn context_renders_perps_panel_when_present() {
        let mut panel = OnchainPanel::default();
        panel.funding_rate_8h = Some(0.0003);
        panel.open_interest_usd = Some(12_000_000.0);
        let out = render_agent_context(&[], &panel);
        assert!(out.contains("funding"));
        assert!(out.contains("12000000") || out.contains("12,000,000") || out.contains("12.0M"));
    }

    #[test]
    fn context_omits_panel_when_all_none() {
        let out = render_agent_context(&[], &OnchainPanel::default());
        assert!(!out.contains("Onchain"));
    }
}
```

- [ ] **Step 3: Run → FAIL.** `scripts/cargo test -p xvision-engine context::tests`

- [ ] **Step 4: Implement** `render_agent_context(skills: &[SkillRef], panel: &OnchainPanel) -> String` by porting the two blocks from `prompt.rs:68` and `prompt.rs:132` verbatim (same wording/format the model already expects). Return empty string for the panel when all fields are `None`. Add `pub mod context;` to `agents/mod.rs`.

- [ ] **Step 5: Run → PASS.**

- [ ] **Step 6: Commit.** `git commit -am "feat(engine): agents/context.rs — folds Intern skill+perps rendering into single agent"`

---

## Task 4: Wire context into `build_decision_seed` + retire the Intern stage (Section A, integration)

**Files:**
- Modify: `crates/xvision-engine/src/eval/executor/backtest.rs` (`DecisionSeedInput` :5108; `build_decision_seed` :5135-5160)
- Modify: `crates/xvision-engine/src/agent/dispatch_capability.rs` (:323-330 dispatch arm; remove `dispatch_intern_stub` :572)
- Modify: `crates/xvision-eval/src/baselines/trader_arm.rs:159-205` (collapse Intern→Trader)
- Test: inline in `backtest.rs`

- [ ] **Step 1: Read** `backtest.rs:5100-5165`, `dispatch_capability.rs:320-340` and `:560-590`, and `trader_arm.rs:150-210`.

- [ ] **Step 2: Write the failing test** in `backtest.rs` tests:

```rust
#[test]
fn decision_seed_includes_skills_and_perps_context() {
    let input = DecisionSeedInput { /* minimal fixture per existing tests */
        skills: &[/* byreal SkillRef */],
        funding_rate: Some(0.0002), open_interest: Some(9_000_000.0), mark_basis: Some(0.0005),
        ../* existing fixture */ };
    let seed = build_decision_seed(&input);
    let md = seed.get("market_data").unwrap();
    assert!(md.get("loaded_skills").is_some());
    assert!(md.get("funding_rate").is_some());
}
```

- [ ] **Step 3: Run → FAIL.**

- [ ] **Step 4: Implement.**
  - Extend `DecisionSeedInput` (`backtest.rs:5108`) with `skills: &'a [SkillRef]`, `funding_rate: Option<f64>`, `open_interest: Option<f64>`, `mark_basis: Option<f64>`.
  - In `build_decision_seed`, under the `market_data` object, add `loaded_skills` (from `render_agent_context` or a structured list) and the three perps scalars (omit keys when `None`).
  - In `dispatch_capability.rs`: route the `Capability::Intern` arm to the trader/single-agent path (or return a clear `Err`/validation message); delete `dispatch_intern_stub`. Add a comment referencing this plan + spec for why the stage is retired.
  - In `trader_arm.rs`: remove the separate Intern `brief()` call; build the agent context via `render_agent_context` and pass `skills` through to the single trader call.

- [ ] **Step 5: Run.** `scripts/cargo test -p xvision-engine -p xvision-eval` → PASS (existing spot tests stay green with perps `None`).

- [ ] **Step 6: Commit.** `git commit -am "feat(engine): single-stage agent — retire Intern stage, thread skills+perps into decision seed"`

---

## Task 5: `ByrealPerpsApi` subprocess seam (Section B, part 1)

**Files:**
- Create: `crates/xvision-execution/src/byreal.rs`
- Modify: `crates/xvision-execution/src/lib.rs` (add `pub mod byreal;` + re-exports, mirror :8-21)
- Reference (read): `crates/xvision-execution/src/orderly.rs:257-294` (`OrderlyApi` trait + `MockOrderlyApi`), `probes/m0-byreal/src/main.rs:1-154` (envelope + npx invocation)
- Test: inline in `byreal.rs`

- [ ] **Step 1: Read** the two reference files above. Note the `{success, meta, data}` envelope (`main.rs`) and the `OrderlyApi` 7-method shape.

- [ ] **Step 2: Write the failing test** (with a mock, no real subprocess):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn mock_api_returns_account() {
        let api = MockByrealApi::with_account(ByrealAccount { equity_usd: 1000.0 });
        let acct = api.account_info().await.unwrap();
        assert_eq!(acct.equity_usd, 1000.0);
    }
}
```

- [ ] **Step 3: Run → FAIL.** `scripts/cargo test -p xvision-execution byreal::`

- [ ] **Step 4: Implement** the seam:

```rust
#[async_trait::async_trait]
pub trait ByrealPerpsApi: Send + Sync {
    async fn account_info(&self) -> Result<ByrealAccount, ExecutorError>;
    async fn order_market(&self, symbol: &str, side: Side, qty: f64, reduce_only: bool)
        -> Result<ByrealOrderAck, ExecutorError>;
    async fn order_limit(&self, symbol: &str, side: Side, qty: f64, price: f64)
        -> Result<ByrealOrderAck, ExecutorError>;
    async fn position_list(&self) -> Result<Vec<ByrealPosition>, ExecutorError>;
    async fn close_market(&self, symbol: &str) -> Result<ByrealOrderAck, ExecutorError>;
    async fn close_all(&self) -> Result<(), ExecutorError>;
}
```
Define POD structs `ByrealAccount`, `ByrealOrderAck { venue_order_id, avg_fill_price, filled_qty }`, `ByrealPosition { symbol, qty_signed, avg_open_price, mark_price, leverage: Option<f64>, liq_price: Option<f64>, funding_paid_usd: f64, unrealized_pnl_usd: f64 }`. Add `MockByrealApi` (canned values) and `SubprocessByrealApi` using `tokio::process::Command` with a 60s timeout, base command `npx -y @byreal-io/byreal-perps-cli@latest <verb> -o json`, deserializing the `{success, meta, data}` envelope into the structs. Map non-zero exit / `success=false` → `ExecutorError::Rejected`/`Internal`, timeout → `ExecutorError::Timeout`.

- [ ] **Step 5: Run → PASS.**

- [ ] **Step 6: Commit.** `git commit -am "feat(execution): ByrealPerpsApi subprocess seam + mock"`

---

## Task 6: `ByrealPerpsExecutor` implements `Executor` (Section B, part 2)

**Files:**
- Modify: `crates/xvision-execution/src/byreal.rs`
- Reference (read): `crates/xvision-execution/src/executor.rs:23-86` (trait + `ExecutionReceipt`/`ExecutorError`), `orderly.rs:776-1035` (`Executor` impl flow)
- Test: inline in `byreal.rs`

- [ ] **Step 1: Read** `executor.rs:23-86` and skim `orderly.rs:776-1035` for the submit→poll→receipt flow and the `close_position` reduce-only pattern.

- [ ] **Step 2: Write failing tests** with `MockByrealApi`:

```rust
#[tokio::test]
async fn submit_vetoed_is_not_actionable() {
    let exec = ByrealPerpsExecutor::new(MockByrealApi::default());
    let d = RiskDecision::vetoed_fixture(); // use existing test ctor
    let err = exec.submit(&d).await.unwrap_err();
    assert!(matches!(err, ExecutorError::NotActionable));
}

#[tokio::test]
async fn submit_market_returns_receipt_with_byreal_venue() {
    let api = MockByrealApi::with_fill("BTC", 0.01, 60_000.0, "ord-123");
    let exec = ByrealPerpsExecutor::new(api);
    let d = RiskDecision::approved_long_fixture("BTC"); // existing ctor
    let r = exec.submit(&d).await.unwrap();
    assert_eq!(r.venue, "byreal");
    assert_eq!(r.venue_order_id, "ord-123");
}

#[tokio::test]
async fn close_position_submits_reduce_only_opposite() {
    let api = MockByrealApi::with_long("BTC", 0.01);
    let exec = ByrealPerpsExecutor::new(api);
    let r = exec.close_position("BTC".into()).await.unwrap();
    assert_eq!(r.venue, "byreal");
}
```

- [ ] **Step 3: Run → FAIL.**

- [ ] **Step 4: Implement** `impl<A: ByrealPerpsApi> Executor for ByrealPerpsExecutor<A>`:
  - `submit`: return `ExecutorError::NotActionable` on `RiskDecision::Vetoed`; map asset → HL symbol (`byreal_symbol_for`); `account_info` + `position_list`; compute base qty from decision notional and mark price; `order_market`; build `ExecutionReceipt { venue: "byreal", venue_order_id, avg_fill_price, ... }`.
  - `close_position`: `position_list` → find open → `close_market`.
  - `portfolio`: `account_info` + `position_list` → `PortfolioState`.
  - Add `from_env()` reading `BYREAL_*` (mirror `orderly.rs:659-703`).

- [ ] **Step 5: Run → PASS.** `scripts/cargo test -p xvision-execution byreal::`

- [ ] **Step 6: Commit.** `git commit -am "feat(execution): ByrealPerpsExecutor implements Executor (venue=byreal -> Hyperliquid)"`

---

## Task 7: Public perps feed (Section C, live source)

**Files:**
- Create: `crates/xvision-data/src/perp_feed.rs`
- Modify: `crates/xvision-data/src/lib.rs` (export), and the live source assembly that fills `OnchainPanel` / `Bar` perps fields
- Reference (read): `crates/xvision-data/src/alpaca_live_poll.rs` (polling source pattern), `crates/xvision-core/src/market.rs:60-95` (`OnchainPanel`)
- Test: inline with a mock HTTP client

- [ ] **Step 1: Read** `alpaca_live_poll.rs` for the established polling-source shape and how a source feeds the bar stream.

- [ ] **Step 2: Write a failing parse test** (no network — feed a canned JSON body):

```rust
#[test]
fn parses_hyperliquid_funding_and_oi() {
    let body = r#"{ "funding": "0.0000125", "openInterest": "9000000", "markPx": "60000.5" }"#;
    let snap = parse_perp_snapshot(body).unwrap();
    assert!((snap.funding_rate - 0.0000125).abs() < 1e-9);
    assert_eq!(snap.open_interest, 9_000_000.0);
    assert_eq!(snap.mark_price, 60_000.5);
}
```

- [ ] **Step 3: Run → FAIL.** `scripts/cargo test -p xvision-data perp_feed`

- [ ] **Step 4: Implement** `PerpSnapshot { funding_rate, open_interest, mark_price }`, `parse_perp_snapshot(&str)`, and an async `fetch_perp_snapshot(symbol)` hitting the public Hyperliquid endpoint (no auth). Provide a helper `apply_to_onchain(&PerpSnapshot, &mut OnchainPanel)` setting `funding_rate_8h`, `open_interest_usd` (and `mark`/basis where the live `Bar` is assembled). Wire the live assembly so the agent context (Task 4) and `Bar` (Task 1) receive real values; backtest path passes `None`.

- [ ] **Step 5: Run → PASS.**

- [ ] **Step 6: Commit.** `git commit -am "feat(data): public Hyperliquid perps feed -> OnchainPanel + Bar"`

---

## Task 8: Capability + skill badge UI (Section F)

**Files:**
- Create: `frontend/web/src/components/strategy/CapabilityBadge.tsx`
- Modify: `frontend/web/src/routes/strategies.tsx` (render alongside `TagList` ~:507/:628)
- Test: `frontend/web/src/components/strategy/CapabilityBadge.test.tsx`

- [ ] **Step 1: Read** `strategies.tsx:415-520` (search-terms + row render) and `api/strategies.ts:26,63` (`capabilities`, `Capability` type). Note dark-mode border rule (no full-white borders) from `CLAUDE.md`.

- [ ] **Step 2: Write the failing test:**

```tsx
import { render, screen } from "@testing-library/react";
import { CapabilityBadge } from "./CapabilityBadge";

test("highlights byreal skill capability", () => {
  render(<CapabilityBadge capabilities={["trader"]} skills={["byreal"]} />);
  expect(screen.getByText(/byreal/i)).toBeInTheDocument();
  expect(screen.getByTestId("cap-badge-byreal")).toHaveClass("ring-1"); // highlighted treatment
});

test("renders nothing when no capabilities or skills", () => {
  const { container } = render(<CapabilityBadge capabilities={[]} skills={[]} />);
  expect(container).toBeEmptyDOMElement();
});
```

- [ ] **Step 3: Run → FAIL.** `cd frontend/web && npm test -- CapabilityBadge`

- [ ] **Step 4: Implement** `CapabilityBadge` rendering capability chips + a highlighted chip for the `byreal` skill (use `border-border`/theme tokens + low-opacity `dark:` variants per project rule; `data-testid="cap-badge-<name>"`). Render it in the strategy row next to `TagList`, reading `row.capabilities` and the new loaded-skills field.

- [ ] **Step 5: Run → PASS.**

- [ ] **Step 6: Commit.** `git commit -am "feat(web): highlighted capability/skill badge (byreal) in strategy list"`

---

## Phase 1 verification gate

- [ ] `scripts/cargo build --workspace && scripts/cargo test --workspace` → PASS
- [ ] `cd frontend/web && npm run test && npm run build` → PASS
- [ ] Live smoke (manual, optional but recommended for the demo): set `BYREAL_*`, run one tiny decision through `ByrealPerpsExecutor`, confirm an `ExecutionReceipt` with `venue=byreal` and a real `venue_order_id`. Capture for the verifiability demo.

---

# PHASE 2 — HIGH-VALUE (D already done as Tasks 1–2; E here)

## Task 9: Perps fields on positions (Section E, core)

**Files:**
- Modify: `crates/xvision-core/src/trading.rs` (`OpenPosition` :397)
- Modify: `crates/xvision-execution/src/orderly.rs:434` (`PositionEntry` — parse `est_liq_price`, `imr_with_orders`) and the `OrderlyPosition` mapping :218
- Test: inline in `trading.rs` + `orderly.rs`

- [ ] **Step 1: Read** `trading.rs:390-440` and `orderly.rs:210-240,430-470`.

- [ ] **Step 2: Write failing tests:**

```rust
// trading.rs
#[test]
fn open_position_defaults_perps_fields() {
    let p = OpenPosition::flat_fixture("BTC");
    assert_eq!(p.leverage, None);
    assert_eq!(p.liquidation_price, None);
    assert_eq!(p.funding_paid_usd, 0.0);
}
```

- [ ] **Step 3: Run → FAIL.**

- [ ] **Step 4: Implement.** Add to `OpenPosition`: `leverage: Option<f64>`, `liquidation_price: Option<f64>`, `funding_paid_usd: f64`, `notional_usd: f64`, `unrealized_pnl_usd: f64` (default `None`/`0.0` in all constructors). In `orderly.rs`, parse Orderly's `est_liq_price`/`imr_with_orders` into `OrderlyPosition` and forward. (Byreal already carries these via `ByrealPosition` from Task 5.)

- [ ] **Step 5: Run → PASS.** `scripts/cargo test -p xvision-core -p xvision-execution`

- [ ] **Step 6: Commit.** `git commit -am "feat(core): perps fields on OpenPosition; parse liq/leverage from venues"`

---

## Task 10: Positions UI perps columns (Section E, frontend)

**Files:**
- Modify: `frontend/web/src/api/live.ts:15` (`VenuePosition`), `frontend/web/src/features/.../live-account.ts:150` (`PositionRow`), `LivePositionsTable.tsx`
- Test: `LivePositionsTable.test.tsx`

- [ ] **Step 1: Read** the three files + the existing `LivePositionsTable.test.tsx`.

- [ ] **Step 2: Write failing test** asserting new columns render (`leverage`, `liq_price`, `funding_paid`) when present and degrade gracefully (`—`) when absent.

- [ ] **Step 3: Run → FAIL.**

- [ ] **Step 4: Implement.** Add optional `leverage`, `liq_price`, `funding_paid` to `VenuePosition`/`PositionRow`; add the three columns to `LivePositionsTable` with `—` fallback.

- [ ] **Step 5: Run → PASS.** `npm test -- LivePositionsTable`

- [ ] **Step 6: Commit.** `git commit -am "feat(web): leverage/liq/funding columns in live positions table"`

---

## Task 11: DB migration for persisted position perps fields (Section E, if applicable)

- [ ] **Step 1:** Read the `cycle-migration` skill (REQUIRED before any `*.sql` under `crates/`). Grep for a persisted positions table: `rg -l "open_position|positions" crates/*/migrations`.
- [ ] **Step 2:** If positions are persisted, add an additive migration (new nullable columns `leverage`, `liquidation_price`, `funding_paid_usd`, `notional_usd`, `unrealized_pnl_usd`) in the correct migrations dir per the skill; update the SQLx query structs. If positions are **not** persisted (live-derived only), record that finding in the commit message and skip. 
- [ ] **Step 3:** `scripts/cargo test -p <crate>` → PASS. Commit.

---

# PHASE 3 — STRETCH (priority order 1→2→3→4; each independently shippable)

> Lighter task outlines — implement TDD-first like Phase 1 (failing test → impl → pass → commit). Do only as time/tokens allow.

## Stretch 1: Funding-aware sizing / carry guard (perps-mgmt)
- **Where:** risk gate (locate via `rg "RiskDecision" crates/xvision-*/src | rg -i risk`). Add a modifier that reads `OnchainPanel.funding_rate_8h` (now populated by Task 7) and: dampens/blocks entries paying punitive funding beyond a configurable threshold; optionally up-sizes when funding favors the position (carry).
- **Tests:** funding above threshold → decision modified/vetoed; favorable funding → unchanged or up-sized; missing funding data → no-op (fail safe).
- **Commit:** `feat(risk): funding-aware sizing / carry guard`.

## Stretch 2: Liquidation-distance guard (perps-mgmt)
- **Depends on:** Task 9 (`leverage`, `liquidation_price`).
- **Where:** risk gate + a monitor. Veto/modify any decision whose computed liq price sits within `X%` of mark at chosen leverage; monitor auto-deleverages/closes on breach.
- **Tests:** liq within X% → veto/downsize; safe distance → pass; monitor triggers close when breached.
- **Commit:** `feat(risk): liquidation-distance guard + monitor`.

## Stretch 3: Perps-native order semantics (perps-mgmt)
- **Where:** `RiskDecision` + `Executor` impls. Add `leverage`, `reduce_only`, `post_only`, native bracket TP/SL, trailing stop, funding-time-aware exit. Generalize Orderly's best-effort brackets (`orderly.rs:~900`) and mirror in `byreal.rs`.
- **Tests:** reduce_only/post_only propagate to the API mock; trailing stop recomputes; bracket placed after fill.
- **Commit:** `feat(execution): perps-native order semantics (leverage, reduce/post-only, trailing, funding exit)`.

## Stretch 4: Byreal CLMM LP action (original stretch)
- **Where:** new thin wrapper invoking the vendored `.claude/skills/byreal/` CLMM CLI (`@byreal-io/byreal-cli`, Solana) for `open → rebalance → close` of a tiny LP position; surface in the run trace.
- **Tests:** mock subprocess returns position id; trace records open/rebalance/close.
- **Commit:** `feat(byreal): CLMM LP open/rebalance/close action surfaced in trace`.

---

## Finish

- [ ] Full workspace + frontend green (`scripts/cargo test --workspace`; `npm test && npm run build`).
- [ ] Update `CLAUDE.md` Docker/terminology notes only if a public surface changed (e.g. new env vars `BYREAL_*`).
- [ ] Run `/self-reflect` (per project workflow) before PR.
- [ ] Open PR from `feat/byreal-perps-single-stage` with the spec + live-smoke evidence (receipt with `venue=byreal`).
- [ ] **Confirm with Byreal judges** (Ask Question tab) that Perps-CLI execution counts as Byreal integration despite routing to Hyperliquid — before judging.

---

## Self-review notes (author)

- **Spec coverage:** A→Tasks 3–4; B→Tasks 5–6; C→Tasks 1,7; D→Tasks 1–2; E→Tasks 9–11; F→Task 8; stretch 1–4→Phase 3. All spec sections mapped.
- **Type consistency:** `ByrealPosition` fields (Task 5) reused by Task 6 receipts and align with `OpenPosition` additions (Task 9). `IndicatorName` variants (Task 2) consumed by `Bar`/engine (Task 1). `render_agent_context` (Task 3) consumed by Task 4.
- **Known anchor risk:** line numbers (`:5108`, `:900`, `:507`) are from recon and may drift; every modify-task instructs the implementer to Read the cited range first and match the real code. Constructors named `*_fixture`/`flat_fixture` assume existing test ctors — if absent, build the struct literally from its real fields (Read step covers this).
