# Optimizer Home — Remove LiveCycleView (Level 2 IA fix) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove the raw eval-runner widget (`LiveCycleView`) from the Optimizer home page, wire up the currently-disabled Pause/Cancel/Resume controls in the status hero, and add a "Watch live →" link to the run-detail drill-down page.

**Architecture:** All changes are frontend-only, confined to `OptimizerHome.tsx` and its test file. `StatusHero` gains real mutation hooks (same hooks already used by `RunDetail.tsx`) and a `Link` to `/optimizer/run/:sessionId`. The `<LiveCycleView embedded />` line is removed from the page root. No new files, no backend changes, no new components.

**Tech Stack:** React, TanStack Query (`useMutation`), react-router-dom `Link`, vitest + testing-library/react

---

## File map

| File | Change |
|---|---|
| `frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx` | Remove `LiveCycleView` import + usage; add `usePauseSession`/`useResumeSession`/`useCancelSession` + `Link` to `StatusHero` |
| `frontend/web/src/features/autooptimizer/screens/OptimizerHome.test.tsx` | Add tests for: no LiveCycleView, "Watch live →" link, functional controls |

Context for understanding the codebase before you touch anything:
- `StatusHero` lives at the top of `OptimizerHome.tsx:56`. It already renders Pause/Cancel/Resume buttons but they are all `disabled` (no `onClick` wired). The controls in `RunDetail.tsx` (`ControlsRow`) use the same three hooks and are the reference implementation.
- `<LiveCycleView embedded />` is at `OptimizerHome.tsx:313`. The `embedded` prop suppresses the full-page header but the component still mounts an SSE connection (`EventSource`) and shows a "Live · cycle in progress" label that is semantically unrelated to the optimizer session state shown in `StatusHero`.
- The three mutation hooks (`usePauseSession`, `useResumeSession`, `useCancelSession`) are exported from `../api` and each accept a `sessionId: string`.
- `session.session_id` is the id to pass to the mutations; it is available from `status?.active_session` (type `SessionSummary`).
- The test file at `OptimizerHome.test.tsx` already mocks `EventSource` in `beforeEach` (required by `LiveCycleView`). Once `LiveCycleView` is removed, that mock becomes harmless but can stay.

---

## Task 1: Write failing tests

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/screens/OptimizerHome.test.tsx`

Add a new `describe` block after all existing blocks. All tests in this block must FAIL before Task 2 is implemented.

- [ ] **Step 1.1: Open the test file and add mutation hook mocks at the top of the new describe block**

Add this new describe block at the end of `OptimizerHome.test.tsx` (after the last existing `describe` block, before the file ends):

```typescript
// ─── Helpers shared across new tests ─────────────────────────────────────────

function mockMutations() {
  const pauseMutateMock = vi.fn();
  const resumeMutateMock = vi.fn();
  const cancelMutateMock = vi.fn();

  vi.spyOn(apiModule, "usePauseSession").mockReturnValue({
    mutate: pauseMutateMock,
    isPending: false,
  } as unknown as ReturnType<typeof apiModule.usePauseSession>);

  vi.spyOn(apiModule, "useResumeSession").mockReturnValue({
    mutate: resumeMutateMock,
    isPending: false,
  } as unknown as ReturnType<typeof apiModule.useResumeSession>);

  vi.spyOn(apiModule, "useCancelSession").mockReturnValue({
    mutate: cancelMutateMock,
    isPending: false,
  } as unknown as ReturnType<typeof apiModule.useCancelSession>);

  return { pauseMutateMock, resumeMutateMock, cancelMutateMock };
}

const runningSessionStatus = {
  active_session: {
    session_id: "sess_01ABCDEFGHIJ",
    strategy_id: "strat-xyz",
    state: "running",
    mode: "explore",
    cycles_completed: 3,
    kept_count: 1,
    suspect_count: 0,
    dropped_count: 2,
  },
  last_event_seq: 10,
};

