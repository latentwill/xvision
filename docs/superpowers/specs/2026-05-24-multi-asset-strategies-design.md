# Multi-Asset Strategies — design

**Date:** 2026-05-24
**Status:** Draft for operator review (spec only — no implementation until approved).
**Surface:** `crates/xvision-core/src/{trading,...}`, `crates/xvision-engine/src/{eval/scenario.rs, eval/executor/**, agent/{signal_cache,dispatch_capability,briefing,filter_dispatch}.rs, strategies/**, api/{eval,scenario,strategy}.rs}`, `crates/xvision-cli/src/commands/{strategy,scenario,eval/**}.rs`, `frontend/web/src/{components/scenario,routes/authoring,routes/eval-*}.tsx`.
**Branch:** `feat/multi-asset` (worktree `.worktrees/multi-asset`, off clean `main`).

## Guiding principle (load-bearing)

**When in doubt between encoding behavior in harness code vs. Strategy config, prefer config.** The harness is the substrate prompts run on; anything that could plausibly vary across strategies belongs as a `Strategy` field with a sensible default, not as a hardcoded loop shape or struct invariant.

Why this matters here specifically: xvision is heading toward DSPy-style prompt optimization, where strategies are *data an optimizer searches over*. Every pipeline-shape decision baked into harness code is a hypothesis the optimizer cannot reach — and experiments discover the wall only by silently failing to express it (basket bets, pairs trades, cross-asset regime calls, capital tiers). This spec deliberately pushes shape decisions into `Strategy` data with v1 defaults, implementing only the default arm now.

## Goal

Make multi-asset a **strategy capability**, end to end:

- A strategy declares the **universe** of assets it trades (`asset_universe`, already exists).
- **Scenarios are asset-free** — any asset can run against any scenario.
- The asset is a **minimal briefing field** the user's prompt references; no injected prose.
- Pipeline **shape** (fan-out, signal scope, capital pooling) is **Strategy config**, not a harness invariant. v1 ships the `PerAsset` fan-out with shared NAV as the default and only-implemented arm.
- All surfaces (CLI + dashboard) respond to multi-asset.

## Already shipped on `main` (not in scope to build)

Confirmed present in the clean tree this branch forks from:

