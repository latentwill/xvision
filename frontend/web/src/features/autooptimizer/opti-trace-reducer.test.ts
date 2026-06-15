// frontend/web/src/features/autooptimizer/opti-trace-reducer.test.ts
//
// WS-11a: the OPTI scope reducer projects the existing optimizer cycle SSE
// stream (CycleProgressEvent) into trace-dock rows (RunSpan[]) under a
// dedicated `opti.*` kind taxonomy. This is render/scope wiring on top of the
// existing instrumentation — no new event types are invented here.
//
// The reducer reuses `formatEventLabel` (the operator-surface label map) for
// the span name and carries each event's salient fields (gate outcome +
// day-Sharpe delta, judge severity/code, honesty pass/fail) into
// `span.attributes` for the inspector.

import { describe, expect, test } from "vitest";
import type { CycleProgressEvent } from "./api";
import { projectOptiRows, OPTI_CYCLE_ROOT_ID } from "./opti-trace-reducer";

// A representative cycle sequence covering every phase the contract names.
function sampleCycle(): CycleProgressEvent[] {
  return [
    { event_type: "cycle_started", cycle_id: "cyc_1", parent_count: 2, ts: "2026-06-13T10:00:00Z" },
    { event_type: "parent_selected", cycle_id: "cyc_1", parent_hash: "parentaaaa1111", ts: "2026-06-13T10:00:01Z" },
    {
      event_type: "mutation_proposed",
      cycle_id: "cyc_1",
      parent_hash: "parentaaaa1111",
      child_hash: "kept0000aaaa",
      mutator_model: "claude-haiku",
      ts: "2026-06-13T10:00:02Z",
    },
    {
      event_type: "mutation_gated",
      cycle_id: "cyc_1",
      child_hash: "kept0000aaaa",
      outcome: "kept",
      passed: true,
      delta_day: 0.42,
      ts: "2026-06-13T10:00:03Z",
    },
    {
      event_type: "mutation_proposed",
      cycle_id: "cyc_1",
      parent_hash: "parentaaaa1111",
      child_hash: "suspect00bbbb",
      mutator_model: "claude-haiku",
      ts: "2026-06-13T10:00:04Z",
    },
    {
      event_type: "mutation_gated",
      cycle_id: "cyc_1",
      child_hash: "suspect00bbbb",
      outcome: "suspect",
      delta_day: 0.05,
      ts: "2026-06-13T10:00:05Z",
    },
    {
      event_type: "mutation_proposed",
      cycle_id: "cyc_1",
      parent_hash: "parentaaaa1111",
      child_hash: "dropped00cccc",
      mutator_model: "claude-haiku",
      ts: "2026-06-13T10:00:06Z",
    },
    {
      event_type: "mutation_gated",
      cycle_id: "cyc_1",
      child_hash: "dropped00cccc",
      passed: false,
      delta_day: -0.31,
      ts: "2026-06-13T10:00:07Z",
    },
    {
      event_type: "honesty_check_run",
      cycle_id: "cyc_1",
      passed: true,
      message: "sabotaged variant `kill-trades` correctly scored worse",
      ts: "2026-06-13T10:00:08Z",
    },
    {
      event_type: "judge_finding",
      cycle_id: "cyc_1",
      child_hash: "kept0000aaaa",
      severity: "warn",
      code: "overfit_risk",
      ts: "2026-06-13T10:00:09Z",
    },
    {
      event_type: "flywheel_compiled",
      cycle_id: "cyc_1",
      ts: "2026-06-13T10:00:10Z",
    },
    {
      event_type: "cycle_finished",
      cycle_id: "cyc_1",
      active_count: 1,
      suspect_count: 1,
      rejected_count: 1,
      ts: "2026-06-13T10:00:11Z",
    },
  ];
}

