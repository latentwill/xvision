# v1 Frontend — Plan 5: Findings + Compare + Polish

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete v1 by shipping the **findings system** end-to-end (schema + rule-based extractor + UI on Run detail with "Draft variant from this →"), the **per-trade ledger** persistence + UI, the **full Compare runs view** (overlaid equity + side-by-side findings), the **⌘K command palette** with FTS5 search across strategies/runs/findings, and a polish pass (error boundaries, empty states, a11y).

**Architecture:** Two new SQLite tables (`findings`, `trades`) and one new FTS5 virtual table (`search_index`). A rule-based extractor runs as a post-eval pass producing `Finding` rows; LLM-based extraction is deliberately deferred. `engine::api::findings::*`, `engine::api::trades::*`, and `engine::api::search::*` proxy to the dashboard. The command palette is a Radix Dialog opened with ⌘K from anywhere; results are paginated and grouped by kind. The Compare view fetches multiple runs in parallel via `useQueries` and renders a single overlaid equity chart on top with per-run columns below.

**Tech Stack:** Inherits all prior plans. Adds nothing client-side. Backend leans on SQLite's built-in [FTS5](https://www.sqlite.org/fts5.html) virtual table.

---

## Scope and split

Plan 5 of 5. Depends on Plans 1, 2, 3 hard. Plan 4 is a soft prereq — the chat rail and wizard hand-offs are nicer with `?seed=` plumbing, but Plan 5's "Draft variant from this →" works as a redirect even if the wizard backend hasn't read the seed yet.

## Prerequisites

- **Required:** Plans 1, 2, 3 landed. Eval engine plan (`eval_runs` table) landed.
- **Recommended:** Plan 4 landed (so the "Draft variant from this →" redirect lands in a working wizard).

## File structure

```
crates/xvision-core/migrations/
├── 0004_findings.sql                NEW
├── 0005_trades.sql                  NEW
└── 0006_search_index.sql            NEW

crates/xvision-eval/src/
└── findings/                        NEW
    ├── mod.rs
    ├── extractor.rs                 # rule-based finding generators
    └── rules/
        ├── large_drawdown.rs
        ├── regime_fit_mismatch.rs
        ├── stop_loss_clustering.rs
        └── long_holding_outperforms.rs

crates/xvision-engine/src/api/
├── findings.rs                      NEW
├── trades.rs                        NEW
└── search.rs                        NEW

crates/xvision-dashboard/src/routes/
├── findings.rs                      NEW
├── trades.rs                        NEW
└── search.rs                        NEW

frontend/web/src/
├── api/
│   ├── findings.ts                  NEW
│   ├── trades.ts                    NEW
│   └── search.ts                    NEW
├── components/
│   ├── chrome/
│   │   ├── CommandPalette.tsx       NEW
│   │   └── ErrorBoundary.tsx        NEW
│   ├── findings/
│   │   ├── FindingsList.tsx         NEW
│   │   ├── FindingCard.tsx          NEW
│   │   └── EvidenceBadge.tsx        NEW
│   └── tables/
│       └── TradeLedger.tsx          NEW
├── stores/
│   └── command-palette.ts           NEW
├── hooks/
│   └── useCmdK.ts                   NEW
└── routes/
    ├── eval-runs-detail.tsx         AUGMENT (mount findings + trade ledger)
    └── eval-compare.tsx             REPLACE shell with full view
```

---

## Tasks

### Task 1: Migration — `findings` table

**Files:**
- Create: `crates/xvision-core/migrations/0004_findings.sql`

- [ ] **Step 1.1: Write the migration**

```sql
-- 0004_findings.sql
CREATE TABLE findings (
  finding_id      TEXT PRIMARY KEY NOT NULL,
  run_id          TEXT NOT NULL REFERENCES eval_runs(run_id) ON DELETE CASCADE,
  kind            TEXT NOT NULL,                  -- snake_case label, e.g. "regime_fit_mismatch"
  severity        TEXT NOT NULL CHECK (severity IN ('critical', 'warning', 'info')),
  title           TEXT NOT NULL,
  summary_md      TEXT NOT NULL,
  evidence_json   TEXT NOT NULL DEFAULT '[]',     -- JSON array of EvidenceRef
  created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX idx_findings_run_id ON findings(run_id);
CREATE INDEX idx_findings_kind ON findings(kind);
CREATE INDEX idx_findings_severity ON findings(severity);
```

- [ ] **Step 1.2: Verify it applies**

```bash
sqlite3 /tmp/xvn-test.db < crates/xvision-core/migrations/0001_init.sql
sqlite3 /tmp/xvn-test.db < crates/xvision-core/migrations/0002_rename_setup_to_cycle.sql
sqlite3 /tmp/xvn-test.db < crates/xvision-core/migrations/0003_bundle_status_lineage.sql
sqlite3 /tmp/xvn-test.db < crates/xvision-core/migrations/0004_findings.sql
sqlite3 /tmp/xvn-test.db ".schema findings"
```

(If `eval_runs` doesn't exist in the test DB, apply the eval engine plan's migration too — adjust order accordingly.)

- [ ] **Step 1.3: Commit**

```bash
git add crates/xvision-core/migrations/0004_findings.sql
git commit -m "feat(core): findings table migration"
```

---

### Task 2: Migration — `trades` table

**Files:**
- Create: `crates/xvision-core/migrations/0005_trades.sql`

- [ ] **Step 2.1: Migration**

```sql
-- 0005_trades.sql
CREATE TABLE trades (
  trade_id        TEXT PRIMARY KEY NOT NULL,
  run_id          TEXT NOT NULL REFERENCES eval_runs(run_id) ON DELETE CASCADE,
  cycle_id        TEXT REFERENCES cycles(cycle_id),
  opened_at       TEXT NOT NULL,
  closed_at       TEXT,
  side            TEXT NOT NULL CHECK (side IN ('long', 'short')),
  symbol          TEXT NOT NULL,
  qty             REAL NOT NULL,
  entry_price     REAL NOT NULL,
  exit_price      REAL,
  realized_pnl_usd REAL
);

CREATE INDEX idx_trades_run_id ON trades(run_id);
CREATE INDEX idx_trades_opened_at ON trades(opened_at);
```

- [ ] **Step 2.2: Commit**

```bash
git add crates/xvision-core/migrations/0005_trades.sql
git commit -m "feat(core): trades table migration"
```

---

### Task 3: Migration — FTS5 search index

**Files:**
- Create: `crates/xvision-core/migrations/0006_search_index.sql`

- [ ] **Step 3.1: Migration**

