# Optimizer Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rebuild the optimizer frontend as an editorial mission-control surface per the approved spec `docs/superpowers/specs/2026-06-11-optimizer-redesign-design.md`: five screens fold into three, a stacked ConsoleModule (phase ribbon → experiment board → narrated feed) replaces LiveCycleView with last-cycle replay instead of waiting states, and a lineage-river signature chart replaces the improvement chart.

**Architecture:** Four small, additive backend changes unlock everything (spec §8, amended after plan-review-gate escalation: enrich progress events with child_hash/writer/delta; persist cycle events at the existing broadcast point; an events-by-cycle read endpoint for replay; a river read endpoint joining lineage nodes with gate scores). All frontend logic lands as pure, unit-tested selectors (`narrateEvent`, `buildBoardState`, `buildHeadline`, `buildRiverLayout`) consumed by thin components. Screens are assembled last, then old screens/routes are deleted with redirects.

**Tech Stack:** Rust (axum, sqlx/SQLite) for the two endpoints; React + TypeScript + TanStack Query + Tailwind tokens + hand-rolled SVG (river) on the frontend; vitest + @testing-library/react; cargo test for backend.

**Execution environment (hard rules from CLAUDE.md):**
- Work in an isolated worktree, never the main checkout:
  ```bash
  git worktree add .worktrees/optimizer-redesign -b feat/optimizer-redesign
  cd .worktrees/optimizer-redesign
  export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
  ```
- Run cargo through the disk-guard wrapper: `scripts/cargo test -p xvision-dashboard`.
- Frontend tests: `cd frontend/web && npm run test -- <pattern>`.
- Terminology: operator-facing strings say **Experiment / Experiment writer / Rejected / Suspect / honesty check**; code identifiers keep the `autooptimizer` codename. Never touch DSPy `Optimizer*` tokens.
- No popups/modals/popovers; no white borders in dark mode (theme tokens only).

---

## File structure

**Backend:**
- Modify: `crates/xvision-engine/src/autooptimizer/progress.rs` + `cycle.rs` — additive event fields (Task 0a).
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs` — persist events at the broadcast point (Task 0b) + `get_cycle_events` handler (Task 1).
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer.rs` — add `get_river` handler (Task 2) + `table_exists` visibility.
- Modify: `crates/xvision-dashboard/src/server.rs` — register both routes.

**Frontend selectors & primitives (`frontend/web/src/features/autooptimizer/`):**
- Create: `selectors/narrateEvent.ts` + test — SSE/persisted event → human sentence.
- Create: `selectors/buildBoardState.ts` + test — events → per-experiment card states + active phase.
- Create: `selectors/buildHeadline.ts` + test — status + last cycle → editorial sentence parts.
- Create: `selectors/buildRiverLayout.ts` + test — river nodes → positioned lines/fans/stubs.
- Create: `ui/EditorialHeadline.tsx` + test.
- Create: `ui/ExpandableArtifact.tsx` + test.
- Create: `ui/PhaseRibbon.tsx` + test.
- Create: `ui/ExperimentBoard.tsx` + test.
- Create: `ui/NarratedFeed.tsx` + test.
- Create: `ui/ConsoleModule.tsx` + test — composition + live/replay/never-ran modes.
- Create: `ui/LineageRiver.tsx` + test — SVG chart + inline readout card.
- Modify: `api.ts` + `api.test.ts` — `useCycleEvents(cycleId)`, `useRiver()`, types.

**Screens & routes:**
- Rewrite: `screens/OptimizerHome.tsx` + test.
- Rewrite: `screens/CycleDetail.tsx` + test.
- Modify: `frontend/web/src/routes.tsx` — redirects; delete two routes.
- Delete: `LiveCycleView.tsx`, `screens/ExperimentDetail.tsx`, `screens/RunDetail.tsx` (+ their tests); fold their still-needed panels (GateScorecard, FindingsList, ParentDiffPanel, RegimeCards) into the ExpandableArtifact/CycleDetail.

---

### Task 0a: Backend — enrich progress events (spec §8.3)

The real `CycleProgressEvent` enum (`crates/xvision-engine/src/autooptimizer/progress.rs`, serde `tag = "type"`, fields flattened at top level — there is NO `payload` envelope) is too thin for the console: `MutationProposed { session_id, cycle_id, parent_hash }` carries no experiment hash or writer; `MutationGated { session_id, cycle_id, child_hash, passed, outcome }` carries no ΔSharpe. Add additive `#[serde(default)]` fields so old consumers (CLI IPC, bus.js, existing SSE clients) are unaffected.

**Files:**
- Modify: `crates/xvision-engine/src/autooptimizer/progress.rs` — `MutationProposed` gains `child_hash: String` + `mutator_model: String` (both `#[serde(default)]`); `MutationGated` gains `delta_day: Option<f64>` (`#[serde(default)]`).
- Modify: the emit sites in `crates/xvision-engine/src/autooptimizer/cycle.rs` — at propose time the mutation's blob hash and the configured mutator model are in scope; at gate time the gate record's `delta_day` is in scope. Populate the new fields there.

- [ ] **Step 1: Write failing serde round-trip tests** in `progress.rs`'s test mod: a `MutationProposed` with `child_hash`/`mutator_model` serializes with those keys at top level under `"type":"mutation_proposed"`; deserializing OLD json (without the new keys) still succeeds via `#[serde(default)]`. Same for `MutationGated.delta_day`.
- [ ] **Step 2: Run** `scripts/cargo test -p xvision-engine progress` → FAIL.
- [ ] **Step 3: Add the fields + populate the emit sites in cycle.rs** (find them with `rg "MutationProposed|MutationGated" crates/xvision-engine/src/autooptimizer/cycle.rs`). The `display_label`/`event_kind` matches in `crates/xvision-dashboard/src/sse/autooptimizer_labels.rs` use `{ .. }` patterns — no variant change, so they compile untouched (their exhaustiveness test still passes).
- [ ] **Step 4: Run** `scripts/cargo test -p xvision-engine progress && scripts/cargo test -p xvision-dashboard autooptimizer_labels` → PASS.
- [ ] **Step 5: Commit** `git commit -m "feat(engine): enrich optimizer progress events — child_hash, mutator_model, delta_day"`

---

### Task 0b: Backend — persist cycle events (spec §8.4)

Today nothing writes cycle events to `autooptimizer_events` (the only production writer is `scheduler.rs:99` logging `schedule_skipped`); the dashboard's `start_cycle` callback only broadcasts: `move |ev| { let _ = tx.send(ev); }` (`routes/autooptimizer_cycle.rs` ~line 363). Without persistence the replay mode has no data — this task is the foundation of "idle = replay".

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`

- [ ] **Step 1: Write the failing test:** unit-test a new `persist_progress_event(pool, &ev)` helper — given a pool with the events table (reuse Task 1's `create_events_table` helper; define it in this task since 0b lands first) and a `CycleProgressEvent::MutationGated { .. }`, it inserts one row with `kind = "mutation_gated_passed"` (via the existing `event_kind()` from `crate::sse::autooptimizer_labels`), `cycle_id` set, and `payload_json` = the full serde JSON of the event.
- [ ] **Step 2: Run** `scripts/cargo test -p xvision-dashboard persist_progress_event` → FAIL.
- [ ] **Step 3: Implement:**

```rust
use crate::sse::autooptimizer_labels::event_kind;
use xvision_engine::autooptimizer::{events_store, progress::CycleProgressEvent};

pub(crate) async fn persist_progress_event(pool: &sqlx::SqlitePool, ev: &CycleProgressEvent) {
    let (session_id, cycle_id) = event_ids(ev); // small match extracting the two id fields per variant
    let payload = serde_json::to_string(ev).unwrap_or_else(|_| "{}".into());
    // best-effort: persistence must never fail the cycle
    let _ = events_store::append_event(pool, &session_id, cycle_id.as_deref(), event_kind(ev), &payload).await;
}
```

(Match `append_event`'s real signature in `events_store.rs:17` — adapt argument order/types to it.) Wire it in `start_cycle`: change the progress callback to also forward into an `mpsc::unbounded_channel`, and spawn a persister task that drains the receiver calling `persist_progress_event` (the callback is sync, so it cannot await; the channel hop keeps the cycle loop non-blocking):

```rust
let (persist_tx, mut persist_rx) = tokio::sync::mpsc::unbounded_channel::<CycleProgressEvent>();
let persist_pool = pool.clone();
tokio::spawn(async move {
    while let Some(ev) = persist_rx.recv().await {
        persist_progress_event(&persist_pool, &ev).await;
    }
});
// in the run_cycle callback:
move |ev| {
    let _ = persist_tx.send(ev.clone());
    let _ = tx.send(ev);
},
```

- [ ] **Step 4: Run** `scripts/cargo test -p xvision-dashboard persist_progress_event` → PASS. Also `scripts/cargo test -p xvision-dashboard` for collateral.
- [ ] **Step 5: Commit** `git commit -m "feat(dashboard): persist optimizer cycle events at the broadcast point"`

> **Replay coverage note (recorded):** cycles run before this task shipped have no event log; the ConsoleModule's node-derived fallback (Task 10) covers them.

---

### Task 1: Backend — `GET /api/autooptimizer/cycles/:cycle_id/events` (replay)

With Task 0b persisting cycle events into `autooptimizer_events` (seq, session_id, cycle_id, kind, payload_json, ts — migration 057), add the read endpoint so the frontend can replay the last completed cycle. Note `prune_old_events` keeps only the most recent 50 sessions — acceptable for "last cycle" replay; older cycles use the node-derived fallback (Task 10).

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`
- Modify: `crates/xvision-dashboard/src/server.rs` (near the existing `/api/autooptimizer/cycles/:cycle_id/cost` registration, ~line 369)

- [ ] **Step 0: Make `table_exists` reachable.** The only `table_exists` is a private `async fn` in `routes/autooptimizer.rs:1436`; `autooptimizer_cycle.rs` cannot see it. Change its visibility to `pub(super)` and import it in `autooptimizer_cycle.rs` (`use super::autooptimizer::table_exists;`). No behavior change; existing callers unaffected.

