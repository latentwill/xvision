// frontend/web/src/stores/trace-dock.test.ts
import { beforeEach, describe, expect, test, vi } from "vitest";
import {
  DOCK_HEIGHT_STORAGE_KEY,
  DOCK_COLLAPSED_SPANS_STORAGE_KEY,
  DOCK_MIN_PX,
  DEFAULT_DOCK_PX,
  clampDockPx,
  dockMaxPx,
  readPersistedCollapsedSpanIds,
  useTraceDock,
} from "./trace-dock";
import type {
  AgentRunDetail,
  RunSpan,
} from "@/api/types-agent-runs";

function span(overrides: Partial<RunSpan> = {}): RunSpan {
  return {
    span_id: "s_a",
    parent_span_id: null,
    name: "model.call gpt-5",
    kind: "model.call",
    started_at: "2026-05-17T10:00:00.000Z",
    finished_at: null,
    status: "in_progress",
    attributes: {},
    ...overrides,
  };
}

function resetStore() {
  useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
  useTraceDock.getState().setActiveRun("live", null, "post-hoc");
}

describe("trace-dock store — dock shell", () => {
  beforeEach(resetStore);

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

  test("setActiveRun resets selectedSpan within its scope", () => {
    useTraceDock.getState().setSelectedSpan("eval", "s5");
    useTraceDock.getState().setActiveRun("eval", "run_other", "post-hoc");
    expect(useTraceDock.getState().byScope.eval.selectedSpanId).toBeNull();
    expect(useTraceDock.getState().byScope.eval.activeRunId).toBe("run_other");
    expect(useTraceDock.getState().byScope.eval.mode).toBe("post-hoc");
  });
});

describe("trace-dock store — per-scope state", () => {
  beforeEach(resetStore);

  test("setActiveRun on eval leaves the live scope untouched", () => {
    useTraceDock.getState().setActiveRun("eval", "A", "live");
    expect(useTraceDock.getState().byScope.eval.activeRunId).toBe("A");
    expect(useTraceDock.getState().byScope.eval.mode).toBe("live");
    // Live scope stays at its init values.
    expect(useTraceDock.getState().byScope.live.activeRunId).toBeNull();
    expect(useTraceDock.getState().byScope.live.mode).toBe("post-hoc");
  });

  test("setActiveRun on live leaves the eval scope untouched", () => {
    useTraceDock.getState().setActiveRun("eval", "A", "post-hoc");
    useTraceDock.getState().setActiveRun("live", "B", "live");
    expect(useTraceDock.getState().byScope.live.activeRunId).toBe("B");
    // Eval scope keeps its earlier run.
    expect(useTraceDock.getState().byScope.eval.activeRunId).toBe("A");
  });

  test("nulling the eval scope does not affect the live scope", () => {
    useTraceDock.getState().setActiveRun("eval", "A", "post-hoc");
    useTraceDock.getState().setActiveRun("live", "B", "live");
    useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
    expect(useTraceDock.getState().byScope.eval.activeRunId).toBeNull();
    // Live run survives the eval-side cleanup — the bug this reshape fixes.
    expect(useTraceDock.getState().byScope.live.activeRunId).toBe("B");
  });

  test("setSelectedSpan is per-scope", () => {
    useTraceDock.getState().setSelectedSpan("eval", "s_eval");
    useTraceDock.getState().setSelectedSpan("live", "s_live");
    expect(useTraceDock.getState().byScope.eval.selectedSpanId).toBe("s_eval");
    expect(useTraceDock.getState().byScope.live.selectedSpanId).toBe("s_live");
  });

  test("setCostOverrideUsd is per-scope", () => {
    useTraceDock.getState().setCostOverrideUsd("eval", 0.11);
    useTraceDock.getState().setCostOverrideUsd("live", 0.22);
    expect(useTraceDock.getState().byScope.eval.costOverrideUsd).toBe(0.11);
    expect(useTraceDock.getState().byScope.live.costOverrideUsd).toBe(0.22);
  });
});