const pausedSessionStatus = {
  active_session: {
    session_id: "sess_01ABCDEFGHIJ",
    strategy_id: "strat-xyz",
    state: "paused",
    mode: "explore",
    cycles_completed: 3,
    kept_count: 1,
    suspect_count: 0,
    dropped_count: 2,
  },
  last_event_seq: 10,
};

describe("OptimizerHome — LiveCycleView removed (Level-2 IA fix)", () => {
  function setupRunning() {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(runningSessionStatus);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    return mockMutations();
  }

  function setupPaused() {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(pausedSessionStatus);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    return mockMutations();
  }

  it("does NOT render 'Live · cycle in progress' text at any time", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    mockMutations();

    renderWithProviders(<OptimizerHome />);
    await screen.findByText("Idle");

    expect(screen.queryByText(/Live · cycle in progress/i)).toBeNull();
  });

  it("shows 'Watch live →' link when optimizer is running", async () => {
    setupRunning();
    renderWithProviders(<OptimizerHome />);
    const link = await screen.findByRole("link", { name: /watch live/i });
    expect(link).toBeInTheDocument();
    expect(link).toHaveAttribute("href", "/optimizer/run/sess_01ABCDEFGHIJ");
  });

  it("shows 'Watch live →' link when optimizer is paused", async () => {
    setupPaused();
    renderWithProviders(<OptimizerHome />);
    const link = await screen.findByRole("link", { name: /watch live/i });
    expect(link).toBeInTheDocument();
    expect(link).toHaveAttribute("href", "/optimizer/run/sess_01ABCDEFGHIJ");
  });

  it("shows 'Watch live →' link when optimizer is cancelling", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: {
        session_id: "sess_01ABCDEFGHIJ",
        strategy_id: "strat-xyz",
        state: "cancelling",
        mode: "explore",
        cycles_completed: 3,
        kept_count: 1,
        suspect_count: 0,
        dropped_count: 2,
      },
      last_event_seq: 10,
    });
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    mockMutations();

    renderWithProviders(<OptimizerHome />);
    const link = await screen.findByRole("link", { name: /watch live/i });
    expect(link).toBeInTheDocument();
    expect(link).toHaveAttribute("href", "/optimizer/run/sess_01ABCDEFGHIJ");
  });

  it("hides Pause and Cancel action buttons when state is cancelling", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: {
        session_id: "sess_01ABCDEFGHIJ",
        strategy_id: "strat-xyz",
        state: "cancelling",
        mode: "explore",
        cycles_completed: 3,
        kept_count: 1,
        suspect_count: 0,
        dropped_count: 2,
      },
      last_event_seq: 10,
    });
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    mockMutations();

    renderWithProviders(<OptimizerHome />);
    await screen.findByText("Cancelling");
    expect(screen.queryByRole("button", { name: /pause/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /cancel/i })).toBeNull();
    expect(screen.queryByRole("button", { name: /resume/i })).toBeNull();
  });

  it("Pause button calls usePauseSession mutate with sessionId when running", async () => {
    const { pauseMutateMock } = setupRunning();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);
    const pauseBtn = await screen.findByRole("button", { name: /pause/i });
    await user.click(pauseBtn);
    expect(pauseMutateMock).toHaveBeenCalledWith("sess_01ABCDEFGHIJ");
  });

  it("Cancel button calls useCancelSession mutate with sessionId when running", async () => {
    const { cancelMutateMock } = setupRunning();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);
    const cancelBtn = await screen.findByRole("button", { name: /cancel/i });
    await user.click(cancelBtn);
    expect(cancelMutateMock).toHaveBeenCalledWith("sess_01ABCDEFGHIJ");
  });

  it("Resume button calls useResumeSession mutate with sessionId when paused", async () => {
    const { resumeMutateMock } = setupPaused();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);
    const resumeBtn = await screen.findByRole("button", { name: /resume/i });
    await user.click(resumeBtn);
    expect(resumeMutateMock).toHaveBeenCalledWith("sess_01ABCDEFGHIJ");
  });

  it("Cancel button calls useCancelSession mutate with sessionId when paused", async () => {
    const { cancelMutateMock } = setupPaused();
    const user = userEvent.setup();
    renderWithProviders(<OptimizerHome />);
    const cancelBtn = await screen.findByRole("button", { name: /cancel/i });
    await user.click(cancelBtn);
    expect(cancelMutateMock).toHaveBeenCalledWith("sess_01ABCDEFGHIJ");
  });

  it("Pause button is disabled while pauseMutation.isPending", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(runningSessionStatus);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    vi.spyOn(apiModule, "usePauseSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: true,
    } as unknown as ReturnType<typeof apiModule.usePauseSession>);
    vi.spyOn(apiModule, "useResumeSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof apiModule.useResumeSession>);
    vi.spyOn(apiModule, "useCancelSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof apiModule.useCancelSession>);

    renderWithProviders(<OptimizerHome />);
    const pauseBtn = await screen.findByRole("button", { name: /pause/i });
    expect(pauseBtn).toBeDisabled();
  });

  it("Cancel button is disabled while cancelMutation.isPending", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue(runningSessionStatus);
    vi.spyOn(apiModule, "useSessionList").mockReturnValue({
      data: [],
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useSessionList>);
    vi.spyOn(apiModule, "usePauseSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof apiModule.usePauseSession>);
    vi.spyOn(apiModule, "useResumeSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: false,
    } as unknown as ReturnType<typeof apiModule.useResumeSession>);
    vi.spyOn(apiModule, "useCancelSession").mockReturnValue({
      mutate: vi.fn(),
      isPending: true,
    } as unknown as ReturnType<typeof apiModule.useCancelSession>);

    renderWithProviders(<OptimizerHome />);
    const cancelBtn = await screen.findByRole("button", { name: /cancel/i });
    expect(cancelBtn).toBeDisabled();
  });
});
```

- [ ] **Step 1.2: Run tests to verify they all FAIL**

```bash
cd /Users/edkennedy/Code/xvision/frontend/web
pnpm test -- --reporter=verbose OptimizerHome.test 2>&1 | tail -40
```

Expected: The six new tests in "OptimizerHome — LiveCycleView removed (Level-2 IA fix)" should FAIL. Existing tests should still PASS.

Failing reason will vary:
- "does NOT render 'Live · cycle in progress'" — will FAIL because LiveCycleView is still rendered (the text IS present)
- "Watch live →" tests — will FAIL because the link doesn't exist yet
- Mutation click tests — will FAIL because the buttons are `disabled` (no onClick)

If ALL tests in the new describe block pass before Task 2, something is wrong — stop and investigate.

---

## Task 2: Wire mutations and "Watch live →" link in StatusHero

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx`

