# Byreal Perps + Perps-Aware Single-Stage Agent — Design Spec

**Date:** 2026-06-12
**Status:** Draft (approved for plan authoring)
**Track motivation:** The Byreal hackathon track is our weakest. Today Byreal is
research-only — a vendored CLMM Agent Skills catalog loaded as context and a
*verified-but-inactive* Perps CLI probe (`probes/m0-byreal/`). Execution pivoted
to Orderly (`decisions/0006-executor-choice.md`). This spec converts Byreal from
"surface-level integration" into a real perpetual-futures executor and threads
perps data through the filter, agent, and positions surfaces — the exact
rubric language for the heavily-weighted dimensions.

## Goals

1. **Salvage / fold the Intern.** Collapse the legacy two-stage Intern→Trader
   model into a **single-stage agent**. Preserve the Intern's useful function
   (skill-catalog injection + onchain/perps context rendering) by folding it into
   the single agent's context builder. Retire the separate Intern stage.
2. **Byreal as a real perps executor.** Activate the proven Perps CLI as a live
   `Executor`, routing to Hyperliquid. One adapter covers both "Byreal" and
   "Hype".
3. **Perps-aware everywhere.** Filter, agent context, and positions all consume
   real perps data (funding rate, open interest, mark/index basis,
   leverage, liquidation price, funding paid).
4. **Highlight the capability.** Surface the Byreal perps skill/capability as a
   highlighted badge in the strategy + agent UI (today capabilities are
   searchable-only).

## Non-goals / Deferred

- **Perps backtesting.** No historical funding/OI feed and no in-engine perps
  simulator in this scope. The eval/backtest path leaves perps fields `None`.
  The seam (optional `Bar` fields, neutral defaults) is left ready so a synthetic
  perps simulator can be added later without touching call sites.
- **Native Hyperliquid executor.** Out of scope — the single Byreal Perps CLI
  adapter (which executes on Hyperliquid) covers the "Hype" requirement. A future
  native HL executor can register against the same unchanged `Executor` trait.
- **Cross-venue portfolio netting** (Byreal/HL + Orderly unified book). Future.

## Background facts (grounding)

- **The Intern is not dead code.** `crates/xvision-intern/` is a production crate
  (`AnthropicIntern`, `OpenAICompatIntern`, full test suite) used today by
  `xvision-eval/src/baselines/trader_arm.rs:159` in the Stage 1 → Stage 2 pipeline.
  What *is* stubbed is the graph-capability path: `Capability::Intern` routes to
  `dispatch_intern_stub()` (`crates/xvision-engine/src/agent/dispatch_capability.rs:572`)
  returning `"stub intern"` with zero tokens.
- **The skill-injection seam exists but is unused at runtime.**
  `build_intern_prompt(state, skills, opts)` (`crates/xvision-intern/src/prompt.rs:32`)
  accepts `&[SkillRef]` and renders a `# Loaded skill catalogs` section at
  `prompt.rs:68`. Every live caller passes `&[]` (e.g. `trader_arm.rs:179`), so
  Byreal skills are never injected.
- **`OnchainPanel` already has the perps schema.** `crates/xvision-core/src/market.rs:66`
  defines `funding_rate_8h`, `open_interest_usd`, `long_short_ratio`,
  `liquidations_24h_usd`, etc. It is always `OnchainPanel::default()` in every
  live path — never populated. The Intern prompt renders it when present
  (`prompt.rs:132`).
- **Filter `IndicatorName` is a closed catalog** (`crates/xvision-filters/src/types.rs:177`)
  with no perps fields. The runtime evaluator consumes `Bar { open, high, low,
  close, volume, timestamp }` (`crates/xvision-filters/src/indicators.rs:54`).
- **Positions already support shorts** (`Direction::Short`, `OpenPosition` at
  `crates/xvision-core/src/trading.rs:397`) but lack leverage, liquidation price,
  funding paid, notional, and an explicit unrealized-PnL field.