describe("projectOptiRows — opti.* kind taxonomy", () => {
  test("emits a single opti.cycle root carrying the cycle_id", () => {
    const rows = projectOptiRows(sampleCycle());
    const roots = rows.filter((r) => r.kind === "opti.cycle");
    expect(roots).toHaveLength(1);
    expect(roots[0].span_id).toBe(OPTI_CYCLE_ROOT_ID("cyc_1"));
    expect(roots[0].parent_span_id).toBeNull();
    expect(roots[0].attributes.cycle_id).toBe("cyc_1");
  });

  test("cycle root closes (finished_at + ok) when CycleFinished arrives", () => {
    const rows = projectOptiRows(sampleCycle());
    const root = rows.find((r) => r.kind === "opti.cycle")!;
    expect(root.finished_at).not.toBeNull();
    expect(root.status).toBe("ok");
    // tallies from cycle_finished surface in attributes for the inspector.
    expect(root.attributes.active_count).toBe(1);
    expect(root.attributes.suspect_count).toBe(1);
    expect(root.attributes.rejected_count).toBe(1);
  });

  test("parent_selected projects an opti.parent row under the cycle", () => {
    const rows = projectOptiRows(sampleCycle());
    const parent = rows.find((r) => r.kind === "opti.parent")!;
    expect(parent).toBeDefined();
    expect(parent.parent_span_id).toBe(OPTI_CYCLE_ROOT_ID("cyc_1"));
    expect(parent.attributes.parent_hash).toBe("parentaaaa1111");
  });

  test("each mutation_proposed → an opti.experiment row nested under the cycle", () => {
    const rows = projectOptiRows(sampleCycle());
    const experiments = rows.filter((r) => r.kind === "opti.experiment");
    expect(experiments).toHaveLength(3);
    for (const e of experiments) {
      expect(e.parent_span_id).toBe(OPTI_CYCLE_ROOT_ID("cyc_1"));
    }
    const hashes = experiments.map((e) => e.attributes.child_hash);
    expect(hashes).toEqual(["kept0000aaaa", "suspect00bbbb", "dropped00cccc"]);
  });

  test("each gate row nests under its experiment (matched by child_hash)", () => {
    const rows = projectOptiRows(sampleCycle());
    const experiments = rows.filter((r) => r.kind === "opti.experiment");
    const gates = rows.filter((r) => r.kind === "opti.gate");
    expect(gates).toHaveLength(3);
    for (const g of gates) {
      const parentExp = experiments.find((e) => e.span_id === g.parent_span_id);
      expect(parentExp).toBeDefined();
      expect(parentExp!.attributes.child_hash).toBe(g.attributes.child_hash);
    }
  });

  test("kept gate carries outcome=kept + ΔSharpe and an ok status (Active tone)", () => {
    const rows = projectOptiRows(sampleCycle());
    const keptGate = rows.find(
      (r) => r.kind === "opti.gate" && r.attributes.child_hash === "kept0000aaaa",
    )!;
    expect(keptGate.attributes.outcome).toBe("kept");
    expect(keptGate.attributes.delta_day).toBe(0.42);
    expect(keptGate.status).toBe("ok");
  });

  test("suspect gate carries outcome=suspect (warn tone via in_progress→warn mapping)", () => {
    const rows = projectOptiRows(sampleCycle());
    const suspectGate = rows.find(
      (r) => r.kind === "opti.gate" && r.attributes.child_hash === "suspect00bbbb",
    )!;
    expect(suspectGate.attributes.outcome).toBe("suspect");
    expect(suspectGate.attributes.delta_day).toBe(0.05);
  });

  test("rejected gate carries outcome=rejected + error status (muted/Rejected tone)", () => {
    const rows = projectOptiRows(sampleCycle());
    const droppedGate = rows.find(
      (r) => r.kind === "opti.gate" && r.attributes.child_hash === "dropped00cccc",
    )!;
    expect(droppedGate.attributes.outcome).toBe("rejected");
    expect(droppedGate.attributes.delta_day).toBe(-0.31);
    expect(droppedGate.status).toBe("error");
  });

  test("honesty_check_run → an opti.honesty row under the cycle with pass + message", () => {
    const rows = projectOptiRows(sampleCycle());
    const honesty = rows.find((r) => r.kind === "opti.honesty")!;
    expect(honesty.parent_span_id).toBe(OPTI_CYCLE_ROOT_ID("cyc_1"));
    expect(honesty.attributes.passed).toBe(true);
    expect(honesty.attributes.message).toMatch(/kill-trades/);
  });

  test("judge_finding → an opti.judge row nested under its experiment with severity+code", () => {
    const rows = projectOptiRows(sampleCycle());
    const judge = rows.find((r) => r.kind === "opti.judge")!;
    const experiments = rows.filter((r) => r.kind === "opti.experiment");
    const parentExp = experiments.find((e) => e.span_id === judge.parent_span_id);
    expect(parentExp).toBeDefined();
    expect(parentExp!.attributes.child_hash).toBe("kept0000aaaa");
    expect(judge.attributes.severity).toBe("warn");
    expect(judge.attributes.code).toBe("overfit_risk");
  });

  test("flywheel_compiled → an opti.flywheel row under the cycle", () => {
    const rows = projectOptiRows(sampleCycle());
    const flywheel = rows.find((r) => r.kind === "opti.flywheel")!;
    expect(flywheel.parent_span_id).toBe(OPTI_CYCLE_ROOT_ID("cyc_1"));
  });

  test("every row carries the operator label as its name (formatEventLabel)", () => {
    const rows = projectOptiRows(sampleCycle());
    const cycleRoot = rows.find((r) => r.kind === "opti.cycle")!;
    expect(cycleRoot.name).toBe("Optimizer run finished");
    const proposed = rows.find((r) => r.kind === "opti.experiment")!;
    expect(proposed.name).toBe("Experiment proposed");
    const honesty = rows.find((r) => r.kind === "opti.honesty")!;
    // formatEventLabel returns the message for honesty_check_run when present.
    expect(honesty.name).toMatch(/kill-trades|Honesty check/);
  });

  test("a judge_finding whose hash matches no experiment falls back under the cycle root", () => {
    const events: CycleProgressEvent[] = [
      { event_type: "cycle_started", cycle_id: "cyc_2", ts: "2026-06-13T11:00:00Z" },
      {
        event_type: "judge_finding",
        cycle_id: "cyc_2",
        child_hash: "orphanhash999",
        severity: "info",
        code: "note",
        ts: "2026-06-13T11:00:01Z",
      },
    ];
    const rows = projectOptiRows(events);
    const judge = rows.find((r) => r.kind === "opti.judge")!;
    expect(judge.parent_span_id).toBe(OPTI_CYCLE_ROOT_ID("cyc_2"));
  });

  // WS-11b — nest each candidate's persisted eval run under its experiment.
  describe("eval-run nesting (WS-11b)", () => {
    test("a gate carrying eval_run_id puts the id on the experiment + emits a nested opti.eval-run node", () => {
      const events: CycleProgressEvent[] = [
        { event_type: "cycle_started", cycle_id: "cyc_e1", ts: "2026-06-14T10:00:00Z" },
        {
          event_type: "mutation_proposed",
          cycle_id: "cyc_e1",
          parent_hash: "parent0001",
          child_hash: "child0001",
          mutator_model: "claude-haiku",
          ts: "2026-06-14T10:00:01Z",
        },
        {
          event_type: "mutation_gated",
          cycle_id: "cyc_e1",
          child_hash: "child0001",
          outcome: "kept",
          passed: true,
          delta_day: 0.21,
          eval_run_id: "01EVALRUNULID",
          ts: "2026-06-14T10:00:02Z",
        },
      ];
      const rows = projectOptiRows(events);
      const experiment = rows.find((r) => r.kind === "opti.experiment")!;
      expect(experiment).toBeDefined();
      // The eval_run_id is surfaced on the experiment row's attributes.
      expect(experiment.attributes.eval_run_id).toBe("01EVALRUNULID");

      // A navigable eval-run node is nested under the experiment.
      const evalRun = rows.find((r) => r.kind === "opti.eval-run")!;
      expect(evalRun).toBeDefined();
      expect(evalRun.parent_span_id).toBe(experiment.span_id);
      // It carries the run id so the inspector can drill to its trace.
      expect(evalRun.attributes.eval_run_id).toBe("01EVALRUNULID");
    });

    test("a gate WITHOUT eval_run_id leaves the experiment with no eval-run child (no dangling node)", () => {
      const events: CycleProgressEvent[] = [
        { event_type: "cycle_started", cycle_id: "cyc_e2", ts: "2026-06-14T11:00:00Z" },
        {
          event_type: "mutation_proposed",
          cycle_id: "cyc_e2",
          parent_hash: "parent0002",
          child_hash: "child0002",
          mutator_model: "claude-haiku",
          ts: "2026-06-14T11:00:01Z",
        },
        {
          event_type: "mutation_gated",
          cycle_id: "cyc_e2",
          child_hash: "child0002",
          outcome: "dropped",
          passed: false,
          delta_day: -0.1,
          // no eval_run_id (regime path / test-stub runner)
          ts: "2026-06-14T11:00:02Z",
        },
      ];
      const rows = projectOptiRows(events);
      const experiment = rows.find((r) => r.kind === "opti.experiment")!;
      expect(experiment).toBeDefined();
      expect(experiment.attributes.eval_run_id).toBeUndefined();
      // No eval-run node was synthesized.
      expect(rows.find((r) => r.kind === "opti.eval-run")).toBeUndefined();
    });

    test("a NoCandidate experiment row renders fine with no eval-run child", () => {
      const events: CycleProgressEvent[] = [
        { event_type: "cycle_started", cycle_id: "cyc_e3", ts: "2026-06-14T12:00:00Z" },
        {
          event_type: "no_candidate",
          cycle_id: "cyc_e3",
          parent_hash: "parent0003",
          reason: "writer produced only no-op diffs",
          ts: "2026-06-14T12:00:01Z",
        },
      ];
      const rows = projectOptiRows(events);
      const experiment = rows.find((r) => r.kind === "opti.experiment")!;
      expect(experiment).toBeDefined();
      expect(rows.find((r) => r.kind === "opti.eval-run")).toBeUndefined();
    });

    test("a gate whose eval_run_id matches no experiment still nests the eval-run under the cycle root (no orphan)", () => {
      // Page reload lands mid-cycle: only the gate event is buffered, so the
      // experiment row is synthesized by the gate's fallback. The eval-run
      // should still parent off whatever experiment id the gate resolves to.
      const events: CycleProgressEvent[] = [
        { event_type: "cycle_started", cycle_id: "cyc_e4", ts: "2026-06-14T13:00:00Z" },
        {
          event_type: "mutation_gated",
          cycle_id: "cyc_e4",
          child_hash: "orphan0004",
          outcome: "kept",
          passed: true,
          eval_run_id: "01ORPHANRUN",
          ts: "2026-06-14T13:00:01Z",
        },
      ];
      const rows = projectOptiRows(events);
      const evalRun = rows.find((r) => r.kind === "opti.eval-run");
      expect(evalRun).toBeDefined();
      // Its parent must exist in the row set (no dangling parent_span_id).
      const parent = rows.find((r) => r.span_id === evalRun!.parent_span_id);
      expect(parent).toBeDefined();
    });
  });

  test("empty stream → empty rows (no synthetic cycle)", () => {
    expect(projectOptiRows([])).toEqual([]);
  });

  test("events without a leading cycle_started still synthesize a cycle root", () => {
    // A page reload mid-run starts the SSE buffer fresh — the first event the
    // dock sees may be a mutation_gated. We still want a cycle root to hang
    // rows under so the tree is well-formed.
    const events: CycleProgressEvent[] = [
      {
        event_type: "mutation_gated",
        cycle_id: "cyc_3",
        child_hash: "h1",
        passed: true,
        outcome: "kept",
        ts: "2026-06-13T12:00:00Z",
      },
    ];
    const rows = projectOptiRows(events);
    expect(rows.find((r) => r.kind === "opti.cycle")).toBeDefined();
    expect(rows.find((r) => r.kind === "opti.gate")).toBeDefined();
  });
});