- [ ] **Step 2.1: Add missing imports to the import block at the top of OptimizerHome.tsx**

The file currently imports from `"../api"`:
```typescript
import { useOptimizerStatus, useOptimizerStats, useSessionList, type SessionListItem } from "../api";
```

Change to (add the three mutation hooks):
```typescript
import {
  useOptimizerStatus,
  useOptimizerStats,
  useSessionList,
  usePauseSession,
  useResumeSession,
  useCancelSession,
  type SessionListItem,
} from "../api";
```

The file already imports `Link` from `"react-router-dom"` — verify it is there. If not, add it to the react-router-dom import line.

- [ ] **Step 2.2: Replace the StatusHero function body**

Find the entire `function StatusHero()` (lines 56–138 in the current file). Replace it with:

```typescript
function StatusHero() {
  const status = useOptimizerStatus();
  const session = status?.active_session ?? null;
  const state = session?.state ?? "idle";
  const isRunning = state === "running";
  const isPaused = state === "paused";
  const isCancelling = state === "cancelling";
  const isActive = isRunning || isPaused || isCancelling;

  const pauseMutation = usePauseSession();
  const resumeMutation = useResumeSession();
  const cancelMutation = useCancelSession();

  return (
    <div className="rounded-md border border-border bg-surface-card px-5 py-4 space-y-3">
      <div className="flex items-start justify-between gap-4 flex-wrap">
        <div className="space-y-1.5">
          <div className="flex items-center gap-2">
            <span className="uppercase tracking-[0.22em] text-[9.5px] text-text-3 font-medium">
              Optimizer
            </span>
            <StatePill state={state} />
          </div>
          {isActive && session ? (
            <h2 className="text-lg font-semibold tracking-tight text-text">
              Run {session.session_id.slice(0, 8)} · {session.strategy_id} ·{" "}
              {modeLabel(session.mode)}
            </h2>
          ) : (
            <h2 className="text-lg font-semibold tracking-tight text-text-3">
              No run in progress
            </h2>
          )}
          {isActive && session && (
            <p className="font-mono text-[11.5px] text-text-3">
              {session.cycles_completed} cycles · {session.kept_count} kept ·{" "}
              {session.suspect_count} suspect · {session.dropped_count} dropped
            </p>
          )}
        </div>
        <div className="flex items-center gap-2 flex-wrap justify-end">
          {/* "Watch live →" appears for running, paused, and cancelling states */}
          {isActive && session && (
            <Link
              to={`/optimizer/run/${session.session_id}`}
              className="text-[13px] text-accent hover:underline"
            >
              Watch live →
            </Link>
          )}
          {/* Action buttons only for running (Pause + Cancel) and paused (Resume + Cancel) */}
          {isRunning && session && (
            <>
              <button
                type="button"
                onClick={() => pauseMutation.mutate(session.session_id)}
                disabled={pauseMutation.isPending}
                className="rounded border border-border px-3 py-1.5 text-[13px] text-text-2 hover:bg-surface-elev/40 transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
              >
                Pause
              </button>
              <button
                type="button"
                onClick={() => cancelMutation.mutate(session.session_id)}
                disabled={cancelMutation.isPending}
                className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
              >
                Cancel
              </button>
            </>
          )}
          {isPaused && session && (
            <>
              <button
                type="button"
                onClick={() => resumeMutation.mutate(session.session_id)}
                disabled={resumeMutation.isPending}
                className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90 transition-opacity disabled:opacity-60 disabled:cursor-not-allowed"
              >
                Resume
              </button>
              <button
                type="button"
                onClick={() => cancelMutation.mutate(session.session_id)}
                disabled={cancelMutation.isPending}
                className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
              >
                Cancel
              </button>
            </>
          )}
        </div>
      </div>
      {isRunning && (
        <PhaseStepper currentPhase={null} completedPhases={[]} />
      )}
    </div>
  );
}
```

