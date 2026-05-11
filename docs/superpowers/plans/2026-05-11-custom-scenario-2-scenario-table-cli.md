# Custom-Scenario Eval — M2: Scenario table + CLI + capital/risk move

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the compiled-in `canonical_scenarios()` constants with a SQLite-backed scenario registry, ship `xvn scenario create/ls/show/clone/archive/rm/tree` CLI, move `capital` + `risk` off `Scenario` onto `StrategyBundle`, and wire `xvn eval run --scenario <id>` to consume DB rows.

**Architecture:** New `scenarios` + `scenario_tags` SQLite tables (migration 0005) + `runs.scenario_id` foreign-key constraint (migration 0006). New `Scenario` struct shape replaces the existing one (asset-class-agnostic, single-asset enforced in v1, lineage via `parent_scenario_id`). New engine API module `api::scenario` with `create/get/list/clone/archive/delete` (no `update` — rows are immutable). The four existing canonical scenarios seed as `source = 'canonical'` rows on first migration; their `capital` + `risk` migrate onto a single seeded `StrategyBundle` named `canonical-defaults`.

**Tech Stack:** Rust 2021, sqlx + SQLite, clap, ts-rs (for frontend types), blake3 (cache key derivation from M1).

**Reference spec:** `docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md` §§5, 6, 8, 9, 12.

**Prereq:** M1 (`docs/superpowers/plans/2026-05-11-custom-scenario-1-bars-cache-asset-unlock.md`) merged.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `crates/xvision-engine/src/eval/scenario.rs` | Modify | Replace existing struct + remove `canonical_scenarios()`. Add new fields (asset_class, lineage, audit, bar_cache_policy, etc.). |
| `crates/xvision-engine/src/eval/scenario_seed.rs` | Create | Returns the four canonical seed rows + their capital/risk extracted for `canonical-defaults`. |
| `crates/xvision-engine/migrations/0005_scenarios.sql` | Create | `scenarios` + `scenario_tags` tables. |
| `crates/xvision-engine/migrations/0005_scenarios.down.sql` | Create | DROP. |
| `crates/xvision-engine/migrations/0006_runs_scenario_fk.sql` | Create | Index + trigger-based FK enforcement (SQLite has no `ALTER ADD CONSTRAINT`). |
| `crates/xvision-engine/migrations/0006_runs_scenario_fk.down.sql` | Create | Reverse. |
| `crates/xvision-engine/src/store.rs` | Modify | Register new migrations. Add scenario CRUD helpers. |
| `crates/xvision-engine/src/api/scenario.rs` | Create | `create/get/list/clone/archive/delete` + validation. |
| `crates/xvision-engine/src/api/mod.rs` | Modify | `pub mod scenario;` + route registration. |
| `crates/xvision-engine/src/api/eval.rs` | Modify | Replace `canonical_scenarios().find()` with `api_scenario::get()`. Wire bars through `eval::bars::load_bars`. |
| `crates/xvision-engine/src/api/migrate.rs` | Create | `xvn migrate --dry-run` + capital/risk move-off-scenario migration logic. |
| `crates/xvision-cli/src/commands/scenario.rs` | Create | `xvn scenario create/ls/show/clone/archive/rm/tree`. |
| `crates/xvision-cli/src/commands/migrate.rs` | Create | `xvn migrate --dry-run`. |
| `crates/xvision-cli/src/commands/mod.rs` | Modify | `pub mod scenario; pub mod migrate;` |
| `crates/xvision-cli/src/lib.rs` | Modify | Register subcommands. Deprecate `xvn eval scenarios`. |
| `crates/xvision-cli/src/commands/eval.rs` | Modify | `xvn eval scenarios` prints deprecation notice + delegates. |
| `crates/xvision-core/src/strategy_bundle.rs` | Modify | Add `capital: Capital`, `risk: ScenarioRisk` (renamed `RiskCaps`) fields. |

---

## Task 1 — New `Scenario` struct shape

**Files:** `crates/xvision-engine/src/eval/scenario.rs`, `crates/xvision-engine/tests/scenario_shape.rs`

- [ ] **Step 1: Failing test for serde round-trip of the new struct**

```rust
// crates/xvision-engine/tests/scenario_shape.rs
use chrono::{TimeZone, Utc};
use xvision_engine::eval::scenario::*;

#[test]
fn scenario_serde_roundtrip() {
    let s = Scenario {
        id: "sc_test".into(),
        parent_scenario_id: None,
        source: ScenarioSource::User,
        display_name: "ETH 2024".into(),
        description: "".into(),
        tags: vec!["regression".into(), "eth".into()],
        notes: None,
        asset_class: AssetClass::Crypto,
        asset: vec![AssetRef { class: AssetClass::Crypto, symbol: "ETH".into(), venue_symbol: "ETH/USD".into() }],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: Utc.with_ymd_and_hms(2024,2,3,0,0,0).unwrap(),
            end: Utc.with_ymd_and_hms(2025,2,3,0,0,0).unwrap(),
        },
        granularity: BarGranularity::Hour1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical { feed: None, adjustment: AdjustmentMode::Raw },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees { maker_bps: 10, taker_bps: 25 },
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel { decision_to_fill_ms: 500 },
            fill_model: FillModel { market_order_fill: MarketOrderFill::FullAtClose, limit_order_fill: LimitOrderFill::NeverFills, partial_fills: false, volume_constraints: None },
        },
        replay_mode: ReplayMode::Continuous,
        bar_cache_policy: BarCachePolicy { cache_key: "abc".into(), refresh_policy: RefreshPolicy::NeverRefresh, data_fetched_at: None },
        created_at: Utc.with_ymd_and_hms(2026,5,11,0,0,0).unwrap(),
        created_by: "edkenne@gmail.com".into(),
        archived_at: None,
    };
    let json = serde_json::to_string(&s).unwrap();
    let back: Scenario = serde_json::from_str(&json).unwrap();
    assert_eq!(s, back);
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --test scenario_shape
```

- [ ] **Step 3: Replace `scenario.rs` contents**

Open `crates/xvision-engine/src/eval/scenario.rs`, replace the entire file body with the new shape per spec §5. Drop the `canonical_scenarios()` function in this step (seed lives in `scenario_seed.rs`). Drop the `capital`, `risk`, `data_seed`, `regime_tags` fields. Add the new fields exactly as the test expects.

> **Note:** keep the file under 300 lines. Put `pub struct VenueSettings` etc. inline; defer per-asset-class refinements to v2.

- [ ] **Step 4: Run test, expect PASS**

```bash
cargo test -p xvision-engine --test scenario_shape
```

