# Custom-Scenario Eval — Design

> **Status (2026-05-21):** Largely SHIPPED. M1 bars-cache + Alpaca fetcher, M2 (scenario table + CLI), and M3 (dashboard wizard) all landed. The residual work is the BTC-only wall removal (asset unlock) — `AssetSymbol` enum expansion, the `assets.len() != 1` validator at `crates/xvision-engine/src/eval/scenario.rs::validate_v1` line 263, and the BTC-only checks in `crates/xvision-execution/src/alpaca.rs`. That residual is tracked in `docs/superpowers/plans/2026-05-21-multi-asset-alpaca-unlock.md`. Treat this file as historical design context for the broader Custom-Scenario program; the asset-unlock residual has its own focused plan.
> **Status:** Design / spec — accepted, ready for implementation planning. Drafted 2026-05-11.
> **Author:** xvision team (with brainstorming input from a consulting agent).
> **Companion specs:** [Marketplace Plugin](./2026-05-09-marketplace-plugin-design.md) · [Karpathy AutoOptimizer](./2026-05-09-karpathy-autooptimizer-design.md) · [xvn scheduling & agent CLI](./2026-05-10-xvn-scheduling-and-agent-cli-design.md) · [Install Customizer](./2026-05-11-install-customizer-design.md)
> **Tracking:** F30 (this spec) + F31 (replay-mode follow-ups). Adds to / supersedes parts of: the `BTC-only` scope wall in `crates/xvision-execution/src/alpaca.rs`, the compiled-in `canonical_scenarios()` in `crates/xvision-engine/src/eval/scenario.rs`, and the parts of F18 (`TraderDecision.asset`) that this spec pulls forward.

---

## 1. Purpose

The eval engine today can only run against four hard-coded BTC/USD scenarios baked into `canonical_scenarios()`. There is no surface — CLI, dashboard, or otherwise — to say "test strategy X on ETH from 2024-02-03 to 2025-02-03." Two walls cause this:

1. **Scope wall.** `xvision-execution/src/alpaca.rs:3` carries the comment "v1 scope: BTC-only via Alpaca's crypto endpoint"; the parser at line 46 rejects anything that isn't BTC. This is a self-imposed scope decision, not an Alpaca platform limitation — Alpaca's `/v1beta3/crypto/{loc}/bars` endpoint supports ~20 crypto pairs (BTC, ETH, LTC, SOL, AVAX, LINK, AAVE, UNI, DOT, DOGE, SHIB, MATIC, BCH, USDT, USDC, …) and ~thousands of US equities.
2. **Scenario wall.** Scenarios live as compiled-in Rust constants. There is no scenario-create surface and no scenario storage layer. Adding a new scenario requires editing source and recompiling.

This design opens both walls, replacing `canonical_scenarios()` with a SQLite-backed scenario registry, an Alpaca historical-bars fetcher, and a CLI + dashboard surface for authoring custom scenarios. It also re-maps `Scenario` to carry **only** properties of the world (asset, time window, granularity, venue settings); strategy-level properties (capital, risk caps) move to `StrategyBundle` where they belong.

The spec covers **crypto-only** for v1. The schema is asset-class-agnostic — `asset_class: Crypto | Equity | Option | Future` is on the struct — but the data path and validator restrict to crypto until v2.

---

## 2. Locked decisions