---

## Task 3: Remove LiveCycleView from the page root

**Files:**
- Modify: `frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx`

- [ ] **Step 3.1: Remove the LiveCycleView import**

Find the import line:
```typescript
import { LiveCycleView } from "../LiveCycleView";
```
Delete it entirely.

- [ ] **Step 3.2: Remove the LiveCycleView JSX from the page root**

In the `OptimizerHome` function (around line 312–313), find and delete these two lines:
```tsx
        {/* In-flight cycle + live event feed (existing dashboard body). */}
        <LiveCycleView embedded />
```

The `ExperimentWritersPanel` and `RecentCyclesTable` that follow remain in place.

---

## Task 4: Verify tests pass

- [ ] **Step 4.1: Run the full OptimizerHome test suite**

```bash
cd /Users/edkennedy/Code/xvision/frontend/web
pnpm test -- --reporter=verbose OptimizerHome.test 2>&1 | tail -60
```

Expected: All tests PASS, including all 6 new tests in "OptimizerHome — LiveCycleView removed (Level-2 IA fix)".

If any existing tests fail, check:
- "shows Pause + Cancel buttons when a session is active" — these now call real mutations; the test must mock `usePauseSession`/`useResumeSession`/`useCancelSession`. The new `mockMutations()` helper in the test file covers NEW tests only. If existing tests break, add `mockMutations()` to their setup too.