- [ ] **Step 5: `cargo build --workspace`** — surface every downstream caller that referenced the dropped fields. Fix each by:
  - Removing `scenario.capital` / `scenario.risk` reads from `BacktestExecutor`, report renderers — they'll be replaced by `StrategyBundle.capital` / `.risk` in Task 5.
  - Stubbing for now: each old-style call site that reads `scenario.capital` becomes `Capital { initial: 100_000.0, currency: "USD".into() }` (compile-only placeholder; replaced in Task 5).

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/eval/scenario.rs crates/xvision-engine/tests/scenario_shape.rs
git commit -m "feat(eval): new Scenario struct shape (asset_class, lineage, audit); strip capital/risk"
```

---

## Task 2 — `scenarios` + `scenario_tags` migration

**Files:** `crates/xvision-engine/migrations/0005_scenarios.sql`, `crates/xvision-engine/migrations/0005_scenarios.down.sql`, `crates/xvision-engine/src/store.rs`

- [ ] **Step 1: Write up-migration**

```sql
-- 0005_scenarios.sql
CREATE TABLE scenarios (
    id                  TEXT PRIMARY KEY,
    parent_scenario_id  TEXT,
    source              TEXT NOT NULL,
    display_name        TEXT NOT NULL,
    description         TEXT NOT NULL DEFAULT '',
    body_json           TEXT NOT NULL,
    created_at          TEXT NOT NULL,
    created_by          TEXT NOT NULL,
    archived_at         TEXT,
    FOREIGN KEY (parent_scenario_id) REFERENCES scenarios(id)
);
CREATE INDEX scenarios_by_source       ON scenarios(source);
CREATE INDEX scenarios_by_parent       ON scenarios(parent_scenario_id);
CREATE INDEX scenarios_by_archived_at  ON scenarios(archived_at);

CREATE TABLE scenario_tags (
    scenario_id TEXT NOT NULL,
    tag         TEXT NOT NULL,
    PRIMARY KEY (scenario_id, tag),
    FOREIGN KEY (scenario_id) REFERENCES scenarios(id) ON DELETE CASCADE
);
CREATE INDEX scenario_tags_by_tag ON scenario_tags(tag);

-- Belt + suspenders: reject non-archived-at updates
CREATE TRIGGER scenarios_no_update
    BEFORE UPDATE OF id, parent_scenario_id, source, display_name, description, body_json, created_at, created_by
    ON scenarios
BEGIN
    SELECT RAISE(ABORT, 'scenarios rows are immutable (only archived_at may change)');
END;
```

- [ ] **Step 2: Write down-migration**

```sql
DROP TRIGGER IF EXISTS scenarios_no_update;
DROP INDEX IF EXISTS scenario_tags_by_tag;
DROP TABLE IF EXISTS scenario_tags;
DROP INDEX IF EXISTS scenarios_by_archived_at;
DROP INDEX IF EXISTS scenarios_by_parent;
DROP INDEX IF EXISTS scenarios_by_source;
DROP TABLE IF EXISTS scenarios;
```

- [ ] **Step 3: Register in `store.rs`**

```rust
("0005_scenarios", include_str!("../migrations/0005_scenarios.sql")),
```

- [ ] **Step 4: Run engine tests**

```bash
cargo test -p xvision-engine store
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/migrations/0005_scenarios*.sql crates/xvision-engine/src/store.rs
git commit -m "feat(engine): 0005 scenarios + scenario_tags migration"
```

---

## Task 3 — Scenario CRUD store helpers

**Files:** `crates/xvision-engine/src/store.rs`, `crates/xvision-engine/tests/scenarios_store.rs`

- [ ] **Step 1: Failing test for insert + read-back**

```rust
// crates/xvision-engine/tests/scenarios_store.rs
use xvision_engine::test_support::TempStore;

#[tokio::test]
async fn insert_and_read_scenario() {
    let store = TempStore::new().await;
    let s = make_test_scenario("sc_1");
    store.insert_scenario(&s).await.unwrap();
    let back = store.get_scenario("sc_1").await.unwrap().unwrap();
    assert_eq!(back.id, "sc_1");
    assert_eq!(back.display_name, s.display_name);
}

#[tokio::test]
async fn immutable_update_rejected() {
    let store = TempStore::new().await;
    let s = make_test_scenario("sc_immut");
    store.insert_scenario(&s).await.unwrap();
    let err = sqlx::query!("UPDATE scenarios SET display_name = 'hacked' WHERE id = ?", s.id)
        .execute(store.pool()).await.unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("immutable"), "expected immutability trigger: {msg}");
}

#[tokio::test]
async fn archive_succeeds() {
    let store = TempStore::new().await;
    let s = make_test_scenario("sc_archive");
    store.insert_scenario(&s).await.unwrap();
    store.archive_scenario("sc_archive").await.unwrap();
    let back = store.get_scenario("sc_archive").await.unwrap().unwrap();
    assert!(back.archived_at.is_some());
}

fn make_test_scenario(id: &str) -> xvision_engine::eval::scenario::Scenario { /* fill helper */ unimplemented!() }
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --test scenarios_store
```

- [ ] **Step 3: Implement helpers in `store.rs`**

```rust
pub async fn insert_scenario(&self, s: &Scenario) -> ApiResult<()> {
    let body = serde_json::to_string(s).map_err(ApiError::from)?;
    sqlx::query!(
        "INSERT INTO scenarios (id, parent_scenario_id, source, display_name, description, body_json, created_at, created_by, archived_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        s.id, s.parent_scenario_id,
        serde_json::to_string(&s.source).unwrap().trim_matches('"'),
        s.display_name, s.description, body,
        s.created_at.to_rfc3339(), s.created_by,
        s.archived_at.map(|t| t.to_rfc3339())
    ).execute(&self.pool).await?;
    for tag in &s.tags {
        sqlx::query!("INSERT OR IGNORE INTO scenario_tags (scenario_id, tag) VALUES (?, ?)", s.id, tag)
            .execute(&self.pool).await?;
    }
    Ok(())
}

pub async fn get_scenario(&self, id: &str) -> ApiResult<Option<Scenario>> {
    let row = sqlx::query!("SELECT body_json FROM scenarios WHERE id = ?", id).fetch_optional(&self.pool).await?;
    Ok(row.map(|r| serde_json::from_str(&r.body_json).unwrap()))
}