- [ ] **Step 1: Write the failing test** in the `#[cfg(test)] mod tests` block of `autooptimizer_cycle.rs` (reuse the existing `open_pool()` helper at ~line 1325). There is no shared table-setup function for `autooptimizer_events` (the production table comes from migration `057_autooptimizer_sessions.sql`; `events_store.rs`'s own tests use inline DDL in a private `open_test_pool()`), so create the table in the test with DDL copied verbatim from migration 057:

```rust
async fn create_events_table(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS autooptimizer_events (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            cycle_id TEXT,
            kind TEXT NOT NULL,
            payload_json TEXT NOT NULL,
            ts TEXT NOT NULL
        )", // mirror crates/xvision-engine/migrations/057 exactly — verify before committing
    )
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_get_cycle_events_returns_ordered_events() {
    let pool = open_pool().await;
    create_events_table(&pool).await;
    for (kind, cycle) in [
        ("cycle_started", "cyc-1"),
        ("mutation_proposed", "cyc-1"),
        ("mutation_gated", "cyc-1"),
        ("cycle_started", "cyc-2"), // other cycle — must be filtered out
        ("cycle_finished", "cyc-1"),
    ] {
        sqlx::query(
            "INSERT INTO autooptimizer_events (session_id, cycle_id, kind, payload_json, ts)
             VALUES ('sess-1', ?1, ?2, '{}', '2026-06-11T00:00:00Z')",
        )
        .bind(cycle)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    }
    let state = test_state(pool); // follow the existing test AppState constructor in this mod
    let resp = get_cycle_events(Path("cyc-1".into()), State(state)).await.unwrap();
    let events = resp.0;
    assert_eq!(events.len(), 4);
    assert_eq!(events[0].kind, "cycle_started");
    assert_eq!(events[3].kind, "cycle_finished");
    assert!(events.windows(2).all(|w| w[0].seq < w[1].seq));
}

#[tokio::test]
async fn test_get_cycle_events_missing_table_returns_empty() {
    let pool = open_pool().await; // no ensure_tables
    let state = test_state(pool);
    let resp = get_cycle_events(Path("cyc-x".into()), State(state)).await.unwrap();
    assert!(resp.0.is_empty());
}
```

If the test mod lacks a `test_state` helper, mirror how sibling handler tests construct `AppState` from a pool.

- [ ] **Step 2: Run to verify failure**

Run: `scripts/cargo test -p xvision-dashboard test_get_cycle_events`
Expected: FAIL — `get_cycle_events` not found.

- [ ] **Step 3: Implement the handler** in `autooptimizer_cycle.rs`, following the `get_cycle_cost_handler` pattern (graceful when table absent):

```rust
#[derive(Serialize, sqlx::FromRow)]
pub struct PersistedCycleEvent {
    pub seq: i64,
    pub session_id: String,
    pub cycle_id: Option<String>,
    pub kind: String,
    pub payload_json: String,
    pub ts: String,
}

// GET /api/autooptimizer/cycles/:cycle_id/events
//
// Replay source for the ConsoleModule's idle state: the persisted event log
// of a completed cycle, oldest-first. Absent table (fresh install) → empty
// list, never an error — "no events yet" is a designed product state.
pub async fn get_cycle_events(
    Path(cycle_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<PersistedCycleEvent>>, DashboardError> {
    if !table_exists(&state.pool, "autooptimizer_events").await? {
        return Ok(Json(Vec::new()));
    }
    let events: Vec<PersistedCycleEvent> = sqlx::query_as(
        "SELECT seq, session_id, cycle_id, kind, payload_json, ts
         FROM autooptimizer_events WHERE cycle_id = ?1 ORDER BY seq ASC LIMIT 1000",
    )
    .bind(&cycle_id)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(Json(events))
}
```

- [ ] **Step 4: Register the route** in `server.rs` next to the cycle cost route:

```rust
.route(
    "/api/autooptimizer/cycles/:cycle_id/events",
    get(routes::autooptimizer_cycle::get_cycle_events),
)
```

- [ ] **Step 5: Run tests**

Run: `scripts/cargo test -p xvision-dashboard test_get_cycle_events`
Expected: PASS (both tests).

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs crates/xvision-dashboard/src/server.rs
git commit -m "feat(dashboard): GET /api/autooptimizer/cycles/:id/events for console replay"
```

---

### Task 2: Backend — `GET /api/autooptimizer/river` (lineage + scores)

The river needs, per lineage node, the Sharpe level (`child_day_score`) and `delta_day` — which live in `autooptimizer_gate_records`, not on `lineage_nodes` (the existing `/lineage` endpoint selects no scores; per-hash experiment detail would be an N+1 for a whole-history chart). Add a dedicated read-only LEFT JOIN endpoint; do not modify `LineageStore` or the existing `/lineage` response shape.

> **Recorded spec deviation:** spec §7 allows "a possible events-by-cycle read endpoint" (Task 1). This second read endpoint is required data-plumbing for the §3a river (Y = Sharpe per node) and adds no new computation, but it exceeds §7's literal single-endpoint allowance. Surface it explicitly in the PR description for operator sign-off.

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`

- [ ] **Step 1: Write the failing test** in `autooptimizer.rs`'s test mod (mirror its existing pool/table setup; create minimal `lineage_nodes` + `autooptimizer_gate_records` rows):

```rust
#[tokio::test]
async fn test_get_river_joins_scores_and_orders_by_created_at() {
    let pool = open_test_pool_with_lineage_tables().await; // reuse/extract this mod's table-setup helper
    sqlx::query(
        "INSERT INTO lineage_nodes (bundle_hash, parent_hash, cycle_id, status, gate_verdict, created_at)
         VALUES ('hash-a', NULL, 'cyc-1', 'active', 'Pass', '2026-06-10T00:00:00Z'),
                ('hash-b', 'hash-a', 'cyc-2', 'rejected', '{\"Fail\":{\"reason\":\"overfit\"}}', '2026-06-11T00:00:00Z')",
    ).execute(&pool).await.unwrap();
    sqlx::query(
        "INSERT INTO autooptimizer_gate_records (bundle_hash, child_day_score, delta_day, verdict, created_at)
         VALUES ('hash-b', 1.52, 0.21, 'Fail', '2026-06-11T00:00:00Z')",
    ).execute(&pool).await.unwrap();
    let state = test_state(pool);
    let resp = get_river(State(state)).await.unwrap();
    let nodes = resp.0;
    assert_eq!(nodes.len(), 2);
    assert_eq!(nodes[0].bundle_hash, "hash-a");
    assert_eq!(nodes[0].child_day_score, None);
    assert_eq!(nodes[1].child_day_score, Some(1.52));
    assert_eq!(nodes[1].delta_day, Some(0.21));
    assert_eq!(nodes[1].parent_hash.as_deref(), Some("hash-a"));
}
```

Adapt INSERT column lists to the real DDL in migrations 048/058 if they differ (check `crates/*/migrations/`); the assertion set is the contract.

- [ ] **Step 2: Run to verify failure**

Run: `scripts/cargo test -p xvision-dashboard test_get_river`
Expected: FAIL — `get_river` not found.

- [ ] **Step 3: Implement**

```rust
#[derive(Serialize, sqlx::FromRow)]
pub struct RiverNode {
    pub bundle_hash: String,
    pub parent_hash: Option<String>,
    pub cycle_id: Option<String>,
    pub status: String,
    pub created_at: String,
    pub child_day_score: Option<f64>,
    pub delta_day: Option<f64>,
}

// GET /api/autooptimizer/river
//
// Feed for the lineage-river chart: every lineage node with its gate scores
// joined in, oldest-first so the frontend can build generations in order.
pub async fn get_river(
    State(state): State<AppState>,
) -> Result<Json<Vec<RiverNode>>, DashboardError> {
    if !table_exists(&state.pool, "lineage_nodes").await? {
        return Ok(Json(Vec::new()));
    }
    let has_gates = table_exists(&state.pool, "autooptimizer_gate_records").await?;
    let sql = if has_gates {
        "SELECT n.bundle_hash, n.parent_hash, n.cycle_id, n.status, n.created_at,
                g.child_day_score, g.delta_day
         FROM lineage_nodes n
         LEFT JOIN autooptimizer_gate_records g ON g.bundle_hash = n.bundle_hash
         ORDER BY n.created_at ASC LIMIT 2000"
    } else {
        "SELECT bundle_hash, parent_hash, cycle_id, status, created_at,
                NULL AS child_day_score, NULL AS delta_day
         FROM lineage_nodes ORDER BY created_at ASC LIMIT 2000"
    };
    let nodes: Vec<RiverNode> = sqlx::query_as(sql)
        .fetch_all(&state.pool)
        .await
        .map_err(|e| DashboardError::Internal(e.into()))?;
    Ok(Json(nodes))
}
```

- [ ] **Step 4: Register** in `server.rs` near the `/api/autooptimizer/lineage` route: `.route("/api/autooptimizer/river", get(routes::autooptimizer::get_river))`

- [ ] **Step 5: Run tests** — `scripts/cargo test -p xvision-dashboard test_get_river` → PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-dashboard/src/routes/autooptimizer.rs crates/xvision-dashboard/src/server.rs
git commit -m "feat(dashboard): GET /api/autooptimizer/river — lineage nodes joined with gate scores"
```

---

### Task 3: Frontend API hooks — `useCycleEvents`, `useRiver`

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Test: `frontend/web/src/features/autooptimizer/api.test.ts`

- [ ] **Step 1: Failing tests** (follow this file's existing fetch-mock pattern):

```typescript
it("useCycleEvents fetches persisted events for a cycle", async () => {
  mockFetchOnce([{ seq: 1, session_id: "s", cycle_id: "cyc-1", kind: "cycle_started", payload_json: "{}", ts: "2026-06-11T00:00:00Z" }]);
  const { result } = renderHookWithClient(() => useCycleEvents("cyc-1"));
  await waitFor(() => expect(result.current.data).toHaveLength(1));
  expect(lastFetchUrl()).toContain("/api/autooptimizer/cycles/cyc-1/events");
});

it("useCycleEvents is disabled without a cycle id", () => {
  const { result } = renderHookWithClient(() => useCycleEvents(null));
  expect(result.current.fetchStatus).toBe("idle");
});

it("useRiver fetches river nodes", async () => {
  mockFetchOnce([{ bundle_hash: "h", parent_hash: null, cycle_id: "c", status: "active", created_at: "t", child_day_score: 1.2, delta_day: 0.1 }]);
  const { result } = renderHookWithClient(() => useRiver());
  await waitFor(() => expect(result.current.data?.[0].bundle_hash).toBe("h"));
});

it("useCycleEvents and useRiver surface isError without retry on older backends (404)", async () => {
  mockFetchOnce({ status: 404 }); // adapt to the file's mock idiom for non-OK responses
  const { result } = renderHookWithClient(() => useRiver());
  await waitFor(() => expect(result.current.isError).toBe(true));
});
```

(`mockFetchOnce` / `renderHookWithClient` / `lastFetchUrl`: reuse whatever helpers `api.test.ts` already defines for the other hooks — match the local idiom exactly.)

- [ ] **Step 2: Run** `cd frontend/web && npm run test -- api.test` → new tests FAIL.

- [ ] **Step 3: Implement in `api.ts`:**

```typescript
export type PersistedCycleEvent = {
  seq: number;
  session_id: string;
  cycle_id: string | null;
  kind: string;
  payload_json: string;
  ts: string;
};

export type RiverNode = {
  bundle_hash: string;
  parent_hash: string | null;
  cycle_id: string | null;
  status: LineageStatus | string;
  created_at: string;
  child_day_score: number | null;
  delta_day: number | null;
};

export function useCycleEvents(cycleId: string | null) {
  return useQuery<PersistedCycleEvent[]>({
    queryKey: ["optimizer/cycle-events", cycleId],
    queryFn: () => fetchJson(`/api/autooptimizer/cycles/${cycleId}/events`),
    enabled: !!cycleId,
    staleTime: 60_000,
    retry: false, // endpoint may not exist on older backends
  });
}

export function useRiver(opts?: { refetchIntervalWhileRunning?: boolean }) {
  return useQuery<RiverNode[]>({
    queryKey: ["optimizer/river"],
    queryFn: () => fetchJson(`/api/autooptimizer/river`),
    staleTime: 30_000,
    refetchInterval: opts?.refetchIntervalWhileRunning ? 15_000 : false,
    retry: false, // endpoint may not exist on older backends — consumers render their empty states
  });
}
```

(Use the file's existing fetch helper — whatever `useCycleRuns` uses — instead of `fetchJson` if it is named differently.)

- [ ] **Step 4: Run** `npm run test -- api.test` → PASS.
- [ ] **Step 5: Commit** — `git add frontend/web/src/features/autooptimizer/api.ts frontend/web/src/features/autooptimizer/api.test.ts && git commit -m "feat(frontend): useCycleEvents + useRiver hooks"`

---

### Task 4: `narrateEvent` selector

Pure function: one event (live or persisted) → `{ sentence, tone, hash }`. **Wire-shape ground truth** (`crates/xvision-engine/src/autooptimizer/progress.rs`): events are serde-tagged `{"type":"mutation_gated", ...fields flattened at top level}` — there is NO `payload` envelope. Key fields: `child_hash`, `parent_hash`, `outcome` (`"kept"|"suspect"|"dropped"`), `passed`, `delta_day` (Task 0a), `mutator_model` (Task 0a), `reason` (no_candidate), `passed`/`message` (honesty_check_run), `severity`/`code` (judge_finding), `active_count`/`suspect_count`/`rejected_count` (cycle_finished), plus `phase_started`/`phase_finished`/`session_state_changed`/`flywheel_compiled`. Persisted rows use `event_kind()` kinds where gating is three-way: `mutation_gated_passed` / `mutation_gated_suspect` / `mutation_gated_dropped` — normalize these to the `type` discriminant. Also verify `SSE_EVENT_NAMES` in `hooks/useCycleEventStream.ts` subscribes to the frame names `event_kind()` actually emits (the three `mutation_gated_*` names) — add them if missing. The hook converts `lagged` frames to dropped-count markers, so no `lagged` narration is needed (the unknown-kind fallback covers any stragglers).

**Files:**
- Create: `frontend/web/src/features/autooptimizer/selectors/narrateEvent.ts`
- Test: `frontend/web/src/features/autooptimizer/selectors/narrateEvent.test.ts`

- [ ] **Step 1: Failing tests** (one per kind + unknown + persisted-shape normalization):

```typescript
import { describe, expect, it } from "vitest";
import { narrateEvent, normalizePersisted } from "./narrateEvent";

// Fixtures mirror the REAL wire shapes (progress.rs): flattened fields, "type" tag.
it("narrates mutation_proposed with writer and hash", () => {
  const n = narrateEvent({
    type: "mutation_proposed", cycle_id: "c1",
    parent_hash: "ffff0000aa", child_hash: "abcd1234ef", mutator_model: "gemini-2.5-pro",
  });
  expect(n.sentence).toBe("Writer gemini-2.5-pro proposed an experiment → abcd1234");
  expect(n.tone).toBe("neutral");
  expect(n.hash).toBe("abcd1234ef");
});

it("narrates the three gate outcomes with delta", () => {
  const kept = narrateEvent({ type: "mutation_gated", child_hash: "abcd1234ef", passed: true, outcome: "kept", delta_day: 0.21 });
  expect(kept.sentence).toBe("Gate passed abcd1234 · ΔSharpe +0.21 — kept");
  expect(kept.tone).toBe("kept");
  const dropped = narrateEvent({ type: "mutation_gated", child_hash: "abcd1234ef", passed: false, outcome: "dropped", delta_day: -0.08 });
  expect(dropped.sentence).toBe("Gate failed abcd1234 · ΔSharpe −0.08 — rejected");
  expect(dropped.tone).toBe("rejected");
  const suspect = narrateEvent({ type: "mutation_gated", child_hash: "abcd1234ef", passed: false, outcome: "suspect" });
  expect(suspect.sentence).toBe("Gate flagged abcd1234 — suspect");
  expect(suspect.tone).toBe("suspect");
});

it("narrates honesty_check_run with its message", () => {
  const n = narrateEvent({ type: "honesty_check_run", passed: true, message: "sabotage caught" });
  expect(n.sentence).toBe("Honesty check passed — sabotage caught");
  expect(n.tone).toBe("kept");
  expect(narrateEvent({ type: "honesty_check_run", passed: false, message: "" }).tone).toBe("suspect");
});

it("narrates the remaining kinds", () => {
  expect(narrateEvent({ type: "cycle_started", cycle_id: "cyc-7f3a", parent_count: 3 }).sentence)
    .toBe("Cycle cyc-7f3a started · 3 parents");
  expect(narrateEvent({ type: "cycle_finished", active_count: 2, suspect_count: 1, rejected_count: 11 }).sentence)
    .toBe("Cycle finished — 2 kept · 1 suspect · 11 rejected");
  expect(narrateEvent({ type: "parent_selected", parent_hash: "abcd1234ef" }).sentence)
    .toBe("Parent selected: abcd1234");
  expect(narrateEvent({ type: "no_candidate", parent_hash: "abcd1234ef", reason: "identity diff" }).tone).toBe("warn");
  expect(narrateEvent({ type: "judge_finding", child_hash: "abcd1234ef", severity: "warn", code: "lookahead" }).tone)
    .toBe("warn");
  expect(narrateEvent({ type: "phase_started", phase: "eval", detail: "backtesting" }).tone).toBe("neutral");
});

it("falls back gracefully on unknown kinds", () => {
  const n = narrateEvent({ type: "future_event" });
  expect(n.sentence).toBe("future_event");
  expect(n.tone).toBe("neutral");
});

it("normalizePersisted parses payload_json (the full serialized event) and maps 3-way gated kinds", () => {
  const e = normalizePersisted({
    seq: 1, session_id: "s", cycle_id: "c", ts: "t",
    kind: "mutation_gated_passed",
    payload_json: '{"type":"mutation_gated","child_hash":"abcd1234ef","passed":true,"outcome":"kept","delta_day":0.21}',
  });
  expect(e.type).toBe("mutation_gated");
  expect(e.child_hash).toBe("abcd1234ef");
  expect(e.ts).toBe("t"); // row ts wins so the feed has times
});
```

- [ ] **Step 2: Run** `npm run test -- narrateEvent` → FAIL (module missing).

- [ ] **Step 3: Implement:**

```typescript
import type { CycleProgressEvent, PersistedCycleEvent } from "../api";

export type NarrationTone = "kept" | "rejected" | "suspect" | "warn" | "neutral";
export type Narration = { sentence: string; tone: NarrationTone; hash: string | null };

const short = (h?: string | null) => (h ? h.slice(0, 8) : "");
const fmtDelta = (d: unknown) =>
  typeof d === "number" ? `${d >= 0 ? "+" : "−"}${Math.abs(d).toFixed(2)}` : null;

// Persisted rows store the full serialized event in payload_json (flattened,
// "type"-tagged). Spread it back to top level; map the 3-way persisted kinds
// (mutation_gated_passed/_suspect/_dropped) to the serde discriminant; the
// row's ts wins so the feed has stable times.
export function normalizePersisted(e: PersistedCycleEvent): CycleProgressEvent {
  let parsed: Record<string, unknown> = {};
  try { parsed = JSON.parse(e.payload_json) ?? {}; } catch { parsed = {}; }
  const type = (parsed.type as string) ?? e.kind.replace(/^mutation_gated_(passed|suspect|dropped)$/, "mutation_gated");
  return { ...parsed, type, cycle_id: e.cycle_id, ts: e.ts } as CycleProgressEvent;
}

export function narrateEvent(e: CycleProgressEvent): Narration {
  const kind = e.type ?? e.event_type ?? e.kind ?? "";
  const x = e as Record<string, unknown>; // flattened wire fields
  const hash = (e.child_hash ?? e.bundle_hash ?? null) as string | null;
  switch (kind) {
    case "cycle_started":
      return { sentence: `Cycle ${e.cycle_id ?? "?"} started · ${x.parent_count ?? "?"} parents`, tone: "neutral", hash };
    case "parent_selected":
      return { sentence: `Parent selected: ${short(x.parent_hash as string)}`, tone: "neutral", hash: (x.parent_hash as string) ?? null };
    case "mutation_proposed": {
      const writer = (x.mutator_model as string) || "writer";
      return { sentence: `Writer ${writer} proposed an experiment → ${short(hash)}`, tone: "neutral", hash };
    }
    case "no_candidate":
      return { sentence: `No experiment produced for ${short(x.parent_hash as string)}${x.reason ? ` — ${x.reason}` : ""}`, tone: "warn", hash };
    case "mutation_gated": {
      const delta = fmtDelta(x.delta_day);
      const d = delta ? ` · ΔSharpe ${delta}` : "";
      if (x.outcome === "suspect")
        return { sentence: `Gate flagged ${short(hash)}${d} — suspect`, tone: "suspect", hash };
      if (x.passed === true || x.outcome === "kept")
        return { sentence: `Gate passed ${short(hash)}${d} — kept`, tone: "kept", hash };
      return { sentence: `Gate failed ${short(hash)}${d} — rejected`, tone: "rejected", hash };
    }
    case "honesty_check_run": {
      const msg = (x.message as string) ? ` — ${x.message}` : "";
      return x.passed
        ? { sentence: `Honesty check passed${msg}`, tone: "kept", hash }
        : { sentence: `Honesty check failed${msg} — results suspect`, tone: "suspect", hash };
    }
    case "judge_finding":
      return { sentence: `Judge (${x.severity ?? "info"}): ${x.code ?? "finding"} on ${short(hash)}`, tone: "warn", hash };
    case "cycle_finished":
      return {
        sentence: `Cycle finished — ${x.active_count ?? 0} kept · ${x.suspect_count ?? 0} suspect · ${x.rejected_count ?? 0} rejected`,
        tone: "neutral", hash,
      };
    case "phase_started":
      return { sentence: `Phase ${x.phase ?? "?"} started${x.detail ? ` — ${x.detail}` : ""}`, tone: "neutral", hash };
    case "phase_finished":
      return { sentence: `Phase ${x.phase ?? "?"} finished`, tone: "neutral", hash };
    default:
      return { sentence: kind || "event", tone: "neutral", hash };
  }
}
```

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): narrateEvent selector — events as sentences"`

---

### Task 5: `buildBoardState` selector

Events → board model: per-experiment card states and the active phase for the ribbon.

**Files:**
- Create: `frontend/web/src/features/autooptimizer/selectors/buildBoardState.ts`
- Test: `frontend/web/src/features/autooptimizer/selectors/buildBoardState.test.ts`

- [ ] **Step 1: Failing tests:**

```typescript
import { buildBoardState } from "./buildBoardState";

const ev = (type: string, extra: object = {}) => ({ type, ...extra });

it("tracks experiments through propose → gate (real flattened wire shapes)", () => {
  const s = buildBoardState([
    ev("cycle_started", { cycle_id: "c1", parent_count: 1 }),
    ev("mutation_proposed", { child_hash: "aaa", mutator_model: "gemini-2.5-pro", parent_hash: "p" }),
    ev("mutation_proposed", { child_hash: "bbb", mutator_model: "gpt-5.2", parent_hash: "p" }),
    ev("mutation_gated", { child_hash: "aaa", passed: true, outcome: "kept", delta_day: 0.21 }),
  ]);
  expect(s.phase).toBe("gate");
  expect(s.cards).toHaveLength(2);
  expect(s.cards[0]).toMatchObject({ hash: "aaa", writer: "gemini-2.5-pro", state: "kept", delta: 0.21 });
  expect(s.cards[1]).toMatchObject({ hash: "bbb", state: "evaluating" });
});

it("maps the 3-way gate outcomes and finished cycles", () => {
  const s = buildBoardState([
    ev("cycle_started", { cycle_id: "c1" }),
    ev("mutation_proposed", { child_hash: "aaa" }),
    ev("mutation_proposed", { child_hash: "bbb" }),
    ev("mutation_gated", { child_hash: "aaa", passed: false, outcome: "dropped", delta_day: -0.1 }),
    ev("mutation_gated", { child_hash: "bbb", passed: false, outcome: "suspect" }),
    ev("cycle_finished", { active_count: 0, suspect_count: 1, rejected_count: 1 }),
  ]);
  expect(s.phase).toBe("done");
  expect(s.cards[0].state).toBe("rejected");
  expect(s.cards[1].state).toBe("suspect");
});

it("is empty for no events", () => {
  const s = buildBoardState([]);
  expect(s.phase).toBe("idle");
  expect(s.cards).toEqual([]);
});
```

- [ ] **Step 2: Run** → FAIL.

- [ ] **Step 3: Implement:**

```typescript
import type { CycleProgressEvent } from "../api";

export type BoardCardState = "queued" | "evaluating" | "kept" | "rejected" | "suspect";
export type BoardCard = { hash: string; label: string | null; state: BoardCardState; delta: number | null; writer: string | null };
export type Phase = "idle" | "propose" | "eval" | "gate" | "keep" | "done";
export type BoardState = { phase: Phase; cards: BoardCard[]; cycleId: string | null };

export function buildBoardState(events: CycleProgressEvent[]): BoardState {
  const cards = new Map<string, BoardCard>();
  let phase: Phase = "idle";
  let cycleId: string | null = null;
  for (const e of events) {
    const kind = e.type ?? e.event_type ?? e.kind ?? "";
    const x = e as Record<string, unknown>; // flattened wire fields (progress.rs)
    const hash = (e.child_hash ?? e.bundle_hash ?? null) as string | null;
    if (kind === "cycle_started") { phase = "propose"; cycleId = e.cycle_id ?? null; cards.clear(); }
    if (kind === "mutation_proposed" && hash) {
      cards.set(hash, {
        hash, label: null, state: "evaluating", delta: null,
        writer: (x.mutator_model as string) || null,
      });
      phase = "eval";
    }
    if (kind === "mutation_gated" && hash) {
      const prev = cards.get(hash) ?? { hash, label: null, state: "evaluating" as const, delta: null, writer: null };
      const state =
        x.outcome === "suspect" ? "suspect"
        : x.passed === true || x.outcome === "kept" ? "kept"
        : "rejected";
      cards.set(hash, { ...prev, state, delta: typeof x.delta_day === "number" ? x.delta_day : null });
      phase = "gate";
    }
    if (kind === "honesty_check_run" && x.passed === false) {
      for (const c of cards.values()) if (c.state === "kept") cards.set(c.hash, { ...c, state: "suspect" });
    }
    if (kind === "cycle_finished") phase = "done";
  }
  return { phase, cards: [...cards.values()], cycleId };
}
```

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): buildBoardState selector"`