| # | Decision |
|---|---|
| 1 | **Scenario registry replaces `canonical_scenarios()`.** All scenarios live as rows in a new `scenarios` SQLite table; the four existing BTC scenarios seed the table on first migration with their existing IDs preserved (`source = 'canonical'`). |
| 2 | **Drop the BTC-only wall.** All Alpaca crypto assets are unlocked at the executor + parser layer. The asset whitelist becomes the compile-time list of Alpaca-supported crypto pairs. |
| 3 | **Schema asset-class-agnostic, crypto-only data path.** `asset_class` and `quote_currency` exist on the struct; the v1 validator rejects anything that isn't `Crypto` + `USD`. Equities / options re-open the validator in v2 without a schema break. |
| 4 | **SQLite bar cache.** New `bars_cache` table keyed by `cache_key = blake3(asset ‖ granularity ‖ window ‖ data_source)`. Cache is the only thing the harness reads; the fetcher is invisible to runs. |
| 5 | **Scenario field is `Vec<AssetRef>`; v1 validator enforces `len == 1`.** Schema accommodates basket eval in v2 without a break. |
| 6 | **Capital + risk move OFF `Scenario`, ON to `StrategyBundle`.** Breaking change to the engine, justified by separation of concerns: the world ≠ the agent. M2 migration moves canonical scenarios' (capital, risk) into a single `StrategyBundle` named `canonical-defaults`. |
| 7 | **Granularity enum** (`Minute1`, `Minute5`, `Minute15`, `Hour1`, `Day1`); v1 validator enforces `{Hour1, Day1}` only. |
| 8 | **`ReplayMode::Continuous` only in v1**; `Stepped`, `Accelerated`, `Realtime` ship as follow-ups in that priority order (F31). No pause-channel machinery in v1. |
| 9 | **Hybrid mutability — immutable rows + optional `parent_scenario_id` for lineage.** Rows never edit in place. "Edit" means clone-and-fork. `runs.scenario_id` FK guarantees every run pins a stable scenario definition. Lineage breadcrumbs reconstruct version history. |
| 10 | **Three-milestone staging.** M1: bar cache + Alpaca fetcher + asset unlock. M2: scenario table + CLI + capital/risk move + canonical seed. M3: dashboard wizard + inline form + run launcher. |
| 11 | **Naming discipline.** v1 mode = **Alpaca historical scenario replay with simulated execution** (`RunMode::Backtest`). The existing `RunMode::Paper` path (routes orders to Alpaca's real paper API) reserves the name **"Paper mirror"** for future docs and UI copy. v1 dashboard surfaces both names distinctly. |
| 12 | **Wizard is minimal-by-default + collapsible Advanced.** Three required fields (asset, date range, granularity) + Advanced pane (fees / slippage / latency / fill model). |
| 13 | **F18 partial pull-in.** `TraderDecision.asset` lands in M1 as a field with a default sourced from the active scenario's single asset; the broader multi-asset cascade (Trader prompt schema, Risk param drops, Eval `BacktestConfig.instrument` drop) stays on F18 proper. |

---

## 3. In scope / out of scope

### 3.1 In scope (v1, all three milestones)

- New `crates/xvision-data/src/alpaca.rs` historical-bars fetcher (crypto only, v1beta3 endpoint, paginated, rate-limited).
- New `bars_cache` SQLite table + `eval::bars::load_bars` cache wrapper.
- New `scenarios` SQLite table + `scenario_tags` junction.
- `scenarios` ⇄ `runs` foreign key (existing `runs.scenario_id` text column gets the constraint).
- Re-shaped `Scenario` struct: capital + risk out; lineage + tags + notes + source in; venue settings unified; reproducibility fields (cache key + refresh policy + fetched-at).
- `xvn bars fetch / ls / rm / gc` CLI.
- `xvn scenario create / ls / show / clone / archive / rm / tree` CLI.
- `xvn eval run --scenario <id>` consumes DB rows (no more compiled-in constants).
- Drop BTC-only walls in `xvision-execution/alpaca.rs`; expand `AssetSymbol` enum to cover the Alpaca crypto whitelist.
- Capital + risk migration onto `StrategyBundle`; single `canonical-defaults` bundle seeded for canonical scenarios.
- Dashboard: `/scenarios` list, `/scenarios/new` wizard (minimal + Advanced), `/scenarios/:id` detail with Clone CTA, "+ New scenario" inline-form on `/eval-runs`, run launcher on `/eval-runs`.
- Four basic scenarios seeded as `source = 'canonical'` (existing IDs preserved).

### 3.2 Out of scope (deferred)

- Basket scenarios with > 1 asset (schema ready; validator gated; v2).
- Equities, options, futures data paths (schema ready; validator gated).
- `ReplayMode::Stepped`, `Accelerated`, `Realtime` (F31).
- `RunMode::Paper` ("Paper mirror") expansion beyond what currently works.
- Synthetic walk data source for production use (`SyntheticWalk` exists as an enum variant but is unit-test scaffolding only in v1).
- Event injection, bar perturbation, universe filters.
- Bar adjustments (splits / dividends) — equities-only, deferred.
- Versioned mutable scenarios (chose lineage-via-fork instead).
- Operator-supplied CSV / parquet bar imports.
- Live-paper mirror mode (the future home of `RunMode::Paper`).

---

## 4. Architecture

### 4.1 Conceptual split

```
Scenario  =  the world         { asset, time_window, granularity, asset_class,
                                 quote_currency, timezone, calendar,
                                 venue settings, data source, replay mode,
                                 lineage, audit }

Strategy  =  the agent         { capital, risk caps, intern/trader/risk config,
                                 prompts, position sizing }    ← StrategyBundle
```

### 4.2 Data flow (post-M3)

```
┌─ Operator ──────┐    ┌─ Operator ────────┐
│ scenario create │    │ strategy create   │
└────────┬────────┘    └────────┬──────────┘
         ▼                      ▼
   ┌───────────┐          ┌─────────────┐
   │ scenarios │          │  strategy_  │
   │   (DB)    │          │  bundles    │
   └─────┬─────┘          └──────┬──────┘
         │                       │
         └──────┬────────────────┘
                ▼
         ┌─────────────┐    ┌──────────────────┐
         │  eval run   │ ←─ │   bars_cache     │
         │  (engine)   │    │  (Alpaca, DB)    │
         └──────┬──────┘    └────────┬─────────┘
                │                    ▲
                ▼                    │
         ┌─────────────┐    ┌────────┴─────────┐
         │  runs +     │    │ AlpacaBarsFetcher│
         │  metrics    │    │ (xvision-data)   │
         └─────────────┘    └──────────────────┘
```

### 4.3 Milestones

| M | Ships | Unlocks |
|---|---|---|
| **M1 — Asset unlock + bars** | Alpaca crypto fetcher, `bars_cache` table, `xvn bars fetch` CLI, BTC-only wall removal in `xvision-execution/alpaca.rs`, `AssetSymbol` enum expansion. F18 partial: `TraderDecision.asset` field added with single-asset default. | "Test on ETH today" via CLI: `xvn ab-compare --asset ETH …`. No schema work yet. |
| **M2 — Scenario shape** | `scenarios` + `scenario_tags` tables + `runs.scenario_id` FK; `Scenario` struct re-mapped (capital/risk off); `xvn scenario create/ls/show/clone/archive/rm/tree` CLI; canonical scenarios seeded; `canonical-defaults` `StrategyBundle` seeded; `xvn eval run --scenario <id>` consumes DB rows; `xvn eval scenarios` deprecated. | Full custom-scenario CLI flow; v1 capability complete. |
| **M3 — Dashboard surface** | `/scenarios`, `/scenarios/new`, `/scenarios/:id`, inline "+ New scenario" on `/eval-runs`, run launcher on `/eval-runs`. | Visual flow for the gap originally flagged ("no UI to start a run with a custom date range + asset"). |

### 4.4 Crate boundaries

- **New code:** `crates/xvision-data/src/alpaca.rs` (~400 LOC), `crates/xvision-engine/src/eval/bars.rs` (cache wrapper), `crates/xvision-engine/src/api/scenario.rs` (CRUD API), `crates/xvision-cli/src/commands/scenario.rs`, `crates/xvision-cli/src/commands/bars.rs`, `frontend/web/src/routes/scenarios.tsx` + `scenarios/new.tsx` + `scenarios/$id.tsx`.
- **Touched code:** `xvision-engine/src/eval/scenario.rs` (struct re-shape + drop `canonical_scenarios()`), `xvision-engine/src/api/eval.rs` (DB-resolved scenario lookup, cached-bars wiring), `xvision-engine/src/eval/store.rs` (migrations), `xvision-execution/src/alpaca.rs` (BTC-only wall removal + `AssetSymbol` expansion), `xvision-eval/src/backtest.rs` (BacktestConfig accepts venue settings from scenario), `xvision-cli/src/lib.rs` (subcommand wiring), `frontend/web/src/routes/eval-runs.tsx` (inline form + run launcher).

---

## 5. Scenario schema

```rust
pub struct Scenario {
    // identity & lineage
    pub id: ScenarioId,                       // ULID
    pub parent_scenario_id: Option<ScenarioId>,
    pub source: ScenarioSource,               // Canonical | User | Clone | Generated
    pub display_name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub notes: Option<String>,

    // the world
    pub asset_class: AssetClass,              // Crypto | Equity | Option | Future
    pub asset: Vec<AssetRef>,                 // v1 validator: len == 1
    pub quote_currency: QuoteCurrency,        // Usd | Usdt | Usdc — v1: Usd only
    pub time_window: TimeWindow,
    pub granularity: BarGranularity,          // v1 validator: Hour1 | Day1
    pub timezone: String,                     // IANA tz; "UTC" for crypto v1
    pub calendar: CalendarRef,                // Continuous24x7 | UsEquities | Custom(...)

    // data + execution model
    pub data_source: DataSource,
    pub venue: VenueSettings,
    pub replay_mode: ReplayMode,              // v1 validator: Continuous

    // reproducibility
    pub bar_cache_policy: BarCachePolicy,

    // audit
    pub created_at: DateTime<Utc>,
    pub created_by: String,
    pub archived_at: Option<DateTime<Utc>>,
}

pub enum ScenarioSource { Canonical, User, Clone, Generated }

pub enum AssetClass { Crypto, Equity, Option, Future }  // v1 validator: Crypto only

pub struct AssetRef {
    pub class: AssetClass,
    pub symbol: String,                       // e.g. "ETH"
    pub venue_symbol: String,                 // e.g. "ETH/USD"
}

pub enum QuoteCurrency { Usd, Usdt, Usdc }

pub struct TimeWindow {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

pub enum BarGranularity { Minute1, Minute5, Minute15, Hour1, Day1 }
// v1 validator: Hour1 | Day1

pub enum CalendarRef {
    Continuous24x7,                           // crypto
    UsEquities,                               // v2 — NYSE/NASDAQ calendar
    Custom(String),                           // operator-supplied ref
}

pub enum DataSource {
    AlpacaHistorical {
        feed: Option<String>,                 // None for crypto; "iex"/"sip" for equities (v2)
        adjustment: AdjustmentMode,           // Raw | SplitAdjusted | SplitDividendAdjusted (v2)
    },
    SyntheticWalk { seed: u64, model: WalkModel },   // v1: unit-test scaffolding only
}

pub enum AdjustmentMode { Raw, SplitAdjusted, SplitDividendAdjusted }

pub struct VenueSettings {
    pub venue: Venue,                         // Alpaca | (future) Orderly
    pub fees: Fees { maker_bps: u32, taker_bps: u32 },
    pub slippage: SlippageModel,              // Linear { bps } | None
    pub latency: LatencyModel,                // { decision_to_fill_ms: u32 }
    pub fill_model: FillModel,                // v1: { market_only, full_fills, no_volume_constraint }
}

pub struct FillModel {
    pub market_order_fill: MarketOrderFill,   // v1: FullAtClose
    pub limit_order_fill: LimitOrderFill,     // v1: NeverFills (v2 surface)
    pub partial_fills: bool,                  // v1: false
    pub volume_constraints: Option<VolumeConstraint>,  // v1: None
}

pub enum ReplayMode {
    Continuous,                               // v1 only
    Stepped,                                  // follow-up F31 #1
    Accelerated { speed: f64 },               // follow-up F31 #2
    Realtime,                                 // follow-up F31 #3 (gated on live-paper mirror)
}

pub struct BarCachePolicy {
    pub cache_key: String,                    // blake3 of (asset, granularity, window, data_source)
    pub refresh_policy: RefreshPolicy,        // NeverRefresh | RefreshIfOlderThan(Duration)
    pub data_fetched_at: Option<DateTime<Utc>>,
}

pub enum RefreshPolicy { NeverRefresh, RefreshIfOlderThan(Duration) }
```

### 5.1 What comes OFF the struct (vs `crates/xvision-engine/src/eval/scenario.rs` today)

- `capital: Capital` → `StrategyBundle.capital`.
- `risk: ScenarioRisk` → `StrategyBundle.risk` (merges with existing `xvision-risk` config).
- `regime_tags: Vec<String>` → folded into the more general `tags: Vec<String>` (regime tags use a `regime:bull`, `regime:bear`, … prefix convention).
- `data_seed: String` → eliminated; reproducibility handled by `bar_cache_policy.cache_key`.

### 5.2 Cache-key derivation

```rust
fn cache_key(
    asset: &str,
    granularity: BarGranularity,
    window: &TimeWindow,
    source: &DataSource,
) -> String {
    let mut h = blake3::Hasher::new();
    h.update(asset.as_bytes());
    h.update(granularity.as_str().as_bytes());
    h.update(window.start.to_rfc3339().as_bytes());
    h.update(window.end.to_rfc3339().as_bytes());
    h.update(serde_json::to_string(source).unwrap().as_bytes());
    h.finalize().to_hex().to_string()
}
```

Same `(asset, granularity, window, data_source)` → same `cache_key` → same bars. Determinism free.

---

## 6. Database schema

```sql
-- 0003_scenarios.sql  (M2)
CREATE TABLE scenarios (
    id                  TEXT PRIMARY KEY,             -- ULID
    parent_scenario_id  TEXT REFERENCES scenarios(id),
    source              TEXT NOT NULL,                -- 'canonical' | 'user' | 'clone' | 'generated'
    display_name        TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    body_json           TEXT NOT NULL,                -- full Scenario serialized (immutable post-insert)
    created_at          TEXT NOT NULL,
    created_by          TEXT NOT NULL,
    archived_at         TEXT
);
CREATE INDEX scenarios_by_source       ON scenarios(source);
CREATE INDEX scenarios_by_parent       ON scenarios(parent_scenario_id);
CREATE INDEX scenarios_by_archived_at  ON scenarios(archived_at);

CREATE TABLE scenario_tags (
    scenario_id TEXT NOT NULL REFERENCES scenarios(id) ON DELETE CASCADE,
    tag         TEXT NOT NULL,
    PRIMARY KEY (scenario_id, tag)
);
CREATE INDEX scenario_tags_by_tag ON scenario_tags(tag);

-- 0004_bars_cache.sql  (M1)
CREATE TABLE bars_cache (
    cache_key     TEXT PRIMARY KEY,
    asset         TEXT NOT NULL,
    granularity   TEXT NOT NULL,                      -- '1h', '1d'
    window_start  TEXT NOT NULL,
    window_end    TEXT NOT NULL,
    data_source   TEXT NOT NULL,                      -- 'alpaca-historical-v1'
    fetched_at    TEXT NOT NULL,
    bar_count     INTEGER NOT NULL,
    bars_blob     BLOB NOT NULL                       -- newline-delimited JSON; gzipped for windows > 30d
);
CREATE INDEX bars_cache_by_asset_window
    ON bars_cache(asset, granularity, window_start, window_end);

-- 0005_runs_scenario_fk.sql  (M2)
-- runs already stores scenario_id as text; this migration enforces the FK + adds an index
-- (executed via table-rebuild dance in SQLite since ALTER ADD CONSTRAINT isn't supported directly)
CREATE INDEX IF NOT EXISTS runs_by_scenario ON runs(scenario_id);
-- enforcement via trigger or table-rebuild — see migration script
```

### 6.1 Immutability enforcement

- `scenarios::update()` does not exist in the API. Only `archive()` mutates a row, and only the `archived_at` column.
- An optional trigger (`scenarios_no_update`) rejects any UPDATE that touches columns other than `archived_at`. Belt + suspenders.

---

## 7. Bar fetcher + cache

### 7.1 Fetcher (`crates/xvision-data/src/alpaca.rs`)

```rust
pub struct AlpacaBarsFetcher {
    base_url: String,                          // https://data.alpaca.markets
    api_key: String,
    api_secret: String,
    rate_limiter: Arc<RateLimiter>,            // shared via ApiContext
}

impl AlpacaBarsFetcher {
    pub async fn fetch_crypto_bars(
        &self,
        asset: &str,                           // "BTC/USD", "ETH/USD", ...
        granularity: BarGranularity,
        window: TimeWindow,
    ) -> Result<Vec<MarketBar>, FetchError>;
}

pub enum FetchError {
    Unauthorized,                              // 401
    RateLimited { retry_after_secs: u32 },     // 429 — auto-retry inside fetcher
    AssetNotFound(String),                     // 404
    RangeOutsideHistory { earliest_available: DateTime<Utc> },
    Network(reqwest::Error),
    Parse(serde_json::Error),
}
```

- Endpoint: `GET /v1beta3/crypto/us/bars?symbols={asset}&timeframe={1Hour|1Day}&start={iso}&end={iso}&limit=10000&page_token={cursor}`.
- Pagination: follow `next_page_token` until null. Year-long 1h ≈ 8760 bars → 1–2 pages.
- Auth: `APCA-API-KEY-ID` + `APCA-API-SECRET-KEY` headers; reads from `XVN_HOME/secrets/brokers.toml` (per `build_alpaca_paper_broker` at `crates/xvision-engine/src/api/eval.rs:447`), env-var fallback.
- Bar normalisation: Alpaca's `{t, o, h, l, c, v, n, vw}` → `MarketBar { timestamp, open, high, low, close, volume }`.
- Rate limiter shared workspace-wide via `Arc<RateLimiter>` in `ApiContext`; default 200 rpm, configurable via `[data.alpaca] rate_limit_rpm` in `config/default.toml`.

### 7.2 Cache wrapper (`crates/xvision-engine/src/eval/bars.rs`)

```rust
pub async fn load_bars(
    ctx: &ApiContext,
    cache_key: &str,
    asset: &str,
    granularity: BarGranularity,
    window: &TimeWindow,
    data_source: &DataSource,
) -> ApiResult<Vec<MarketBar>>;
```

1. `SELECT bars_blob FROM bars_cache WHERE cache_key = ?`.
2. Hit → decompress + return.
3. Miss → fetcher call → compress for windows > 30d → `INSERT INTO bars_cache`.
4. Single-flight: concurrent misses on the same key serialize through a per-key mutex held in `ApiContext`.

The harness only sees the cache wrapper. The fetcher is invisible to runs.

### 7.3 `xvn bars` CLI (M1)

```
xvn bars fetch --asset ETH --from 2024-02-03 --to 2025-02-03 --granularity 1h
xvn bars ls                                    # cached entries
xvn bars rm <cache_key>
xvn bars gc --older-than 90d
```

---

## 8. Engine API surface

### 8.1 `crates/xvision-engine/src/api/scenario.rs` (new, M2)

```rust
pub async fn create(ctx: &ApiContext, req: CreateScenarioRequest) -> ApiResult<Scenario>;
pub async fn get(ctx: &ApiContext, id: &ScenarioId) -> ApiResult<Scenario>;
pub async fn list(ctx: &ApiContext, filter: ListScenariosFilter) -> ApiResult<Vec<Scenario>>;
pub async fn clone(ctx: &ApiContext, parent: &ScenarioId, mutations: ScenarioMutations) -> ApiResult<Scenario>;
pub async fn archive(ctx: &ApiContext, id: &ScenarioId) -> ApiResult<()>;
pub async fn delete(ctx: &ApiContext, id: &ScenarioId) -> ApiResult<()>;  // blocked if runs reference it
// NOTE: no update().
```

`CreateScenarioRequest` mirrors the `Scenario` struct minus the engine-assigned fields (`id`, `created_at`, `bar_cache_policy.cache_key`, `bar_cache_policy.data_fetched_at`).

### 8.2 Validation (rejects via `ApiError::Validation`)

- `asset.len() == 1` (v1 single-asset wall).
- `granularity ∈ {Hour1, Day1}` (v1 enforced subset).
- `replay_mode == Continuous` (v1 enforced subset).
- `asset_class == Crypto` and `quote_currency == Usd` (v1 wall).
- `time_window.start < time_window.end`.
- `time_window.end <= now`.
- `time_window.start >= alpaca_crypto_history_start()` (= 2021-09-26, compile-time const).
- `asset[0].symbol ∈ ALPACA_CRYPTO_WHITELIST` (compile-time const list of supported crypto pairs).
- If `parent_scenario_id` is set: parent exists and is not archived.

### 8.3 `eval::run` wiring (`crates/xvision-engine/src/api/eval.rs`, M2 patch)

```rust
// Before (line 526):
let scenario: Scenario = canonical_scenarios()
    .into_iter()
    .find(|s| s.id == req.scenario_id)
    .ok_or_else(|| ApiError::NotFound(format!("scenario '{}'", req.scenario_id)))?;

// After:
let scenario: Scenario = api_scenario::get(ctx, &req.scenario_id).await?;
let bars = eval::bars::load_bars(
    ctx,
    &scenario.bar_cache_policy.cache_key,
    &scenario.asset[0].venue_symbol,
    scenario.granularity,
    &scenario.time_window,
    &scenario.data_source,
).await?;
let executor: Box<dyn Executor> = match req.mode {
    RunMode::Backtest => Box::new(BacktestExecutor::new(bars, &scenario.venue)),
    RunMode::Paper => /* unchanged — real Alpaca paper API; the "Paper mirror" mode */,
};
```

Existing `audit::record` calls in `run()` keep firing on the DB-resolved scenario.

---

## 9. CLI surface (M2)

```bash
# Scenarios
xvn scenario create \
    --name "ETH 2024" \
    --asset ETH \
    --from 2024-02-03 --to 2025-02-03 \
    --granularity 1h \
    --venue alpaca \
    --fees-maker 10 --fees-taker 25 \
    --slippage linear:5 \
    --latency-ms 500 \
    --tag "regression" --tag "eth" \
    --notes "year-long ETH baseline for v2 regressions"

xvn scenario create --from-file ./eth-2024.toml
xvn scenario ls [--source user|canonical|clone] [--tag <t>] [--archived]
xvn scenario show <id> [--json | --toml]
xvn scenario clone <id> --name "ETH 2024 H1" --to 2024-07-01
xvn scenario archive <id>
xvn scenario rm <id>                          # blocked if runs reference it
xvn scenario tree <id>                        # lineage view

# Eval (M2 patch)
xvn eval run --strategy <agent_id> --scenario <scenario_id> --mode backtest
xvn eval scenarios                            # DEPRECATED; prints notice + delegates to scenario ls
```

`--from-file` accepts TOML matching `CreateScenarioRequest`; `xvn scenario show --toml` round-trips. Tab-completion for `--asset` reads `ALPACA_CRYPTO_WHITELIST`.

---

## 10. Dashboard surface (M3)

### 10.1 Routes

```
/scenarios                  list view
/scenarios/new              wizard
/scenarios/:id              detail view (Definition / Runs / Bar cache tabs)
/eval-runs                  existing, + inline "+ New scenario" + run launcher
```

### 10.2 Wizard layout

```
┌────────────────────────────────────────────────────────────────┐
│  New scenario                              [Cancel] [Create →] │
├────────────────────────────────────────────────────────────────┤
│  Name      [ ETH 2024                                       ]  │
│  Notes     [ year-long ETH baseline                         ]  │
│  Tags      [ regression ] [ eth ] [+ add tag]                  │
│                                                                │
│  ┌─ Market ─────────────────────────────────────────────────┐  │
│  │  Asset class    ( ● Crypto   ○ Equity*  ○ Option* )      │  │
│  │  Asset          [ ETH ▼ ]   Quote [ USD ▼ ]              │  │
│  │  Date range     [ 2024-02-03 ] → [ 2025-02-03 ]          │  │
│  │                 [ Last year | YTD | Last 90 days | … ]   │  │
│  │  Granularity    ( ● 1h   ○ 1d )                          │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                │
│  ┌─ Venue (Alpaca) ─────────────────────────────────────────┐  │
│  │  ▸ Advanced  ◂                                           │  │
│  │      Fees    maker [ 10 ] bps   taker [ 25 ] bps         │  │
│  │      Slippage [ linear ▼ ]   [ 5 ] bps                   │  │
│  │      Latency  [ 500 ] ms                                 │  │
│  │      Fill     [ market-only, full-fills ▼ ] (v1 locked)  │  │
│  └──────────────────────────────────────────────────────────┘  │
│                                                                │
│  Replay      [ Continuous ▼ ] (Stepped/Realtime — follow-up)   │
│                                                                │
│  Lineage     parent: (none)         source: User               │
├────────────────────────────────────────────────────────────────┤
│            [Estimated bars to fetch: 8,760 · 1 API call]       │
└────────────────────────────────────────────────────────────────┘
```

`*` Equity / Option greyed out with tooltip ("Available in a future release — schema supports it; data path lands later").

### 10.3 Scenario list (`/scenarios`)

Columns: name · asset · window · granularity · source · tags · created_by · runs (count) · archived. Filters: source, asset, tag, include-archived. Row actions: View · Clone · Archive · Delete. Empty state directs to the wizard.

### 10.4 Scenario detail (`/scenarios/:id`)

- Header: name, lineage breadcrumb ("ETH 2024 v3 ← v2 ← v1"), source badge, archived banner.
- Tabs: **Definition** (read-only form, "Clone to edit" CTA) · **Runs** (sorted desc) · **Bar cache** (`bars_cache` rows backing this scenario; "Refetch" action).

### 10.5 Inline "+ New scenario" on `/eval-runs`

Button top-right expands an inline form with the same fields as the wizard. On submit: POST → scenario API → on success, pre-select the new scenario in the run launcher.

### 10.6 Run launcher on `/eval-runs`

```
┌────────────────────────────────────────────────────────────────┐
│  Launch a run                                                  │
│  Strategy [ default-trader ▼ ]   Scenario [ ETH 2024 ▼ ]       │
│  Mode     ( ● Backtest   ○ Paper mirror (live Alpaca paper) )  │
│                                              [Launch run →]   │
└────────────────────────────────────────────────────────────────┘
```

Naming discipline: **Backtest** = historical scenario replay with simulated execution; **Paper mirror** = the existing `RunMode::Paper` path (real Alpaca paper API).

### 10.7 Dashboard API routes

```
GET    /api/scenarios
POST   /api/scenarios
GET    /api/scenarios/:id
POST   /api/scenarios/:id/clone
POST   /api/scenarios/:id/archive
DELETE /api/scenarios/:id
```

Plus existing eval-run endpoints, extended with paginated progress events via `ProgressEvent` (`crates/xvision-engine/src/eval/progress.rs`).

---

## 11. Seeded basic scenarios

Four basic scenarios ship as `source = 'canonical'` rows inserted by the M2 boot migration. IDs preserved so any existing runs keep resolving.

| Seed ID | Display name | Asset | Window | Granularity | Regime tag |
|---|---|---|---|---|---|
| `crypto-bull-q1-2025` | Crypto bull — Q1 2025 | BTC/USD | 2025-01-01 → 2025-04-01 | 1h | `regime:bull` |
| `crypto-bear-q3-2024` | Crypto bear — Q3 2024 | BTC/USD | 2024-07-01 → 2024-10-01 | 1h | `regime:bear` |
| `crypto-rangebound-q2-2025` | Crypto range-bound — Q2 2025 | BTC/USD | 2025-04-01 → 2025-06-01 | 1h | `regime:chop` |
| `flash-crash-aug-2024` | Crypto flash crash — Aug 2024 | BTC/USD | 2024-08-01 → 2024-08-31 | 1h | `regime:event` |

All four use `venue.fees = { maker: 10, taker: 25 }`, `slippage = linear(5 bps)`, `latency = 500 ms`, `fill_model = { market_only, full_fills, no_volume_constraint }`. `bar_cache_policy.cache_key` is derived at seed time; `bars_cache` populates lazily on first run.

The seed set stays BTC-only because (a) existing run history references those IDs and (b) the first-impression demo for the multi-asset surface lives in the wizard's date-range presets ("Last year on ETH" is one minute of work), not in the seed set.

---

## 12. Migration plan (M2)

### 12.1 Order of operations

1. **Pre-flight check.** `xvn migrate --dry-run` reports schema delta + data movement (capital/risk move-off-scenario, scenario seed, runs FK).
2. **DB migrations.**
   1. `0003_scenarios.sql` (CREATE scenarios + scenario_tags).
   2. Seed the 4 canonical rows with `source = 'canonical'` and IDs from §11.
   3. `0004_bars_cache.sql` (CREATE bars_cache).
   4. `0005_runs_scenario_fk.sql` (FK + index on runs.scenario_id).
3. **Capital/risk move.** One-shot migration function reads each canonical scenario's `(capital, risk)` and writes a single `StrategyBundle` row named `canonical-defaults` (id `bundle-canonical-defaults`). Any existing run with a NULL bundle reference gets backfilled to point at this bundle.
4. **Code deletions.**
   - `crates/xvision-engine/src/eval/scenario.rs::canonical_scenarios()` removed.
   - `Scenario::{capital, risk, data_seed}` removed.
   - `xvn eval scenarios` ships with a deprecation notice; removed in the release after M2.

### 12.2 Forward compat

- Serde tolerates removed fields via `#[serde(default)]` on incoming JSON for one release cycle.
- New required fields (`asset_class`, `quote_currency`, etc.) on incoming JSON default to crypto-USD-Continuous for backward compat with any pre-M2 JSON the operator might re-import.

### 12.3 Rollback

`0003_scenarios_down.sql` / `0004_bars_cache_down.sql` / `0005_runs_scenario_fk_down.sql` drop the new tables in reverse order. Capital/risk values moved off-scenario are *not* automatically pushed back — the original constants live in git history if recovery is needed.

---

## 13. Error handling

### 13.1 Typed errors at each layer

```rust
// xvision-data — FetchError (§7.1) maps to ApiError as follows:
FetchError::Unauthorized              → ApiError::Validation("Alpaca credentials missing or rejected")
FetchError::RateLimited{..}           → handled inside fetcher (auto-retry); escalates to ApiError::Internal on retry-exhaustion
FetchError::AssetNotFound(s)          → ApiError::Validation(format!("asset '{}' not found on Alpaca", s))
FetchError::RangeOutsideHistory{..}   → ApiError::Validation(...)
FetchError::Network(_)                → ApiError::Internal
FetchError::Parse(_)                  → ApiError::Internal

// xvision-engine::api::scenario — typical validation messages:
ApiError::Validation("asset 'XRP' is not in the Alpaca crypto whitelist (got XRP; allowed: BTC, ETH, LTC, SOL, AVAX, LINK, AAVE, UNI, DOT, DOGE, SHIB, MATIC, BCH, USDT, USDC)")
ApiError::Validation("time_window.start (2020-01-01) is before Alpaca crypto history (2021-09-26)")
ApiError::Validation("granularity '5m' is not supported in v1 (allowed: 1h, 1d)")
ApiError::Validation("cannot delete scenario 'sc_01HQ…': 12 runs reference it. Archive instead.")
ApiError::NotFound("scenario 'sc_01HQ…'")
ApiError::Conflict("scenario with display_name 'ETH 2024' already exists for this operator")
```

### 13.2 Cache miss + Alpaca fail

If a backtest run hits an uncached bar window and Alpaca returns any error other than `RateLimited` (which auto-retries), the run fails fast with `ApiError::Validation` and a copy-paste hint: `Try 'xvn bars fetch --asset {} --from {} --to {} --granularity {}' first.` No silent fallback to partial data — backtest correctness is the priority.

---

## 14. Testing

| Layer | Test type | Coverage |
|---|---|---|
| `xvision-data::alpaca` | Unit + recorded-fixture | Bar normalisation, pagination, all `FetchError` variants. Fixtures in `crates/xvision-data/tests/fixtures/alpaca/`. `wiremock` for HTTP — no live API in CI. |
| `xvision-engine::eval::bars` (cache) | Unit + integration | Cache hit (identical bars), miss (single fetcher call + insert), concurrent misses (single-flight). |
| `xvision-engine::api::scenario` | Integration on tempdir SQLite | CRUD, all validators, FK rejection on delete-with-runs, lineage tree traversal. |
| `xvision-engine::api::eval::run` | Integration (`MockBrokerSurface` + `MockDispatch`) | End-to-end run against a created scenario; bars via cache; metrics produced. |
| Migrations 0003–0005 | Migration test | Forward against a pre-M2 DB snapshot: canonical seeds present, FK enforced, capital/risk moved to `bundle-canonical-defaults`. Down-migrations idempotent. |
| CLI | `assert_cmd` E2E | `xvn scenario create` round-trip, `clone`, `rm` blocked by runs, `xvn bars fetch` against wiremocked Alpaca. |
| Frontend | Vitest (components) + Playwright (E2E) | Wizard validation (date order, granularity subset), Clone-from-detail, inline-form on `/eval-runs`. Playwright drives headless dashboard against a seeded SQLite. |

### 14.1 Determinism guarantees the test suite defends

1. Same `cache_key` → same bars (cache integrity).
2. Same scenario + same `StrategyBundle` + same dispatch seed → same run metrics (full reproducibility).
3. Scenario rows never mutate post-insert (assertion fires if any non-`archived_at` column changes).

### 14.2 Observability

- Bar fetches: existing flight-recorder trace adds `event = "alpaca.bars.fetch"` with `cache_key` + `bar_count`.
- Scenario CRUD: `audit::record` calls mirror the pattern already in `eval::run`.

---

## 15. Open questions (resolve during implementation)

- **Alpaca crypto-history boundary.** Spec uses 2021-09-26 as `alpaca_crypto_history_start()`. Verify against Alpaca docs at M1 start; some pairs (newer alts) have later listing dates — the whitelist may need per-asset history floors instead of a single constant.
- **`xvn migrate --dry-run` shape.** Phrase the output so an operator can preview the (capital, risk) row that lands on `canonical-defaults`. Decide between "verbose human" and "JSON for scripting" defaults.
- **Bar blob compression threshold.** Spec says "gzip for windows > 30d." Validate the threshold empirically — at 1h granularity 30d ≈ 720 bars ≈ ~50KB raw; gzip may not pay off until ~5 MB. Could be 365d.
- **Whitelist source of truth.** `ALPACA_CRYPTO_WHITELIST` as a compile-time const is simplest. Alternative: load at startup from a versioned `data/alpaca-crypto-assets.toml`. Defer to M1 implementation — depends on how often Alpaca adds pairs.
- **Wizard "Estimated bars to fetch" calculation.** Needs a cheap pre-flight ("8,760 bars · 1 API call"); decide whether to ping Alpaca's `/v1beta3/crypto/{loc}/assets` endpoint or compute purely from `(window, granularity)`.

---

## 16. Acceptance criteria

- **M1**: `xvn ab-compare --asset ETH --from 2024-02-03 --to 2025-02-03 --granularity 1h` runs successfully, fetching bars from Alpaca on first run and from the cache on subsequent runs. Re-runs are deterministic. `AlpacaPaperSurface` accepts ETH (paper mode) without runtime error.
- **M2**: `xvn scenario create` writes a new immutable row. `xvn eval run --scenario <new_id> --strategy default --mode backtest` produces a Run with metrics, pulling bars through the cache. Canonical scenarios are still resolvable by their original IDs. `StrategyBundle.capital` and `StrategyBundle.risk` populate from the moved data; existing runs still resolve.
- **M3**: `/scenarios/new` wizard creates a scenario end-to-end with no CLI involvement. "+ New scenario" inline form on `/eval-runs` produces the same row shape. Run launcher kicks off a Backtest mode run that lands on `/eval-runs` and updates live via progress events.

---

## 17. Related follow-ups

- **F30** — implementation tracker for this spec.
- **F31** — replay-mode follow-ups: Stepped (#1), Accelerated (#2), Realtime (#3, gated on live-paper mirror).
- **F18** — multi-asset `TraderDecision.asset` cascade; partially pulled into M1.
- **F25** — Claude Code skill; the scenario CLI surface lands as a section in that skill post-M2.
- **F27** — install customizer; the "Eval" pane in Settings mirrors this scenario surface read-only.