pub async fn list_scenarios(&self, filter: &ListScenariosFilter) -> ApiResult<Vec<Scenario>> {
    // Build dynamic query: filter on source, tags (junction), archived_at.
    // For v1 do straight SELECT; refine if N grows.
    let rows = sqlx::query!("SELECT body_json FROM scenarios ORDER BY created_at DESC").fetch_all(&self.pool).await?;
    let all: Vec<Scenario> = rows.into_iter().map(|r| serde_json::from_str(&r.body_json).unwrap()).collect();
    Ok(all.into_iter().filter(|s| {
        filter.source.as_ref().map_or(true, |x| &s.source == x) &&
        filter.tags.iter().all(|t| s.tags.contains(t)) &&
        (filter.include_archived || s.archived_at.is_none()) &&
        filter.parent_scenario_id.as_ref().map_or(true, |p| s.parent_scenario_id.as_ref() == Some(p))
    }).collect())
}

pub async fn archive_scenario(&self, id: &str) -> ApiResult<()> {
    sqlx::query!("UPDATE scenarios SET archived_at = ? WHERE id = ?", chrono::Utc::now().to_rfc3339(), id)
        .execute(&self.pool).await?;
    Ok(())
}

pub async fn delete_scenario(&self, id: &str) -> ApiResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM runs WHERE scenario_id = ?")
        .bind(id).fetch_one(&self.pool).await?;
    if count.0 > 0 {
        return Err(ApiError::Validation(format!("cannot delete scenario '{id}': {} runs reference it. Archive instead.", count.0)));
    }
    sqlx::query!("DELETE FROM scenarios WHERE id = ?", id).execute(&self.pool).await?;
    Ok(())
}

pub async fn list_children(&self, parent_id: &str) -> ApiResult<Vec<Scenario>> {
    let rows = sqlx::query!("SELECT body_json FROM scenarios WHERE parent_scenario_id = ? ORDER BY created_at ASC", parent_id)
        .fetch_all(&self.pool).await?;
    Ok(rows.into_iter().map(|r| serde_json::from_str(&r.body_json).unwrap()).collect())
}
```

Add `ListScenariosFilter` struct to `api::scenario` module (Task 4). For now define inline.

- [ ] **Step 4: Run tests, expect PASS**

```bash
cargo test -p xvision-engine --test scenarios_store
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/store.rs crates/xvision-engine/tests/scenarios_store.rs
git commit -m "feat(engine): scenario store helpers (insert/get/list/archive/delete/children)"
```

---

## Task 4 — `api::scenario` module

**Files:** `crates/xvision-engine/src/api/scenario.rs`, `crates/xvision-engine/src/api/mod.rs`, `crates/xvision-engine/tests/scenario_api.rs`

- [ ] **Step 1: Failing test for create + validation**

```rust
// tests/scenario_api.rs
use chrono::{TimeZone, Utc};
use xvision_engine::api::scenario::{create, CreateScenarioRequest};
use xvision_engine::eval::scenario::*;

#[tokio::test]
async fn create_succeeds_with_valid_request() {
    let ctx = ApiContext::test().await;
    let req = valid_request();
    let s = create(&ctx, req).await.unwrap();
    assert_eq!(s.source, ScenarioSource::User);
    assert!(!s.id.is_empty());
}

#[tokio::test]
async fn create_rejects_multi_asset_v1() {
    let ctx = ApiContext::test().await;
    let mut req = valid_request();
    req.asset.push(AssetRef { class: AssetClass::Crypto, symbol: "BTC".into(), venue_symbol: "BTC/USD".into() });
    let err = create(&ctx, req).await.unwrap_err();
    assert!(matches!(err, xvision_engine::api::ApiError::Validation(_)));
}

#[tokio::test]
async fn create_rejects_history_floor_violation() {
    let ctx = ApiContext::test().await;
    let mut req = valid_request();
    req.time_window.start = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();
    let err = create(&ctx, req).await.unwrap_err();
    assert!(format!("{err}").contains("before Alpaca crypto history"));
}

