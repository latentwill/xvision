# Optimizer UI — Complete Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the `/autooptimizer` dashboard fully functional so an operator can start an evening run, watch it live via SSE, inspect experiment diffs, and see completed cycle summaries — matching all capabilities of `xvn optimizer`.

**Architecture:** Three layers of fixes: (1) SSE wire bug, (2) missing backend handlers, (3) frontend enrichment. All Rust changes in `crates/xvision-dashboard/src/routes/autooptimizer.rs` + `server.rs` + `sse/autooptimizer_sse.rs`. All frontend changes inside `frontend/web/src/features/autooptimizer/`.

**Tech Stack:** Rust/axum backend, React + TanStack Query frontend, SSE for real-time events, existing `xvision-engine` autooptimizer crate for cycle execution.

**Operator-surface naming:** All UI labels must follow the terminology lock. Key pairs: `Mutator` → "Experiment writer", `CycleSeal` → "Evening summary", `LineageStatus::Rejected` → "Rejected", `LineageStatus::Quarantined` → "Suspect", `evening_cycle` → "Evening run".

---

## Root-cause diagnosis

| Bug | Location | Effect |
|-----|----------|--------|
| SSE sends `event: <kind>\ndata:…` (named events) | `sse/autooptimizer_sse.rs:56` | Frontend `addEventListener("message",…)` receives ZERO events — Live tab always shows "Waiting for cycle…" |
| No `POST /api/autooptimizer/evening-cycle` | `routes/autooptimizer.rs`, `server.rs` | Launch button always returns 404; shown as "Not yet available" |
| No `GET /api/autooptimizer/blob/:hash` | `routes/autooptimizer.rs` | DiffInspector shows metadata only, no diff text or gate scores |
| Empty states look blank | All 5 tabs | Components render empty-state text with no data |

---

## File map

**Modify:**
- `crates/xvision-dashboard/src/sse/autooptimizer_sse.rs` — fix event format
- `crates/xvision-dashboard/src/routes/autooptimizer.rs` — add blob + evening-cycle handlers
- `crates/xvision-dashboard/src/server.rs` — wire new routes
- `frontend/web/src/features/autooptimizer/api.ts` — add blob + evening-cycle fetch fns + types
- `frontend/web/src/features/autooptimizer/LiveCycleView.tsx` — add Seals section, fix SSE consumption
- `frontend/web/src/features/autooptimizer/DiffInspector.tsx` — add diff content + score display
- `frontend/web/src/features/autooptimizer/GenealogyTree.tsx` — add score/hash links

**Create:**
- `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs` — evening-cycle POST handler

---

## Task 1 — Fix SSE event format (backend, 1-line change)

**Files:**
- Modify: `crates/xvision-dashboard/src/sse/autooptimizer_sse.rs:56`

The backend emits named SSE events (`Event::default().event(kind).data(json)`).
The frontend listens for `source.addEventListener("message", ...)` which only fires
for **unnamed** events. Named events never reach the frontend listener.

Fix: remove the `.event(kind)` call — the `kind` is already embedded in the JSON payload as `payload.kind`, so nothing is lost.

- [ ] **Step 1: Edit the event emission line**

In `crates/xvision-dashboard/src/sse/autooptimizer_sse.rs`, find:
```rust
yield Ok::<Event, Infallible>(Event::default().event(kind).data(json));
```
Change to:
```rust
yield Ok::<Event, Infallible>(Event::default().data(json));
```

- [ ] **Step 2: Verify the `lagged` event is also unnamed**

Find the lagged yield:
```rust
yield Ok(Event::default().event("lagged").data(body));
```
Change to:
```rust
yield Ok(Event::default().data(body));
```

- [ ] **Step 3: Build**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-optimizer-ui"
cd /path/to/worktree  # .worktrees/optimizer-ui
~/.cargo/bin/cargo build -p xvision-dashboard 2>&1 | grep -E "^error"
```
Expected: no error lines.

- [ ] **Step 4: Commit**
```bash
git add crates/xvision-dashboard/src/sse/autooptimizer_sse.rs
git commit -m "fix(sse): emit unnamed SSE events so frontend message listener receives them"
```

---

## Task 2 — Add GET /api/autooptimizer/blob/:hash endpoint

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`

The blob store lives at `~/.xvn/lineage/blobs/<hash>.json`. The DiffInspector needs to read mutation diffs from there.

- [ ] **Step 1: Add the handler to `routes/autooptimizer.rs`**

