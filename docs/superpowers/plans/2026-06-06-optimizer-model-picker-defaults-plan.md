# Optimizer Model Picker Defaults Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the Optimizer run controls use the same provider/model pick list semantics as the rest of the app, and replace unclear "default reviewer / default experiment writer" behavior with explicit, visible fallback behavior.

**Architecture:** Keep `frontend/web/src/components/ModelPicker.tsx` as the single shared picker. Add a small Optimizer defaults read-model so the run card can show the actual config fallback, make the run card's selected provider/model state the launch source of truth, and fix the backend reviewer-provider provenance mismatch when a reviewer override is used.

**Tech Stack:** React + TanStack Query + Vitest for the dashboard, Axum + Rust route tests for the backend, existing Settings provider API (`/api/settings/providers`) for the standard pick list.

---

## Orientation

Current `origin/main` context:

- Latest model picker updates landed in PR #830/#834:
  - `frontend/web/src/lib/providers.ts` defines `isProviderConfigured`, including no-auth local providers (`ollama`, `llama-cpp`, `vllm`, `local-candle`).
  - `frontend/web/src/components/ModelPicker.tsx:47-54` filters through `isProviderConfigured` and builds options from `ProviderRow.enabled_models`.
  - `frontend/web/src/components/ModelPicker.test.tsx` verifies Ollama models render even with `api_key_env=""` and `api_key_set=false`.
- Optimizer run controls already import `ModelPicker`, but the wiring is still fragile:
  - `frontend/web/src/features/autooptimizer/LiveCycleView.tsx:251-256` launches from `localStorage` getters, not from the visible select state.
  - `frontend/web/src/features/autooptimizer/LiveCycleView.tsx:359-383` renders picker placeholders `"Use config default"` and `"Use writer provider/default"` without showing what those defaults are.
  - `frontend/web/src/features/autooptimizer/preferences.ts:35-48` can set provider/model overrides but has no clear helpers, so clearing a select cannot clear persisted overrides.
  - `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs:105-110` computes the effective mutator/reviewer provider/model, but `build_cycle_config` records `judge_provider: cfg.mutator.provider.clone()` at `autooptimizer_cycle.rs:475`, ignoring reviewer-provider overrides.

## File Structure

- Modify `frontend/web/src/features/autooptimizer/api.ts`
  - Add `OptimizerRunDefaults` wire type, fetcher, and query key.
- Modify `frontend/web/src/features/autooptimizer/preferences.ts`
  - Add clear helpers for mutator and judge provider/model overrides.
- Modify `frontend/web/src/features/autooptimizer/LiveCycleView.tsx`
  - Hoist optimizer model selection state into the run-card flow.
  - Use `ModelPicker` only for real provider/model options.
  - Show explicit fallback text from the new defaults endpoint.
  - Launch with visible state, not stale localStorage.
- Modify `frontend/web/src/features/autooptimizer/LiveCycleView.test.tsx`
  - Add regression tests for Ollama/no-auth picker options, clearing overrides, and launch payloads.
- Modify `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`
  - Add `GET /api/autooptimizer/run-defaults`.
  - Fix `build_cycle_config` to store the resolved reviewer provider.
- Modify `crates/xvision-dashboard/src/server.rs`
  - Register the new defaults route.
- Modify `crates/xvision-dashboard/tests/flywheel_routes.rs`
  - Add route/defaults coverage and reviewer-provider provenance coverage if the existing run-cycle test can assert it without running a full cycle.
- Follow-up modify `frontend/web/src/components/shell/ChatRail.tsx`
  - Apply the same visible-selection/default clarity treatment after the Optimizer picker fix lands.
- Follow-up test `frontend/web/src/components/shell/ChatRail.test.tsx`
  - Keep the existing no-auth model coverage and add stale-storage/default-label coverage for the rail.

## Task 1: Backend Defaults Read-Model

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Test: `crates/xvision-dashboard/tests/flywheel_routes.rs`

- [ ] **Step 1: Write the failing backend test**

Add a test that calls `GET /api/autooptimizer/run-defaults` and expects the backend to expose the config fallback the UI will label.