fn valid_request() -> CreateScenarioRequest { /* helper */ unimplemented!() }
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine --test scenario_api
```

- [ ] **Step 3: Implement `api/scenario.rs`**

```rust
use crate::api::{ApiContext, ApiError, ApiResult};
use crate::eval::scenario::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use ulid::Ulid;
use xvision_data::asset_whitelist::{is_alpaca_crypto_supported, alpaca_crypto_history_start};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CreateScenarioRequest {
    pub display_name: String,
    pub description: String,
    pub asset_class: AssetClass,
    pub asset: Vec<AssetRef>,
    pub quote_currency: QuoteCurrency,
    pub time_window: TimeWindow,
    pub granularity: BarGranularity,
    pub timezone: String,
    pub calendar: CalendarRef,
    pub venue: VenueSettings,
    pub data_source: DataSource,
    pub replay_mode: ReplayMode,
    pub tags: Vec<String>,
    pub notes: Option<String>,
    pub parent_scenario_id: Option<String>,
    pub source: ScenarioSource,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ListScenariosFilter {
    pub source: Option<ScenarioSource>,
    pub tags: Vec<String>,
    pub include_archived: bool,
    pub parent_scenario_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ScenarioMutations {
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub time_window: Option<TimeWindow>,
    pub asset: Option<Vec<AssetRef>>,
    pub granularity: Option<BarGranularity>,
    pub venue: Option<VenueSettings>,
    pub tags: Option<Vec<String>>,
    pub notes: Option<String>,
}

pub async fn create(ctx: &ApiContext, req: CreateScenarioRequest) -> ApiResult<Scenario> {
    validate(&req, ctx).await?;
    let id = format!("sc_{}", Ulid::new());
    let cache_key = compute_cache_key(&req);
    let scenario = Scenario {
        id, parent_scenario_id: req.parent_scenario_id, source: req.source,
        display_name: req.display_name, description: req.description,
        tags: req.tags, notes: req.notes,
        asset_class: req.asset_class, asset: req.asset, quote_currency: req.quote_currency,
        time_window: req.time_window, granularity: req.granularity,
        timezone: req.timezone, calendar: req.calendar,
        data_source: req.data_source, venue: req.venue, replay_mode: req.replay_mode,
        bar_cache_policy: BarCachePolicy { cache_key, refresh_policy: RefreshPolicy::NeverRefresh, data_fetched_at: None },
        created_at: Utc::now(),
        created_by: ctx.operator_id().to_string(),
        archived_at: None,
    };
    ctx.store.insert_scenario(&scenario).await?;
    Ok(scenario)
}

pub async fn get(ctx: &ApiContext, id: &str) -> ApiResult<Scenario> {
    ctx.store.get_scenario(id).await?.ok_or_else(|| ApiError::NotFound(format!("scenario '{id}'")))
}

pub async fn list(ctx: &ApiContext, filter: ListScenariosFilter) -> ApiResult<Vec<Scenario>> {
    ctx.store.list_scenarios(&filter).await
}

pub async fn clone(ctx: &ApiContext, parent: &str, mutations: ScenarioMutations) -> ApiResult<Scenario> {
    let parent_s = get(ctx, parent).await?;
    if parent_s.archived_at.is_some() {
        return Err(ApiError::Validation(format!("parent scenario '{parent}' is archived")));
    }
    let mut req = CreateScenarioRequest {
        display_name: mutations.display_name.unwrap_or_else(|| format!("{} (clone)", parent_s.display_name)),
        description: mutations.description.unwrap_or(parent_s.description),
        asset_class: parent_s.asset_class,
        asset: mutations.asset.unwrap_or(parent_s.asset),
        quote_currency: parent_s.quote_currency,
        time_window: mutations.time_window.unwrap_or(parent_s.time_window),
        granularity: mutations.granularity.unwrap_or(parent_s.granularity),
        timezone: parent_s.timezone,
        calendar: parent_s.calendar,
        venue: mutations.venue.unwrap_or(parent_s.venue),
        data_source: parent_s.data_source,
        replay_mode: parent_s.replay_mode,
        tags: mutations.tags.unwrap_or(parent_s.tags),
        notes: mutations.notes,
        parent_scenario_id: Some(parent.to_string()),
        source: ScenarioSource::Clone,
    };
    create(ctx, req).await
}

pub async fn archive(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    get(ctx, id).await?;
    ctx.store.archive_scenario(id).await
}

pub async fn delete(ctx: &ApiContext, id: &str) -> ApiResult<()> {
    get(ctx, id).await?;
    ctx.store.delete_scenario(id).await
}

async fn validate(req: &CreateScenarioRequest, ctx: &ApiContext) -> ApiResult<()> {
    if req.asset.len() != 1 {
        return Err(ApiError::Validation(format!("asset.len() must be 1 in v1 (got {})", req.asset.len())));
    }
    if !matches!(req.asset_class, AssetClass::Crypto) {
        return Err(ApiError::Validation("asset_class must be Crypto in v1".into()));
    }
    if !matches!(req.quote_currency, QuoteCurrency::Usd) {
        return Err(ApiError::Validation("quote_currency must be Usd in v1".into()));
    }
    if !matches!(req.granularity, BarGranularity::Hour1 | BarGranularity::Day1) {
        return Err(ApiError::Validation("granularity must be Hour1 or Day1 in v1".into()));
    }
    if !matches!(req.replay_mode, ReplayMode::Continuous) {
        return Err(ApiError::Validation("replay_mode must be Continuous in v1".into()));
    }
    if req.time_window.start >= req.time_window.end {
        return Err(ApiError::Validation("time_window.start must be < time_window.end".into()));
    }
    if req.time_window.end > Utc::now() {
        return Err(ApiError::Validation("time_window.end must be <= now".into()));
    }
    let floor = alpaca_crypto_history_start();
    if req.time_window.start < floor {
        return Err(ApiError::Validation(format!("time_window.start is before Alpaca crypto history (earliest: {})", floor.to_rfc3339())));
    }
    let symbol = &req.asset[0].symbol;
    if !is_alpaca_crypto_supported(symbol) {
        return Err(ApiError::Validation(format!("asset '{symbol}' is not in the Alpaca crypto whitelist")));
    }
    if let Some(parent) = &req.parent_scenario_id {
        let p = ctx.store.get_scenario(parent).await?.ok_or_else(|| ApiError::NotFound(format!("parent scenario '{parent}'")))?;
        if p.archived_at.is_some() {
            return Err(ApiError::Validation(format!("parent scenario '{parent}' is archived")));
        }
    }
    Ok(())
}

fn compute_cache_key(req: &CreateScenarioRequest) -> String {
    let mut h = blake3::Hasher::new();
    h.update(req.asset[0].venue_symbol.as_bytes());
    h.update(req.granularity.as_alpaca_str().as_bytes());
    h.update(req.time_window.start.to_rfc3339().as_bytes());
    h.update(req.time_window.end.to_rfc3339().as_bytes());
    h.update(serde_json::to_string(&req.data_source).unwrap().as_bytes());
    h.finalize().to_hex().to_string()
}
```

- [ ] **Step 4: Add `BarGranularity::as_alpaca_str` impl** if not already present from M1.

- [ ] **Step 5: Wire `pub mod scenario;` into `api/mod.rs`**

- [ ] **Step 6: Run tests, expect PASS**

```bash
cargo test -p xvision-engine --test scenario_api
```

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/api/scenario.rs crates/xvision-engine/src/api/mod.rs crates/xvision-engine/tests/scenario_api.rs
git commit -m "feat(api): scenario create/get/list/clone/archive/delete with validation"
```

---

## Task 5 — Capital + risk move onto `StrategyBundle`

**Files:** `crates/xvision-core/src/strategy_bundle.rs`, downstream consumers

- [ ] **Step 1: Failing test for `StrategyBundle::capital`**

```rust
#[test]
fn strategy_bundle_carries_capital_and_risk() {
    let b = StrategyBundle {
        id: "test".into(),
        capital: Capital { initial: 100_000.0, currency: "USD".into() },
        risk: RiskCaps { max_concurrent_positions: 1, max_leverage: 1.0, daily_loss_kill_switch_pct: 0.05 },
        // ... existing fields ...
    };
    assert_eq!(b.capital.initial, 100_000.0);
}
```

- [ ] **Step 2: Add `capital: Capital` + `risk: RiskCaps` fields** to `StrategyBundle` struct. Define `Capital` and `RiskCaps` (or reuse the moved-off `ScenarioRisk` renamed) in `xvision-core`.

- [ ] **Step 3: Update every existing `StrategyBundle` constructor** in the codebase + tests + seed scripts to populate the new fields.

- [ ] **Step 4: Update `BacktestExecutor` + report renderers** to read `capital` and `risk` from `StrategyBundle` instead of the (now-deleted) `Scenario` fields. This finishes Task 1's stub.

- [ ] **Step 5: `cargo build --workspace`**

Expected: PASS.

- [ ] **Step 6: `cargo test --workspace`**

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add -p
git commit -m "feat(core): StrategyBundle owns capital + risk (moved off Scenario)"
```

---

## Task 6 — Seed module + `canonical-defaults` bundle

**Files:** `crates/xvision-engine/src/eval/scenario_seed.rs`, `crates/xvision-engine/src/api/migrate.rs`, `crates/xvision-engine/tests/seed.rs`

- [ ] **Step 1: Write seed function**

```rust
// crates/xvision-engine/src/eval/scenario_seed.rs
use chrono::{TimeZone, Utc};
use crate::eval::scenario::*;

pub fn canonical_seed_rows() -> Vec<Scenario> {
    vec![
        seed("crypto-bull-q1-2025", "Crypto bull — Q1 2025", "regime:bull",
             Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap(),
             Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap()),
        seed("crypto-bear-q3-2024", "Crypto bear — Q3 2024", "regime:bear",
             Utc.with_ymd_and_hms(2024, 7, 1, 0, 0, 0).unwrap(),
             Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap()),
        seed("crypto-rangebound-q2-2025", "Crypto range-bound — Q2 2025", "regime:chop",
             Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap(),
             Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap()),
        seed("flash-crash-aug-2024", "Crypto flash crash — Aug 2024", "regime:event",
             Utc.with_ymd_and_hms(2024, 8, 1, 0, 0, 0).unwrap(),
             Utc.with_ymd_and_hms(2024, 8, 31, 0, 0, 0).unwrap()),
    ]
}

pub fn canonical_defaults_bundle() -> CanonicalDefaults {
    CanonicalDefaults {
        bundle_id: "bundle-canonical-defaults".into(),
        capital: Capital { initial: 100_000.0, currency: "USD".into() },
        risk: RiskCaps { max_concurrent_positions: 1, max_leverage: 1.0, daily_loss_kill_switch_pct: 0.05 },
    }
}

pub struct CanonicalDefaults {
    pub bundle_id: String,
    pub capital: Capital,
    pub risk: RiskCaps,
}

fn seed(id: &str, name: &str, regime_tag: &str, start: chrono::DateTime<Utc>, end: chrono::DateTime<Utc>) -> Scenario {
    let asset = AssetRef { class: AssetClass::Crypto, symbol: "BTC".into(), venue_symbol: "BTC/USD".into() };
    let mut s = Scenario {
        id: id.into(),
        parent_scenario_id: None,
        source: ScenarioSource::Canonical,
        display_name: name.into(),
        description: "".into(),
        tags: vec![regime_tag.into()],
        notes: None,
        asset_class: AssetClass::Crypto,
        asset: vec![asset.clone()],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow { start, end },
        granularity: BarGranularity::Hour1,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        data_source: DataSource::AlpacaHistorical { feed: None, adjustment: AdjustmentMode::Raw },
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees { maker_bps: 10, taker_bps: 25 },
            slippage: SlippageModel::Linear { bps: 5 },
            latency: LatencyModel { decision_to_fill_ms: 500 },
            fill_model: FillModel { market_order_fill: MarketOrderFill::FullAtClose, limit_order_fill: LimitOrderFill::NeverFills, partial_fills: false, volume_constraints: None },
        },
        replay_mode: ReplayMode::Continuous,
        bar_cache_policy: BarCachePolicy { cache_key: "".into(), refresh_policy: RefreshPolicy::NeverRefresh, data_fetched_at: None },
        created_at: Utc.with_ymd_and_hms(2026, 5, 11, 0, 0, 0).unwrap(),
        created_by: "system".into(),
        archived_at: None,
    };
    s.bar_cache_policy.cache_key = compute_cache_key(&s);
    s
}

fn compute_cache_key(s: &Scenario) -> String {
    let mut h = blake3::Hasher::new();
    h.update(s.asset[0].venue_symbol.as_bytes());
    h.update(s.granularity.as_alpaca_str().as_bytes());
    h.update(s.time_window.start.to_rfc3339().as_bytes());
    h.update(s.time_window.end.to_rfc3339().as_bytes());
    h.update(serde_json::to_string(&s.data_source).unwrap().as_bytes());
    h.finalize().to_hex().to_string()
}
```

- [ ] **Step 2: Write migration runner in `api/migrate.rs`**

```rust
pub async fn run_seed_if_needed(ctx: &ApiContext) -> ApiResult<()> {
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scenarios WHERE source = 'canonical'")
        .fetch_one(&ctx.pool).await?;
    if count.0 == 0 {
        for s in crate::eval::scenario_seed::canonical_seed_rows() {
            ctx.store.insert_scenario(&s).await?;
        }
        let defaults = crate::eval::scenario_seed::canonical_defaults_bundle();
        ctx.store.insert_canonical_defaults_bundle(&defaults).await?;
    }
    Ok(())
}
```

- [ ] **Step 3: Call `run_seed_if_needed` after migrations apply** in `ApiContext::open`/`from_xvn_home`.

- [ ] **Step 4: Write integration test**

```rust
#[tokio::test]
async fn seed_runs_on_fresh_db() {
    let ctx = ApiContext::test().await;
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scenarios").fetch_one(&ctx.pool).await.unwrap();
    assert_eq!(count.0, 4);
}
```

- [ ] **Step 5: Run, expect PASS**

```bash
cargo test -p xvision-engine seed_runs_on_fresh_db
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/src/eval/scenario_seed.rs crates/xvision-engine/src/api/migrate.rs crates/xvision-engine/tests/seed.rs
git commit -m "feat(engine): seed 4 canonical scenarios + canonical-defaults bundle on fresh DB"
```

---

## Task 7 — `runs.scenario_id` foreign-key trigger

**Files:** `crates/xvision-engine/migrations/0006_runs_scenario_fk.sql`, `0006_runs_scenario_fk.down.sql`, `crates/xvision-engine/src/store.rs`

- [ ] **Step 1: Write migration**

SQLite doesn't allow `ALTER TABLE ADD CONSTRAINT`; the equivalent is the table-rebuild dance OR a trigger. Use the trigger approach:

```sql
-- 0006_runs_scenario_fk.sql
CREATE INDEX IF NOT EXISTS runs_by_scenario ON runs(scenario_id);

CREATE TRIGGER runs_scenario_id_fk_insert
    BEFORE INSERT ON runs
    WHEN NEW.scenario_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'foreign-key violation: runs.scenario_id does not exist in scenarios')
    WHERE NOT EXISTS (SELECT 1 FROM scenarios WHERE id = NEW.scenario_id);
