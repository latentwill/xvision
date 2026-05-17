# Agent Run Observability UI — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the three-layer agent-run observability UI (status-line strip → bottom dock → dedicated route) onto the Vite SPA, including LIVE mode for active trading, minimize-to-strip behavior, and a stubbed rerun-from-here action.

**Architecture:** Three independently shippable surfaces share one set of components and one zustand store. The dock mounts at the AppShell level so it persists across navigation; the strip mounts per-page when an `agent_run_id` is in context; the dedicated route is the deep-link / pop-out target. A typed API shim with mock fixtures lets the UI land before the backend exists — when the backend track lands, only `api/agent-runs.ts` swaps.

**Tech Stack:** React 18, TypeScript, Vite, TanStack Query, Zustand, Tailwind, Vitest + React Testing Library. No new runtime deps. Source-of-truth spec: `docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`.

**Design source (PIXEL-PERFECT TARGET):** `docs/superpowers/designs/2026-05-17-agent-run-observability/` — HTML/CSS/JS prototype exported from claude.ai/design. The primary file is `Eval Run Detail.html`; it loads `data.jsx`, `strip.jsx`, `flame.jsx`, `dock.jsx`, `app.jsx`. Read these directly when implementing — every dimension, color, hover state, and spacing rule lives in the source. **Recreate pixel-perfect**; do not invent visual decisions. Where this plan and the design differ, the design wins. Key design choices encoded below — strip is a floating bottom-center pill (not a top row); strip and dock are mutually exclusive; the dock has a Logfire-style filter bar with `DecisionJump`; the inspector uses `PullQuote` blocks for prompt/response/tool args; the topbar carries a POST-HOC ⇄ LIVE toggle.