```rust
#[tokio::test]
async fn autooptimizer_run_defaults_expose_config_fallback() {
    let app = test_app().await;

    let res = app
        .oneshot(
            Request::builder()
                .uri("/api/autooptimizer/run-defaults")
                .method("GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = read_json(res).await;
    assert_eq!(body["mutator_provider"], "test");
    assert_eq!(body["mutator_model"], "test-model");
    assert_eq!(body["judge_provider"], "test");
    assert_eq!(body["judge_model"], "test-model");
    assert!(body["config_path"].as_str().unwrap().contains("autooptimizer.toml"));
    assert!(body["config_exists"].is_boolean());
}
```

Run:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
scripts/cargo test -p xvision-dashboard autooptimizer_run_defaults_expose_config_fallback
```

Expected: FAIL because the route does not exist.

- [ ] **Step 2: Implement the defaults response**

In `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`, add:

```rust
#[derive(Serialize)]
pub struct RunDefaultsResponse {
    pub mutator_provider: String,
    pub mutator_model: String,
    pub judge_provider: String,
    pub judge_model: String,
    pub config_path: String,
    pub config_exists: bool,
}

pub async fn run_defaults() -> Result<Json<RunDefaultsResponse>, DashboardError> {
    let path = AutoOptimizerConfig::default_path()?;
    let config_exists = path.exists();
    let cfg = if config_exists {
        AutoOptimizerConfig::load(&path)?
    } else {
        AutoOptimizerConfig::default()
    };

    Ok(Json(RunDefaultsResponse {
        mutator_provider: cfg.mutator.provider.clone(),
        mutator_model: cfg.mutator.model.clone(),
        judge_provider: cfg.mutator.provider.clone(),
        judge_model: cfg.mutator.model.clone(),
        config_path: path.display().to_string(),
        config_exists,
    }))
}
```

Register in `crates/xvision-dashboard/src/server.rs`:

```rust
.route(
    "/api/autooptimizer/run-defaults",
    get(autooptimizer_cycle::run_defaults),
)
```

- [ ] **Step 3: Run the backend test**

Run:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
scripts/cargo test -p xvision-dashboard autooptimizer_run_defaults_expose_config_fallback
```

Expected: PASS.

## Task 2: Fix Reviewer Provider Provenance

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/autooptimizer_cycle.rs`
- Test: `crates/xvision-dashboard/tests/flywheel_routes.rs` or a local unit-test module in `autooptimizer_cycle.rs`

- [ ] **Step 1: Write the failing provenance test**

Add a narrow test for `build_cycle_config` so it fails before the implementation change:

```rust
#[test]
fn build_cycle_config_uses_resolved_judge_provider() {
    let cfg = AutoOptimizerConfig::default();
    let judge = Judge {
        provider: "ollama".into(),
        model: "qwen2.5-coder:7b".into(),
        dispatch: Arc::new(MockDispatch::default()),
    };

    let cycle = build_cycle_config(
        &cfg,
        &judge,
        sample_day_scenario(),
        sample_baseline_scenario(),
        HashMap::new(),
        vec![],
    );

    assert_eq!(cycle.judge_provider, "ollama");
    assert_eq!(cycle.judge_model, "qwen2.5-coder:7b");
}
```

If `MockDispatch` and sample scenarios are not available in `autooptimizer_cycle.rs`, prefer extracting a tiny pure helper:

```rust
fn resolved_judge_fields(judge: &Judge) -> (String, String) {
    (judge.provider.clone(), judge.model.clone())
}
```

Then test that helper directly.

Run:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
scripts/cargo test -p xvision-dashboard build_cycle_config_uses_resolved_judge_provider
```

Expected: FAIL because the code currently uses `cfg.mutator.provider`.

- [ ] **Step 2: Implement the fix**

Change `build_cycle_config`:

```rust
CycleConfig {
    // ...
    judge_provider: judge.provider.clone(),
    judge_model: judge.model.clone(),
    // ...
}
```

- [ ] **Step 3: Run the provenance test**