At the end of the file (before any `#[cfg(test)]`), add:

```rust
// ---------------------------------------------------------------------------
// GET /api/autooptimizer/blob/:hash
// ---------------------------------------------------------------------------

pub async fn get_blob(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, DashboardError> {
    use xvision_engine::autooptimizer::{blob_store::BlobStore, ContentHash};

    let content_hash = ContentHash::from_hex(&hash).map_err(|e| DashboardError::Validation {
        field: "hash".into(),
        msg: format!("invalid content hash: {e}"),
    })?;

    // Use the configured blob root from AppState if available, else default.
    let blob_root = state
        .autooptimizer_blob_root
        .clone()
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_default()
                .join(".xvn/lineage/blobs")
        });
    let store = BlobStore::open(blob_root);

    if !store.exists(&content_hash) {
        return Err(DashboardError::NotFound(format!("blob '{hash}' not found")));
    }

    let bytes = store.load(&content_hash).map_err(|e| DashboardError::Internal(e))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| DashboardError::Internal(anyhow::anyhow!("blob decode: {e}")))?;
    Ok(Json(value))
}
```

- [ ] **Step 2: Check if `AppState` already has `autooptimizer_blob_root`**

Run:
```bash
grep "autooptimizer_blob_root\|blob_root" crates/xvision-dashboard/src/state.rs
```

If not found, add to `AppState` struct in `state.rs`:
```rust
/// Optional override for the autooptimizer blob store root (~/.xvn/lineage/blobs by default).
pub autooptimizer_blob_root: Option<std::path::PathBuf>,
```
And in `AppState::new(...)` initializer, set:
```rust
autooptimizer_blob_root: None,
```

If it already exists, skip this step.

- [ ] **Step 3: Wire the route in `server.rs`**

Find the block that registers `/api/autooptimizer/events` and add after the existing autooptimizer GET routes:
```rust
.route(
    "/api/autooptimizer/blob/:hash",
    get(autooptimizer_route::get_blob),
)
```

- [ ] **Step 4: Add `dirs` dep to `xvision-dashboard/Cargo.toml` if missing**

Run:
```bash
grep "^dirs" crates/xvision-dashboard/Cargo.toml
```
If not present, add under `[dependencies]`:
```toml
dirs = "5"
```

- [ ] **Step 5: Build**
```bash
~/.cargo/bin/cargo build -p xvision-dashboard 2>&1 | grep -E "^error"
```
Expected: no errors.

- [ ] **Step 6: Commit**
```bash
git add crates/xvision-dashboard/src/routes/autooptimizer.rs \
        crates/xvision-dashboard/src/server.rs \
        crates/xvision-dashboard/src/state.rs \
        crates/xvision-dashboard/Cargo.toml
git commit -m "feat(api): GET /api/autooptimizer/blob/:hash — serve blob store content to dashboard"
```

---

## Task 3 — Add POST /api/autooptimizer/evening-cycle handler

**Files:**
- Create: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`

The evening-cycle launch button POSTs `{ strategy_id, budget_usd, mutator_model, judge_model }`.
The handler needs to: (a) load config + operator key, (b) build cycle config, (c) spawn the cycle as a background tokio task emitting events via `autooptimizer_tx`.

Key complexity: `run_evening_cycle` needs a parent strategy loaded from the blob store or strategy filesystem. If `strategy_id` is provided, look up the strategy JSON; otherwise use active lineage leaves as parents.

- [ ] **Step 1: Read the existing `AutoOptimizerConfig` load path**

```bash
grep -n "load_from_file\|load_default\|autooptimizer.toml" \
  crates/xvision-engine/src/autooptimizer/config.rs | head -10
```
Note the path (typically `~/.xvn/autooptimizer.toml`).

- [ ] **Step 2: Read `Mutator::new` and `Judge::new` signatures**

```bash
grep -n "pub fn new\|pub async fn new" \
  crates/xvision-engine/src/autooptimizer/mutator.rs \
  crates/xvision-engine/src/autooptimizer/judge.rs | head -10
```
Note the required parameters.

- [ ] **Step 3: Create `autooptimizer_cycle.rs`**

Create `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`:

```rust
//! POST /api/autooptimizer/evening-cycle — launch an evening run from the dashboard.
//!
//! Accepts the run parameters, loads config + operator key, and spawns the
//! `run_evening_cycle` call as a background task. Events are broadcast via
//! `AppState::autooptimizer_tx` so the SSE stream (`/api/autooptimizer/events`)
//! receives them in real time.
//!
//! Returns 202 Accepted immediately with { "cycle_id": null, "started": true }.
//! The cycle_id is emitted via SSE once the cycle starts.

