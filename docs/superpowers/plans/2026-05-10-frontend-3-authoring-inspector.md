# v1 Frontend — Plan 3: Authoring (Inspector)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the Inspector / authoring screen end-to-end (form-only slot editor — live preview ships with Plan 4) with full bundle CRUD, validation diagnostics, token estimates, bundle status (`Draft`/`Validated`/`Warnings`/`Archived`), and bundle lineage (`parent_bundle_id`). Update the Strategies list to surface the new status + forked-from columns.

**Architecture:** Inspector is a 4-column layout: sidebar (200) · bundle outline (220) · split editor (flex; left pane only in this plan) · validation rail (280). The form is React Hook Form orchestrating one slot at a time, with optimistic updates via TanStack Query. New backend additions: SQLite migration adding `archived BOOLEAN` and `parent_bundle_id TEXT REFERENCES bundles(bundle_id)` to the bundles table; `engine::api::strategy::validate(bundle)` returning `Vec<ValidationDiagnostic>`; computed `status` field exposed on `StrategySummary`; `engine::api::wizard::list_templates()`.

**Tech Stack:** Inherits Plan 1 + Plan 2 stacks. No new client deps.

---

## Scope and split

Plan 3 of 5. Depends on Plan 1 (foundation) and Plan 2 (provides Settings + Topbar polish). Does not block Plan 4 or Plan 5.

## Prerequisites

- Plan 1 + Plan 2 landed.
- Existing `xvision-engine::api::strategy::{list, get, create, update, delete}` functions (per the engine API foundation plan). If `create`/`update`/`delete` are not yet exposed, this plan adds them.

## File structure

```
crates/xvision-core/migrations/
└── 0003_bundle_status_lineage.sql               NEW

crates/xvision-engine/src/api/
├── strategy.rs                                   AUGMENT (status, parent_bundle_id, validate)
└── wizard.rs                                     NEW (list_templates)

crates/xvision-dashboard/src/routes/
├── strategies.rs                                 AUGMENT (POST, PUT, DELETE, validate)
└── wizard.rs                                     NEW (templates listing)

frontend/web/src/
├── api/
│   ├── strategies.ts                             AUGMENT
│   └── wizard.ts                                 NEW (templates only — chat ships in Plan 4)
├── components/
│   ├── editors/
│   │   ├── BundleOutline.tsx                     NEW
│   │   ├── SlotEditor.tsx                        NEW
│   │   ├── ValidationRail.tsx                    NEW
│   │   ├── TokenEstimate.tsx                     NEW
│   │   └── BundleJsonView.tsx                    NEW
│   └── tables/
│       └── StrategiesTable.tsx                   AUGMENT (status + forked-from)
└── routes/
    ├── authoring.tsx                             REPLACE placeholder
    └── strategies.tsx                            AUGMENT (use real status, "New from template")
```

---

## Tasks

### Task 1: Migration — bundle status + lineage columns

**Files:**
- Create: `crates/xvision-core/migrations/0003_bundle_status_lineage.sql`

- [ ] **Step 1.1: Write the migration**

```sql
-- 0003_bundle_status_lineage.sql
-- Add archived flag and parent lineage to bundles.

ALTER TABLE bundles ADD COLUMN archived BOOLEAN NOT NULL DEFAULT 0;
ALTER TABLE bundles ADD COLUMN parent_bundle_id TEXT REFERENCES bundles(bundle_id);

-- Backfill: any bundle with a published_at attestation is implicitly not archived
-- (no-op since default is already FALSE/0).

CREATE INDEX idx_bundles_parent_bundle_id ON bundles(parent_bundle_id);
CREATE INDEX idx_bundles_archived ON bundles(archived);
```

- [ ] **Step 1.2: Verify migration ordering**

Run: `ls crates/xvision-core/migrations/`
Expected: `0001_init.sql`, `0002_rename_setup_to_cycle.sql`, `0003_bundle_status_lineage.sql` (ours).

If migrations are loaded by a `migrations/` runner (check existing pattern in `xvision-core` — usually `include_str!` macro or `refinery`), no further wiring is needed.

- [ ] **Step 1.3: Run migrations against a fresh test DB**

If there's a `cargo test -p xvision-core` test that runs migrations, run it. Else manually:

```bash
sqlite3 /tmp/xvn-test.db < crates/xvision-core/migrations/0001_init.sql
sqlite3 /tmp/xvn-test.db < crates/xvision-core/migrations/0002_rename_setup_to_cycle.sql
sqlite3 /tmp/xvn-test.db < crates/xvision-core/migrations/0003_bundle_status_lineage.sql
sqlite3 /tmp/xvn-test.db ".schema bundles"
```

Expected: schema includes `archived` and `parent_bundle_id` columns.

- [ ] **Step 1.4: Commit**

```bash
git add crates/xvision-core/migrations/0003_bundle_status_lineage.sql
git commit -m "feat(core): bundle archived flag + parent lineage migration"
```

---

### Task 2: Engine — surface `status` and `parent_bundle_id` on `StrategySummary`

**Files:**
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Modify: `crates/xvision-engine/src/bundle/mod.rs` (or wherever the bundle store lives)

- [ ] **Step 2.1: Extend `StrategySummary`**

