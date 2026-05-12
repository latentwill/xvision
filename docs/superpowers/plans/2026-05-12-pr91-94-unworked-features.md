# PR 91-94 Unworked Features Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Finish the feature work left exposed by PRs 91-94: make PR 94 merge-ready, continue the agent-composition refactor beyond PR 93, and close the QA Pass 3 UI gaps around chat, setup, inspector, settings, and strategies.

**Architecture:** Treat PR 94 chart work as the stabilization base, then layer the strategy-agent composition API below the dashboard, and finally replace the old fixed-slot frontend surfaces with agent-backed workflows. The backend keeps legacy slot routes during the deprecation window, but all new UI should call agent-reference and pipeline endpoints.

**Tech Stack:** Rust workspace (`xvision-engine`, `xvision-dashboard`, `xvision-cli`), SQLite migrations, Axum routes, React + TypeScript + TanStack Query, Vite, `pnpm`, `cargo`, generated TS types through `cargo xtask gen-types`.

---

## Current State

- PR 91 was closed as redundant after PR 92.
- PR 92 is merged and closed QA Pass 2 default-LLM work.
- PR 93 is merged and shipped only Task 1 of `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md`.
- PR 94 is open on branch `worktree-scenario-eval-and-charts`; local branch is ahead of remote with scenario-capital fixes and QA Pass 3 spec.
- This shell currently lacks `cargo` and `pnpm`. Execution sessions should either run in an environment with the toolchains installed or install/use the project toolchain before verification.

## File Structure

- `crates/xvision-engine/src/authoring.rs` — add strategy-agent authoring functions and keep `update_slot` as deprecated compatibility.
- `crates/xvision-engine/src/api/strategy.rs` — expose agent-reference and pipeline API handlers.
- `crates/xvision-dashboard/src/routes/strategies.rs` — route new API endpoints and add deprecation headers to old slot routes.
- `crates/xvision-cli/src/commands/strategy.rs` — add `add-agent`, `remove-agent`, `set-pipeline`, and `migrate-agents`.
- `crates/xvision-engine/src/strategies/store.rs` — support migration writes and idempotent legacy-to-agent conversion.
- `crates/xvision-engine/src/agents/store.rs` — resolve existing agents during strategy migration.
- `frontend/web/src/api/strategies.ts` — add frontend calls for agent refs, pipeline updates, and migration-visible strategy shape.
- `frontend/web/src/routes/authoring.tsx` — replace fixed slot cards with strategy outline, agent editor, pipeline summary, and validation rail.
- `frontend/web/src/components/agent/AgentForm.tsx` — reuse for agent editing inside the inspector.
- `frontend/web/src/routes/setup.tsx` — make wizard strategy-only, render markdown, and show inline tool events as audit rows.
- `frontend/web/src/components/shell/ChatRail.tsx` — add session history drawer and make tool events first-class transcript rows.
- `frontend/web/src/routes/strategies.tsx` — remove dead filters or implement them; stop showing hardcoded validated status.
- `frontend/web/src/routes/settings/providers.tsx` and `frontend/web/src/components/shell/Sidebar.tsx` — hide onboarding chrome once providers are usable.
- `frontend/web/src/components/chart/LiveChart.tsx`, `frontend/web/src/components/chart/RunChart.tsx`, `frontend/web/src/components/chart/use-run-stream.ts` — finish PR 94 follow/freeze and manual-smoke defects.
- `docs/superpowers/specs/2026-05-12-qa-pass-3-spec.md` — source QA Pass 3 requirements.
- `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md` — source agent-composition Tasks 2-9.

---

## Task 1: Make PR 94 Merge-Ready

**Files:**
- Modify: `frontend/web/src/components/chart/LiveChart.tsx`
- Modify: `frontend/web/src/components/chart/RunChart.tsx`
- Modify: `frontend/web/src/components/chart/use-run-stream.ts`
- Verify: `frontend/web/src/routes/authoring.tsx`
- Verify: `frontend/web/src/routes/scenarios-new.tsx`
- Verify: `frontend/web/src/components/scenario/ScenarioForm.tsx`

- [ ] **Step 1: Confirm the branch is clean and ahead only by intended local commits**

Run:

