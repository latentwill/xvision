# AutoOptimizer Current Status Completion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the non-cryptographic AutoOptimizer surface so `xvn optimizer evening-cycle` and `/autooptimizer` compile, run real paper-test evaluation, stream live progress, and give operators useful genealogy, diff, ladder, and provenance views.

**Architecture:** The 2026-05-27 spine is now historical for cryptographic provenance. The active implementation keeps content-addressed lineage and numeric evaluation, but removes `CycleSeal`, Merkle roots, session commitments, and operator signing per `docs/superpowers/specs/2026-06-01-remove-autooptimizer-crypto-provenance-design.md`. This plan repairs the current `origin/main` drift, then enriches the existing React/axum autooptimizer surface in small, independently testable pieces.

**Tech Stack:** Rust, axum, sqlx, xvision-engine `autooptimizer`, Vite, React, TanStack Query, Vitest, Testing Library.

**Base branch:** `origin/main` at or after merge `5d4f0b12` (`feat/optimizer-ui-complete`). If work starts from a stale local branch, first fetch and create an isolated worktree from `origin/main`.

**Non-goals:** Do not reintroduce `CycleSeal`, Merkle root computation, Ed25519 operator keys, `session_commitments`, `cycle_seals`, or any operator-facing cryptographic ceremony.

---

## Current Status Snapshot

The implementation is partially present but not complete:

| Area | Current state | Completion target |
|---|---|---|
| Crypto provenance | Superseded and removed by 2026-06-01 design | Keep removed; update references only |
| CLI/dashboard cycle contract | `run_evening_cycle` signature drift leaves CLI/dashboard call sites stale | `cargo check -p xvision-cli -p xvision-dashboard` passes |
| Dashboard launch | Uses `StubPaperTester` with fixed metrics | Uses `CachedBacktestPaperTester` for real backtest paper tests |
| SSE | Backend emits `{ kind, display_label, data }`; frontend reads it as `CycleProgressEvent` | Frontend normalizes the envelope and renders timestamp/cycle/kind correctly |
| Diff inspector | Placeholder text despite `GET /api/autooptimizer/blob/:hash` existing | Loads child and parent bundle blobs and renders a readable before/after JSON diff |
| Genealogy | Flat grouped list with parent arrows | Nested tree with cycle grouping, root/child structure, status and diversity scanability |
| Provenance | Synthetic grouping by model because lineage response lacks attribution | Backend joins `mutator_attribution`; frontend groups by real provider/model/prompt |
| Tests | No focused frontend tests for `/autooptimizer`; no focused route tests for autooptimizer API | Route tests, reducer/parser tests, and component tests cover the operator surface |

## File Map

**Modify:**
- `crates/xvision-cli/src/commands/autooptimizer.rs` - repair `run_evening_cycle` call and removed session argument.
- `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs` - repair signature drift and use real paper-test adapter.
- `crates/xvision-dashboard/src/routes/autooptimizer.rs` - return lineage DTOs with attribution and support blob reads.
- `crates/xvision-dashboard/src/server.rs` - verify autooptimizer routes remain registered in read/mutate routers.
- `frontend/web/src/features/autooptimizer/api.ts` - add SSE envelope normalization, blob API, and attribution fields.
- `frontend/web/src/features/autooptimizer/LiveCycleView.tsx` - parse envelopes and render richer live state.
- `frontend/web/src/features/autooptimizer/DiffInspector.tsx` - load blobs and render diff content.
- `frontend/web/src/features/autooptimizer/GenealogyTree.tsx` - replace flat rows with tree rendering.
- `frontend/web/src/features/autooptimizer/LadderWithProvenance.tsx` - use real attribution fields.
- `frontend/web/src/features/autooptimizer/AutoOptimizerLayout.tsx` - add compact status summary and consistent labels if needed.
- `frontend/web/src/features/autooptimizer/preferences.ts` - keep model persistence; no required change unless tests expose a bug.

**Create:**
- `crates/xvision-dashboard/tests/autooptimizer_routes.rs` - route-level coverage for lineage, blob, and launch validation.
- `frontend/web/src/features/autooptimizer/api.test.ts` - parser and label helper tests.
- `frontend/web/src/features/autooptimizer/LiveCycleView.test.tsx` - live feed tests.
- `frontend/web/src/features/autooptimizer/DiffInspector.test.tsx` - blob diff tests.
- `frontend/web/src/features/autooptimizer/GenealogyTree.test.tsx` - tree rendering tests.
- `frontend/web/src/features/autooptimizer/LadderWithProvenance.test.tsx` - attribution grouping tests.

---

## 100x Work Packet Index

Assign one packet per 100x CLI worker. Every packet is independently reviewable and should land as one commit.

| Packet | Worker focus | Depends on |
|---|---|---|
| A | Contract repair: CLI + dashboard compile | none |
| B | Dashboard route tests and launch validation | A |
| C | SSE envelope normalization and live feed UI | A |
| D | Blob-backed diff inspector | A |
| E | Genealogy tree rendering | A |
| F | Real provenance attribution | A |
| G | Real paper-test adapter for dashboard launch | A, B |
| H | Final UX polish, docs, and full verification | B, C, D, E, F, G |

Recommended dispatch command shape for each worker:

```bash
100x run --repo /path/to/xvision --base origin/main --task "Packet A from docs/superpowers/plans/2026-06-02-autooptimizer-current-status-completion.md"
```

If the local `100x` wrapper uses a different argument format, pass the packet name and this plan path verbatim.

---

## Packet A - Repair CLI and Dashboard Contract Drift

**Goal:** Remove stale crypto/session call-site assumptions so the workspace compiles after the provenance removal.