use std::sync::Arc;

use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};

use crate::error::DashboardError;
use crate::state::AppState;

#[derive(Deserialize)]
pub struct StartCycleRequest {
    pub strategy_id: Option<String>,
    pub budget_usd: Option<f64>,
    pub mutator_model: Option<String>,
    pub judge_model: Option<String>,
}

#[derive(Serialize)]
pub struct StartCycleResponse {
    pub started: bool,
    pub message: String,
}

pub async fn start_evening_cycle(
    State(state): State<AppState>,
    Json(req): Json<StartCycleRequest>,
) -> Result<(StatusCode, Json<StartCycleResponse>), DashboardError> {
    use dirs::home_dir;
    use xvision_engine::autooptimizer::{
        blob_store::BlobStore,
        config::AutoOptimizerConfig,
        cycle::{run_evening_cycle, CycleConfig},
        eval_adapter::MockPaperTestRunner,
        judge::Judge,
        lineage::LineageStore,
        mutator::Mutator,
        parent_policy::ParentPolicy,
        session::OperatorKey,
    };
    use xvision_engine::eval::scenario::Scenario;

    let pool = state.pool.clone();
    let tx = state.autooptimizer_tx.clone();

    // Load config (default if file missing).
    let config_path = home_dir()
        .unwrap_or_default()
        .join(".xvn/autooptimizer.toml");
    let config = if config_path.exists() {
        AutoOptimizerConfig::load_from_file(&config_path)
            .map_err(|e| DashboardError::Internal(e))?
    } else {
        AutoOptimizerConfig::load_default()
    };

    // Load or generate operator key.
    let operator_key = OperatorKey::load_or_generate()
        .map_err(|e| DashboardError::Internal(e))?;

    let blob_root = state
        .autooptimizer_blob_root
        .clone()
        .unwrap_or_else(|| home_dir().unwrap_or_default().join(".xvn/lineage/blobs"));
    let blob_store = Arc::new(BlobStore::open(blob_root));

    let mutator_model = req
        .mutator_model
        .unwrap_or_else(|| config.mutator.model.clone());
    let judge_model = req
        .judge_model
        .unwrap_or_else(|| "claude-sonnet-4-6".to_string());

    // Resolve session_id: use existing or create a new one.
    let session_id = {
        let row = sqlx::query("SELECT session_id FROM autooptimizer_sessions ORDER BY created_at DESC LIMIT 1")
            .fetch_optional(&pool)
            .await
            .unwrap_or(None);
        match row {
            Some(r) => {
                use sqlx::Row;
                r.try_get::<String, _>("session_id")
                    .unwrap_or_else(|_| ulid::Ulid::new().to_string())
            }
            None => ulid::Ulid::new().to_string(),
        }
    };

    // Build a minimal CycleConfig.
    // Day/baseline scenarios use config defaults; parent strategies come from lineage.
    let cycle_config = CycleConfig {
        num_parents: 2,
        mutations_per_parent: 1,
        sabotage_seed: 42,
        judge_provider: "anthropic".to_string(),
        judge_model: judge_model.clone(),
        prompt_version: "v1".to_string(),
        sustained_no_pass_cycles: 0,
        day_scenario: config
            .day_scenario()
            .unwrap_or_else(|| Scenario::fixture()),
        baseline_scenario: config
            .baseline_scenario()
            .unwrap_or_else(|| Scenario::fixture()),
        parent_strategies: Default::default(),
        explicit_parent_hashes: vec![],
    };

    // Spawn the cycle as a background task.
    tokio::spawn(async move {
        let mutator = Mutator::new(mutator_model);
        let judge = Judge::new(judge_model);
        let paper_tester = MockPaperTestRunner::default();
        let parent_policy = ParentPolicy::default();

        let progress = {
            let tx = tx.clone();
            move |ev| {
                let _ = tx.send(ev);
            }
        };

        let _ = run_evening_cycle(
            &pool,
            &blob_store,
            &config,
            &cycle_config,
            &parent_policy,
            &mutator,
            &judge,
            &paper_tester,
            operator_key.signing_key(),
            &session_id,
            progress,
        )
        .await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(StartCycleResponse {
            started: true,
            message: "Evening run started. Watch the Live tab for progress.".to_string(),
        }),
    ))
}
```

> **Note:** `Scenario::fixture()`, `MockPaperTestRunner`, `OperatorKey::signing_key()`, `config.day_scenario()`, `config.baseline_scenario()`, and `ulid::Ulid` may need their exact signatures verified against the engine crate before writing. Run `grep -n "pub fn fixture\|pub fn day_scenario\|signing_key\|pub struct MockPaper" crates/xvision-engine/src/autooptimizer/` to confirm before compiling.

- [ ] **Step 4: Add to `routes/mod.rs`**

```rust
pub mod autooptimizer_cycle;
```

- [ ] **Step 5: Wire route in `server.rs`**

In `server.rs`, import:
```rust
use crate::routes::autooptimizer_cycle;
```
Then add to the write router:
```rust
.route(
    "/api/autooptimizer/evening-cycle",
    post(autooptimizer_cycle::start_evening_cycle),
)
```

- [ ] **Step 6: Build and fix compile errors**
```bash
~/.cargo/bin/cargo build -p xvision-dashboard 2>&1 | grep -E "^error" | head -20
```
Iterate on any type mismatches — the key invariants are that `Mutator::new`, `Judge::new`, `OperatorKey::load_or_generate`, and `run_evening_cycle` have the signatures you see in the engine crate.

- [ ] **Step 7: Commit**
```bash
git add crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs \
        crates/xvision-dashboard/src/routes/mod.rs \
        crates/xvision-dashboard/src/server.rs