---### Task 6: `buildHeadline` selector + `EditorialHeadline` component

**Files:**
- Create: `frontend/web/src/features/autooptimizer/selectors/buildHeadline.ts` (+ test)
- Create: `frontend/web/src/features/autooptimizer/ui/EditorialHeadline.tsx` (+ test)

- [ ] **Step 1: Failing selector tests** (`selectors/buildHeadline.test.ts`):

```typescript
import { buildHeadline } from "./buildHeadline";

it("running state names the cycle count and lineages", () => {
  const h = buildHeadline({
    state: "running",
    activeLineages: 5,
    lastCycle: null,
    lastCycleAgo: null,
  });
  expect(h.title).toBe("A run is in progress.");
  expect(h.subtitle).toBe("1 cycle running · 5 active lineages.");
});

it("idle state reports the last cycle outcome", () => {
  const h = buildHeadline({
    state: "idle",
    activeLineages: 5,
    lastCycle: { kept: 2, total: 14 },
    lastCycleAgo: "3h ago",
  });
  expect(h.title).toBe("Last ran 3h ago — kept 2 of 14 experiments.");
});

it("idle state appends the best find one-liner when available", () => {
  const h = buildHeadline({
    state: "idle",
    activeLineages: 5,
    lastCycle: { kept: 2, total: 14 },
    lastCycleAgo: "3h ago",
    bestFind: { hash: "abcd1234ef", delta: 0.21 },
  });
  expect(h.subtitle).toBe("Best find: abcd1234 (ΔSharpe +0.21) · 5 active lineages.");
});

it("paused state names the state", () => {
  expect(buildHeadline({ state: "paused", activeLineages: 0, lastCycle: null, lastCycleAgo: null }).title)
    .toBe("A run is paused.");
});

it("never-ran state invites the first launch", () => {
  const h = buildHeadline({ state: "idle", activeLineages: 0, lastCycle: null, lastCycleAgo: null });
  expect(h.title).toBe("The optimizer hasn't run yet.");
  expect(h.subtitle).toBe("Launch its first cycle.");
});

it("never says tonight", () => {
  for (const state of ["running", "paused", "idle"] as const) {
    const h = buildHeadline({ state, activeLineages: 3, lastCycle: { kept: 1, total: 5 }, lastCycleAgo: "1d ago" });
    expect(`${h.title} ${h.subtitle}`.toLowerCase()).not.toContain("tonight");
  }
});
```

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement:**

