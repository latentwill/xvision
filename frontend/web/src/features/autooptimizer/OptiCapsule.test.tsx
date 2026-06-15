// frontend/web/src/features/autooptimizer/OptiCapsule.test.tsx
//
// WS-11a: the OptiCapsule renders the live autooptimizer cycle as a floating
// trace capsule on the /optimizer route. It shares the CapsuleShell + CapsuleRow
// chrome with the eval/live capsules (same visual language) and projects its
// rows from the existing cycle SSE stream via the OPTI reducer.

import { afterEach, describe, expect, test } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import type { ReactElement } from "react";
import { OptiCapsule } from "./OptiCapsule";
import { projectOptiRows } from "./opti-trace-reducer";
import type { CycleProgressEvent } from "./api";

function renderCapsule(node: ReactElement) {
  return render(<MemoryRouter>{node}</MemoryRouter>);
}

afterEach(() => cleanup());

const runningCycle: CycleProgressEvent[] = [
  { event_type: "cycle_started", cycle_id: "cyc_live", parent_count: 1, ts: "2026-06-13T10:00:00Z" },
  { event_type: "parent_selected", cycle_id: "cyc_live", parent_hash: "par111", ts: "2026-06-13T10:00:01Z" },
  {
    event_type: "mutation_proposed",
    cycle_id: "cyc_live",
    child_hash: "exp111",
    mutator_model: "claude-haiku",
    ts: "2026-06-13T10:00:02Z",
  },
  {
    event_type: "mutation_gated",
    cycle_id: "cyc_live",
    child_hash: "exp111",
    outcome: "kept",
    passed: true,
    delta_day: 0.4,
    ts: "2026-06-13T10:00:03Z",
  },
];

describe("OptiCapsule", () => {
  test("renders the OPTI prefix + a focused cycle row", () => {
    const rows = projectOptiRows(runningCycle);
    renderCapsule(<OptiCapsule rows={rows} cycleId="cyc_live" running />);
    const cap = screen.getByTestId("opti-capsule");
    expect(within(cap).getByText("OPTI")).toBeInTheDocument();
  });

  test("renders cycle phase rows projected from the event stream", () => {
    const rows = projectOptiRows(runningCycle);
    renderCapsule(<OptiCapsule rows={rows} cycleId="cyc_live" running />);
    const cap = screen.getByTestId("opti-capsule");
    // The operator-labelled phase rows appear (Experiment proposed + the gate
    // outcome). They come from formatEventLabel / optiSpanLabel, not raw kinds.
    // "Active" legitimately appears twice — in the phase row AND the live
    // current-phase chip (the kept gate is the most-recent row).
    expect(within(cap).getByText(/Experiment proposed/)).toBeInTheDocument();
    expect(within(cap).getAllByText(/Active/).length).toBeGreaterThanOrEqual(1);
  });

  test("surfaces the current phase chip while running", () => {
    const rows = projectOptiRows(runningCycle);
    renderCapsule(<OptiCapsule rows={rows} cycleId="cyc_live" running />);
    const cap = screen.getByTestId("opti-capsule");
    const chip = within(cap).getByTestId("opti-capsule-current-phase");
    expect(chip).toBeInTheDocument();
  });

  test("renders an idle empty state when there are no rows", () => {
    renderCapsule(<OptiCapsule rows={[]} cycleId={null} running={false} />);
    expect(screen.queryByTestId("opti-capsule")).toBeNull();
  });

  test("a finished cycle freezes (no pulsing running tone)", () => {
    const finished = [
      ...runningCycle,
      {
        event_type: "cycle_finished",
        cycle_id: "cyc_live",
        active_count: 1,
        suspect_count: 0,
        rejected_count: 0,
        ts: "2026-06-13T10:00:05Z",
      },
    ];
    const rows = projectOptiRows(finished);
    renderCapsule(<OptiCapsule rows={rows} cycleId="cyc_live" running={false} />);
    const cap = screen.getByTestId("opti-capsule");
    expect(cap.getAttribute("data-tone")).not.toBe("running");
  });
});