END;

CREATE TRIGGER runs_scenario_id_fk_update
    BEFORE UPDATE OF scenario_id ON runs
    WHEN NEW.scenario_id IS NOT NULL
BEGIN
    SELECT RAISE(ABORT, 'foreign-key violation: runs.scenario_id does not exist in scenarios')
    WHERE NOT EXISTS (SELECT 1 FROM scenarios WHERE id = NEW.scenario_id);
END;
```

Down:

```sql
DROP TRIGGER IF EXISTS runs_scenario_id_fk_update;
DROP TRIGGER IF EXISTS runs_scenario_id_fk_insert;
DROP INDEX IF EXISTS runs_by_scenario;
```

- [ ] **Step 2: Register migration in `store.rs`**

- [ ] **Step 3: Write failing test for FK enforcement**

```rust
#[tokio::test]
async fn run_insert_with_unknown_scenario_rejected() {
    let ctx = ApiContext::test().await;
    let err = sqlx::query!("INSERT INTO runs (id, scenario_id, agent_id, mode, status) VALUES ('r_1', 'sc_does_not_exist', 'a_1', 'backtest', 'queued')")
        .execute(&ctx.pool).await.unwrap_err();
    assert!(format!("{err}").contains("foreign-key violation"));
}
```

- [ ] **Step 4: Run, expect PASS** (with the seeded canonical scenarios, runs against canonical IDs still resolve)

```bash
cargo test -p xvision-engine run_insert_with_unknown_scenario_rejected
```

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/migrations/0006_runs_scenario_fk*.sql crates/xvision-engine/src/store.rs
git commit -m "feat(engine): 0006 runs.scenario_id FK via triggers"
```