```sql
-- 0006_search_index.sql
-- One contentless FTS5 table covering strategies, runs, findings.
-- Each row has (kind, ref_id, title, body) for ranking.

CREATE VIRTUAL TABLE search_index USING fts5(
  kind UNINDEXED,           -- "strategy" | "run" | "finding"
  ref_id UNINDEXED,
  title,
  body,
  tokenize = 'porter unicode61'
);

-- Trigger: re-index on bundle insert/update
CREATE TRIGGER search_index_bundles_ai AFTER INSERT ON bundles BEGIN
  INSERT INTO search_index(kind, ref_id, title, body)
    VALUES ('strategy', NEW.bundle_id, NEW.name, COALESCE(NEW.template, '') || ' ' || COALESCE(NEW.parent_bundle_id, ''));
END;

CREATE TRIGGER search_index_bundles_au AFTER UPDATE ON bundles BEGIN
  DELETE FROM search_index WHERE kind = 'strategy' AND ref_id = OLD.bundle_id;
  INSERT INTO search_index(kind, ref_id, title, body)
    VALUES ('strategy', NEW.bundle_id, NEW.name, COALESCE(NEW.template, '') || ' ' || COALESCE(NEW.parent_bundle_id, ''));
END;

CREATE TRIGGER search_index_bundles_ad AFTER DELETE ON bundles BEGIN
  DELETE FROM search_index WHERE kind = 'strategy' AND ref_id = OLD.bundle_id;
END;

-- Findings triggers (only insert — findings are append-mostly)
CREATE TRIGGER search_index_findings_ai AFTER INSERT ON findings BEGIN
  INSERT INTO search_index(kind, ref_id, title, body)
    VALUES ('finding', NEW.finding_id, NEW.title, NEW.kind || ' ' || NEW.summary_md);
END;

CREATE TRIGGER search_index_findings_ad AFTER DELETE ON findings BEGIN
  DELETE FROM search_index WHERE kind = 'finding' AND ref_id = OLD.finding_id;
END;

-- Eval runs triggers (assumes the eval-engine plan provides eval_runs with run_id, strategy, scenario columns)
CREATE TRIGGER search_index_runs_ai AFTER INSERT ON eval_runs BEGIN
  INSERT INTO search_index(kind, ref_id, title, body)
    VALUES ('run', NEW.run_id, NEW.run_id, NEW.strategy || ' ' || NEW.scenario);
END;

CREATE TRIGGER search_index_runs_ad AFTER DELETE ON eval_runs BEGIN
  DELETE FROM search_index WHERE kind = 'run' AND ref_id = OLD.run_id;
END;
```

- [ ] **Step 3.2: Backfill**

After running the migration, backfill any pre-existing rows:

```sql
INSERT INTO search_index(kind, ref_id, title, body)
  SELECT 'strategy', bundle_id, name, COALESCE(template, '') || ' ' || COALESCE(parent_bundle_id, '')
  FROM bundles;
INSERT INTO search_index(kind, ref_id, title, body)
  SELECT 'finding', finding_id, title, kind || ' ' || summary_md
  FROM findings;
INSERT INTO search_index(kind, ref_id, title, body)
  SELECT 'run', run_id, run_id, strategy || ' ' || scenario
  FROM eval_runs;
```

The migration runner should detect this is a one-time backfill (run conditionally if SQLite version >= 3.20).

- [ ] **Step 3.3: Commit**

```bash
git add crates/xvision-core/migrations/0006_search_index.sql
git commit -m "feat(core): FTS5 search index across strategies, runs, findings"
```

---

### Task 4: Engine — `Finding` type + `findings::*` API

**Files:**
- Create: `crates/xvision-engine/src/api/findings.rs`
- Modify: `crates/xvision-engine/src/api/mod.rs`

- [ ] **Step 4.1: Define types**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub finding_id: String,
    pub run_id: String,
    pub kind: String,
    pub severity: Severity,
    pub title: String,
    pub summary_md: String,
    pub evidence: Vec<EvidenceRef>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub created_at: DateTime<Utc>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Severity { Critical, Warning, Info }

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EvidenceRef {
    TradeRange { from_trade_id: String, to_trade_id: String },
    RegimeWindow { from_iso: String, to_iso: String },
    MetricThreshold { metric: String, threshold: f64, observed: f64 },
}

pub async fn list_for_run(ctx: &ApiContext, run_id: &str) -> Result<Vec<Finding>, ApiError> {
    ctx.findings_store().list_by_run(run_id).await.map_err(ApiError::from)
}

pub async fn get(ctx: &ApiContext, finding_id: &str) -> Result<Finding, ApiError> {
    ctx.findings_store()
        .get(finding_id)
        .await
        .map_err(ApiError::from)?
        .ok_or_else(|| ApiError::NotFound(format!("finding {finding_id}")))
}

pub async fn extract_for_run(ctx: &ApiContext, run_id: &str) -> Result<Vec<Finding>, ApiError> {
    use xvision_eval::findings::extractor;
    let run_data = ctx.eval_store().load_for_extraction(run_id).await.map_err(ApiError::from)?;
    let new_findings = extractor::extract(&run_data);
    ctx.findings_store().replace_for_run(run_id, &new_findings).await.map_err(ApiError::from)?;
    Ok(new_findings)
}
```

- [ ] **Step 4.2: Wire `findings_store` into ApiContext**

In `xvision-engine/src/api/context.rs` (or similar), add a `findings_store()` accessor that returns a struct backed by SQLite. Implement standard methods: `list_by_run`, `get`, `replace_for_run` (delete-by-run + insert-batch).

```rust
impl ApiContext {
    pub fn findings_store(&self) -> FindingsStore {
        FindingsStore { db: self.db.clone() }
    }
}

pub struct FindingsStore { db: sqlx::SqlitePool /* or your DB handle */ }

impl FindingsStore {
    pub async fn list_by_run(&self, run_id: &str) -> Result<Vec<Finding>, sqlx::Error> { /* SELECT */ }
    pub async fn get(&self, id: &str) -> Result<Option<Finding>, sqlx::Error> { /* SELECT */ }
    pub async fn replace_for_run(&self, run_id: &str, items: &[Finding]) -> Result<(), sqlx::Error> {
        // BEGIN; DELETE WHERE run_id = ?; INSERT each; COMMIT
    }
}
```

(Match the actual DB layer — `sqlx`, `rusqlite`, etc. If the engine uses `rusqlite`, use `tokio::task::spawn_blocking` for the queries.)

- [ ] **Step 4.3: Add `pub mod findings;` to `api/mod.rs`**

- [ ] **Step 4.4: Test**

```rust
#[tokio::test]
async fn extract_then_list_returns_findings() {
    let ctx = test_ctx_with_run("01H8N7Z").await;
    let extracted = extract_for_run(&ctx, "01H8N7Z").await.unwrap();
    let listed = list_for_run(&ctx, "01H8N7Z").await.unwrap();
    assert_eq!(extracted.len(), listed.len());
}
```

- [ ] **Step 4.5: Commit**

```bash
cargo xtask gen-types
git add . && git commit -m "feat(engine): findings API (list, get, extract_for_run)"
```

---

### Task 5: Eval — rule-based extractor

**Files:**
- Create: `crates/xvision-eval/src/findings/mod.rs`
- Create: `crates/xvision-eval/src/findings/extractor.rs`
- Create: `crates/xvision-eval/src/findings/rules/large_drawdown.rs`
- Create: `crates/xvision-eval/src/findings/rules/regime_fit_mismatch.rs`
- Create: `crates/xvision-eval/src/findings/rules/stop_loss_clustering.rs`
- Create: `crates/xvision-eval/src/findings/rules/long_holding_outperforms.rs`
- Modify: `crates/xvision-eval/src/lib.rs` (add `pub mod findings`)

- [ ] **Step 5.1: Module wiring**

Create `crates/xvision-eval/src/findings/mod.rs`:

```rust
pub mod extractor;
pub mod rules;

pub use extractor::extract;
```

Create `crates/xvision-eval/src/findings/rules/mod.rs`:

```rust
pub mod large_drawdown;
pub mod regime_fit_mismatch;
pub mod stop_loss_clustering;
pub mod long_holding_outperforms;
```

In `xvision-eval/src/lib.rs`: `pub mod findings;`.

- [ ] **Step 5.2: Define `RunData` shape extractors consume**

Create `crates/xvision-eval/src/findings/extractor.rs`:

```rust
use xvision_engine::api::findings::{Finding, Severity};
use ulid::Ulid;
use chrono::Utc;