```bash
git status --short --branch
git rev-list --left-right --count origin/worktree-scenario-eval-and-charts...HEAD
git log --oneline --decorate --max-count=12
```

Expected: clean worktree, `0 3` or an explicitly understood ahead count, and the latest local commits are scenario-capital / QA Pass 3 docs work.

- [ ] **Step 2: Re-run PR 94 cleanup scans**

Run:

```bash
rg -n "BundleStore|StrategyBundle|bundle\.capital|capital:|risk_caps:" crates frontend/web/src frontend/web/package.json frontend/web/pnpm-lock.yaml --glob '!**/dist/**' --glob '!**/node_modules/**'
```

Expected: no compile-relevant `BundleStore`, `StrategyBundle`, `bundle.capital`, or stale `risk_caps` hits. Documentation-only historical hits are acceptable if outside `crates` and `frontend/web/src`.

- [ ] **Step 3: Wire LiveChart follow/freeze into RunChart autoscroll**

In `frontend/web/src/components/chart/RunChart.tsx`, expose a `follow?: boolean` prop and use it when setting the chart time scale. The implementation should keep existing layer behavior intact:

```tsx
export function RunChart({
  payload,
  layers,
  onLayersChange,
  follow = false,
}: RunChartProps & { follow?: boolean }) {
  // existing setup...

  useEffect(() => {
    if (!chartRef.current || !follow) return;
    chartRef.current.timeScale().scrollToRealTime();
  }, [follow, payload.equity, payload.markers]);

  // existing render...
}
```

In `frontend/web/src/components/chart/LiveChart.tsx`, pass the local follow state:

```tsx
<RunChart
  payload={payload}
  layers={layers}
  onLayersChange={setLayers}
  follow={follow}
/>
```

- [ ] **Step 4: Add a focused frontend test for follow mode**

Modify `frontend/web/src/components/chart/LiveChart.test.tsx` or `RunChart.test.tsx` with a mocked `scrollToRealTime`:

```tsx
it("scrolls to real time when live follow mode is enabled", () => {
  const scrollToRealTime = vi.fn();
  mockTimeScale.mockReturnValue({ scrollToRealTime, fitContent: vi.fn() });

  render(<RunChart payload={samplePayload} follow />);

  expect(scrollToRealTime).toHaveBeenCalled();
});
```

- [ ] **Step 5: Verify chart and frontend checks**

Run:

```bash
cd frontend/web
pnpm test -- RunChart LiveChart
pnpm typecheck
pnpm build
```

Expected: chart tests pass, typecheck clean, build clean.

- [ ] **Step 6: Verify Rust workspace**

Run:

```bash
cargo test --workspace
```

Expected: all workspace tests pass.

- [ ] **Step 7: Push local PR 94 fixes**

Run:

```bash
git push origin HEAD:worktree-scenario-eval-and-charts
```

Expected: remote PR 94 head advances to the local merge-ready commit.

---

## Task 2: Strategy-Agent Authoring API

**Files:**
- Modify: `crates/xvision-engine/src/authoring.rs`
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Test: `crates/xvision-engine/tests/api_strategy.rs`

- [ ] **Step 1: Add failing engine tests for agent refs**

Add tests to `crates/xvision-engine/tests/api_strategy.rs`:

```rust
#[tokio::test]
async fn add_agent_ref_appends_role_and_audits() {
    let ctx = test_context().await;
    let strategy = create_sample_strategy(&ctx).await;
    let agent = create_sample_agent(&ctx, "Mean Rev Agent").await;

    let out = xvision_engine::api::strategy::add_agent(
        &ctx,
        xvision_engine::api::strategy::AddAgentReq {
            strategy_id: strategy.id.clone(),
            agent_id: agent.id.clone(),
            role: "trader".into(),
        },
    )
    .await
    .unwrap();

    assert_eq!(out.strategy_id, strategy.id);
    assert_eq!(out.agents.len(), 1);
    assert_eq!(out.agents[0].agent_id, agent.id);
    assert_eq!(out.agents[0].role, "trader");
    assert!(audit_row_exists(&ctx, "strategy_add_agent", &strategy.id).await);
}

#[tokio::test]
async fn set_pipeline_rejects_graph_edges_for_non_graph_kind() {
    let ctx = test_context().await;
    let strategy = create_sample_strategy(&ctx).await;

    let err = xvision_engine::api::strategy::set_pipeline(
        &ctx,
        xvision_engine::api::strategy::SetPipelineReq {
            strategy_id: strategy.id.clone(),
            kind: xvision_engine::strategies::PipelineKind::Single,
            edges: vec![xvision_engine::strategies::PipelineEdge {
                from_role: "a".into(),
                to_role: "b".into(),
            }],
        },
    )
    .await
    .unwrap_err();

    assert_eq!(err.code(), "validation");
}
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p xvision-engine --test api_strategy add_agent_ref_appends_role_and_audits set_pipeline_rejects_graph_edges_for_non_graph_kind
```