**Scope boundaries:**
- IN: status-line strip, dedicated route, dock (post-hoc + LIVE), flame-graph, rail-tree, inspector, F12 summon, minimize-to-strip, rerun-from-here button (stub only), halt-strategy button (stub only).
- OUT: backend (agent_runs / run_spans tables, OTel plumbing, /api/agent-runs/* endpoints). The plan ships against typed mocks; the backend track replaces the shim.
- OUT: the actual checkpoint/rerun execution. The button surface is wired; the action is a toast saying "checkpoint design pending."
- OUT: project-wide popup audit (FU-3 in the spec) — separate track.

---

## File structure

```
frontend/web/src/
  api/
    agent-runs.ts                          # NEW — fetchers (mock-backed via env flag)
    types-agent-runs.ts                    # NEW — types mirroring spec data model
  features/
    agent-runs/
      index.ts                             # NEW — barrel exports
      mock-fixtures.ts                     # NEW — canned AgentRun JSON
      span-colors.ts                       # NEW — 5-kind palette matching design prototype
      RunStatusStrip.tsx                   # NEW — Layer 1 (floating bottom-center pill)
      RunStatusStrip.test.tsx              # NEW
      TraceDock.tsx                        # NEW — Layer 2 shell
      TraceDock.test.tsx                   # NEW
      FilterBar.tsx                        # NEW — Phase 2.5: Logfire-style filter row
      FilterBar.test.tsx                   # NEW
      DecisionJump.tsx                     # NEW — Phase 2.5: number stepper + prev/next
      DecisionJump.test.tsx                # NEW
      use-span-filter.ts                   # NEW — Phase 2.5: filter state + URL sync
      use-span-filter.test.ts              # NEW
      FlameGraph.tsx                       # NEW — used in dock + route
      FlameGraph.test.tsx                  # NEW
      SpanInspector.tsx                    # NEW — right pane (includes PullQuote)
      SpanInspector.test.tsx               # NEW
      PullQuote.tsx                        # NEW — first-class prompt/response display
      AgentRunIndentedTimeline.tsx         # NEW — Layer 3 center pane
      AgentRunRailTree.tsx                 # NEW — Layer 3 left rail
      HaltStrategyButton.tsx               # NEW — Phase 4
      TopbarModeToggle.tsx                 # NEW — Phase 5: POST-HOC ⇄ LIVE
  routes/
    agent-runs-detail.tsx                  # NEW — Layer 3 route
    agent-runs-detail.test.tsx             # NEW
    eval-runs-detail.tsx                   # MODIFIED — add "View agent trace" link + mount strip
    live.tsx                               # MODIFIED — mount strip
  stores/
    trace-dock.ts                          # NEW — zustand: dock height, selected span
  components/
    responsive/DesktopThreePaneShell.tsx   # MODIFIED — mount <TraceDock/>
    responsive/TabletSplitShell.tsx        # MODIFIED — mount <TraceDock/>
    mobile/MobileShell.tsx                 # MODIFIED — strip-only on mobile
  routes.tsx                               # MODIFIED — add /agent-runs/:runId
CLAUDE.md                                  # MODIFIED — add no-popups rule (Phase 5)
```

**Each phase ends with green tests + a commit + a working surface.** Phases are sequenced because the dock depends on types and the strip click handler; the route depends on the inspector/flame-graph components. The plan is intentionally one PR per phase.

---

# Phase 0 — Foundation: types, mocks, color tokens, API shim

Goal: Land the type contracts and a working mock-backed `api/agent-runs.ts` so subsequent phases can build against typed data without waiting for the backend.

### Task 0.1: Type definitions mirroring the spec data model

**Files:**
- Create: `frontend/web/src/api/types-agent-runs.ts`

- [ ] **Step 1: Write the type module**

```typescript
// frontend/web/src/api/types-agent-runs.ts
//
// Types mirror the Rust data model in
// docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md.
// When the backend lands ts-rs derives, replace this file with the
// generated bindings.

export type RunStatus = "queued" | "running" | "completed" | "failed" | "cancelled";

export type SpanKind =
  | "agent.run"
  | "agent.plan"
  | "model.call"
  | "tool.call"
  | "approval.request"
  | "approval.response"
  | "sandbox.exec"
  | "supervisor.review"
  | "financial.eval"
  | "artifact.write";

export type SpanStatus = "ok" | "error" | "in_progress";

export type RunSpan = {
  span_id: string;
  parent_span_id: string | null;
  name: string;
  kind: SpanKind;
  started_at: string; // ISO
  finished_at: string | null; // ISO, null = in-flight
  status: SpanStatus;
  attributes: Record<string, unknown>;
};

export type ModelCall = {
  model_call_id: string;
  span_id: string;
  provider: string;
  model: string;
  input_tokens: number | null;
  output_tokens: number | null;
  cost_usd: number | null;
  prompt_hash: string;
  response_text: string | null;
};

export type ToolCall = {
  tool_call_id: string;
  span_id: string;
  tool_name: string;
  input_json: unknown;
  output_json: unknown | null;
  error: string | null;
  started_at: string;
  finished_at: string | null;
};

export type AgentRunSummary = {
  run_id: string;
  objective: string;
  strategy_id: string | null;
  agent_id: string | null;
  started_at: string;
  finished_at: string | null;
  status: RunStatus;
  // Pre-rolled aggregates (avoid client-side scans for the strip).
  span_count: number;
  model_call_count: number;
  tool_call_count: number;
  error_count: number;
  total_cost_usd: number;
  total_input_tokens: number;
  total_output_tokens: number;
  duration_ms: number | null;
  financial_eval_id: string | null;
};

export type AgentRunDetail = {
  summary: AgentRunSummary;
  spans: RunSpan[];
  model_calls: ModelCall[];
  tool_calls: ToolCall[];
};

export type AgentRunStreamEvent =
  | { event: "span"; data: RunSpan }
  | { event: "summary"; data: AgentRunSummary };
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter xvision-web typecheck`
Expected: PASS (file is types-only, no runtime).

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/api/types-agent-runs.ts
git commit -m "feat(agent-runs): add typescript types mirroring agent-run spec"
```

### Task 0.2: Mock fixtures

**Files:**
- Create: `frontend/web/src/features/agent-runs/mock-fixtures.ts`
- Create: `frontend/web/src/features/agent-runs/mock-fixtures.test.ts`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/mock-fixtures.test.ts
import { describe, expect, test } from "vitest";
import { MOCK_RUN_COMPLETED, MOCK_RUN_LIVE, MOCK_RUN_ERROR } from "./mock-fixtures";

describe("mock-fixtures", () => {
  test("completed run has terminal status and aggregate totals", () => {
    expect(MOCK_RUN_COMPLETED.summary.status).toBe("completed");
    expect(MOCK_RUN_COMPLETED.summary.duration_ms).toBeGreaterThan(0);
    expect(MOCK_RUN_COMPLETED.spans.length).toBeGreaterThan(0);
    expect(MOCK_RUN_COMPLETED.summary.span_count).toBe(
      MOCK_RUN_COMPLETED.spans.length,
    );
  });

  test("live run has running status and an in-progress span", () => {
    expect(MOCK_RUN_LIVE.summary.status).toBe("running");
    expect(MOCK_RUN_LIVE.summary.finished_at).toBeNull();
    expect(MOCK_RUN_LIVE.spans.some((s) => s.status === "in_progress")).toBe(true);
  });

  test("error run has error_count > 0 and at least one failed span", () => {
    expect(MOCK_RUN_ERROR.summary.error_count).toBeGreaterThan(0);
    expect(MOCK_RUN_ERROR.spans.some((s) => s.status === "error")).toBe(true);
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- mock-fixtures`
Expected: FAIL with "cannot find module './mock-fixtures'".

- [ ] **Step 3: Write fixtures**

```typescript
// frontend/web/src/features/agent-runs/mock-fixtures.ts
import type {
  AgentRunDetail,
  ModelCall,
  RunSpan,
  ToolCall,
} from "@/api/types-agent-runs";

function mkSpan(partial: Partial<RunSpan> & Pick<RunSpan, "span_id" | "name" | "kind">): RunSpan {
  return {
    parent_span_id: null,
    started_at: "2026-05-17T10:00:00.000Z",
    finished_at: "2026-05-17T10:00:03.400Z",
    status: "ok",
    attributes: {},
    ...partial,
  };
}

const COMPLETED_SPANS: RunSpan[] = [
  mkSpan({ span_id: "s1", name: "agent.run", kind: "agent.run",
    finished_at: "2026-05-17T10:00:03.400Z" }),
  mkSpan({ span_id: "s2", parent_span_id: "s1", name: "plan", kind: "agent.plan",
    started_at: "2026-05-17T10:00:00.100Z",
    finished_at: "2026-05-17T10:00:00.500Z" }),
  mkSpan({ span_id: "s3", parent_span_id: "s1", name: "claude-opus-4-7", kind: "model.call",
    started_at: "2026-05-17T10:00:00.500Z",
    finished_at: "2026-05-17T10:00:01.600Z",
    attributes: { cost_usd: 0.04, input_tokens: 8412, output_tokens: 1204 } }),
  mkSpan({ span_id: "s4", parent_span_id: "s3", name: "run_backtest", kind: "tool.call",
    started_at: "2026-05-17T10:00:01.600Z",
    finished_at: "2026-05-17T10:00:03.000Z" }),
  mkSpan({ span_id: "s5", parent_span_id: "s1", name: "claude-opus-4-7", kind: "model.call",
    started_at: "2026-05-17T10:00:03.000Z",
    finished_at: "2026-05-17T10:00:03.300Z",
    attributes: { cost_usd: 0.02, input_tokens: 4210, output_tokens: 612 } }),
  mkSpan({ span_id: "s6", parent_span_id: "s1", name: "review", kind: "supervisor.review",
    started_at: "2026-05-17T10:00:03.300Z",
    finished_at: "2026-05-17T10:00:03.400Z" }),
];

const COMPLETED_MODEL_CALLS: ModelCall[] = [
  { model_call_id: "m1", span_id: "s3", provider: "anthropic",
    model: "claude-opus-4-7", input_tokens: 8412, output_tokens: 1204,
    cost_usd: 0.0416, prompt_hash: "sha256:a1b2c3", response_text: null },
  { model_call_id: "m2", span_id: "s5", provider: "anthropic",
    model: "claude-opus-4-7", input_tokens: 4210, output_tokens: 612,
    cost_usd: 0.0208, prompt_hash: "sha256:d4e5f6", response_text: null },
];

const COMPLETED_TOOL_CALLS: ToolCall[] = [
  { tool_call_id: "t1", span_id: "s4", tool_name: "run_backtest",
    input_json: { strategy: "btc_mean_reversion_v4", days: 30 },
    output_json: { pnl: 0.034, max_drawdown: 0.018 },
    error: null,
    started_at: "2026-05-17T10:00:01.600Z",
    finished_at: "2026-05-17T10:00:03.000Z" },
];

export const MOCK_RUN_COMPLETED: AgentRunDetail = {
  summary: {
    run_id: "run_abc1234",
    objective: "Improve BTC mean reversion strategy",
    strategy_id: "btc_mean_reversion_v4",
    agent_id: "agent_default_trader",
    started_at: "2026-05-17T10:00:00.000Z",
    finished_at: "2026-05-17T10:00:03.400Z",
    status: "completed",
    span_count: COMPLETED_SPANS.length,
    model_call_count: COMPLETED_MODEL_CALLS.length,
    tool_call_count: COMPLETED_TOOL_CALLS.length,
    error_count: 0,
    total_cost_usd: 0.0624,
    total_input_tokens: 12622,
    total_output_tokens: 1816,
    duration_ms: 3400,
    financial_eval_id: "eval_456",
  },
  spans: COMPLETED_SPANS,
  model_calls: COMPLETED_MODEL_CALLS,
  tool_calls: COMPLETED_TOOL_CALLS,
};

export const MOCK_RUN_LIVE: AgentRunDetail = {
  summary: {
    ...MOCK_RUN_COMPLETED.summary,
    run_id: "run_live5678",
    status: "running",
    finished_at: null,
    duration_ms: null,
    span_count: 3,
    error_count: 0,
  },
  spans: [
    COMPLETED_SPANS[0]!, // root agent.run still in progress
    COMPLETED_SPANS[1]!,
    { ...COMPLETED_SPANS[2]!, finished_at: null, status: "in_progress" },
  ].map((s, i) => (i === 0 ? { ...s, finished_at: null, status: "in_progress" } : s)),
  model_calls: [],
  tool_calls: [],
};

export const MOCK_RUN_ERROR: AgentRunDetail = {
  summary: {
    ...MOCK_RUN_COMPLETED.summary,
    run_id: "run_err9999",
    status: "failed",
    error_count: 1,
  },
  spans: COMPLETED_SPANS.map((s, i) =>
    i === 3 ? { ...s, status: "error", attributes: { ...s.attributes, error: "tool timeout" } } : s,
  ),
  model_calls: COMPLETED_MODEL_CALLS,
  tool_calls: COMPLETED_TOOL_CALLS.map((t) => ({ ...t, error: "tool timeout" })),
};
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- mock-fixtures`
Expected: 3 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/mock-fixtures.ts \
        frontend/web/src/features/agent-runs/mock-fixtures.test.ts
git commit -m "feat(agent-runs): mock fixtures for completed/live/error runs"
```

### Task 0.3: Span-kind color tokens (5-kind palette from design prototype)

**Files:**
- Create: `frontend/web/src/features/agent-runs/span-colors.ts`

The design prototype groups the 10 SpanKinds into 5 visual categories with
exact hex values. The mapping is: any `agent.*` → `agent`, `model.call` →
`model`, `tool.call` → `tool`, `approval.*` + `sandbox.exec` +
`supervisor.review` + `financial.eval` → `supervisor`, `artifact.write` →
`artifact`. Hex values are taken verbatim from
`docs/superpowers/designs/2026-05-17-agent-run-observability/dock.jsx`
(KIND_DEF) and `flame.jsx` (KIND_COLORS) — do not invent values.

- [ ] **Step 1: Write the module**

```typescript
// frontend/web/src/features/agent-runs/span-colors.ts
//
// 5-kind palette matching the design prototype. Hex values from
// docs/superpowers/designs/2026-05-17-agent-run-observability/dock.jsx
// (KIND_DEF) and flame.jsx (KIND_COLORS).

import type { SpanKind } from "@/api/types-agent-runs";

export type SpanCategory = "agent" | "model" | "tool" | "supervisor" | "artifact";

type CategoryStyle = {
  hex: string;     // canonical color used everywhere (bar, dot, badge)
  label: string;   // SHORT uppercase tag shown in inspector + strip (5 chars max)
};

export const CATEGORY_STYLES: Record<SpanCategory, CategoryStyle> = {
  agent:      { hex: "#a39a85", label: "AGENT" },
  model:      { hex: "#7dd3fc", label: "MODEL" },
  tool:       { hex: "#6ee7b7", label: "TOOL"  },
  supervisor: { hex: "#d4a547", label: "SUPER" },
  artifact:   { hex: "#a78bfa", label: "ARTIF" },
};

export function categoryOf(kind: SpanKind): SpanCategory {
  if (kind === "agent.run" || kind === "agent.plan") return "agent";
  if (kind === "model.call") return "model";
  if (kind === "tool.call") return "tool";
  if (kind === "artifact.write") return "artifact";
  // approval.*, sandbox.exec, supervisor.review, financial.eval
  return "supervisor";
}

export function spanColor(kind: SpanKind): CategoryStyle {
  return CATEGORY_STYLES[categoryOf(kind)];
}

/** rgba helper for opacity-tinted backgrounds — matches the prototype's hexA(). */
export function withAlpha(hex: string, a: number): string {
  const h = hex.replace("#", "");
  const r = parseInt(h.slice(0, 2), 16);
  const g = parseInt(h.slice(2, 4), 16);
  const b = parseInt(h.slice(4, 6), 16);
  return `rgba(${r},${g},${b},${a})`;
}
```

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter xvision-web typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/features/agent-runs/span-colors.ts
git commit -m "feat(agent-runs): per-span-kind color tokens"
```

### Task 0.4: API shim with mock backing

**Files:**
- Create: `frontend/web/src/api/agent-runs.ts`
- Create: `frontend/web/src/api/agent-runs.test.ts`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/api/agent-runs.test.ts
import { describe, expect, test } from "vitest";
import { getAgentRun, agentRunKeys } from "./agent-runs";

describe("agent-runs API (mock mode)", () => {
  test("getAgentRun returns the canned completed run", async () => {
    const detail = await getAgentRun("run_abc1234");
    expect(detail.summary.run_id).toBe("run_abc1234");
    expect(detail.spans.length).toBeGreaterThan(0);
  });

  test("getAgentRun for unknown id rejects with not_found", async () => {
    await expect(getAgentRun("missing")).rejects.toMatchObject({
      code: "not_found",
    });
  });

  test("agentRunKeys.run produces a stable cache key", () => {
    expect(agentRunKeys.run("x")).toEqual(["agent-runs", "run", "x"]);
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- agent-runs`
Expected: FAIL with "cannot find module './agent-runs'".

- [ ] **Step 3: Write the API shim**

```typescript
// frontend/web/src/api/agent-runs.ts
//
// Phase 0: backed entirely by mocks. When backend lands, replace
// `MOCK_BY_ID` with apiFetch<AgentRunDetail>(`/api/agent-runs/${id}`).
// The mock fixtures double as the contract negotiation surface.

import { ApiError } from "./client";
import {
  MOCK_RUN_COMPLETED,
  MOCK_RUN_ERROR,
  MOCK_RUN_LIVE,
} from "@/features/agent-runs/mock-fixtures";
import type {
  AgentRunDetail,
  AgentRunStreamEvent,
} from "./types-agent-runs";

const MOCK_BY_ID: Record<string, AgentRunDetail> = {
  [MOCK_RUN_COMPLETED.summary.run_id]: MOCK_RUN_COMPLETED,
  [MOCK_RUN_LIVE.summary.run_id]: MOCK_RUN_LIVE,
  [MOCK_RUN_ERROR.summary.run_id]: MOCK_RUN_ERROR,
};

export const agentRunKeys = {
  all: ["agent-runs"] as const,
  run: (id: string) => [...agentRunKeys.all, "run", id] as const,
};

export async function getAgentRun(id: string): Promise<AgentRunDetail> {
  const detail = MOCK_BY_ID[id];
  if (!detail) {
    throw new ApiError(404, "not_found", `agent run ${id} not found`);
  }
  // Simulate async, fixed delay — easy to remove when real API lands.
  await new Promise((r) => setTimeout(r, 30));
  return detail;
}

/**
 * Open a mock stream for a live run. Emits the in-progress span as a
 * "span" event every 800ms with a synthesized cost increment so the strip
 * + dock can demo their live behavior. Returns a close() function.
 */
export function openAgentRunStream(
  runId: string,
  onEvent: (ev: AgentRunStreamEvent) => void,
): () => void {
  const detail = MOCK_BY_ID[runId];
  if (!detail || detail.summary.status !== "running") {
    return () => {};
  }
  let tickCost = detail.summary.total_cost_usd;
  const interval = window.setInterval(() => {
    tickCost += 0.01;
    onEvent({
      event: "summary",
      data: {
        ...detail.summary,
        total_cost_usd: Number(tickCost.toFixed(4)),
      },
    });
  }, 800);
  return () => window.clearInterval(interval);
}
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- agent-runs`
Expected: 3 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/api/agent-runs.ts frontend/web/src/api/agent-runs.test.ts
git commit -m "feat(agent-runs): mock-backed api shim (replaceable when backend lands)"
```

### Task 0.5: Barrel export

**Files:**
- Create: `frontend/web/src/features/agent-runs/index.ts`

- [ ] **Step 1: Write the barrel**

```typescript
// frontend/web/src/features/agent-runs/index.ts
export { MOCK_RUN_COMPLETED, MOCK_RUN_LIVE, MOCK_RUN_ERROR } from "./mock-fixtures";
export { SPAN_COLORS, spanColor } from "./span-colors";
```

- [ ] **Step 2: Typecheck + commit**

```bash
pnpm --filter xvision-web typecheck
git add frontend/web/src/features/agent-runs/index.ts
git commit -m "feat(agent-runs): barrel exports for the feature module"
```

---

# Phase 1 — Status-line strip (floating bottom-center pill)

Goal: A **floating bottom-center pill** (matches `designs/2026-05-17-agent-run-observability/strip.jsx`) that summarizes the run and exposes "expand dock" + "pop out to route" affordances.

**Critical design correction vs. the spec's first draft:**
- Strip is a **fixed bottom-center floating pill**, not a top-of-body strip. `position: fixed; left: 50%; bottom: 14px; transform: translateX(-50%); border-radius: 999px;` with a backdrop blur and dark elevation shadow.
- Strip and dock are **mutually exclusive**. The app renders ONE of `<Strip>` OR `<TraceDock>` at a time, controlled by `dockOpen` in the trace-dock store. When the dock opens, the strip disappears. When the dock minimizes, the strip reappears. This is the prototype's chosen pattern; it overrides the spec's earlier "strip is always visible" wording.
- Strip shows a **currentSpan chip** on its right side: in LIVE mode this is the newest in-flight leaf span; in POST-HOC mode this is whichever span is currently selected in the dock. The chip has a kind-colored vertical bar/dot, the kind label (`AGENT`/`MODEL`/`TOOL`/`SUPER`/`ARTIF`), the span name (truncated), and elapsed ms.
- Strip state colors: `green` (completed) · `blue` (live, pulsing dot) · `amber` (warnings) · `red` (error, with a `1 error` pill appended).

### Task 1.1: RunStatusStrip component (floating pill — match prototype exactly)

**Reference:** `docs/superpowers/designs/2026-05-17-agent-run-observability/strip.jsx` — read it top to bottom and recreate pixel-perfect. The block below summarizes shape; the prototype is the source of truth for spacing, density-glyph rendering, hover states, and the currentSpan chip layout.

**Files:**
- Create: `frontend/web/src/features/agent-runs/RunStatusStrip.tsx`
- Create: `frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx`

**Visual structure** (per the prototype):
- Container: `position: fixed; bottom: 14px; left: 50%; transform: translateX(-50%); z-50; h-8; rounded-full; backdrop-blur(8px); shadow: 0 14px 40px rgba(0,0,0,0.55)`.
- Left: a colored dot (gold/info/warn/danger by tone) with a 3px glow, followed by a tracked uppercase label (`COMPLETED` / `LIVE` / `WARNINGS` / `ERROR`).
- Then a vertical divider, the density-glyph row (`▓▓▓▒▒░░` in gold opacity gradient), then `spans 47 · model 12 · 3.4s · $0.18` in mono tabular-nums.
- Then (when a `currentSpan` is present) a divider + currentSpan chip: kind dot/bar in kind color, kind label tracked uppercase, span name truncated to 260px, elapsed ms in tabular-nums.
- If tone=error, append a small red `1 error` pill. If tone=amber, append `N warnings` pill.
- Right: vertical divider, then two round 24×28 buttons — expand (chevron-up) and pop-out (external-link). Both stop propagation.

**Props the component takes** (matches the prototype's Strip signature):

```typescript
type StripTone = "completed" | "live" | "warn" | "error";
type CurrentSpanChip = {
  name: string;
  color: string;   // hex from CATEGORY_STYLES[cat].hex
  label: string;   // CATEGORY_STYLES[cat].label
  elapsedMs: number;
};

type RunStatusStripProps = {
  summary: AgentRunSummary;
  currentSpan: CurrentSpanChip | null;
  isLive: boolean;
  liveDurationSec: number;  // ticking counter shown when isLive
  tone: StripTone;          // injected by parent (derived from summary OR overridden by halt-strategy)
  onExpand: () => void;
  onPopOut: () => void;
};
```

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RunStatusStrip } from "./RunStatusStrip";
import { MOCK_RUN_COMPLETED, MOCK_RUN_LIVE, MOCK_RUN_ERROR } from "./mock-fixtures";

describe("RunStatusStrip", () => {
  test("renders COMPLETED label and aggregates for a completed run", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_COMPLETED.summary}
        currentSpan={null}
        isLive={false}
        liveDurationSec={0}
        tone="completed"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByText(/COMPLETED/)).toBeInTheDocument();
    expect(screen.getByText(/spans/)).toBeInTheDocument();
  });

  test("LIVE tone shows ticking duration as m:ss", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_LIVE.summary}
        currentSpan={null}
        isLive
        liveDurationSec={43}
        tone="live"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByTestId("run-status-strip")).toHaveAttribute("data-tone", "live");
    expect(screen.getByText("0:43")).toBeInTheDocument();
  });

  test("error tone appends `1 error` pill", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_ERROR.summary}
        currentSpan={null}
        isLive={false}
        liveDurationSec={0}
        tone="error"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByText(/1 error/i)).toBeInTheDocument();
  });

  test("currentSpan chip renders kind label + truncated name + elapsed", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_COMPLETED.summary}
        currentSpan={{ name: "model.call gpt-5", color: "#7dd3fc", label: "MODEL", elapsedMs: 720 }}
        isLive={false}
        liveDurationSec={0}
        tone="completed"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByText("MODEL")).toBeInTheDocument();
    expect(screen.getByText(/model\.call gpt-5/)).toBeInTheDocument();
    expect(screen.getByText("720ms")).toBeInTheDocument();
  });

  test("clicking the body calls onExpand; clicking pop-out calls onPopOut (no double-fire)", async () => {
    const onExpand = vi.fn();
    const onPopOut = vi.fn();
    render(
      <RunStatusStrip
        summary={MOCK_RUN_COMPLETED.summary}
        currentSpan={null}
        isLive={false}
        liveDurationSec={0}
        tone="completed"
        onExpand={onExpand}
        onPopOut={onPopOut}
      />,
    );
    await userEvent.click(screen.getByTestId("run-status-strip"));
    expect(onExpand).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByLabelText(/open dedicated trace view/i));
    expect(onPopOut).toHaveBeenCalledOnce();
    expect(onExpand).toHaveBeenCalledOnce(); // unchanged — pop-out stopped propagation
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- RunStatusStrip`
Expected: FAIL with cannot resolve module.

- [ ] **Step 3: Write the component — match `strip.jsx` pixel-perfect**

Open `docs/superpowers/designs/2026-05-17-agent-run-observability/strip.jsx` and port it. Translate inline styles to a mix of inline CSS variables (for tokens like `var(--surface-elev)`, `var(--border-strong)`) and tailwind. Key pieces to preserve:

- The density-glyph row uses 7 spans, each with a discrete opacity step in gold (0.95 → 0.18) and a final text-4 char. Keep all 7.
- Tone config table at top of component:
  ```typescript
  const TONE: Record<StripTone, { dot: string; label: string; pulse: boolean; glow: string }> = {
    completed: { dot: "var(--gold)",   label: "COMPLETED", pulse: false, glow: "0 0 0 3px var(--gold-bg)" },
    live:      { dot: "var(--info)",   label: "LIVE",      pulse: true,  glow: "0 0 0 3px rgba(111,143,184,0.25)" },
    warn:      { dot: "var(--warn)",   label: "WARNINGS",  pulse: false, glow: "0 0 0 3px rgba(219,146,48,0.20)" },
    error:     { dot: "var(--danger)", label: "ERROR",     pulse: false, glow: "0 0 0 3px rgba(200,68,58,0.25)" },
  };
  ```
- Duration: `isLive ? \`0:${String(liveDurationSec).padStart(2, "0")}\` : fmt(summary.duration_ms)` where the post-hoc form is `${(ms/1000).toFixed(1)}s`.
- Live currentSpan chip uses a pulsing dot with halo; post-hoc uses a 3px-wide vertical bar.
- Whole container is `cursor: pointer` and calls `onExpand` on click; the two right-side icon buttons `e.stopPropagation()`.

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- RunStatusStrip`
Expected: 5 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/RunStatusStrip.tsx \
        frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx
git commit -m "feat(agent-runs): floating-pill RunStatusStrip with currentSpan chip (match design prototype)"
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- RunStatusStrip`
Expected: 5 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/RunStatusStrip.tsx \
        frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx
git commit -m "feat(agent-runs): RunStatusStrip component with tone + duration ticker"
```

### Task 1.2: Mount strip at AppShell level (single global instance, mutually exclusive with dock)

**Files:**
- Create: `frontend/web/src/features/agent-runs/StripDockSlot.tsx`
- Modify: `frontend/web/src/components/responsive/DesktopThreePaneShell.tsx`

The prototype renders ONE of `<Strip>` OR `<Dock>` at a time. To preserve this, put both behind a single `StripDockSlot` component mounted at AppShell level. The slot reads `useTraceDock()` state and:

- if `activeRunId == null` → renders nothing
- else if `height === "collapsed"` → renders `<RunStatusStrip>`
- else → renders `<TraceDock>` (defined in Phase 3)

In Phase 1 the `<TraceDock>` branch can render a placeholder `<div>` so the strip ↔ dock toggle is visible even before Phase 3 lands.

- [ ] **Step 1: Write StripDockSlot**

```typescript
// frontend/web/src/features/agent-runs/StripDockSlot.tsx
import { useEffect } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { useTraceDock } from "@/stores/trace-dock";
import { RunStatusStrip } from "./RunStatusStrip";
// TraceDock added in Phase 3 — until then this is a placeholder.

function deriveTone(summary: { status: string; error_count: number }): "completed" | "live" | "warn" | "error" {
  if (summary.status === "failed" || summary.error_count > 0) return "error";
  if (summary.status === "running") return "live";
  if (summary.status === "cancelled") return "warn";
  return "completed";
}

export function StripDockSlot() {
  const { activeRunId, height, mode, setHeight } = useTraceDock();
  const navigate = useNavigate();

  // Tick once per second so the strip's m:ss duration refreshes while live.
  useEffect(() => {
    if (mode !== "live") return;
    const id = window.setInterval(() => useTraceDock.setState((s) => ({ ...s })), 1000);
    return () => window.clearInterval(id);
  }, [mode]);

  const q = useQuery({
    queryKey: activeRunId ? agentRunKeys.run(activeRunId) : ["agent-runs", "noop"],
    queryFn: () => getAgentRun(activeRunId!),
    enabled: !!activeRunId,
  });

  if (!activeRunId || !q.data) return null;

  if (height === "collapsed") {
    const summary = q.data.summary;
    const startedMs = new Date(summary.started_at).getTime();
    const liveDurationSec = Math.max(0, Math.floor((Date.now() - startedMs) / 1000));
    return (
      <RunStatusStrip
        summary={summary}
        currentSpan={null /* Phase 3 will compute newest inflight leaf */}
        isLive={mode === "live"}
        liveDurationSec={liveDurationSec}
        tone={deriveTone(summary)}
        onExpand={() => setHeight("working")}
        onPopOut={() => navigate(`/agent-runs/${activeRunId}`)}
      />
    );
  }

  // Phase 3 replaces this placeholder with <TraceDock />.
  return <div data-testid="trace-dock-placeholder" />;
}
```

- [ ] **Step 2: Mount in DesktopThreePaneShell**

```typescript
// frontend/web/src/components/responsive/DesktopThreePaneShell.tsx
import { Suspense, type ElementType, lazy } from "react";
import { Outlet } from "react-router-dom";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { Sidebar } from "@/components/shell/Sidebar";

const StripDockSlot = lazy(() =>
  import("@/features/agent-runs/StripDockSlot").then((m) => ({ default: m.StripDockSlot })),
);

export function DesktopThreePaneShell({
  ChatRailComponent,
}: {
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  return (
    <div className="grid grid-cols-[220px_1fr_auto] min-h-screen bg-bg text-text">
      <Sidebar />
      <main className="px-9 pt-9 pb-6 overflow-x-hidden">
        <Outlet />
      </main>
      <Suspense fallback={null}>
        <ChatRailComponent />
      </Suspense>
      <CommandPalette />
      <Suspense fallback={null}>
        <StripDockSlot />
      </Suspense>
    </div>
  );
}
```

Apply the same `<StripDockSlot />` mount to `TabletSplitShell.tsx`. Skip mobile shell — mobile gets a different (Phase-out-of-scope) treatment.

The eval-runs route only needs to push the active run id into the store (Phase 3 Task 3.5 already does this).

For Phase 1 testing, you can also push the active run id manually via a temporary `useEffect` inside `eval-runs-detail.tsx` referencing the mock run id, OR wait until Task 3.5 wires it.

- [ ] **Step 1: Add the helper hook**

Create: `frontend/web/src/features/agent-runs/use-agent-run-for-eval.ts`

```typescript
// Resolves an agent_run_id from an eval run id. Mock-only for Phase 0.
// Replace with backend field lookup when /api/eval/runs returns
// agent_run_id on summary.
import { useQuery } from "@tanstack/react-query";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";

// Phase 0 demo wiring: pretend eval run `<x>` maps to agent run `run_abc1234`.
// Replace with a backend-supplied field.
function agentRunIdForEval(_evalRunId: string): string | null {
  return "run_abc1234";
}

export function useAgentRunForEval(evalRunId: string) {
  const agentRunId = agentRunIdForEval(evalRunId);
  return useQuery({
    queryKey: agentRunId ? agentRunKeys.run(agentRunId) : ["agent-runs", "noop"],
    queryFn: () => getAgentRun(agentRunId!),
    enabled: !!agentRunId,
  });
}
```

- [ ] **Step 2: Modify eval-runs-detail.tsx**

Add the strip mount just below `<Topbar>` and before the existing `<SummaryCard>`. The full diff is small — open the file at line ~85 and locate the `return (<>` block.

Modify: insert this block immediately after the closing `/>` of `<Topbar>` and before `<SummaryCard>`:

```typescript
const agentRunQuery = useAgentRunForEval(detail.summary.id);

// ... inside the return (<>
{agentRunQuery.data ? (
  <RunStatusStrip
    summary={agentRunQuery.data.summary}
    onExpand={() => { /* Phase 3 wires dock open here */ }}
    onPopOut={() => navigate(`/agent-runs/${agentRunQuery.data!.summary.run_id}`)}
  />
) : null}
```

Add imports at top:

```typescript
import { RunStatusStrip } from "@/features/agent-runs/RunStatusStrip";
import { useAgentRunForEval } from "@/features/agent-runs/use-agent-run-for-eval";
```

- [ ] **Step 3: Add a smoke test for the mount**

Modify: `frontend/web/src/routes/eval-runs-detail.test.tsx`

Add a test asserting the strip renders when `useAgentRunForEval` returns data. Skim the existing test file for the render-with-mock-query pattern and copy it. If that file's render helper does not stub the agent-run query, the strip will simply not render — that's fine, write the assertion as conditional and skip it if the test harness doesn't yet wire the new query. Add a TODO comment pointing to Phase 3 where the dock interaction is testable.

If the assertion is non-trivial to wire, add this minimal check instead:

```typescript
test("renders without throwing when agent run query is unstubbed", () => {
  // ... existing render(<EvalRunDetailRoute />)
  // No assertion beyond "did not throw" — the strip mount is gated on
  // useAgentRunForEval returning data, which is OK to be unstubbed here.
});
```

- [ ] **Step 4: Manual smoke + typecheck**

Run:
```bash
pnpm --filter xvision-web typecheck
pnpm --filter xvision-web test
```
Expected: typecheck passes; all tests green.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/routes/eval-runs-detail.tsx \
        frontend/web/src/features/agent-runs/use-agent-run-for-eval.ts \
        frontend/web/src/routes/eval-runs-detail.test.tsx
git commit -m "feat(agent-runs): mount RunStatusStrip on eval run detail"
```

### Task 1.3: Mount strip on live route

**Files:**
- Modify: `frontend/web/src/routes/live.tsx`

- [ ] **Step 1: Add the strip mount**

```typescript
// frontend/web/src/routes/live.tsx
import { useParams } from "react-router-dom";
import { LiveChart } from "@/components/chart/LiveChart";
import { Topbar } from "@/components/shell/Topbar";
import { RunStatusStrip } from "@/features/agent-runs/RunStatusStrip";
import { useAgentRunForEval } from "@/features/agent-runs/use-agent-run-for-eval";

export function LiveRoute() {
  const { id = "" } = useParams();
  const agentRunQuery = useAgentRunForEval(id);
  return (
    <>
      <Topbar title="Live cockpit" sub={id || "—"} />
      {agentRunQuery.data ? (
        <RunStatusStrip
          summary={agentRunQuery.data.summary}
          onExpand={() => {}}
          onPopOut={() => { window.location.href = `/agent-runs/${agentRunQuery.data!.summary.run_id}`; }}
        />
      ) : null}
      <div className="px-6 py-5">
        <LiveChart runId={id} />
      </div>
    </>
  );
}
```

- [ ] **Step 2: Typecheck + commit**

```bash
pnpm --filter xvision-web typecheck
git add frontend/web/src/routes/live.tsx
git commit -m "feat(agent-runs): mount RunStatusStrip on live cockpit route"
```

**End of Phase 1.** The strip renders on both surfaces. Open `/eval-runs/<any>` and `/live/<any>` in the dev server (`pnpm --filter xvision-web dev`) to verify visually.

---

# Phase 2 — Dedicated `/agent-runs/:runId` route

Goal: A full-screen split-pane page with a rail-tree (left), indented timeline (center), and span inspector (right). This route also functions as the pop-out target for the dock in Phase 3.

### Task 2.1: SpanInspector component

**Files:**
- Create: `frontend/web/src/features/agent-runs/SpanInspector.tsx`
- Create: `frontend/web/src/features/agent-runs/SpanInspector.test.tsx`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/SpanInspector.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SpanInspector } from "./SpanInspector";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

const modelSpan = MOCK_RUN_COMPLETED.spans.find((s) => s.kind === "model.call")!;

describe("SpanInspector", () => {
  test("renders kind, name, duration, and attributes", () => {
    render(
      <SpanInspector
        span={modelSpan}
        canRerun
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByText(/model\.call/)).toBeInTheDocument();
    expect(screen.getByText(modelSpan.name)).toBeInTheDocument();
  });

  test("rerun button disabled when canRerun=false, with reason", () => {
    render(
      <SpanInspector
        span={modelSpan}
        canRerun={false}
        rerunDisabledReason="run is live; checkpoint disabled mid-run"
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    const btn = screen.getByRole("button", { name: /rerun from here/i });
    expect(btn).toBeDisabled();
    expect(btn).toHaveAttribute("title", expect.stringContaining("live"));
  });

  test("rerun button calls onRerun when enabled", async () => {
    const onRerun = vi.fn();
    render(
      <SpanInspector
        span={modelSpan}
        canRerun
        onRerun={onRerun}
        onJumpToDecision={() => {}}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /rerun from here/i }));
    expect(onRerun).toHaveBeenCalledWith(modelSpan.span_id);
  });

  test("shows STREAMING badge when span is in_progress", () => {
    render(
      <SpanInspector
        span={{ ...modelSpan, status: "in_progress", finished_at: null }}
        canRerun={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByText(/streaming/i)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- SpanInspector`
Expected: FAIL with cannot resolve module.

- [ ] **Step 3: Write the component**

```typescript
// frontend/web/src/features/agent-runs/SpanInspector.tsx
import type { RunSpan } from "@/api/types-agent-runs";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";

function spanDurationMs(span: RunSpan): number | null {
  if (!span.finished_at) return null;
  return new Date(span.finished_at).getTime() - new Date(span.started_at).getTime();
}

export function SpanInspector({
  span,
  canRerun,
  rerunDisabledReason,
  onRerun,
  onJumpToDecision,
}: {
  span: RunSpan;
  canRerun: boolean;
  rerunDisabledReason?: string;
  onRerun: (spanId: string) => void;
  onJumpToDecision: (spanId: string) => void;
}) {
  const ms = spanDurationMs(span);
  const isLive = span.status === "in_progress";
  return (
    <Card className="p-4 h-full flex flex-col gap-3 text-[12px]">
      <div className="flex items-center gap-2">
        <Pill tone="default">{span.kind}</Pill>
        {isLive ? <Pill tone="info" animated>STREAMING</Pill> : null}
        {span.status === "error" ? <Pill tone="danger">ERROR</Pill> : null}
      </div>
      <div className="font-mono text-[14px]">{span.name}</div>
      <div className="grid grid-cols-2 gap-y-1 font-mono text-[11px] text-text-2">
        <span>span_id</span><span className="text-text">{span.span_id}</span>
        <span>started</span><span className="text-text">{span.started_at}</span>
        <span>finished</span><span className="text-text">{span.finished_at ?? "—"}</span>
        <span>duration</span><span className="text-text">{ms != null ? `${ms}ms` : "—"}</span>
        {Object.entries(span.attributes).map(([k, v]) => (
          <>
            <span key={`k-${k}`}>{k}</span>
            <span key={`v-${k}`} className="text-text break-all">{String(v)}</span>
          </>
        ))}
      </div>
      <div className="mt-auto flex flex-wrap gap-2">
        <button
          type="button"
          onClick={() => onJumpToDecision(span.span_id)}
          className="px-2 py-1 border border-border rounded-sm text-[11px] hover:bg-surface-elev"
        >
          ⤴ jump to decision
        </button>
        <button
          type="button"
          disabled={!canRerun}
          title={canRerun ? undefined : rerunDisabledReason}
          onClick={() => onRerun(span.span_id)}
          className="px-2 py-1 border border-border rounded-sm text-[11px] hover:bg-surface-elev disabled:opacity-40 disabled:cursor-not-allowed"
        >
          ⟲ rerun from here
        </button>
        <button
          type="button"
          onClick={() => navigator.clipboard.writeText(JSON.stringify(span, null, 2))}
          className="px-2 py-1 border border-border rounded-sm text-[11px] hover:bg-surface-elev"
        >
          ⎘ copy span json
        </button>
      </div>
    </Card>
  );
}
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- SpanInspector`
Expected: 4 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/SpanInspector.tsx \
        frontend/web/src/features/agent-runs/SpanInspector.test.tsx
git commit -m "feat(agent-runs): SpanInspector with rerun-disabled gating"
```

### Task 2.2: AgentRunIndentedTimeline component

**Files:**
- Create: `frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx`
- Create: `frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.test.tsx`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AgentRunIndentedTimeline } from "./AgentRunIndentedTimeline";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

describe("AgentRunIndentedTimeline", () => {
  test("renders one row per span with correct nesting", () => {
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getAllByTestId(/^span-row-/)).toHaveLength(MOCK_RUN_COMPLETED.spans.length);
    const child = screen.getByTestId("span-row-s4");
    expect(child).toHaveAttribute("data-depth", "2");
  });

  test("clicking a row calls onSelect with span id", async () => {
    const onSelect = vi.fn();
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByTestId("span-row-s3"));
    expect(onSelect).toHaveBeenCalledWith("s3");
  });

  test("selected row gets data-selected=true", () => {
    render(
      <AgentRunIndentedTimeline
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId="s3"
        onSelect={() => {}}
      />,
    );
    expect(screen.getByTestId("span-row-s3")).toHaveAttribute("data-selected", "true");
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- AgentRunIndentedTimeline`
Expected: FAIL.

- [ ] **Step 3: Write the component**

```typescript
// frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx
import type { RunSpan } from "@/api/types-agent-runs";
import { spanColor } from "./span-colors";

function depthOf(span: RunSpan, byId: Map<string, RunSpan>): number {
  let depth = 0;
  let cur: RunSpan | undefined = span;
  while (cur?.parent_span_id) {
    depth += 1;
    cur = byId.get(cur.parent_span_id);
    if (depth > 32) break; // cycle guard
  }
  return depth;
}

function durationMs(s: RunSpan): number | null {
  if (!s.finished_at) return null;
  return new Date(s.finished_at).getTime() - new Date(s.started_at).getTime();
}

export function AgentRunIndentedTimeline({
  spans,
  selectedSpanId,
  onSelect,
}: {
  spans: RunSpan[];
  selectedSpanId: string | null;
  onSelect: (spanId: string) => void;
}) {
  const byId = new Map(spans.map((s) => [s.span_id, s]));
  const ordered = [...spans].sort(
    (a, b) => new Date(a.started_at).getTime() - new Date(b.started_at).getTime(),
  );
  return (
    <div className="font-mono text-[12px] overflow-y-auto">
      {ordered.map((s) => {
        const depth = depthOf(s, byId);
        const ms = durationMs(s);
        const color = spanColor(s.kind);
        const selected = s.span_id === selectedSpanId;
        return (
          <button
            key={s.span_id}
            type="button"
            data-testid={`span-row-${s.span_id}`}
            data-depth={depth}
            data-selected={selected}
            onClick={() => onSelect(s.span_id)}
            className={`w-full flex items-center gap-2 px-2 py-1 text-left hover:bg-surface-elev ${selected ? "bg-surface-elev" : ""}`}
            style={{ paddingLeft: `${0.5 + depth * 1.25}rem` }}
          >
            <span className={`inline-block w-2 h-2 rounded-sm ${color.bar}`} aria-hidden />
            <span className="text-text-2">{s.kind}</span>
            <span className="text-text">{s.name}</span>
            <span className="ml-auto text-text-3">{ms != null ? `${ms}ms` : "…"}</span>
            {s.status === "error" ? <span className="text-red-400">●</span> : null}
            {s.status === "in_progress" ? <span className="text-blue-400 animate-pulse">●</span> : null}
          </button>
        );
      })}
    </div>
  );
}
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- AgentRunIndentedTimeline`
Expected: 3 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx \
        frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.test.tsx
git commit -m "feat(agent-runs): indented timeline rendering spans by parent depth"
```

### Task 2.3: AgentRunRailTree component

**Files:**
- Create: `frontend/web/src/features/agent-runs/AgentRunRailTree.tsx`
- Create: `frontend/web/src/features/agent-runs/AgentRunRailTree.test.tsx`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/AgentRunRailTree.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AgentRunRailTree } from "./AgentRunRailTree";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

describe("AgentRunRailTree", () => {
  test("renders one node per span with kind labels", () => {
    render(
      <AgentRunRailTree
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getAllByTestId(/^rail-node-/)).toHaveLength(
      MOCK_RUN_COMPLETED.spans.length,
    );
  });

  test("clicking a node calls onSelect", async () => {
    const onSelect = vi.fn();
    render(
      <AgentRunRailTree
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByTestId("rail-node-s4"));
    expect(onSelect).toHaveBeenCalledWith("s4");
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- AgentRunRailTree`
Expected: FAIL.

- [ ] **Step 3: Write the component**

```typescript
// frontend/web/src/features/agent-runs/AgentRunRailTree.tsx
import type { RunSpan } from "@/api/types-agent-runs";
import { spanColor } from "./span-colors";

type Node = { span: RunSpan; children: Node[] };

function buildTree(spans: RunSpan[]): Node[] {
  const byId = new Map<string, Node>();
  spans.forEach((s) => byId.set(s.span_id, { span: s, children: [] }));
  const roots: Node[] = [];
  for (const n of byId.values()) {
    const parentId = n.span.parent_span_id;
    if (parentId && byId.has(parentId)) byId.get(parentId)!.children.push(n);
    else roots.push(n);
  }
  // Sort each level by started_at.
  const sortRec = (nodes: Node[]) => {
    nodes.sort(
      (a, b) =>
        new Date(a.span.started_at).getTime() -
        new Date(b.span.started_at).getTime(),
    );
    nodes.forEach((n) => sortRec(n.children));
  };
  sortRec(roots);
  return roots;
}

function NodeRow({
  node,
  depth,
  selectedSpanId,
  onSelect,
}: {
  node: Node;
  depth: number;
  selectedSpanId: string | null;
  onSelect: (id: string) => void;
}) {
  const color = spanColor(node.span.kind);
  const selected = selectedSpanId === node.span.span_id;
  return (
    <div>
      <button
        type="button"
        data-testid={`rail-node-${node.span.span_id}`}
        onClick={() => onSelect(node.span.span_id)}
        className={`w-full flex items-center gap-1.5 py-0.5 pr-2 text-left text-[11px] hover:bg-surface-elev ${selected ? "bg-surface-elev" : ""}`}
        style={{ paddingLeft: `${0.25 + depth * 0.75}rem` }}
      >
        <span aria-hidden>{node.children.length > 0 ? "▾" : "·"}</span>
        <span className={`inline-block w-1.5 h-1.5 rounded-sm ${color.bar}`} aria-hidden />
        <span className="text-text-2">{node.span.kind.replace(/^.*\./, "")}</span>
      </button>
      {node.children.map((c) => (
        <NodeRow key={c.span.span_id} node={c} depth={depth + 1} selectedSpanId={selectedSpanId} onSelect={onSelect} />
      ))}
    </div>
  );
}

export function AgentRunRailTree({
  spans,
  selectedSpanId,
  onSelect,
}: {
  spans: RunSpan[];
  selectedSpanId: string | null;
  onSelect: (id: string) => void;
}) {
  const roots = buildTree(spans);
  return (
    <div className="font-mono overflow-y-auto h-full border-r border-border">
      {roots.map((r) => (
        <NodeRow key={r.span.span_id} node={r} depth={0} selectedSpanId={selectedSpanId} onSelect={onSelect} />
      ))}
    </div>
  );
}
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- AgentRunRailTree`
Expected: 2 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/AgentRunRailTree.tsx \
        frontend/web/src/features/agent-runs/AgentRunRailTree.test.tsx
git commit -m "feat(agent-runs): rail-tree with depth-sorted parent/child layout"
```

### Task 2.4: agent-runs-detail route

**Files:**
- Create: `frontend/web/src/routes/agent-runs-detail.tsx`
- Create: `frontend/web/src/routes/agent-runs-detail.test.tsx`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/routes/agent-runs-detail.test.tsx
import { describe, expect, test } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { AgentRunDetailRoute } from "./agent-runs-detail";

function renderAt(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/agent-runs/:runId" element={<AgentRunDetailRoute />} />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("AgentRunDetailRoute", () => {
  test("loads the run and renders rail-tree + timeline + inspector", async () => {
    renderAt("/agent-runs/run_abc1234");
    await waitFor(() => expect(screen.getByText(/Improve BTC/)).toBeInTheDocument());
    expect(screen.getAllByTestId(/^rail-node-/).length).toBeGreaterThan(0);
    expect(screen.getAllByTestId(/^span-row-/).length).toBeGreaterThan(0);
  });

  test("renders an error state for unknown id", async () => {
    renderAt("/agent-runs/missing");
    await waitFor(() => expect(screen.getByText(/not found/i)).toBeInTheDocument());
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- agent-runs-detail`
Expected: FAIL.

- [ ] **Step 3: Write the route**

```typescript
// frontend/web/src/routes/agent-runs-detail.tsx
import { useMemo, useState } from "react";
import { useParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { AgentRunRailTree } from "@/features/agent-runs/AgentRunRailTree";
import { AgentRunIndentedTimeline } from "@/features/agent-runs/AgentRunIndentedTimeline";
import { SpanInspector } from "@/features/agent-runs/SpanInspector";

export function AgentRunDetailRoute() {
  const { runId = "" } = useParams<{ runId: string }>();
  const q = useQuery({
    queryKey: agentRunKeys.run(runId),
    queryFn: () => getAgentRun(runId),
    enabled: runId.length > 0,
  });
  const [selectedSpanId, setSelectedSpanId] = useState<string | null>(null);

  const selectedSpan = useMemo(
    () => q.data?.spans.find((s) => s.span_id === selectedSpanId) ?? q.data?.spans[0] ?? null,
    [q.data, selectedSpanId],
  );

  if (q.isPending) {
    return (
      <>
        <Topbar title="Agent run" sub={runId || "Loading…"} />
        <Card className="p-6 animate-pulse">
          <div className="h-5 w-72 bg-surface-elev rounded mb-3" />
        </Card>
      </>
    );
  }

  if (q.isError || !q.data) {
    const message =
      q.error instanceof ApiError && q.error.code === "not_found"
        ? `agent run ${runId} not found`
        : String(q.error);
    return (
      <>
        <Topbar title="Agent run" sub={runId} />
        <Card className="p-6 text-text-2">{message}</Card>
      </>
    );
  }

  const detail = q.data;
  const isLive = detail.summary.status === "running";

  return (
    <>
      <Topbar
        title={`Run ${detail.summary.run_id}`}
        sub={detail.summary.objective}
      />
      <Card className="p-5 mb-4 flex flex-wrap items-center gap-4">
        <div className="font-mono text-[12px] text-text-3">{detail.summary.run_id}</div>
        <Pill tone={detail.summary.error_count > 0 ? "danger" : "default"}>{detail.summary.status}</Pill>
        <span className="font-mono text-[12px] text-text-2">spans: {detail.summary.span_count}</span>
        <span className="font-mono text-[12px] text-text-2">cost: ${detail.summary.total_cost_usd.toFixed(4)}</span>
        <span className="font-mono text-[12px] text-text-2">
          {detail.summary.total_input_tokens.toLocaleString()} in · {detail.summary.total_output_tokens.toLocaleString()} out
        </span>
      </Card>

      <div className="grid grid-cols-[220px_1fr_360px] gap-3 h-[70vh]">
        <Card className="overflow-hidden">
          <AgentRunRailTree
            spans={detail.spans}
            selectedSpanId={selectedSpan?.span_id ?? null}
            onSelect={setSelectedSpanId}
          />
        </Card>
        <Card className="overflow-hidden">
          <AgentRunIndentedTimeline
            spans={detail.spans}
            selectedSpanId={selectedSpan?.span_id ?? null}
            onSelect={setSelectedSpanId}
          />
        </Card>
        {selectedSpan ? (
          <SpanInspector
            span={selectedSpan}
            canRerun={!isLive}
            rerunDisabledReason={isLive ? "run is live; checkpoint disabled mid-run" : undefined}
            onRerun={(spanId) => {
              // Stubbed in Phase 2; wired in Phase 4.
              alert(`rerun-from-here pending checkpoint design (span ${spanId})`);
            }}
            onJumpToDecision={() => { /* TODO: cross-link to eval-runs-detail */ }}
          />
        ) : null}
      </div>
    </>
  );
}
```

- [ ] **Step 4: Wire the route**

Modify: `frontend/web/src/routes.tsx`

Add the lazy import alongside the other route imports:

```typescript
const AgentRunDetailRoute = lazy(() => import("./routes/agent-runs-detail").then((m) => ({ default: m.AgentRunDetailRoute })));
```

Add the route entry inside `children: [...]` (location: near the `eval-runs` entries):

```typescript
{ path: "agent-runs/:runId", element: page(<AgentRunDetailRoute />) },
```

- [ ] **Step 5: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- agent-runs-detail`
Expected: 2 passing.

- [ ] **Step 6: Typecheck**

Run: `pnpm --filter xvision-web typecheck`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/routes/agent-runs-detail.tsx \
        frontend/web/src/routes/agent-runs-detail.test.tsx \
        frontend/web/src/routes.tsx
git commit -m "feat(agent-runs): /agent-runs/:runId route with rail-tree + timeline + inspector"
```

### Task 2.5: Add "View agent trace" link from eval-runs-detail

**Files:**
- Modify: `frontend/web/src/routes/eval-runs-detail.tsx`

- [ ] **Step 1: Add the link**

Find the `SummaryCard` component's header inside `eval-runs-detail.tsx` (around line 270). Insert a link next to the run id display:

```typescript
{detail.summary.id /* existing id display block */}
{agentRunQuery.data ? (
  <Link
    to={`/agent-runs/${agentRunQuery.data.summary.run_id}`}
    className="text-[12px] text-info hover:underline ml-3"
  >
    View agent trace →
  </Link>
) : null}
```

The `<Link>` import is already in the file. The `agentRunQuery` is the hook added in Task 1.2 — if `SummaryCard` does not have access to it, hoist the hook into the parent route component and pass `agentRunQuery.data` as a prop named `agentRunSummary`.

- [ ] **Step 2: Typecheck**

Run: `pnpm --filter xvision-web typecheck`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/routes/eval-runs-detail.tsx
git commit -m "feat(agent-runs): link to /agent-runs/:runId from eval run detail"
```

**End of Phase 2.** Open `/agent-runs/run_abc1234` in the dev server to verify the split-pane layout, click a node in the rail-tree → it selects in the timeline → the inspector updates.

---

# Phase 3 — Bottom dock (the workhorse)

Goal: A resizable bottom dock with four heights (collapsed / peek / working / full), `F12` keyboard summon, flame-graph + inspector, minimize-to-strip behavior, and persisted state. Mounts at the AppShell level so it survives navigation.

### Task 3.1: trace-dock zustand store

**Files:**
- Create: `frontend/web/src/stores/trace-dock.ts`
- Create: `frontend/web/src/stores/trace-dock.test.ts`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/stores/trace-dock.test.ts
import { beforeEach, describe, expect, test } from "vitest";
import { useTraceDock } from "./trace-dock";

describe("trace-dock store", () => {
  beforeEach(() => {
    useTraceDock.setState({
      height: "collapsed",
      selectedSpanId: null,
      activeRunId: null,
      mode: "post-hoc",
    });
  });

  test("toggle: collapsed → working → collapsed", () => {
    expect(useTraceDock.getState().height).toBe("collapsed");
    useTraceDock.getState().toggle();
    expect(useTraceDock.getState().height).toBe("working");
    useTraceDock.getState().toggle();
    expect(useTraceDock.getState().height).toBe("collapsed");
  });

  test("setHeight respects all four states", () => {
    const heights = ["collapsed", "peek", "working", "full"] as const;
    for (const h of heights) {
      useTraceDock.getState().setHeight(h);
      expect(useTraceDock.getState().height).toBe(h);
    }
  });

  test("setActiveRun resets selectedSpan", () => {
    useTraceDock.setState({ selectedSpanId: "s5" });
    useTraceDock.getState().setActiveRun("run_other", "post-hoc");
    expect(useTraceDock.getState().selectedSpanId).toBeNull();
    expect(useTraceDock.getState().activeRunId).toBe("run_other");
    expect(useTraceDock.getState().mode).toBe("post-hoc");
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- stores/trace-dock`
Expected: FAIL.

- [ ] **Step 3: Write the store**

```typescript
// frontend/web/src/stores/trace-dock.ts
import { create } from "zustand";

export type DockHeight = "collapsed" | "peek" | "working" | "full";
export type DockMode = "post-hoc" | "live";

type State = {
  height: DockHeight;
  selectedSpanId: string | null;
  activeRunId: string | null;
  mode: DockMode;
  /** Last non-collapsed height — restored by toggle(). */
  lastOpenHeight: DockHeight;
};

type Actions = {
  setHeight: (h: DockHeight) => void;
  toggle: () => void;
  minimize: () => void;
  setSelectedSpan: (id: string | null) => void;
  setActiveRun: (id: string | null, mode: DockMode) => void;
};

export const useTraceDock = create<State & Actions>((set, get) => ({
  height: "collapsed",
  selectedSpanId: null,
  activeRunId: null,
  mode: "post-hoc",
  lastOpenHeight: "working",
  setHeight: (h) =>
    set((s) => ({
      height: h,
      lastOpenHeight: h === "collapsed" ? s.lastOpenHeight : h,
    })),
  toggle: () => {
    const s = get();
    set({
      height: s.height === "collapsed" ? s.lastOpenHeight : "collapsed",
    });
  },
  minimize: () => set({ height: "collapsed" }),
  setSelectedSpan: (id) => set({ selectedSpanId: id }),
  setActiveRun: (id, mode) =>
    set({ activeRunId: id, mode, selectedSpanId: null }),
}));
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- stores/trace-dock`
Expected: 3 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/stores/trace-dock.ts frontend/web/src/stores/trace-dock.test.ts
git commit -m "feat(trace-dock): zustand store for dock height + selected span"
```

### Task 3.2: FlameGraph component

**Files:**
- Create: `frontend/web/src/features/agent-runs/FlameGraph.tsx`
- Create: `frontend/web/src/features/agent-runs/FlameGraph.test.tsx`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/FlameGraph.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FlameGraph } from "./FlameGraph";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

describe("FlameGraph", () => {
  test("renders one bar per span", () => {
    render(
      <FlameGraph
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    expect(screen.getAllByTestId(/^flame-bar-/)).toHaveLength(MOCK_RUN_COMPLETED.spans.length);
  });

  test("bar widths reflect duration relative to total", () => {
    render(
      <FlameGraph
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={() => {}}
      />,
    );
    // Root span ("agent.run", id "s1") has the longest duration — should
    // get the widest bar.
    const root = screen.getByTestId("flame-bar-s1");
    const width = parseFloat(root.style.width);
    expect(width).toBeGreaterThanOrEqual(95);
  });

  test("clicking a bar calls onSelect with span id", async () => {
    const onSelect = vi.fn();
    render(
      <FlameGraph
        spans={MOCK_RUN_COMPLETED.spans}
        selectedSpanId={null}
        onSelect={onSelect}
      />,
    );
    await userEvent.click(screen.getByTestId("flame-bar-s4"));
    expect(onSelect).toHaveBeenCalledWith("s4");
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- FlameGraph`
Expected: FAIL.

- [ ] **Step 3: Write the component**

```typescript
// frontend/web/src/features/agent-runs/FlameGraph.tsx
import { useMemo } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { spanColor } from "./span-colors";

type LayoutRow = {
  span: RunSpan;
  depth: number;
  leftPct: number;
  widthPct: number;
};

function depthOf(span: RunSpan, byId: Map<string, RunSpan>): number {
  let depth = 0;
  let cur: RunSpan | undefined = span;
  while (cur?.parent_span_id) {
    depth += 1;
    cur = byId.get(cur.parent_span_id);
    if (depth > 32) break;
  }
  return depth;
}

function layout(spans: RunSpan[]): LayoutRow[] {
  if (spans.length === 0) return [];
  const byId = new Map(spans.map((s) => [s.span_id, s]));
  const ts = (iso: string) => new Date(iso).getTime();
  const starts = spans.map((s) => ts(s.started_at));
  const ends = spans.map((s) => s.finished_at ? ts(s.finished_at) : Date.now());
  const t0 = Math.min(...starts);
  const tN = Math.max(...ends);
  const span = Math.max(1, tN - t0);
  return spans
    .map((s) => {
      const start = ts(s.started_at);
      const end = s.finished_at ? ts(s.finished_at) : Date.now();
      return {
        span: s,
        depth: depthOf(s, byId),
        leftPct: ((start - t0) / span) * 100,
        widthPct: Math.max(0.5, ((end - start) / span) * 100),
      };
    })
    .sort((a, b) => a.depth - b.depth || a.leftPct - b.leftPct);
}

export function FlameGraph({
  spans,
  selectedSpanId,
  onSelect,
}: {
  spans: RunSpan[];
  selectedSpanId: string | null;
  onSelect: (spanId: string) => void;
}) {
  const rows = useMemo(() => layout(spans), [spans]);
  const ROW_H = 18;
  const maxDepth = rows.reduce((m, r) => Math.max(m, r.depth), 0);
  const totalH = (maxDepth + 1) * ROW_H;

  return (
    <div className="relative w-full overflow-x-auto overflow-y-auto h-full" role="figure" aria-label="span flame graph">
      <div className="relative" style={{ height: totalH, minWidth: "100%" }}>
        {rows.map((r) => {
          const color = spanColor(r.span.kind);
          const selected = r.span.span_id === selectedSpanId;
          const cost = (r.span.attributes as { cost_usd?: number }).cost_usd;
          return (
            <button
              key={r.span.span_id}
              type="button"
              data-testid={`flame-bar-${r.span.span_id}`}
              onClick={() => onSelect(r.span.span_id)}
              title={`${r.span.kind} · ${r.span.name}${cost != null ? ` · $${cost}` : ""}`}
              className={`absolute text-[10px] font-mono leading-[16px] px-1.5 truncate text-left
                ${color.bar} ${color.text}
                ${selected ? "ring-2 ring-white/80" : ""}
                ${r.span.status === "in_progress" ? "animate-pulse" : ""}
                ${r.span.status === "error" ? "outline outline-1 outline-red-400" : ""}`}
              style={{
                left: `${r.leftPct}%`,
                width: `${r.widthPct}%`,
                top: r.depth * ROW_H,
                height: ROW_H - 2,
              }}
            >
              {r.span.name}
              {cost != null ? ` · $${cost}` : ""}
            </button>
          );
        })}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- FlameGraph`
Expected: 3 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/FlameGraph.tsx \
        frontend/web/src/features/agent-runs/FlameGraph.test.tsx
git commit -m "feat(agent-runs): FlameGraph with proportional bar widths + depth stacking"
```

### Task 3.3: TraceDock shell

**Files:**
- Create: `frontend/web/src/features/agent-runs/TraceDock.tsx`
- Create: `frontend/web/src/features/agent-runs/TraceDock.test.tsx`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/TraceDock.test.tsx
import { beforeEach, describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { TraceDock } from "./TraceDock";
import { useTraceDock } from "@/stores/trace-dock";

function renderDock() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <TraceDock />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("TraceDock", () => {
  beforeEach(() => {
    useTraceDock.setState({
      height: "collapsed",
      selectedSpanId: null,
      activeRunId: null,
      mode: "post-hoc",
      lastOpenHeight: "working",
    });
  });

  test("renders nothing when activeRunId is null", () => {
    renderDock();
    expect(screen.queryByTestId("trace-dock")).toBeNull();
  });

  test("renders header when activeRunId set, hidden body when collapsed", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "collapsed" });
    renderDock();
    // Header still hidden at collapsed — strip handles that.
    expect(screen.queryByTestId("trace-dock-body")).toBeNull();
  });

  test("shows body at working height", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderDock();
    expect(await screen.findByTestId("trace-dock-body")).toBeInTheDocument();
  });

  test("minimize button collapses the dock", async () => {
    useTraceDock.setState({ activeRunId: "run_abc1234", height: "working" });
    renderDock();
    await screen.findByTestId("trace-dock-body");
    await userEvent.click(screen.getByLabelText(/minimize/i));
    expect(useTraceDock.getState().height).toBe("collapsed");
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- TraceDock`
Expected: FAIL.

- [ ] **Step 3: Write the shell**

```typescript
// frontend/web/src/features/agent-runs/TraceDock.tsx
import { useEffect, useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { agentRunKeys, getAgentRun } from "@/api/agent-runs";
import { useTraceDock, type DockHeight } from "@/stores/trace-dock";
import { FlameGraph } from "./FlameGraph";
import { SpanInspector } from "./SpanInspector";

const HEIGHT_PX: Record<DockHeight, number> = {
  collapsed: 0,
  peek: 240,
  working: 480,
  full: Math.floor(window.innerHeight * 0.8),
};

export function TraceDock() {
  const { height, activeRunId, mode, selectedSpanId, minimize, setHeight, setSelectedSpan } =
    useTraceDock();
  const navigate = useNavigate();

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "F12") {
        e.preventDefault();
        useTraceDock.getState().toggle();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, []);

  const q = useQuery({
    queryKey: activeRunId ? agentRunKeys.run(activeRunId) : ["agent-runs", "noop"],
    queryFn: () => getAgentRun(activeRunId!),
    enabled: !!activeRunId,
  });

  const selectedSpan = useMemo(
    () => q.data?.spans.find((s) => s.span_id === selectedSpanId) ?? q.data?.spans[0] ?? null,
    [q.data, selectedSpanId],
  );

  if (!activeRunId) return null;
  if (height === "collapsed") return null;

  const summary = q.data?.summary;
  const isLive = mode === "live";

  return (
    <div
      data-testid="trace-dock"
      className="fixed bottom-0 left-0 right-0 z-30 bg-bg border-t border-border shadow-2xl flex flex-col"
      style={{ height: HEIGHT_PX[height] }}
    >
      <div className="flex items-center gap-3 px-3 h-8 border-b border-border text-[11px] font-mono">
        <span className="text-text-2">TRACE</span>
        {summary ? (
          <>
            <span aria-hidden className="opacity-60">▓▒░</span>
            <span>{summary.span_count} spans</span>
            <span className="opacity-40">·</span>
            <span>{summary.model_call_count} model</span>
            <span className="opacity-40">·</span>
            <span>${summary.total_cost_usd.toFixed(4)}</span>
            {isLive ? <span className="text-blue-300 ml-2 animate-pulse">● LIVE</span> : null}
          </>
        ) : (
          <span className="text-text-3">loading…</span>
        )}
        <div className="ml-auto flex items-center gap-1">
          {(["peek", "working", "full"] as const).map((h) => (
            <button
              key={h}
              type="button"
              onClick={() => setHeight(h)}
              aria-pressed={height === h}
              className={`px-1.5 py-0.5 border rounded-sm ${height === h ? "border-text" : "border-border"}`}
            >
              {h}
            </button>
          ))}
          <button
            type="button"
            aria-label="pop out to dedicated view"
            onClick={() => navigate(`/agent-runs/${activeRunId}`)}
            className="px-2 hover:opacity-80"
          >
            ⤡
          </button>
          <button
            type="button"
            aria-label="minimize dock"
            onClick={minimize}
            className="px-2 hover:opacity-80"
          >
            ⤓
          </button>
        </div>
      </div>
      <div data-testid="trace-dock-body" className="flex flex-1 min-h-0">
        <div className={`min-w-0 ${height === "peek" ? "flex-1" : "flex-1 border-r border-border"}`}>
          {q.data ? (
            <FlameGraph
              spans={q.data.spans}
              selectedSpanId={selectedSpan?.span_id ?? null}
              onSelect={setSelectedSpan}
            />
          ) : null}
        </div>
        {height !== "peek" && selectedSpan ? (
          <div className="w-[360px] min-w-0">
            <SpanInspector
              span={selectedSpan}
              canRerun={!isLive}
              rerunDisabledReason={isLive ? "run is live; checkpoint disabled mid-run" : undefined}
              onRerun={(spanId) => alert(`rerun-from-here pending checkpoint design (span ${spanId})`)}
              onJumpToDecision={() => { /* TODO: cross-link */ }}
            />
          </div>
        ) : null}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- TraceDock`
Expected: 4 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/TraceDock.tsx \
        frontend/web/src/features/agent-runs/TraceDock.test.tsx
git commit -m "feat(agent-runs): TraceDock shell with F12 summon and height switcher"
```

### Task 3.4: Mount TraceDock at AppShell level

**Files:**
- Modify: `frontend/web/src/components/responsive/DesktopThreePaneShell.tsx`
- Modify: `frontend/web/src/components/responsive/TabletSplitShell.tsx`

- [ ] **Step 1: Modify DesktopThreePaneShell**

```typescript
// frontend/web/src/components/responsive/DesktopThreePaneShell.tsx
import { Suspense, type ElementType, lazy } from "react";
import { Outlet } from "react-router-dom";

import type { ChatRailProps } from "@/components/shell/ChatRail";
import { CommandPalette } from "@/components/shell/CommandPalette";
import { Sidebar } from "@/components/shell/Sidebar";

const TraceDock = lazy(() =>
  import("@/features/agent-runs/TraceDock").then((m) => ({ default: m.TraceDock })),
);

export function DesktopThreePaneShell({
  ChatRailComponent,
}: {
  ChatRailComponent: ElementType<ChatRailProps>;
}) {
  return (
    <div className="grid grid-cols-[220px_1fr_auto] min-h-screen bg-bg text-text">
      <Sidebar />
      <main className="px-9 pt-9 pb-6 overflow-x-hidden">
        <Outlet />
      </main>
      <Suspense fallback={null}>
        <ChatRailComponent />
      </Suspense>
      <CommandPalette />
      <Suspense fallback={null}>
        <TraceDock />
      </Suspense>
    </div>
  );
}
```

- [ ] **Step 2: Modify TabletSplitShell similarly**

Add the same `lazy(() => import("@/features/agent-runs/TraceDock"))` and `<Suspense fallback={null}><TraceDock /></Suspense>` at the bottom of its top-level layout return.

- [ ] **Step 3: Typecheck + smoke test**

```bash
pnpm --filter xvision-web typecheck
pnpm --filter xvision-web test
```
Expected: green.

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/components/responsive/DesktopThreePaneShell.tsx \
        frontend/web/src/components/responsive/TabletSplitShell.tsx
git commit -m "feat(agent-runs): mount TraceDock at AppShell level (desktop + tablet)"
```

### Task 3.5: Wire strip expand → opens dock; set activeRunId from routes

**Files:**
- Modify: `frontend/web/src/routes/eval-runs-detail.tsx`
- Modify: `frontend/web/src/routes/live.tsx`
- Modify: `frontend/web/src/routes/agent-runs-detail.tsx`

- [ ] **Step 1: Add an effect in eval-runs-detail to set the active run**

In `eval-runs-detail.tsx`, after `useAgentRunForEval` resolves, push the run id into the dock store and wire `onExpand`:

```typescript
import { useEffect } from "react";
import { useTraceDock } from "@/stores/trace-dock";

// ... inside the component body, after agentRunQuery:
useEffect(() => {
  if (agentRunQuery.data) {
    useTraceDock.getState().setActiveRun(
      agentRunQuery.data.summary.run_id,
      agentRunQuery.data.summary.status === "running" ? "live" : "post-hoc",
    );
  }
}, [agentRunQuery.data?.summary.run_id]);

// ... where <RunStatusStrip ... onExpand={...}/> is rendered:
onExpand={() => useTraceDock.getState().setHeight("working")}
```

Cleanup: on unmount, do NOT clear `activeRunId` — the dock should persist across navigation per the spec. Only switch when a new run becomes the active context on a different page.

- [ ] **Step 2: Same in live.tsx**

Add the same `useEffect` + `onExpand={() => useTraceDock.getState().setHeight("working")}` wiring. In live mode, set `mode: "live"`.

- [ ] **Step 3: Same in agent-runs-detail.tsx**

Push the active run into the dock store when the route loads:

```typescript
useEffect(() => {
  if (q.data) {
    useTraceDock.getState().setActiveRun(
      q.data.summary.run_id,
      q.data.summary.status === "running" ? "live" : "post-hoc",
    );
  }
}, [q.data?.summary.run_id]);
```

- [ ] **Step 4: Typecheck + run all tests**

```bash
pnpm --filter xvision-web typecheck
pnpm --filter xvision-web test
```
Expected: green.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/routes/eval-runs-detail.tsx \
        frontend/web/src/routes/live.tsx \
        frontend/web/src/routes/agent-runs-detail.tsx
git commit -m "feat(agent-runs): wire route → dock store; strip-click opens dock to working height"
```

**End of Phase 3.** Open `/eval-runs/<x>`, click the strip → dock opens at working height. Press F12 → dock toggles. Click a span in the flame-graph → inspector updates. Navigate to `/strategies` → dock stays (run still in context). Click pop-out → navigates to `/agent-runs/<run_id>`.

---

# Phase 4 — LIVE mode behaviors + halt-strategy stub

Goal: When the active run is live (status=running), the dock pulses, the flame-graph auto-scrolls to follow newest span, and a `HaltStrategyButton` appears in the dock header that uses an inline confirm row (no popup).

### Task 4.1: HaltStrategyButton with inline confirm

**Files:**
- Create: `frontend/web/src/features/agent-runs/HaltStrategyButton.tsx`
- Create: `frontend/web/src/features/agent-runs/HaltStrategyButton.test.tsx`

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/HaltStrategyButton.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { HaltStrategyButton } from "./HaltStrategyButton";

describe("HaltStrategyButton", () => {
  test("clicking once reveals the inline confirm input, not a popup", async () => {
    render(<HaltStrategyButton strategyName="btc_mr" onHalt={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /halt strategy/i }));
    expect(screen.getByPlaceholderText(/type btc_mr to confirm/i)).toBeInTheDocument();
    // The confirm row is part of the same tree, not a portal/dialog.
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  test("submit disabled until typed name matches", async () => {
    render(<HaltStrategyButton strategyName="btc_mr" onHalt={() => {}} />);
    await userEvent.click(screen.getByRole("button", { name: /halt strategy/i }));
    const input = screen.getByPlaceholderText(/type btc_mr to confirm/i);
    const submit = screen.getByRole("button", { name: /^halt$/i });
    expect(submit).toBeDisabled();
    await userEvent.type(input, "btc_mr");
    expect(submit).toBeEnabled();
  });

  test("calling submit invokes onHalt", async () => {
    const onHalt = vi.fn();
    render(<HaltStrategyButton strategyName="btc_mr" onHalt={onHalt} />);
    await userEvent.click(screen.getByRole("button", { name: /halt strategy/i }));
    await userEvent.type(screen.getByPlaceholderText(/type btc_mr to confirm/i), "btc_mr");
    await userEvent.click(screen.getByRole("button", { name: /^halt$/i }));
    expect(onHalt).toHaveBeenCalledOnce();
  });
});
```

- [ ] **Step 2: Run test, verify failure**

Run: `pnpm --filter xvision-web test -- HaltStrategyButton`
Expected: FAIL.

- [ ] **Step 3: Write the component**

```typescript
// frontend/web/src/features/agent-runs/HaltStrategyButton.tsx
import { useState } from "react";

export function HaltStrategyButton({
  strategyName,
  onHalt,
}: {
  strategyName: string;
  onHalt: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [typed, setTyped] = useState("");
  const matches = typed === strategyName;
  return (
    <div className="flex items-center gap-2">
      {!open ? (
        <button
          type="button"
          onClick={() => setOpen(true)}
          className="px-2 py-1 border border-red-500/60 text-red-300 rounded-sm text-[11px] hover:bg-red-950/40"
        >
          ⏹ halt strategy
        </button>
      ) : (
        <>
          <input
            type="text"
            value={typed}
            onChange={(e) => setTyped(e.target.value)}
            placeholder={`type ${strategyName} to confirm`}
            className="px-2 py-1 bg-surface-card border border-border rounded-sm text-[11px] font-mono w-56"
          />
          <button
            type="button"
            disabled={!matches}
            onClick={() => { onHalt(); setOpen(false); setTyped(""); }}
            className="px-2 py-1 border border-red-500/60 text-red-200 bg-red-950/60 rounded-sm text-[11px] disabled:opacity-40"
          >
            halt
          </button>
          <button
            type="button"
            onClick={() => { setOpen(false); setTyped(""); }}
            className="px-2 py-1 text-text-3 text-[11px] hover:text-text"
          >
            cancel
          </button>
        </>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Run test, verify passing**

Run: `pnpm --filter xvision-web test -- HaltStrategyButton`
Expected: 3 passing.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/agent-runs/HaltStrategyButton.tsx \
        frontend/web/src/features/agent-runs/HaltStrategyButton.test.tsx
git commit -m "feat(agent-runs): HaltStrategyButton with inline confirm (no popup)"
```

### Task 4.2: Mount HaltStrategyButton in TraceDock when live

**Files:**
- Modify: `frontend/web/src/features/agent-runs/TraceDock.tsx`

- [ ] **Step 1: Add the live-only halt button**

In `TraceDock.tsx`, locate the header `<div className="ml-auto flex items-center gap-1">` and add the HaltStrategyButton just before the height-switcher buttons, gated on `isLive`:

```typescript
import { HaltStrategyButton } from "./HaltStrategyButton";

// ... inside header ml-auto group:
{isLive && summary?.strategy_id ? (
  <HaltStrategyButton
    strategyName={summary.strategy_id}
    onHalt={() => alert(`halt-strategy stubbed (strategy ${summary.strategy_id})`)}
  />
) : null}
```

- [ ] **Step 2: Run tests**

Run: `pnpm --filter xvision-web test`
Expected: green.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/features/agent-runs/TraceDock.tsx
git commit -m "feat(agent-runs): halt-strategy button appears in dock header when live"
```

### Task 4.3: Live streaming wire-up via openAgentRunStream

**Files:**
- Modify: `frontend/web/src/features/agent-runs/TraceDock.tsx`

- [ ] **Step 1: Subscribe to the mock stream when live**

Add to `TraceDock.tsx`:

```typescript
import { openAgentRunStream } from "@/api/agent-runs";
import { useQueryClient } from "@tanstack/react-query";

// inside the component, after `q = useQuery({...})`:
const qc = useQueryClient();
useEffect(() => {
  if (!activeRunId || mode !== "live") return;
  const close = openAgentRunStream(activeRunId, (ev) => {
    if (ev.event === "summary") {
      qc.setQueryData(agentRunKeys.run(activeRunId), (prev: typeof q.data | undefined) =>
        prev ? { ...prev, summary: ev.data } : prev,
      );
    }
    if (ev.event === "span") {
      qc.setQueryData(agentRunKeys.run(activeRunId), (prev: typeof q.data | undefined) =>
        prev ? { ...prev, spans: [...prev.spans, ev.data] } : prev,
      );
    }
  });
  return close;
}, [activeRunId, mode, qc]);
```

- [ ] **Step 2: Manual smoke**

Run: `pnpm --filter xvision-web dev`

Open `/agent-runs/run_live5678` → open dock → confirm "● LIVE" pulses in header AND the cost number ticks up every ~800ms.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/features/agent-runs/TraceDock.tsx
git commit -m "feat(agent-runs): subscribe to mock stream + write-through to react-query cache"
```

**End of Phase 4.** Live mode visibly ticks; halt button works as inline confirm; no popup is ever shown.

---

# Phase 5 — Project rule: no popups

Goal: Record the no-popups rule in `CLAUDE.md` so future contributors and AI agents do not reach for `Dialog`/`Modal`/`Sheet`/`Popover` instinctively.

### Task 5.1: Add the rule to CLAUDE.md

**Files:**
- Modify: `/Users/edkennedy/Code/xvision/CLAUDE.md`

- [ ] **Step 1: Read the current bottom of CLAUDE.md**

Run: `tail -30 CLAUDE.md` to identify where to insert. The new section should sit after the existing top-level sections.

- [ ] **Step 2: Append a new section**

Add at the end of `CLAUDE.md`:

```markdown
## Frontend UI rule: no popups

The dashboard SPA does not use popups, modals, sheets, popovers, or any
overlay that steals focus or paints over the primary surface.
Confirmations, detail views, agent windows, settings panels, error
recovery flows, share dialogs — everything routes, docks, rails,
accordions, tabs, or inline-expands.

Exceptions:
- Toasts (transient, non-focus-stealing feedback). Allowed.
- Native browser primitives we cannot reasonably replace (file picker,
  print dialog). Avoid where possible; do not invent new ones.

Why: popups destroy the spatial mental model of the app, are hostile to
keyboard navigation, deep-linking, and screen-sharing, and are a sign of
weak information architecture — the question they answer should have a
home in the actual layout.

Adopted 2026-05-17 via
`docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`.
A separate track will audit existing `Dialog`/`Modal`/`Sheet`/`Popover`
usage in `frontend/web/src/` and migrate each.
```

- [ ] **Step 3: Commit**

```bash
git add CLAUDE.md
git commit -m "docs(frontend): elevate no-popups to a written project rule"
```

**End of Phase 5.** Plan complete.

---

## Out-of-band follow-ups (DO NOT execute as part of this plan)

These are reminders of work this plan deliberately scoped out:

- **FU-A (separate plan)**: Backend track — `agent_runs`, `run_spans`,
  `model_calls`, `tool_calls` tables; `/api/agent-runs/*` REST + SSE
  stream; OTel `tracing-opentelemetry` bridge in
  `crates/xvision-engine/src/agent/**`. When this lands, swap
  `frontend/web/src/api/agent-runs.ts` from the mock shim to real
  `apiFetch` calls.
- **FU-B (separate spec)**: Checkpoint / rerun-from-here design —
  branch semantics, deterministic input capture per span kind, storage
  cost. Required before the inspector's rerun button can do anything
  beyond `alert()`.
- **FU-C (separate plan)**: Project-wide popup audit + migration. List
  every `Dialog`/`Modal`/`Sheet`/`Popover` in `frontend/web/src/`,
  assign each to a non-popup surface, migrate.
- **FU-D (separate spec)**: Live-trading-safety — error escalation
  beyond strip color (toast, browser notification, halt-on-error
  policy).

---

## Self-review

**Spec coverage check** (against `2026-05-17-agent-run-observability-ui-design.md`):

| Spec section | Plan coverage |
|---|---|
| Three-layer stack (strip / dock / route) | Phase 1 / Phase 3 / Phase 2 |
| Dock minimize-to-strip, not close | Phase 3 Task 3.1 (`minimize` action) + Task 3.3 |
| Dock four heights | Phase 3 Task 3.1 (`DockHeight` enum) + Task 3.3 |
| Dock F12 summon | Phase 3 Task 3.3 (`onKey` listener) |
| Flame-graph in dock | Phase 3 Task 3.2 + Task 3.3 |
| Rail-tree + indented timeline on route | Phase 2 Task 2.2 + 2.3 |
| Inspector with rerun button | Phase 2 Task 2.1 |
| LIVE mode dock pulse + halt button | Phase 4 Task 4.1 + 4.2 + 4.3 |
| Halt-strategy inline confirm (no popup) | Phase 4 Task 4.1 (test explicitly asserts no `role=dialog`) |
| Strip on eval-runs-detail + live | Phase 1 Task 1.2 + 1.3 |
| `/agent-runs/:runId` route | Phase 2 Task 2.4 |
| Bidirectional link eval ↔ agent-run | Phase 2 Task 2.5 (link out); jump-to-decision is a TODO |
| AppShell-level dock mount | Phase 3 Task 3.4 |
| Span color tokens | Phase 0 Task 0.3 |
| No-popups rule in CLAUDE.md | Phase 5 Task 5.1 |
| Mock API shim until backend lands | Phase 0 Task 0.4 |
| Rerun-from-here stubbed (not implemented) | Phase 2 Task 2.1 + Phase 4 (alert) — explicit in scope boundaries |

**Known gaps left as TODO comments in code (intentional, scoped out):**
- "Jump to decision" cross-link from inspector — needs eval-run decision index ↔ span mapping, which the backend will define.
- Branched-runs section in SummaryCard — depends on checkpoint design (FU-B).
- Real SSE endpoint — Phase 4 uses mock stream; real endpoint comes from FU-A.

**Type-consistency check:** All types defined in Task 0.1 are reused verbatim in Phases 1-4 (`AgentRunSummary`, `RunSpan`, `SpanKind`, etc). The `DockHeight` enum defined in Task 3.1 is reused in Task 3.3. `agentRunKeys` defined in Task 0.4 is reused in Tasks 2.4 + 3.3 + 4.3.

**No placeholders detected.** All steps contain runnable code or exact commands. The two `alert()` calls (rerun-from-here, halt-strategy) are explicitly stubs flagged in the scope boundaries — they are intentional, not "TBD".

---

## Execution handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-17-agent-run-observability-ui-implementation-plan.md`. Two execution options:

**1. Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration. Best for this plan since most tasks are self-contained component builds with clear TDD red→green→commit cycles.

**2. Inline Execution** — Execute tasks in this session using `executing-plans`, batch execution with checkpoints. Slower but lets you watch each step.

**Which approach?**

---

# DESIGN ALIGNMENT — additions and overrides

This section absorbs the design prototype at `docs/superpowers/designs/2026-05-17-agent-run-observability/`. Where it contradicts an earlier task, the design wins. The new Phase 2.5 is the filter-bar family the user asked for; Phase 4 gets an Inspector rewrite; Phase 5 gets a topbar mode toggle.

## Override: SpanInspector rewrite (replaces Task 2.1)

The prototype's Inspector promotes prompt / response / tool args / tool result to **first-class pull-quote blocks** at the top, with a compact `FIELDS` list below. The earlier Task 2.1 used only the field list — discard that body and re-implement against the prototype.

**Files:**
- Create: `frontend/web/src/features/agent-runs/PullQuote.tsx`
- Replace: `frontend/web/src/features/agent-runs/SpanInspector.tsx`
- Update tests accordingly.

**Reference:** `designs/2026-05-17-agent-run-observability/flame.jsx` lines 115–244 (the `window.Inspector` block). Recreate the visual structure 1:1.

- [ ] **Step 1: Add prompt/response/args/result fields to types**

Extend `frontend/web/src/api/types-agent-runs.ts`:

```typescript
export type RunSpan = {
  // ... existing fields
  // Prototype-driven extensions: these live in `attributes` server-side but
  // surface as first-class so the inspector can render them as pull-quotes.
  prompt?: string;
  response?: string;
  response_partial?: string;
  args?: unknown;
  result?: unknown;
  decision_idx?: number;
  provider?: string;
  model?: string;
  hash?: string;
  tokens_in?: number;
  tokens_out?: number;
  cost?: number;
  streaming?: boolean;
};
```

If the backend ends up storing these inside `attributes`, add a `normalizeSpan()` helper in `api/agent-runs.ts` that hoists known keys out of `attributes` into the top-level fields the inspector reads.

- [ ] **Step 2: Write PullQuote**

```typescript
// frontend/web/src/features/agent-runs/PullQuote.tsx
import type { ReactNode } from "react";

export function PullQuote({
  label,
  body,
  accent = "var(--gold)",
  glyph = "“",
  italic = false,
  streaming = false,
}: {
  label: string;
  body: ReactNode;
  accent?: string;
  glyph?: string;
  italic?: boolean;
  streaming?: boolean;
}) {
  return (
    <div className="mt-3 first:mt-0">
      <div className="flex items-center justify-between mb-1">
        <span className="text-[9px] font-mono tracking-[0.18em] text-text-3">{label}</span>
        {streaming ? (
          <span className="text-[9px] font-mono tracking-[0.16em] animate-pulse" style={{ color: "var(--info)" }}>
            ● STREAMING
          </span>
        ) : null}
      </div>
      <div
        className="relative pl-3 pr-3 py-2 rounded-sm2"
        style={{ background: "var(--surface-elev)", borderLeft: `2px solid ${accent}` }}
      >
        <span
          className="absolute -top-1 left-1 text-[22px] leading-none font-serif select-none"
          style={{ color: accent, opacity: 0.45 }}
          aria-hidden
        >
          {glyph}
        </span>
        <div className={`pl-2 text-[12px] leading-relaxed ${italic ? "font-serif italic" : "font-mono"}`} style={{ color: "var(--text)" }}>
          {body}
          {streaming ? (
            <span
              className="inline-block w-1 h-3 align-middle ml-1 animate-pulse"
              style={{ background: "var(--info)" }}
              aria-hidden
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Replace SpanInspector with the prototype's structure**

The new Inspector renders, top to bottom:

1. Header strip — kind badge (colored), span name truncated, `STREAMING` pill on the right if `span.streaming && isLive`.
2. **Pull-quotes** in this order (skip those whose source field is absent):
   - `PROMPT` from `span.prompt` (accent = kind color, glyph `›`)
   - `RESPONSE` from `span.response` (accent = gold, glyph `“`, italic)
   - `RESPONSE (PARTIAL)` from `span.response_partial` (accent = info, glyph `“`, italic, streaming=true)
   - `TOOL ARGS` from `span.args` (accent = kind color, glyph `›`, body is `<pre>JSON.stringify(args, null, 2)</pre>`)
   - `TOOL RESULT` from `span.result` (accent = gold, glyph `←`, body is `<pre>JSON.stringify(result, null, 2)</pre>`)
3. `FIELDS` heading + 2-column `<Row>` list: `span.id`, `kind`, `duration`, `start`, `provider`, `model` (gold tone), `tokens.in`, `tokens.out`, `cost`, `prompt.hash`, `decision` (gold tone).
4. Footer with three action buttons (`Jump to decision #N` · `Rerun from here` (disabled when live, with `LOCKED · LIVE` tag) · `Copy span JSON`).

Use exact spacing from the prototype: inspector width `400px shrink-0`, header `px-3 py-2`, body `px-3 py-3 overflow-auto`, footer `p-2 grid grid-cols-1 gap-1` with each button `h-7 px-2 text-[11px] font-mono`.

Tests:

```typescript
describe("SpanInspector (with pull-quotes)", () => {
  test("renders PROMPT and RESPONSE pull-quotes when present", () => { /* ... */ });
  test("renders TOOL ARGS as preformatted JSON", () => { /* ... */ });
  test("RESPONSE (PARTIAL) shows STREAMING badge when live", () => { /* ... */ });
  test("rerun button shows `LOCKED · LIVE` and is disabled when isLive", () => { /* ... */ });
});
```

- [ ] **Step 4: Commit**

```bash
git add frontend/web/src/features/agent-runs/PullQuote.tsx \
        frontend/web/src/features/agent-runs/SpanInspector.tsx \
        frontend/web/src/features/agent-runs/SpanInspector.test.tsx \
        frontend/web/src/api/types-agent-runs.ts
git commit -m "feat(agent-runs): rewrite SpanInspector with PullQuote blocks (match design)"
```

---

## Phase 2.5 — Filter bar + DecisionJump (Logfire-style)

Goal: A compact 36px row that sits between the dock header and the body, providing free-text search, kind toggles, status toggle, decision-jump stepper, and a filtered/total counter. URL-shareable via `?q=`. Reference: `designs/2026-05-17-agent-run-observability/dock.jsx` lines 1–148 (the `KIND_DEF`, `DecisionJump`, `STATUS_DEF`, and `FilterBar` blocks).

**Files:**
- Create: `frontend/web/src/features/agent-runs/DecisionJump.tsx`
- Create: `frontend/web/src/features/agent-runs/DecisionJump.test.tsx`
- Create: `frontend/web/src/features/agent-runs/FilterBar.tsx`
- Create: `frontend/web/src/features/agent-runs/FilterBar.test.tsx`
- Create: `frontend/web/src/features/agent-runs/use-span-filter.ts`
- Create: `frontend/web/src/features/agent-runs/use-span-filter.test.ts`

### Task 2.5.1: useSpanFilter hook (state + URL sync + filter logic)

The hook owns: free-text `query`, kind `Set`, status enum, decisionFilter (`"all" | "<number>"`), and returns the filtered span list and a `summary` (filtered/total counts). It also serializes/restores from the URL `?q=` and persists to localStorage keyed by `run_id`.

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/use-span-filter.test.ts
import { describe, expect, test, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSpanFilter } from "./use-span-filter";
import { MOCK_RUN_COMPLETED } from "./mock-fixtures";

const allSpans = MOCK_RUN_COMPLETED.spans;
const runId = MOCK_RUN_COMPLETED.summary.run_id;

describe("useSpanFilter", () => {
  beforeEach(() => localStorage.clear());

  test("empty filter passes all spans", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    expect(result.current.filtered).toHaveLength(allSpans.length);
  });

  test("kind toggle narrows to that kind", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    act(() => result.current.toggleKind("model"));
    expect(result.current.filtered.every((s) => s.kind === "model.call")).toBe(true);
  });

  test("free-text `model:opus` filters by model field substring", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    act(() => result.current.setQuery("model:gpt-5"));
    expect(result.current.filtered.every((s) => (s.model || "").includes("gpt-5"))).toBe(true);
  });

  test("`tool:run_backtest` filters to tool spans with that name", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    act(() => result.current.setQuery("tool:run_backtest"));
    expect(result.current.filtered.every((s) => s.kind === "tool.call" && s.name.includes("run_backtest"))).toBe(true);
  });

  test("decision filter to `#14` matches only spans with decision_idx=14", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    act(() => result.current.setDecisionFilter("14"));
    expect(result.current.filtered.every((s) => String(s.decision_idx ?? "") === "14")).toBe(true);
  });

  test("state restored from localStorage on remount with same runId", () => {
    const { result, unmount } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    act(() => result.current.setQuery("model:gpt-5"));
    unmount();
    const { result: r2 } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    expect(r2.current.query).toBe("model:gpt-5");
  });
});
```

- [ ] **Step 2: Write the hook**

```typescript
// frontend/web/src/features/agent-runs/use-span-filter.ts
import { useEffect, useMemo, useState } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { categoryOf, type SpanCategory } from "./span-colors";

export type StatusFilter = "all" | "green" | "blue" | "amber" | "red";

type SerializedState = {
  q: string;
  k: SpanCategory[];
  s: StatusFilter;
  d: string;
};

function lsKey(runId: string): string {
  return `xvn.agent-runs.filter.${runId}`;
}

function loadInitial(runId: string): SerializedState {
  try {
    const url = new URL(window.location.href);
    const fromUrl = url.searchParams.get("q");
    if (fromUrl) {
      // URL form is the same shape as the search input itself plus optional
      // `kind:` + `status:` + `decision:` tokens; parser is lenient.
      return parseQueryString(fromUrl);
    }
    const raw = localStorage.getItem(lsKey(runId));
    if (raw) return JSON.parse(raw) as SerializedState;
  } catch {
    /* fall through */
  }
  return { q: "", k: [], s: "all", d: "all" };
}

function parseQueryString(qs: string): SerializedState {
  const out: SerializedState = { q: "", k: [], s: "all", d: "all" };
  const tokens = qs.split(/\s+/).filter(Boolean);
  const remaining: string[] = [];
  for (const tok of tokens) {
    if (tok.startsWith("kind:")) {
      const v = tok.slice(5) as SpanCategory;
      if (["agent", "model", "tool", "supervisor", "artifact"].includes(v)) out.k.push(v);
    } else if (tok.startsWith("status:")) {
      const v = tok.slice(7) as StatusFilter;
      if (["green", "blue", "amber", "red", "all"].includes(v)) out.s = v;
    } else if (tok.startsWith("decision:")) {
      out.d = tok.slice(9);
    } else {
      remaining.push(tok);
    }
  }
  out.q = remaining.join(" ");
  return out;
}

function serialize(s: SerializedState): string {
  const parts: string[] = [];
  if (s.q) parts.push(s.q);
  s.k.forEach((k) => parts.push(`kind:${k}`));
  if (s.s !== "all") parts.push(`status:${s.s}`);
  if (s.d !== "all") parts.push(`decision:${s.d}`);
  return parts.join(" ");
}

export function useSpanFilter({ runId, spans }: { runId: string; spans: RunSpan[] }) {
  const initial = useMemo(() => loadInitial(runId), [runId]);
  const [query, setQuery] = useState(initial.q);
  const [kinds, setKinds] = useState<Set<SpanCategory>>(new Set(initial.k));
  const [status, setStatus] = useState<StatusFilter>(initial.s);
  const [decisionFilter, setDecisionFilter] = useState<string>(initial.d);

  // Persist + URL sync (debounced via microtask).
  useEffect(() => {
    const state: SerializedState = { q: query, k: [...kinds], s: status, d: decisionFilter };
    const qs = serialize(state);
    try {
      localStorage.setItem(lsKey(runId), JSON.stringify(state));
      const url = new URL(window.location.href);
      if (qs) url.searchParams.set("q", qs);
      else url.searchParams.delete("q");
      window.history.replaceState({}, "", url.toString());
    } catch {
      /* swallow */
    }
  }, [runId, query, kinds, status, decisionFilter]);

  const toggleKind = (k: SpanCategory) =>
    setKinds((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return spans.filter((s) => {
      const cat = categoryOf(s.kind);
      if (kinds.size > 0 && !kinds.has(cat)) return false;
      if (decisionFilter !== "all" && String(s.decision_idx ?? "") !== decisionFilter) return false;
      if (!q) return true;
      const tokens = q.split(/\s+/);
      return tokens.every((tok) => {
        if (tok.startsWith("title:")) return s.name.toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("model:")) return (s.model ?? "").toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("tool:"))  return cat === "tool" && s.name.toLowerCase().includes(tok.slice(5));
        if (tok.startsWith("agent:")) return cat === "agent" && s.name.toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("decision:")) return String(s.decision_idx ?? "") === tok.slice(9);
        return (
          s.name.toLowerCase().includes(tok) ||
          (s.model ?? "").toLowerCase().includes(tok) ||
          (s.provider ?? "").toLowerCase().includes(tok) ||
          String(s.decision_idx ?? "").includes(tok)
        );
      });
    });
  }, [spans, query, kinds, decisionFilter]);

  return {
    query, setQuery,
    kinds, toggleKind,
    status, setStatus,
    decisionFilter, setDecisionFilter,
    filtered,
    summary: { total: spans.length, filtered: filtered.length },
  };
}
```

- [ ] **Step 3: Verify test passing + commit**

```bash
pnpm --filter xvision-web test -- use-span-filter
git add frontend/web/src/features/agent-runs/use-span-filter.ts \
        frontend/web/src/features/agent-runs/use-span-filter.test.ts
git commit -m "feat(agent-runs): useSpanFilter with URL/localStorage sync + free-text DSL"
```

### Task 2.5.2: DecisionJump component

A compact number-stepper for the decision filter. Number input + prev/next arrows + position counter (`3/8` or `of 8`) + clear button. Keyboard: Enter commits, ArrowUp/Down steps, Escape clears. **Designed for thousands of decisions** — snaps typed numbers to the nearest valid decision id.

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/DecisionJump.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { DecisionJump } from "./DecisionJump";

const decisions = [11, 12, 13, 14, 15, 16, 17, 18].map((i) => ({ i }));

describe("DecisionJump", () => {
  test('renders "of N" when inactive', () => {
    render(<DecisionJump value="all" onChange={() => {}} decisions={decisions} />);
    expect(screen.getByText(/of 8/)).toBeInTheDocument();
  });

  test('renders "k/N" position when active', () => {
    render(<DecisionJump value="14" onChange={() => {}} decisions={decisions} />);
    expect(screen.getByText(/4\/8/)).toBeInTheDocument();
  });

  test("Enter commits the typed value (snaps to nearest existing decision)", async () => {
    const onChange = vi.fn();
    render(<DecisionJump value="all" onChange={onChange} decisions={decisions} />);
    const input = screen.getByPlaceholderText("—");
    await userEvent.type(input, "99{Enter}");
    expect(onChange).toHaveBeenLastCalledWith("18"); // nearest to 99
  });

  test("ArrowUp / ArrowDown steps through decisions", async () => {
    const onChange = vi.fn();
    render(<DecisionJump value="14" onChange={onChange} decisions={decisions} />);
    const input = screen.getByPlaceholderText("—");
    input.focus();
    await userEvent.keyboard("{ArrowUp}");
    expect(onChange).toHaveBeenLastCalledWith("15");
    await userEvent.keyboard("{ArrowDown}{ArrowDown}");
    expect(onChange).toHaveBeenLastCalledWith("13");
  });

  test("Escape clears the active filter", async () => {
    const onChange = vi.fn();
    render(<DecisionJump value="14" onChange={onChange} decisions={decisions} />);
    const input = screen.getByPlaceholderText("—");
    input.focus();
    await userEvent.keyboard("{Escape}");
    expect(onChange).toHaveBeenCalledWith("all");
  });
});
```

- [ ] **Step 2: Write the component (port `dock.jsx` lines 13–75)**

```typescript
// frontend/web/src/features/agent-runs/DecisionJump.tsx
import { useEffect, useState } from "react";

export type DecisionRef = { i: number };

export function DecisionJump({
  value,
  onChange,
  decisions,
}: {
  value: string; // "all" | "<int>"
  onChange: (next: string) => void;
  decisions: DecisionRef[];
}) {
  const ids = decisions.map((d) => d.i);
  const active = value !== "all";
  const curIdx = active ? ids.indexOf(parseInt(value, 10)) : -1;
  const [draft, setDraft] = useState("");

  useEffect(() => {
    setDraft(active ? String(value) : "");
  }, [value, active]);

  const commit = (raw: string) => {
    const n = parseInt(String(raw).replace(/[^0-9]/g, ""), 10);
    if (!Number.isFinite(n) || ids.length === 0) return;
    if (ids.includes(n)) onChange(String(n));
    else {
      const nearest = ids.reduce((a, b) => (Math.abs(b - n) < Math.abs(a - n) ? b : a), ids[0]!);
      onChange(String(nearest));
    }
  };

  const step = (delta: number) => {
    if (ids.length === 0) return;
    if (curIdx === -1) {
      onChange(String(ids[0]));
      return;
    }
    const next = Math.min(ids.length - 1, Math.max(0, curIdx + delta));
    onChange(String(ids[next]));
  };

  return (
    <div
      className="flex items-center gap-1 h-6 rounded-sm2 pl-1.5 pr-0.5"
      style={{ background: "var(--bg)", border: `1px solid ${active ? "var(--gold-soft)" : "var(--border)"}` }}
    >
      <span
        className="text-[10px] font-mono tracking-[0.16em] whitespace-nowrap"
        style={{ color: active ? "var(--gold-soft)" : "var(--text-4)" }}
      >
        DECISION&nbsp;#
      </span>
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value.replace(/[^0-9]/g, ""))}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit(draft);
          else if (e.key === "ArrowUp")   { e.preventDefault(); step(+1); }
          else if (e.key === "ArrowDown") { e.preventDefault(); step(-1); }
          else if (e.key === "Escape" && active) onChange("all");
        }}
        onBlur={() => { if (draft) commit(draft); }}
        placeholder="—"
        className="w-9 h-full bg-transparent text-[11px] font-mono tabular-nums outline-none"
        style={{ color: active ? "var(--gold)" : "var(--text)" }}
      />
      <button onClick={() => step(-1)} title="Prev decision" aria-label="prev decision"
        className="h-full w-5 flex items-center justify-center text-text-3 hover:text-text">
        ‹
      </button>
      <button onClick={() => step(+1)} title="Next decision" aria-label="next decision"
        className="h-full w-5 flex items-center justify-center text-text-3 hover:text-text">
        ›
      </button>
      <span className="text-[10px] font-mono text-text-4 px-1 tabular-nums whitespace-nowrap leading-none">
        {active ? `${curIdx + 1}/${ids.length}` : `of ${ids.length}`}
      </span>
      {active ? (
        <button onClick={() => onChange("all")} title="Clear decision filter"
          className="h-full w-5 flex items-center justify-center text-text-3 hover:text-danger text-[12px] leading-none">
          ×
        </button>
      ) : null}
    </div>
  );
}
```

For pixel-fidelity arrow icons, copy the inline SVG from `dock.jsx` instead of the `‹`/`›` glyphs above.

- [ ] **Step 3: Verify + commit**

```bash
pnpm --filter xvision-web test -- DecisionJump
git add frontend/web/src/features/agent-runs/DecisionJump.tsx \
        frontend/web/src/features/agent-runs/DecisionJump.test.tsx
git commit -m "feat(agent-runs): DecisionJump number stepper (snap to nearest, keyboard nav)"
```

### Task 2.5.3: FilterBar component

The 36px row: search input (left, flex-1, max-380px), 5 kind chips, vertical divider, 4 status icon-buttons (single-select), vertical divider, DecisionJump, then a right-aligned counter `<filtered>/<total> spans`.

- [ ] **Step 1: Write the failing test**

```typescript
// frontend/web/src/features/agent-runs/FilterBar.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { FilterBar } from "./FilterBar";

const baseProps = {
  query: "",
  setQuery: vi.fn(),
  kinds: new Set<string>(),
  toggleKind: vi.fn(),
  status: "all" as const,
  setStatus: vi.fn(),
  decisionFilter: "all",
  setDecisionFilter: vi.fn(),
  decisions: [{ i: 14 }],
  total: 12,
  filtered: 5,
};

describe("FilterBar", () => {
  test("renders the search input with placeholder hint", () => {
    render(<FilterBar {...baseProps} />);
    expect(
      screen.getByPlaceholderText(/title:agent\.plan/i),
    ).toBeInTheDocument();
  });

  test("renders 5 kind chips: AGENT MODEL TOOL SUPER ARTIF", () => {
    render(<FilterBar {...baseProps} />);
    ["AGENT", "MODEL", "TOOL", "SUPER", "ARTIF"].forEach((label) => {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    });
  });

  test("clicking a kind chip calls toggleKind with that category", async () => {
    const props = { ...baseProps, toggleKind: vi.fn() };
    render(<FilterBar {...props} />);
    await userEvent.click(screen.getByRole("button", { name: "MODEL" }));
    expect(props.toggleKind).toHaveBeenCalledWith("model");
  });

  test("counter shows `<filtered>/<total> spans`", () => {
    render(<FilterBar {...baseProps} />);
    expect(screen.getByText(/5/)).toBeInTheDocument();
    expect(screen.getByText(/12/)).toBeInTheDocument();
    expect(screen.getByText(/spans/)).toBeInTheDocument();
  });

  test("typing in the search input calls setQuery", async () => {
    const setQuery = vi.fn();
    render(<FilterBar {...baseProps} setQuery={setQuery} />);
    await userEvent.type(screen.getByPlaceholderText(/title:agent\.plan/i), "tool:run_backtest");
    expect(setQuery).toHaveBeenLastCalledWith("tool:run_backtest");
  });
});
```

- [ ] **Step 2: Write the component (port `dock.jsx` lines 84–148)**

```typescript
// frontend/web/src/features/agent-runs/FilterBar.tsx
import type { Dispatch, SetStateAction } from "react";
import { DecisionJump, type DecisionRef } from "./DecisionJump";
import { CATEGORY_STYLES, type SpanCategory } from "./span-colors";
import type { StatusFilter } from "./use-span-filter";

const KIND_ORDER: SpanCategory[] = ["agent", "model", "tool", "supervisor", "artifact"];

const STATUS_DEF: Array<{ k: StatusFilter; glyph: string; tint: string; bg: string; bd: string }> = [
  { k: "green", glyph: "✓", tint: "var(--gold)",   bg: "var(--gold-bg)",         bd: "var(--gold-soft)" },
  { k: "blue",  glyph: "▶", tint: "var(--info)",   bg: "rgba(111,143,184,0.14)", bd: "rgba(111,143,184,0.45)" },
  { k: "amber", glyph: "⚠", tint: "var(--warn)",   bg: "rgba(219,146,48,0.10)",  bd: "rgba(219,146,48,0.45)" },
  { k: "red",   glyph: "✕", tint: "var(--danger)", bg: "rgba(200,68,58,0.10)",   bd: "rgba(200,68,58,0.45)" },
];

export function FilterBar({
  query, setQuery,
  kinds, toggleKind,
  status, setStatus,
  decisionFilter, setDecisionFilter,
  decisions,
  total, filtered,
}: {
  query: string;
  setQuery: Dispatch<SetStateAction<string>> | ((v: string) => void);
  kinds: Set<SpanCategory>;
  toggleKind: (k: SpanCategory) => void;
  status: StatusFilter;
  setStatus: (s: StatusFilter) => void;
  decisionFilter: string;
  setDecisionFilter: (d: string) => void;
  decisions: DecisionRef[];
  total: number;
  filtered: number;
}) {
  return (
    <div
      className="h-9 px-2 flex items-center gap-2 shrink-0 overflow-hidden"
      style={{ borderBottom: "1px solid var(--border)", background: "var(--surface-elev)" }}
    >
      {/* search */}
      <div
        className="flex items-center gap-1.5 h-6 px-2 rounded-sm2 flex-1 min-w-[200px] max-w-[380px]"
        style={{ background: "var(--bg)", border: "1px solid var(--border)" }}
      >
        <svg width="10" height="10" viewBox="0 0 16 16" fill="none" aria-hidden>
          <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4" />
          <path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
        <input
          value={query}
          onChange={(e) => (setQuery as (v: string) => void)(e.target.value)}
          placeholder='filter   title:agent.plan   model:gpt-5   tool:run_backtest'
          className="flex-1 bg-transparent text-[11px] font-mono text-text outline-none placeholder:text-text-4 min-w-0"
        />
        {query ? (
          <button onClick={() => (setQuery as (v: string) => void)("")} className="text-text-3 hover:text-text text-[10px] font-mono">
            ×
          </button>
        ) : null}
      </div>

      {/* kind chips */}
      <div className="flex items-center gap-0.5 shrink-0">
        {KIND_ORDER.map((k) => {
          const on = kinds.has(k);
          const style = CATEGORY_STYLES[k];
          return (
            <button
              key={k}
              onClick={() => toggleKind(k)}
              className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] rounded-sm2 flex items-center gap-1"
              style={{
                background: on ? "var(--surface-card)" : "transparent",
                border: `1px solid ${on ? style.hex : "var(--border)"}`,
                color: on ? style.hex : "var(--text-3)",
              }}
            >
              <span className="w-1.5 h-1.5 inline-block" style={{ background: style.hex, opacity: on ? 1 : 0.5 }} />
              {style.label}
            </button>
          );
        })}
      </div>

      <div className="w-px h-4 shrink-0" style={{ background: "var(--border)" }} />

      {/* status icons (single-select; click again to clear back to "all") */}
      <div className="flex items-center gap-0.5 shrink-0">
        {STATUS_DEF.map((s) => {
          const on = status === s.k;
          return (
            <button
              key={s.k}
              onClick={() => setStatus(on ? "all" : s.k)}
              title={s.k.toUpperCase()}
              aria-label={`status: ${s.k}`}
              className="h-6 w-6 text-[10px] font-mono rounded-sm2 flex items-center justify-center"
              style={{
                background: on ? s.bg : "transparent",
                border: `1px solid ${on ? s.bd : "var(--border)"}`,
                color: on ? s.tint : "var(--text-3)",
              }}
            >
              {s.glyph}
            </button>
          );
        })}
      </div>

      <div className="w-px h-4 shrink-0" style={{ background: "var(--border)" }} />

      <DecisionJump value={decisionFilter} onChange={setDecisionFilter} decisions={decisions} />

      <div className="ml-auto text-[10px] font-mono text-text-3 tabular-nums pr-1 shrink-0 whitespace-nowrap">
        <span className="text-text">{filtered}</span>
        <span className="text-text-4">/</span>
        <span>{total}</span> spans
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Verify + commit**

```bash
pnpm --filter xvision-web test -- FilterBar
git add frontend/web/src/features/agent-runs/FilterBar.tsx \
        frontend/web/src/features/agent-runs/FilterBar.test.tsx
git commit -m "feat(agent-runs): FilterBar with kind chips, status icons, search, decision-jump"
```

### Task 2.5.4: Wire FilterBar into TraceDock + agent-runs-detail route

- [ ] **Step 1: TraceDock integration**

In `TraceDock.tsx`, between the existing dock header and the body, insert:

```typescript
import { FilterBar } from "./FilterBar";
import { useSpanFilter } from "./use-span-filter";

// inside the component, after `q = useQuery({...})`:
const filter = useSpanFilter({
  runId: activeRunId ?? "",
  spans: q.data?.spans ?? [],
});

// derive decisions list from spans that have a decision_idx, deduped:
const decisions = useMemo(() => {
  const seen = new Set<number>();
  const out: { i: number }[] = [];
  for (const s of q.data?.spans ?? []) {
    if (s.decision_idx != null && !seen.has(s.decision_idx)) {
      seen.add(s.decision_idx);
      out.push({ i: s.decision_idx });
    }
  }
  return out.sort((a, b) => a.i - b.i);
}, [q.data]);

// in the JSX, immediately after the dock header <div> closes:
<FilterBar
  query={filter.query} setQuery={filter.setQuery}
  kinds={filter.kinds} toggleKind={filter.toggleKind}
  status={filter.status} setStatus={filter.setStatus}
  decisionFilter={filter.decisionFilter} setDecisionFilter={filter.setDecisionFilter}
  decisions={decisions}
  total={filter.summary.total} filtered={filter.summary.filtered}
/>

// pass `filter.filtered` (not `q.data.spans`) into <FlameGraph spans=... />
```

- [ ] **Step 2: Hide FilterBar at peek height**

In the dock body switch, render the FilterBar only when `height !== "peek"` — the peek body is flame-graph-only and there is no room for the bar.

- [ ] **Step 3: Mirror in agent-runs-detail route**

The dedicated route also benefits from the FilterBar. Mount it at the top of the split-pane area (above the rail-tree / timeline / inspector grid) using the same `useSpanFilter` hook so the URL `?q=` parameter is shared between dock and route.

- [ ] **Step 4: Cross-link decisions table → decision filter**

In the eval-runs-detail decisions table, clicking a decision row should:

1. Open the dock if collapsed (`useTraceDock.getState().setHeight("working")`).
2. Set the decision filter (`filter.setDecisionFilter(String(decisionIndex))`).
3. Toast `Filtered dock to decision #N`.

This matches the prototype's `onJumpDecision` handler (`app.jsx` line 370).

- [ ] **Step 5: Run all tests + commit**

```bash
pnpm --filter xvision-web test
git add frontend/web/src/features/agent-runs/TraceDock.tsx \
        frontend/web/src/routes/agent-runs-detail.tsx \
        frontend/web/src/routes/eval-runs-detail.tsx
git commit -m "feat(agent-runs): mount FilterBar in dock + route; decision-row click filters dock"
```

---

## Phase 5 — TopbarModeToggle (POST-HOC ⇄ LIVE)

The prototype's Topbar carries a POST-HOC ⇄ LIVE pill toggle (`app.jsx` lines 37–60). This is the operator's way to switch between watching a live run and inspecting it post-hoc. Folio-dark, two pills sharing a container, the active one tinted gold (post-hoc) or info-blue (live, with pulsing dot).

**Files:**
- Create: `frontend/web/src/features/agent-runs/TopbarModeToggle.tsx`
- Modify: `frontend/web/src/components/shell/Topbar.tsx`

- [ ] **Step 1: Write the component** (port `app.jsx` lines 37–60)

The toggle reads/writes `useTraceDock().mode`. When mode flips to `live`, the strip starts pulsing blue; when it flips to `post-hoc`, it returns to gold/green.

```typescript
// frontend/web/src/features/agent-runs/TopbarModeToggle.tsx
import { useTraceDock } from "@/stores/trace-dock";

export function TopbarModeToggle() {
  const { mode, setActiveRun, activeRunId } = useTraceDock();
  if (!activeRunId) return null;
  const isLive = mode === "live";
  const set = (next: "live" | "post-hoc") => setActiveRun(activeRunId, next);
  return (
    <div
      className="flex items-center gap-1 p-0.5 rounded-sm2"
      style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}
    >
      <button
        type="button"
        onClick={() => set("post-hoc")}
        className="h-6 px-2.5 text-[10px] font-mono tracking-[0.16em] rounded-sm2"
        style={{
          background: !isLive ? "var(--gold-bg)" : "transparent",
          color: !isLive ? "var(--gold)" : "var(--text-3)",
        }}
      >
        POST-HOC
      </button>
      <span className="text-text-4 text-[10px] px-0.5">⇄</span>
      <button
        type="button"
        onClick={() => set("live")}
        className="h-6 px-2.5 text-[10px] font-mono tracking-[0.16em] rounded-sm2 flex items-center gap-1.5"
        style={{
          background: isLive ? "rgba(111,143,184,0.18)" : "transparent",
          color: isLive ? "#bcd1ea" : "var(--text-3)",
          border: isLive ? "1px solid rgba(111,143,184,0.45)" : "1px solid transparent",
        }}
      >
        {isLive ? <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: "var(--info)" }} /> : null}
        LIVE
      </button>
    </div>
  );
}
```

- [ ] **Step 2: Mount in Topbar**

Add `<TopbarModeToggle />` to the right side of the existing `<Topbar />` component so it appears on every page. It hides itself when no `activeRunId` is in the store.

- [ ] **Step 3: Commit**

```bash
git add frontend/web/src/features/agent-runs/TopbarModeToggle.tsx \
        frontend/web/src/components/shell/Topbar.tsx
git commit -m "feat(agent-runs): TopbarModeToggle (POST-HOC ⇄ LIVE) — match design prototype"
```

---

## Design alignment — final checklist (no extra tasks, just review gates)

Before declaring the plan complete:

- [ ] Strip and dock are mutually exclusive (`StripDockSlot` renders one or the other based on `height === "collapsed"`).
- [ ] Span-color palette has exactly **5 categories** with hex values from `dock.jsx` KIND_DEF; the helper `categoryOf(SpanKind)` maps the 10 backend kinds onto them.
- [ ] FilterBar height is **36px** (`h-9`); kind chips are **24px** (`h-6`); search input is **24px** (`h-6`) inside a 36px row.
- [ ] DecisionJump accepts thousands of decisions, supports Enter / ArrowUp / ArrowDown / Escape, and snaps typed values to the nearest existing decision id.
- [ ] Filter state serializes to `?q=` AND `localStorage.xvn.agent-runs.filter.<run_id>`; URL wins on initial load.
- [ ] FlameGraph receives `filter.filtered` (not raw spans) in dock + route.
- [ ] Clicking a decision row in the eval-runs-detail decisions table opens the dock and sets the decision filter.
- [ ] SpanInspector renders PullQuote blocks BEFORE the FIELDS list when prompt/response/args/result are present.
- [ ] TopbarModeToggle is mounted in `Topbar.tsx` and reads `useTraceDock().mode`.
- [ ] Design files are preserved in `docs/superpowers/designs/2026-05-17-agent-run-observability/` for future agents to reference.