pub struct RunData {
    pub run_id: String,
    pub trades: Vec<TradeRow>,
    pub equity_series: Vec<(chrono::DateTime<Utc>, f64)>,    // (time, value_pct)
    pub regime_windows: Vec<RegimeWindow>,
    pub max_drawdown_pct: f64,
}

pub struct TradeRow {
    pub trade_id: String,
    pub side: String,
    pub opened_at: chrono::DateTime<Utc>,
    pub closed_at: Option<chrono::DateTime<Utc>>,
    pub realized_pnl_usd: Option<f64>,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub stop_distance_atr: Option<f64>,
    pub regime_at_entry: Option<String>,
}

pub struct RegimeWindow {
    pub regime: String,           // "bull" | "bear" | "chop" | "bull_pullback" ...
    pub from: chrono::DateTime<Utc>,
    pub to: chrono::DateTime<Utc>,
}

pub fn extract(data: &RunData) -> Vec<Finding> {
    let mut out = Vec::new();
    out.extend(super::rules::large_drawdown::detect(data));
    out.extend(super::rules::regime_fit_mismatch::detect(data));
    out.extend(super::rules::stop_loss_clustering::detect(data));
    out.extend(super::rules::long_holding_outperforms::detect(data));
    out
}

pub(crate) fn new_finding(
    run_id: &str,
    kind: &str,
    severity: Severity,
    title: &str,
    summary_md: &str,
    evidence: Vec<xvision_engine::api::findings::EvidenceRef>,
) -> Finding {
    Finding {
        finding_id: Ulid::new().to_string(),
        run_id: run_id.to_string(),
        kind: kind.to_string(),
        severity,
        title: title.to_string(),
        summary_md: summary_md.to_string(),
        evidence,
        created_at: Utc::now(),
    }
}
```

- [ ] **Step 5.3: Implement `large_drawdown`**

Create `crates/xvision-eval/src/findings/rules/large_drawdown.rs`:

```rust
use xvision_engine::api::findings::{EvidenceRef, Finding, Severity};
use crate::findings::extractor::{new_finding, RunData};

pub fn detect(data: &RunData) -> Vec<Finding> {
    if data.max_drawdown_pct <= -20.0 {
        let severity = if data.max_drawdown_pct <= -30.0 { Severity::Critical } else { Severity::Warning };
        vec![new_finding(
            &data.run_id,
            "large_drawdown",
            severity,
            "Max drawdown exceeded 20%",
            &format!(
                "Max drawdown reached {:.1}%. Consider tightening stop-loss multiples or sizing.",
                data.max_drawdown_pct
            ),
            vec![EvidenceRef::MetricThreshold {
                metric: "max_drawdown_pct".into(),
                threshold: -20.0,
                observed: data.max_drawdown_pct,
            }],
        )]
    } else {
        vec![]
    }
}
```

- [ ] **Step 5.4: Implement `regime_fit_mismatch`**

```rust
use xvision_engine::api::findings::{EvidenceRef, Finding, Severity};
use crate::findings::extractor::{new_finding, RunData};

pub fn detect(data: &RunData) -> Vec<Finding> {
    let chop_trades: Vec<_> = data.trades.iter()
        .filter(|t| t.regime_at_entry.as_deref() == Some("chop"))
        .collect();
    if chop_trades.len() < 5 { return vec![]; }

    let pnl_sum: f64 = chop_trades.iter().filter_map(|t| t.realized_pnl_usd).sum();
    let count = chop_trades.len();
    let avg_pnl = pnl_sum / count as f64;
    if avg_pnl >= 0.0 { return vec![]; }

    vec![new_finding(
        &data.run_id,
        "regime_fit_mismatch",
        Severity::Critical,
        "Strategy underperforms in chop regimes",
        &format!(
            "{} trades opened in chop regime returned {:.2} avg PnL. Consider gating entries by regime.",
            count, avg_pnl
        ),
        vec![EvidenceRef::MetricThreshold {
            metric: "chop_avg_pnl_usd".into(),
            threshold: 0.0,
            observed: avg_pnl,
        }],
    )]
}
```

- [ ] **Step 5.5: Implement `stop_loss_clustering`**

```rust
use xvision_engine::api::findings::{EvidenceRef, Finding, Severity};
use crate::findings::extractor::{new_finding, RunData};

pub fn detect(data: &RunData) -> Vec<Finding> {
    if data.trades.is_empty() { return vec![]; }
    let near_stop = data.trades.iter()
        .filter(|t| t.stop_distance_atr.map(|d| d.abs() <= 2.0).unwrap_or(false))
        .count();
    let pct = near_stop as f64 / data.trades.len() as f64;
    if pct < 0.4 { return vec![]; }

    vec![new_finding(
        &data.run_id,
        "stop_loss_clustering",
        Severity::Warning,
        "Stops cluster near entry",
        &format!(
            "{:.0}% of stops triggered within 2× ATR of entry. Either tighten or widen.",
            pct * 100.0
        ),
        vec![EvidenceRef::MetricThreshold {
            metric: "stops_within_2x_atr_pct".into(),
            threshold: 0.4,
            observed: pct,
        }],
    )]
}
```

- [ ] **Step 5.6: Implement `long_holding_outperforms`**

```rust
use xvision_engine::api::findings::{EvidenceRef, Finding, Severity};
use crate::findings::extractor::{new_finding, RunData};

pub fn detect(data: &RunData) -> Vec<Finding> {
    let closed: Vec<_> = data.trades.iter()
        .filter(|t| t.closed_at.is_some() && t.realized_pnl_usd.is_some())
        .collect();
    if closed.len() < 10 { return vec![]; }

    let mut long_held = Vec::new();
    let mut short_held = Vec::new();
    for t in &closed {
        let dur = (t.closed_at.unwrap() - t.opened_at).num_minutes();
        if dur >= 240 { long_held.push(t.realized_pnl_usd.unwrap()); }
        else { short_held.push(t.realized_pnl_usd.unwrap()); }
    }
    if long_held.is_empty() || short_held.is_empty() { return vec![]; }
    let long_avg: f64 = long_held.iter().sum::<f64>() / long_held.len() as f64;
    let short_avg: f64 = short_held.iter().sum::<f64>() / short_held.len() as f64;
    if short_avg.abs() < 1e-6 || (long_avg / short_avg) < 2.0 { return vec![]; }

    vec![new_finding(
        &data.run_id,
        "long_holding_outperforms",
        Severity::Info,
        "Trades held > 4h outperform short holds",
        &format!(
            "Long-held trades returned {:.2}× short-held trades' average. Consider widening profit targets.",
            long_avg / short_avg
        ),
        vec![EvidenceRef::MetricThreshold {
            metric: "long_to_short_ratio".into(),
            threshold: 2.0,
            observed: long_avg / short_avg,
        }],
    )]
}
```

- [ ] **Step 5.7: Test extraction**

In `crates/xvision-eval/src/findings/extractor.rs` (or a tests/ file), add:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use xvision_engine::api::findings::Severity;

    fn rd_with_drawdown(d: f64) -> RunData {
        RunData { run_id: "r1".into(), trades: vec![], equity_series: vec![], regime_windows: vec![], max_drawdown_pct: d }
    }

    #[test]
    fn detects_large_drawdown() {
        let f = extract(&rd_with_drawdown(-25.0));
        assert!(f.iter().any(|x| x.kind == "large_drawdown" && x.severity == Severity::Warning));
    }

    #[test]
    fn detects_critical_drawdown() {
        let f = extract(&rd_with_drawdown(-35.0));
        assert!(f.iter().any(|x| x.kind == "large_drawdown" && x.severity == Severity::Critical));
    }

    #[test]
    fn no_drawdown_finding_when_small() {
        let f = extract(&rd_with_drawdown(-5.0));
        assert!(f.iter().all(|x| x.kind != "large_drawdown"));
    }
}
```