Expected: FAIL because `add_agent`, `set_pipeline`, and request/response types do not exist yet.

- [ ] **Step 3: Implement authoring functions**

In `crates/xvision-engine/src/authoring.rs`, add:

```rust
pub struct AddAgentRefRequest {
    pub strategy_id: String,
    pub agent_id: String,
    pub role: String,
}

pub struct RemoveAgentRefRequest {
    pub strategy_id: String,
    pub role: String,
}

pub struct SetPipelineRequest {
    pub strategy_id: String,
    pub pipeline: PipelineDef,
}

pub async fn add_agent_ref(
    store: &impl StrategyStore,
    req: AddAgentRefRequest,
) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    if req.role.trim().is_empty() {
        anyhow::bail!("role is required");
    }
    if strategy.agents.iter().any(|a| a.role == req.role) {
        anyhow::bail!("role '{}' already exists on strategy", req.role);
    }
    strategy.agents.push(AgentRef {
        agent_id: req.agent_id,
        role: req.role,
    });
    if strategy.pipeline.kind == PipelineKind::Single && strategy.agents.len() > 1 {
        strategy.pipeline.kind = PipelineKind::Sequential;
    }
    store.save(&strategy).await?;
    Ok(strategy)
}

pub async fn remove_agent_ref(
    store: &impl StrategyStore,
    req: RemoveAgentRefRequest,
) -> anyhow::Result<Strategy> {
    let mut strategy = store.load(&req.strategy_id).await?;
    let before = strategy.agents.len();
    strategy.agents.retain(|a| a.role != req.role);
    if strategy.agents.len() == before {
        anyhow::bail!("role '{}' not found on strategy", req.role);
    }
    if strategy.agents.len() <= 1 {
        strategy.pipeline = PipelineDef::default();
    }
    store.save(&strategy).await?;
    Ok(strategy)
}

pub async fn set_pipeline(
    store: &impl StrategyStore,
    req: SetPipelineRequest,
) -> anyhow::Result<Strategy> {
    if req.pipeline.kind != PipelineKind::Graph && !req.pipeline.edges.is_empty() {
        anyhow::bail!("pipeline edges are only valid for graph pipelines");
    }
    let mut strategy = store.load(&req.strategy_id).await?;
    strategy.pipeline = req.pipeline;
    store.save(&strategy).await?;
    Ok(strategy)
}
```

Adjust imports to use `StrategyStore`, `AgentRef`, `PipelineDef`, and `PipelineKind` from the existing `strategies` module.

- [ ] **Step 4: Add API request/response wrappers**