- **The Executor trait is venue-agnostic** (`crates/xvision-execution/src/executor.rs:72`):
  `submit`, `close_position`, `portfolio`. `OrderlyExecutor` (`orderly.rs`,
  ~1,979 lines) is the reference impl, with an internal `OrderlyApi` trait
  (`orderly.rs:257`) as a mockable seam (`MockOrderlyApi`).
- **The Byreal Perps CLI is verified.** `probes/m0-byreal/src/main.rs:113`
  confirmed primitives `account.info`, `order.market`, `order.limit`,
  `order.cancel`, `position.list`, and `close-market`/`close-limit`/`close-all`
  via `npx -y @byreal-io/byreal-perps-cli@latest`. It executes on Hyperliquid.
- **`.claude/skills/byreal/`** is the **CLMM (Solana LP)** Agent Skills catalog
  (`@byreal-io/byreal-cli`) — distinct from the perps CLI. Used by stretch item 1.

---

## Architecture

### A. Fold the Intern into the single agent ("salvage")

**Decision: retire the Intern stage entirely** (not switchable).

- Extract the two reusable helpers from `xvision-intern` into a new shared
  module `crates/xvision-engine/src/agents/context.rs`:
  - **Skill-catalog injection** (logic from `prompt.rs:68`) — renders loaded
    skill summaries into the agent prompt.
  - **Onchain/perps panel rendering** (logic from `prompt.rs:132`) — renders
    `OnchainPanel` fields into the prompt.
- Wire these into `build_decision_seed` (`crates/xvision-engine/src/eval/executor/backtest.rs:5135`)
  so the single agent's `market_data` JSON carries: loaded-skill summaries +
  populated perps/onchain context. Extend `DecisionSeedInput` (`backtest.rs:5108`)
  with `skills: &[SkillRef]` and the perps scalars.
- **Retire the Intern stage:**
  - Remove `dispatch_intern_stub` and the `Capability::Intern` *stage* dispatch
    arm (`dispatch_capability.rs:327,572`). Keep the `Capability` enum variant
    deserializable for back-compat with stored slots, but route it to the single
    trader path (or reject at validation with a migration note).
  - Collapse `xvision-eval/src/baselines/trader_arm.rs:159-205` from Intern→Trader
    into a single agent call that uses the folded context.
  - The `xvision-intern` crate's backends (`AnthropicIntern`,
    `OpenAICompatIntern`) remain available but are no longer invoked as a separate
    stage; the moved prompt helpers are deleted from `prompt.rs` once `context.rs`
    owns them. `xvn intern` CLI subcommands (`crates/xvision-cli/src/commands/intern.rs`)
    are updated to call the single-agent context builder (or marked deprecated).
- The single agent **declares its loaded skill catalogs** as a first-class field
  (e.g. on the strategy/agent slot record), feeding both the prompt and the UI
  badge (Section F).

**Interface:** `agents::context::build_agent_context(snapshot, skills, perps, opts)
-> AgentContext` — one entry point, unit-testable, no separate stage.

### B. ByrealPerpsExecutor (one venue, covers Byreal + Hype)

- New `crates/xvision-execution/src/byreal.rs`, mirroring `orderly.rs`'s shape
  but far smaller (subprocess, not signed HTTP).
- Internal `ByrealPerpsApi` trait — the **mockable subprocess seam**, parallel to
  `OrderlyApi` (`orderly.rs:257`). Methods wrap `npx @byreal-io/byreal-perps-cli`
  invocations (pattern from `probes/m0-byreal/src/main.rs`): `account_info`,
  `order_market`, `order_limit`, `order_cancel`, `position_list`,
  `close_market` / `close_all`, optionally `signal_scan`.
  - Real impl: `SubprocessByrealApi` using `tokio::process::Command` with a
    bounded timeout (probe uses 60s) and the `{success, meta, data}` envelope
    deserialization.
  - Test impl: `MockByrealApi` with canned envelopes.
- `ByrealPerpsExecutor<A: ByrealPerpsApi>` implements the existing `Executor`
  trait **unchanged**:
  - `submit(&RiskDecision)` — bail on `Vetoed`; map asset → HL market symbol;
    fetch account + positions; compute base qty from notional; place market
    order; poll for fill; build `ExecutionReceipt { venue: "byreal", ... }`.
    Best-effort bracket TP/SL (generalized in stretch item 4).
  - `close_position(asset)` — `position.list` → opposing `close-market`
    (reduce-only).
  - `portfolio()` — `account.info` + `position.list` → `PortfolioState`.
