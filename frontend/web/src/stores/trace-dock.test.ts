// frontend/web/src/stores/trace-dock.test.ts
import { beforeEach, describe, expect, test, vi } from "vitest";
import {
  DOCK_HEIGHT_STORAGE_KEY,
  DOCK_MIN_PX,
  DEFAULT_DOCK_PX,
  clampDockPx,
  dockMaxPx,
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
  useTraceDock.getState().setActiveRun(null, "post-hoc");
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

  test("setActiveRun resets selectedSpan", () => {
    useTraceDock.setState({ selectedSpanId: "s5" });
    useTraceDock.getState().setActiveRun("run_other", "post-hoc");
    expect(useTraceDock.getState().selectedSpanId).toBeNull();
    expect(useTraceDock.getState().activeRunId).toBe("run_other");
    expect(useTraceDock.getState().mode).toBe("post-hoc");
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
    useTraceDock.getState().setActiveRun("run_new", "live");
    const s = useTraceDock.getState().streamingState;
    expect([...s.activeSpanIds]).toEqual([]);
    expect(s.droppedEvents).toBe(0);
    expect(s.activeSpanMeta).toEqual({});
  });
});