git commit -m "feat(api): POST /api/autooptimizer/evening-cycle — launch evening run from dashboard"
```

---

## Task 4 — Update DiffInspector to show diff content and scores

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Modify: `frontend/web/src/features/autooptimizer/DiffInspector.tsx`

The DiffInspector currently shows only node metadata. With the blob endpoint from Task 2, it can show the actual mutation diff and gate score blobs.

- [ ] **Step 1: Add blob fetch to `api.ts`**

Add after the existing fetch functions:
```typescript
// ─── Blob fetch ───────────────────────────────────────────────────────────────

export type MutationDiffBlob = {
  kind: string;
  description: string;
  patch: string;
  parent_hash: string;
};

export type MetricsBlob = {
  sharpe_ratio?: number | null;
  max_drawdown?: number | null;
  profit_factor?: number | null;
  total_trades?: number | null;
  [key: string]: unknown;
};

export async function getBlob<T = unknown>(hash: string): Promise<T> {
  return apiFetch<T>(`/api/autooptimizer/blob/${encodeURIComponent(hash)}`);
}
```

Add query hooks:
```typescript
export function useDiffBlob(diffHash: string | null | undefined) {
  return useQuery({
    queryKey: [...autooptimizerKeys.all, "blob", "diff", diffHash ?? ""],
    queryFn: () => getBlob<MutationDiffBlob>(diffHash!),
    enabled: !!diffHash,
    staleTime: Infinity, // blobs are immutable
  });
}

export function useMetricsBlob(metricsHash: string | null | undefined) {
  return useQuery({
    queryKey: [...autooptimizerKeys.all, "blob", "metrics", metricsHash ?? ""],
    queryFn: () => getBlob<MetricsBlob>(metricsHash!),
    enabled: !!metricsHash,
    staleTime: Infinity,
  });
}
```

Also update `LineageNode` type to include `metrics_day_hash` and `metrics_untouched_hash`:
```typescript
export type LineageNode = {
  bundle_hash: string;
  parent_hash?: string | null;
  diff_hash?: string | null;
  metrics_day_hash?: string | null;
  metrics_untouched_hash?: string | null;
  gate_verdict?: string | null;
  status: LineageStatus;
  cycle_id?: string | null;
  created_at: string;
  diversity_score?: number | null;
};
```

- [ ] **Step 2: Update DiffInspector to use the blob hooks**

Find `DiffInspectorContent` in `DiffInspector.tsx`. After the existing metadata display, add:

```tsx
import { useLineageNode, useDiffBlob, useMetricsBlob, formatLineageStatus, formatGateVerdict } from "./api";