```typescript
export type HeadlineInput = {
  state: "running" | "paused" | "cancelling" | "idle";
  activeLineages: number;
  lastCycle: { kept: number; total: number } | null;
  lastCycleAgo: string | null;
  bestFind?: { hash: string; delta: number } | null;
};
export type Headline = { title: string; subtitle: string };

export function buildHeadline(i: HeadlineInput): Headline {
  if (i.state === "running")
    return { title: "A run is in progress.", subtitle: `1 cycle running · ${i.activeLineages} active lineages.` };
  if (i.state === "paused")
    return { title: "A run is paused.", subtitle: "Resume it to keep experimenting." };
  if (i.state === "cancelling")
    return { title: "A run is cancelling.", subtitle: "Winding down in-flight experiments." };
  if (i.lastCycle && i.lastCycleAgo) {
    const best = i.bestFind
      ? `Best find: ${i.bestFind.hash.slice(0, 8)} (ΔSharpe ${i.bestFind.delta >= 0 ? "+" : "−"}${Math.abs(i.bestFind.delta).toFixed(2)}) · `
      : "";
    return {
      title: `Last ran ${i.lastCycleAgo} — kept ${i.lastCycle.kept} of ${i.lastCycle.total} experiments.`,
      subtitle: `${best}${i.activeLineages} active lineages.`,
    };
  }
  return { title: "The optimizer hasn't run yet.", subtitle: "Launch its first cycle." };
}
```

(`bestFind` is derived in Task 13 from the last cycle's `StatsRow.best_delta_holdout` + the cycle's best kept node hash from `useCycleRun(lastCycle.cycle_id)`; omit when no kept node exists.)

- [ ] **Step 4: Component test** (`ui/EditorialHeadline.test.tsx`, use `renderWithProviders` from `../test-utils`): renders title as an `<h1>`, the digest line (experiments · kept · tokens · spend, all passed as props), and a `children` action slot. Assert the four digest values appear and the action button renders.

```typescript
it("renders headline, digest line, and action slot", () => {
  renderWithProviders(
    <EditorialHeadline
      headline={{ title: "Last ran 3h ago — kept 2 of 14 experiments.", subtitle: "5 active lineages." }}
      digest={{ experiments: 54, kept: 7, tokens: "31.8M", spend: "$15.57" }}
    >
      <button>Launch run</button>
    </EditorialHeadline>,
  );
  expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent("Last ran 3h ago");
  expect(screen.getByText(/54 experiments/)).toBeInTheDocument();
  expect(screen.getByText(/\$15\.57/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Launch run" })).toBeInTheDocument();
});
```

- [ ] **Step 5: Implement `EditorialHeadline.tsx`** (style per shipped dashboard patterns — `tracking-tight` headline, mono digest, muted dot separators):

```tsx
import type { ReactNode } from "react";
import type { Headline } from "../selectors/buildHeadline";

export type Digest = { experiments: number; kept: number; tokens: string; spend: string };

export function EditorialHeadline({
  headline, digest, children,
}: { headline: Headline; digest: Digest | null; children?: ReactNode }) {
  return (
    <div className="flex items-end justify-between gap-6 flex-wrap">
      <div className="min-w-0 max-w-[780px]">
        <h1 className="text-[24px] font-semibold tracking-tight leading-tight">
          {headline.title} <span className="text-text-3 font-normal">{headline.subtitle}</span>
        </h1>
        {digest && (
          <div className="mt-2.5 font-mono text-[11.5px] text-text-3">
            <span><span className="text-text-2">{digest.experiments}</span> experiments this week</span>
            <span className="mx-2 text-text-4">·</span>
            <span><span className="text-gold">{digest.kept}</span> kept</span>
            <span className="mx-2 text-text-4">·</span>
            <span><span className="text-text-2">{digest.tokens}</span> tokens</span>
            <span className="mx-2 text-text-4">·</span>
            <span><span className="text-gold">{digest.spend}</span> spend</span>
          </div>
        )}
      </div>
      <div className="flex items-center gap-2">{children}</div>
    </div>
  );
}
```

- [ ] **Step 6: Run both test files** → PASS. **Step 7: Commit** `git commit -m "feat(frontend): buildHeadline selector + EditorialHeadline"`

---

### Task 7: `ExpandableArtifact` accordion

Inline accordion (no popups) used by board cards, feed items, and the river readout. Lazy-loads the experiment artifact via `useExperimentDetail(hash)` only when opened. Renders the **full persisted artifact** (spec §3): rationale ("why tested" — the writer's recorded reasoning), gate numbers (`GateScorecard`), **config diff vs parent** (`ParentDiffPanel`, folded in from the deleted ExperimentDetail screen), per-regime results (`RegimeCards` — this doubles as the "ΔSharpe vs parent" mini-chart since `RegimeResult.delta_sharpe` is the per-regime delta series), judge findings (`FindingsList`), and the **writer model** row when the caller can supply it (live `mutation_proposed` payloads carry `mutator_model`; persisted lineage records do not — omit the row rather than fabricate it, and note this data gap in the PR description). Raw writer prompt / raw model response are **not persisted by the backend today** (only `rationale` is, per `ExperimentDetailResponse`); render an honest "Full prompt/response transcripts aren't persisted yet." footnote line instead of empty sections — this is a recorded data-gap deviation from spec §3 to surface in the PR.

**Files:**
- Create: `frontend/web/src/features/autooptimizer/ui/ExpandableArtifact.tsx`
- Test: `frontend/web/src/features/autooptimizer/ui/ExpandableArtifact.test.tsx`

- [ ] **Step 1: Failing tests:** renders a summary row with `aria-expanded="false"`; clicking expands (`aria-expanded="true"`) and shows rationale + gate numbers from a mocked `useExperimentDetail`; `defaultOpen` prop starts expanded; while loading shows "Loading experiment…"; when the detail endpoint 404s shows "Artifact not available on this backend." (the hook already has `retry: false`).

```typescript
vi.mock("../api", async (orig) => ({
  ...(await orig()),
  useExperimentDetail: vi.fn(() => ({
    data: {
      lineage_node: { bundle_hash: "abcd1234ef", status: "active" },
      rationale: "Tighten the stop to cut tail losses",
      gate_record: { delta_day: 0.21, verdict: "Pass" },
      findings: [],
      regime_results: [],
    },
    isLoading: false,
    isError: false,
  })),
}));

it("expands inline to show the artifact", async () => {
  renderWithProviders(<ExpandableArtifact hash="abcd1234ef" summary={<span>v3.1.g · kept · +0.21</span>} />);
  const btn = screen.getByRole("button", { name: /v3\.1\.g/ });
  expect(btn).toHaveAttribute("aria-expanded", "false");
  await userEvent.click(btn);
  expect(btn).toHaveAttribute("aria-expanded", "true");
  expect(screen.getByText(/Tighten the stop/)).toBeInTheDocument();
});
```

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement:**

```tsx
import { useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { useExperimentDetail } from "../api";
import { GateScorecard } from "../panels/GateScorecard";
import { FindingsList } from "../panels/FindingsList";

export function ExpandableArtifact({
  hash, summary, defaultOpen = false,
}: { hash: string; summary: ReactNode; defaultOpen?: boolean }) {
  const [open, setOpen] = useState(defaultOpen);
  return (
    <div className="rounded-sm border border-border-soft bg-surface-card">
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen((v) => !v)}
        className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-[12px] hover:bg-surface-elev"
      >
        <span className="min-w-0 truncate">{summary}</span>
        <span className="text-text-4">{open ? "−" : "+"}</span>
      </button>
      {open && <ArtifactBody hash={hash} />}
    </div>
  );
}

function ArtifactBody({ hash, writerModel }: { hash: string; writerModel?: string | null }) {
  const detail = useExperimentDetail(hash);
  if (detail.isLoading) return <div className="px-3 py-3 text-[12px] text-text-3">Loading experiment…</div>;
  if (detail.isError || !detail.data)
    return <div className="px-3 py-3 text-[12px] text-text-3">Artifact not available on this backend.</div>;
  const d = detail.data;
  return (
    <div className="space-y-3 border-t border-border-soft px-3 py-3">
      {writerModel && (
        <div className="font-mono text-[11px] text-text-3">
          Writer: <span className="text-text-2">{writerModel}</span>
        </div>
      )}
      {d.rationale && (
        <div>
          <div className="text-[10px] uppercase tracking-widest text-text-4">Why tested</div>
          <p className="mt-1 text-[12.5px] text-text-2">{d.rationale}</p>
        </div>
      )}
      {d.gate_record && <GateScorecard gate_record={d.gate_record} />}
      <ParentDiffPanel hash={hash} /* match the panel's real props — it computed the diff client-side on ExperimentDetail; port its prop wiring */ />
      {d.regime_results.length > 0 && <RegimeCards results={d.regime_results} />}
      {d.findings.length > 0 && <FindingsList findings={d.findings} />}
      <p className="text-[10.5px] text-text-4">Full prompt/response transcripts aren't persisted yet.</p>
      <Link to={`/optimizer/strategy/${hash}`} className="inline-block text-[11px] text-gold hover:underline">
        Open strategy →
      </Link>
    </div>
  );
}
```

`ExpandableArtifact` accepts and forwards an optional `writerModel?: string | null` prop (callers that have live event payloads — NarratedFeed/ExperimentBoard via `BoardCard.writer` — pass it; replay/river callers omit it). Add tests: writer row renders when provided and is absent otherwise; ParentDiffPanel and RegimeCards sections render from the mocked detail; the transcript footnote always renders.

```tsx
// (props threading: add `writer: string | null` to BoardCard in Task 5 — buildBoardState
// captures payload.mutator_model on mutation_proposed — and pass through ExperimentBoard.)
```

(Match `GateScorecard`/`FindingsList` prop names to their actual signatures — check the panel files; adjust if they take different prop names.)

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): ExpandableArtifact inline accordion"`