Run:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
scripts/cargo test -p xvision-dashboard build_cycle_config_uses_resolved_judge_provider
```

Expected: PASS.

## Task 3: Frontend API And Preference Helpers

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/api.ts`
- Modify: `frontend/web/src/features/autooptimizer/preferences.ts`
- Test: `frontend/web/src/features/autooptimizer/LiveCycleView.test.tsx`

- [ ] **Step 1: Add the frontend defaults fetcher**

In `frontend/web/src/features/autooptimizer/api.ts`, add:

```ts
export type OptimizerRunDefaults = {
  mutator_provider: string;
  mutator_model: string;
  judge_provider: string;
  judge_model: string;
  config_path: string;
  config_exists: boolean;
};

export async function getRunDefaults(): Promise<OptimizerRunDefaults> {
  return apiFetch<OptimizerRunDefaults>("/api/autooptimizer/run-defaults");
}
```

Extend `autooptimizerKeys`:

```ts
runDefaults: () => [...autooptimizerKeys.all, "run-defaults"] as const,
```

- [ ] **Step 2: Add explicit clear helpers**

In `frontend/web/src/features/autooptimizer/preferences.ts`, add:

```ts
function removeStoredValue(key: string, legacyKey?: string): void {
  localStorage.removeItem(key);
  if (legacyKey) localStorage.removeItem(legacyKey);
}

export function clearStoredMutatorModel(): void {
  removeStoredValue(MUTATOR_MODEL_KEY, LEGACY_MUTATOR_MODEL_KEY);
}

export function clearStoredJudgeModel(): void {
  removeStoredValue(JUDGE_MODEL_KEY, LEGACY_JUDGE_MODEL_KEY);
}

export function clearStoredMutatorProvider(): void {
  removeStoredValue(MUTATOR_PROVIDER_KEY);
}

export function clearStoredJudgeProvider(): void {
  removeStoredValue(JUDGE_PROVIDER_KEY);
}
```

- [ ] **Step 3: Run typecheck for immediate compile errors**

Run:

```bash
cd frontend/web
pnpm typecheck
```

Expected: PASS after consumers are updated in Task 4; if run before Task 4, expected FAIL for unused exports is acceptable depending on lint/type settings.

## Task 4: Make Optimizer Launch Use Visible Picker State

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/LiveCycleView.tsx`
- Test: `frontend/web/src/features/autooptimizer/LiveCycleView.test.tsx`

- [ ] **Step 1: Write the failing no-auth picker regression**

Extend the `listProviders` mock in `LiveCycleView.test.tsx` with an Ollama row and verify the Optimizer run card exposes the same enabled models as the shared picker:

```ts
vi.mocked(listProviders).mockResolvedValue({
  providers: [
    {
      name: "ollama",
      kind: "ollama",
      base_url: "http://localhost:11434",
      api_key_env: "",
      api_key_set: false,
      synthetic: false,
      is_default: false,
      enabled_models: ["llama3.2:latest", "qwen2.5-coder:7b"],
    },
  ],
  default_model: null,
});

renderLiveCycleView();