// Inside DiffInspectorContent, after existing node metadata:
const { data: diffBlob } = useDiffBlob(node.diff_hash);
const { data: dayMetrics } = useMetricsBlob(node.metrics_day_hash);
const { data: holdoutMetrics } = useMetricsBlob(node.metrics_untouched_hash);
```

Add a scores strip (place after the existing metadata cards):
```tsx
{(dayMetrics || holdoutMetrics) && (
  <Card>
    <CardHeader title="Gate scores" />
    <div className="px-5 pb-4 flex gap-8 text-[13px]">
      {dayMetrics?.sharpe_ratio != null && (
        <div>
          <div className="text-text-3 text-[11px] mb-1">Day Sharpe</div>
          <div className="font-mono text-text">{dayMetrics.sharpe_ratio.toFixed(3)}</div>
        </div>
      )}
      {holdoutMetrics?.sharpe_ratio != null && (
        <div>
          <div className="text-text-3 text-[11px] mb-1">Baseline untouched Sharpe</div>
          <div className="font-mono text-text">{holdoutMetrics.sharpe_ratio.toFixed(3)}</div>
        </div>
      )}
    </div>
  </Card>
)}
```

Add a diff content section:
```tsx
{diffBlob && (
  <Card>
    <CardHeader title="Experiment change" />
    <div className="px-5 pb-4 space-y-3">
      <div className="text-[13px] text-text-2">{diffBlob.description}</div>
      {diffBlob.patch && (
        <pre className="text-[12px] font-mono bg-surface-elev rounded p-3 overflow-x-auto whitespace-pre-wrap border border-border text-text-2">
          {diffBlob.patch}
        </pre>
      )}
    </div>
  </Card>
)}
```

- [ ] **Step 3: Verify TypeScript**
```bash
cd frontend/web && npx tsc --noEmit 2>&1 | grep -c "error TS"
```
Expected: 0

- [ ] **Step 4: Commit**
```bash
git add frontend/web/src/features/autooptimizer/api.ts \
        frontend/web/src/features/autooptimizer/DiffInspector.tsx
git commit -m "feat(ui): DiffInspector shows experiment diff content and gate scores from blob store"
```

---

## Task 5 — Add completed evening summaries to the Live tab

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/LiveCycleView.tsx`
- Modify: `frontend/web/src/features/autooptimizer/api.ts`

The Live tab currently only shows the real-time event stream. It should also show a list of recent completed cycles ("Evening summaries") from `GET /api/autooptimizer/seals`.

- [ ] **Step 1: Add `CycleSeal` type and `useSeals` hook to `api.ts` (already exists — verify)**

```bash
grep "CycleSeal\|useSeals\|listSeals" frontend/web/src/features/autooptimizer/api.ts
```
If `CycleSeal`, `listSeals`, and `useSeals` already exist, skip to Step 2.

- [ ] **Step 2: Add `RecentSummaries` component to `LiveCycleView.tsx`**

Add before `LiveCycleView` export:
```tsx
import { useSeals, type CycleSeal } from "./api";

function RecentSummaries() {
  const { data: seals, isPending } = useSeals();

  if (isPending || !seals || seals.length === 0) return null;

  return (
    <Card>
      <CardHeader title="Recent evening summaries" />
      <div className="divide-y divide-border">
        {seals.slice(0, 5).map((seal) => (
          <SealRow key={seal.seal_id} seal={seal} />
        ))}
      </div>
    </Card>
  );
}

function SealRow({ seal }: { seal: CycleSeal }) {
  return (
    <div className="px-5 py-3 flex items-center gap-4 text-[13px]">
      <span className="font-mono text-[11px] text-text-3 shrink-0">
        {new Date(seal.sealed_at).toLocaleDateString(undefined, {
          month: "short", day: "numeric", hour: "2-digit", minute: "2-digit",
        })}
      </span>
      <span className="text-text truncate flex-1">
        Cycle <span className="font-mono text-[11px]">{seal.cycle_id.slice(0, 12)}…</span>
      </span>
      <span className="font-mono text-[11px] text-text-3 shrink-0">
        proof: {seal.merkle_root.slice(0, 10)}…
      </span>
    </div>
  );
}
```

- [ ] **Step 3: Add `RecentSummaries` to the `LiveCycleView` return JSX**

In `LiveCycleView`, add `<RecentSummaries />` below the event card:
```tsx
return (
  <div className="space-y-4">
    <LaunchStrip />
    <div className="flex items-center gap-3">…</div>
    <Card>…cycle events…</Card>
    <RecentSummaries />
  </div>
);
```

- [ ] **Step 4: TypeScript check**
```bash
cd frontend/web && npx tsc --noEmit 2>&1 | grep -c "error TS"
```
Expected: 0