In `crates/xvision-engine/src/api/strategy.rs`, add `ts-rs`-compatible structs and handlers:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct AddAgentReq {
    pub strategy_id: String,
    pub agent_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct RemoveAgentReq {
    pub strategy_id: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SetPipelineReq {
    pub strategy_id: String,
    pub kind: PipelineKind,
    pub edges: Vec<PipelineEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct StrategyAgentsOut {
    pub strategy_id: String,
    pub agents: Vec<AgentRef>,
    pub pipeline: PipelineDef,
}
```

Handlers should call `authoring::add_agent_ref`, `authoring::remove_agent_ref`, and `authoring::set_pipeline`, then write audit actions named `strategy_add_agent`, `strategy_remove_agent`, and `strategy_set_pipeline`.

- [ ] **Step 5: Run focused tests**

Run:

```bash
cargo test -p xvision-engine --test api_strategy add_agent_ref_appends_role_and_audits set_pipeline_rejects_graph_edges_for_non_graph_kind
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/xvision-engine/src/authoring.rs crates/xvision-engine/src/api/strategy.rs crates/xvision-engine/tests/api_strategy.rs
git commit -m "feat(strategy): add agent reference authoring API"
```

---

## Task 3: Dashboard Routes for Strategy Agents

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Test: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 1: Add failing route tests**

Add dashboard integration tests:

```rust
#[tokio::test]
async fn strategy_add_agent_route_returns_updated_refs() {
    let app = test_app().await;
    let strategy = create_strategy_via_http(&app).await;
    let agent = create_agent_via_http(&app, "route-agent").await;

    let res = app
        .post_json(
            &format!("/api/strategies/{}/agents", strategy.id),
            serde_json::json!({ "agent_id": agent.id, "role": "trader" }),
        )
        .await;

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await;
    assert_eq!(body["agents"][0]["agent_id"], agent.id);
    assert_eq!(body["agents"][0]["role"], "trader");
}

#[tokio::test]
async fn legacy_slot_route_marks_deprecated() {
    let app = test_app().await;
    let strategy = create_strategy_via_http(&app).await;

    let res = app
        .put_json(
            &format!("/api/strategies/{}/slot/trader", strategy.id),
            serde_json::json!({ "prompt": "trade carefully" }),
        )
        .await;

    assert_eq!(res.headers().get("x-deprecated").unwrap(), "true");
}
```

- [ ] **Step 2: Run tests and confirm failure**

Run:

```bash
cargo test -p xvision-dashboard strategy_add_agent_route_returns_updated_refs legacy_slot_route_marks_deprecated
```

Expected: FAIL because routes and headers are not wired.

- [ ] **Step 3: Add route handlers**

In `crates/xvision-dashboard/src/routes/strategies.rs`, add handlers:

```rust
#[derive(Debug, Deserialize)]
pub struct AddAgentBody {
    pub agent_id: String,
    pub role: String,
}

pub async fn add_agent(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(body): Json<AddAgentBody>,
) -> Result<Json<StrategyAgentsOut>, DashboardError> {
    let out = strategy::add_agent(
        &state.api_context(),
        AddAgentReq {
            strategy_id: id,
            agent_id: body.agent_id,
            role: body.role,
        },
    )
    .await?;
    Ok(Json(out))
}
```

Add equivalent `remove_agent` and `set_pipeline` handlers.

- [ ] **Step 4: Register routes**

In `crates/xvision-dashboard/src/server.rs`, add:

```rust
.route("/api/strategies/:id/agents", post(strategies::add_agent))
.route(
    "/api/strategies/:id/agents/:role",
    delete(strategies::remove_agent),
)
.route(
    "/api/strategies/:id/pipeline",
    put(strategies::set_pipeline),
)
```

Update the existing slot route handler to attach `X-Deprecated: true` to responses.

- [ ] **Step 5: Run dashboard tests**

Run:

```bash
cargo test -p xvision-dashboard strategy_add_agent_route_returns_updated_refs legacy_slot_route_marks_deprecated
```

Expected: PASS.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/xvision-dashboard/src/routes/strategies.rs crates/xvision-dashboard/src/server.rs crates/xvision-dashboard/tests/http.rs
git commit -m "feat(dashboard): expose strategy agent routes"
```

---

## Task 4: Strategy Migration CLI

**Files:**
- Modify: `crates/xvision-engine/src/strategies/store.rs`
- Modify: `crates/xvision-cli/src/commands/strategy.rs`
- Test: `crates/xvision-engine/tests/bundle_store.rs`

- [ ] **Step 1: Add failing idempotent migration test**

Add:

```rust
#[tokio::test]
async fn migrate_legacy_slots_to_agent_refs_is_idempotent() {
    let temp = tempfile::tempdir().unwrap();
    let store = FilesystemStore::open(temp.path()).await.unwrap();
    let legacy = sample_legacy_slot_strategy("legacy-one");
    store.save(&legacy).await.unwrap();

    let first = store.migrate_legacy_slots_to_agent_refs(true).await.unwrap();
    assert_eq!(first.changed, 1);

    let second = store.migrate_legacy_slots_to_agent_refs(true).await.unwrap();
    assert_eq!(second.changed, 0);

    let loaded = store.load("legacy-one").await.unwrap();
    assert!(!loaded.agents.is_empty());
    assert_eq!(loaded.pipeline.kind, PipelineKind::Sequential);
}
```

- [ ] **Step 2: Run test and confirm failure**

Run:

```bash
cargo test -p xvision-engine --test bundle_store migrate_legacy_slots_to_agent_refs_is_idempotent
```

Expected: FAIL because migration helper does not exist.

- [ ] **Step 3: Implement migration report and store helper**

In `crates/xvision-engine/src/strategies/store.rs`, add:

```rust
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyMigrationReport {
    pub scanned: usize,
    pub changed: usize,
    pub unchanged: usize,
    pub strategy_ids: Vec<String>,
}
```

Add `FilesystemStore::migrate_legacy_slots_to_agent_refs(dry_run: bool)` that:

- loads every strategy JSON,
- skips strategies where `agents` is non-empty,
- creates one `AgentRef` per populated legacy slot with the same role,
- sets `PipelineKind::Sequential` if more than one ref exists,
- writes only when `dry_run == false`,
- returns the report.

- [ ] **Step 4: Add CLI command**

In `crates/xvision-cli/src/commands/strategy.rs`, add:

```rust
#[derive(Debug, Args)]
pub struct MigrateAgentsArgs {
    #[arg(long)]
    pub dry_run: bool,
}
```

Wire `xvn strategy migrate-agents [--dry-run]` to call the store helper and print:

```text
scanned=<n> changed=<n> unchanged=<n>
changed strategy ids:
- <id>
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test -p xvision-engine --test bundle_store migrate_legacy_slots_to_agent_refs_is_idempotent
cargo build -p xvision-cli
```

Expected: PASS and CLI builds.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/xvision-engine/src/strategies/store.rs crates/xvision-engine/tests/bundle_store.rs crates/xvision-cli/src/commands/strategy.rs
git commit -m "feat(strategy): add legacy slot migration command"
```

---

## Task 5: Rebuild Strategy Inspector Around Agents

**Files:**
- Modify: `frontend/web/src/api/strategies.ts`
- Modify: `frontend/web/src/routes/authoring.tsx`
- Reuse: `frontend/web/src/components/agent/AgentForm.tsx`

- [ ] **Step 1: Add frontend API methods**

In `frontend/web/src/api/strategies.ts`, add:

```ts
export type AddStrategyAgentInput = {
  agent_id: string;
  role: string;
};

export type StrategyAgentsOut = {
  strategy_id: string;
  agents: Array<{ agent_id: string; role: string }>;
  pipeline: {
    kind: "single" | "sequential" | "graph";
    edges: Array<{ from_role: string; to_role: string }>;
  };
};

export function addStrategyAgent(
  strategyId: string,
  input: AddStrategyAgentInput,
) {
  return apiFetch<StrategyAgentsOut>(
    `/api/strategies/${encodeURIComponent(strategyId)}/agents`,
    { method: "POST", body: JSON.stringify(input) },
  );
}

export function removeStrategyAgent(strategyId: string, role: string) {
  return apiFetch<StrategyAgentsOut>(
    `/api/strategies/${encodeURIComponent(strategyId)}/agents/${encodeURIComponent(role)}`,
    { method: "DELETE" },
  );
}
```

- [ ] **Step 2: Replace fixed slot rendering**

In `frontend/web/src/routes/authoring.tsx`, remove `SLOT_ROLES.map(...)` from `BundleEditor` and render:

```tsx
function StrategyComposition({ strategy }: { strategy: Strategy }) {
  return (
    <Card>
      <SectionHeader label="Agents" hint="Reusable agents assigned to this strategy." />
      <div className="px-5 pb-5 space-y-2">
        {strategy.agents.length === 0 ? (
          <p className="m-0 text-[13px] text-text-3">
            No agents assigned yet. Add a trader agent to make this strategy runnable.
          </p>
        ) : (
          strategy.agents.map((ref) => (
            <button
              key={ref.role}
              className="w-full flex items-center justify-between px-3 py-2 border border-border-soft rounded text-left"
              onClick={() => setSelectedRole(ref.role)}
            >
              <span className="font-mono text-[13px]">{ref.role}</span>
              <span className="text-text-3 text-[12px]">{ref.agent_id}</span>
            </button>
          ))
        )}
      </div>
    </Card>
  );
}
```

- [ ] **Step 3: Reuse AgentForm for selected agent**

When a role is selected, load the referenced agent and render `AgentForm`. Show this warning when editing an agent record:

```tsx
<div className="border border-amber-500/40 bg-amber-500/10 text-amber-200 px-3 py-2 rounded text-[12px]">
  Editing this agent changes every strategy that references it.
</div>
```

- [ ] **Step 4: Remove fake model requirement UI**

Delete the user-facing `Model requirement` label and free-text constraint input from the inspector. Provider/model editing should happen through `AgentForm` slot fields.

- [ ] **Step 5: Verify frontend**

Run:

```bash
cd frontend/web
pnpm typecheck
pnpm build
```

Expected: clean.

- [ ] **Step 6: Commit**

Run:

```bash
git add frontend/web/src/api/strategies.ts frontend/web/src/routes/authoring.tsx
git commit -m "feat(inspector): edit strategy agent composition"
```

---

## Task 6: QA Pass 3 Chat Rail and Setup Wizard

**Files:**
- Modify: `frontend/web/src/components/shell/ChatRail.tsx`
- Modify: `frontend/web/src/routes/setup.tsx`
- Modify: `frontend/web/src/api/chat_rail.ts`
- Modify: `crates/xvision-dashboard/src/wizard_loop.rs`

- [ ] **Step 1: Make setup wizard markdown-capable**

Copy the `MarkdownView` helper pattern from `ChatRail.tsx` into a shared component:

```tsx
// frontend/web/src/components/chat/MarkdownView.tsx
export function MarkdownView({ text }: { text: string }) {
  return (
    <ReactMarkdown remarkPlugins={[remarkGfm]}>
      {text}
    </ReactMarkdown>
  );
}
```

Use it in both `ChatRail.tsx` and `setup.tsx`.

- [ ] **Step 2: Make tool calls transcript rows, not only pills**

Add a `ToolEventRow` component:

```tsx
function ToolEventRow({ tool }: { tool: Tool }) {
  return (
    <div className="border border-border-soft bg-surface-2/40 rounded px-2.5 py-2 text-[12px]">
      <div className="font-mono text-text">{tool.call}</div>
      {tool.summary ? <div className="text-text-3 mt-1">{tool.summary}</div> : null}
      {tool.pending ? <div className="text-text-3 mt-1">running…</div> : null}
    </div>
  );
}
```

Render tool events directly below the assistant text in chronological order in both chat surfaces.

- [ ] **Step 3: Restrict setup wizard copy to strategy-only**

In `setup.tsx`, replace broad wording with:

```tsx
The setup agent builds strategy drafts only. Use the chat rail for workspace operations, eval control, and broader xvn commands.
```

Do not mention eval execution as a wizard capability unless the wizard route actually starts evals.

- [ ] **Step 4: Add chat rail history drawer**

Expose a `GET /api/chat-rail/sessions` route returning session metadata:

```rust
pub struct ChatSessionSummary {
    pub session_id: String,
    pub label: String,
    pub scope: ContextScope,
    pub updated_at: DateTime<Utc>,
}
```

In `ChatRail.tsx`, add a `History` button in the header that toggles a drawer listing summaries and calls `history(session_id)` when selected.

- [ ] **Step 5: Verify**

Run:

```bash
cargo test -p xvision-dashboard chat_rail
cd frontend/web
pnpm typecheck
pnpm build
```

Expected: dashboard chat-rail tests pass, frontend clean.

- [ ] **Step 6: Commit**

Run:

```bash
git add crates/xvision-dashboard/src/wizard_loop.rs crates/xvision-dashboard/src/routes/chat_rail.rs frontend/web/src/components/shell/ChatRail.tsx frontend/web/src/routes/setup.tsx frontend/web/src/components/chat/MarkdownView.tsx
git commit -m "feat(chat): make rail and wizard transcript-first"
```

---

## Task 7: Settings and Strategies Cleanup

**Files:**
- Modify: `frontend/web/src/routes/strategies.tsx`
- Modify: `frontend/web/src/components/shell/Sidebar.tsx`
- Modify: `frontend/web/src/routes/settings/providers.tsx`

- [ ] **Step 1: Remove disabled filters or make them real**

For the QA Pass 3 scope, remove disabled filter controls from `FilterBar` unless fully implemented. Keep only working actions:

```tsx
function FilterBar() {
  return (
    <div className="flex items-center justify-end mb-4 gap-2">
      <Link to="/strategies/new" className="...">
        <Icon name="plus" size={13} /> New from template
      </Link>
      <Link to="/setup" className="...">
        <Icon name="plus" size={13} /> New strategy
      </Link>
    </div>
  );
}
```

- [ ] **Step 2: Stop hardcoding validated status**

Change the status cell to render unknown state honestly:

```tsx
<td className="py-3 px-3">
  {row.status ? (
    <Pill tone={row.status === "validated" ? "gold" : "info"}>{row.status}</Pill>
  ) : (
    <span className="text-text-3 text-[12px]">not validated</span>
  )}
</td>
```

If `StrategySummary` has no status field yet, omit the column entirely.

- [ ] **Step 3: Hide sidebar onboarding when providers are configured**

Use providers report data:

```tsx
const hasUsableProvider = providers.data?.providers.some(
  (p) => p.api_key_set && !p.synthetic && p.enabled_models.length > 0,
);
```

Render “Add LLM key” only when `hasUsableProvider === false`.

- [ ] **Step 4: Verify**

Run:

```bash
cd frontend/web
pnpm typecheck
pnpm build
```

Expected: clean.

- [ ] **Step 5: Commit**

Run:

```bash
git add frontend/web/src/routes/strategies.tsx frontend/web/src/components/shell/Sidebar.tsx frontend/web/src/routes/settings/providers.tsx
git commit -m "fix(ui): remove dead strategies and onboarding controls"
```

---

## Task 8: Final Verification and PR Updates

**Files:**
- Verify: whole workspace
- Update: PR 94 body/comment if branch remains PR 94

- [ ] **Step 1: Run full verification**

Run:

```bash
cargo test --workspace
cd frontend/web
pnpm test
pnpm typecheck
pnpm build
```

Expected: all pass.

- [ ] **Step 2: Regenerate TS types if Rust API types changed**

Run:

```bash
cargo xtask gen-types
cd frontend/web
pnpm typecheck
```

Expected: generated type diffs only where API structs changed; typecheck clean.

- [ ] **Step 3: Manual smoke**

Run the dashboard and verify:

```bash
cargo run -p xvision-cli -- dashboard serve --bind 127.0.0.1:8788
```

Smoke paths:

- `/strategies` shows no disabled filters and no fake validated status.
- `/strategies/new` creates from a template and opens `/authoring/:id`.
- `/authoring/:id` shows agent composition, not fixed Regime/Intern/Trader cards.
- `/setup` renders markdown and only describes strategy authoring.
- Chat rail shows inline tool rows and can browse session history.
- `/live/<run_id>` follows live updates when follow is enabled and stops autoscrolling when frozen.
- `/scenarios/new` preview still renders with capital in request payload.

- [ ] **Step 4: Push and update PR**

Run:

```bash
git push origin HEAD:worktree-scenario-eval-and-charts
```

Then update PR 94 with:

- merge conflicts resolved,
- local-only cleanup commits pushed,
- verification command results,
- remaining manual smoke caveats if any.

---

## Self-Review

**Spec coverage:** This plan covers PR 94 stabilization, the known remaining Tasks 2-9 from the agent-composition plan, and all QA Pass 3 batches at implementation granularity.

**Placeholder scan:** No task says “TBD”, “TODO”, “similar to”, or “write tests” without naming a concrete file and command. Some code blocks are illustrative integration snippets and must be reconciled with exact local test helpers during execution.

**Type consistency:** New strategy-agent concepts consistently use `AgentRef`, `PipelineDef`, `PipelineKind`, `StrategyStore`, and route names under `/api/strategies/:id/agents` plus `/api/strategies/:id/pipeline`.