---

### Task 8: `PhaseRibbon` + `ExperimentBoard`

**Files:**
- Create: `frontend/web/src/features/autooptimizer/ui/PhaseRibbon.tsx` (+ test)
- Create: `frontend/web/src/features/autooptimizer/ui/ExperimentBoard.tsx` (+ test)

- [ ] **Step 1: Failing tests.** PhaseRibbon: renders PROPOSE/EVAL/GATE/KEEP; the active phase has `aria-current="step"`; `phase="done"` marks all complete; `phase="idle"` renders nothing active. ExperimentBoard: given `BoardCard[]`, renders one ExpandableArtifact summary per card with state styling — kept shows the +delta in gold, rejected in danger, evaluating shows an animated chip; empty cards array renders nothing (parent handles empty states).

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement:**

```tsx
// PhaseRibbon.tsx
import type { Phase } from "../selectors/buildBoardState";

const PHASES: { key: Phase; label: string }[] = [
  { key: "propose", label: "Propose" },
  { key: "eval", label: "Eval" },
  { key: "gate", label: "Gate" },
  { key: "keep", label: "Keep" },
];
const ORDER: Phase[] = ["idle", "propose", "eval", "gate", "keep", "done"];

export function PhaseRibbon({ phase }: { phase: Phase }) {
  const idx = ORDER.indexOf(phase);
  return (
    <ol className="flex gap-1.5" aria-label="Cycle phases">
      {PHASES.map((p, i) => {
        const pos = i + 1; // position in ORDER
        const isDone = phase === "done" || pos < idx;
        const isActive = pos === idx && phase !== "done";
        return (
          <li
            key={p.key}
            aria-current={isActive ? "step" : undefined}
            className={`flex-1 rounded-sm px-2 py-1.5 text-center text-[10px] uppercase tracking-widest ${
              isActive ? "bg-gold text-on-accent font-semibold"
              : isDone ? "bg-gold-bg text-gold"
              : "bg-surface-elev text-text-4"
            }`}
          >
            {p.label}
          </li>
        );
      })}
    </ol>
  );
}
```

```tsx
// ExperimentBoard.tsx
import type { BoardCard } from "../selectors/buildBoardState";
import { ExpandableArtifact } from "./ExpandableArtifact";

const stateChip: Record<BoardCard["state"], string> = {
  queued: "text-text-4",
  evaluating: "text-warn animate-pulse",
  kept: "text-gold",
  rejected: "text-danger",
  suspect: "text-warn",
};
const stateLabel: Record<BoardCard["state"], string> = {
  queued: "queued", evaluating: "evaluating…", kept: "kept", rejected: "rejected", suspect: "suspect",
};

export function ExperimentBoard({ cards }: { cards: BoardCard[] }) {
  if (cards.length === 0) return null;
  // mobile: the grid collapses to one column = cards read as compact rows (spec §2 mobile note);
  // the summary line is a single flex row, so no separate mobile variant is needed.
  return (
    <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
      {cards.map((c) => (
        <ExpandableArtifact
          key={c.hash}
          hash={c.hash}
          summary={
            <span className="flex items-center gap-2 font-mono text-[12px]">
              <span className="text-text-2">{c.hash.slice(0, 8)}</span>
              {c.label && <span className="truncate text-text-3">{c.label}</span>}
              <span className={stateChip[c.state]}>
                {stateLabel[c.state]}
                {c.delta != null && ` ${c.delta >= 0 ? "+" : "−"}${Math.abs(c.delta).toFixed(2)}`}
              </span>
            </span>
          }
        />
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): PhaseRibbon + ExperimentBoard"`

---

### Task 9: `NarratedFeed`

**Files:**
- Create: `frontend/web/src/features/autooptimizer/ui/NarratedFeed.tsx` (+ test)

- [ ] **Step 1: Failing tests:** given events, renders one line per event in order with a time stamp and the `narrateEvent` sentence; tone classes applied (kept → gold, rejected → danger, warn/suspect → warn); lines whose narration has a `hash` render as ExpandableArtifact summaries (clickable, expandable); lines without a hash are plain rows; respects a `maxItems` prop (default 100, newest kept).

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement:**

```tsx
import type { CycleProgressEvent } from "../api";
import { narrateEvent, type NarrationTone } from "../selectors/narrateEvent";
import { ExpandableArtifact } from "./ExpandableArtifact";

const toneClass: Record<NarrationTone, string> = {
  kept: "text-gold", rejected: "text-danger", suspect: "text-warn", warn: "text-warn", neutral: "text-text-2",
};

function fmtTime(ts?: string) {
  if (!ts) return "";
  try { return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" }); }
  catch { return ""; }
}

export function NarratedFeed({ events, maxItems = 100 }: { events: CycleProgressEvent[]; maxItems?: number }) {
  const rows = events.slice(-maxItems);
  return (
    <ol className="space-y-1" aria-label="Cycle events">
      {rows.map((e, i) => {
        const n = narrateEvent(e);
        const line = (
          <span className="flex gap-3 font-mono text-[12px]">
            <span className="flex-none text-text-4">{fmtTime(e.ts)}</span>
            <span className={toneClass[n.tone]}>{n.sentence}</span>
          </span>
        );
        return (
          <li key={i}>
            {n.hash ? <ExpandableArtifact hash={n.hash} summary={line} /> : <div className="px-3 py-2">{line}</div>}
          </li>
        );
      })}
    </ol>
  );
}
```

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): NarratedFeed"`

---

### Task 10: `ConsoleModule` — live / replay / never-ran

The heart of the redesign. One component, three modes; the strings "Waiting for connection…" / "Waiting for the cycle…" must not exist.

**Files:**
- Create: `frontend/web/src/features/autooptimizer/ui/ConsoleModule.tsx` (+ test)