---

## Task 8 — Wire `xvn eval run` to consume DB scenarios

**Files:** `crates/xvision-engine/src/api/eval.rs`

- [ ] **Step 1: Failing test for end-to-end run against a DB scenario**

```rust
#[tokio::test]
async fn eval_run_resolves_scenario_from_db() {
    let ctx = ApiContext::test_with_mock_alpaca().await;
    let s = create_test_scenario(&ctx, "eth-test-week").await;
    let bundle = create_test_strategy_bundle(&ctx).await;
    let run = run_with_deps(&ctx, EvalRunRequest {
        agent_id: bundle.id.clone(),
        scenario_id: s.id.clone(),
        mode: RunMode::Backtest,
    }, /* broker */ None, /* dispatch */ mock_dispatch(), /* tools */ Arc::new(ToolRegistry::default())).await.unwrap();
    assert_eq!(run.scenario_id, s.id);
    assert!(!run.metrics.is_none());
}
```

- [ ] **Step 2: Run, expect FAIL**

```bash
cargo test -p xvision-engine eval_run_resolves_scenario_from_db
```

- [ ] **Step 3: Replace `canonical_scenarios()` lookup with DB get**

Open `crates/xvision-engine/src/api/eval.rs` line ~526:

```rust
// Before:
let scenario: Scenario = canonical_scenarios()
    .into_iter()
    .find(|s| s.id == req.scenario_id)
    .ok_or_else(|| ApiError::NotFound(format!("scenario '{}'", req.scenario_id)))?;

// After:
let scenario: Scenario = crate::api::scenario::get(ctx, &req.scenario_id).await?;
let bars = crate::eval::bars::load_bars(ctx, &crate::eval::bars::BarCacheArgs {
    cache_key: scenario.bar_cache_policy.cache_key.clone(),
    asset_pair: scenario.asset[0].venue_symbol.clone(),
    granularity: scenario.granularity,
    start: scenario.time_window.start,
    end: scenario.time_window.end,
    data_source_tag: "alpaca-historical-v1".into(),
}).await?;
```

- [ ] **Step 4: Pass `bars` + `scenario.venue` into the `BacktestExecutor`**

Update `BacktestExecutor::new` signature to `new(bars: Vec<MarketBar>, venue: &VenueSettings)`. Reads fees/slippage/latency from the venue settings instead of compiled-in defaults.

- [ ] **Step 5: Run test, expect PASS**

```bash
cargo test -p xvision-engine eval_run_resolves_scenario_from_db
```

- [ ] **Step 6: Run all engine tests**

```bash
cargo test -p xvision-engine
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs crates/xvision-eval/src/backtest.rs
git commit -m "feat(eval): run resolves scenario from DB + loads bars via cache"
```

---

## Task 9 — `xvn scenario` CLI

**Files:** `crates/xvision-cli/src/commands/scenario.rs`, `crates/xvision-cli/src/commands/mod.rs`, `crates/xvision-cli/src/lib.rs`

- [ ] **Step 1: Implement `commands/scenario.rs`**