**Files:**
- Modify: `crates/xvision-cli/src/commands/autooptimizer.rs`
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`

- [ ] **Step 1: Confirm the current engine signature**

Run:

```bash
rg -n "pub async fn run_evening_cycle" crates/xvision-engine/src/autooptimizer/cycle.rs
sed -n '68,82p' crates/xvision-engine/src/autooptimizer/cycle.rs
```

Expected signature shape:

```rust
pub async fn run_evening_cycle(
    pool: &SqlitePool,
    _blob_store: &BlobStore,
    config: &AutoOptimizerConfig,
    cycle_config: &CycleConfig,
    parent_policy: &ParentPolicy,
    mutator: &Mutator,
    judge: &Judge,
    paper_tester: &dyn PaperTestRunner,
    progress: impl Fn(CycleProgressEvent) + Send + Sync,
    dspy_ctx: Option<&DspyContext>,
) -> Result<CycleResult>
```

- [ ] **Step 2: Update the CLI call site**

In `crates/xvision-cli/src/commands/autooptimizer.rs`, remove the stale `args.session_id.clone()` argument from `run_evening_cycle(...)`.

The call should end like this:

```rust
let result = run_evening_cycle(
    &pool,
    &obs_blob_store,
    &cfg,
    &cycle_config,
    &parent_policy,
    &mutator,
    &judge,
    paper_tester.as_ref(),
    |event| {
        if let Ok(line) = serde_json::to_string(&event) {
            println!("{}", line);
        }
    },
    None,
)
.await
.map_err(|e| CliError::upstream(anyhow::anyhow!("run_evening_cycle: {e}")))?;
```

- [ ] **Step 3: Remove the CLI `session_id` flag if it is still declared**

Search:

```bash
rg -n "session_id|session-id" crates/xvision-cli/src/commands/autooptimizer.rs
```

If `EveningCycleArgs` still has a field like this:

```rust
#[arg(long)]
pub session_id: Option<String>,
```

delete it. The flag belongs to the removed cryptographic session-commitment layer.

- [ ] **Step 4: Update the dashboard imports**

In `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`, remove any import of:

```rust
session::{default_key_path, load_or_generate_key},
```

and delete local variables:

```rust
let key_path = default_key_path()?;
let operator_key = load_or_generate_key(&key_path)?;
let session_id = Ulid::new().to_string();
```

Keep `Ulid` only if `build_day_scenario` still uses it for scenario IDs.

- [ ] **Step 5: Update the dashboard `run_evening_cycle` call**

Replace the call tail in `start_evening_cycle` with:

```rust
let result = run_evening_cycle(
    &pool,
    &obs_blob_store,
    &cfg,
    &cycle_config,
    &ParentPolicy::RoundRobin,
    &mutator,
    &judge,
    paper_tester.as_ref(),
    move |ev| {
        let _ = tx.send(ev);
    },
    None,
)
.await;
```

- [ ] **Step 6: Verify compile**

Run:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-autooptimizer-completion"
scripts/cargo check -p xvision-cli -p xvision-dashboard
```

Expected: command exits 0. If there are warnings, they must not be from `autooptimizer_cycle.rs` or `autooptimizer.rs`.

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-cli/src/commands/autooptimizer.rs \
        crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs
git commit -m "fix(autooptimizer): align cycle call sites with provenance removal"
```

---

## Packet B - Add Dashboard AutoOptimizer Route Tests

**Goal:** Create route tests that pin the active non-crypto API contract before frontend work consumes it.

**Files:**
- Create: `crates/xvision-dashboard/tests/autooptimizer_routes.rs`

- [ ] **Step 1: Create the test file**

Create `crates/xvision-dashboard/tests/autooptimizer_routes.rs` with this structure:

```rust
#![allow(clippy::unwrap_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use serde_json::json;
use tempfile::TempDir;

use xvision_dashboard::server::build_router;
use xvision_dashboard::AppState;
use xvision_engine::autooptimizer::{
    blob_store::BlobStore,
    content_hash::ContentHash,
    gate::GateVerdict,
    lineage::{LineageNode, LineageStatus, LineageStore},
};

async fn boot() -> (TestServer, TempDir, AppState) {
    let tmp = TempDir::new().unwrap();
    let state = AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init dashboard state");
    let server = TestServer::new(build_router(state.clone())).unwrap();
    (server, tmp, state)
}

async fn seed_lineage(state: &AppState) -> (ContentHash, ContentHash) {
    let blob_store = BlobStore::new(state.xvn_home.join("lineage").join("blobs"));
    let parent_json = json!({
        "manifest": {
            "id": "parent-strategy",
            "name": "Parent",
            "asset_universe": ["BTC/USD"]
        },
        "risk": { "max_position_pct": 10 }
    });
    let child_json = json!({
        "manifest": {
            "id": "child-strategy",
            "name": "Child",
            "asset_universe": ["BTC/USD"]
        },
        "risk": { "max_position_pct": 6 }
    });
    let parent_hash = blob_store.put_json(&parent_json).await.unwrap();
    let child_hash = blob_store.put_json(&child_json).await.unwrap();
    let store = LineageStore::new(state.pool.clone());
    store
        .insert(&LineageNode {
            bundle_hash: parent_hash,
            parent_hash: None,
            gate_verdict: GateVerdict::Pass,
            status: LineageStatus::Active,
            cycle_id: Some("cycle-a".to_string()),
            created_at: chrono::Utc::now(),
            diversity_score: Some(0.12),
        })
        .await
        .unwrap();
    store
        .insert(&LineageNode {
            bundle_hash: child_hash,
            parent_hash: Some(parent_hash),
            gate_verdict: GateVerdict::Pass,
            status: LineageStatus::Active,
            cycle_id: Some("cycle-a".to_string()),
            created_at: chrono::Utc::now(),
            diversity_score: Some(0.44),
        })
        .await
        .unwrap();
    (parent_hash, child_hash)
}