describe("trace-dock store — heightPx slice", () => {
  beforeEach(() => {
    localStorage.clear();
    useTraceDock.setState({ heightPx: DEFAULT_DOCK_PX });
  });

  test("clampDockPx enforces min and max bounds", () => {
    expect(clampDockPx(10)).toBe(DOCK_MIN_PX);
    expect(clampDockPx(10_000)).toBe(dockMaxPx());
    expect(clampDockPx(Number.NaN)).toBe(DEFAULT_DOCK_PX);
  });

  test("setHeightPx clamps and writes localStorage", () => {
    useTraceDock.getState().setHeightPx(20);
    expect(useTraceDock.getState().heightPx).toBe(DOCK_MIN_PX);
    expect(localStorage.getItem(DOCK_HEIGHT_STORAGE_KEY)).toBe(
      String(DOCK_MIN_PX),
    );

    useTraceDock.getState().setHeightPx(640);
    expect(useTraceDock.getState().heightPx).toBe(640);
    expect(localStorage.getItem(DOCK_HEIGHT_STORAGE_KEY)).toBe("640");
  });
});

describe("trace-dock store — streamingState", () => {
  beforeEach(resetStore);

  test("snapshot frame seeds activeSpanIds from in-flight spans only", () => {
    const detail: AgentRunDetail = {
      summary: {
        run_id: "run_x",
        objective: "test",
        strategy_id: null,
        agent_id: null,
        started_at: "2026-05-17T10:00:00.000Z",
        finished_at: null,
        status: "running",
        span_count: 2,
        model_call_count: 0,
        tool_call_count: 0,
        error_count: 0,
        total_cost_usd: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        duration_ms: null,
        financial_eval_id: null,
        retention_mode: "hash_only",
      },
      spans: [
        span({ span_id: "s_done", finished_at: "2026-05-17T10:00:00.500Z" }),
        span({ span_id: "s_live", started_at: "2026-05-17T10:00:01.000Z" }),
      ],
      model_calls: [],
      tool_calls: [],
    };

    useTraceDock.getState().applyStreamEvent({ event: "snapshot", data: detail });

    const s = useTraceDock.getState().streamingState;
    expect([...s.activeSpanIds]).toEqual(["s_live"]);
    expect(s.activeSpanMeta.s_live).toEqual({
      name: "model.call gpt-5",
      started_at: "2026-05-17T10:00:01.000Z",
      kind: "model.call",
    });
  });

  test("appendDelta accumulates per-span delta_len; multiple frames sum", () => {
    useTraceDock.getState().applyStreamEvent({
      event: "assistant_text_delta",
      data: { span_id: "s_a", run_id: "run_x", delta_len: 12 },
    });
    useTraceDock.getState().applyStreamEvent({
      event: "assistant_text_delta",
      data: { span_id: "s_a", run_id: "run_x", delta_len: 7 },
    });
    useTraceDock.getState().applyStreamEvent({
      event: "assistant_text_delta",
      data: { span_id: "s_b", run_id: "run_x", delta_len: 4 },
    });

    const deltas = useTraceDock.getState().streamingState.deltaCharsBySpan;
    expect(deltas.s_a).toBe(19);
    expect(deltas.s_b).toBe(4);
  });

  test("appendDelta concatenates delta_text per span across frames", () => {
    useTraceDock.getState().applyStreamEvent({
      event: "assistant_text_delta",
      data: {
        span_id: "s_body",
        run_id: "run_x",
        delta_len: 5,
        delta_text: "hello",
      },
    });
    useTraceDock.getState().applyStreamEvent({
      event: "assistant_text_delta",
      data: {
        span_id: "s_body",
        run_id: "run_x",
        delta_len: 6,
        delta_text: " world",
      },
    });

    const bodies = useTraceDock.getState().streamingState.bodiesBySpan;
    expect(bodies.s_body).toBe("hello world");
  });

  test("appendDelta tolerates missing delta_text without dropping the count", () => {
    useTraceDock.getState().applyStreamEvent({
      event: "assistant_text_delta",
      data: { span_id: "s_c", run_id: "run_x", delta_len: 3 },
    });

    const s = useTraceDock.getState().streamingState;
    expect(s.deltaCharsBySpan.s_c).toBe(3);
    expect(s.bodiesBySpan.s_c).toBeUndefined();
  });

  test("lagged increments droppedEvents and logs a console warning", () => {
    const spy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      useTraceDock.getState().applyStreamEvent({
        event: "lagged",
        data: { dropped: 3 },
      });
      useTraceDock.getState().applyStreamEvent({
        event: "lagged",
        data: { dropped: 2 },
      });
      expect(useTraceDock.getState().streamingState.droppedEvents).toBe(5);
      expect(spy).toHaveBeenCalledTimes(2);
    } finally {
      spy.mockRestore();
    }
  });

  test("span_started followed by span_finished clears the span from active", () => {
    useTraceDock.getState().applyStreamEvent({
      event: "span_started",
      data: {
        span_id: "s_x",
        run_id: "run_x",
        parent_span_id: null,
        kind: "tool.call",
        name: "execute_slot",
        started_at: "2026-05-17T10:00:02.000Z",
      },
    });
    expect(useTraceDock.getState().streamingState.activeSpanIds.has("s_x")).toBe(true);
    expect(useTraceDock.getState().streamingState.activeSpanMeta.s_x?.kind).toBe(
      "tool.call",
    );

    useTraceDock.getState().applyStreamEvent({
      event: "span_finished",
      data: {
        span_id: "s_x",
        ended_at: "2026-05-17T10:00:02.300Z",
        status: "ok",
      },
    });
    expect(useTraceDock.getState().streamingState.activeSpanIds.has("s_x")).toBe(false);
    expect(useTraceDock.getState().streamingState.activeSpanMeta.s_x).toBeUndefined();
  });

  test("reconnect snapshot REPLACES activeSpanIds — stale entries are dropped", () => {
    // Simulate the wire order: a first snapshot leaves us with one
    // active span, then the connection drops, and the resync snapshot
    // shows the span has finished while we were disconnected.
    useTraceDock.getState().markSpanActive("s_stale", {
      name: "stale span",
      started_at: "2026-05-17T10:00:00.000Z",
      kind: "model.call",
    });
    expect(useTraceDock.getState().streamingState.activeSpanIds.has("s_stale")).toBe(true);

    const detail: AgentRunDetail = {
      summary: {
        run_id: "run_x",
        objective: "test",
        strategy_id: null,
        agent_id: null,
        started_at: "2026-05-17T10:00:00.000Z",
        finished_at: null,
        status: "running",
        span_count: 1,
        model_call_count: 0,
        tool_call_count: 0,
        error_count: 0,
        total_cost_usd: 0,
        total_input_tokens: 0,
        total_output_tokens: 0,
        duration_ms: null,
        financial_eval_id: null,
        retention_mode: "hash_only",
      },
      // No in-flight spans in the resync.
      spans: [span({ span_id: "s_stale", finished_at: "2026-05-17T10:00:00.500Z" })],
      model_calls: [],
      tool_calls: [],
    };
    useTraceDock.getState().applyStreamEvent({ event: "snapshot", data: detail });
    const s = useTraceDock.getState().streamingState;
    expect([...s.activeSpanIds]).toEqual([]);
    expect(s.activeSpanMeta).toEqual({});
  });

  test("run_finished clears any lingering active spans", () => {
    useTraceDock.getState().markSpanActive("s_x", {
      name: "leftover",
      started_at: "2026-05-17T10:00:00.000Z",
      kind: "model.call",
    });
    useTraceDock.getState().applyStreamEvent({
      event: "run_finished",
      data: {
        run_id: "run_x",
        finished_at: "2026-05-17T10:00:05.000Z",
        status: "completed",
      },
    });
    const s = useTraceDock.getState().streamingState;
    expect([...s.activeSpanIds]).toEqual([]);
    expect(s.activeSpanMeta).toEqual({});
  });

  test("run_interrupted also clears active spans", () => {
    useTraceDock.getState().markSpanActive("s_y", {
      name: "leftover",
      started_at: "2026-05-17T10:00:00.000Z",
      kind: "tool.call",
    });
    useTraceDock.getState().applyStreamEvent({
      event: "run_interrupted",
      data: {
        run_id: "run_x",
        finished_at: "2026-05-17T10:00:05.000Z",
        reason: "user_halt",
      },
    });
    const s = useTraceDock.getState().streamingState;
    expect([...s.activeSpanIds]).toEqual([]);
  });

  test("backpressure_dropped increments droppedEvents like lagged", () => {
    const spy = vi.spyOn(console, "warn").mockImplementation(() => {});
    try {
      useTraceDock.getState().applyStreamEvent({
        event: "backpressure_dropped",
        data: { dropped: 4 },
      });
      expect(useTraceDock.getState().streamingState.droppedEvents).toBe(4);
      expect(spy).toHaveBeenCalled();
    } finally {
      spy.mockRestore();
    }
  });

  test("setActiveRun resets streaming slice between runs", () => {
    useTraceDock.getState().markSpanActive("s_old", {
      name: "old",
      started_at: "2026-05-17T10:00:00.000Z",
      kind: "model.call",
    });
    useTraceDock.getState().recordLag(2);
    useTraceDock.getState().setActiveRun("eval", "run_new", "live");
    const s = useTraceDock.getState().streamingState;
    expect([...s.activeSpanIds]).toEqual([]);
    expect(s.droppedEvents).toBe(0);
    expect(s.activeSpanMeta).toEqual({});
  });
});