- [ ] **Step 4.2: Run TypeScript check**

```bash
cd /Users/edkennedy/Code/xvision/frontend/web
pnpm tsc --noEmit 2>&1 | head -30
```

Expected: No errors. If `LiveCycleView` is still referenced anywhere in `OptimizerHome.tsx`, TypeScript will report a missing import.

- [ ] **Step 4.3: Run the broader autooptimizer test suite**

```bash
cd /Users/edkennedy/Code/xvision/frontend/web
pnpm test -- --reporter=verbose features/autooptimizer 2>&1 | tail -40
```

Expected: All tests pass. No regression in other autooptimizer test files.

---

## Task 5: Commit

- [ ] **Step 5.1: Stage and commit**

```bash
cd /Users/edkennedy/Code/xvision
git add frontend/web/src/features/autooptimizer/screens/OptimizerHome.tsx
git add frontend/web/src/features/autooptimizer/screens/OptimizerHome.test.tsx
git commit -m "fix(optimizer): remove LiveCycleView from home, wire controls + Watch live link

The embedded LiveCycleView widget tracked eval/backtest cycle state via SSE
independently of the optimizer session state shown in StatusHero. This made
the home page show 'Optimizer → Idle' alongside 'Live · cycle in progress',
looking like a contradiction or two separate systems.

- Remove <LiveCycleView embedded /> from OptimizerHome
- Wire Pause/Cancel/Resume mutations in StatusHero (were disabled/no-op)
- Add 'Watch live →' link to /optimizer/run/:sessionId when active
- Tests: verify no LiveCycleView content, link href, mutation call sites"
```

---

## Self-review

**Spec coverage:**
- ✅ Remove LiveCycleView from home → Task 3
- ✅ Wire Pause/Cancel/Resume controls → Task 2 (Step 2.2)
- ✅ "Watch live →" link to RunDetail → Task 2 (Step 2.2)
- ✅ TDD: tests written before implementation → Task 1 before Task 2/3

**Placeholder scan:** None — all code blocks are complete and explicit.

**Type consistency:**
- `session.session_id` (type `string`) passed to all three mutations (`(sessionId: string) => …`) — consistent.
- `usePauseSession`, `useResumeSession`, `useCancelSession` are exported from `../api` — confirmed.
- `Link` from `react-router-dom` — file already imports it.

**`cancelling` state design decision (explicit):** When `state === "cancelling"`, `isActive = true`, so the "Watch live →" link renders. `isRunning` and `isPaused` are both false, so no action buttons render. This matches RunDetail's `ControlsRow` (which returns null unless running or paused) and gives operators visibility into the cancellation in progress without offering buttons that can't do anything useful.

**`isPending` disabled guard:** The code explicitly sets `disabled={pauseMutation.isPending}`, `disabled={resumeMutation.isPending}`, and `disabled={cancelMutation.isPending}` on all three buttons. Two tests verify the disabled state for Pause and Cancel (the most critical paths); Resume follows the same pattern.

**Existing test regression risk:** The existing "shows Running pill and Pause + Cancel buttons" test clicks buttons that were previously `disabled`. After Task 2, those buttons have `onClick` handlers. The test doesn't click them — it only checks they're in the DOM — so it will still pass. However, `usePauseSession`/`useResumeSession`/`useCancelSession` are now called unconditionally in `StatusHero`. When those tests render `OptimizerHome` without mocking these hooks, TanStack Query's real `useMutation` runs (with no network calls triggered), which is fine in tests. **No regression expected**, but if any test fails on "missing mutation mock", add `mockMutations()` to that test's setup.