#[tokio::test]
async fn lineage_and_blob_routes_return_seeded_data() {
    let (server, _tmp, state) = boot().await;
    let (_parent_hash, child_hash) = seed_lineage(&state).await;

    let lineage = server.get("/api/autooptimizer/lineage").await;
    lineage.assert_status_ok();
    let body: serde_json::Value = lineage.json();
    assert_eq!(body.as_array().unwrap().len(), 2);

    let detail = server
        .get(&format!("/api/autooptimizer/lineage/{}", child_hash.to_hex()))
        .await;
    detail.assert_status_ok();
    let detail_body: serde_json::Value = detail.json();
    assert_eq!(detail_body["bundle_hash"], child_hash.to_hex());
    assert_eq!(detail_body["cycle_id"], "cycle-a");

    let blob = server
        .get(&format!("/api/autooptimizer/blob/{}", child_hash.to_hex()))
        .await;
    blob.assert_status_ok();
    let blob_body: serde_json::Value = blob.json();
    assert_eq!(blob_body["manifest"]["id"], "child-strategy");
}

#[tokio::test]
async fn evening_cycle_launch_requires_strategy_id() {
    let (server, _tmp, _state) = boot().await;
    let resp = server
        .post("/api/autooptimizer/evening-cycle")
        .json(&json!({ "mutator_model": "dummy", "judge_model": "dummy" }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    let body: serde_json::Value = resp.json();
    assert_eq!(body["field"], "strategy_id");
}
```

- [ ] **Step 2: Run the new route tests**

Run:

```bash
scripts/cargo test -p xvision-dashboard --test autooptimizer_routes
```

Expected: tests pass after Packet A. If `DashboardError::Validation` serializes under a different key than `field`, adjust the assertion to match the existing error JSON shape used by other route tests.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-dashboard/tests/autooptimizer_routes.rs
git commit -m "test(dashboard): cover autooptimizer lineage blob and launch routes"
```

---

## Packet C - Normalize SSE Envelope and Fix Live Feed Rendering

**Goal:** Make the live feed consume the backend envelope `{ kind, display_label, data }` and render real time, event, and cycle values.

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Modify: `frontend/web/src/features/autooptimizer/LiveCycleView.tsx`
- Create: `frontend/web/src/features/autooptimizer/api.test.ts`
- Create: `frontend/web/src/features/autooptimizer/LiveCycleView.test.tsx`

- [ ] **Step 1: Add envelope types and parser tests**

Create `frontend/web/src/features/autooptimizer/api.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { normalizeCycleProgressEnvelope, formatEventLabel } from "./api";

describe("normalizeCycleProgressEnvelope", () => {
  it("unwraps backend SSE envelope", () => {
    const event = normalizeCycleProgressEnvelope({
      kind: "cycle_started",
      display_label: "Evening run started",
      data: {
        cycle_id: "cycle-1",
        started_at: "2026-06-02T00:00:00Z",
        parent_count: 2,
      },
    });

    expect(event).toEqual({
      event_type: "cycle_started",
      display_label: "Evening run started",
      cycle_id: "cycle-1",
      ts: "2026-06-02T00:00:00Z",
      payload: {
        cycle_id: "cycle-1",
        started_at: "2026-06-02T00:00:00Z",
        parent_count: 2,
      },
    });
  });

  it("keeps direct event payloads for backward compatibility", () => {
    const event = normalizeCycleProgressEnvelope({
      event_type: "mutation_accepted",
      display_label: "Experiment accepted",
      cycle_id: "cycle-2",
      ts: "2026-06-02T01:00:00Z",
    });
    expect(event?.event_type).toBe("mutation_accepted");
    expect(formatEventLabel(event!)).toBe("Experiment accepted");
  });

  it("turns lagged notices into a visible event", () => {
    const event = normalizeCycleProgressEnvelope({ dropped: 7 });
    expect(event?.event_type).toBe("lagged");
    expect(event?.display_label).toBe("Stream lagged; 7 events were dropped");
  });
});
```

- [ ] **Step 2: Implement the parser**

In `frontend/web/src/features/autooptimizer/api.ts`, add:

```ts
export type CycleProgressEnvelope =
  | {
      kind: string;
      display_label?: string | null;
      data?: Record<string, unknown> | null;
    }
  | { dropped: number }
  | CycleProgressEvent;

export function normalizeCycleProgressEnvelope(raw: unknown): CycleProgressEvent | null {
  if (!raw || typeof raw !== "object") return null;
  const value = raw as Record<string, unknown>;

  if (typeof value.dropped === "number") {
    return {
      event_type: "lagged",
      display_label: `Stream lagged; ${value.dropped} events were dropped`,
      ts: new Date().toISOString(),
      payload: { dropped: value.dropped },
    };
  }

  if (typeof value.event_type === "string") {
    return value as CycleProgressEvent;
  }

  if (typeof value.kind !== "string") return null;
  const data = value.data && typeof value.data === "object"
    ? (value.data as Record<string, unknown>)
    : {};
  const cycleId =
    typeof data.cycle_id === "string"
      ? data.cycle_id
      : typeof data.cycleId === "string"
        ? data.cycleId
        : null;
  const ts =
    typeof data.ts === "string"
      ? data.ts
      : typeof data.started_at === "string"
        ? data.started_at
        : typeof data.finished_at === "string"
          ? data.finished_at
          : new Date().toISOString();

  return {
    event_type: value.kind,
    display_label:
      typeof value.display_label === "string" ? value.display_label : null,
    cycle_id: cycleId,
    bundle_hash: typeof data.bundle_hash === "string" ? data.bundle_hash : null,
    ts,
    payload: data,
  };
}
```

- [ ] **Step 3: Use the parser in `LiveCycleView`**

In `frontend/web/src/features/autooptimizer/LiveCycleView.tsx`, change the import:

```ts
import {
  type CycleProgressEvent,
  formatEventLabel,
  normalizeCycleProgressEnvelope,
} from "./api";
```

Replace the JSON parse block in the `message` listener with:

```ts
source.addEventListener("message", (ev) => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(ev.data as string);
  } catch {
    return;
  }
  const normalized = normalizeCycleProgressEnvelope(parsed);
  if (!normalized) return;
  setEvents((prev) => {
    const row: EventRow = { ...normalized, _row_id: nextRowId++ };
    const next = prev.length >= 200 ? prev.slice(1) : prev;
    return [...next, row];
  });
});
```

- [ ] **Step 4: Add a focused LiveCycleView test**

Create `frontend/web/src/features/autooptimizer/LiveCycleView.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LiveCycleView } from "./LiveCycleView";

class FakeEventSource {
  static instances: FakeEventSource[] = [];
  listeners = new Map<string, Array<(event: MessageEvent) => void>>();
  constructor(public url: string) {
    FakeEventSource.instances.push(this);
  }
  addEventListener(type: string, cb: (event: MessageEvent) => void) {
    const arr = this.listeners.get(type) ?? [];
    arr.push(cb);
    this.listeners.set(type, arr);
  }
  close() {}
  emit(type: string, data: unknown) {
    for (const cb of this.listeners.get(type) ?? []) {
      cb({ data: JSON.stringify(data) } as MessageEvent);
    }
  }
}

function renderView() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <LiveCycleView />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  FakeEventSource.instances = [];
  vi.restoreAllMocks();
});

describe("LiveCycleView", () => {
  it("renders backend SSE envelopes", async () => {
    vi.stubGlobal("EventSource", FakeEventSource);
    renderView();
    const source = FakeEventSource.instances[0];
    source.emit("open", {});
    source.emit("message", {
      kind: "cycle_started",
      display_label: "Evening run started",
      data: {
        cycle_id: "cycle-1",
        started_at: "2026-06-02T00:00:00Z",
      },
    });
    await waitFor(() => {
      expect(screen.getByText("Evening run started")).toBeInTheDocument();
    });
    expect(screen.getByText("cycle-1")).toBeInTheDocument();
  });
});
```

- [ ] **Step 5: Run tests**

```bash
cd frontend/web
pnpm exec vitest run src/features/autooptimizer/api.test.ts src/features/autooptimizer/LiveCycleView.test.tsx
pnpm exec tsc --noEmit
```

Expected: both test files pass and `tsc` exits 0.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/features/autooptimizer/api.ts \
        frontend/web/src/features/autooptimizer/api.test.ts \
        frontend/web/src/features/autooptimizer/LiveCycleView.tsx \
        frontend/web/src/features/autooptimizer/LiveCycleView.test.tsx
git commit -m "fix(web): normalize autooptimizer SSE envelopes"
```

---

## Packet D - Implement Blob-Backed Diff Inspector

**Goal:** Replace the diff placeholder with useful parent/child bundle inspection using the existing blob route.

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Modify: `frontend/web/src/features/autooptimizer/DiffInspector.tsx`
- Create: `frontend/web/src/features/autooptimizer/DiffInspector.test.tsx`

- [ ] **Step 1: Add blob API helpers**

In `frontend/web/src/features/autooptimizer/api.ts`, add:

```ts
export type BundleBlob = Record<string, unknown>;

export async function getBlob(hash: string): Promise<BundleBlob> {
  return apiFetch<BundleBlob>(`/api/autooptimizer/blob/${encodeURIComponent(hash)}`);
}
```

Extend `autooptimizerKeys`:

```ts
blob: (hash: string) => [...autooptimizerKeys.all, "blob", hash] as const,
```

Add the hook:

```ts
export function useBlob(hash?: string | null) {
  return useQuery({
    queryKey: autooptimizerKeys.blob(hash ?? ""),
    queryFn: () => getBlob(hash ?? ""),
    enabled: !!hash,
    staleTime: 60_000,
  });
}
```

- [ ] **Step 2: Add diff test**

Create `frontend/web/src/features/autooptimizer/DiffInspector.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, cleanup } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DiffInspector } from "./DiffInspector";
import { getBlob, getLineageNode } from "./api";

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    getLineageNode: vi.fn(),
    getBlob: vi.fn(),
  };
});