describe("trace-dock store — costOverrideUsd", () => {
  beforeEach(resetStore);

  test("setCostOverrideUsd stores the eval-side cost", () => {
    expect(useTraceDock.getState().byScope.eval.costOverrideUsd).toBeNull();
    useTraceDock.getState().setCostOverrideUsd("eval", 0.4242);
    expect(useTraceDock.getState().byScope.eval.costOverrideUsd).toBe(0.4242);
    useTraceDock.getState().setCostOverrideUsd("eval", null);
    expect(useTraceDock.getState().byScope.eval.costOverrideUsd).toBeNull();
  });

  test("setActiveRun clears any pinned cost override for its scope", () => {
    useTraceDock.getState().setCostOverrideUsd("eval", 1.23);
    useTraceDock.getState().setActiveRun("eval", "run_next", "post-hoc");
    expect(useTraceDock.getState().byScope.eval.costOverrideUsd).toBeNull();
  });
});

describe("trace-dock store — collapsed span tree (WS-16)", () => {
  beforeEach(() => {
    localStorage.clear();
    // Reset the shared collapse slice to a clean state.
    useTraceDock.getState().setCollapsedSpanIds([]);
  });

  test("collapsedSpanIds defaults to empty", () => {
    expect(useTraceDock.getState().collapsedSpanIds).toEqual(new Set());
  });

  test("toggleSpanCollapsed flips a single node and persists to localStorage", () => {
    useTraceDock.getState().toggleSpanCollapsed("s1");
    expect(useTraceDock.getState().collapsedSpanIds.has("s1")).toBe(true);
    // Persisted as a JSON array under the collapsed-spans key.
    const raw = localStorage.getItem(DOCK_COLLAPSED_SPANS_STORAGE_KEY);
    expect(JSON.parse(raw ?? "[]")).toEqual(["s1"]);

    useTraceDock.getState().toggleSpanCollapsed("s1");
    expect(useTraceDock.getState().collapsedSpanIds.has("s1")).toBe(false);
    expect(JSON.parse(localStorage.getItem(DOCK_COLLAPSED_SPANS_STORAGE_KEY) ?? "[]")).toEqual([]);
  });

  test("collapseAllSpans seeds the set with every supplied id", () => {
    useTraceDock.getState().collapseAllSpans(["s1", "s2", "s3"]);
    expect(useTraceDock.getState().collapsedSpanIds).toEqual(
      new Set(["s1", "s2", "s3"]),
    );
    expect(
      JSON.parse(localStorage.getItem(DOCK_COLLAPSED_SPANS_STORAGE_KEY) ?? "[]").sort(),
    ).toEqual(["s1", "s2", "s3"]);
  });

  test("expandAllSpans clears the set", () => {
    useTraceDock.getState().collapseAllSpans(["s1", "s2"]);
    useTraceDock.getState().expandAllSpans();
    expect(useTraceDock.getState().collapsedSpanIds).toEqual(new Set());
    expect(JSON.parse(localStorage.getItem(DOCK_COLLAPSED_SPANS_STORAGE_KEY) ?? "[]")).toEqual([]);
  });

  test("readPersistedCollapsedSpanIds rehydrates a persisted set", () => {
    localStorage.setItem(
      DOCK_COLLAPSED_SPANS_STORAGE_KEY,
      JSON.stringify(["s4", "s7"]),
    );
    expect(readPersistedCollapsedSpanIds()).toEqual(new Set(["s4", "s7"]));
  });

  test("readPersistedCollapsedSpanIds returns an empty set for malformed JSON", () => {
    localStorage.setItem(DOCK_COLLAPSED_SPANS_STORAGE_KEY, "{not json");
    expect(readPersistedCollapsedSpanIds()).toEqual(new Set());
  });
});