Run: `cargo test -p xvision-eval findings::`. Expect 3 passed.

- [ ] **Step 5.8: Hook the extractor into the eval pipeline (auto-extract on run completion)**

In `xvision-eval/src/runner.rs` (or wherever a run finalizes), after persisting the `eval_run` row, run the extractor and persist findings. Pseudocode:

```rust
let run_data = build_run_data(&store, &run_id).await?;
let findings = crate::findings::extract(&run_data);
ctx.findings_store().replace_for_run(&run_id, &findings).await?;
```

Wrap in `if cfg.auto_extract_findings { … }` defaulting to `true` for v1 (per the open question — auto-extract is the resolution).

- [ ] **Step 5.9: Commit**

```bash
cargo test -p xvision-eval
git add crates/xvision-eval/src/findings/ crates/xvision-eval/src/runner.rs  # adjust path
git commit -m "feat(eval): rule-based finding extractor (4 rules) + auto-extract on run completion"
```

---

### Task 6: Engine — `trades` API

**Files:**
- Create: `crates/xvision-engine/src/api/trades.rs`
- Modify: `api/mod.rs`

- [ ] **Step 6.1: Define + implement**

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub trade_id: String,
    pub run_id: String,
    pub cycle_id: Option<String>,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub opened_at: DateTime<Utc>,
    #[cfg_attr(feature = "ts-export", ts(type = "string|null"))]
    pub closed_at: Option<DateTime<Utc>>,
    pub side: String,
    pub symbol: String,
    pub qty: f64,
    pub entry_price: f64,
    pub exit_price: Option<f64>,
    pub realized_pnl_usd: Option<f64>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeListResponse {
    pub items: Vec<Trade>,
    pub total: u32,
    pub page: u32,
    pub page_size: u32,
}

pub async fn list_for_run(
    ctx: &ApiContext,
    run_id: &str,
    page: u32,
    page_size: u32,
) -> Result<TradeListResponse, ApiError> {
    let total = ctx.trades_store().count(run_id).await.map_err(ApiError::from)?;
    let items = ctx.trades_store().list(run_id, page, page_size).await.map_err(ApiError::from)?;
    Ok(TradeListResponse { items, total, page, page_size })
}
```

Implement `TradesStore::{count, list}` mirroring `FindingsStore`. Add to `api/mod.rs`: `pub mod trades;`.

- [ ] **Step 6.2: Eval pipeline writes trades**

Wherever fills/closes are processed in `xvision-eval`, append a `Trade` row to `trades_store` after each cycle. The audit confirmed `ArmResult.fills` is in-memory; this task persists each fill to the new table on run completion.

- [ ] **Step 6.3: Commit**

```bash
cargo test -p xvision-engine -p xvision-eval
cargo xtask gen-types
git add . && git commit -m "feat(engine): trades persistence + list_for_run API"
```

---

### Task 7: Engine — `search::*` API

**Files:**
- Create: `crates/xvision-engine/src/api/search.rs`
- Modify: `api/mod.rs`

- [ ] **Step 7.1: Define + implement**

```rust
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub kind: String,           // "strategy" | "run" | "finding"
    pub ref_id: String,
    pub title: String,
    pub snippet: String,
    pub rank: f64,
}

pub async fn search(
    ctx: &ApiContext,
    query: &str,
    kinds: &[&str],
    limit: u32,
) -> Result<Vec<SearchHit>, ApiError> {
    if query.trim().is_empty() { return Ok(Vec::new()); }
    let hits = ctx.search_store().query(query, kinds, limit).await.map_err(ApiError::from)?;
    Ok(hits)
}
```

Implement `SearchStore::query` against the `search_index` virtual table:

```sql
SELECT kind, ref_id, title, snippet(search_index, 3, '<<', '>>', '...', 16) AS snippet, rank
FROM search_index
WHERE search_index MATCH ?
  AND kind IN (...)
ORDER BY rank
LIMIT ?
```

(The `?` for kinds is safest as a quoted-list interpolation since SQLite parameters don't bind into IN clauses naturally — sanitize the input strictly to the allowed set.)

Add to `api/mod.rs`: `pub mod search;`.

- [ ] **Step 7.2: Commit**

```bash
cargo test -p xvision-engine
cargo xtask gen-types
git add . && git commit -m "feat(engine): search API backed by FTS5 search_index"
```

---

### Task 8: Dashboard routes — findings, trades, search

**Files:**
- Create: `crates/xvision-dashboard/src/routes/findings.rs`
- Create: `crates/xvision-dashboard/src/routes/trades.rs`
- Create: `crates/xvision-dashboard/src/routes/search.rs`
- Modify: `routes/mod.rs`, `server.rs`

- [ ] **Step 8.1: Findings handlers**

Create `crates/xvision-dashboard/src/routes/findings.rs`:

```rust
use axum::extract::Path;
use axum::Json;
use xvision_engine::api::findings::{extract_for_run, list_for_run, Finding};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn list_handler(Path(run_id): Path<String>) -> Result<Json<Vec<Finding>>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(list_for_run(&ctx, &run_id).await.map_err(map_api_err)?))
}

pub async fn extract_handler(Path(run_id): Path<String>) -> Result<Json<Vec<Finding>>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(extract_for_run(&ctx, &run_id).await.map_err(map_api_err)?))
}
```

- [ ] **Step 8.2: Trades handler**

```rust
use axum::extract::{Path, Query};
use axum::Json;
use serde::Deserialize;
use xvision_engine::api::trades::{list_for_run, TradeListResponse};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

#[derive(Deserialize, Default)]
pub struct PageQ {
    pub page: Option<u32>,
    pub page_size: Option<u32>,
}

pub async fn list_handler(
    Path(run_id): Path<String>,
    Query(q): Query<PageQ>,
) -> Result<Json<TradeListResponse>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    let page = q.page.unwrap_or(0);
    let size = q.page_size.unwrap_or(50).min(200);
    Ok(Json(list_for_run(&ctx, &run_id, page, size).await.map_err(map_api_err)?))
}
```

- [ ] **Step 8.3: Search handler**

```rust
use axum::extract::Query;
use axum::Json;
use serde::Deserialize;
use xvision_engine::api::search::{search, SearchHit};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

#[derive(Deserialize)]
pub struct SearchQ {
    pub q: String,
    #[serde(default)]
    pub kinds: Option<String>,         // comma-separated
    pub limit: Option<u32>,
}