function renderDiff() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={["/autooptimizer/diff/childhash"]}>
        <Routes>
          <Route path="/autooptimizer/diff/:hash" element={<DiffInspector />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DiffInspector", () => {
  it("renders parent and child bundle blob differences", async () => {
    vi.mocked(getLineageNode).mockResolvedValue({
      bundle_hash: "childhash",
      parent_hash: "parenthash",
      gate_verdict: "passed",
      status: "active",
      cycle_id: "cycle-1",
      created_at: "2026-06-02T00:00:00Z",
      diversity_score: 0.44,
    });
    vi.mocked(getBlob).mockImplementation(async (hash: string) => {
      if (hash === "parenthash") {
        return { manifest: { id: "parent" }, risk: { max_position_pct: 10 } };
      }
      return { manifest: { id: "child" }, risk: { max_position_pct: 6 } };
    });

    renderDiff();

    expect(await screen.findByText("Experiment diff")).toBeInTheDocument();
    expect(await screen.findByText(/risk.max_position_pct/i)).toBeInTheDocument();
    expect(screen.getByText("10")).toBeInTheDocument();
    expect(screen.getByText("6")).toBeInTheDocument();
  });
});
```

- [ ] **Step 3: Implement simple changed-path diff helpers**

In `DiffInspector.tsx`, add:

```tsx
function changedPaths(before: unknown, after: unknown, prefix = ""): string[] {
  if (JSON.stringify(before) === JSON.stringify(after)) return [];
  if (!isRecord(before) || !isRecord(after)) return [prefix || "value"];
  const keys = new Set([...Object.keys(before), ...Object.keys(after)]);
  return [...keys].flatMap((key) =>
    changedPaths(before[key], after[key], prefix ? `${prefix}.${key}` : key),
  );
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function valueAtPath(value: unknown, path: string): unknown {
  return path.split(".").reduce<unknown>((acc, key) => {
    if (!isRecord(acc)) return undefined;
    return acc[key];
  }, value);
}
```

- [ ] **Step 4: Replace placeholder with blob-backed content**

Inside `DiffInspectorContent`, call:

```tsx
const childBlob = useBlob(node?.bundle_hash);
const parentBlob = useBlob(node?.parent_hash);
```

Replace the placeholder card body with a component that:

```tsx
const paths =
  parentBlob.data && childBlob.data
    ? changedPaths(parentBlob.data, childBlob.data).slice(0, 30)
    : [];
```

Render:

```tsx
{!node.parent_hash ? (
  <div className="rounded border border-border bg-surface-elev/40 px-4 py-6 text-[13px] text-text-3">
    Root experiment. No parent bundle is available for comparison.
  </div>
) : parentBlob.isPending || childBlob.isPending ? (
  <div className="text-[13px] text-text-3">Loading bundle blobs...</div>
) : parentBlob.isError || childBlob.isError ? (
  <div className="text-[13px] text-danger">Could not load one of the bundle blobs.</div>
) : (
  <div className="overflow-x-auto">
    <table className="w-full text-[13px] border-collapse">
      <thead>
        <tr className="border-b border-border">
          <th className="text-left text-text-3 font-medium px-3 py-2">Field</th>
          <th className="text-left text-text-3 font-medium px-3 py-2">Parent</th>
          <th className="text-left text-text-3 font-medium px-3 py-2">Experiment</th>
        </tr>
      </thead>
      <tbody>
        {paths.map((path) => (
          <tr key={path} className="border-b border-border last:border-0">
            <td className="px-3 py-2 font-mono text-[12px] text-text">{path}</td>
            <td className="px-3 py-2 font-mono text-[12px] text-text-3">
              {formatDiffValue(valueAtPath(parentBlob.data, path))}
            </td>
            <td className="px-3 py-2 font-mono text-[12px] text-text">
              {formatDiffValue(valueAtPath(childBlob.data, path))}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  </div>
)}
```

Add:

```tsx
function formatDiffValue(value: unknown): string {
  if (value === undefined) return "missing";
  if (value === null) return "null";
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}
```

- [ ] **Step 5: Run tests**

```bash
cd frontend/web
pnpm exec vitest run src/features/autooptimizer/DiffInspector.test.tsx
pnpm exec tsc --noEmit
```

Expected: test passes and `tsc` exits 0.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/features/autooptimizer/api.ts \
        frontend/web/src/features/autooptimizer/DiffInspector.tsx \
        frontend/web/src/features/autooptimizer/DiffInspector.test.tsx
git commit -m "feat(web): render autooptimizer bundle diffs from blobs"
```

---

## Packet E - Replace Flat Genealogy List With a Real Tree

**Goal:** Make the genealogy view visually useful by rendering parent-child structure rather than a flat cycle list.

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/GenealogyTree.tsx`
- Create: `frontend/web/src/features/autooptimizer/GenealogyTree.test.tsx`

- [ ] **Step 1: Add tree rendering test**

Create `frontend/web/src/features/autooptimizer/GenealogyTree.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, cleanup } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";
import { GenealogyTree } from "./GenealogyTree";
import { listLineageNodes } from "./api";

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    listLineageNodes: vi.fn(),
  };
});

