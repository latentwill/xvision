# QA Pass 4 Surface Consistency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the dashboard, strategy creation flows, strategy list, Inspector, and Eval surfaces agree on persisted product state and use consistent user-facing terminology.

**Architecture:** Fix the product at the real source-of-truth seams instead of only patching labels. The critical backend change is to route wizard mutations through the same audited/indexed strategy API path used by `/api/strategies`, then layer frontend refresh, terminology, cadence formatting, and full risk editing on top. Eval visibility stays on the existing async run model and focuses on the launcher and query invalidation path rather than redesigning the runs table.

**Tech Stack:** Rust workspace (`xvision-engine`, `xvision-dashboard`), Axum routes, filesystem-backed strategy store plus SQLite-backed dashboard state, React 18 + TypeScript + TanStack Query, Vite, Vitest, `cargo`, `pnpm`.

---

## File Structure

- `crates/xvision-dashboard/src/wizard_loop.rs` — stop bypassing the API layer; wizard tool execution should call the same audited/indexed strategy mutation functions as the public dashboard API.
- `crates/xvision-engine/src/api/strategy.rs` — enrich strategy summaries for list surfaces and keep post-mutation indexing centralized.
- `crates/xvision-dashboard/tests/http.rs` — lock in wizard/create/list and summary-shape behavior with real tempdir-backed state.
- `frontend/web/src/routes/home.tsx` — rename the route to `Dashboard` and remove obsolete status chrome.
- `frontend/web/src/routes/strategies.tsx` — fix user-facing naming, real status behavior, and new summary fields.
- `frontend/web/src/routes/authoring.tsx` — replace preset-only risk controls with editable persisted fields and turn “Run eval” into a real product launcher.
- `frontend/web/src/routes/eval-runs.tsx` — accept preselected strategy state and keep list refresh aligned with launched runs.
- `frontend/web/src/api/strategies.ts` and `frontend/web/src/api/eval.ts` — align request/response types with the updated dashboard and strategy APIs.
- `frontend/web/src/lib/format.ts` — add one shared cadence/timeframe formatter instead of per-route ad hoc minute formatting.
- `frontend/web/src/components/shell/CommandPalette.tsx`, `README.md`, `frontend/README.md` — update route naming from `Control Tower` to `Dashboard` where the shipped product documents it.

## Task 1: Unify Wizard Writes with the Public Strategy API

**Files:**
- Modify: `crates/xvision-dashboard/src/wizard_loop.rs`
- Modify: `crates/xvision-engine/src/api/strategy.rs`
- Test: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 1: Write the failing dashboard integration test**

Add a test to `crates/xvision-dashboard/tests/http.rs` that proves a strategy created by the wizard path lands in the public strategy list without any secondary bootstrap logic:

```rust
#[tokio::test]
async fn wizard_created_strategy_is_visible_in_public_strategies_list() {
    let (server, tmp) = boot().await;

    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}/xvn.db",
        tmp.path().display()
    ))
    .await
    .unwrap();

    let state = xvision_dashboard::AppState::new(tmp.path().to_path_buf())
        .await
        .expect("init app state");
    let ctx = state.api_context();
    let out = xvision_engine::api::strategy::create_strategy(
        &ctx,
        xvision_engine::authoring::CreateStrategyReq {
            template: "mean_reversion".into(),
            name: "Wizard Visible".into(),
            creator: Some("@wizard".into()),
        },
    )
    .await
    .unwrap();

    let response = server.get("/api/strategies").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let items = body["items"].as_array().unwrap();
    let created = items
        .iter()
        .find(|item| item["agent_id"] == out.id)
        .expect("created strategy present in list");

    assert_eq!(created["display_name"], "Wizard Visible");
}
```

- [ ] **Step 2: Run the focused dashboard test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-dashboard wizard_created_strategy_is_visible_in_public_strategies_list -- --nocapture
```

Expected: FAIL because `StrategySummary` does not yet expose `display_name`, and the wizard loop still bypasses the API mutation path the list/indexing surfaces use.

- [ ] **Step 3: Expand `StrategySummary` to carry user-facing fields**

In `crates/xvision-engine/src/api/strategy.rs`, extend the summary type so the strategies page can render real user-facing metadata without loading each strategy detail separately:

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategySummary {
    pub agent_id: String,
    pub display_name: String,
    pub template: String,
    pub decision_cadence_minutes: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}
```