- [ ] **Step 1: Failing tests** (mock `../hooks/useCycleEventStream`, `../api`'s `useCycleEvents`, `useCycleRuns`):

```typescript
it("live mode renders ribbon, board and feed from the stream", () => { /* isRunning: true, events fixture → expect phase ribbon active, 2 board cards, feed lines */ });

it("idle mode replays the last completed cycle with a label", async () => {
  // isRunning: false; useCycleRuns → [{ cycle_id: "cyc-1", last_created_at: "…", node_count: 14, active_count: 2 }]
  // useCycleEvents("cyc-1") → persisted fixture events
  // expect: "Last cycle" label with relative time, ribbon all done, feed rendered
});

it("never renders waiting copy", () => {
  // isRunning false, connected false, no cycles
  expect(screen.queryByText(/waiting for/i)).not.toBeInTheDocument();
});

it("never-ran renders the phase explainer with launch slot", () => {
  // no cycles at all → explainer text for the four phases + the launch child rendered
});

it("replay with pruned/empty events falls back to a node-derived board", () => {
  // cycles exist; useCycleEvents → [] (pruned, or pre-persistence cycle, or older backend isError);
  // useCycleRun(replayId) → CycleRunDetail fixture with 3 nodes (active/rejected/quarantined).
  // expect: "Last cycle" label, ribbon all-done, board with 3 cards derived from the nodes
  // (kept/rejected/suspect), and the feed area showing "Event log unavailable for this cycle."
  // This keeps ?exp= deep links working for cycles without an event log (board still has cards).
});
```

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement:**

```tsx
import { useMemo, type ReactNode } from "react";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { useCycleEvents, useCycleRuns } from "../api";
import { normalizePersisted } from "../selectors/narrateEvent";
import { buildBoardState } from "../selectors/buildBoardState";
import { PhaseRibbon } from "./PhaseRibbon";
import { ExperimentBoard } from "./ExperimentBoard";
import { NarratedFeed } from "./NarratedFeed";

export function ConsoleModule({ launchAction, cycleId }: { launchAction?: ReactNode; cycleId?: string }) {
  const stream = useCycleEventStream();
  const cycles = useCycleRuns();
  // explicit cycleId (CycleDetail) > live cycle > most recent completed cycle
  const replayId = cycleId ?? (!stream.isRunning ? cycles.data?.[0]?.cycle_id ?? null : null);
  const persisted = useCycleEvents(stream.isRunning && !cycleId ? null : replayId);
  const events = useMemo(() => {
    if (stream.isRunning && !cycleId) return stream.events;
    return (persisted.data ?? []).map(normalizePersisted);
  }, [stream.isRunning, stream.events, persisted.data, cycleId]);
  const board = buildBoardState(events);

  if (!stream.isRunning && !cycleId && !cycles.isLoading && (cycles.data?.length ?? 0) === 0) {
    return <NeverRanExplainer launchAction={launchAction} />;
  }
  return (
    <section className="space-y-4 rounded-md border border-border bg-surface-card p-5">
      <div className="flex items-center justify-between gap-3">
        <div className="text-[11px] uppercase tracking-widest text-text-4">
          {stream.isRunning && !cycleId
            ? <span className="text-gold">Live · cycle {board.cycleId ?? "…"}</span>
            : <>Last cycle{lastCycleAgo ? ` · ${lastCycleAgo}` : ""}</>} {/* relative time via formatRelativeTime(CycleRunSummary.last_created_at) */}
        </div>
        {launchAction}
      </div>
      <PhaseRibbon phase={stream.isRunning && !cycleId ? board.phase : "done"} />
      <ExperimentBoard cards={board.cards} />
      <NarratedFeed events={events} />
    </section>
  );
}

function NeverRanExplainer({ launchAction }: { launchAction?: ReactNode }) {
  const phases = [
    ["Propose", "Experiment writers draft variations of your strategy."],
    ["Eval", "Each experiment is backtested across regimes."],
    ["Gate", "A gate compares each result to its parent — honestly."],
    ["Keep", "Winners join the lineage; the rest are recorded and rejected."],
  ];
  return (
    <section className="space-y-4 rounded-md border border-border bg-surface-card p-5">
      <p className="text-[14px] text-text-2">Each cycle runs four phases:</p>
      <div className="grid gap-3 sm:grid-cols-4">
        {phases.map(([t, d]) => (
          <div key={t} className="rounded-sm border border-border-soft p-3">
            <div className="text-[11px] uppercase tracking-widest text-gold">{t}</div>
            <p className="mt-1 text-[12px] text-text-3">{d}</p>
          </div>
        ))}
      </div>
      {launchAction}
    </section>
  );
}
```

Replay edge case (implement + tested above): when `persisted` resolves empty or errors while a replay id exists (events pruned by `prune_old_events`, cycles run before Task 0b shipped, or an older backend lacking the endpoint), fall back to a **node-derived board**: add a small pure helper `boardFromNodes(nodes: CycleNodeDetail[]) → BoardCard[]` (status → kept/rejected/suspect; delta from the node's gate verdict where present; writer null) in `buildBoardState.ts` (+ unit test), feed area renders "Event log unavailable for this cycle." — never a blank board/feed, and `?exp=` deep links still have cards to expand. CycleDetail already fetches `useCycleRun(cycleId)`; the home's replay fetches it for the replay id.

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): ConsoleModule — live, replay, never-ran"`

---

### Task 11: `buildRiverLayout` selector

Pure layout: `RiverNode[]` → positioned lineage lines, attempt stubs, and the live frontier. Coordinates are abstract (x = generation index, y = Sharpe score); the SVG component scales them.

**Files:**
- Create: `frontend/web/src/features/autooptimizer/selectors/buildRiverLayout.ts` (+ test)

- [ ] **Step 1: Failing tests:**

```typescript
import { buildRiverLayout } from "./buildRiverLayout";

const node = (hash: string, parent: string | null, status: string, score: number | null, delta: number | null, at: string) =>
  ({ bundle_hash: hash, parent_hash: parent, cycle_id: `c-${hash}`, status, created_at: at, child_day_score: score, delta_day: delta });

it("chains kept nodes into a lineage line and hangs rejected as stubs", () => {
  const layout = buildRiverLayout([
    node("root", null, "active", 1.0, null, "2026-06-01"),
    node("kept1", "root", "active", 1.2, 0.2, "2026-06-02"),
    node("rej1", "root", "rejected", 0.9, -0.1, "2026-06-02"),
    node("sus1", "kept1", "quarantined", 1.5, 0.3, "2026-06-03"),
    node("kept2", "kept1", "active", 1.4, 0.2, "2026-06-03"),
  ]);
  const lines = layout.lines;
  expect(lines).toHaveLength(1);
  expect(lines[0].points.map((p) => p.hash)).toEqual(["root", "kept1", "kept2"]);
  expect(lines[0].points.map((p) => p.y)).toEqual([1.0, 1.2, 1.4]);
  const stubKinds = layout.stubs.map((s) => [s.hash, s.kind]);
  expect(stubKinds).toContainEqual(["rej1", "rejected"]);
  expect(stubKinds).toContainEqual(["sus1", "suspect"]);
});

it("assigns stub ageRank by created_at for fade-with-age rendering", () => {
  const layout = buildRiverLayout([
    node("root", null, "active", 1.0, null, "2026-06-01"),
    node("oldRej", "root", "rejected", 0.9, -0.1, "2026-06-02"),
    node("kept1", "root", "active", 1.2, 0.2, "2026-06-03"),
    node("newRej", "kept1", "rejected", 1.1, -0.1, "2026-06-09"),
  ]);
  const old = layout.stubs.find((s) => s.hash === "oldRej")!;
  const recent = layout.stubs.find((s) => s.hash === "newRej")!;
  expect(old.ageRank).toBeLessThan(recent.ageRank);
  expect(recent.ageRank).toBe(1);
});

it("marks a line dead (retired) when its tip stopped producing while newer cycles exist", () => {
  const layout = buildRiverLayout([
    node("a", null, "active", 1.0, null, "2026-06-01"),
    node("a2", "a", "active", 1.3, 0.3, "2026-06-09"), // alive: tip in latest cycle window
    node("b", null, "active", 0.9, null, "2026-06-01"), // dead: no descendants since, newer activity exists
  ]);
  const lineA = layout.lines.find((l) => l.points.some((p) => p.hash === "a2"))!;
  const lineB = layout.lines.find((l) => l.points.at(-1)?.hash === "b")!;
  expect(lineA.alive).toBe(true);
  expect(lineB.alive).toBe(false);
});

it("marks the highest-scoring live line as champion", () => {
  const layout = buildRiverLayout([
    node("a", null, "active", 1.0, null, "2026-06-01"),
    node("a2", "a", "active", 1.6, 0.6, "2026-06-02"),
    node("b", null, "active", 1.1, null, "2026-06-01"),
  ]);
  expect(layout.lines.find((l) => l.champion)?.points.at(-1)?.hash).toBe("a2");
});

it("handles a single node with no score", () => {
  const layout = buildRiverLayout([node("only", null, "active", null, null, "2026-06-01")]);
  expect(layout.lines).toHaveLength(1);
  expect(layout.lines[0].points[0].y).toBe(1.0); // default baseline when score missing
  expect(layout.yDomain[0]).toBeLessThan(layout.yDomain[1]);
});

it("returns empty layout for no nodes", () => {
  expect(buildRiverLayout([]).lines).toEqual([]);
});

it("zero-kept case: roots with only rejected children yield 1-point lines and stubs only", () => {
  const layout = buildRiverLayout([
    node("root", null, "active", 1.0, null, "2026-06-01"),
    node("r1", "root", "rejected", 0.9, -0.1, "2026-06-02"),
  ]);
  expect(layout.lines[0].points).toHaveLength(1);
  expect(layout.stubs).toHaveLength(1);
});
```

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement:**

```typescript
import type { RiverNode } from "../api";

export type RiverPoint = { hash: string; x: number; y: number; cycleId: string | null };
export type RiverLine = { points: RiverPoint[]; champion: boolean; alive: boolean };
export type RiverStub = {
  hash: string; fromX: number; fromY: number; y: number;
  kind: "rejected" | "suspect"; delta: number | null; cycleId: string | null;
  ageRank: number; // 0 = oldest … 1 = newest, by created_at; renderer maps to opacity (fade with age)
};
export type RiverLayout = { lines: RiverLine[]; stubs: RiverStub[]; xMax: number; yDomain: [number, number] };

const DEFAULT_Y = 1.0;

export function buildRiverLayout(nodes: RiverNode[]): RiverLayout {
  if (nodes.length === 0) return { lines: [], stubs: [], xMax: 0, yDomain: [0, 2] };
  const byHash = new Map(nodes.map((n) => [n.bundle_hash, n]));
  const childrenOf = new Map<string | null, RiverNode[]>();
  for (const n of nodes) {
    const key = n.parent_hash && byHash.has(n.parent_hash) ? n.parent_hash : null;
    childrenOf.set(key, [...(childrenOf.get(key) ?? []), n]);
  }
  const yOf = (n: RiverNode) => n.child_day_score ?? DEFAULT_Y;
  const pos = new Map<string, RiverPoint>();
  const lines: RiverLine[] = [];
  const stubs: RiverStub[] = [];

  // walk each root's keep-chain: at each node, the next "active" child continues
  // the line; rejected/quarantined children become stubs off that node.
  for (const root of childrenOf.get(null) ?? []) {
    let cur: RiverNode | undefined = root;
    let x = 0;
    const points: RiverPoint[] = [];
    while (cur) {
      const p: RiverPoint = { hash: cur.bundle_hash, x, y: yOf(cur), cycleId: cur.cycle_id };
      pos.set(cur.bundle_hash, p);
      points.push(p);
      const kids: RiverNode[] = childrenOf.get(cur.bundle_hash) ?? [];
      for (const k of kids) {
        if (k.status === "rejected" || k.status === "quarantined") {
          stubs.push({
            hash: k.bundle_hash, fromX: x, fromY: p.y, y: yOf(k),
            kind: k.status === "rejected" ? "rejected" : "suspect",
            delta: k.delta_day, cycleId: k.cycle_id, ageRank: 0, // set after the walk
          });
        }
      }
      const next = kids
        .filter((k) => k.status === "active")
        .sort((a, b) => a.created_at.localeCompare(b.created_at));
      // first active child continues this line; further active children start new lines
      for (const extra of next.slice(1)) {
        const sub = walkLine(extra, x + 1, childrenOf, yOf, pos, stubs);
        lines.push(sub);
      }
      cur = next[0];
      x += 1;
    }
    lines.push({ points, champion: false, alive: points.length > 0 });
  }

  // ageRank: normalized recency of each stub's created_at over the dataset's time span
  const times = nodes.map((n) => Date.parse(n.created_at)).filter(Number.isFinite);
  const t0 = Math.min(...times), t1 = Math.max(...times);
  for (const s of stubs) {
    const t = Date.parse(byHash.get(s.hash)!.created_at);
    s.ageRank = t1 === t0 ? 1 : (t - t0) / (t1 - t0);
  }
  // alive: a line is alive if its tip is in the newest 25% of the dataset's time
  // span OR is the newest node overall; otherwise it retired (dim it out).
  for (const l of lines) {
    const tip = l.points.at(-1);
    const tipT = tip ? Date.parse(byHash.get(tip.hash)!.created_at) : t0;
    l.alive = t1 === t0 || tipT >= t1 - (t1 - t0) * 0.25;
  }
  const allY = [...lines.flatMap((l) => l.points.map((p) => p.y)), ...stubs.map((s) => s.y)];
  const yMin = Math.min(...allY), yMax = Math.max(...allY);
  const pad = Math.max(0.1, (yMax - yMin) * 0.15);
  const xMax = Math.max(...lines.flatMap((l) => l.points.map((p) => p.x)), 0);
  // champion: live line whose tip has the highest y
  const live = lines.filter((l) => l.alive && l.points.length > 0);
  const champ = live.sort((a, b) => (b.points.at(-1)!.y) - (a.points.at(-1)!.y))[0];
  if (champ) champ.champion = true;
  return { lines, stubs, xMax, yDomain: [yMin - pad, yMax + pad] };
}

function walkLine(
  start: RiverNode, x0: number,
  childrenOf: Map<string | null, RiverNode[]>, yOf: (n: RiverNode) => number,
  pos: Map<string, RiverPoint>, stubs: RiverStub[],
): RiverLine {
  const points: RiverPoint[] = [];
  let cur: RiverNode | undefined = start;
  let x = x0;
  while (cur) {
    const p: RiverPoint = { hash: cur.bundle_hash, x, y: yOf(cur), cycleId: cur.cycle_id };
    pos.set(cur.bundle_hash, p);
    points.push(p);
    const kids: RiverNode[] = childrenOf.get(cur.bundle_hash) ?? [];
    for (const k of kids) {
      if (k.status === "rejected" || k.status === "quarantined") {
        stubs.push({
          hash: k.bundle_hash, fromX: x, fromY: p.y, y: yOf(k),
          kind: k.status === "rejected" ? "rejected" : "suspect",
          delta: k.delta_day, cycleId: k.cycle_id, ageRank: 0, // set after the walk
        });
      }
    }
    const next = kids.filter((k) => k.status === "active").sort((a, b) => a.created_at.localeCompare(b.created_at));
    cur = next[0];
    x += 1;
  }
  return { points, champion: false, alive: points.length > 0 };
}
```

(Note: branch active children beyond the first start new lines mid-walk in the root loop but not in `walkLine` recursion depth >1 — if the second test surfaces ordering issues, extract the root loop body to use `walkLine` for everything; the tests are the contract.)

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): buildRiverLayout selector"`

---

### Task 12: `LineageRiver` component

SVG renderer + hover readout card + **live frontier**. No floating tooltips: hover populates a fixed readout strip below the chart, rendered as an `ExpandableArtifact` when a branch is selected. While a cycle runs, the river consumes the **same SSE stream as the console** (`useCycleEventStream` + `buildBoardState`) and renders a pulsing frontier node at the champion line's tip with a ghost-fan: one dashed ghost stub per in-flight experiment, resolving as gates land (resolved experiments leave the ghost-fan because `buildBoardState` marks them kept/rejected; the persisted picture catches up on the next `useRiver` refetch — set `refetchInterval: 15_000` while `isRunning`).

**Files:**
- Create: `frontend/web/src/features/autooptimizer/ui/LineageRiver.tsx` (+ test)

- [ ] **Step 1: Failing tests** (mock `useRiver` and `../hooks/useCycleEventStream`):
  - renders an `<svg role="img" aria-label="Lineage river">` with one `path` per line and per stub (use `data-testid="river-line"` / `"river-stub"`);
  - **fade with age:** stub `opacity` attribute increases with `ageRank` (assert an old stub's opacity < a new stub's);
  - **retired dim-out:** a line with `alive: false` carries the dimmed class (`stroke-text-4/40`); live lines don't;
  - hovering a stub (fireEvent.mouseOver) populates the readout strip with the experiment hash and "Rejected"/"Suspect" + delta;
  - clicking a stub or line point renders the readout as an expanded artifact with an "Open cycle →" link to `/optimizer/cycle/<cycleId>?exp=<hash>`;
  - **live-end routing:** each live line's tip renders an `aria-label="Open strategy <hash>"` affordance (`data-testid="river-live-end"`) that navigates to `/optimizer/strategy/<hash>` on click;
  - **live frontier:** with `isRunning: true` and stream events containing two `mutation_proposed` (one later `mutation_gated`, one still unresolved), renders `data-testid="river-frontier"` (a pulsing node containing an SVG `<animate>`) and exactly one `data-testid="river-ghost"` dashed stub for the unresolved experiment; with `isRunning: false`, neither testid renders;
  - zero kept experiments still renders with the label "nothing kept yet";
  - **empty river but history exists** (`useRiver` → `[]`, prop `hasHistory={true}`): renders an honest labeled panel ("No lineage recorded yet.") — never a blank panel;
  - empty river and `hasHistory={false}`: renders nothing (`container.firstChild === null` — the home composes the never-ran explainer instead);
  - degenerate single-node data renders without NaN in any path `d` attribute (regression for audit F8-class bugs): `expect(container.querySelector('path[d*="NaN"]')).toBeNull()`.

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement** (compact version; executor may refine visuals, tests pin behavior):