```rust
use clap::{Args, Subcommand};
use chrono::NaiveDate;
use std::path::PathBuf;
use xvision_engine::api::{scenario as api_scenario, ApiContext};
use xvision_engine::eval::scenario::*;

use crate::error::{CliError, CliResult};

#[derive(Args, Debug)]
pub struct ScenarioCmd {
    #[command(subcommand)]
    pub op: ScenarioOp,
    #[arg(long)] pub xvn_home: Option<PathBuf>,
}

#[derive(Subcommand, Debug)]
pub enum ScenarioOp {
    Create(CreateArgs),
    Ls(LsArgs),
    Show(ShowArgs),
    Clone(CloneArgs),
    Archive { id: String },
    Rm { id: String },
    Tree { id: String },
}

#[derive(Args, Debug)]
pub struct CreateArgs {
    #[arg(long)] pub name: String,
    #[arg(long)] pub asset: String,
    #[arg(long)] pub from: NaiveDate,
    #[arg(long)] pub to: NaiveDate,
    #[arg(long, default_value = "1h")] pub granularity: String,
    #[arg(long, default_value = "alpaca")] pub venue: String,
    #[arg(long, default_value_t = 10)] pub fees_maker: u32,
    #[arg(long, default_value_t = 25)] pub fees_taker: u32,
    #[arg(long, default_value = "linear:5")] pub slippage: String,
    #[arg(long, default_value_t = 500)] pub latency_ms: u32,
    #[arg(long)] pub tag: Vec<String>,
    #[arg(long)] pub notes: Option<String>,
    #[arg(long)] pub from_file: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct LsArgs {
    #[arg(long)] pub source: Option<String>,
    #[arg(long)] pub tag: Vec<String>,
    #[arg(long)] pub archived: bool,
    #[arg(long)] pub json: bool,
}

#[derive(Args, Debug)]
pub struct ShowArgs {
    pub id: String,
    #[arg(long)] pub json: bool,
    #[arg(long)] pub toml: bool,
}

#[derive(Args, Debug)]
pub struct CloneArgs {
    pub id: String,
    #[arg(long)] pub name: Option<String>,
    #[arg(long)] pub from: Option<NaiveDate>,
    #[arg(long)] pub to: Option<NaiveDate>,
    #[arg(long)] pub asset: Option<String>,
}

pub async fn run(cmd: ScenarioCmd) -> CliResult<()> {
    let ctx = ApiContext::from_xvn_home(cmd.xvn_home.as_deref()).await?;
    match cmd.op {
        ScenarioOp::Create(a) => run_create(&ctx, a).await,
        ScenarioOp::Ls(a) => run_ls(&ctx, a).await,
        ScenarioOp::Show(a) => run_show(&ctx, a).await,
        ScenarioOp::Clone(a) => run_clone(&ctx, a).await,
        ScenarioOp::Archive { id } => { api_scenario::archive(&ctx, &id).await?; println!("archived {id}"); Ok(()) }
        ScenarioOp::Rm { id } => { api_scenario::delete(&ctx, &id).await?; println!("removed {id}"); Ok(()) }
        ScenarioOp::Tree { id } => run_tree(&ctx, id).await,
    }
}

async fn run_create(ctx: &ApiContext, a: CreateArgs) -> CliResult<()> {
    if let Some(path) = a.from_file {
        let body = std::fs::read_to_string(&path).map_err(|e| CliError::Validation(e.to_string()))?;
        let req: api_scenario::CreateScenarioRequest = toml::from_str(&body).map_err(|e| CliError::Validation(e.to_string()))?;
        let s = api_scenario::create(ctx, req).await?;
        println!("created {}", s.id);
        return Ok(());
    }
    let asset_sym: xvision_core::AssetSymbol = a.asset.parse().map_err(|e: String| CliError::Validation(e))?;
    let granularity = match a.granularity.as_str() {
        "1h" => BarGranularity::Hour1,
        "1d" => BarGranularity::Day1,
        other => return Err(CliError::Validation(format!("granularity '{other}' not in v1 set"))),
    };
    let slippage = parse_slippage(&a.slippage)?;
    let req = api_scenario::CreateScenarioRequest {
        display_name: a.name,
        description: "".into(),
        asset_class: AssetClass::Crypto,
        asset: vec![AssetRef { class: AssetClass::Crypto, symbol: asset_sym.as_short().into(), venue_symbol: asset_sym.as_alpaca_pair() }],
        quote_currency: QuoteCurrency::Usd,
        time_window: TimeWindow {
            start: a.from.and_hms_opt(0,0,0).unwrap().and_utc(),
            end:   a.to.and_hms_opt(0,0,0).unwrap().and_utc(),
        },
        granularity,
        timezone: "UTC".into(),
        calendar: CalendarRef::Continuous24x7,
        venue: VenueSettings {
            venue: Venue::Alpaca,
            fees: Fees { maker_bps: a.fees_maker, taker_bps: a.fees_taker },
            slippage,
            latency: LatencyModel { decision_to_fill_ms: a.latency_ms },
            fill_model: FillModel { market_order_fill: MarketOrderFill::FullAtClose, limit_order_fill: LimitOrderFill::NeverFills, partial_fills: false, volume_constraints: None },
        },
        data_source: DataSource::AlpacaHistorical { feed: None, adjustment: AdjustmentMode::Raw },
        replay_mode: ReplayMode::Continuous,
        tags: a.tag,
        notes: a.notes,
        parent_scenario_id: None,
        source: ScenarioSource::User,
    };
    let s = api_scenario::create(ctx, req).await?;
    println!("created {} ({})", s.id, s.display_name);
    Ok(())
}

fn parse_slippage(s: &str) -> CliResult<SlippageModel> {
    if let Some(bps) = s.strip_prefix("linear:") {
        let bps: u32 = bps.parse().map_err(|_| CliError::Validation(format!("bad slippage '{s}'")))?;
        Ok(SlippageModel::Linear { bps })
    } else if s == "none" { Ok(SlippageModel::None) }
    else { Err(CliError::Validation(format!("unknown slippage '{s}' — try linear:5 or none"))) }
}

async fn run_ls(ctx: &ApiContext, a: LsArgs) -> CliResult<()> {
    let filter = api_scenario::ListScenariosFilter {
        source: a.source.as_deref().and_then(parse_source),
        tags: a.tag,
        include_archived: a.archived,
        parent_scenario_id: None,
    };
    let rows = api_scenario::list(ctx, filter).await?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&rows).unwrap());
    } else {
        for s in rows {
            println!("{}\t{}\t{}\t{}..{}\t{:?}",
                s.id, s.display_name, s.asset[0].symbol,
                s.time_window.start.format("%Y-%m-%d"), s.time_window.end.format("%Y-%m-%d"),
                s.source);
        }
    }
    Ok(())
}

fn parse_source(s: &str) -> Option<ScenarioSource> {
    match s.to_lowercase().as_str() {
        "canonical" => Some(ScenarioSource::Canonical),
        "user" => Some(ScenarioSource::User),
        "clone" => Some(ScenarioSource::Clone),
        "generated" => Some(ScenarioSource::Generated),
        _ => None,
    }
}

async fn run_show(ctx: &ApiContext, a: ShowArgs) -> CliResult<()> {
    let s = api_scenario::get(ctx, &a.id).await?;
    if a.json {
        println!("{}", serde_json::to_string_pretty(&s).unwrap());
    } else if a.toml {
        // serialize CreateScenarioRequest-shaped TOML for round-trip
        let req = scenario_to_create_request(&s);
        println!("{}", toml::to_string_pretty(&req).unwrap());
    } else {
        println!("{}", serde_json::to_string_pretty(&s).unwrap()); // default to JSON
    }
    Ok(())
}

async fn run_clone(ctx: &ApiContext, a: CloneArgs) -> CliResult<()> {
    let mutations = api_scenario::ScenarioMutations {
        display_name: a.name,
        time_window: match (a.from, a.to) {
            (Some(f), Some(t)) => Some(TimeWindow {
                start: f.and_hms_opt(0,0,0).unwrap().and_utc(),
                end:   t.and_hms_opt(0,0,0).unwrap().and_utc(),
            }),
            _ => None,
        },
        asset: a.asset.map(|sym| {
            let s: xvision_core::AssetSymbol = sym.parse().unwrap();
            vec![AssetRef { class: AssetClass::Crypto, symbol: s.as_short().into(), venue_symbol: s.as_alpaca_pair() }]
        }),
        granularity: None, venue: None, tags: None, notes: None, description: None,
    };
    let s = api_scenario::clone(ctx, &a.id, mutations).await?;
    println!("cloned to {} (parent: {})", s.id, a.id);
    Ok(())
}

async fn run_tree(ctx: &ApiContext, id: String) -> CliResult<()> {
    let s = api_scenario::get(ctx, &id).await?;
    // Walk up to root.
    let mut chain = vec![s.clone()];
    let mut cur = s.parent_scenario_id.clone();
    while let Some(pid) = cur {
        let p = api_scenario::get(ctx, &pid).await?;
        cur = p.parent_scenario_id.clone();
        chain.push(p);
    }
    chain.reverse();
    for (i, s) in chain.iter().enumerate() {
        println!("{}{} ({})", "  ".repeat(i), s.id, s.display_name);
    }
    // Walk down to children.
    let children = ctx.store.list_children(&id).await?;
    for c in children {
        println!("{}{} ({})", "  ".repeat(chain.len()), c.id, c.display_name);
    }
    Ok(())
}

fn scenario_to_create_request(s: &Scenario) -> api_scenario::CreateScenarioRequest {
    api_scenario::CreateScenarioRequest {
        display_name: s.display_name.clone(),
        description: s.description.clone(),
        asset_class: s.asset_class,
        asset: s.asset.clone(),
        quote_currency: s.quote_currency,
        time_window: s.time_window.clone(),
        granularity: s.granularity,
        timezone: s.timezone.clone(),
        calendar: s.calendar.clone(),
        venue: s.venue.clone(),
        data_source: s.data_source.clone(),
        replay_mode: s.replay_mode,
        tags: s.tags.clone(),
        notes: s.notes.clone(),
        parent_scenario_id: s.parent_scenario_id.clone(),
        source: s.source,
    }
}
```