pub async fn handler(Query(q): Query<SearchQ>) -> Result<Json<Vec<SearchHit>>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    let kinds: Vec<&str> = q.kinds.as_deref().map(|s| s.split(',').collect()).unwrap_or_else(|| vec!["strategy", "run", "finding"]);
    Ok(Json(search(&ctx, &q.q, &kinds, q.limit.unwrap_or(20)).await.map_err(map_api_err)?))
}
```

- [ ] **Step 8.4: Register**

In `server.rs`:

```rust
.route("/api/eval/runs/:id/findings", get(crate::routes::findings::list_handler))
.route("/api/eval/runs/:id/findings/extract", post(crate::routes::findings::extract_handler))
.route("/api/eval/runs/:id/trades", get(crate::routes::trades::list_handler))
.route("/api/search", get(crate::routes::search::handler))
```

- [ ] **Step 8.5: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(dashboard): findings, trades, search routes"
```

---

### Task 9: Frontend — findings + trades + search API clients

**Files:**
- Create: `frontend/web/src/api/findings.ts`
- Create: `frontend/web/src/api/trades.ts`
- Create: `frontend/web/src/api/search.ts`

- [ ] **Step 9.1: Findings**

```ts
import { apiFetch } from "./client";
import type { Finding } from "./types.gen";

export const findingsApi = {
  list: (runId: string) => apiFetch<Finding[]>(`/api/eval/runs/${encodeURIComponent(runId)}/findings`),
  extract: (runId: string) =>
    apiFetch<Finding[]>(`/api/eval/runs/${encodeURIComponent(runId)}/findings/extract`, { method: "POST" }),
};
```

- [ ] **Step 9.2: Trades**

```ts
import { apiFetch } from "./client";
import type { TradeListResponse } from "./types.gen";

export const tradesApi = {
  list: (runId: string, page = 0, pageSize = 50) =>
    apiFetch<TradeListResponse>(`/api/eval/runs/${encodeURIComponent(runId)}/trades?page=${page}&page_size=${pageSize}`),
};
```

- [ ] **Step 9.3: Search**

```ts
import { apiFetch } from "./client";
import type { SearchHit } from "./types.gen";

export const searchApi = {
  query: (q: string, kinds?: string[], limit = 20) => {
    const params = new URLSearchParams({ q, limit: String(limit) });
    if (kinds?.length) params.set("kinds", kinds.join(","));
    return apiFetch<SearchHit[]>(`/api/search?${params.toString()}`);
  },
};
```

- [ ] **Step 9.4: Commit**

```bash
git add frontend/web/src/api/
git commit -m "feat(frontend): findings, trades, search API clients"
```

---

### Task 10: Frontend — `FindingsList` + `FindingCard` + `EvidenceBadge`

**Files:**
- Create: `frontend/web/src/components/findings/EvidenceBadge.tsx`
- Create: `frontend/web/src/components/findings/FindingCard.tsx`
- Create: `frontend/web/src/components/findings/FindingsList.tsx`

- [ ] **Step 10.1: `EvidenceBadge`**

```tsx
import type { EvidenceRef } from "@/api/types.gen";

export function EvidenceBadge({ e }: { e: EvidenceRef }) {
  let label = "";
  if (e.type === "trade_range") label = `Trades ${e.from_trade_id.slice(0, 6)}…→${e.to_trade_id.slice(0, 6)}…`;
  else if (e.type === "regime_window") label = `${e.from_iso.slice(0, 10)} → ${e.to_iso.slice(0, 10)}`;
  else if (e.type === "metric_threshold") label = `${e.metric} = ${e.observed.toFixed(2)} (≥ ${e.threshold.toFixed(2)})`;
  return (
    <span className="inline-flex items-center px-2 py-0.5 border border-border rounded-sm text-[10.5px] font-mono text-text-2 mr-1">
      {label}
    </span>
  );
}
```

- [ ] **Step 10.2: `FindingCard`**

```tsx
import { Link } from "react-router-dom";
import ReactMarkdown from "react-markdown";
import { Dot } from "@/components/primitives/Dot";
import { EvidenceBadge } from "./EvidenceBadge";
import type { Finding } from "@/api/types.gen";

const TONE: Record<string, "danger" | "warn" | "info"> = {
  critical: "danger",
  warning: "warn",
  info: "info",
};

export function FindingCard({ f }: { f: Finding }) {
  return (
    <div className="flex gap-3 items-start">
      <span className="mt-1.5"><Dot tone={TONE[f.severity]} /></span>
      <div className="flex-1 min-w-0">
        <div className="font-mono text-[12px] text-text">{f.kind}</div>
        <div className="text-[12px] text-text-2 leading-relaxed mt-0.5">
          <ReactMarkdown>{f.summary_md}</ReactMarkdown>
        </div>
        {f.evidence.length > 0 && (
          <div className="mt-2">
            {f.evidence.map((e, i) => <EvidenceBadge key={i} e={e} />)}
          </div>
        )}
        <div className="flex gap-3 mt-2">
          <Link
            to={`/setup?seed=finding:${encodeURIComponent(f.run_id)}:${encodeURIComponent(f.finding_id)}`}
            className="text-gold text-xs no-underline"
          >
            Draft variant from this →
          </Link>
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 10.3: `FindingsList`**

```tsx
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { findingsApi } from "@/api/findings";
import { FindingCard } from "./FindingCard";
import { useToasts } from "@/components/chrome/ToastRegion";