```tsx
import { useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useRiver } from "../api";
import { useCycleEventStream } from "../hooks/useCycleEventStream";
import { buildBoardState } from "../selectors/buildBoardState";
import { buildRiverLayout, type RiverStub, type RiverPoint } from "../selectors/buildRiverLayout";
import { ExpandableArtifact } from "./ExpandableArtifact";

const W = 640, H = 220, PAD = 24;

type Hover =
  | { kind: "stub"; stub: RiverStub }
  | { kind: "point"; point: RiverPoint; champion: boolean }
  | null;

export function LineageRiver({ hasHistory = false }: { hasHistory?: boolean }) {
  const stream = useCycleEventStream();
  const river = useRiver(); // pass { refetchIntervalWhileRunning: stream.isRunning } — add the option to the Task 3 hook
  const layout = useMemo(() => buildRiverLayout(river.data ?? []), [river.data]);
  const board = buildBoardState(stream.isRunning ? stream.events : []);
  const inflight = board.cards.filter((c) => c.state === "evaluating" || c.state === "queued");
  const [hover, setHover] = useState<Hover>(null);
  const [pinned, setPinned] = useState<Hover>(null);
  const navigate = useNavigate();
  if (!river.data || river.data.length === 0) {
    if (!hasHistory) return null;
    return (
      <section className="rounded-md border border-border bg-surface-card p-5">
        <div className="text-[11px] uppercase tracking-widest text-text-4">Lineage · Sharpe over generations</div>
        <p className="mt-2 text-[12px] text-text-3">No lineage recorded yet.</p>
      </section>
    );
  }

  const sx = (x: number) => PAD + (layout.xMax === 0 ? 0 : (x / layout.xMax) * (W - 2 * PAD));
  const [y0, y1] = layout.yDomain;
  const sy = (y: number) => H - PAD - ((y - y0) / (y1 - y0)) * (H - 2 * PAD);
  const keptCount = layout.lines.reduce((n, l) => n + l.points.length - 1, 0);
  const active = pinned ?? hover;

  return (
    <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
      <div className="text-[11px] uppercase tracking-widest text-text-4">
        Lineage · Sharpe over generations
        {keptCount <= 0 && (
          <span className="ml-2 text-text-3 normal-case tracking-normal">
            {layout.lines.length} line{layout.lines.length === 1 ? "" : "s"} · nothing kept yet
          </span>
        )}
      </div>
      <svg role="img" aria-label="Lineage river" viewBox={`0 0 ${W} ${H}`} className="w-full">
        {layout.stubs.map((s) => (
          <line
            key={s.hash} data-testid="river-stub"
            x1={sx(s.fromX)} y1={sy(s.fromY)} x2={sx(s.fromX) + 26} y2={sy(s.y)}
            className={s.kind === "suspect" ? "stroke-warn/70" : "stroke-border-strong"}
            opacity={0.35 + 0.65 * s.ageRank} // fade with age: oldest stubs are faintest
            strokeDasharray={s.kind === "suspect" ? "3 2" : undefined}
            strokeWidth={hover?.kind === "stub" && hover.stub.hash === s.hash ? 2.5 : 1.2}
            onMouseOver={() => setHover({ kind: "stub", stub: s })}
            onClick={() => setPinned({ kind: "stub", stub: s })}
            style={{ cursor: "pointer" }}
          />
        ))}
        {layout.lines.map((l, i) => (
          <g key={i}>
            <path
              data-testid="river-line"
              d={l.points.map((p, j) => `${j === 0 ? "M" : "L"}${sx(p.x)},${sy(p.y)}`).join(" ")}
              fill="none"
              className={!l.alive ? "stroke-text-4/40" : l.champion ? "stroke-gold" : "stroke-gold-soft/60"}
              strokeWidth={l.champion ? 2.4 : 1.5}
            />
            {l.points.map((p) => (
              <circle
                key={p.hash} cx={sx(p.x)} cy={sy(p.y)} r={l.champion ? 3.5 : 2.5}
                className={l.champion ? "fill-gold" : "fill-gold-soft"}
                onMouseOver={() => setHover({ kind: "point", point: p, champion: l.champion })}
                onClick={() => setPinned({ kind: "point", point: p, champion: l.champion })}
                style={{ cursor: "pointer" }}
              />
            ))}
            {l.alive && l.points.length > 0 && (
              <circle
                data-testid="river-live-end"
                role="link"
                aria-label={`Open strategy ${l.points.at(-1)!.hash}`}
                cx={sx(l.points.at(-1)!.x)} cy={sy(l.points.at(-1)!.y)} r={6}
                className="fill-transparent"
                onClick={() => navigate(`/optimizer/strategy/${l.points.at(-1)!.hash}`)}
                style={{ cursor: "pointer" }}
              />
            )}
          </g>
        ))}
        {stream.isRunning && layout.lines.some((l) => l.champion) && (() => {
          const tip = layout.lines.find((l) => l.champion)!.points.at(-1)!;
          return (
            <g data-testid="river-frontier">
              <circle cx={sx(tip.x)} cy={sy(tip.y)} r={5} className="fill-gold" opacity={0.3}>
                <animate attributeName="r" values="5;9;5" dur="2s" repeatCount="indefinite" />
              </circle>
              {inflight.map((c, k) => (
                <line
                  key={c.hash} data-testid="river-ghost"
                  x1={sx(tip.x)} y1={sy(tip.y)}
                  x2={sx(tip.x) + 30} y2={sy(tip.y) + (k - inflight.length / 2) * 12}
                  className="stroke-gold/40" strokeDasharray="2 3"
                />
              ))}
            </g>
          );
        })()}
      </svg>
      <RiverReadout active={active} onOpenCycle={(cycleId, hash) => navigate(`/optimizer/cycle/${cycleId}?exp=${hash}`)} />
    </section>
  );
}

function RiverReadout({ active, onOpenCycle }: {
  active: Hover; onOpenCycle: (cycleId: string, hash: string) => void;
}) {
  if (!active)
    return <div className="rounded-sm border border-border-soft px-3 py-2 font-mono text-[11px] text-text-4">hover a branch…</div>;
  const hash = active.kind === "stub" ? active.stub.hash : active.point.hash;
  const cycleId = active.kind === "stub" ? active.stub.cycleId : active.point.cycleId;
  const summary =
    active.kind === "stub" ? (
      <span className="font-mono text-[11px]">
        {hash.slice(0, 8)} · <span className={active.stub.kind === "suspect" ? "text-warn" : "text-danger"}>
          {active.stub.kind === "suspect" ? "Suspect" : "Rejected"}
        </span>
        {active.stub.delta != null && ` · ΔSharpe ${active.stub.delta >= 0 ? "+" : "−"}${Math.abs(active.stub.delta).toFixed(2)}`}
      </span>
    ) : (
      <span className="font-mono text-[11px]">
        {hash.slice(0, 8)} · <span className="text-gold">{active.champion ? "Champion" : "Kept"}</span> · Sharpe {active.point.y.toFixed(2)}
      </span>
    );
  return (
    <div className="space-y-1">
      <ExpandableArtifact hash={hash} summary={summary} />
      {cycleId && (
        <button type="button" onClick={() => onOpenCycle(cycleId, hash)} className="text-[11px] text-gold hover:underline">
          Open cycle →
        </button>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): LineageRiver signature chart with inline readout"`

---

### Task 13: New OptimizerHome

Assemble: EditorialHeadline → ConsoleModule → charts row (LineageRiver + EdgeVsRandomChart) → ExperimentWritersPanel → cycle history. Honors `?session=` scoping (absorbs RunDetail). The launch panel (existing inline launcher in OptimizerHome) is retained as the EditorialHeadline action slot.

**Files:**
- Rewrite: `frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx`
- Rewrite test: `frontend/web/src/features/autooptimizer/screens/OptimizerHome.test.tsx`

- [ ] **Step 1: Rewrite the test file first** (mock the api hooks + event stream as the current test does):
  - idle with history → renders "Last ran … — kept …" heading, the Launch run button, the console module replay label, the river section, writers panel, history table;
  - running → "A run is in progress." + Pause and Cancel buttons; no Launch;
  - paused → "A run is paused." + Resume and Cancel buttons; no Launch;
  - idle with an enabled schedule (mock `useSchedule` → `{ enabled: true, next_run_at: … }`) → the headline area shows "next run <relative time>";
  - `route: "/optimizer?session=sess-1"` → history filtered: `useOptimizerStats` called with `{ session_id: "sess-1" }` and a "Session sess-1" scope chip with a clear (×) link back to `/optimizer`;
  - never-ran (no cycles, no session) → "The optimizer hasn't run yet." + the four-phase explainer;
  - `expect(screen.queryByText(/waiting for/i)).toBeNull()` in all states.

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement the screen:**

Structure (full JSX in implementation; key wiring):

```tsx
export function OptimizerHome() {
  const [params] = useSearchParams();
  const sessionId = params.get("session");
  const status = useOptimizerStatus(); // NB: returns StatusResponse | undefined directly, NOT a query object (api.ts:669)
  const stats = useOptimizerStats(sessionId ? { session_id: sessionId } : undefined);
  const cycles = useCycleRuns();
  const lineage = useLineageNodes({ status: "active" });
  const session = status?.active_session ?? null;
  const state = (session?.state ?? "idle") as HeadlineInput["state"];
  const lastCycle = cycles.data?.[0] ?? null; // CycleRunSummary: use last_created_at (no created_at/finished_at fields)
  const headline = buildHeadline({
    state,
    activeLineages: countActiveLineages(lineage.data ?? []), // port helper from LiveCycleView before deleting it
    lastCycle: lastCycle ? { kept: lastCycle.active_count, total: lastCycle.node_count } : null,
    lastCycleAgo: lastCycle ? formatRelativeTime(lastCycle.last_created_at) : null, // keep existing helper
    bestFind: deriveBestFind(stats.data, lastCycle), // best_delta_holdout from the last cycle's StatsRow + its best kept hash via useCycleRun
  });
  const digest = buildDigest(stats.data ?? []); // sums kept/spend/tokens over trailing 7 days from StatsRow[]
  // …
  return (
    <>
      <Topbar title="Optimizer" />
      <div className="space-y-5">
        <EditorialHeadline headline={headline} digest={digest}>
          {/* contextual action: Launch (toggles existing launch panel) | Pause+Cancel | Resume+Cancel
              — port the mutation wiring (usePauseCycle/useResumeCycle/useCancelSession) from the old CommandBar */}
        </EditorialHeadline>
        {sessionId && <SessionScopeChip sessionId={sessionId} />}
        {launcherOpen && <LaunchPanel /> /* keep the existing inline launch panel exactly as-is */}
        <ConsoleModule launchAction={launchButton} />
        <div className="grid gap-4 lg:grid-cols-2">
          <LineageRiver hasHistory={(cycles.data?.length ?? 0) > 0} />
          <EdgeVsRandomChart /* existing props */ />
        </div>
        <ExperimentWritersPanel />
        <RecentCyclesTableBody /* existing props; rows already link to cycle detail */ />
      </div>
    </>
  );
}
```