expect(await screen.findAllByRole("option", { name: "qwen2.5-coder:7b" })).toHaveLength(2);
```

Run:

```bash
cd frontend/web
pnpm test src/features/autooptimizer/LiveCycleView.test.tsx
```

Expected: PASS if the existing import works; keep the test as coverage for future drift.

- [ ] **Step 2: Write the failing stale-storage launch regression**

Add a test that proves the launch body comes from the visible picker state, not stale localStorage:

```ts
it("does not launch with stale stored optimizer models that are absent from the picker", async () => {
  localStorage.setItem("autooptimizer_mutator_provider", "openrouter");
  localStorage.setItem("autooptimizer_mutator_model", "old/model");
  localStorage.setItem("autooptimizer_judge_provider", "openrouter");
  localStorage.setItem("autooptimizer_judge_model", "old/judge");

  const user = userEvent.setup();
  vi.mocked(listProviders).mockResolvedValue({
    providers: [
      {
        name: "ollama",
        kind: "ollama",
        base_url: "http://localhost:11434",
        api_key_env: "",
        api_key_set: false,
        synthetic: false,
        is_default: false,
        enabled_models: ["qwen2.5-coder:7b"],
      },
    ],
    default_model: null,
  });

  renderLiveCycleView();
  await screen.findByRole("option", { name: "Trend follower" });
  await user.selectOptions(screen.getByLabelText("Strategy"), "strategy-1");
  await user.click(screen.getByRole("button", { name: "Run optimizer" }));

  await waitFor(() => {
    const call = vi.mocked(apiFetch).mock.calls.find(([path]) => path === "/api/autooptimizer/run-cycle");
    const body = JSON.parse(String(call?.[1]?.body ?? "{}"));
    expect(body).toMatchObject({ strategy_id: "strategy-1" });
    expect(body.mutator_provider).toBeNull();
    expect(body.mutator_model).toBeNull();
    expect(body.judge_provider).toBeNull();
    expect(body.judge_model).toBeNull();
  });
});
```

Expected: FAIL before refactor because `handleLaunch` reads localStorage directly.

- [ ] **Step 3: Refactor model selection state**

In `LiveCycleView.tsx`, import the new API/default helpers:

```ts
import {
  getRunDefaults,
  // existing imports...
} from "./api";
import {
  clearStoredJudgeModel,
  clearStoredJudgeProvider,
  clearStoredMutatorModel,
  clearStoredMutatorProvider,
  // existing imports...
} from "./preferences";
```

Create a local selection type:

```ts
type OptimizerModelSelection = {
  mutatorProvider: string | null;
  mutatorModel: string;
  judgeProvider: string | null;
  judgeModel: string;
};
```

Move selection state into `CycleLeftCard` and pass it to both `LaunchStrip` and `ModelSelectRow`:

```tsx
function CycleLeftCard() {
  const [selection, setSelection] = useState<OptimizerModelSelection>({
    mutatorProvider: getStoredMutatorProvider(),
    mutatorModel: getStoredMutatorModel() ?? "",
    judgeProvider: getStoredJudgeProvider(),
    judgeModel: getStoredJudgeModel() ?? "",
  });

  return (
    <div id="optimizer-run-controls" className="...">
      <span className="...">Optimizer Run</span>
      <Pill tone="default">No cycle running</Pill>
      <LaunchStrip modelSelection={selection} />
      <ModelSelectRow selection={selection} onSelectionChange={setSelection} />
    </div>
  );
}
```

Change `LaunchStrip` to use props:

```ts
function LaunchStrip({ modelSelection }: { modelSelection: OptimizerModelSelection }) {
  // ...
  launchMutation.mutate({
    strategy_id: trimmed,
    mutator_provider: modelSelection.mutatorProvider,
    mutator_model: modelSelection.mutatorModel || null,
    judge_provider: modelSelection.judgeProvider,
    judge_model: modelSelection.judgeModel || null,
    budget_usd: budget,
    day_start: orNull(dayStart),
    day_end: orNull(dayEnd),
    baseline_start: orNull(baselineStart),
    baseline_end: orNull(baselineEnd),
  });
}
```

- [ ] **Step 4: Preserve only valid, visible picker selections**

In `ModelSelectRow`, derive valid option keys from the same `ProviderRow.enabled_models` data passed to `ModelPicker`:

```ts
const optionKeys = new Set(
  rows.flatMap((r) => r.enabled_models.map((m) => `${r.name}::${m}`)),
);