Populate it from the loaded bundle:

```rust
out.push(StrategySummary {
    agent_id: bundle.manifest.id.clone(),
    display_name: bundle.manifest.display_name.clone(),
    template: bundle.manifest.template.clone(),
    decision_cadence_minutes: bundle.manifest.decision_cadence_minutes,
    model,
});
```

- [ ] **Step 4: Route wizard tool execution through the API mutation layer**

Refactor `crates/xvision-dashboard/src/wizard_loop.rs` so `run_tool` uses a real `ApiContext` for the mutating authoring verbs instead of calling the raw filesystem-store dispatcher directly.

Add an `api_context` field on `WizardLoop` and construct it once:

```rust
use xvision_engine::api::{Actor, ApiContext};

pub struct WizardLoop {
    api_context: ApiContext,
    // existing fields...
}

let api_context = ApiContext::new(
    pool.clone(),
    Actor::Cli { user: "wizard".to_string() },
    xvn_home.clone(),
);
```

For create/get/update/risk/validate, call the API wrappers:

```rust
let out = xvision_engine::api::strategy::create_strategy(&self.api_context, req).await?;
let out = xvision_engine::api::strategy::get(&self.api_context, id).await?;
let out = xvision_engine::api::strategy::update_slot(&self.api_context, req).await?;
let out = xvision_engine::api::strategy::set_risk_config(&self.api_context, req).await?;
let out = xvision_engine::api::strategy::validate_draft(&self.api_context, id).await?;
```

Keep `list_templates` and `set_mechanical_param` on the direct authoring path unless you also add an API wrapper for the mechanical params mutation in the same change.

- [ ] **Step 5: Add an audited API wrapper for mechanical params**

Because the wizard uses `set_mechanical_param`, add a matching API wrapper so all wizard-side strategy writes share the same audit/indexing behavior:

```rust
pub async fn set_mechanical_param(
    ctx: &ApiContext,
    req: authoring::SetMechanicalParamReq,
) -> ApiResult<serde_json::Value> {
    let started = Instant::now();
    let agent_id = req.id.clone();
    let args_json = serde_json::to_string(&req).ok();
    let store = FilesystemStore::new(strategy_store_dir(&ctx.xvn_home));
    let result = authoring::set_mechanical_param(&store, req)
        .await
        .map(|_| serde_json::json!({ "ok": true }))
        .map_err(|e| map_authoring_error(e, Some(&agent_id)));

    let outcome = match &result {
        Ok(_) => Outcome::Ok,
        Err(e) => Outcome::Error(e.to_string()),
    };
    let _ = audit::record(
        ctx,
        "strategy",
        "set_mechanical_param",
        Some(&agent_id),
        args_json.as_deref(),
        outcome,
        started.elapsed().as_millis() as i64,
    )
    .await;
    if result.is_ok() {
        index_strategy_after_mutation(ctx, &store, &agent_id).await;
    }
    result
}
```

Then update the wizard loop tool handler:

```rust
let req: authoring::SetMechanicalParamReq = serde_json::from_value(input)?;
let out = xvision_engine::api::strategy::set_mechanical_param(&self.api_context, req).await?;
Ok(out)
```

- [ ] **Step 6: Re-run the focused dashboard test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-dashboard wizard_created_strategy_is_visible_in_public_strategies_list -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-dashboard/src/wizard_loop.rs crates/xvision-engine/src/api/strategy.rs crates/xvision-dashboard/tests/http.rs
git commit -m "fix(wizard): persist strategy mutations through audited api path"
```

## Task 2: Rename Dashboard Home and Remove Dead Chrome

**Files:**
- Modify: `frontend/web/src/routes/home.tsx`
- Modify: `frontend/web/src/components/shell/CommandPalette.tsx`
- Modify: `README.md`
- Modify: `frontend/README.md`
- Test: `frontend/web/src/routes/home.test.tsx`

- [ ] **Step 1: Write the failing frontend test**

Create `frontend/web/src/routes/home.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { HomeRoute } from "./home";
import * as healthApi from "@/api/health";
import * as evalApi from "@/api/eval";
import * as strategyApi from "@/api/strategies";
import * as agentApi from "@/api/agents";
import * as settingsApi from "@/api/settings";