- Constructor `ByrealPerpsExecutor::from_env()` reading `BYREAL_*` env vars,
  registered alongside Orderly; venue selection by config (which `Arc<dyn
  Executor>` is injected at engine construction).

### C. Real perps data feed (live-only)

- New `crates/xvision-data/src/perp_feed.rs` — polls **public, no-auth**
  Hyperliquid endpoints for funding rate, open interest, and mark/index price at
  the bar cadence (parallel to the Alpaca sources in `xvision-data`).
- Populates `OnchainPanel` (`market.rs:66`) — already the correct schema — and
  threads through `MarketSnapshot` into the folded agent context (Section A).
- Extends `Bar` (`crates/xvision-filters/src/indicators.rs:54`) with
  `funding_rate: Option<f64>`, `open_interest: Option<f64>`, `mark_price:
  Option<f64>`, populated from the live source at OHLCV cadence.
- Backtest/eval path leaves these `None` (perps backtest deferred). The folded
  context and filter evaluator treat `None` as "not available" — conditions on
  perps indicators simply don't fire / the panel section is omitted.

### D. Filter consumes perps

- New `IndicatorName` variants (`types.rs:177`): `FundingRate`, `OpenInterest`,
  `MarkPrice`, `MarkIndexBasis`, `LongShortRatio`. Non-windowed — `has_period =
  false`, read directly like `Open`/`Close`.
- Update enum method arms (`has_period`, `dsl_prefix`, `period_bounds` at
  `types.rs:251-445`), DSL tokens in `parse_dsl` (`types.rs:491`:
  `"funding_rate"`, `"open_interest"`, `"mark_price"`, `"mark_index_basis"`,
  `"long_short_ratio"`).
- Evaluator: store latest perps values in `IndicatorEngine` and return them from
  `IndicatorEngine::value` (`indicators.rs:321`) for the new variants (read
  `last_*` fields, no rolling state).
- Frontend: the inline composer (`InlineFilterComposer.tsx` / `firingPredicate.ts`)
  offers the new tokens; `FilterCard.tsx` (free-form JSON) round-trips them
  automatically once the server enum accepts them.

### E. Positions consume perps

- Extend `OpenPosition` (`trading.rs:397`): add `leverage: Option<f64>`,
  `liquidation_price: Option<f64>`, `funding_paid_usd: f64`, `notional_usd: f64`,
  `unrealized_pnl_usd: f64`. (`Direction::Short` already exists.)
- Venue position structs: parse liq price + IMR/leverage from the Byreal/HL
  `position.list` envelope (and, for parity, Orderly's `est_liq_price` /
  `imr_with_orders` in `orderly.rs:434` `PositionEntry`).
- Frontend: add `leverage`, `liq_price`, `funding_paid` columns to
  `LivePositionsTable.tsx`; extend `VenuePosition` (`frontend/web/src/api/live.ts:15`)
  and `PositionRow` (`live-account.ts:150`).
- DB: if positions are persisted (check `xvision-observability` migrations), add
  a migration for the new columns following the `cycle-migration` skill
  conventions.

### F. Capability + skill badge UI ("highlight")

- Render a highlighted capability/skill chip in the strategy list and agent card,
  reading `row.capabilities` (`frontend/web/src/api/strategies.ts:26`) and the new
  loaded-skills field. Byreal perps gets a distinct highlighted treatment.
- Today capabilities are pushed into search terms only (`strategies.tsx:420`) and
  never rendered; `TagList` (`strategies.tsx:628`) renders `tags`, a separate
  array. Add a `CapabilityBadge` component rendered alongside tags.

---

## Stretch (in priority order)

> The requested LP item plus 3 perps-management items. All perps-management items
> are reusable for the Hyperliquid path and feed the rubric's autonomy +
> verifiability dimensions.

