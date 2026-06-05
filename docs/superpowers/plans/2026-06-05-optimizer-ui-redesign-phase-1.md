# Optimizer UI Redesign — Phase 1 (Shell + Visual System + Terminology) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the Optimizer's five flat tabs (`Live / Genealogy / Diff / Ladder / Provenance`) with a three-screen drill-in hierarchy — **Optimizer Home → Cycle → Experiment** — carrying the mockups' gold-on-dark visual primitives and lock-conformant terminology, wired to the data the backend already exposes. Panels needing future backend work render as honest empty-states.

**Architecture:** New React Router routes under `/optimizer` (with `/autooptimizer` redirecting in), three screen components that compose small panels, and a set of reusable themed UI primitives. No backend changes in Phase 1 — every screen consumes the existing `/api/autooptimizer/*` hooks in `features/autooptimizer/api.ts`. Empty-state panels (`<EmptyPanel kind="regime-matrix" …/>`) mark where Phases 2–4 will mount real data.

**Tech Stack:** React 18, React Router v6 (`createBrowserRouter`), TanStack Query, Tailwind (theme tokens exposed as classes: `text-gold`, `text-text-3`, `bg-surface-card`, `border-border`…), Geist / Geist Mono (already installed), Vitest + React Testing Library + jsdom.

**Spec:** `docs/superpowers/specs/2026-06-05-optimizer-ui-redesign-design.md` (§4 IA, §5 terminology, §6 visual, §8 phasing).

**Conventions to follow (verified in-repo):**
- Tests run with `pnpm -C frontend/web test` (script `vitest run`); typecheck `pnpm -C frontend/web typecheck`; build `pnpm -C frontend/web build`. Setup file `src/test-setup.ts` already stubs `localStorage`, `matchMedia`, `EventSource`.
- Route elements are wrapped with the existing `page(...)` helper in `src/routes.tsx`.
- Detail/inspector routes pass `<Topbar back={{ to, label }} … />` for the back affordance.
- Operator-facing labels go through the existing `formatLineageStatus` / `formatGateVerdict` helpers in `api.ts`. Never display raw `quarantined` / `rejected:<reason>`.
- Layout rules: single full-width column (`space-y-5`), **no** `grid-cols-12 … col-span-4` right sidebar, **no** popups/modals/sheets. Inline/dock/accordion only.
- Codename stays `autooptimizer` in code/paths/API; operator surface says **Optimizer**. Never collapse to bare `optimizer` in code (DSPy owns that token).

**Test wrapper (referenced by multiple tasks).** Create it first in Task 1 so later tasks import it:
`src/features/autooptimizer/test-utils.tsx` exports `renderWithProviders(ui, { route })` — wraps in a fresh `QueryClient` (retries off) + `MemoryRouter`.

---

## File structure (created/modified in Phase 1)

```
frontend/web/src/features/autooptimizer/
  test-utils.tsx                 (new)  — renderWithProviders helper
  api.ts                         (mod)  — add getCycleRun + cycle_id lineage filter + types
  ui/
    HashSigil.tsx                (new)  — deterministic identicon from a hash
    GateBadge.tsx                (new)  — Kept / Suspect / Dropped / Pending badge
    ExperimentPill.tsx           (new)  — experiment-kind pill
    ProgressDial.tsx             (new)  — circular progress dial
    Breadcrumb.tsx               (new)  — Optimizer › cycle › experiment trail
    EmptyPanel.tsx               (new)  — "lights up in Phase N" placeholder panel
    *.test.tsx                   (new)  — one render test per primitive
  panels/
    ExperimentWritersPanel.tsx   (new)  — merged Ladder + Provenance
    RecentCyclesTable.tsx        (new)  — recent cycles, rows link to Cycle screen
    ActiveLineagesGrid.tsx       (new)  — lineage cards
    CycleExperimentsTable.tsx    (new)  — per-cycle experiments (genealogy slice)
    ParentDiffPanel.tsx          (new)  — inline parent→child diff (from DiffInspector)
    *.test.tsx                   (new)
  screens/
    OptimizerHome.tsx            (new)  — /optimizer
    CycleDetail.tsx              (new)  — /optimizer/cycle/:cycleId
    ExperimentDetail.tsx         (new)  — /optimizer/experiment/:hash
    *.test.tsx                   (new)
  AutoOptimizerLayout.tsx        (DELETE at end — replaced by screens)

frontend/web/src/
  routes.tsx                     (mod)  — add /optimizer subtree + /autooptimizer redirect
  components/shell/Sidebar.tsx   (mod)  — nav entry /autooptimizer → /optimizer
  routes.test.tsx                (mod, if it pins the old path)
```

Reused as-is (no reproduction here): `DiffInspector.tsx` logic (extracted into `ParentDiffPanel`), `GenealogyTree.tsx` (becomes the inline lineage-tree view on Cycle), `LiveCycleView.tsx` (embedded as the in-flight section of Home), `ExperimentWriterLadder.tsx` / `LadderWithProvenance.tsx` (their table logic folds into `ExperimentWritersPanel`).

---

## Task 1: Test wrapper + API additions (cycle detail + cycle-filtered lineage)

**Files:**
- Create: `src/features/autooptimizer/test-utils.tsx`
- Modify: `src/features/autooptimizer/api.ts`
- Test: `src/features/autooptimizer/api.test.ts`

- [ ] **Step 1: Write the test wrapper helper**

Create `src/features/autooptimizer/test-utils.tsx`:

```tsx
import type { ReactElement, ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { render } from "@testing-library/react";

export function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false, gcTime: 0 } },
  });
}

export function renderWithProviders(
  ui: ReactElement,
  opts: { route?: string } = {},
) {
  const client = makeClient();
  const wrapper = ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>
      <MemoryRouter initialEntries={[opts.route ?? "/"]}>{children}</MemoryRouter>
    </QueryClientProvider>
  );
  return render(ui, { wrapper });
}
```

- [ ] **Step 2: Write the failing test for the new API additions**

Append to a new file `src/features/autooptimizer/api.test.ts`:

```ts
import { describe, expect, it, vi, afterEach } from "vitest";
import { getCycleRun, listLineageNodes, type CycleRunDetail } from "./api";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("autooptimizer api additions", () => {
  it("getCycleRun fetches the per-cycle detail endpoint", async () => {
    const spy = vi
      .spyOn(client, "apiFetch")
      .mockResolvedValue({ cycle_id: "cyc-1", nodes: [] } as CycleRunDetail);
    await getCycleRun("cyc 1");
    expect(spy).toHaveBeenCalledWith(
      "/api/autooptimizer/cycles/cyc%201",
    );
  });

  it("listLineageNodes forwards a cycle_id filter", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue([]);
    await listLineageNodes({ cycleId: "cyc-1" });
    expect(spy).toHaveBeenCalledWith(
      "/api/autooptimizer/lineage?cycle_id=cyc-1",
    );
  });
});
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- api.test`
Expected: FAIL — `getCycleRun` / `CycleRunDetail` not exported; `listLineageNodes` takes no args.

- [ ] **Step 4: Implement the API additions**

In `src/features/autooptimizer/api.ts`:

1. Add the detail type after `CycleRunSummary` (line ~124):