vi.mock("@/api/health");
vi.mock("@/api/eval");
vi.mock("@/api/strategies");
vi.mock("@/api/agents");
vi.mock("@/api/settings");

it("renders Dashboard and hides on-chain identity copy", async () => {
  vi.mocked(healthApi.getHealth).mockResolvedValue({ status: "ok", probes: [] } as any);
  vi.mocked(evalApi.listRuns).mockResolvedValue([]);
  vi.mocked(strategyApi.listStrategies).mockResolvedValue([]);
  vi.mocked(agentApi.listAgents).mockResolvedValue([]);
  vi.mocked(settingsApi.listProviders).mockResolvedValue({ providers: [] } as any);
  vi.mocked(settingsApi.getBrokers).mockResolvedValue({} as any);
  vi.mocked(settingsApi.getIdentity).mockResolvedValue({ feature_compiled_in: false } as any);

  render(
    <MemoryRouter>
      <QueryClientProvider client={new QueryClient()}>
        <HomeRoute />
      </QueryClientProvider>
    </MemoryRouter>
  );

  expect(await screen.findByText("Dashboard")).toBeInTheDocument();
  expect(screen.queryByText(/On-chain identity/i)).not.toBeInTheDocument();
});
```

- [ ] **Step 2: Run the focused frontend test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- home.test.tsx
```

Expected: FAIL because the page still renders `Control Tower` and the `On-chain identity` card.

- [ ] **Step 3: Implement the home-route cleanup**

In `frontend/web/src/routes/home.tsx`, change the route title/subtitle and delete the identity card block:

```tsx
<Topbar
  title="Dashboard"
  sub="paper · localhost · workspace status at a glance"
/>
```

Remove the entire card that begins with:

```tsx
<div className="text-text-3 text-[11px] uppercase tracking-wider mb-2">
  On-chain identity
</div>
```

If the old local-health probe panel exists in the route version you are editing, delete that card as part of the same task rather than leaving a parallel diagnostics section behind.

- [ ] **Step 4: Rename shipped product references**

Update the user-facing shipped-product docs and command palette copy:

```tsx
{ kind: "action", artifact_id: "nav:home", title: "Home", summary: "Dashboard", tags: ["nav"], href: "/", updated_at: "", bm25_score: 0 }
```

In `README.md` and `frontend/README.md`, replace the shipped route description:

```md
V1 routes: `/` Dashboard, `/setup` Wizard, `/strategies`, `/authoring/:id` ...
```

- [ ] **Step 5: Re-run the frontend test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- home.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/routes/home.tsx frontend/web/src/components/shell/CommandPalette.tsx frontend/web/src/routes/home.test.tsx README.md frontend/README.md
git commit -m "fix(home): rename dashboard and remove obsolete status chrome"
```

## Task 3: Strategies Page Naming, Display Name, and Shared Cadence Formatting

**Files:**
- Modify: `frontend/web/src/api/strategies.ts`
- Modify: `frontend/web/src/lib/format.ts`
- Modify: `frontend/web/src/routes/strategies.tsx`
- Test: `frontend/web/src/routes/strategies.test.tsx`

- [ ] **Step 1: Write the failing strategies-route test**

Create `frontend/web/src/routes/strategies.test.tsx`:

```tsx
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { StrategiesRoute } from "./strategies";
import * as strategiesApi from "@/api/strategies";

vi.mock("@/api/strategies");