1. **Funding-aware sizing / carry guard** *(perps-mgmt)* — risk gate factors
   funding rate into entry: avoid punitive funding, optionally size to harvest
   positive funding (carry). Implemented as a `RiskDecision` modifier consuming
   the Section C feed. *(Recommended first stretch.)*
2. **Liquidation-distance guard** *(perps-mgmt)* — risk gate vetoes/modifies any
   decision whose computed liquidation price sits within X% of mark at the chosen
   leverage; a continuous monitor auto-deleverages/closes on breach. Depends on
   Section E fields.
3. **Perps-native order semantics** *(perps-mgmt)* — extend `RiskDecision` and the
   executor to carry `leverage`, `reduce_only`, `post_only`, plus native bracket
   TP/SL + **trailing stop** + funding-time-aware exit (generalizing Orderly's
   best-effort brackets at `orderly.rs:~900`).
4. **Byreal CLMM LP action** *(original stretch)* — one `open → rebalance → close`
   of a tiny LP position via the vendored `.claude/skills/byreal/` CLMM CLI,
   surfaced in the run trace. Checks the rubric "LP management" box. Lowest
   priority (separate CLI, Solana, less reusable).

---

## Data flow

```
Hyperliquid public API ──> perp_feed.rs ──┬─> OnchainPanel ──> agents/context.rs ──> single agent prompt
                                          ├─> Bar.{funding_rate,open_interest,mark_price} ──> filter engine
                                          └─> (live mark) ──> positions

single agent ──> TraderDecision ──> risk gate (+ funding/liq guards) ──> RiskDecision
                                                                            │
                                                                            ▼
                                          ByrealPerpsExecutor.submit() ──> npx byreal-perps-cli ──> Hyperliquid
                                                                            │
                                                                            ▼
                                                          ExecutionReceipt { venue: "byreal", venue_order_id }
                                                                            │
                                                                            ▼
                                                          positions (leverage, liq_price, funding_paid) ──> UI
```

## Error handling

- Executor maps subprocess failures to existing `ExecutorError` variants:
  non-zero exit / bad envelope → `Internal`; network/timeout → `Timeout` /
  `Network`; CLI rejection → `Rejected`; veto → `NotActionable`.
- Perps feed failures are non-fatal: on poll failure the perps fields stay
  `None`/stale and the agent/filter degrade gracefully (panel omitted, perps
  conditions don't fire). The feed never blocks a decision cycle.
- Stretch guards (funding/liq) fail safe: if required perps data is missing, the
  guard is a no-op rather than a spurious veto.

## Testing

- **Unit:** `MockByrealApi` mirrors `MockOrderlyApi` — test `submit` /
  `close_position` / `portfolio` against canned `{success, meta, data}` envelopes,
  including the `close-market`/`close-all` split. Filter perps variants and
  position-field changes are pure-unit testable. Folded `agents/context.rs` gets a
  fixture test (Byreal `SkillRef` + populated `OnchainPanel`) replacing the
  current `prompt.rs:244` fixture.
- **Integration:** one live testnet **smoke** — a few small Byreal/Hyperliquid
  trades producing `ExecutionReceipt`s with real venue order IDs, surfaced in the
  live run UI for the "verifiable alpha" demo.
- **No perps backtest** (deferred). Existing spot eval/backtest suites must stay
  green with perps fields defaulting to `None`.
- Coverage gate per `.coverage-thresholds.json` applies to new Rust modules.

## Rollout / cut line

Given limited time and tokens:

- **Must-do:** A (fold intern) → B (Byreal executor) → C (feed) → F (badge).
- **High-value:** D (filter) → E (positions).
- **Stretch order:** 1 (funding-aware) → 2 (liq guard) → 3 (order semantics) →
  4 (LP).

This plausibly moves the track from "Basic Byreal integration present" to "Deep,
purposeful Byreal integration; verified on-chain."

## Open risk to confirm with judges

Confirm (Ask Question tab) that **Perps CLI execution counts as Byreal
integration even though it routes to Hyperliquid** — the CLI is Byreal's product,
but trades settle on HL. Better to confirm before judging than after.