export function FindingsList({ runId }: { runId: string }) {
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const { data: findings = [], isLoading } = useQuery({
    queryKey: ["findings", runId],
    queryFn: () => findingsApi.list(runId),
  });
  const reMut = useMutation({
    mutationFn: () => findingsApi.extract(runId),
    onSuccess: (f) => {
      qc.setQueryData(["findings", runId], f);
      push({ title: `Re-extracted ${f.length} findings`, kind: "ok" });
    },
  });

  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between px-5 py-4">
        <h2 className="font-serif font-medium text-[22px] m-0">
          Findings <Pill className="ml-2">{findings.length}</Pill>
        </h2>
        <button
          onClick={() => reMut.mutate()}
          disabled={reMut.isPending}
          className="border border-border text-text-2 rounded-sm px-3 py-1.5 text-xs"
        >
          {reMut.isPending ? "Re-extracting…" : "Re-extract"}
        </button>
      </div>
      <div className="px-5 pb-5 flex flex-col gap-3.5">
        {isLoading && <div className="text-text-2 text-sm">Loading…</div>}
        {!isLoading && findings.length === 0 && (
          <div className="text-text-3 text-sm">No findings yet — try Re-extract.</div>
        )}
        {findings.map((f) => <FindingCard key={f.finding_id} f={f} />)}
      </div>
    </Card>
  );
}
```

- [ ] **Step 10.4: Commit**

```bash
git add frontend/web/src/components/findings/
git commit -m "feat(frontend): FindingsList, FindingCard, EvidenceBadge"
```

---

### Task 11: Frontend — `TradeLedger`

**Files:**
- Create: `frontend/web/src/components/tables/TradeLedger.tsx`

- [ ] **Step 11.1: Implement**

```tsx
import { useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { tradesApi } from "@/api/trades";
import { clsx } from "clsx";

export function TradeLedger({ runId }: { runId: string }) {
  const [page, setPage] = useState(0);
  const pageSize = 25;
  const { data } = useQuery({
    queryKey: ["trades", runId, page],
    queryFn: () => tradesApi.list(runId, page, pageSize),
  });

  const items = data?.items ?? [];
  const total = data?.total ?? 0;
  const lastPage = Math.max(0, Math.ceil(total / pageSize) - 1);

  return (
    <Card className="overflow-hidden">
      <div className="flex items-center justify-between px-5 py-4">
        <h2 className="font-serif font-medium text-[22px] m-0">
          Trade ledger <Pill className="ml-2">{total}</Pill>
        </h2>
        <span className="text-text-2 text-xs">
          {total === 0 ? "No trades" : `Showing ${page * pageSize + 1}–${page * pageSize + items.length} of ${total}`}
        </span>
      </div>
      <table className="w-full border-collapse">
        <thead>
          <tr className="text-xs text-text-2">
            <Th className="pl-5">Time</Th><Th>Side</Th>
            <Th className="text-right">Qty</Th><Th className="text-right">Entry</Th>
            <Th className="text-right">Exit</Th><Th className="text-right pr-5">PnL</Th>
          </tr>
        </thead>
        <tbody>
          {items.map((t) => (
            <tr key={t.trade_id}>
              <Td className="pl-5 font-mono text-text-2">{t.opened_at.replace("T", " ").slice(5, 16)}</Td>
              <Td className="text-gold">{t.side}</Td>
              <Td className="font-mono text-right">{t.qty.toFixed(4)}</Td>
              <Td className="font-mono text-right">{t.entry_price.toFixed(2)}</Td>
              <Td className="font-mono text-right">{t.exit_price?.toFixed(2) ?? "—"}</Td>
              <Td className={clsx("font-mono text-right pr-5",
                t.realized_pnl_usd != null && (t.realized_pnl_usd >= 0 ? "text-gold" : "text-danger"))}>
                {t.realized_pnl_usd != null
                  ? `${t.realized_pnl_usd >= 0 ? "+" : "−"}$${Math.abs(t.realized_pnl_usd).toFixed(2)}`
                  : "—"}
              </Td>
            </tr>
          ))}
        </tbody>
      </table>
      {lastPage > 0 && (
        <div className="flex justify-end gap-2 px-5 py-3 border-t border-border-soft">
          <button onClick={() => setPage((p) => Math.max(0, p - 1))} disabled={page === 0} className="text-text-2 text-xs disabled:opacity-30">
            ← Prev
          </button>
          <span className="text-text-3 text-xs">{page + 1} / {lastPage + 1}</span>
          <button onClick={() => setPage((p) => Math.min(lastPage, p + 1))} disabled={page >= lastPage} className="text-text-2 text-xs disabled:opacity-30">
            Next →
          </button>
        </div>
      )}
    </Card>
  );
}

function Th({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <th className={`text-left font-normal py-2.5 px-3 border-b border-border-soft ${className}`}>{children}</th>;
}
function Td({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <td className={`py-3 px-3 border-b border-border-soft text-[13px] last:border-b-0 ${className}`}>{children}</td>;
}
```

- [ ] **Step 11.2: Commit**

```bash
git add frontend/web/src/components/tables/TradeLedger.tsx
git commit -m "feat(frontend): TradeLedger paginated component"
```

---

### Task 12: Mount Findings + TradeLedger on Run detail

**Files:**
- Modify: `frontend/web/src/routes/eval-runs-detail.tsx`

- [ ] **Step 12.1: Replace placeholder sections**

In the existing `eval-runs-detail.tsx`, replace the two placeholder cards at the bottom with:

```tsx
import { FindingsList } from "@/components/findings/FindingsList";
import { TradeLedger } from "@/components/tables/TradeLedger";

// inside the JSX:
<div className="grid grid-cols-2 gap-4">
  <FindingsList runId={s.run_id} />
  <TradeLedger runId={s.run_id} />
</div>
```

- [ ] **Step 12.2: Commit**

```bash
git add frontend/web/src/routes/eval-runs-detail.tsx
git commit -m "feat(frontend): mount Findings + TradeLedger on Run detail"
```

---

### Task 13: Full Compare runs view

**Files:**
- Modify: `frontend/web/src/routes/eval-compare.tsx`

- [ ] **Step 13.1: Implement overlay + side-by-side**

Replace `frontend/web/src/routes/eval-compare.tsx`:

```tsx
import { useQueries } from "@tanstack/react-query";
import { useSearchParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { KpiTile } from "@/components/kpi/KpiTile";
import { evalApi } from "@/api/eval";
import { findingsApi } from "@/api/findings";
import { FindingCard } from "@/components/findings/FindingCard";

const COLORS = ["var(--gold)", "#6F8FB8", "#DB9230"];   // up to 3

export default function EvalCompare() {
  const [params] = useSearchParams();
  const ids = (params.get("ids") ?? "").split(",").filter(Boolean).slice(0, 3);
  const runs = useQueries({
    queries: ids.map((id) => ({ queryKey: ["eval", "run", id], queryFn: () => evalApi.get(id) })),
  });
  const findings = useQueries({
    queries: ids.map((id) => ({ queryKey: ["findings", id], queryFn: () => findingsApi.list(id) })),
  });

  if (ids.length < 2) {
    return (
      <>
        <Topbar title="Compare runs" />
        <div className="text-text-2 text-sm">Select 2 or 3 runs from the runs list to compare.</div>
      </>
    );
  }

  const allLoaded = runs.every((q) => q.data);
  if (!allLoaded) {
    return (
      <>
        <Topbar title="Compare runs" />
        <div className="text-text-2 text-sm">Loading…</div>
      </>
    );
  }

  // Compute combined min/max for the overlay
  const allPoints = runs.flatMap((q) => q.data!.equity_series.map((p) => p.value_pct));
  const min = Math.min(...allPoints);
  const max = Math.max(...allPoints);
  const range = max - min || 1;

  const W = 900, H = 240;

  return (
    <>
      <Topbar title="Compare runs" sub={`${ids.length} runs`} />

      <Card className="mb-5">
        <div className="px-5 py-4">
          <h2 className="font-serif font-medium text-[22px] m-0">Equity overlay</h2>
        </div>
        <div className="px-5 pb-5">
          <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none">
            {[0, 0.25, 0.5, 0.75, 1].map((t) => (
              <line key={t} x1={0} x2={W} y1={t * H} y2={t * H} stroke="var(--border)" strokeDasharray="2 4" strokeWidth="0.5" />
            ))}
            {runs.map((q, i) => {
              const series = q.data!.equity_series;
              if (series.length < 2) return null;
              const path = "M" + series.map((p, j) => {
                const x = (j / (series.length - 1)) * W;
                const y = H - ((p.value_pct - min) / range) * (H - 8) - 4;
                return `${x},${y}`;
              }).join(" L");
              return <path key={ids[i]} d={path} fill="none" stroke={COLORS[i]} strokeWidth="1.5" />;
            })}
          </svg>
          <div className="flex gap-4 text-xs text-text-2 mt-2">
            {ids.map((id, i) => (
              <span key={id}>
                <span className="inline-block w-3 h-px align-middle mr-2" style={{ background: COLORS[i] }} />
                <span className="font-mono">{id}</span>
              </span>
            ))}
          </div>
        </div>
      </Card>

      <div className={`grid gap-4 ${ids.length === 2 ? "grid-cols-2" : "grid-cols-3"}`}>
        {runs.map((q, i) => {
          const r = q.data!;
          const f = findings[i].data ?? [];
          return (
            <Card key={ids[i]} className="p-5">
              <div className="font-mono text-text mb-1">{r.summary.run_id}</div>
              <div className="text-text-2 text-xs mb-4">{r.summary.strategy} · {r.summary.scenario}</div>
              <div className="grid grid-cols-2 gap-2 mb-5">
                <KpiTile label="Sharpe" value={r.summary.sharpe?.toFixed(2) ?? "—"} />
                <KpiTile
                  label="Return"
                  value={r.summary.return_pct != null ? `${r.summary.return_pct.toFixed(1)}%` : "—"}
                  tone={r.summary.return_pct != null && r.summary.return_pct >= 0 ? "up" : "down"}
                />
                <KpiTile label="Max DD" value={r.summary.max_dd_pct != null ? `${r.summary.max_dd_pct.toFixed(1)}%` : "—"} tone="down" />
                <KpiTile label="Win rate" value={r.summary.win_rate_pct != null ? `${r.summary.win_rate_pct.toFixed(0)}%` : "—"} />
              </div>
              <div className="font-serif text-base mb-2">Findings ({f.length})</div>
              <div className="flex flex-col gap-3 max-h-96 overflow-y-auto">
                {f.length === 0 && <div className="text-text-3 text-xs">No findings.</div>}
                {f.map((x) => <FindingCard key={x.finding_id} f={x} />)}
              </div>
            </Card>
          );
        })}
      </div>
    </>
  );
}
```

- [ ] **Step 13.2: Commit**

```bash
git add frontend/web/src/routes/eval-compare.tsx
git commit -m "feat(frontend): full Compare view with equity overlay + side-by-side findings"
```

---

### Task 14: Command palette ⌘K

**Files:**
- Create: `frontend/web/src/stores/command-palette.ts`
- Create: `frontend/web/src/hooks/useCmdK.ts`
- Create: `frontend/web/src/components/chrome/CommandPalette.tsx`
- Modify: `frontend/web/src/components/shell/AppShell.tsx`

- [ ] **Step 14.1: Store**

Create `frontend/web/src/stores/command-palette.ts`:

```ts
import { create } from "zustand";

export const useCmdKStore = create<{ open: boolean; setOpen: (v: boolean) => void }>((set) => ({
  open: false,
  setOpen: (open) => set({ open }),
}));
```

- [ ] **Step 14.2: Global keybind**

Create `frontend/web/src/hooks/useCmdK.ts`:

```ts
import { useEffect } from "react";
import { useCmdKStore } from "@/stores/command-palette";

export function useCmdK() {
  const setOpen = useCmdKStore((s) => s.setOpen);
  useEffect(() => {
    function onKey(e: KeyboardEvent) {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setOpen(true);
      }
    }
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [setOpen]);
}
```

- [ ] **Step 14.3: `CommandPalette`**

Create `frontend/web/src/components/chrome/CommandPalette.tsx`:

```tsx
import { useEffect, useState } from "react";
import * as Dialog from "@radix-ui/react-dialog";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Icon } from "@/components/primitives/Icon";
import { useCmdKStore } from "@/stores/command-palette";
import { searchApi } from "@/api/search";
import type { SearchHit } from "@/api/types.gen";

type Action = { id: string; label: string; hint?: string; run: () => void };

export function CommandPalette() {
  const open = useCmdKStore((s) => s.open);
  const setOpen = useCmdKStore((s) => s.setOpen);
  const nav = useNavigate();
  const [q, setQ] = useState("");
  const [active, setActive] = useState(0);

  const actions: Action[] = [
    { id: "new-strategy", label: "Create new strategy", hint: "/setup", run: () => nav("/setup") },
    { id: "go-strategies", label: "Go to Strategies", run: () => nav("/strategies") },
    { id: "go-runs", label: "Go to Eval runs", run: () => nav("/eval/runs") },
    { id: "go-settings", label: "Open Settings", run: () => nav("/settings/providers") },
  ];

  const { data: hits = [] } = useQuery({
    queryKey: ["search", q],
    queryFn: () => searchApi.query(q),
    enabled: q.trim().length > 0,
  });

  useEffect(() => {
    if (!open) { setQ(""); setActive(0); }
  }, [open]);

  const filteredActions = q.trim() === ""
    ? actions
    : actions.filter((a) => a.label.toLowerCase().includes(q.toLowerCase()));
  const items: { kind: "action" | "hit"; key: string; label: string; sub?: string; run: () => void }[] = [
    ...filteredActions.map((a) => ({
      kind: "action" as const,
      key: a.id,
      label: a.label,
      sub: a.hint,
      run: () => { a.run(); setOpen(false); },
    })),
    ...hits.map((h) => ({
      kind: "hit" as const,
      key: `${h.kind}:${h.ref_id}`,
      label: `${h.kind}: ${h.title}`,
      sub: h.snippet,
      run: () => {
        if (h.kind === "strategy") nav(`/authoring/${h.ref_id}`);
        else if (h.kind === "run") nav(`/eval/runs/${h.ref_id}`);
        else if (h.kind === "finding") {
          const [, runId] = h.ref_id.split(":");
          nav(`/eval/runs/${runId}#finding-${h.ref_id}`);
        }
        setOpen(false);
      },
    })),
  ];

  function onKeyDown(e: React.KeyboardEvent) {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setActive((a) => Math.min(items.length - 1, a + 1));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setActive((a) => Math.max(0, a - 1));
    } else if (e.key === "Enter") {
      e.preventDefault();
      items[active]?.run();
    }
  }

  return (
    <Dialog.Root open={open} onOpenChange={setOpen}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/50" />
        <Dialog.Content
          className="fixed left-1/2 top-[15vh] -translate-x-1/2 bg-surface-card border border-border rounded-card w-[640px] max-w-[90vw] max-h-[70vh] flex flex-col overflow-hidden"
          onKeyDown={onKeyDown}
        >
          <Dialog.Title className="sr-only">Command palette</Dialog.Title>
          <div className="flex items-center gap-2 px-4 py-3 border-b border-border-soft">
            <Icon name="search" size={14} color="var(--text-3)" />
            <input
              autoFocus
              value={q}
              onChange={(e) => { setQ(e.target.value); setActive(0); }}
              placeholder="Jump to anything…"
              className="flex-1 bg-transparent text-text outline-none text-[14px]"
            />
            <span className="font-mono text-[10px] text-text-3 border border-border-strong rounded-sm px-1.5 py-0.5">esc</span>
          </div>
          <div className="overflow-y-auto flex-1 py-2">
            {items.length === 0 && <div className="text-text-3 text-sm px-4 py-8 text-center">No results.</div>}
            {items.map((it, i) => (
              <button
                key={it.key}
                onMouseEnter={() => setActive(i)}
                onClick={it.run}
                className={`w-full text-left px-4 py-2.5 flex items-center justify-between ${
                  i === active ? "bg-[rgba(212,165,71,0.08)]" : ""
                }`}
              >
                <div>
                  <div className="text-text text-sm">{it.label}</div>
                  {it.sub && <div className="text-text-3 text-xs mt-0.5 truncate">{it.sub}</div>}
                </div>
                {it.kind === "action" && <span className="text-text-3 text-[10px]">action</span>}
                {it.kind === "hit" && <span className="text-gold text-[10px]">↵</span>}
              </button>
            ))}
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
```

- [ ] **Step 14.4: Mount in `AppShell` and bind ⌘K**

In `AppShell.tsx`:

```tsx
import { CommandPalette } from "@/components/chrome/CommandPalette";
import { useCmdK } from "@/hooks/useCmdK";

// inside the function:
useCmdK();

// in JSX, after <ToastRegion />:
<CommandPalette />
```

Also update the topbar's `⌘K` chip to be clickable, opening the palette:

```tsx
// In Topbar.tsx, wrap the cmdk div in a button that calls useCmdKStore.setOpen(true).
```

- [ ] **Step 14.5: Commit**

```bash
git add frontend/web/src/stores/ frontend/web/src/hooks/ frontend/web/src/components/chrome/CommandPalette.tsx frontend/web/src/components/shell/
git commit -m "feat(frontend): command palette ⌘K with FTS5 search + actions"
```

---

### Task 15: Error boundaries on every route

**Files:**
- Create: `frontend/web/src/components/chrome/ErrorBoundary.tsx`
- Modify: `frontend/web/src/routes.tsx`

- [ ] **Step 15.1: Boundary component**

Create `frontend/web/src/components/chrome/ErrorBoundary.tsx`:

```tsx
import { useRouteError, isRouteErrorResponse } from "react-router-dom";
import { ApiError } from "@/api/client";

export function RouteError() {
  const e = useRouteError();
  let title = "Something went wrong";
  let detail = "";
  if (e instanceof ApiError) {
    title = e.body.message;
    detail = `(${e.body.code}, status ${e.status})`;
  } else if (isRouteErrorResponse(e)) {
    title = e.statusText;
    detail = String(e.data ?? "");
  } else if (e instanceof Error) {
    title = e.message;
  }

  return (
    <div className="bg-surface-card border border-danger/40 rounded-card p-6 m-9">
      <div className="text-danger font-medium mb-1">{title}</div>
      {detail && <div className="text-text-2 text-xs">{detail}</div>}
      <button onClick={() => location.reload()} className="mt-4 border border-border text-text-2 rounded-sm px-3 py-1.5 text-xs">
        Reload
      </button>
    </div>
  );
}
```

- [ ] **Step 15.2: Attach to every route**

In `routes.tsx`, add `errorElement` to each route:

```tsx
import { RouteError } from "@/components/chrome/ErrorBoundary";

// in createBrowserRouter:
{
  path: "/",
  element: <AppShell />,
  errorElement: <RouteError />,
  children: [
    { index: true, element: <Home />, errorElement: <RouteError /> },
    // ...repeat for every child
  ],
}
```

- [ ] **Step 15.3: Commit**

```bash
git add frontend/web/src/components/chrome/ErrorBoundary.tsx frontend/web/src/routes.tsx
git commit -m "feat(frontend): route-level error boundaries"
```

---

### Task 16: Empty-state polish + a11y pass

**Files:**
- Modify: components and routes that lack designed empty states

- [ ] **Step 16.1: Audit empty states**

Visit the app with a clean DB. For each route, verify the empty state is intentional (not just a blank page):
- `/` → "No data yet" KPIs are fine; ensure equity chart placeholder is shown.
- `/strategies` → handled in Plan 1.
- `/eval/runs` → handled in Plan 2.
- `/eval/runs/:id` → "Run not found" already shown.
- `/eval/compare` → "Select 2 or 3 runs" already shown.
- `/setup` → welcome message renders.
- `/settings/*` → existing.

Fix any that show blank cards by adding "No X yet — do Y to get started" copy.

- [ ] **Step 16.2: Keyboard nav check**

Each interactive element should be keyboard-reachable:
- Tab through Home → reaches each KPI tile, chart, table rows? (Not required, but the tables should let you tab to row links.)
- ⌘K opens palette; ↑↓↵ work; Esc closes.
- Modal dialogs trap focus.
- The chat rail Composer's textarea is reachable.

Where focus traps are missing, audit the Radix Dialog setups (`<Dialog.Content>` traps by default — confirm).

- [ ] **Step 16.3: Color-contrast + aria-label sweep**

Run a quick check:
- Sidebar items have visible focus rings (Tailwind `focus:ring-1 focus:ring-gold-soft` if missing).
- Interactive icons have `aria-label` (e.g., chat rail toggle in Plan 4 already has it).
- All `<button>` elements have text or `aria-label`.

- [ ] **Step 16.4: Commit**

```bash
git add frontend/web/src/
git commit -m "polish(frontend): empty states + a11y sweep"
```

---

### Task 17: Final E2E smoke + DESIGN.md closeout

- [ ] **Step 17.1: Full-loop manual test**

```bash
cargo build --workspace
cargo run -p xvision-cli -- dashboard serve &
sleep 2
```

Navigate the entire v1 surface:
1. `/setup` → talk to wizard → Open in Inspector.
2. Inspector → edit slot → save → live preview updates.
3. `/strategies` → see new draft with "Draft" status.
4. Run a backtest via the CLI: `xvn eval run <bundle> --scenario bull-q1-25` (or trigger from the UI if a button has been added).
5. `/eval/runs` → see the row.
6. `/eval/runs/:id` → KPIs, equity, findings (auto-extracted), trade ledger.
7. Click "Draft variant from this →" on a finding → land on `/setup?seed=…`.
8. Select 2 runs → `/eval/compare?ids=…` → overlay + side-by-side.
9. ⌘K → search "eth" → see strategies + runs + findings.
10. Open chat rail (Plan 4); ask a question scoped to the current run.
11. `/settings/danger` → wipe drafts (after confirming the typed phrase).

- [ ] **Step 17.2: Update README + MANUAL.md**

Append a "v1 Frontend complete" note in `frontend/README.md`:

```markdown
## v1 status

All five frontend plans landed (foundation → read-only → authoring → agent surfaces → findings + compare + polish). The dashboard is feature-complete for v1 scope.
```

- [ ] **Step 17.3: Mark Plan 5 done in DESIGN.md**

In §10, append `✓ landed` to "Phase 4 — polish + missing pieces". Update §11 (open questions) — answer each:
1. "Live deployments" tile → renamed to "Paper deployments" (Plan 2 Task 11).
2. Strategy "Validated" → has eval attestation + zero warnings (Plan 3 Task 2).
3. Findings auto-extract → yes, on run completion (Plan 5 Task 5.8).

- [ ] **Step 17.4: Commit**

```bash
git add frontend/DESIGN.md frontend/README.md
git commit -m "docs: mark Plan 5 phase landed; v1 frontend complete"
```

---

## Self-review

**Spec coverage:** Plan 5 covers DESIGN.md §6.6 (findings + trade ledger), §6.7 (full Compare), §8.1 (command palette), backend gaps #4 (findings), #5 (trade ledger), #15-17 (search, activity feed via api_audit which Plan 2 stubbed and is now indexable). Empty states + error boundaries are §6 (per-screen polish) + §8.2 (toasts already in Plan 2).

**Placeholder scan:** No "TBD". The "(adjust path)" notes in commit commands are wiring flags, not placeholders. Task 5.8's runner integration depends on the eval engine plan's actual file path — flagged inline.

**Type consistency:** `Severity` (`critical|warning|info` rendered) maps to `Dot` tone (`danger|warn|info`). `EvidenceRef` tagged union matches between Rust serde tag="type" rename_all="snake_case" and TS discriminated union (`type: "trade_range" | ...`). `SearchHit` shape matches between Rust and TS.

**Cross-task:** Task 4's `Finding` type used in Tasks 5, 8, 10, 12, 13. Task 6's `Trade` type used in Tasks 8, 11, 12. Task 7's `SearchHit` used in Task 14.

---

## Execution

Plan complete. Subagent-driven (recommended) or inline.

This is the final v1 frontend plan. Once Plans 1–5 land, the dashboard is feature-complete per `frontend/DESIGN.md`.