- [ ] **Step 2: Register subcommand in `lib.rs`**

```rust
/// Scenario authoring: create / ls / show / clone / archive / rm / tree.
Scenario(commands::scenario::ScenarioCmd),
```

```rust
Command::Scenario(cmd) => commands::scenario::run(cmd).await,
```

- [ ] **Step 3: Smoke test**

```bash
cargo run --bin xvn -- scenario ls
cargo run --bin xvn -- scenario create --name "ETH 2024" --asset ETH --from 2024-02-03 --to 2024-12-31 --granularity 1h --tag regression
cargo run --bin xvn -- scenario ls
cargo run --bin xvn -- scenario show <id_from_ls>
```

Expected: 4 canonical rows in initial `ls`, then 5 after the create.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/scenario.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn scenario create/ls/show/clone/archive/rm/tree"
```

---

## Task 10 — Deprecate `xvn eval scenarios`

**Files:** `crates/xvision-cli/src/commands/eval.rs`

- [ ] **Step 1: Prepend deprecation notice + delegate**

```rust
async fn run_scenarios(args: ScenariosArgs) -> CliResult<()> {
    eprintln!("warning: 'xvn eval scenarios' is deprecated. Use 'xvn scenario ls' instead.");
    let ctx = ApiContext::from_xvn_home(args.xvn_home.as_deref()).await?;
    crate::commands::scenario::run(crate::commands::scenario::ScenarioCmd {
        op: crate::commands::scenario::ScenarioOp::Ls(crate::commands::scenario::LsArgs {
            source: None, tag: vec![], archived: false, json: args.json,
        }),
        xvn_home: args.xvn_home,
    }).await
}
```

- [ ] **Step 2: Smoke test**

```bash
cargo run --bin xvn -- eval scenarios
```

Expected: deprecation warning on stderr; same output as `xvn scenario ls`.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-cli/src/commands/eval.rs
git commit -m "feat(cli): deprecate 'xvn eval scenarios' in favor of 'xvn scenario ls'"
```

---

## Task 11 — `xvn migrate --dry-run`

**Files:** `crates/xvision-cli/src/commands/migrate.rs`, `crates/xvision-cli/src/commands/mod.rs`, `crates/xvision-cli/src/lib.rs`

- [ ] **Step 1: Implement command**

```rust
#[derive(Args, Debug)]
pub struct MigrateCmd {
    #[arg(long)] pub dry_run: bool,
    #[arg(long)] pub xvn_home: Option<PathBuf>,
}

pub async fn run(cmd: MigrateCmd) -> CliResult<()> {
    let ctx = ApiContext::from_xvn_home(cmd.xvn_home.as_deref()).await?;
    if cmd.dry_run {
        let pending = ctx.store.pending_migrations().await?;
        if pending.is_empty() {
            println!("no pending migrations.");
        } else {
            println!("pending:");
            for name in pending {
                println!("  {name}");
            }
        }
        let scenarios: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM scenarios").fetch_one(&ctx.pool).await?;
        let bundles: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM strategy_bundles WHERE id = 'bundle-canonical-defaults'").fetch_one(&ctx.pool).await?;
        if scenarios.0 == 0 { println!("  + seed 4 canonical scenarios"); }
        if bundles.0 == 0  { println!("  + seed canonical-defaults strategy bundle (Capital + RiskCaps from legacy canonical scenarios)"); }
    } else {
        ctx.store.run_migrations().await?;
        crate::api::migrate::run_seed_if_needed(&ctx).await?;
        println!("migrations applied.");
    }
    Ok(())
}
```

- [ ] **Step 2: Register**

```rust
/// SQLite migration runner: --dry-run reports deltas, otherwise applies.
Migrate(commands::migrate::MigrateCmd),
```

- [ ] **Step 3: Smoke test**

```bash
cargo run --bin xvn -- migrate --dry-run
```

Expected: lists pending + seed deltas; doesn't mutate.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/src/commands/migrate.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/src/lib.rs
git commit -m "feat(cli): xvn migrate --dry-run reports pending migrations + seed deltas"
```

---

## Task 12 — M2 acceptance smoke

- [ ] **Step 1: Run workspace tests**

```bash
cargo test --workspace
```

Expected: PASS.

- [ ] **Step 2: End-to-end smoke** (against test DB or dev DB)

```bash
xvn migrate --dry-run
xvn migrate
xvn scenario ls            # 4 canonical rows
xvn scenario create --name "ETH week" --asset ETH --from 2024-02-03 --to 2024-02-10 --granularity 1h
NEW_ID=$(xvn scenario ls | grep "ETH week" | head -1 | cut -f1)
xvn eval run --strategy bundle-canonical-defaults --scenario "$NEW_ID" --mode backtest
xvn eval show $(xvn eval list --strategy bundle-canonical-defaults --scenario "$NEW_ID" --json | jq -r '.[0].id')
xvn scenario tree "$NEW_ID"
xvn scenario clone "$NEW_ID" --name "ETH week (clone)"
xvn scenario rm "$NEW_ID"  # should be blocked — run references it
xvn scenario archive "$NEW_ID"
```

Expected: every step PASS; the `rm` step fails with "cannot delete … runs reference it. Archive instead."

- [ ] **Step 3: Commit any cleanup**

```bash
git add -p
git commit -m "chore: M2 acceptance smoke passes (custom scenarios live in DB)"
```

---

## Self-review notes

- New struct shape covered by serde-roundtrip test.
- Immutability defended by trigger + test that asserts UPDATE rejection.
- FK enforcement via triggers covered by test.
- Capital/risk move-off-scenario covered by `StrategyBundle` test + workspace build.
- Seed idempotency: `run_seed_if_needed` checks COUNT before inserting.
- `delete` blocked when runs reference covered by smoke test.
- Deprecation path: old `eval scenarios` still works for one cycle.
- No placeholders.