Find the existing `StrategySummary` struct in `crates/xvision-engine/src/api/strategy.rs`. Add fields:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategySummary {
    pub bundle_id: String,
    pub name: String,
    pub template: String,
    pub parent_bundle_id: Option<String>,
    pub status: BundleStatus,
    pub last_eval: Option<LastEval>,
    pub tokens_per_run: u32,
    #[cfg_attr(feature = "ts-export", ts(type = "string"))]
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BundleStatus {
    Draft,
    Validated,
    Warnings,
    Archived,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LastEval {
    pub run_id: String,
    pub sharpe: f64,
    pub scenario: String,
}
```

- [ ] **Step 2.2: Compute status**

In the same file, add:

```rust
pub(crate) fn compute_status(
    archived: bool,
    has_eval_attestation: bool,
    warnings_count: u32,
) -> BundleStatus {
    if archived {
        BundleStatus::Archived
    } else if warnings_count > 0 {
        BundleStatus::Warnings
    } else if has_eval_attestation {
        BundleStatus::Validated
    } else {
        BundleStatus::Draft
    }
}
```

- [ ] **Step 2.3: Update the `list` query to include the new fields**

Wherever the bundles table is queried (likely in `crates/xvision-engine/src/bundle/store.rs` or similar), update the SELECT to include `archived`, `parent_bundle_id`, and to LEFT JOIN against `eval_attestations`. Example SQL:

```sql
SELECT
  b.bundle_id, b.name, b.template, b.parent_bundle_id, b.archived, b.updated_at,
  ea.run_id, ea.sharpe, ea.scenario,
  (SELECT COUNT(*) FROM bundle_warnings w WHERE w.bundle_id = b.bundle_id) AS warnings_count
FROM bundles b
LEFT JOIN eval_attestations ea ON ea.bundle_id = b.bundle_id AND ea.is_latest = 1
ORDER BY b.updated_at DESC
```

If `bundle_warnings` table doesn't exist (see Task 3), this query temporarily uses `0 AS warnings_count`.

Map rows to `StrategySummary` calling `compute_status(archived, last_eval.is_some(), warnings_count)`.

- [ ] **Step 2.4: Test**

If there's an existing `strategy::list` unit test, update it. Else add:

```rust
#[tokio::test]
async fn list_returns_status_field() {
    let ctx = test_ctx().await;
    seed_bundle(&ctx, "test-1", false /* archived */, None /* parent */).await;
    let resp = list(&ctx).await.unwrap();
    assert!(resp.items.iter().any(|i| i.bundle_id == "test-1"));
    let item = resp.items.iter().find(|i| i.bundle_id == "test-1").unwrap();
    assert_eq!(item.status, BundleStatus::Draft);
}
```

- [ ] **Step 2.5: Commit**

```bash
cargo test -p xvision-engine
cargo xtask gen-types
git add crates/xvision-engine/ frontend/web/src/api/types.gen/
git commit -m "feat(engine): expose computed status + lineage on StrategySummary"
```

---

### Task 3: Engine — `ValidationDiagnostic` + `validate()`

**Files:**
- Create: `crates/xvision-engine/src/api/strategy/validate.rs` (or augment existing strategy.rs)

- [ ] **Step 3.1: Define the diagnostic type**

Add to `crates/xvision-engine/src/api/strategy.rs`:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationDiagnostic {
    pub code: String,            // e.g. "token_budget_exceeded"
    pub severity: ValidationSeverity,
    pub message: String,
    pub hint: Option<String>,
    pub layer: Option<String>,   // e.g. "intern", "regime"
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Error,
    Warning,
    Info,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    pub diagnostics: Vec<ValidationDiagnostic>,
    pub token_estimate: crate::tokens::TokenEstimate,
}
```

- [ ] **Step 3.2: Implement `validate`**

```rust
pub async fn validate(ctx: &ApiContext, bundle_id: &str) -> Result<ValidationReport, ApiError> {
    let bundle = get(ctx, bundle_id).await?.bundle;
    let estimate = crate::tokens::estimate_pipeline_tokens(&bundle, 100 /* decision_points heuristic */);
    let mut diags = Vec::new();

    // Rule 1: token budget
    let soft_limit = 50_000_u32;
    if estimate.total > soft_limit {
        diags.push(ValidationDiagnostic {
            code: "token_budget_exceeded".into(),
            severity: ValidationSeverity::Warning,
            message: format!("Token budget exceeds soft limit ({} > {})", estimate.total, soft_limit),
            hint: Some("Reduce per-slot system prompt length or lower max_tokens".into()),
            layer: None,
        });
    }

    // Rule 2: regime classifier missing fixture
    if bundle.regime_slot.is_some() && !ctx.fixtures().has_chop_fixture().await {
        diags.push(ValidationDiagnostic {
            code: "regime_missing_chop_fixture".into(),
            severity: ValidationSeverity::Warning,
            message: "Regime classifier defined but no chop fixture available".into(),
            hint: Some("Add a chop-regime fixture to validate the classifier".into()),
            layer: Some("regime".into()),
        });
    }

    // Rule 3: empty system prompt
    for (name, slot) in [("intern", &bundle.intern_slot), ("trader", &bundle.trader_slot)] {
        if slot.system_prompt.trim().is_empty() {
            diags.push(ValidationDiagnostic {
                code: "empty_system_prompt".into(),
                severity: ValidationSeverity::Error,
                message: format!("{name} system prompt is empty"),
                hint: Some(format!("Add instructions to the {name} slot")),
                layer: Some(name.into()),
            });
        }
    }

    Ok(ValidationReport {
        diagnostics: diags,
        token_estimate: estimate,
    })
}
```

(Adjust `ctx.fixtures()` and `bundle.{regime_slot,intern_slot,trader_slot}` to match the actual `StrategyBundle` shape — the audit confirmed three LLM slots exist; field names may differ.)

- [ ] **Step 3.3: Test the validator**

```rust
#[tokio::test]
async fn validate_flags_oversized_budget() {
    let ctx = test_ctx().await;
    seed_bundle_with_oversized_prompts(&ctx, "big-1").await;
    let report = validate(&ctx, "big-1").await.unwrap();
    assert!(report.diagnostics.iter().any(|d| d.code == "token_budget_exceeded"));
}

#[tokio::test]
async fn validate_passes_clean_bundle() {
    let ctx = test_ctx().await;
    seed_bundle_clean(&ctx, "clean-1").await;
    let report = validate(&ctx, "clean-1").await.unwrap();
    assert!(report.diagnostics.iter().filter(|d| matches!(d.severity, ValidationSeverity::Error)).count() == 0);
}
```

- [ ] **Step 3.4: Commit**

```bash
cargo test -p xvision-engine
cargo xtask gen-types
git add . && git commit -m "feat(engine): ValidationDiagnostic + strategy::validate()"
```

---

### Task 4: Engine — `wizard::list_templates`

**Files:**
- Create: `crates/xvision-engine/src/api/wizard.rs`
- Modify: `crates/xvision-engine/src/api/mod.rs`

- [ ] **Step 4.1: Define + implement**

Create `crates/xvision-engine/src/api/wizard.rs`:

```rust
use serde::{Deserialize, Serialize};
use crate::api::{ApiContext, ApiError};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Template {
    pub key: String,             // "mean_reversion"
    pub display_name: String,    // "Mean reversion"
    pub one_liner: String,
    pub default_assets: Vec<String>,
    pub default_cadence: String, // "15m"
}

pub async fn list_templates(_ctx: &ApiContext) -> Result<Vec<Template>, ApiError> {
    Ok(vec![
        Template {
            key: "mean_reversion".into(),
            display_name: "Mean reversion".into(),
            one_liner: "Buy oversold, sell overbought; skip chop.".into(),
            default_assets: vec!["ETH/USD".into(), "BTC/USD".into()],
            default_cadence: "15m".into(),
        },
        Template {
            key: "trend_follower".into(),
            display_name: "Trend follower".into(),
            one_liner: "Ride sustained directional moves.".into(),
            default_assets: vec!["BTC/USD".into()],
            default_cadence: "1h".into(),
        },
        Template {
            key: "stat_arb".into(),
            display_name: "Statistical arbitrage".into(),
            one_liner: "Trade pair divergences with rapid revert.".into(),
            default_assets: vec!["ETH/USD".into(), "BTC/USD".into()],
            default_cadence: "5m".into(),
        },
        Template {
            key: "carry".into(),
            display_name: "Carry / funding flow".into(),
            one_liner: "Capture funding-rate edges in stables and perps.".into(),
            default_assets: vec!["USDC".into()],
            default_cadence: "1h".into(),
        },
    ])
}
```

In `api/mod.rs`: `pub mod wizard;`.

- [ ] **Step 4.2: Commit**

```bash
cargo xtask gen-types
git add . && git commit -m "feat(engine): wizard::list_templates with v1 catalog"
```

---

### Task 5: Dashboard — strategies POST/PUT/DELETE/validate + wizard/templates

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Create: `crates/xvision-dashboard/src/routes/wizard.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`

- [ ] **Step 5.1: Add CRUD handlers**

Append to `crates/xvision-dashboard/src/routes/strategies.rs`:

```rust
use axum::extract::Path;
use axum::http::StatusCode;
use xvision_engine::api::strategy::{
    self, BundleStatus, CreateRequest, StrategyBundle, UpdateRequest, ValidationReport,
};

pub async fn get_handler(Path(id): Path<String>) -> Result<Json<StrategyBundle>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(strategy::get(&ctx, &id).await.map_err(map_api_err)?.bundle))
}

pub async fn create_handler(Json(req): Json<CreateRequest>) -> Result<Json<StrategyBundle>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(strategy::create(&ctx, req).await.map_err(map_api_err)?))
}

pub async fn update_handler(
    Path(id): Path<String>,
    Json(req): Json<UpdateRequest>,
) -> Result<Json<StrategyBundle>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(strategy::update(&ctx, &id, req).await.map_err(map_api_err)?))
}

pub async fn delete_handler(Path(id): Path<String>) -> Result<StatusCode, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    strategy::delete(&ctx, &id).await.map_err(map_api_err)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn validate_handler(Path(id): Path<String>) -> Result<Json<ValidationReport>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(strategy::validate(&ctx, &id).await.map_err(map_api_err)?))
}
```

- [ ] **Step 5.2: Add wizard route file**

Create `crates/xvision-dashboard/src/routes/wizard.rs`:

```rust
use axum::Json;
use xvision_engine::api::wizard::{list_templates, Template};

use crate::context::build_context;
use crate::error::DashboardError;
use crate::routes::strategies::map_api_err;

pub async fn templates_handler() -> Result<Json<Vec<Template>>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    Ok(Json(list_templates(&ctx).await.map_err(map_api_err)?))
}
```

In `routes/mod.rs`: `pub mod wizard;`.

- [ ] **Step 5.3: Register routes**

In `server.rs`:

```rust
use axum::routing::{delete, get, post, put};

// inside build_router, before .fallback:
.route("/api/strategies",
    get(crate::routes::strategies::list).post(crate::routes::strategies::create_handler))
.route("/api/strategies/:id",
    get(crate::routes::strategies::get_handler)
        .put(crate::routes::strategies::update_handler)
        .delete(crate::routes::strategies::delete_handler))
.route("/api/strategies/:id/validate", get(crate::routes::strategies::validate_handler))
.route("/api/wizard/templates", get(crate::routes::wizard::templates_handler))
```

- [ ] **Step 5.4: Tests**

Append to `tests/http.rs`:

```rust
#[tokio::test]
async fn wizard_templates_returns_four() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let response = server.get("/api/wizard/templates").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body.as_array().unwrap().len(), 4);
}

#[tokio::test]
async fn create_then_get_strategy_roundtrip() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();
    let create_body = serde_json::json!({
        "name": "test-roundtrip",
        "template": "mean_reversion",
        "parent_bundle_id": null
    });
    let response = server.post("/api/strategies").json(&create_body).await;
    response.assert_status_ok();
    let id = response.json::<serde_json::Value>()["bundle_id"].as_str().unwrap().to_string();
    let get_response = server.get(&format!("/api/strategies/{id}")).await;
    get_response.assert_status_ok();
}
```

- [ ] **Step 5.5: Commit**

```bash
cargo test -p xvision-dashboard
cargo xtask gen-types
git add . && git commit -m "feat(dashboard): strategies CRUD + validate + wizard/templates routes"
```

---

### Task 6: Frontend — strategies API client extended

**Files:**
- Modify: `frontend/web/src/api/strategies.ts`
- Create: `frontend/web/src/api/wizard.ts`

- [ ] **Step 6.1: Extend strategies API**

Replace `frontend/web/src/api/strategies.ts`:

```ts
import { apiFetch } from "./client";
import type {
  StrategySummary,
  StrategyBundle,
  CreateRequest,
  UpdateRequest,
  ValidationReport,
} from "./types.gen";

export type StrategiesListResponse = { items: StrategySummary[] };

export const strategiesApi = {
  list: () => apiFetch<StrategiesListResponse>("/api/strategies"),
  get: (id: string) => apiFetch<StrategyBundle>(`/api/strategies/${encodeURIComponent(id)}`),
  create: (input: CreateRequest) =>
    apiFetch<StrategyBundle>("/api/strategies", { method: "POST", body: JSON.stringify(input) }),
  update: (id: string, input: UpdateRequest) =>
    apiFetch<StrategyBundle>(`/api/strategies/${encodeURIComponent(id)}`, {
      method: "PUT",
      body: JSON.stringify(input),
    }),
  delete: (id: string) =>
    apiFetch<void>(`/api/strategies/${encodeURIComponent(id)}`, { method: "DELETE" }),
  validate: (id: string) =>
    apiFetch<ValidationReport>(`/api/strategies/${encodeURIComponent(id)}/validate`),
};
```

- [ ] **Step 6.2: Wizard API**

Create `frontend/web/src/api/wizard.ts`:

```ts
import { apiFetch } from "./client";
import type { Template } from "./types.gen";

export const wizardApi = {
  templates: () => apiFetch<Template[]>("/api/wizard/templates"),
};
```

- [ ] **Step 6.3: Commit**

```bash
git add frontend/web/src/api/
git commit -m "feat(frontend): extend strategies + wizard API clients"
```

---

### Task 7: Component — `BundleOutline`

**Files:**
- Create: `frontend/web/src/components/editors/BundleOutline.tsx`

- [ ] **Step 7.1: Implement**

Create `frontend/web/src/components/editors/BundleOutline.tsx`:

```tsx
import { clsx } from "clsx";

export type OutlineLayer =
  | "data" | "regime" | "intern" | "trader" | "rules" | "risk" | "execution";

const LAYERS: { key: OutlineLayer; index: string; label: string; tag?: string }[] = [
  { key: "data",       index: "①", label: "Data" },
  { key: "regime",     index: "②", label: "Regime classifier", tag: "LLM" },
  { key: "intern",     index: "③", label: "Intern", tag: "LLM" },
  { key: "trader",     index: "④", label: "Trader", tag: "LLM" },
  { key: "rules",      index: "⑤", label: "Entry / Exit rules" },
  { key: "risk",       index: "⑥", label: "Risk" },
  { key: "execution",  index: "⑦", label: "Execution" },
];

type Props = {
  active: OutlineLayer;
  onSelect: (layer: OutlineLayer) => void;
  warningsCount: number;
  errorsCount: number;
  bundleIdShort?: string;
};

export function BundleOutline({ active, onSelect, warningsCount, errorsCount, bundleIdShort }: Props) {
  return (
    <aside className="bg-surface-sidebar border-r border-border-soft px-4 py-6 overflow-hidden flex flex-col">
      <Section label="Manifest">
        <Row text="Identity" />
        <Row text="Eval attestations" />
      </Section>

      <Section label="Layers">
        {LAYERS.map((l) => (
          <button
            key={l.key}
            onClick={() => onSelect(l.key)}
            className={clsx(
              "px-2.5 py-1.5 text-[13px] flex justify-between items-center rounded-sm border-l-2 text-left",
              active === l.key
                ? "text-text bg-[rgba(212,165,71,0.08)] border-gold"
                : "text-text-2 border-transparent hover:text-text",
            )}
          >
            <span>
              <span className="text-text-3 mr-2">{l.index}</span>
              {l.label}
            </span>
            {l.tag && (
              <span className="text-[9px] text-gold border border-[rgba(212,165,71,0.3)] px-1 py-px rounded-sm">
                {l.tag}
              </span>
            )}
          </button>
        ))}
      </Section>

      <Section label="Validation">
        <div className={clsx("px-2.5 py-1.5 text-[13px]",
          errorsCount > 0 ? "text-danger" : warningsCount > 0 ? "text-warn" : "text-gold")}>
          <span className={clsx("inline-block w-1.5 h-1.5 rounded-full mr-2 align-middle",
            errorsCount > 0 ? "bg-danger" : warningsCount > 0 ? "bg-warn" : "bg-gold")} />
          {errorsCount} errors, {warningsCount} warnings
        </div>
      </Section>

      {bundleIdShort && (
        <div className="text-[11px] text-text-3 font-mono mt-auto pt-4">
          Bundle: {bundleIdShort}
        </div>
      )}
    </aside>
  );
}

function Section({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div className="mb-5">
      <div className="text-[11px] text-text-3 uppercase tracking-wider mb-2.5">{label}</div>
      <div className="flex flex-col gap-0.5">{children}</div>
    </div>
  );
}

function Row({ text }: { text: string }) {
  return <div className="px-2.5 py-1.5 text-[13px] text-text-2">{text}</div>;
}
```

- [ ] **Step 7.2: Commit**

```bash
git add frontend/web/src/components/editors/BundleOutline.tsx
git commit -m "feat(frontend): BundleOutline component"
```

---

### Task 8: Component — `SlotEditor`

**Files:**
- Create: `frontend/web/src/components/editors/SlotEditor.tsx`

- [ ] **Step 8.1: Implement**

Create `frontend/web/src/components/editors/SlotEditor.tsx`:

```tsx
import { useFieldArray, useForm, Controller } from "react-hook-form";
import { Pill } from "@/components/primitives/Pill";

export type SlotForm = {
  model: string;
  system_prompt: string;
  tools_allowed: { name: string }[];
  max_tokens: number;
  enabled: boolean;
};

const MODEL_OPTIONS = [
  "anthropic/claude-haiku-4-5",
  "anthropic/claude-sonnet-4-6",
  "anthropic/claude-opus-4-7",
  "openai/gpt-4o",
  "openai/o4-mini",
];

type Props = {
  initial: SlotForm;
  onSubmit: (values: SlotForm) => void;
  layerLabel: string;     // "Intern", "Trader", "Regime classifier"
  bundleName: string;
  submitting: boolean;
};

export function SlotEditor({ initial, onSubmit, layerLabel, bundleName, submitting }: Props) {
  const { register, handleSubmit, control } = useForm<SlotForm>({ defaultValues: initial });
  const { fields, append, remove } = useFieldArray({ control, name: "tools_allowed" });

  return (
    <form onSubmit={handleSubmit(onSubmit)} className="bg-surface-card border border-border rounded-card px-5 py-5 overflow-hidden flex flex-col gap-4 h-full">
      <div className="flex justify-between items-center">
        <span className="font-serif text-[18px]">Slot configuration</span>
        <label className="text-[11px] text-text-2 flex items-center gap-2">
          <input type="checkbox" {...register("enabled")} className="accent-gold" />
          Use this agent
        </label>
      </div>

      <Field label="Model">
        <select className="input-base font-mono" {...register("model")}>
          {MODEL_OPTIONS.map((m) => <option key={m} value={m}>{m}</option>)}
        </select>
      </Field>

      <Field label="System prompt">
        <textarea
          rows={12}
          className="input-base font-mono text-[11.5px] leading-relaxed resize-y min-h-[220px]"
          {...register("system_prompt")}
        />
      </Field>

      <div className="flex gap-3">
        <div className="flex-1">
          <Field label="Tools allowed">
            <div className="flex gap-1.5 flex-wrap">
              {fields.map((f, i) => (
                <span key={f.id} className="inline-flex items-center gap-1.5">
                  <Controller
                    name={`tools_allowed.${i}.name` as const}
                    control={control}
                    render={({ field }) => (
                      <input
                        {...field}
                        className="bg-transparent border border-border text-text-2 rounded-sm px-2 py-0.5 text-[11px] w-32"
                      />
                    )}
                  />
                  <button type="button" onClick={() => remove(i)} className="text-text-3 text-xs">×</button>
                </span>
              ))}
              <button
                type="button"
                onClick={() => append({ name: "" })}
                className="text-gold text-[11px] border border-[rgba(212,165,71,0.35)] rounded-sm px-2 py-0.5"
              >
                + Add
              </button>
            </div>
          </Field>
        </div>
        <div className="w-32">
          <Field label="Max tokens">
            <input
              type="number"
              className="input-base font-mono"
              {...register("max_tokens", { valueAsNumber: true, min: 1 })}
            />
          </Field>
        </div>
      </div>

      <div className="flex gap-2 mt-auto pt-3 border-t border-border-soft">
        <button type="submit" disabled={submitting} className="bg-gold text-bg rounded-sm px-3.5 py-2 text-sm font-medium disabled:opacity-50">
          {submitting ? "Saving…" : "Save draft"}
        </button>
        <Pill className="ml-auto self-center">{bundleName} · {layerLabel}</Pill>
      </div>
    </form>
  );
}

function Field({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <label className="flex flex-col gap-1.5">
      <span className="text-[11px] text-text-2 uppercase tracking-wider">{label}</span>
      {children}
    </label>
  );
}
```

- [ ] **Step 8.2: Commit**

```bash
git add frontend/web/src/components/editors/SlotEditor.tsx
git commit -m "feat(frontend): SlotEditor with model/prompt/tools/max-tokens form"
```

---

### Task 9: Components — `ValidationRail`, `TokenEstimate`, `BundleJsonView`

**Files:**
- Create: `frontend/web/src/components/editors/ValidationRail.tsx`
- Create: `frontend/web/src/components/editors/TokenEstimate.tsx`
- Create: `frontend/web/src/components/editors/BundleJsonView.tsx`

- [ ] **Step 9.1: `TokenEstimate`**

```tsx
import type { TokenEstimate } from "@/api/types.gen";

export function TokenEstimateView({ data }: { data: TokenEstimate }) {
  return (
    <div className="font-mono text-xs flex flex-col gap-1.5">
      <Row label="input" value={data.input.toLocaleString()} />
      <Row label="output" value={data.output.toLocaleString()} />
      <hr className="border-t border-border-soft my-1" />
      <Row label="total" value={data.total.toLocaleString()} highlight />
    </div>
  );
}

function Row({ label, value, highlight }: { label: string; value: string; highlight?: boolean }) {
  return (
    <div className="flex justify-between">
      <span className="text-text-3">{label}</span>
      <span className={highlight ? "text-gold" : "text-text"}>{value}</span>
    </div>
  );
}
```

- [ ] **Step 9.2: `ValidationRail`**

```tsx
import type { ValidationDiagnostic, ValidationReport } from "@/api/types.gen";
import { Dot } from "@/components/primitives/Dot";
import { TokenEstimateView } from "./TokenEstimate";
import { BundleJsonView } from "./BundleJsonView";

type Props = {
  report?: ValidationReport;
  bundleJson: object;
};

export function ValidationRail({ report, bundleJson }: Props) {
  return (
    <aside className="bg-surface-sidebar border-l border-border-soft px-5 py-7 flex flex-col gap-5 overflow-y-auto">
      <Section title="Validation">
        {!report ? (
          <div className="text-text-3 text-xs">Loading…</div>
        ) : report.diagnostics.length === 0 ? (
          <div className="text-text-2 text-xs">No issues.</div>
        ) : (
          <div className="flex flex-col gap-2.5 text-xs">
            {report.diagnostics.map((d, i) => (
              <DiagRow key={`${d.code}-${i}`} d={d} />
            ))}
          </div>
        )}
      </Section>

      {report && (
        <Section title="Estimated tokens / run">
          <TokenEstimateView data={report.token_estimate} />
        </Section>
      )}

      <Section title="Bundle JSON">
        <BundleJsonView json={bundleJson} />
      </Section>
    </aside>
  );
}

function DiagRow({ d }: { d: ValidationDiagnostic }) {
  const tone = d.severity === "error" ? "danger" : d.severity === "warning" ? "warn" : "info";
  return (
    <div className="flex gap-2 items-start">
      <span className="mt-1"><Dot tone={tone as any} /></span>
      <div>
        <div className="text-text">{d.message}</div>
        {d.hint && <div className="text-text-3 text-[11px] mt-0.5">{d.hint}</div>}
      </div>
    </div>
  );
}

function Section({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div>
      <h3 className="font-serif text-[18px] m-0 mb-3 text-text">{title}</h3>
      {children}
    </div>
  );
}
```

- [ ] **Step 9.3: `BundleJsonView`**

```tsx
import { useState } from "react";

export function BundleJsonView({ json }: { json: object }) {
  const [expanded, setExpanded] = useState(false);
  const text = JSON.stringify(json, null, 2);
  const preview = expanded ? text : text.split("\n").slice(0, 8).join("\n") + (text.split("\n").length > 8 ? "\n..." : "");
  return (
    <>
      <pre className="bg-surface-elev border border-border rounded-sm p-2.5 font-mono text-[10.5px] text-text-2 leading-snug overflow-auto max-h-64">
        {preview}
      </pre>
      <button
        onClick={() => setExpanded((v) => !v)}
        className="text-gold text-xs mt-2"
      >
        {expanded ? "Collapse" : "Expand"}
      </button>
    </>
  );
}
```

- [ ] **Step 9.4: Commit**

```bash
git add frontend/web/src/components/editors/
git commit -m "feat(frontend): ValidationRail, TokenEstimate, BundleJsonView"
```

---

### Task 10: Implement Inspector route

**Files:**
- Modify: `frontend/web/src/routes/authoring.tsx`

- [ ] **Step 10.1: Replace placeholder**

```tsx
import { useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Pill } from "@/components/primitives/Pill";
import { BundleOutline, OutlineLayer } from "@/components/editors/BundleOutline";
import { SlotEditor, SlotForm } from "@/components/editors/SlotEditor";
import { ValidationRail } from "@/components/editors/ValidationRail";
import { strategiesApi } from "@/api/strategies";
import { useToasts } from "@/components/chrome/ToastRegion";
import type { StrategyBundle } from "@/api/types.gen";

const LAYER_LABELS: Record<OutlineLayer, string> = {
  data: "Data",
  regime: "Regime classifier",
  intern: "Intern",
  trader: "Trader",
  rules: "Entry / Exit rules",
  risk: "Risk",
  execution: "Execution",
};

export default function Authoring() {
  const { bundleId = "" } = useParams();
  const nav = useNavigate();
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const [active, setActive] = useState<OutlineLayer>("intern");

  const { data: bundle, isLoading } = useQuery({
    queryKey: ["strategy", bundleId],
    queryFn: () => strategiesApi.get(bundleId),
    enabled: !!bundleId,
  });
  const { data: report } = useQuery({
    queryKey: ["strategy", bundleId, "validate"],
    queryFn: () => strategiesApi.validate(bundleId),
    enabled: !!bundleId,
  });

  const saveMut = useMutation({
    mutationFn: (form: SlotForm) => {
      if (!bundle) throw new Error("no bundle");
      const slot = active === "intern" ? "intern_slot" : active === "trader" ? "trader_slot" : "regime_slot";
      const updated: StrategyBundle = {
        ...bundle,
        [slot]: {
          ...((bundle as any)[slot] ?? {}),
          model_requirement: form.model,
          system_prompt: form.system_prompt,
          allowed_tools: form.tools_allowed.map((t) => t.name).filter(Boolean),
          max_tokens: form.max_tokens,
          enabled: form.enabled,
        },
      } as any;
      return strategiesApi.update(bundleId, { bundle: updated });
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["strategy", bundleId] });
      qc.invalidateQueries({ queryKey: ["strategy", bundleId, "validate"] });
      push({ title: "Slot saved", kind: "ok" });
    },
    onError: (e: Error) => push({ title: "Save failed", description: e.message, kind: "error" }),
  });

  if (isLoading || !bundle) return <div className="p-8 text-text-2">Loading…</div>;

  const slot = active === "intern"
    ? (bundle as any).intern_slot
    : active === "trader"
    ? (bundle as any).trader_slot
    : active === "regime"
    ? (bundle as any).regime_slot
    : null;

  const isLLMLayer = active === "intern" || active === "trader" || active === "regime";

  const initialForm: SlotForm = slot
    ? {
        model: slot.model_requirement ?? "anthropic/claude-haiku-4-5",
        system_prompt: slot.system_prompt ?? "",
        tools_allowed: (slot.allowed_tools ?? []).map((n: string) => ({ name: n })),
        max_tokens: slot.max_tokens ?? 1200,
        enabled: slot.enabled ?? true,
      }
    : { model: "anthropic/claude-haiku-4-5", system_prompt: "", tools_allowed: [], max_tokens: 1200, enabled: true };

  const errors = (report?.diagnostics ?? []).filter((d) => d.severity === "error").length;
  const warnings = (report?.diagnostics ?? []).filter((d) => d.severity === "warning").length;

  return (
    <div className="grid grid-cols-[220px_1fr_280px] h-full -mx-9 -mt-9">
      <BundleOutline
        active={active}
        onSelect={setActive}
        warningsCount={warnings}
        errorsCount={errors}
        bundleIdShort={bundleId.slice(0, 7) + "…"}
      />

      <div className="px-7 pt-7 pb-0 overflow-y-auto flex flex-col gap-4">
        <div className="flex justify-between items-center">
          <div>
            <div className="text-[11px] text-text-3 uppercase tracking-wider mb-1">
              Authoring · {bundle.name}
            </div>
            <h1 className="font-serif font-medium text-[30px] m-0">
              {LAYER_LABELS[active]}
              {isLLMLayer && <span className="text-text-2 text-base font-sans ml-2">· LLM slot</span>}
            </h1>
          </div>
          <div className="flex gap-2">
            <button onClick={() => nav("/strategies")} className="border border-border text-text-2 rounded-sm px-3 py-2 text-sm">
              Back
            </button>
          </div>
        </div>

        {isLLMLayer ? (
          <SlotEditor
            key={active}
            initial={initialForm}
            onSubmit={(form) => saveMut.mutate(form)}
            layerLabel={LAYER_LABELS[active]}
            bundleName={bundle.name}
            submitting={saveMut.isPending}
          />
        ) : (
          <div className="bg-surface-card border border-border rounded-card p-8 text-text-3 text-sm">
            Non-LLM layer ({LAYER_LABELS[active]}) editing ships in a follow-up — for v1 these are configured via the bundle JSON.
          </div>
        )}
      </div>

      <ValidationRail report={report} bundleJson={bundle as any} />
    </div>
  );
}
```

- [ ] **Step 10.2: Verify**

```bash
cd frontend/web && pnpm dev
# in another terminal:
cargo run -p xvision-cli -- dashboard serve
```

Open http://localhost:5173/strategies, click an existing bundle (need at least one — create one via `xvn strategy new` if empty), then visit `/authoring/<id>`. Confirm:
- Bundle outline lists 7 layers
- Selecting Intern/Trader/Regime shows the form
- Edit the system prompt, click Save, see toast
- Validation rail updates after save

- [ ] **Step 10.3: Commit**

```bash
git add frontend/web/src/routes/authoring.tsx
git commit -m "feat(frontend): Inspector route with slot editing + validation rail"
```

---

### Task 11: Update Strategies list with status + lineage + "New from template"

**Files:**
- Modify: `frontend/web/src/components/tables/StrategiesTable.tsx`
- Modify: `frontend/web/src/routes/strategies.tsx`

- [ ] **Step 11.1: Augment `StrategiesTable`**

Replace `frontend/web/src/components/tables/StrategiesTable.tsx`:

```tsx
import { Link } from "react-router-dom";
import type { StrategySummary } from "@/api/types.gen";
import { Dot } from "@/components/primitives/Dot";
import { fmtRelative } from "@/lib/format";

const STATUS_TONE: Record<string, "gold" | "warn" | "danger" | "muted"> = {
  validated: "gold",
  warnings: "warn",
  draft: "muted",
  archived: "muted",
};

export function StrategiesTable({ rows }: { rows: StrategySummary[] }) {
  if (rows.length === 0) {
    return (
      <div className="bg-surface-card border border-border rounded-card p-12 text-center text-text-2 text-sm">
        No strategies yet. Click <span className="text-gold">New strategy</span> to start.
      </div>
    );
  }
  return (
    <div className="bg-surface-card border border-border rounded-card overflow-x-auto">
      <table className="w-full border-collapse min-w-[900px]">
        <thead>
          <tr className="text-xs text-text-2">
            <Th className="pl-5">Name</Th>
            <Th>Template</Th>
            <Th>Forked from</Th>
            <Th>Status</Th>
            <Th>Last eval</Th>
            <Th className="text-right">Tokens / run</Th>
            <Th className="pr-5">Updated</Th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.bundle_id} className="hover:bg-surface-hover">
              <Td className="pl-5 font-mono text-text">
                <Link to={`/authoring/${r.bundle_id}`} className="text-text hover:text-gold no-underline">{r.name}</Link>
              </Td>
              <Td className="text-text-2">{r.template}</Td>
              <Td className="text-text-2 font-mono">{r.parent_bundle_id ?? "—"}</Td>
              <Td>
                <Dot tone={STATUS_TONE[r.status]} />
                {r.status}
              </Td>
              <Td className="font-mono text-text-2">
                {r.last_eval ? `${r.last_eval.sharpe.toFixed(2)} · ${r.last_eval.scenario}` : "—"}
              </Td>
              <Td className="font-mono text-right">{(r.tokens_per_run / 1000).toFixed(1)}k</Td>
              <Td className="pr-5 text-text-2">{fmtRelative(r.updated_at)}</Td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Th({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <th className={`text-left font-normal py-2.5 px-3 border-b border-border-soft ${className}`}>{children}</th>;
}
function Td({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <td className={`py-3 px-3 border-b border-border-soft text-[13px] last:border-b-0 ${className}`}>{children}</td>;
}
```

- [ ] **Step 11.2: Augment Strategies route with "New" buttons + template modal**

Replace `frontend/web/src/routes/strategies.tsx`:

```tsx
import { useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import * as Dialog from "@radix-ui/react-dialog";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { StrategiesTable } from "@/components/tables/StrategiesTable";
import { strategiesApi } from "@/api/strategies";
import { wizardApi } from "@/api/wizard";
import { useToasts } from "@/components/chrome/ToastRegion";

export default function Strategies() {
  const nav = useNavigate();
  const qc = useQueryClient();
  const push = useToasts((s) => s.push);
  const { data, isLoading } = useQuery({ queryKey: ["strategies", "list"], queryFn: () => strategiesApi.list() });
  const { data: templates = [] } = useQuery({ queryKey: ["wizard", "templates"], queryFn: () => wizardApi.templates() });
  const [picking, setPicking] = useState(false);

  const createMut = useMutation({
    mutationFn: (template: string) => strategiesApi.create({ name: `${template}-draft`, template, parent_bundle_id: null }),
    onSuccess: (b) => {
      qc.invalidateQueries({ queryKey: ["strategies", "list"] });
      setPicking(false);
      push({ title: "Draft created", kind: "ok" });
      nav(`/authoring/${b.bundle_id}`);
    },
  });

  return (
    <>
      <Topbar
        title="Strategies"
        sub={data ? `${data.items.length} bundles` : "Loading…"}
      />
      <div className="flex justify-end gap-2 mb-4">
        <button onClick={() => setPicking(true)} className="border border-border text-text rounded-sm px-3.5 py-2 text-sm">
          + New from template
        </button>
        <button onClick={() => createMut.mutate("mean_reversion")} className="bg-gold text-bg rounded-sm px-3.5 py-2 text-sm font-medium">
          + New strategy
        </button>
      </div>

      {isLoading ? <div className="text-text-2 text-sm">Loading…</div> : <StrategiesTable rows={data?.items ?? []} />}

      <Dialog.Root open={picking} onOpenChange={setPicking}>
        <Dialog.Portal>
          <Dialog.Overlay className="fixed inset-0 bg-black/50" />
          <Dialog.Content className="fixed left-1/2 top-1/2 -translate-x-1/2 -translate-y-1/2 bg-surface-card border border-border rounded-card p-6 w-[480px] max-w-[90vw]">
            <Dialog.Title className="font-serif text-[24px] mb-4">Pick a template</Dialog.Title>
            <div className="flex flex-col gap-2">
              {templates.map((t) => (
                <button
                  key={t.key}
                  onClick={() => createMut.mutate(t.key)}
                  disabled={createMut.isPending}
                  className="text-left border border-border rounded-sm p-3 hover:border-gold-soft transition-colors"
                >
                  <div className="text-text">{t.display_name}</div>
                  <div className="text-text-2 text-xs mt-1">{t.one_liner}</div>
                  <div className="text-text-3 text-[11px] mt-1 font-mono">
                    {t.default_assets.join(", ")} · {t.default_cadence}
                  </div>
                </button>
              ))}
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </>
  );
}
```

- [ ] **Step 11.3: Commit**

```bash
git add frontend/web/src/components/tables/StrategiesTable.tsx frontend/web/src/routes/strategies.tsx
git commit -m "feat(frontend): Strategies list with status, lineage, New-from-template"
```

---

### Task 12: E2E smoke + docs

- [ ] **Step 12.1: Full smoke**

```bash
cargo build --workspace
cargo run -p xvision-cli -- dashboard serve --bind 127.0.0.1:8788 &
sleep 2
```

Browser flow:
1. Visit `/strategies`. Click "New from template" → pick "Mean reversion".
2. Should redirect to `/authoring/<id>`. Bundle outline shows 7 layers; Intern selected.
3. Edit system prompt → click Save → toast appears → validation rail updates.
4. Back to `/strategies`. New row visible with status `Draft`.

- [ ] **Step 12.2: Update DESIGN.md**

In §10, append `✓ landed` to "Phase 2 — authoring".

- [ ] **Step 12.3: Commit**

```bash
git add frontend/DESIGN.md
git commit -m "docs: mark Plan 3 phase landed"
```

---

## Self-review

**Spec coverage:** Plan 3 covers DESIGN.md §6.4 (Inspector — minus live preview, deferred to Plan 4) and §6.3 (Strategies list status + lineage). Backend gaps #6 (status), #7 (lineage), #8 (validation diagnostics) closed. Live preview (#9) deferred to Plan 4 — explicit handoff at Task 8.

**Placeholder scan:** No "TBD". The non-LLM-layer placeholder ("ships in a follow-up") is flagged inline.

**Type consistency:** `BundleStatus` (snake_case in serialization → matches CSS class derivation in `STATUS_TONE`). `ValidationDiagnostic.severity` enum values match the Dot tone derivation (`error → danger`, `warning → warn`, `info → info`).

**Cross-task:** Task 11's `StrategiesTable` uses fields added in Task 2. Task 10 calls `strategiesApi.validate` which is wired in Task 5. Task 11 calls `strategiesApi.create` which is wired in Task 5.

---

## Execution

Plan complete. Subagent-driven (recommended) or inline.