```ts
/** Full detail for one cycle: summary fields + its lineage nodes + honesty check. */
export type CycleRunDetail = CycleRunSummary & {
  nodes: LineageNode[];
  honesty_check?: {
    passed: boolean;
    sabotage_variant: string;
    message: string;
  } | null;
};
```

2. Replace `listLineageNodes` (line ~108) with a filtered version:

```ts
export type LineageQuery = { cycleId?: string; status?: LineageStatus; limit?: number };

export async function listLineageNodes(q?: LineageQuery): Promise<LineageNode[]> {
  const params = new URLSearchParams();
  if (q?.cycleId) params.set("cycle_id", q.cycleId);
  if (q?.status) params.set("status", q.status);
  if (q?.limit != null) params.set("limit", String(q.limit));
  const qs = params.toString();
  return apiFetch<LineageNode[]>(
    qs ? `/api/autooptimizer/lineage?${qs}` : "/api/autooptimizer/lineage",
  );
}

export async function getCycleRun(cycleId: string): Promise<CycleRunDetail> {
  return apiFetch<CycleRunDetail>(
    `/api/autooptimizer/cycles/${encodeURIComponent(cycleId)}`,
  );
}
```

3. Update the `useLineageNodes` hook (line ~171) to accept the optional query and extend the key:

```ts
export function useLineageNodes(q?: LineageQuery) {
  return useQuery({
    queryKey: [...autooptimizerKeys.lineage(), q ?? {}],
    queryFn: () => listLineageNodes(q),
    staleTime: 30_000,
  });
}
```

4. Add a cycle-detail hook + key after `useCycleRuns` (line ~186):

```ts
export const cycleRunKey = (id: string) =>
  [...autooptimizerKeys.cycles(), id] as const;

export function useCycleRun(cycleId: string | undefined) {
  return useQuery({
    queryKey: cycleRunKey(cycleId ?? ""),
    queryFn: () => getCycleRun(cycleId!),
    enabled: !!cycleId,
    staleTime: 30_000,
  });
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- api.test`
Expected: PASS (2 tests).