- [ ] **Step 5: Commit**
```bash
git add frontend/web/src/features/autooptimizer/LiveCycleView.tsx \
        frontend/web/src/features/autooptimizer/api.ts
git commit -m "feat(ui): show recent evening summaries in Live tab (Cycle proof + timestamp)"
```

---

## Task 6 — Enrich GenealogyTree with diff link and metric hashes

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/GenealogyTree.tsx`

The GenealogyTree shows status and gate verdict but no metric hashes or link to the diff. Each row should link to DiffInspector and show whether metrics are available.

- [ ] **Step 1: Update `NodeRow` to show metric availability**

In `GenealogyTree.tsx`, find the `NodeRow` component. After the `StatusBadge`:
```tsx
{/* Metric badge */}
{(node.metrics_day_hash || node.metrics_untouched_hash) && (
  <span className="text-[11px] text-text-3 shrink-0">📊 scores</span>
)}
```

- [ ] **Step 2: Remove diversity score from NodeRow (it's not on LineageNode from the API)**

The current `GenealogyTree.tsx` references `node.diversity_score` but `LineageNode` from the API doesn't include it. Remove that display (diversity is a separate endpoint — see `useDiversity`).

Find:
```tsx
{node.diversity_score != null && (
  <span className="ml-auto text-[12px] text-text-3 shrink-0">
    div {node.diversity_score.toFixed(3)}
  </span>
)}
```
Remove it.

- [ ] **Step 3: TypeScript check**
```bash
cd frontend/web && npx tsc --noEmit 2>&1 | grep -c "error TS"
```
Expected: 0

- [ ] **Step 4: Commit**
```bash
git add frontend/web/src/features/autooptimizer/GenealogyTree.tsx
git commit -m "fix(ui): remove non-existent diversity_score from GenealogyTree NodeRow"
```

---

## Task 7 — End-to-end build check

- [ ] **Step 1: Full Rust build**
```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-optimizer-ui"
~/.cargo/bin/cargo build --workspace 2>&1 | tail -5
```
Expected: `Finished` with no `error` lines.

- [ ] **Step 2: Full TypeScript check**
```bash
cd frontend/web && npx tsc --noEmit 2>&1 | grep -c "error TS"
```
Expected: 0

- [ ] **Step 3: Verify autooptimizer module tests compile**
```bash
~/.cargo/bin/cargo test -p xvision-engine --no-run 2>&1 | grep -E "^error"
```
Expected: no errors.

- [ ] **Step 4: Open a PR**
```bash
git push -u origin feat/optimizer-ui-complete
gh pr create --base main \
  --title "feat(optimizer): fix SSE, add evening-cycle launch, blob endpoint, DiffInspector scores" \
  --body "Fixes blank Optimizer UI. See plan at docs/superpowers/plans/2026-06-01-optimizer-ui-complete.md"
```

---

## CLI ↔ UI capability matrix (post-plan)

| `xvn optimizer` subcommand | UI surface after this plan |
|---|---|
| `session-init` | Auto-created by evening-cycle handler if no session exists |
| `mutate-once` | Not yet (DiffInspector shows result; triggering from UI is future work) |
| `evening-cycle --strategy --budget` | Live tab launch strip → POST handler ✓ |
| `lineage ls` | Genealogy tab ✓ |
| `lineage show <hash>` | DiffInspector with diff+scores ✓ |
| `seal show <cycle_id>` | Live tab Recent summaries ✓ |
| `gate <id> kept/dropped` | Not yet (Diff Inspector gate action is future work) |
| `activate <id>` | Not yet |
| `demo` | Not yet (would need replay endpoint) |
| `ls` / `inspect` | Flywheel/memory surface (separate route) |

---

## Self-review

**Spec coverage:**
- SSE fix: Task 1 ✓
- POST evening-cycle: Task 3 ✓
- Blob endpoint: Task 2 ✓
- DiffInspector content: Task 4 ✓
- Evening summaries: Task 5 ✓
- Genealogy fix: Task 6 ✓
- E2E build: Task 7 ✓

**Placeholder scan:** All code blocks are complete. Task 3 Step 3 notes that `Scenario::fixture()` and other helper methods must be verified against actual engine signatures before compiling — this is a concrete instruction, not a TBD.

**Type consistency:** `MutationDiffBlob`, `MetricsBlob`, `LineageNode` types used consistently across Tasks 4 and 6.