it("renders Strategy ID and display name with a humanized cadence", async () => {
  vi.mocked(strategiesApi.listStrategies).mockResolvedValue([
    {
      agent_id: "01TEST",
      display_name: "Trend 4H",
      template: "trend_follower",
      decision_cadence_minutes: 240,
      model: "claude-sonnet"
    } as any,
  ]);

  render(
    <MemoryRouter>
      <QueryClientProvider client={new QueryClient()}>
        <StrategiesRoute />
      </QueryClientProvider>
    </MemoryRouter>
  );

  expect(await screen.findByText("Strategy ID")).toBeInTheDocument();
  expect(screen.getByText("Trend 4H")).toBeInTheDocument();
  expect(screen.getByText("4h")).toBeInTheDocument();
});
```

- [ ] **Step 2: Run the focused test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- strategies.test.tsx
```

Expected: FAIL because the table still uses `Agent ID` and does not render `display_name` or a shared cadence formatter.

- [ ] **Step 3: Add a shared cadence formatter**

In `frontend/web/src/lib/format.ts`, add a single helper and use it everywhere user-facing cadence is shown:

```ts
export function formatCadence(minutes: number): string {
  if (minutes % 1440 === 0) return `${minutes / 1440}d`;
  if (minutes % 60 === 0) return `${minutes / 60}h`;
  return `${minutes}m`;
}
```

- [ ] **Step 4: Extend the frontend strategy types**

In `frontend/web/src/api/strategies.ts`, align the list response with the enriched backend shape:

```ts
export type StrategyListItem = {
  agent_id: string;
  display_name: string;
  template: string;
  decision_cadence_minutes: number;
  model?: string;
};

export function listStrategies(): Promise<StrategyListItem[]> {
  return apiFetch<StrategiesListResponse>("/api/strategies").then((r) => r.items);
}
```

- [ ] **Step 5: Implement the strategies page changes**

In `frontend/web/src/routes/strategies.tsx`:

```tsx
import { formatCadence } from "@/lib/format";

<th className="font-normal py-2.5 px-5">Strategy ID</th>
<th className="font-normal py-2.5 px-3">Name</th>
<th className="font-normal py-2.5 px-3">Cadence</th>
```

Render the list rows from real summary fields:

```tsx
<td className="py-3 px-3 text-text">{row.display_name}</td>
<td className="py-3 px-3 font-mono text-text-2 text-[12px]">
  {formatCadence(row.decision_cadence_minutes)}
</td>
```

Replace the hardcoded validated status pill with a neutral persisted-state label until real status exists:

```tsx
<Pill>
  <span className="w-1.5 h-1.5 rounded-full bg-text-3" /> draft
</Pill>
```

- [ ] **Step 6: Re-run the focused test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- strategies.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/api/strategies.ts frontend/web/src/lib/format.ts frontend/web/src/routes/strategies.tsx frontend/web/src/routes/strategies.test.tsx
git commit -m "fix(strategies): show strategy names and shared cadence formatting"
```

## Task 4: Replace the Inspector Eval Stub with a Real Launch Flow

**Files:**
- Modify: `frontend/web/src/routes/authoring.tsx`
- Modify: `frontend/web/src/routes/eval-runs.tsx`
- Test: `frontend/web/src/routes/eval-runs.test.tsx`

- [ ] **Step 1: Write the failing start-eval preselection test**

Add to `frontend/web/src/routes/eval-runs.test.tsx`:

```tsx
it("preselects strategy from the query string in the start eval dialog", async () => {
  vi.mocked(evalApi.listRuns).mockResolvedValue([]);
  vi.mocked(evalApi.listScenarios).mockResolvedValue([
    { id: "crypto-bull-q1-2025", display_name: "Bull", asset_universe: [], regime_tags: [], time_window_days: 90 },
  ] as any);
  vi.mocked(strategyApi.listStrategies).mockResolvedValue([
    { agent_id: "01TEST", display_name: "Trend 4H", template: "trend_follower", decision_cadence_minutes: 240 } as any,
  ]);

  render(
    <MemoryRouter initialEntries={["/eval-runs?strategy=01TEST"]}>
      <QueryClientProvider client={new QueryClient()}>
        <EvalRunsRoute />
      </QueryClientProvider>
    </MemoryRouter>
  );

  expect(await screen.findByLabelText("Strategy")).toHaveValue("01TEST");
});
```

- [ ] **Step 2: Run the focused test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- eval-runs.test.tsx
```