- **Capability-first agent model** (Phases A–E): `Capability {Trader,Filter,Critic,Intern,Router}`, `AgentSlot.capabilities`, `AgentRef.activates`, `dispatch_capability`, `filter_dispatch`, unified recorder, capability-aware templates.
- **F18 cascade** (#533): `TraderDecision.asset: AssetSymbol` required; risk reads `asset` directly; executors (Alpaca + Orderly) route per-decision; `BacktestConfig::instrument` removed.
- **Orderly multi-asset expansion** (#540): per-asset Orderly routing.
- **`AssetSymbol`** (15-symbol Alpaca crypto whitelist) with forgiving `FromStr`.
- **`Strategy`/`PublicManifest.asset_universe: Vec<String>`** — already multi-valued.
- **Bars cache + Alpaca fetcher** — fetch any whitelisted asset for any window.
- **`PortfolioState.open_positions: BTreeMap<AssetSymbol, OpenPosition>`** — accounting type is already multi-asset.

So the data/types are multi-asset-*ready*; the gap is that the **run harness and surfaces still assume one asset** (extracted from the scenario), and pipeline shape is hardcoded.

## What's actually single-asset today (the gap)

| Location | Single-asset assumption |
|---|---|
| `eval/scenario.rs:42` | `Scenario.asset: Vec<AssetRef>` carries the symbol; `api/scenario.rs:4` gate enforces `len()==1` |
| `eval/executor/backtest.rs:389` / `paper.rs` | `scenario.asset.first()` — one asset replayed for the whole run |
| `backtest.rs:508–509` | scalar `position`/`entry_price`; `:636/:824/:1483` equity is single-asset mark-to-market |
| `backtest.rs:696…` (seed JSON) | `"asset": asset` set from `scenario.asset[0]` |
| `agent/signal_cache.rs:42` | `SignalCacheKey { strategy_id, role }` — no asset/scope; collides across assets under fan-out |
| `eval/live_config.rs:163` | `if self.assets.len() != 1` single-asset Live wall |
| CLI `strategy.rs` / `scenario.rs` / `ab_compare.rs` | `--asset` singular |
| `frontend ScenarioForm.tsx` / `authoring.tsx` | single-select scenario asset; read-only `asset_universe` |

## Decisions (locked for operator review)

1. **Scenarios become asset-free.** Drop `Scenario.asset: Vec<AssetRef>`. **Keep** `asset_class` and `quote_currency` — they describe the *market* (drive the broker rule set at `backtest.rs:500` and fees, and gate universe compatibility), not a specific symbol. No SQL migration: `scenarios` rows store the struct in a `body_json` blob with no `asset` column, and `Scenario` is not `deny_unknown_fields`, so legacy rows drop the now-unknown `asset` key on parse.

2. **`Strategy.asset_universe: Vec<String>` is the source of truth** for which assets a run trades. (Already exists; no shape change.)

3. **Fan-out is a `Strategy` field, not a harness invariant.** Add `Strategy.execution_mode: ExecutionMode`:
   ```rust
   #[serde(rename_all = "snake_case")]
   pub enum ExecutionMode {
       #[default]
       PerAsset,            // v1: implemented. Pipeline runs once per active asset per bar.
       Portfolio,           // reserved: one cycle sees all assets; trader reasons as a book.
       Custom(String),      // open hatch (free-text), forward-compat for optimizer-authored modes.
   }
   ```
   The harness **branches on this field**. v1 implements only the `PerAsset` arm; `Portfolio`/`Custom` parse and validate but return a clear `not yet implemented: execution_mode=<x>` error at run time. A future portfolio-aware strategy needs no harness refactor — it ships a `Strategy` with `execution_mode: Portfolio`.

4. **Signal scope is first-class** (not an `asset` field bolted onto the key):
   ```rust
   #[serde(rename_all = "snake_case")]
   pub enum SignalScope {
       Global,                          // universe-wide (e.g. market regime)
       Asset(AssetSymbol),              // one symbol
       Pair(AssetSymbol, AssetSymbol),  // spread / pairs
       Custom(String),                  // free-text
   }
   // signal_cache.rs
   pub struct SignalCacheKey { pub strategy_id: String, pub role: String, pub scope: SignalScope }
   ```
   This fixes the per-asset collision correctly **and** makes cross-asset/global signals first-class instead of synthetic asset names. In v1's `PerAsset` arm the dispatcher tags each filter signal `Asset(current_asset)` by default; the type already admits `Global`/`Pair`/`Custom` so a future filter emits them with **no key migration**.

   **Briefing/edge resolution (preserves the existing prompt contract):** the capability model presents filter signals to downstream agents as `filter_signals["<role>"]` (keyed by role name — see capability spec Decision 6) and evaluates edge predicates against the same view. Once the cache is `(role, scope)`-keyed, the per-cycle briefing builder and the edge-predicate evaluator **resolve the scoped map down to the current cycle's asset** — selecting entries whose scope is `Asset(current_asset)` or `Global` — and re-present them keyed by role. Existing trader prompts and edges that reference `filter_signals["regime"]` keep working unchanged; they simply see this asset's `regime`. This is the one load-bearing integration point multi-filter strategies depend on.

5. **The briefing exposes the data; the prompt decides what to attend to.** The cycle briefing carries:
   - `active_assets: Vec<AssetSymbol>` — always present (names only; tiny).
   - `current_asset: AssetSymbol` — present when fanned out (`PerAsset`). This is the existing seed `"asset"` field, now set to the fan-out asset instead of `scenario.asset[0]`. **No prose is injected** — the user's `system_prompt` references the field as it sees fit.
   - `universe_bars: BTreeMap<AssetSymbol, …>` — **gated** to avoid token bloat. Populated only when `execution_mode` warrants cross-asset reasoning (`Portfolio`/`Custom`); omitted in the default `PerAsset` per-asset briefing so the common path pays no extra tokens. (Reconciles "expose loaded data" with the operator's "don't inject too much" caveat.)

6. **No named "universe stage" seam.** The harness asks one question: *given this strategy, what is the active asset set this bar?* In v1 the answer is the static `asset_universe` (optionally narrowed by a run-time `--assets` subset ⊆ universe). A future cross-asset *selector* is **just another free-text agent slot** the strategy declares in `Strategy.agents`; the resolver consults it then. There is no hardcoded stage boundary or reserved slot name — consistent with "agent slots are free-text, user-defined."

7. **Capital pooling and risk caps are Strategy config with defaults, not new harness defaults.**
   - NAV pooling: add `Strategy.capital_mode: CapitalMode { #[default] Pooled, PerAsset }`. v1 implements `Pooled` (one capital pool; per-asset positions via `PortfolioState.open_positions`; shared equity = `initial + Σ realized + Σ (positionₐ × (markₐ − entryₐ))`). `PerAsset` (segregated sub-portfolios) parses but is not implemented in v1.
   - Risk caps: route through the **existing** strategy-level `strategies::risk::RiskConfig` (total-exposure, max-open-positions, correlation-cluster rules already key per-asset). No new harness-default risk behavior is introduced.

8. **Asset injection is minimal and prompt-controlled** (restates 5): the only change to what the LLM sees by default is that the existing `asset` briefing field is now correct per fan-out asset. Nothing is prepended.

### Decisions-with-defaults flagged for review

- **(5) `universe_bars` gating:** default = omit in `PerAsset`, populate in `Portfolio`/`Custom`. Alternative: always populate (simpler, but adds tokens to every per-asset prompt). Defaulting to gated.
- **(7) `capital_mode` field now vs. defer:** included now as a one-arm-implemented enum to honor the config-over-harness principle. If you consider it premature enum-sprawl, we drop the field and keep `Pooled` as implicit v1 behavior (documented), reintroducing the field when a second mode is built.

## Data-model changes

- **`Scenario`** (`eval/scenario.rs`): remove `asset: Vec<AssetRef>`. Keep `asset_class`, `quote_currency`. Update `canonical_scenarios()`, `api/scenario.rs` create/validate (drop the `len()==1` gate), `ScenarioForm`, and tests. (`AssetRef` type stays — still used elsewhere.)
- **`Strategy`/`PublicManifest`** (`strategies/manifest.rs`, `strategies/mod.rs`): add `execution_mode: ExecutionMode` and `capital_mode: CapitalMode`, both `#[serde(default)]` so existing strategy JSON parses unchanged (strategies are filesystem JSON — no migration, mirrors the `Strategy.color` precedent). `asset_universe` unchanged. Add `#[derive(ts_rs::TS)]` exposure so the frontend can read/edit them.
- **`SignalScope`** + **`SignalCacheKey`** (`agent/signal_cache.rs`, `agent/dispatch_capability.rs`): add the enum; extend the key; thread `scope` through `filter_dispatch.rs` and the `filter_signals` map (keyed by `(role, scope)`), and through the cache lookup/insert + granularity freshness checks.
- **Briefing** (`agent/briefing.rs` + seed construction in `eval/executor/*`): add `active_assets`, set `current_asset` per fan-out asset, gate `universe_bars`.
- **Live** (`eval/live_config.rs`): the `assets.len() != 1` wall is **left in place** (multi-asset *live* execution is out of scope — see below; this feature targets the eval/backtest surface).

## Harness changes (v1 implements `PerAsset` + `Pooled` only)

In `eval/executor/backtest.rs` and `paper.rs`:

1. **Resolve the active asset set:** `active_assets(strategy, run_subset) -> Vec<AssetSymbol>` — v1 returns `strategy.asset_universe` (parsed to `AssetSymbol`) ∩ `run_subset` (if `--assets` given). Pure function; no agent consulted in v1.
2. **Branch on `execution_mode`:** `PerAsset` → the loop below; `Portfolio`/`Custom` → `Err(not implemented)`.
3. **Load + align bars:** load each active asset's bars for the scenario window/granularity (existing `load_bars_for_scenario`, parameterized by asset). Build a timeline **aligned by timestamp** (outer-join; an asset missing a bar at `t` simply gets no decision at `t` and carries its position).
4. **Per-bar, per-asset:** for each timestamp, for each active asset with a bar there:
   - build the briefing (`current_asset`, `active_assets`; `universe_bars` omitted in `PerAsset`),
   - run the existing capability pipeline; filter signals tagged `Asset(current_asset)` and cached under the scoped key,
   - produce a `TraderDecision{asset}` → risk → executor.
5. **Accounting (`Pooled`):** replace scalar `position`/`entry_price` with `BTreeMap<AssetSymbol, _>`; shared equity as in Decision 7. `realized_total` stays a single pooled accumulator. Mark-to-market sums per-asset.
6. **Cycle identity:** `cycle_id` becomes per `(bar, asset)` so A/B cache pairing and observability stay unambiguous per decision. (Existing A/B pairing keys on `cycle_id`; making it per-(bar,asset) preserves the invariant.)

## Surfaces (full vertical)

**CLI** (`crates/xvision-cli/src/commands/`):
- `xvn strategy new --assets BTC,ETH,SOL` (plural, comma-delimited → `asset_universe`); keep `--asset` as a deprecated singular alias mapping to a 1-element universe. Add `--execution-mode` (default `per-asset`).
- `xvn scenario create` drops `--asset` (scenarios asset-free); keeps `--asset-class`/`--quote-currency`.
- `xvn eval` / `xvn ab-compare` gain optional `--assets <subset>` (⊆ universe).
- Eval/compare report formatting (`eval/compare_format.rs`) gains per-asset rollup rows/columns (data is already per-decision via `DecisionRowDto.asset`).

**Frontend** (`frontend/web/src/`):
- `ScenarioForm.tsx`: remove the asset picker (asset-free); keep asset-class.
- Strategy authoring (`routes/authoring.tsx`): make `asset_universe` an **editable multi-select** (the `ALPACA_ASSETS` 15-symbol list already exists in `ScenarioForm`); surface `execution_mode` (read-only badge in v1, since only `PerAsset` is implemented).
- Expose `asset_universe` (+ `execution_mode`) on `StrategySummary` so list/detail views render it.
- Eval results (`routes/eval-runs-detail.tsx`, `eval-compare.tsx`): per-asset rollups — group decisions/PnL by `asset`; per-asset equity contribution.
- No popups (workspace rule): all of the above route/inline-expand.

## Testing & evidence

TDD per phase — failing test first, then implementation, then green. Key tests:

- **Data model:** legacy scenario `body_json` with `asset` key parses (key dropped); strategy JSON without `execution_mode`/`capital_mode` parses to defaults; `SignalCacheKey` round-trips each `SignalScope`; two `Asset(BTC)` vs `Asset(ETH)` signals for the same role **don't collide**.
- **Harness:** a `PerAsset` backtest over `[BTC,ETH]` produces decisions for **both** assets; shared NAV equals pooled formula; per-asset positions tracked independently; `Portfolio` mode returns the not-implemented error; `--assets ETH` subsets correctly.
- **Filter scope (single):** a strategy with one Filter + one Trader over 2 assets caches/serves per-asset signals (regression for the collision bug).
- **Multi-filter:** a strategy with 2 Filters (e.g. `regime` + `vol`) + 1 Trader over 2 assets produces 4 non-colliding cache entries; each asset's trader briefing surfaces `filter_signals["regime"]`/`["vol"]` scoped to *that* asset; an edge predicate referencing `regime` gates per-asset correctly.
- **CLI:** `xvn strategy new --assets BTC,ETH,SOL` writes a 3-asset universe; report shows per-asset rollup.
- **No-regression:** existing single-asset (1-element universe) runs produce byte-identical results to pre-change baseline (pin via fixture diff).

**Evidence of completion (beyond green tests):**
1. CLI transcript: create a 3-asset strategy, run a backtest against an asset-free scenario, show the per-asset rollup report + shared NAV.
2. A run's decision trace showing `TraderDecision.asset` varying across BTC/ETH/SOL within one run.
3. UI screenshot: editable multi-select universe + asset-free scenario form + per-asset results view.

## Phases (for the implementation plan)

- **Phase A — data model & types (no behavior):** drop `Scenario.asset`; add `ExecutionMode`, `CapitalMode`, `SignalScope`; extend `SignalCacheKey`; add briefing fields; ts-rs exposure. Parsing/validation/round-trip tests only.
- **Phase B — harness `PerAsset` fan-out + `Pooled` NAV + asset-scoped signals.** The core. `Portfolio`/`Custom`/`PerAsset`-capital arms return not-implemented.
- **Phase C — CLI** (`--assets`, asset-free scenario create, per-asset report rollups).
- **Phase D — frontend** (editable universe multi-select, asset-free scenario form, per-asset result rollups, summary exposure).
- **Phase E — evidence & acceptance** (capture the three evidence artifacts; no-regression fixture diff).

## Out of scope (explicitly)

- `Portfolio` / `Custom` execution-mode behavior (field + branch + rejection ship; the arms don't).
- `PerAsset` capital segregation (`capital_mode: PerAsset` behavior).
- A cross-asset **selector** agent / filter-driven asset selection (the substrate — scoped signals, free-text slots, active-set resolver — is built so this is incremental later; the selector itself is not built).
- `Pair`/`Global` signal **producers** (the scope types exist; no agent emits them in v1). **Known v1 limitation:** a semantically universe-wide filter runs redundantly once per active asset (each result tagged `Asset(x)`, not `Global`) — correct but not deduplicated. `SignalScope::Global` makes the future "run once, fan the result to all assets" optimization a non-breaking change.
- Multi-asset **live** trading execution (the `live_config.rs` single-asset wall stays; lifting it needs its own broker-side safety review).
- New asset classes beyond the existing `AssetSymbol` whitelist.

## Acceptance

1. Operator marks the **Decisions** list reviewed (accept / change), including the two decisions-with-defaults.
2. Spec committed to `docs/superpowers/specs/2026-05-24-multi-asset-strategies-design.md`.
3. Proceed to `writing-plans` for the phased TDD implementation plan.
