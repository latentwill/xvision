// frontend/web/src/features/autooptimizer/OptiCapsuleSlot.test.tsx
//
// WS-11a reachability test: the OptiCapsuleSlot mounts on the /optimizer route
// and renders the live cycle as trace rows projected from the cycle events fed
// in by OptimizerHome's single SSE subscription (`useCycleEventStream`). The
// slot takes the stream result as props — it never opens its own EventSource —
// and writes only `byScope.opti`.

import { afterEach, beforeEach, describe, expect, test } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import type { EventRow } from "./hooks/useCycleEventStream";
import { OptiCapsuleSlot } from "./OptiCapsuleSlot";
import { useTraceDock } from "@/stores/trace-dock";

function row(e: Record<string, unknown>, id: number): EventRow {
  return { ...e, _row_id: id } as EventRow;
}

const LIVE_EVENTS: EventRow[] = [
  row({ event_type: "cycle_started", cycle_id: "cyc_reach", parent_count: 1, ts: "2026-06-13T10:00:00Z" }, 1),
  row({ event_type: "parent_selected", cycle_id: "cyc_reach", parent_hash: "par1", ts: "2026-06-13T10:00:01Z" }, 2),
  row(
    {
      event_type: "mutation_proposed",
      cycle_id: "cyc_reach",
      child_hash: "exp1",
      mutator_model: "claude-haiku",
      ts: "2026-06-13T10:00:02Z",
    },
    3,
  ),
  row(
    {
      event_type: "mutation_gated",
      cycle_id: "cyc_reach",
      child_hash: "exp1",
      outcome: "kept",
      passed: true,
      delta_day: 0.4,
      ts: "2026-06-13T10:00:03Z",
    },
    4,
  ),
];

function mountAtOptimizer(props: {
  events: EventRow[];
  activeCycleId: string | null;
  isRunning: boolean;
}) {
  return render(
    <MemoryRouter initialEntries={["/optimizer"]}>
      <Routes>
        <Route path="/optimizer" element={<OptiCapsuleSlot {...props} />} />
      </Routes>
    </MemoryRouter>,
  );
}

beforeEach(() => {
  // Reset the opti scope; seed eval/live sentinels to prove isolation.
  useTraceDock.getState().setActiveRun("opti", null, "post-hoc");
  useTraceDock.getState().setActiveRun("eval", "E_sentinel", "post-hoc");
  useTraceDock.getState().setActiveRun("live", "L_sentinel", "live");
});

afterEach(() => cleanup());

describe("OptiCapsuleSlot — /optimizer reachability", () => {
  test("mounts on /optimizer and renders projected cycle rows", () => {
    mountAtOptimizer({ events: LIVE_EVENTS, activeCycleId: "cyc_reach", isRunning: true });

    const cap = screen.getByTestId("opti-capsule");
    expect(within(cap).getByText("OPTI")).toBeInTheDocument();
    expect(within(cap).getByText(/Experiment proposed/)).toBeInTheDocument();
  });

  test("writes only byScope.opti — eval/live sentinels are untouched", () => {
    mountAtOptimizer({ events: LIVE_EVENTS, activeCycleId: "cyc_reach", isRunning: true });

    expect(useTraceDock.getState().byScope.opti.activeRunId).toBe("cyc_reach");
    // The other scopes' sentinels survive — the OPTI surface never writes them.
    expect(useTraceDock.getState().byScope.eval.activeRunId).toBe("E_sentinel");
    expect(useTraceDock.getState().byScope.live.activeRunId).toBe("L_sentinel");
  });

  test("idle (no active cycle) renders nothing", () => {
    mountAtOptimizer({ events: [], activeCycleId: null, isRunning: false });
    expect(screen.queryByTestId("opti-capsule")).toBeNull();
  });
});