useEffect(() => {
  if (providers.isLoading) return;
  const mutatorKey = selection.mutatorProvider && selection.mutatorModel
    ? `${selection.mutatorProvider}::${selection.mutatorModel}`
    : "";
  const judgeKey = selection.judgeProvider && selection.judgeModel
    ? `${selection.judgeProvider}::${selection.judgeModel}`
    : "";

  if (mutatorKey && !optionKeys.has(mutatorKey)) {
    clearStoredMutatorProvider();
    clearStoredMutatorModel();
    onSelectionChange((s) => ({ ...s, mutatorProvider: null, mutatorModel: "" }));
  }
  if (judgeKey && !optionKeys.has(judgeKey)) {
    clearStoredJudgeProvider();
    clearStoredJudgeModel();
    onSelectionChange((s) => ({ ...s, judgeProvider: null, judgeModel: "" }));
  }
}, [providers.isLoading, optionKeys, selection, onSelectionChange]);
```

If the inline `Set` causes effect instability, compute it with `useMemo`.

- [ ] **Step 5: Make labels and fallback text explicit**

Add the defaults query in `ModelSelectRow`:

```ts
const defaults = useQuery({
  queryKey: autooptimizerKeys.runDefaults(),
  queryFn: getRunDefaults,
});
```

Render labels as overrides, not defaults:

```tsx
<span className="text-text-3 text-[12px] block">Experiment writer model override</span>
<ModelPicker
  rows={rows}
  loading={providers.isLoading}
  provider={selection.mutatorProvider}
  model={selection.mutatorModel}
  onChange={(p, m) => {
    onSelectionChange((s) => ({ ...s, mutatorProvider: p, mutatorModel: m }));
    if (p === null || m === "") {
      clearStoredMutatorProvider();
      clearStoredMutatorModel();
    } else {
      setStoredMutatorProvider(p);
      setStoredMutatorModel(m);
    }
  }}
  className={`${sel} w-full`}
  ariaLabel="Experiment writer model override"
  placeholder="No override"
/>
<p className="text-[11px] text-text-3">
  No override uses optimizer config: {defaults.data?.mutator_provider ?? "…"} / {defaults.data?.mutator_model ?? "…"}.
</p>
```

Reviewer row:

```tsx
<span className="text-text-3 text-[12px] block">Reviewer model override</span>
<ModelPicker
  rows={rows}
  loading={providers.isLoading}
  provider={selection.judgeProvider}
  model={selection.judgeModel}
  onChange={(p, m) => {
    onSelectionChange((s) => ({ ...s, judgeProvider: p, judgeModel: m }));
    if (p === null || m === "") {
      clearStoredJudgeProvider();
      clearStoredJudgeModel();
    } else {
      setStoredJudgeProvider(p);
      setStoredJudgeModel(m);
    }
  }}
  className={`${sel} w-full`}
  ariaLabel="Reviewer model override"
  placeholder="No override"
/>
<p className="text-[11px] text-text-3">
  No override reviews with {defaults.data?.judge_provider ?? "…"} / {defaults.data?.judge_model ?? "…"} from optimizer config.
</p>
```

Keep the provider/model list source unchanged: `ModelPicker` must receive `rows={providers.data?.providers ?? []}` from `listProviders()`, never a hand-built Optimizer-only option list.

- [ ] **Step 6: Run frontend tests**

Run:

```bash
cd frontend/web
pnpm test src/features/autooptimizer/LiveCycleView.test.tsx src/components/ModelPicker.test.tsx
pnpm typecheck
```

Expected: PASS.

## Task 5: Full Verification

**Files:**
- No new files.

- [ ] **Step 1: Run focused Rust tests**

Run:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
scripts/cargo test -p xvision-dashboard autooptimizer
```

Expected: PASS.

- [ ] **Step 2: Run focused frontend tests**

Run:

```bash
cd frontend/web
pnpm test src/features/autooptimizer/LiveCycleView.test.tsx src/components/ModelPicker.test.tsx src/lib/providers.test.ts
pnpm typecheck
```

Expected: PASS.

- [ ] **Step 3: Manual browser check**

Start the dashboard in the normal local dev flow for this repo, then verify:

1. Settings -> Providers has Ollama or vLLM configured with enabled models.
2. Optimizer -> Configure run shows those no-auth models in both override pickers.
3. The first option is "No override", not a fake "default reviewer" or "default experiment writer" model.
4. Fallback copy names the actual config provider/model and config path behavior.
5. Selecting a reviewer override sends `judge_provider` and `judge_model` in `POST /api/autooptimizer/run-cycle`.
6. Clearing the reviewer override sends nulls so the backend uses the visible config fallback.

## Task 6: Chat Rail Picker Parity Follow-Up

**When to run:** After Tasks 1-5 are implemented and verified. This is intentionally sequenced after the Optimizer fix so the rail can reuse the same clarified picker behavior instead of inventing another variant.