- [ ] **Step 6: Typecheck (existing callers of `useLineageNodes()` still compile — it's now optional-arg)**

Run: `pnpm -C frontend/web typecheck`
Expected: no errors.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/features/autooptimizer/test-utils.tsx \
        frontend/web/src/features/autooptimizer/api.ts \
        frontend/web/src/features/autooptimizer/api.test.ts
git commit -m "feat(optimizer): add cycle-detail + cycle-filtered lineage API + test wrapper"
```

---

## Task 2: `HashSigil` primitive (gen-art replacement)

A deterministic identicon derived from a `bundle_hash`, used everywhere the mockups used gen-art.

**Files:**
- Create: `src/features/autooptimizer/ui/HashSigil.tsx`
- Test: `src/features/autooptimizer/ui/HashSigil.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { HashSigil } from "./HashSigil";

describe("HashSigil", () => {
  it("renders a deterministic svg for a hash", () => {
    const { container, rerender } = render(<HashSigil hash="abc123" size={48} />);
    const svg = container.querySelector("svg");
    expect(svg).toBeTruthy();
    expect(svg).toHaveAttribute("width", "48");
    const first = container.innerHTML;
    rerender(<HashSigil hash="abc123" size={48} />);
    expect(container.innerHTML).toBe(first); // same hash → identical render
  });

  it("renders differently for a different hash", () => {
    const a = render(<HashSigil hash="aaaa" size={32} />).container.innerHTML;
    const b = render(<HashSigil hash="zzzz" size={32} />).container.innerHTML;
    expect(a).not.toBe(b);
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- HashSigil`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `HashSigil`**

```tsx
// Deterministic 5×5 mirrored identicon from a content hash. Replaces the
// gen-art thumbnails from the design mockups (gen-art pipeline is out of
// scope for this redesign — see the design spec §2 non-goals).

function hashToInt(hash: string): number {
  let h = 2166136261;
  for (let i = 0; i < hash.length; i++) {
    h ^= hash.charCodeAt(i);
    h = Math.imul(h, 16777619);
  }
  return h >>> 0;
}

// Gold-family hues so the sigil sits in the optimizer palette.
const HUES = [42, 38, 46, 30, 50];

export function HashSigil({
  hash,
  size = 40,
}: {
  hash: string;
  size?: number;
}) {
  const seed = hashToInt(hash || "•");
  const hue = HUES[seed % HUES.length];
  const fg = `hsl(${hue} 80% 58%)`;
  const cells = 5;
  const unit = size / cells;
  const rects: React.ReactNode[] = [];
  // Build a left half (3 cols) and mirror it for visual symmetry.
  for (let row = 0; row < cells; row++) {
    for (let col = 0; col < 3; col++) {
      const on = ((seed >> (row * 3 + col)) & 1) === 1;
      if (!on) continue;
      const mirror = cells - 1 - col;
      for (const c of new Set([col, mirror])) {
        rects.push(
          <rect
            key={`${row}-${c}`}
            x={c * unit}
            y={row * unit}
            width={unit}
            height={unit}
            fill={fg}
          />,
        );
      }
    }
  }
  return (
    <svg
      width={size}
      height={size}
      viewBox={`0 0 ${size} ${size}`}
      role="img"
      aria-label={`identity ${hash.slice(0, 8)}`}
      className="rounded border border-border bg-surface-elev"
    >
      {rects}
    </svg>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- HashSigil`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/ui/HashSigil.tsx \
        frontend/web/src/features/autooptimizer/ui/HashSigil.test.tsx
git commit -m "feat(optimizer): HashSigil deterministic identity primitive"
```

---

## Task 3: `GateBadge` primitive (Kept / Suspect / Dropped / Pending)

**Files:**
- Create: `src/features/autooptimizer/ui/GateBadge.tsx`
- Test: `src/features/autooptimizer/ui/GateBadge.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { GateBadge } from "./GateBadge";

describe("GateBadge", () => {
  it("renders the Kept label for an active node", () => {
    render(<GateBadge verdict="Accepted" status="active" />);
    expect(screen.getByText("Kept")).toBeInTheDocument();
  });
  it("renders Dropped for a rejected node", () => {
    render(<GateBadge verdict="Rejected" status="rejected" />);
    expect(screen.getByText("Dropped")).toBeInTheDocument();
  });
  it("renders Suspect for a quarantined node", () => {
    render(<GateBadge verdict="Suspect" status="quarantined" />);
    expect(screen.getByText("Suspect")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- GateBadge`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `GateBadge`**

The badge maps operator verdict/status to one of three buckets per the spec §5:
**Kept** (active/Accepted), **Suspect** (quarantined), **Dropped** (rejected). Anything
else → **Pending**.

```tsx
import type { LineageStatus } from "../api";

type Bucket = "Kept" | "Suspect" | "Dropped" | "Pending";

function bucketOf(verdict: string, status?: LineageStatus): Bucket {
  if (status === "quarantined" || verdict === "Suspect") return "Suspect";
  if (status === "active" || verdict === "Accepted") return "Kept";
  if (status === "rejected" || verdict === "Rejected") return "Dropped";
  return "Pending";
}

const STYLE: Record<Bucket, string> = {
  Kept: "text-gold border-gold/40 bg-gold/[0.10]",
  Suspect: "text-warn border-warn/40 bg-warn/[0.10]",
  Dropped: "text-danger border-danger/40 bg-danger/[0.10]",
  Pending: "text-text-3 border-border bg-surface-elev",
};

export function GateBadge({
  verdict,
  status,
}: {
  verdict: string;
  status?: LineageStatus;
}) {
  const bucket = bucketOf(verdict, status);
  return (
    <span
      className={`inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-wide border ${STYLE[bucket]}`}
    >
      {bucket}
    </span>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- GateBadge`
Expected: PASS (3 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/ui/GateBadge.tsx \
        frontend/web/src/features/autooptimizer/ui/GateBadge.test.tsx
git commit -m "feat(optimizer): GateBadge Kept/Suspect/Dropped primitive"
```

---

## Task 4: `ExperimentPill` primitive (experiment kind)

The optimizer doesn't yet emit a structured "kind" per experiment, so derive it from the diff
in later phases; for Phase 1 the pill takes an explicit `kind` string and renders a colored dot
+ label, defaulting to "Experiment".

**Files:**
- Create: `src/features/autooptimizer/ui/ExperimentPill.tsx`
- Test: `src/features/autooptimizer/ui/ExperimentPill.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ExperimentPill } from "./ExperimentPill";

describe("ExperimentPill", () => {
  it("renders the kind label", () => {
    render(<ExperimentPill kind="Prompt tweak" />);
    expect(screen.getByText("Prompt tweak")).toBeInTheDocument();
  });
  it("defaults to Experiment", () => {
    render(<ExperimentPill />);
    expect(screen.getByText("Experiment")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- ExperimentPill`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `ExperimentPill`**

```tsx
const TONE: Record<string, string> = {
  "Prompt tweak": "text-info border-info/40",
  "Threshold tune": "text-violet border-violet/40",
  "Agent +": "text-gold border-gold/40",
  "Agent −": "text-warn border-warn/40",
  "Model swap": "text-violet border-violet/40",
  "Regime detect swap": "text-info border-info/40",
  Experiment: "text-text-3 border-border",
};

export function ExperimentPill({ kind = "Experiment" }: { kind?: string }) {
  const tone = TONE[kind] ?? TONE.Experiment;
  return (
    <span
      className={`inline-flex items-center gap-1.5 rounded px-1.5 py-0.5 font-mono text-[10px] border ${tone}`}
    >
      <span className="h-1 w-1 rounded-full bg-current" aria-hidden />
      {kind}
    </span>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- ExperimentPill`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/ui/ExperimentPill.tsx \
        frontend/web/src/features/autooptimizer/ui/ExperimentPill.test.tsx
git commit -m "feat(optimizer): ExperimentPill kind primitive"
```

---

## Task 5: `ProgressDial` primitive

**Files:**
- Create: `src/features/autooptimizer/ui/ProgressDial.tsx`
- Test: `src/features/autooptimizer/ui/ProgressDial.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { ProgressDial } from "./ProgressDial";

describe("ProgressDial", () => {
  it("shows the rounded percentage and clamps to 0..1", () => {
    render(<ProgressDial value={0.42} label="CYCLE" />);
    expect(screen.getByText("42%")).toBeInTheDocument();
    expect(screen.getByText("CYCLE")).toBeInTheDocument();
  });
  it("clamps out-of-range values", () => {
    render(<ProgressDial value={1.8} />);
    expect(screen.getByText("100%")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- ProgressDial`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `ProgressDial`**

```tsx
export function ProgressDial({
  value,
  size = 64,
  stroke = 6,
  label,
}: {
  value: number;
  size?: number;
  stroke?: number;
  label?: string;
}) {
  const pct = Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0));
  const r = (size - stroke) / 2;
  const c = 2 * Math.PI * r;
  return (
    <div className="relative inline-flex items-center justify-center" style={{ width: size, height: size }}>
      <svg width={size} height={size} className="-rotate-90">
        <circle cx={size / 2} cy={size / 2} r={r} fill="none" stroke="var(--border-strong, #333)" strokeWidth={stroke} />
        <circle
          cx={size / 2}
          cy={size / 2}
          r={r}
          fill="none"
          className="text-gold"
          stroke="currentColor"
          strokeWidth={stroke}
          strokeDasharray={c}
          strokeDashoffset={c * (1 - pct)}
          strokeLinecap="round"
        />
      </svg>
      <div className="absolute flex flex-col items-center leading-none">
        <span className="font-mono text-[13px] font-semibold text-gold">{Math.round(pct * 100)}%</span>
        {label ? <span className="mt-0.5 text-[8.5px] uppercase tracking-widest text-text-3">{label}</span> : null}
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- ProgressDial`
Expected: PASS (2 tests).

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/ui/ProgressDial.tsx \
        frontend/web/src/features/autooptimizer/ui/ProgressDial.test.tsx
git commit -m "feat(optimizer): ProgressDial primitive"
```

---

## Task 6: `Breadcrumb` + `EmptyPanel` primitives

**Files:**
- Create: `src/features/autooptimizer/ui/Breadcrumb.tsx`
- Create: `src/features/autooptimizer/ui/EmptyPanel.tsx`
- Test: `src/features/autooptimizer/ui/Breadcrumb.test.tsx`
- Test: `src/features/autooptimizer/ui/EmptyPanel.test.tsx`

- [ ] **Step 1: Write the failing tests**

`Breadcrumb.test.tsx`:

```tsx
import { describe, expect, it } from "vitest";
import { renderWithProviders } from "../test-utils";
import { screen } from "@testing-library/react";
import { Breadcrumb } from "./Breadcrumb";

describe("Breadcrumb", () => {
  it("renders crumbs with the last as current", () => {
    renderWithProviders(
      <Breadcrumb
        items={[
          { label: "OPTIMIZER", to: "/optimizer" },
          { label: "cycle" },
          { label: "cyc-1" },
        ]}
      />,
    );
    expect(screen.getByText("OPTIMIZER").closest("a")).toHaveAttribute("href", "/optimizer");
    expect(screen.getByText("cyc-1")).toBeInTheDocument();
  });
});
```

`EmptyPanel.test.tsx`:

```tsx
import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { EmptyPanel } from "./EmptyPanel";

describe("EmptyPanel", () => {
  it("renders the title and the phase hint", () => {
    render(<EmptyPanel title="Eval matrix" phase={2} hint="lights up when the regime matrix runs" />);
    expect(screen.getByText("Eval matrix")).toBeInTheDocument();
    expect(screen.getByText(/Phase 2/)).toBeInTheDocument();
    expect(screen.getByText(/regime matrix/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm -C frontend/web test -- Breadcrumb EmptyPanel`
Expected: FAIL — modules not found.

- [ ] **Step 3: Implement `Breadcrumb`**

```tsx
import { Link } from "react-router-dom";

export type Crumb = { label: string; to?: string };

export function Breadcrumb({ items }: { items: Crumb[] }) {
  return (
    <nav aria-label="Breadcrumb" className="mb-4 flex items-center gap-2 font-mono text-[11px] text-text-3">
      {items.map((c, i) => {
        const last = i === items.length - 1;
        return (
          <span key={`${c.label}-${i}`} className="flex items-center gap-2">
            {c.to && !last ? (
              <Link to={c.to} className="uppercase tracking-wide hover:text-text">
                {c.label}
              </Link>
            ) : (
              <span className={last ? "text-text" : "uppercase tracking-wide"}>{c.label}</span>
            )}
            {!last ? <span aria-hidden className="text-text-4">›</span> : null}
          </span>
        );
      })}
    </nav>
  );
}
```

- [ ] **Step 4: Implement `EmptyPanel`**

```tsx
export function EmptyPanel({
  title,
  phase,
  hint,
}: {
  title: string;
  phase: 2 | 3 | 4;
  hint: string;
}) {
  return (
    <section className="rounded-md border border-dashed border-border-strong bg-surface-card p-5">
      <div className="flex items-center justify-between">
        <h2 className="m-0 text-[15px] font-semibold tracking-tight">{title}</h2>
        <span className="rounded border border-border px-1.5 py-0.5 font-mono text-[10px] uppercase tracking-wide text-text-3">
          Phase {phase}
        </span>
      </div>
      <p className="mt-2 text-[12px] text-text-3">{hint}</p>
    </section>
  );
}
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `pnpm -C frontend/web test -- Breadcrumb EmptyPanel`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/features/autooptimizer/ui/Breadcrumb.tsx \
        frontend/web/src/features/autooptimizer/ui/EmptyPanel.tsx \
        frontend/web/src/features/autooptimizer/ui/Breadcrumb.test.tsx \
        frontend/web/src/features/autooptimizer/ui/EmptyPanel.test.tsx
git commit -m "feat(optimizer): Breadcrumb + EmptyPanel primitives"
```

---

## Task 7: `ExperimentWritersPanel` (merge Ladder + Provenance)

Folds the old `Ladder` and `Provenance` tabs into one Home panel: the writer scoreboard, where
each writer row expands to its recent experiments (the provenance grouping).

**Files:**
- Create: `src/features/autooptimizer/panels/ExperimentWritersPanel.tsx`
- Test: `src/features/autooptimizer/panels/ExperimentWritersPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { ExperimentWritersPanel } from "./ExperimentWritersPanel";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("ExperimentWritersPanel", () => {
  it("renders writer rows ranked, with operator labels", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { provider: "anthropic", model: "claude-haiku-4-5", prompt_version: "v1",
        proposals: 10, accepted: 6, rejected_overfit: 4, avg_delta_sharpe: 0.18 },
    ]);
    renderWithProviders(<ExperimentWritersPanel />);
    expect(await screen.findByText("Experiment writers")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("claude-haiku-4-5")).toBeInTheDocument());
    expect(screen.getByText("60%")).toBeInTheDocument(); // accept rate
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- ExperimentWritersPanel`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `ExperimentWritersPanel`**

Reuse `useLadder()` from `api.ts`. Port the ranking/columns from `ExperimentWriterLadder.tsx`
(sort by `avg_delta_sharpe` desc; accept-rate = `accepted / proposals`).

```tsx
import { useLadder, type MutatorScore } from "../api";

function acceptRate(s: MutatorScore): number {
  return s.proposals > 0 ? s.accepted / s.proposals : 0;
}

export function ExperimentWritersPanel() {
  const { data, isLoading, isError } = useLadder();
  const rows = [...(data ?? [])].sort(
    (a, b) => b.avg_delta_sharpe - a.avg_delta_sharpe,
  );

  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <div className="mb-3 flex items-center justify-between">
        <div>
          <h2 className="m-0 text-[15px] font-semibold tracking-tight">Experiment writers</h2>
          <p className="mt-0.5 text-[12px] text-text-3">
            which writer model proposes the best-accepted experiments
          </p>
        </div>
      </div>

      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load writer ladder.</p>
      ) : rows.length === 0 ? (
        <p className="text-[12px] text-text-3">No experiment writers have run yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-[12px]">
            <thead>
              <tr className="text-left text-text-3">
                <th className="py-1.5 pr-3 font-medium">Writer</th>
                <th className="py-1.5 pr-3 text-right font-medium">Proposals</th>
                <th className="py-1.5 pr-3 text-right font-medium">Accepted</th>
                <th className="py-1.5 pr-3 text-right font-medium">Accept %</th>
                <th className="py-1.5 text-right font-medium">Avg ΔSharpe</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {rows.map((s) => {
                const rate = acceptRate(s);
                return (
                  <tr key={`${s.provider}/${s.model}/${s.prompt_version}`} className="border-t border-border-soft">
                    <td className="py-1.5 pr-3">
                      <span className="text-text">{s.model}</span>
                      <span className="ml-1.5 text-[10px] text-text-3">{s.provider} · {s.prompt_version}</span>
                    </td>
                    <td className="py-1.5 pr-3 text-right text-text-2">{s.proposals}</td>
                    <td className="py-1.5 pr-3 text-right text-gold">{s.accepted}</td>
                    <td className={`py-1.5 pr-3 text-right ${rate >= 0.5 ? "text-gold" : rate >= 0.25 ? "text-text" : "text-text-3"}`}>
                      {Math.round(rate * 100)}%
                    </td>
                    <td className={`py-1.5 text-right ${s.avg_delta_sharpe >= 0 ? "text-gold" : "text-danger"}`}>
                      {s.avg_delta_sharpe >= 0 ? "+" : ""}{s.avg_delta_sharpe.toFixed(2)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- ExperimentWritersPanel`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/panels/ExperimentWritersPanel.tsx \
        frontend/web/src/features/autooptimizer/panels/ExperimentWritersPanel.test.tsx
git commit -m "feat(optimizer): ExperimentWritersPanel (merges Ladder + Provenance)"
```

---

## Task 8: `RecentCyclesTable` panel (rows link to the Cycle screen)

**Files:**
- Create: `src/features/autooptimizer/panels/RecentCyclesTable.tsx`
- Test: `src/features/autooptimizer/panels/RecentCyclesTable.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { RecentCyclesTable } from "./RecentCyclesTable";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("RecentCyclesTable", () => {
  it("links each cycle row to its detail screen", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { cycle_id: "cyc-1", node_count: 5, active_count: 2, rejected_count: 3,
        first_created_at: "2026-06-01T00:00:00Z", last_created_at: "2026-06-01T01:00:00Z",
        cost_usd: 4.2, input_tokens: 1000, output_tokens: 500, unpriced_calls: 0 },
    ]);
    renderWithProviders(<RecentCyclesTable />);
    const link = await screen.findByRole("link", { name: /cyc-1/ });
    expect(link).toHaveAttribute("href", "/optimizer/cycle/cyc-1");
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- RecentCyclesTable`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `RecentCyclesTable`**

```tsx
import { Link } from "react-router-dom";
import { useCycleRuns, type CycleRunSummary } from "../api";

function money(n?: number | null): string {
  return n == null ? "—" : `$${n.toFixed(2)}`;
}
function tokens(n?: number | null): string {
  if (n == null) return "—";
  return n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : `${(n / 1000).toFixed(0)}k`;
}

export function RecentCyclesTable() {
  const { data, isLoading, isError } = useCycleRuns();
  const rows: CycleRunSummary[] = data ?? [];
  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <h2 className="m-0 mb-3 text-[15px] font-semibold tracking-tight">Recent cycles</h2>
      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load cycles.</p>
      ) : rows.length === 0 ? (
        <p className="text-[12px] text-text-3">No optimizer cycles have run yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-[12px]">
            <thead>
              <tr className="text-left text-text-3">
                <th className="py-1.5 pr-3 font-medium">Cycle</th>
                <th className="py-1.5 pr-3 text-right font-medium">Experiments</th>
                <th className="py-1.5 pr-3 text-right font-medium">Kept</th>
                <th className="py-1.5 pr-3 text-right font-medium">Tokens</th>
                <th className="py-1.5 text-right font-medium">$</th>
              </tr>
            </thead>
            <tbody className="font-mono">
              {rows.map((c) => (
                <tr key={c.cycle_id} className="border-t border-border-soft hover:bg-gold/[0.03]">
                  <td className="py-1.5 pr-3">
                    <Link to={`/optimizer/cycle/${encodeURIComponent(c.cycle_id)}`} className="text-text hover:text-gold">
                      {c.cycle_id}
                    </Link>
                  </td>
                  <td className="py-1.5 pr-3 text-right text-text-2">{c.node_count}</td>
                  <td className="py-1.5 pr-3 text-right text-gold">{c.active_count}</td>
                  <td className="py-1.5 pr-3 text-right text-text-2">{tokens(c.input_tokens != null && c.output_tokens != null ? c.input_tokens + c.output_tokens : null)}</td>
                  <td className="py-1.5 text-right text-text-2">{money(c.cost_usd)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- RecentCyclesTable`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/panels/RecentCyclesTable.tsx \
        frontend/web/src/features/autooptimizer/panels/RecentCyclesTable.test.tsx
git commit -m "feat(optimizer): RecentCyclesTable panel with cycle deep-links"
```

---

## Task 9: `CycleExperimentsTable` panel (per-cycle genealogy slice)

Rows are the experiments produced in one cycle; each links to the Experiment screen. This is the
old `Genealogy` tab, scoped to a cycle and folded into the Cycle screen.

**Files:**
- Create: `src/features/autooptimizer/panels/CycleExperimentsTable.tsx`
- Test: `src/features/autooptimizer/panels/CycleExperimentsTable.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { CycleExperimentsTable } from "./CycleExperimentsTable";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("CycleExperimentsTable", () => {
  it("lists experiments for the cycle with a link + Kept badge", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([
      { bundle_hash: "deadbeefcafe", parent_hash: "0000", gate_verdict: "Pass",
        status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z",
        diversity_score: 0.24 },
    ]);
    renderWithProviders(<CycleExperimentsTable cycleId="cyc-1" />);
    const link = await screen.findByRole("link", { name: /deadbeef/ });
    expect(link).toHaveAttribute("href", "/optimizer/experiment/deadbeefcafe");
    expect(screen.getByText("Kept")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- CycleExperimentsTable`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `CycleExperimentsTable`**

```tsx
import { Link } from "react-router-dom";
import { useLineageNodes, formatGateVerdict, type LineageNode } from "../api";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";

export function CycleExperimentsTable({ cycleId }: { cycleId: string }) {
  const { data, isLoading, isError } = useLineageNodes({ cycleId });
  const rows: LineageNode[] = data ?? [];
  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <div className="mb-1">
        <h2 className="m-0 text-[15px] font-semibold tracking-tight">Experiments this cycle</h2>
        <p className="mt-0.5 text-[12px] text-text-3">what the optimizer tried · what was kept</p>
      </div>
      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load experiments.</p>
      ) : rows.length === 0 ? (
        <p className="text-[12px] text-text-3">No experiments recorded for this cycle.</p>
      ) : (
        <ul className="mt-2 divide-y divide-border-soft">
          {rows.map((n) => (
            <li key={n.bundle_hash} className="flex items-center gap-3 py-2">
              <HashSigil hash={n.bundle_hash} size={32} />
              <Link
                to={`/optimizer/experiment/${encodeURIComponent(n.bundle_hash)}`}
                className="font-mono text-[12px] text-text hover:text-gold"
              >
                {n.bundle_hash.slice(0, 10)}
              </Link>
              <span className="ml-auto flex items-center gap-3">
                {n.diversity_score != null ? (
                  <span className="font-mono text-[11px] text-text-3">div {n.diversity_score.toFixed(2)}</span>
                ) : null}
                <GateBadge verdict={formatGateVerdict(n.gate_verdict)} status={n.status} />
              </span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- CycleExperimentsTable`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/panels/CycleExperimentsTable.tsx \
        frontend/web/src/features/autooptimizer/panels/CycleExperimentsTable.test.tsx
git commit -m "feat(optimizer): CycleExperimentsTable (folds Genealogy into Cycle)"
```

---

## Task 10: `ParentDiffPanel` (inline diff — folds the Diff tab)

Port the parent↔child diff from `DiffInspector.tsx` into a panel that takes a `bundle_hash`,
fetches the child blob (`useBlob`) and the parent blob (via the node's `parent_hash`), and renders
the key/before/after table inline on the Experiment screen.

**Files:**
- Create: `src/features/autooptimizer/panels/ParentDiffPanel.tsx`
- Test: `src/features/autooptimizer/panels/ParentDiffPanel.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { ParentDiffPanel } from "./ParentDiffPanel";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("ParentDiffPanel", () => {
  it("shows a changed key with before/after values", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/blob/child")) return { entry_threshold: 0.7, name: "child" };
      if (url.includes("/blob/parent")) return { entry_threshold: 0.5, name: "parent" };
      return {};
    });
    renderWithProviders(
      <ParentDiffPanel childHash="child" parentHash="parent" />,
    );
    expect(await screen.findByText("What this experiment changed")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByText("entry_threshold")).toBeInTheDocument());
    expect(screen.getByText("0.5")).toBeInTheDocument();
    expect(screen.getByText("0.7")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- ParentDiffPanel`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `ParentDiffPanel`**

```tsx
import { useBlob, type StrategyBlob } from "../api";

type Row = { key: string; before: unknown; after: unknown; changed: boolean };

function flatten(obj: unknown, prefix = ""): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  if (obj && typeof obj === "object" && !Array.isArray(obj)) {
    for (const [k, v] of Object.entries(obj as Record<string, unknown>)) {
      const key = prefix ? `${prefix}.${k}` : k;
      if (v && typeof v === "object" && !Array.isArray(v)) Object.assign(out, flatten(v, key));
      else out[key] = v;
    }
  }
  return out;
}

function diffRows(parent: StrategyBlob | undefined, child: StrategyBlob | undefined): Row[] {
  const p = flatten(parent ?? {});
  const c = flatten(child ?? {});
  const keys = Array.from(new Set([...Object.keys(p), ...Object.keys(c)])).sort();
  return keys.map((key) => ({
    key,
    before: p[key],
    after: c[key],
    changed: JSON.stringify(p[key]) !== JSON.stringify(c[key]),
  }));
}

function cell(v: unknown): string {
  if (v === undefined) return "—";
  return typeof v === "string" ? v : JSON.stringify(v);
}

export function ParentDiffPanel({
  childHash,
  parentHash,
}: {
  childHash: string;
  parentHash?: string | null;
}) {
  const child = useBlob(childHash);
  const parent = useBlob(parentHash ?? undefined);
  const rows = diffRows(parent.data, child.data);
  const changed = rows.filter((r) => r.changed);
  const loading = child.isLoading || (!!parentHash && parent.isLoading);

  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <h2 className="m-0 text-[15px] font-semibold tracking-tight">What this experiment changed</h2>
      <p className="mt-0.5 text-[12px] text-text-3">
        parent → experiment · {changed.length} field{changed.length === 1 ? "" : "s"} changed
      </p>
      {loading ? (
        <p className="mt-3 text-[12px] text-text-3">Loading diff…</p>
      ) : !parentHash ? (
        <p className="mt-3 text-[12px] text-text-3">Root experiment — no parent to diff against.</p>
      ) : changed.length === 0 ? (
        <p className="mt-3 text-[12px] text-text-3">No field-level differences from the parent.</p>
      ) : (
        <table className="mt-3 w-full border-collapse font-mono text-[11.5px]">
          <thead>
            <tr className="text-left text-text-3">
              <th className="py-1.5 pr-3 font-medium">Field</th>
              <th className="py-1.5 pr-3 font-medium">− before</th>
              <th className="py-1.5 font-medium">+ after</th>
            </tr>
          </thead>
          <tbody>
            {changed.map((r) => (
              <tr key={r.key} className="border-t border-border-soft align-top">
                <td className="py-1.5 pr-3 text-text-2">{r.key}</td>
                <td className="py-1.5 pr-3 text-danger">{cell(r.before)}</td>
                <td className="py-1.5 text-gold">{cell(r.after)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </section>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- ParentDiffPanel`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/panels/ParentDiffPanel.tsx \
        frontend/web/src/features/autooptimizer/panels/ParentDiffPanel.test.tsx
git commit -m "feat(optimizer): ParentDiffPanel (folds Diff tab into Experiment screen)"
```

---

## Task 11: `ExperimentDetail` screen (`/optimizer/experiment/:hash`)

**Files:**
- Create: `src/features/autooptimizer/screens/ExperimentDetail.tsx`
- Test: `src/features/autooptimizer/screens/ExperimentDetail.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { Routes, Route } from "react-router-dom";
import { renderWithProviders } from "../test-utils";
import { ExperimentDetail } from "./ExperimentDetail";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("ExperimentDetail", () => {
  it("renders the experiment hero, diff panel, and phase stubs", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/lineage/")) {
        return { bundle_hash: "deadbeefcafe", parent_hash: "0000", gate_verdict: "Pass",
                 status: "active", cycle_id: "cyc-1", created_at: "2026-06-01T00:00:00Z" };
      }
      return {}; // blobs
    });
    renderWithProviders(
      <Routes>
        <Route path="/optimizer/experiment/:hash" element={<ExperimentDetail />} />
      </Routes>,
      { route: "/optimizer/experiment/deadbeefcafe" },
    );
    await waitFor(() => expect(screen.getByText(/deadbeef/)).toBeInTheDocument());
    expect(screen.getByText("What this experiment changed")).toBeInTheDocument();
    expect(screen.getByText("Per-regime evaluation")).toBeInTheDocument(); // EmptyPanel stub
    expect(screen.getByText("Flight recorder")).toBeInTheDocument();        // EmptyPanel stub
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- ExperimentDetail`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `ExperimentDetail`**

```tsx
import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { useLineageNode, formatGateVerdict } from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";
import { EmptyPanel } from "../ui/EmptyPanel";
import { ParentDiffPanel } from "../panels/ParentDiffPanel";

export function ExperimentDetail() {
  const { hash = "" } = useParams<{ hash: string }>();
  const { data: node, isLoading, isError } = useLineageNode(hash);

  return (
    <>
      <Topbar title="Optimizer" sub="Experiment detail" back={{ to: "/optimizer", label: "Back to Optimizer" }} />
      <div className="space-y-5">
        <Breadcrumb
          items={[
            { label: "OPTIMIZER", to: "/optimizer" },
            { label: "cycle", to: node?.cycle_id ? `/optimizer/cycle/${encodeURIComponent(node.cycle_id)}` : undefined },
            { label: hash.slice(0, 10) },
          ]}
        />

        {isLoading ? (
          <p className="text-[12px] text-text-3">Loading experiment…</p>
        ) : isError || !node ? (
          <p className="text-[12px] text-danger">Couldn't load this experiment.</p>
        ) : (
          <>
            <section className="flex items-start gap-4 rounded-md border border-border bg-surface-card p-5">
              <HashSigil hash={node.bundle_hash} size={72} />
              <div className="min-w-0">
                <div className="mb-1 flex items-center gap-2">
                  <span className="text-[8.5px] uppercase tracking-widest text-text-3">Optimizer · Experiment</span>
                  <GateBadge verdict={formatGateVerdict(node.gate_verdict)} status={node.status} />
                </div>
                <h1 className="m-0 font-mono text-[22px] tracking-tight">{node.bundle_hash.slice(0, 16)}</h1>
                <p className="mt-1 font-mono text-[11px] text-text-3">
                  parent {node.parent_hash ? node.parent_hash.slice(0, 10) : "— (root)"} · cycle {node.cycle_id ?? "—"}
                </p>
              </div>
            </section>

            <ParentDiffPanel childHash={node.bundle_hash} parentHash={node.parent_hash} />

            <EmptyPanel title="Per-regime evaluation" phase={2} hint="Lights up when the regime matrix runs — Δ-Sharpe, return, drawdown, win-rate and an equity curve per regime." />
            <EmptyPanel title="Flight recorder" phase={3} hint="The structured trace (intern → trader → risk → execution) for this experiment, once trace linkage ships." />
            <EmptyPanel title="Sign-off receipts" phase={4} hint="Attester endorsements and the sign-off decision, once attesters ship." />
          </>
        )}
      </div>
    </>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- ExperimentDetail`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/screens/ExperimentDetail.tsx \
        frontend/web/src/features/autooptimizer/screens/ExperimentDetail.test.tsx
git commit -m "feat(optimizer): ExperimentDetail screen"
```

---

## Task 12: `CycleDetail` screen (`/optimizer/cycle/:cycleId`)

**Files:**
- Create: `src/features/autooptimizer/screens/CycleDetail.tsx`
- Test: `src/features/autooptimizer/screens/CycleDetail.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { Routes, Route } from "react-router-dom";
import { renderWithProviders } from "../test-utils";
import { CycleDetail } from "./CycleDetail";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("CycleDetail", () => {
  it("renders the cycle hero, experiments table, and phase stubs", async () => {
    vi.spyOn(client, "apiFetch").mockImplementation(async (url: string) => {
      if (url.includes("/cycles/")) {
        return { cycle_id: "cyc-1", node_count: 3, active_count: 1, rejected_count: 2,
                 first_created_at: "2026-06-01T00:00:00Z", last_created_at: "2026-06-01T01:00:00Z",
                 cost_usd: 4.2, input_tokens: 1000, output_tokens: 500, unpriced_calls: 0, nodes: [] };
      }
      if (url.includes("/lineage")) return [];
      return {};
    });
    renderWithProviders(
      <Routes>
        <Route path="/optimizer/cycle/:cycleId" element={<CycleDetail />} />
      </Routes>,
      { route: "/optimizer/cycle/cyc-1" },
    );
    await waitFor(() => expect(screen.getByText("cyc-1")).toBeInTheDocument());
    expect(screen.getByText("Experiments this cycle")).toBeInTheDocument();
    expect(screen.getByText("Eval matrix")).toBeInTheDocument();         // EmptyPanel
    expect(screen.getByText("Anti-overfit gate")).toBeInTheDocument();   // EmptyPanel
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- CycleDetail`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `CycleDetail`**

```tsx
import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { useCycleRun } from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { EmptyPanel } from "../ui/EmptyPanel";
import { CycleExperimentsTable } from "../panels/CycleExperimentsTable";

function stat(label: string, value: string, tone = "text-text") {
  return (
    <div className="flex flex-col">
      <span className="text-[8.5px] uppercase tracking-widest text-text-3">{label}</span>
      <span className={`font-mono text-[20px] ${tone}`}>{value}</span>
    </div>
  );
}

export function CycleDetail() {
  const { cycleId = "" } = useParams<{ cycleId: string }>();
  const { data: cycle, isLoading, isError } = useCycleRun(cycleId);

  return (
    <>
      <Topbar title="Optimizer" sub="Cycle detail" back={{ to: "/optimizer", label: "Back to Optimizer" }} />
      <div className="space-y-5">
        <Breadcrumb items={[{ label: "OPTIMIZER", to: "/optimizer" }, { label: "cycle" }, { label: cycleId }]} />

        {isLoading ? (
          <p className="text-[12px] text-text-3">Loading cycle…</p>
        ) : isError || !cycle ? (
          <p className="text-[12px] text-danger">Couldn't load this cycle.</p>
        ) : (
          <section className="rounded-md border border-border bg-surface-card p-5">
            <span className="text-[8.5px] uppercase tracking-widest text-text-3">Cycle</span>
            <h1 className="m-0 mb-3 font-mono text-[22px] tracking-tight">{cycle.cycle_id}</h1>
            <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
              {stat("Experiments", String(cycle.node_count))}
              {stat("Kept", String(cycle.active_count), "text-gold")}
              {stat("Dropped", String(cycle.rejected_count), "text-text-2")}
              {stat("$ spend", cycle.cost_usd == null ? "—" : `$${cycle.cost_usd.toFixed(2)}`)}
            </div>
          </section>
        )}

        <EmptyPanel title="Anti-overfit gate" phase={2} hint="Kept / Suspect / Dropped buckets appear once experiments are gated across the regime set." />
        <EmptyPanel title="Eval matrix" phase={2} hint="Experiments × regimes heat-map of Δ-Sharpe — lights up when the regime matrix runs." />

        <CycleExperimentsTable cycleId={cycleId} />

        <EmptyPanel title="Attester activity" phase={4} hint="Local attester sign-offs (endorse / question / reject) per experiment, once attesters ship." />
        <EmptyPanel title="Evening summary preview" phase={4} hint="The local, unpublished nightly summary of kept experiments, once sign-off ships." />
      </div>
    </>
  );
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- CycleDetail`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/features/autooptimizer/screens/CycleDetail.tsx \
        frontend/web/src/features/autooptimizer/screens/CycleDetail.test.tsx
git commit -m "feat(optimizer): CycleDetail screen"
```

---

## Task 13: `OptimizerHome` screen (`/optimizer`)

Composes the existing live dashboard (`LiveCycleView`, reused as-is for the in-flight section)
with the new `RecentCyclesTable` and `ExperimentWritersPanel`. The old `Live` tab's standalone
recent-cycles/lineage blocks are superseded by these panels.

**Files:**
- Create: `src/features/autooptimizer/screens/OptimizerHome.tsx`
- Test: `src/features/autooptimizer/screens/OptimizerHome.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { OptimizerHome } from "./OptimizerHome";
import * as client from "@/api/client";

afterEach(() => vi.restoreAllMocks());

describe("OptimizerHome", () => {
  it("renders the Optimizer header and the writers + recent-cycles panels", async () => {
    vi.spyOn(client, "apiFetch").mockResolvedValue([]);
    renderWithProviders(<OptimizerHome />);
    expect(await screen.findByText("Experiment writers")).toBeInTheDocument();
    expect(screen.getByText("Recent cycles")).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- OptimizerHome`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement `OptimizerHome`**

> The existing `LiveCycleView` renders its own `Topbar`. To avoid a double Topbar, render
> `OptimizerHome`'s own Topbar and embed `LiveCycleView`'s body. If `LiveCycleView` cannot be
> embedded without its Topbar, wrap it as-is for Phase 1 (a single Topbar from `LiveCycleView`)
> and place the new panels beneath it. The test above does not assert Topbar count, so either
> wiring passes; prefer the cleaner single-Topbar arrangement below.

```tsx
import { Topbar } from "@/components/shell/Topbar";
import { LiveCycleView } from "../LiveCycleView";
import { RecentCyclesTable } from "../panels/RecentCyclesTable";
import { ExperimentWritersPanel } from "../panels/ExperimentWritersPanel";

export function OptimizerHome() {
  return (
    <>
      <Topbar title="Optimizer" sub="Tonight's run, experiment writers, and recent cycles" />
      <div className="space-y-5">
        {/* In-flight cycle + live event feed (existing dashboard body). */}
        <LiveCycleView embedded />
        <ExperimentWritersPanel />
        <RecentCyclesTable />
      </div>
    </>
  );
}
```

> **Note for the implementer:** add an optional `embedded?: boolean` prop to `LiveCycleView`
> that, when true, skips rendering its internal `<Topbar>` (the parent supplies it). This is a
> one-line guard around the existing `<Topbar … />` in `LiveCycleView.tsx`. If the existing
> recent-cycles / active-lineages blocks inside `LiveCycleView` now duplicate the new panels,
> leave them for Phase 1 (they're harmless) and note a follow-up to trim them — do NOT delete
> working code under time pressure without a passing test.

- [ ] **Step 4: Add the `embedded` guard to `LiveCycleView`**

In `src/features/autooptimizer/LiveCycleView.tsx`, change the component signature to accept
`{ embedded = false }: { embedded?: boolean }` and wrap its `<Topbar … />` so it only renders
when `!embedded`. Keep all other behavior identical.

- [ ] **Step 5: Run the test to verify it passes**

Run: `pnpm -C frontend/web test -- OptimizerHome`
Expected: PASS.

- [ ] **Step 6: Run the full optimizer test suite + typecheck**

Run: `pnpm -C frontend/web test -- autooptimizer` then `pnpm -C frontend/web typecheck`
Expected: all PASS, no type errors.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx \
        frontend/web/src/features/autooptimizer/screens/OptimizerHome.test.tsx \
        frontend/web/src/features/autooptimizer/LiveCycleView.tsx
git commit -m "feat(optimizer): OptimizerHome screen composing live + writers + cycles"
```

---

## Task 14: Routing + sidebar + redirect (wire the screens, delete the tab layout)

**Files:**
- Modify: `src/routes.tsx`
- Modify: `src/components/shell/Sidebar.tsx`
- Modify: `src/routes.test.tsx` (only if it pins the old `autooptimizer` path)
- Delete: `src/features/autooptimizer/AutoOptimizerLayout.tsx`
- Test: `src/routes.test.tsx` (assert the new routes resolve)

- [ ] **Step 1: Write the failing routing test**

Add to `src/routes.test.tsx` (or a new `src/features/autooptimizer/routing.test.tsx` using the
`RouterProvider` against `router`). Minimal addition asserting the redirect + index:

```tsx
// In src/routes.test.tsx, add inside the existing describe or a new one:
import { router } from "./routes";

it("exposes /optimizer and redirects the legacy /autooptimizer path", () => {
  const paths = JSON.stringify(router.routes);
  expect(paths).toContain("optimizer");
  // legacy redirect present
  expect(paths).toContain("autooptimizer");
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm -C frontend/web test -- routes`
Expected: FAIL — `optimizer` path not yet present.

- [ ] **Step 3: Add lazy imports for the three screens in `routes.tsx`**

Replace the `AutoOptimizerLayout` lazy import (line 59) with:

```tsx
const OptimizerHome = lazy(() => import("./features/autooptimizer/screens/OptimizerHome").then((m) => ({ default: m.OptimizerHome })));
const OptimizerCycle = lazy(() => import("./features/autooptimizer/screens/CycleDetail").then((m) => ({ default: m.CycleDetail })));
const OptimizerExperiment = lazy(() => import("./features/autooptimizer/screens/ExperimentDetail").then((m) => ({ default: m.ExperimentDetail })));
```

- [ ] **Step 4: Replace the `autooptimizer` route subtree (lines 198–204) with the new tree + redirects**

```tsx
      {
        path: "optimizer",
        children: [
          { index: true, element: page(<OptimizerHome />) },
          { path: "cycle/:cycleId", element: page(<OptimizerCycle />) },
          { path: "experiment/:hash", element: page(<OptimizerExperiment />) },
        ],
      },
      // Legacy deep-links (bookmarks, old SSE/diff URLs) → new optimizer surface.
      { path: "autooptimizer", element: <Navigate to="/optimizer" replace /> },
      { path: "autooptimizer/diff/:hash", element: <Navigate to="/optimizer" replace /> },
```

(`Navigate` is already imported in `routes.tsx`.)

- [ ] **Step 5: Update the sidebar nav entry**

In `src/components/shell/Sidebar.tsx` line 20, change:

```tsx
  { to: "/autooptimizer", label: "Optimizer", icon: "pulse" },
```

to:

```tsx
  { to: "/optimizer", label: "Optimizer", icon: "pulse" },
```

- [ ] **Step 6: Delete the obsolete tab layout**

```bash
git rm frontend/web/src/features/autooptimizer/AutoOptimizerLayout.tsx
```

> `GenealogyTree.tsx`, `DiffInspector.tsx`, `ExperimentWriterLadder.tsx`,
> `LadderWithProvenance.tsx` are no longer routed but may still be imported by tests or kept for
> reference. Leave them in place for Phase 1 (the lineage-tree inline view in a later task can
> reuse `GenealogyTree`). Only `AutoOptimizerLayout.tsx` is deleted now.

- [ ] **Step 7: Run the routing test + typecheck**

Run: `pnpm -C frontend/web test -- routes` then `pnpm -C frontend/web typecheck`
Expected: PASS; typecheck clean (no dangling `AutoOptimizerLayout` import).

- [ ] **Step 8: Commit**

```bash
git add frontend/web/src/routes.tsx frontend/web/src/components/shell/Sidebar.tsx frontend/web/src/routes.test.tsx
git rm frontend/web/src/features/autooptimizer/AutoOptimizerLayout.tsx
git commit -m "feat(optimizer): wire /optimizer 3-screen routes, redirect legacy path, drop tab layout"
```

---

## Task 15: Full verification + manual smoke

**Files:** none (verification only).

- [ ] **Step 1: Run the entire frontend test suite**

Run: `pnpm -C frontend/web test`
Expected: all green, including every new optimizer test.

- [ ] **Step 2: Typecheck the whole SPA**

Run: `pnpm -C frontend/web typecheck`
Expected: no errors.

- [ ] **Step 3: Production build (catches lazy-import + tree-shake issues)**

Run: `pnpm -C frontend/web build`
Expected: build succeeds.

- [ ] **Step 4: Manual smoke (dev server)**

Run: `pnpm -C frontend/web dev`, open the dashboard, then:
- Click **Optimizer** in the sidebar → lands on `/optimizer` (Home: live section + Experiment writers + Recent cycles).
- Click a recent cycle row → `/optimizer/cycle/:id` (hero + experiments table + Phase stubs).
- Click an experiment row → `/optimizer/experiment/:hash` (hero + diff + Phase stubs).
- Visit `/autooptimizer` and `/autooptimizer/diff/abc` → both redirect to `/optimizer`.
- Confirm no right-side box appears beside the chat rail, and no modal/popup is used.
- Toggle light/dark theme → no 100%-white borders on the new panels.

- [ ] **Step 5: Commit (if any smoke fixes were needed)**

```bash
git add -A frontend/web/src/features/autooptimizer
git commit -m "fix(optimizer): phase-1 smoke fixes"
```

---

## Self-review (completed by plan author)

**Spec coverage (against `2026-06-05-optimizer-ui-redesign-design.md`):**
- §4.1 Optimizer Home → Tasks 7, 8, 13 (writers panel, recent cycles, home shell + live section). ✅
- §4.2 Cycle detail → Tasks 9, 12 (experiments table; hero + gate/matrix/attester/summary stubs). ✅
- §4.3 Experiment detail → Tasks 10, 11 (inline diff; hero + per-regime/flight/receipt stubs). ✅
- §4 tab→panel fold-in (Diff→panel, Genealogy→cycle table, Ladder+Provenance→one panel, delete tab bar) → Tasks 9, 10, 7, 14. ✅
- §4 route rename `/optimizer` + `/autooptimizer` redirect → Task 14. ✅
- §5 terminology (Kept/Suspect/Dropped via GateBadge; Experiment writer; formatGateVerdict) → Tasks 3, 7, 9. ✅
- §6 visual primitives (HashSigil replacing gen-art, ProgressDial, badges, breadcrumb, single-column, no popups) → Tasks 2–6; layout rule enforced in screens (Tasks 11–13) and verified in Task 15. ✅
- §8 Phase-1 = shell on existing data, ⏳ panels as honest empty-states → EmptyPanel (Task 6) used in Tasks 11–12. ✅
- Out-of-scope (gen-art pipeline, on-chain/Marketplace, settings) → not present; gen-art replaced by HashSigil. ✅

**Placeholder scan:** No "TBD/TODO/handle edge cases" steps; every code step shows full code; every test step shows the assertion. The "follow-up to trim duplicated LiveCycleView blocks" note (Task 13) is an explicit deferral, not a missing step — Phase-1 behavior is complete without it.

**Type consistency:** `useLineageNodes(q?)` optional-arg (Task 1) is back-compat for existing callers; `getCycleRun`/`useCycleRun`/`CycleRunDetail` names used identically in Tasks 1/12; `GateBadge` props `{verdict, status}` used identically in Tasks 3/9/11; `formatGateVerdict` is the existing exported helper; `HashSigil`/`ExperimentPill`/`EmptyPanel`/`Breadcrumb` prop shapes match between definition and use.

**Note on backend dependency:** Phase 1 calls only endpoints the backend already serves
(`/lineage`, `/lineage/:hash`, `/ladder`, `/cycles`, `/cycles/:id`, `/blob/:hash`). `GET
/api/autooptimizer/cycles/:id` is confirmed in the backend map; if the deployed build predates it,
Task 12's `useCycleRun` will surface the error-state branch (already handled) rather than crash.