function renderTree() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <GenealogyTree />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("GenealogyTree", () => {
  it("renders nested root and child experiments", async () => {
    vi.mocked(listLineageNodes).mockResolvedValue([
      {
        bundle_hash: "rootaaaaaaaa",
        parent_hash: null,
        status: "active",
        gate_verdict: "passed",
        cycle_id: "cycle-1",
        created_at: "2026-06-02T00:00:00Z",
        diversity_score: 0.1,
      },
      {
        bundle_hash: "childbbbbbbbb",
        parent_hash: "rootaaaaaaaa",
        status: "rejected",
        gate_verdict: "rejected:overfit",
        cycle_id: "cycle-1",
        created_at: "2026-06-02T01:00:00Z",
        diversity_score: 0.4,
      },
    ]);

    renderTree();

    expect(await screen.findByText("rootaaaa")).toBeInTheDocument();
    expect(screen.getByText("childbbb")).toBeInTheDocument();
    expect(screen.getByText("Rejected")).toBeInTheDocument();
    expect(screen.getByText(/1 child/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Add tree builder helpers**

In `GenealogyTree.tsx`, add:

```tsx
type TreeNode = LineageNode & { children: TreeNode[] };

function buildTree(nodes: LineageNode[]): TreeNode[] {
  const byHash = new Map<string, TreeNode>();
  for (const node of nodes) {
    byHash.set(node.bundle_hash, { ...node, children: [] });
  }
  const roots: TreeNode[] = [];
  for (const node of byHash.values()) {
    if (node.parent_hash && byHash.has(node.parent_hash)) {
      byHash.get(node.parent_hash)!.children.push(node);
    } else {
      roots.push(node);
    }
  }
  const sortRec = (items: TreeNode[]) => {
    items.sort(
      (a, b) =>
        new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
    );
    for (const item of items) sortRec(item.children);
  };
  sortRec(roots);
  return roots;
}
```

- [ ] **Step 3: Render recursive tree rows**

Replace the flat `items.map((node) => <NodeRow ... />)` area with:

```tsx
{buildTree(items).map((node) => (
  <TreeRow key={node.bundle_hash} node={node} depth={0} />
))}
```

Add:

```tsx
function TreeRow({ node, depth }: { node: TreeNode; depth: number }) {
  return (
    <div>
      <NodeRow node={node} depth={depth} childCount={node.children.length} />
      {node.children.length > 0 && (
        <div className="ml-5 border-l border-border pl-3">
          {node.children.map((child) => (
            <TreeRow key={child.bundle_hash} node={child} depth={depth + 1} />
          ))}
        </div>
      )}
    </div>
  );
}
```

Change `NodeRow` signature:

```tsx
function NodeRow({
  node,
  depth,
  childCount,
}: {
  node: LineageNode;
  depth: number;
  childCount: number;
}) {
```

Inside `NodeRow`, render:

```tsx
<span className="text-text-3 text-[11px] font-mono shrink-0">
  {depth === 0 ? "root" : `L${depth}`}
</span>
...
<span className="text-[12px] text-text-3 shrink-0">
  {childCount} {childCount === 1 ? "child" : "children"}
</span>
```

- [ ] **Step 4: Run tests**

```bash
cd frontend/web
pnpm exec vitest run src/features/autooptimizer/GenealogyTree.test.tsx
pnpm exec tsc --noEmit
```

Expected: test passes and `tsc` exits 0.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/GenealogyTree.tsx \
        frontend/web/src/features/autooptimizer/GenealogyTree.test.tsx
git commit -m "feat(web): render autooptimizer genealogy as a tree"
```

---

## Packet F - Add Real Attribution to Provenance

**Goal:** Stop the provenance view from assigning lineage nodes to models by round-robin. Join lineage nodes to `mutator_attribution` and group by real provider/model/prompt version.

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer.rs`
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Modify: `frontend/web/src/features/autooptimizer/LadderWithProvenance.tsx`
- Create: `frontend/web/src/features/autooptimizer/LadderWithProvenance.test.tsx`

- [ ] **Step 1: Extend the dashboard lineage DTO**

In `crates/xvision-dashboard/src/routes/autooptimizer.rs`, create a route DTO instead of returning engine `LineageNode` directly:

```rust
#[derive(Serialize)]
pub struct LineageNodeDto {
    pub bundle_hash: String,
    pub parent_hash: Option<String>,
    pub gate_verdict: String,
    pub status: String,
    pub cycle_id: Option<String>,
    pub created_at: String,
    pub diversity_score: Option<f64>,
    pub writer_provider: Option<String>,
    pub writer_model: Option<String>,
    pub writer_prompt_version: Option<String>,
}
```

- [ ] **Step 2: Update lineage SQL to join attribution**

For list queries, select:

```sql
SELECT
    ln.bundle_hash,
    ln.parent_hash,
    ln.gate_verdict,
    ln.status,
    ln.cycle_id,
    ln.created_at,
    ln.diversity_score,
    ma.provider AS writer_provider,
    ma.model AS writer_model,
    ma.prompt_version AS writer_prompt_version
FROM lineage_nodes ln
LEFT JOIN mutator_attribution ma ON ma.bundle_hash = ln.bundle_hash
```

Keep the existing `WHERE`, `ORDER BY`, `LIMIT`, and `OFFSET` clauses. For `get_lineage_node`, use the same join with `WHERE ln.bundle_hash = ?`.

- [ ] **Step 3: Map rows to DTOs**

Add a mapper:

```rust
fn row_to_lineage_node_dto(
    row: sqlx::sqlite::SqliteRow,
) -> Result<LineageNodeDto, DashboardError> {
    use sqlx::Row;
    Ok(LineageNodeDto {
        bundle_hash: row.try_get("bundle_hash").map_err(|e| DashboardError::Internal(e.into()))?,
        parent_hash: row.try_get("parent_hash").map_err(|e| DashboardError::Internal(e.into()))?,
        gate_verdict: row.try_get("gate_verdict").map_err(|e| DashboardError::Internal(e.into()))?,
        status: row.try_get("status").map_err(|e| DashboardError::Internal(e.into()))?,
        cycle_id: row.try_get("cycle_id").map_err(|e| DashboardError::Internal(e.into()))?,
        created_at: row.try_get("created_at").map_err(|e| DashboardError::Internal(e.into()))?,
        diversity_score: row.try_get("diversity_score").map_err(|e| DashboardError::Internal(e.into()))?,
        writer_provider: row.try_get("writer_provider").map_err(|e| DashboardError::Internal(e.into()))?,
        writer_model: row.try_get("writer_model").map_err(|e| DashboardError::Internal(e.into()))?,
        writer_prompt_version: row.try_get("writer_prompt_version").map_err(|e| DashboardError::Internal(e.into()))?,
    })
}
```

- [ ] **Step 4: Update frontend type**

In `frontend/web/src/features/autooptimizer/api.ts`, extend `LineageNode`:

```ts
writer_provider?: string | null;
writer_model?: string | null;
writer_prompt_version?: string | null;
```

- [ ] **Step 5: Replace heuristic grouping**

In `LadderWithProvenance.tsx`, replace `groupNodesByModel(nodes, sorted)` with:

```tsx
const byModel = groupNodesByAttribution(nodes);
```

Replace the old grouping helper with:

```tsx
function groupNodesByAttribution(nodes: LineageNode[]): ModelGroup[] {
  const sortedNodes = [...nodes].sort(
    (a, b) =>
      new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );
  const groups = new Map<string, ModelGroup>();
  for (const node of sortedNodes) {
    const provider = node.writer_provider ?? "unknown";
    const model = node.writer_model ?? "unknown";
    const prompt = node.writer_prompt_version ?? "unknown";
    const key = `${provider}/${model}/${prompt}`;
    const group =
      groups.get(key) ??
      {
        key,
        model,
        provider,
        nodes: [],
      };
    group.nodes.push(node);
    groups.set(key, group);
  }
  return [...groups.values()];
}
```

- [ ] **Step 6: Add frontend grouping test**

Create `frontend/web/src/features/autooptimizer/LadderWithProvenance.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, cleanup } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { LadderWithProvenance } from "./LadderWithProvenance";
import { getLadder, listLineageNodes } from "./api";

vi.mock("./api", async () => {
  const actual = await vi.importActual<typeof import("./api")>("./api");
  return {
    ...actual,
    getLadder: vi.fn(),
    listLineageNodes: vi.fn(),
  };
});

function renderView() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <LadderWithProvenance />
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("LadderWithProvenance", () => {
  it("groups recent experiments by real attribution", async () => {
    vi.mocked(getLadder).mockResolvedValue([
      {
        provider: "anthropic",
        model: "claude-haiku-4-5",
        prompt_version: "v1",
        proposals: 2,
        accepted: 1,
        rejected_overfit: 1,
        avg_delta_sharpe: 0.2,
      },
    ]);
    vi.mocked(listLineageNodes).mockResolvedValue([
      {
        bundle_hash: "hashaaa111",
        parent_hash: null,
        status: "active",
        gate_verdict: "passed",
        cycle_id: "cycle-1",
        created_at: "2026-06-02T00:00:00Z",
        diversity_score: 0.2,
        writer_provider: "anthropic",
        writer_model: "claude-haiku-4-5",
        writer_prompt_version: "v1",
      },
    ]);

    renderView();

    expect(await screen.findAllByText("claude-haiku-4-5")).not.toHaveLength(0);
    expect(screen.getByText(/anthropic/i)).toBeInTheDocument();
    expect(screen.getByText("hashaaa1")).toBeInTheDocument();
  });
});
```

- [ ] **Step 7: Run tests**

```bash
scripts/cargo test -p xvision-dashboard --test autooptimizer_routes
cd frontend/web
pnpm exec vitest run src/features/autooptimizer/LadderWithProvenance.test.tsx
pnpm exec tsc --noEmit
```

Expected: tests pass and `tsc` exits 0.

- [ ] **Step 8: Commit**

```bash
git add crates/xvision-dashboard/src/routes/autooptimizer.rs \
        frontend/web/src/features/autooptimizer/api.ts \
        frontend/web/src/features/autooptimizer/LadderWithProvenance.tsx \
        frontend/web/src/features/autooptimizer/LadderWithProvenance.test.tsx
git commit -m "feat(autooptimizer): expose real writer attribution"
```

---

## Packet G - Replace Dashboard Stub Evaluation With Cached Backtest

**Goal:** Make dashboard-launched evening runs use the same real paper-test adapter as the CLI non-mock path.

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`

- [ ] **Step 1: Import production adapter and tools**

In `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`, replace:

```rust
eval_adapter::StubPaperTester,
```

with:

```rust
eval_adapter::CachedBacktestPaperTester,
```

Add:

```rust
use xvision_engine::tools::ToolRegistry;
```

Remove `use xvision_engine::eval::run::MetricsSummary;` if it is only used by `stub_paper_tester`.

- [ ] **Step 2: Build the paper tester from dashboard state**

Before `tokio::spawn`, create:

```rust
let api_ctx = state.api_context();
let tools = Arc::new(ToolRegistry::default_with_builtins());
```

Inside the spawned task, replace:

```rust
let paper_tester = Arc::new(stub_paper_tester());
```

with:

```rust
let paper_tester = CachedBacktestPaperTester::new(
    api_ctx,
    Arc::clone(&mutator.dispatch),
    Arc::clone(&tools),
);
```

Then pass:

```rust
&paper_tester,
```

to `run_evening_cycle`.

- [ ] **Step 3: Delete the stub helper**

Delete:

```rust
fn stub_paper_tester() -> StubPaperTester { ... }
```

The dashboard route must not ship fixed Sharpe, return, drawdown, win-rate, or trade-count metrics.

- [ ] **Step 4: Keep route tests deterministic**

Do not add a route test that runs the full evening cycle against real market data. Route tests should verify validation and bootstrapping only. The real evaluation path is covered by:

```bash
scripts/cargo test -p xvision-engine --test autooptimizer_eval_adapter
scripts/cargo test -p xvision-engine --test autooptimizer_cycle
```

- [ ] **Step 5: Verify**

Run:

```bash
scripts/cargo check -p xvision-dashboard
scripts/cargo test -p xvision-engine --test autooptimizer_eval_adapter
scripts/cargo test -p xvision-engine --test autooptimizer_cycle
```

Expected: all commands exit 0.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs
git commit -m "fix(dashboard): run autooptimizer cycles through cached backtests"
```

---

## Packet H - Final UX Polish and Verification

**Goal:** Make `/autooptimizer` feel like a usable operator surface rather than a set of thin debug tables.

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/AutoOptimizerLayout.tsx`
- Modify: `frontend/web/src/features/autooptimizer/LiveCycleView.tsx`
- Modify: `frontend/web/src/features/autooptimizer/GenealogyTree.tsx`
- Modify: `frontend/web/src/features/autooptimizer/DiffInspector.tsx`
- Modify: `frontend/web/src/features/autooptimizer/LadderWithProvenance.tsx`
- Modify: `MANUAL.md`

- [ ] **Step 1: Add a compact summary row**

In `AutoOptimizerLayout.tsx`, add a summary band above tabs with three cells:

```tsx
<div className="grid gap-3 sm:grid-cols-3">
  <div className="rounded border border-border bg-surface-card px-4 py-3">
    <div className="text-[11px] uppercase text-text-3">Live status</div>
    <div className="text-[15px] font-medium text-text">Evening run monitor</div>
  </div>
  <div className="rounded border border-border bg-surface-card px-4 py-3">
    <div className="text-[11px] uppercase text-text-3">Review path</div>
    <div className="text-[15px] font-medium text-text">Diff before accepting changes</div>
  </div>
  <div className="rounded border border-border bg-surface-card px-4 py-3">
    <div className="text-[11px] uppercase text-text-3">Evidence</div>
    <div className="text-[15px] font-medium text-text">Lineage, ladder, provenance</div>
  </div>
</div>
```

Keep it compact. Do not add marketing copy or a landing-page hero.

- [ ] **Step 2: Replace weak empty states**

Use concrete empty-state text:

| File | Empty state |
|---|---|
| `LiveCycleView.tsx` | `No evening run events yet. Start an evening run or run xvn optimizer evening-cycle from another terminal.` |
| `GenealogyTree.tsx` | `No lineage experiments yet. Launch an evening run with a strategy ID to seed the first parent.` |
| `DiffInspector.tsx` | `Select an experiment from Genealogy to inspect bundle differences.` |
| `LadderWithProvenance.tsx` | `No experiment-writer attribution yet. The ladder fills after proposals are recorded.` |

- [ ] **Step 3: Make status labels consistent**

Search frontend autooptimizer files:

```bash
rg -n "Mutator|CycleSeal|Merkle|session commitment|Quarantined|Ghost|proposer" frontend/web/src/features/autooptimizer
```

Expected: no operator-visible references remain. Use:

| Technical term | Operator label |
|---|---|
| `Mutator` | `Experiment writer` |
| `Rejected` | `Rejected` |
| `Lineage` | `Genealogy` where the user is navigating; `lineage` can remain in API names |
| `Cycle` | `Evening run` where visible to operators |

- [ ] **Step 4: Add MANUAL section**

Append a concise section to `MANUAL.md`:

```markdown
### Optimizer dashboard

The Optimizer page (`/autooptimizer`) is the operator view for evening runs.
Use the Live tab to start a run from a strategy ID and watch progress events.
Use Genealogy to inspect accepted and rejected experiments. Select an experiment
to open Diff and compare the parent bundle against the experiment bundle.
The Ladder and Provenance tabs show which experiment-writer model produced
useful changes over recent runs.

The optimizer is eval-only. It does not create or require cryptographic
operator signatures, session commitments, Merkle roots, or cycle seals.
```

- [ ] **Step 5: Run full verification**

Run:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-autooptimizer-completion"
scripts/cargo check -p xvision-cli -p xvision-dashboard
scripts/cargo test -p xvision-dashboard --test autooptimizer_routes
scripts/cargo test -p xvision-engine --test autooptimizer_cycle
scripts/cargo test -p xvision-engine --test autooptimizer_eval_adapter
cd frontend/web
pnpm exec vitest run src/features/autooptimizer
pnpm exec tsc --noEmit
pnpm exec vite build
```

Expected:
- Rust commands exit 0.
- Vitest exits 0.
- TypeScript exits 0.
- Vite build exits 0.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/features/autooptimizer \
        MANUAL.md
git commit -m "polish(web): complete autooptimizer operator surface"
```

---

## Final Acceptance Gate

The plan is complete when all of these are true:

1. `scripts/cargo check -p xvision-cli -p xvision-dashboard` exits 0.
2. `scripts/cargo test -p xvision-dashboard --test autooptimizer_routes` exits 0.
3. `scripts/cargo test -p xvision-engine --test autooptimizer_cycle` exits 0.
4. `scripts/cargo test -p xvision-engine --test autooptimizer_eval_adapter` exits 0.
5. `cd frontend/web && pnpm exec vitest run src/features/autooptimizer` exits 0.
6. `cd frontend/web && pnpm exec tsc --noEmit` exits 0.
7. `/autooptimizer` contains no visible placeholder saying a follow-up PR is required.
8. `/autooptimizer` live feed renders backend SSE envelopes with correct labels, cycle id, and time.
9. Diff inspector loads parent and child bundle blobs and shows changed fields.
10. Provenance groups experiments by real writer attribution, not synthetic ordering.
11. `rg -n "CycleSeal|Merkle|session commitment|operator signature|Ed25519" frontend/web/src/features/autooptimizer MANUAL.md` returns no operator-surface references except the MANUAL sentence explaining those things are not required.

## Review Notes For Conductors

- Review Packet A before dispatching Packet G. If compile is broken, UI packets can still proceed against types, but final verification will fail.
- Packets C, D, E, and F can run in parallel after Packet A.
- Packet H must run last because it touches the same frontend files as the feature packets.
- Do not accept a PR that restores `seal.rs`, `session.rs`, `cycle_seals`, or `session_commitments`.
- Do not accept dashboard launch code that uses fixed `MetricsSummary` values outside tests.