Expected: FAIL because the start dialog does not yet consume the `?strategy=` query string.

- [ ] **Step 3: Make the Inspector CTA open a real launch path**

In `frontend/web/src/routes/authoring.tsx`, replace the copy/paste CLI card with a link into the existing start-eval modal flow:

```tsx
<Link
  to={`/eval-runs?strategy=${encodeURIComponent(agentId)}&start=1`}
  className="inline-flex items-center gap-2 px-3 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
>
  Run eval →
</Link>
```

Keep a small optional CLI helper block only as secondary text if you still want the raw command visible for operators.

- [ ] **Step 4: Consume query params in the eval-runs route**

In `frontend/web/src/routes/eval-runs.tsx`, read the router query string and seed the dialog state:

```tsx
import { useSearchParams } from "react-router-dom";

const [searchParams, setSearchParams] = useSearchParams();
const preselectedStrategy = searchParams.get("strategy") ?? "";
const startRequested = searchParams.get("start") === "1";
const [startOpen, setStartOpen] = useState(startRequested);
```

Initialize the dialog strategy state from that value:

```tsx
const [agentId, setAgentId] = useState<string>(initialAgentId);
```

where the component prop carries:

```tsx
<StartEvalDialog
  initialAgentId={preselectedStrategy}
  onClose={() => {
    setStartOpen(false);
    setSearchParams((prev) => {
      prev.delete("start");
      return prev;
    });
  }}
/>;
```

- [ ] **Step 5: Keep runs list invalidation after successful launch**

Preserve the existing `invalidateQueries({ queryKey: evalKeys.runs() })` and navigate to the detail page after the returned queued run:

```tsx
onSuccess: (detail) => {
  qc.invalidateQueries({ queryKey: evalKeys.runs() });
  onClose();
  navigate(`/eval-runs/${encodeURIComponent(detail.summary.id)}`);
}
```

- [ ] **Step 6: Re-run the focused test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- eval-runs.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/routes/authoring.tsx frontend/web/src/routes/eval-runs.tsx frontend/web/src/routes/eval-runs.test.tsx
git commit -m "feat(eval): launch runs from inspector through dashboard flow"
```

## Task 5: Replace Preset-Only Risk Controls with Full Editable Risk Fields

**Files:**
- Modify: `frontend/web/src/routes/authoring.tsx`
- Modify: `frontend/web/src/api/strategies.ts`
- Test: `frontend/web/src/routes/authoring-risk.test.tsx`

- [ ] **Step 1: Write the failing risk-editor test**

Create `frontend/web/src/routes/authoring-risk.test.tsx`:

```tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import * as strategyApi from "@/api/strategies";
import { AuthoringRoute } from "./authoring";

vi.mock("@/api/strategies");
vi.mock("@/api/chart");
vi.mock("@/api/settings");

it("edits explicit risk fields and saves them", async () => {
  vi.mocked(strategyApi.getStrategy).mockResolvedValue({
    manifest: { id: "01TEST", display_name: "Trend 4H", template: "trend_follower", creator: "@t", asset_universe: [], decision_cadence_minutes: 240, risk_preset_or_config: "balanced" },
    regime_slot: null,
    intern_slot: null,
    trader_slot: null,
    risk: {
      risk_pct_per_trade: 0.015,
      max_concurrent_positions: 2,
      max_leverage: 3,
      stop_loss_atr_multiple: 2,
      daily_loss_kill_pct: 0.05,
    },
    mechanical_params: {},
  } as any);
  vi.mocked(strategyApi.validateDraft).mockResolvedValue({ id: "01TEST", ok: true, errors: [] });
  vi.mocked(strategyApi.setRiskConfig).mockResolvedValue({ id: "01TEST", applied: "explicit" });

  render(
    <MemoryRouter initialEntries={["/authoring/01TEST"]}>
      <QueryClientProvider client={new QueryClient()}>
        <AuthoringRoute />
      </QueryClientProvider>
    </MemoryRouter>
  );

  const input = await screen.findByLabelText("Risk per trade (%)");
  fireEvent.change(input, { target: { value: "2.50" } });
  fireEvent.click(screen.getByRole("button", { name: "Save risk" }));

  expect(strategyApi.setRiskConfig).toHaveBeenCalledWith("01TEST", {
    explicit: {
      risk_pct_per_trade: 0.025,
      max_concurrent_positions: 2,
      max_leverage: 3,
      stop_loss_atr_multiple: 2,
      daily_loss_kill_pct: 0.05,
    },
  });
});
```

- [ ] **Step 2: Run the focused test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- authoring-risk.test.tsx
```