**Files:**
- Modify: `frontend/web/src/components/shell/ChatRail.tsx`
- Test: `frontend/web/src/components/shell/ChatRail.test.tsx`

- [ ] **Step 1: Preserve existing no-auth coverage**

Before changing the rail, run the existing regression test that proves ChatRail includes no-auth provider models:

```bash
cd frontend/web
pnpm test src/components/shell/ChatRail.test.tsx -t "includes a no-auth Ollama provider in the model picker"
```

Expected: PASS. If this fails, fix the shared `ModelPicker`/`isProviderConfigured` path first, not ChatRail-specific option generation.

- [ ] **Step 2: Add a stale-storage regression for the rail**

Add a test that seeds `xvn.chat_rail.provider` and the rail model storage with a provider/model no longer present in `listProviders()`, then verifies ChatRail does not dispatch with that stale pair.

```ts
it("does not dispatch chat with stale stored provider/model that is absent from the picker", async () => {
  localStorage.setItem("xvn.chat_rail.provider", "openrouter");
  localStorage.setItem("xvn.chat_rail.model", "old/model");

  vi.mocked(settingsApi.listProviders).mockResolvedValue({
    providers: [
      {
        name: "ollama",
        kind: "ollama",
        base_url: "http://localhost:11434",
        api_key_env: "",
        api_key_set: false,
        synthetic: false,
        is_default: false,
        enabled_models: ["qwen2.5-coder:7b"],
      },
    ],
    default_model: null,
  });

  const seenDispatches: Array<{ provider?: string; model?: string }> = [];
  mockChatDispatch((req) => {
    seenDispatches.push({ provider: req.provider, model: req.model });
    return okChatResponse();
  });

  renderChatRail();
  await userEvent.type(screen.getByRole("textbox"), "summarize");
  await userEvent.click(screen.getByRole("button", { name: /send/i }));

  expect(seenDispatches[0]).toEqual({
    provider: "ollama",
    model: "qwen2.5-coder:7b",
  });
});
```

Adjust helper names (`mockChatDispatch`, `okChatResponse`, `renderChatRail`) to the existing `ChatRail.test.tsx` utilities; do not add new test infrastructure if the file already has a dispatch/mock harness.

- [ ] **Step 3: Make ChatRail picker fallback explicit**

In `ChatRail.tsx`, keep using:

```tsx
<ModelPicker
  rows={rows}
  loading={loading}
  provider={provider}
  model={model}
  onChange={onChange}
  emptyHint="no models picked - visit Settings > Providers"
/>
```

Then apply the same rule as the Optimizer picker:

- The select displays only options from `listProviders()` via shared `ModelPicker`.
- If a persisted rail provider/model is no longer in the option set, clear or replace it before dispatch.
- The fallback selection is visibly explained as "Workspace default" when it comes from `ProvidersReport.is_default/default_model`, and "First enabled model" only when no workspace default is available.
- No hidden "default model" wording should imply there is a separate ChatRail setting unless one exists in Settings.

- [ ] **Step 4: Run focused ChatRail tests**

Run:

```bash
cd frontend/web
pnpm test src/components/shell/ChatRail.test.tsx
pnpm typecheck
```

Expected: PASS.

## Plan Review Gate Self-Check

The formal metaswarm `Task()` reviewer mechanism is not available in this Codex surface, so this is a local gate check against the rubric.

- Feasibility: PASS. Referenced paths exist on `origin/main`; `ModelPicker`, Settings provider API, `LiveCycleView`, and `autooptimizer_cycle.rs` are real and line-referenced above.
- Completeness: PASS. The plan covers the standard pick list requirement, no-auth provider regression, stale storage launch bug, unclear defaults copy, defaults visibility, reviewer-provider backend provenance, and the requested ChatRail parity follow-up.
- Scope & Alignment: PASS. The plan does not add a new Optimizer settings editor; it makes the current run-level overrides explicit and surfaces the config fallback. ChatRail parity is sequenced as a follow-up after the Optimizer fix, rather than mixed into the first implementation pass. Adding writable Optimizer config can be a separate feature if persistent dashboard defaults are desired.