`buildDigest` is a small pure helper in `selectors/` (+ unit test) over `StatsRow[]` AND `CycleRunSummary[]`: from stats rows where `ts` ≥ 7 days ago sum `kept` and `cost_usd` → `$X.XX`; **tokens come from `CycleRunSummary.input_tokens + output_tokens`** (api.ts:169 — present since F23) summed over cycles whose `last_created_at` ≥ 7 days ago, formatted compactly (`31.8M`). `useCycleRuns` is already fetched by this screen. If every cycle's token fields are null, omit the stat (EditorialHeadline renders a 3-item digest gracefully; test both). FlywheelStrip, ScheduleStrip, PhaseStepper (the old Phase 1–4 project stepper), ImprovementChart, OutcomeStackedChart are dropped from the home per spec — ScheduleStrip's "next run" timestamp moves into the idle headline subtitle when a schedule exists (`useSchedule`).

- [ ] **Step 4: Token restyle sweep of retained panels.** `rg "border-(white|gray-[12]00)|#fff" frontend/web/src/features/autooptimizer/panels/ExperimentWritersPanel.tsx frontend/web/src/features/autooptimizer/panels/RecentCyclesTable.tsx frontend/web/src/features/autooptimizer/ui/EdgeVsRandomChart.tsx` — replace any non-token color/border classes with theme tokens; align section-header typography to `text-[11px] uppercase tracking-widest text-text-4`. Commit only what the grep proves needs changing.

- [ ] **Step 5: Honesty chips (spec §5).** The digest line gains a freshness stamp ("as of <relative time>" from the newest StatsRow ts) and the river/edge charts' section labels carry their sample size ("n attempts", "n cycles"). Add one test: digest renders the freshness stamp; river label includes the attempt count.

- [ ] **Step 6: Run** `npm run test -- OptimizerHome` → PASS. Also run `npm run test -- features/autooptimizer` and fix any collateral failures in panels still imported.
- [ ] **Step 7: Commit** `git commit -m "feat(frontend): OptimizerHome as editorial mission report"`

---

### Task 14: CycleDetail rework + `?exp=` deep link

**Files:**
- Rewrite: `frontend/web/src/features/autooptimizer/screens/CycleDetail.tsx` (+ test)

- [ ] **Step 1: Tests first:**
  - renders an editorial headline built from the cycle, including the best-find clause when a kept node exists ("Cycle 7f3a kept 2 of 14 experiments — best find abcd1234, ΔSharpe +0.21.") and without it otherwise; breadcrumb back to /optimizer;
  - the feed renders ALL events for the cycle (CycleDetail passes `maxItems={Number.POSITIVE_INFINITY}` — spec §4 "feed complete"; fixture with 120 events asserts 120 rows);
  - renders ConsoleModule in replay mode for this cycle (mock `useCycleEvents` returning fixture events);
  - `route: "/optimizer/cycle/cyc-1?exp=abcd1234ef"` → the board's ExpandableArtifact for that hash has `aria-expanded="true"` on mount;
  - without `?exp=`, ALL board cards mount expanded (`expandBoard` — spec §4 "board expanded by default");
  - retains GateBuckets, EvalMatrix, LineageTreePanel sections beneath.

- [ ] **Step 2: Run** → FAIL. **Step 3: Implement:** keep the current data wiring (`useCycleRun`), replace the hero/stat grid with `EditorialHeadline` (title includes the best-find clause derived from the cycle's best kept node's gate delta; no digest line; subtitle = `$spend · n experiments`), insert `<ConsoleModule cycleId={cycleId} />`, keep GateBuckets/EvalMatrix/CycleExperimentsTable/LineageTreePanel below. Per-experiment parent/origin diffs are served by the ExpandableArtifact (ParentDiffPanel folded in, Task 7) inside the board — this satisfies spec §4's "parent/origin diffs" retention; origin diffs additionally remain on StrategyInspector untouched. Run the same token-restyle grep as Task 13 Step 4 over GateBuckets/EvalMatrix/LineageTreePanel/CycleExperimentsTable and fix violations only. Deep link + default expansion: add optional `defaultOpenHash?: string` and `expandBoard?: boolean` props to `ConsoleModule` → `ExperimentBoard`. `defaultOpenHash` sets `defaultOpen` on the matching `ExpandableArtifact`; `expandBoard` (spec §4: "board expanded by default") sets `defaultOpen` on ALL board cards. CycleDetail passes `expandBoard` always and `defaultOpenHash` when `?exp=` is present. Add prop threading + a test at each level in the same commit, including: CycleDetail without `?exp=` renders every board card with `aria-expanded="true"`.

- [ ] **Step 4: Run** → PASS. **Step 5: Commit** `git commit -m "feat(frontend): CycleDetail with shared console module + ?exp deep link"`

---

### Task 15: Routes, redirects, deletions

**Files:**
- Modify: `frontend/web/src/routes.tsx` (lines ~222–233)
- Delete: `LiveCycleView.tsx` + `LiveCycleView.test.tsx`, `screens/ExperimentDetail.tsx` + test, `screens/RunDetail.tsx` + test
- Possibly delete now-orphaned: `ui/ActivityFeed.tsx`, `ui/SpendChart.tsx`, `ui/ImprovementChart.tsx`, `ui/OutcomeStackedChart.tsx`, `ui/PhaseStepper.tsx`, `ui/FlywheelStrip.tsx`, `ui/ScheduleStrip.tsx`, `panels/LiveEvalHeatmap.tsx`, `panels/RecentCyclesTable.tsx` (only if OptimizerHome no longer uses RecentCyclesTableBody — it does use it; keep) — delete **only** what `rg "import .* from .*<Name>"` proves unused.

- [ ] **Step 1: Test the redirects first** (routes test or a small `OptimizerRedirects.test.tsx`):
  - `/optimizer/run/sess-1` → lands on OptimizerHome with `?session=sess-1`;
  - `/optimizer/experiment/abcd1234ef` → mocked `useLineageNode("abcd1234ef")` returns `{ cycle_id: "cyc-1" }` → navigates to `/optimizer/cycle/cyc-1?exp=abcd1234ef`; while loading shows a one-line "Locating experiment…" row; if the node is unknown, falls back to `/optimizer`;
  - legacy `/autooptimizer/diff/:hash` still resolves (now through the new experiment redirect).

- [ ] **Step 2: Implement** in `routes.tsx`:

```tsx
function ExperimentRedirect() {
  const { hash } = useParams();
  const node = useLineageNode(hash ?? "");
  if (node.isLoading) return <div className="p-6 text-[12px] text-text-3">Locating experiment…</div>;
  if (node.data?.cycle_id) return <Navigate to={`/optimizer/cycle/${node.data.cycle_id}?exp=${hash}`} replace />;
  return <Navigate to="/optimizer" replace />;
}
// route table:
// /optimizer                      → OptimizerHome
// /optimizer/cycle/:cycleId       → CycleDetail
// /optimizer/experiment/:hash     → ExperimentRedirect
// /optimizer/run/:sessionId       → <Navigate to={`/optimizer?session=${sessionId}`} replace /> (small wrapper)
// /optimizer/strategy/:hash       → StrategyInspector (unchanged)
// legacy /autooptimizer/*         → unchanged lines, now pointing at ExperimentRedirect
```

- [ ] **Step 3: Delete dead files**, run `rg -l "LiveCycleView|ExperimentDetail|RunDetail" frontend/web/src` until only routes/tests you just rewrote remain. Also run the Task 13 Step 4 token grep over `screens/StrategyInspector.tsx` (it survives unchanged in role, but §5 visual language applies family-wide) — fix violations only, no layout changes. Verify the waiting strings are gone: `rg -i "waiting for (the )?(connection|cycle)" frontend/web/src` → no matches.

- [ ] **Step 4: Full verification**

```bash
cd frontend/web && npm run test          # full vitest suite
npm run build                            # tsc + vite build must pass
cd ../.. && scripts/cargo test -p xvision-dashboard
```
Expected: all PASS.

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(frontend): optimizer fold — redirects, delete LiveCycleView/ExperimentDetail/RunDetail"
```

---

### Task 16: Visual verification + PR

- [ ] **Step 1:** Run the app (per the `verify`/`run` flow for this repo: build the SPA, run the dashboard, or use the dev server `cd frontend/web && npm run dev` against a local backend) and screenshot `/optimizer` in idle, running (launch a cycle if a dev backend is available), and never-ran states, desktop + mobile viewport (set a tall viewport for long pages per the agent-browser memory note). Confirm: no waiting copy, and zero `createLinearGradient`/non-finite console warnings (spec §2.3 / audit F8+F15). The F8 guards already live in `frontend/web/src/components/chart/v2/adapters/uplot-plugins.ts`; the gradient-bearing ImprovementChart is deleted; EdgeVsRandomChart uses flat fills; the river is SVG with a NaN-path regression test. If ANY warning still appears, add the missing guard where it fires before shipping — this is a blocking check, not advisory.
- [ ] **Step 2:** Save before/after screenshots to `docs/design-audit/assets/desktop-optimizer-after-redesign.png` (+ mobile).
- [ ] **Step 3:** Push branch and open PR referencing the spec:

```bash
git push -u origin feat/optimizer-redesign
gh pr create --title "feat(optimizer): editorial mission-control redesign" --body "Implements docs/superpowers/specs/2026-06-11-optimizer-redesign-design.md …

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

---

## Self-review notes

- **Spec coverage:** fold (Tasks 14–15), home anatomy incl. best-find one-liner + paused state + honesty chips (6, 13), console module + replay (incl. pruned-events state) + never-ran (4–10), river incl. live frontier / fade-with-age / retired dim-out / live-end routing / empty-with-history state (11–12), redirects (15), waiting-states deletion (10, 15), full-artifact expansion incl. ParentDiffPanel + RegimeCards fold + writer model (7), panel token-restyle sweeps (13, 14), vocabulary (Experiment/writer/Rejected/Suspect/honesty check), tests (every task TDD).
- **Recorded spec deviations (surface in the PR description):** (1) backend scope follows the amended spec §8 (four additive items: event enrichment, event persistence, two read endpoints) — the original §7 single-endpoint limit was amended by the operator after the plan-review-gate escalation; (2) raw writer prompt / model response are not persisted by the backend, so the artifact expansion renders rationale + an honest "transcripts aren't persisted yet" footnote (§3 data gap); (3) the readout's "sparkline vs parent equity" is delivered as the per-regime ΔSharpe series via RegimeCards — per-experiment equity curves are not persisted; (4) digest tokens come from `CycleRunSummary.input_tokens/output_tokens` (omitted only if all-null); (5) river branch/node clicks pin the readout card and route via its "Open cycle →" action — direct click-routing would make the hover-populated *expandable* readout unusable; all three spec'd navigation targets (cycle?exp=, cycle, strategy via live end) are preserved and tested; (6) cycles run before event persistence shipped replay via the node-derived board fallback (no verbatim feed).
- **Known judgment calls for the executor:** selector fixtures now mirror `progress.rs` wire shapes verbatim (flattened, `type`-tagged, 3-way `outcome`, persisted kinds `mutation_gated_passed/_suspect/_dropped`) — verify `SSE_EVENT_NAMES` subscribes to the frame names `event_kind()` emits and that the live-frame JSON matches (Task 4 note); `GateScorecard`/`FindingsList`/`ParentDiffPanel`/`RegimeCards` prop signatures (Task 7); migration-057 DDL mirrored in Task 0b/1's test helper (verify against the actual migration file); `append_event` signature (events_store.rs:17); branch-line ordering in `buildRiverLayout` deep recursion (Task 11 note).
- **Type consistency check:** `Phase`/`BoardCard` (now with `writer`) defined in Task 5, consumed in Tasks 8/10/12; `RiverNode` defined in Task 3 (api.ts), consumed 11–12; `RiverStub.ageRank` defined Task 11, consumed Task 12; `PersistedCycleEvent` defined Task 3, normalized Task 4, consumed Task 10; `useOptimizerStatus` returns the response directly (not a query object) — Task 13 reflects this; `CycleRunSummary.last_created_at` used consistently (Tasks 10, 13); `defaultOpenHash` threading added in Task 14 where first needed; `hasHistory` prop defined Task 12, passed Task 13.