Expected: FAIL because the current risk card only applies presets and has no explicit field editor.

- [ ] **Step 3: Replace the risk card with a controlled explicit editor**

In `frontend/web/src/routes/authoring.tsx`, replace the preset-button-only `RiskCard` with controlled field inputs seeded from `bundle.risk`:

```tsx
const [form, setForm] = useState({
  risk_pct_per_trade: (bundle.risk.risk_pct_per_trade * 100).toFixed(2),
  max_concurrent_positions: String(bundle.risk.max_concurrent_positions),
  max_leverage: String(bundle.risk.max_leverage),
  stop_loss_atr_multiple: String(bundle.risk.stop_loss_atr_multiple),
  daily_loss_kill_pct: (bundle.risk.daily_loss_kill_pct * 100).toFixed(2),
});
```

On save, send the explicit shape:

```tsx
setRiskConfig(bundle.manifest.id, {
  explicit: {
    risk_pct_per_trade: Number(form.risk_pct_per_trade) / 100,
    max_concurrent_positions: Number(form.max_concurrent_positions),
    max_leverage: Number(form.max_leverage),
    stop_loss_atr_multiple: Number(form.stop_loss_atr_multiple),
    daily_loss_kill_pct: Number(form.daily_loss_kill_pct) / 100,
  },
});
```

Render labeled inputs:

```tsx
<Field label="Risk per trade (%)">
  <input value={form.risk_pct_per_trade} onChange={(e) => setForm({ ...form, risk_pct_per_trade: e.target.value })} />
</Field>
```

Repeat for each of the five persisted `RiskConfig` fields.

- [ ] **Step 4: Add minimal client-side validation**

Before mutation, reject obviously invalid states:

```tsx
if (Number(form.risk_pct_per_trade) <= 0) return setLocalError("Risk per trade must be > 0");
if (Number(form.max_concurrent_positions) < 1) return setLocalError("Max concurrent positions must be at least 1");
if (Number(form.max_leverage) <= 0) return setLocalError("Max leverage must be > 0");
```

Keep server validation as the final authority by still surfacing any API error message returned from `setRiskConfig`.

- [ ] **Step 5: Re-run the focused test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- authoring-risk.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Run broader verification**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4/frontend/web
pnpm test -- home.test.tsx strategies.test.tsx eval-runs.test.tsx authoring-risk.test.tsx
pnpm typecheck
pnpm build

cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-dashboard
```

Expected: frontend tests pass, typecheck clean, build clean, dashboard tests pass.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/routes/authoring.tsx frontend/web/src/api/strategies.ts frontend/web/src/routes/authoring-risk.test.tsx
git commit -m "feat(inspector): expose full editable risk configuration"
```

## Self-Review

- [ ] **Spec coverage:** Task 1 covers wizard/store/public-list consistency, Task 2 covers Dashboard rename and removal of dead chrome, Task 3 covers strategy naming and cadence convention, Task 4 covers Eval visibility through the real launcher path, Task 5 covers full Inspector risk editing. No spec requirement is left without a task.
- [ ] **Placeholder scan:** Search this file for unfinished markers or vague future-tense instructions, and remove any step that lacks explicit files or commands before execution.
- [ ] **Type consistency:** Keep `display_name` and `decision_cadence_minutes` aligned between `StrategySummary`, frontend list types, and tests. Keep `RiskConfig` field names identical to `xvision-engine/src/strategies/risk.rs`.
