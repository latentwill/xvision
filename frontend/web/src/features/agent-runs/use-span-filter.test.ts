// frontend/web/src/features/agent-runs/use-span-filter.test.ts
import { describe, expect, test, beforeEach } from "vitest";
import { renderHook, act } from "@testing-library/react";
import { useSpanFilter } from "./use-span-filter";
import { MOCK_RUN_COMPLETED, MOCK_RUN_ERROR, MOCK_RUN_LIVE } from "./mock-fixtures";

const allSpans = MOCK_RUN_COMPLETED.spans;
const runId = MOCK_RUN_COMPLETED.summary.run_id;

describe("useSpanFilter", () => {
  beforeEach(() => {
    localStorage.clear();
    window.history.replaceState({}, "", "/");
  });

  test("empty filter passes all spans", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    expect(result.current.filtered).toHaveLength(allSpans.length);
  });

  test("kind toggle narrows to that kind", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    act(() => result.current.toggleKind("model"));
    expect(result.current.filtered.every((s) => s.kind === "model.call")).toBe(true);
  });

  // WS-8: engine.event rows are a parallel lifecycle band, not a span
  // category. A span-category kind chip must NOT hide them — otherwise
  // activating e.g. the MODEL chip would silently drop every risk veto /
  // regime transition / order-state signal.
  test("engine.event rows survive a span-category kind chip (never dropped)", () => {
    const spansWithEngineEvent = [
      ...allSpans,
      {
        span_id: "ee_x",
        parent_span_id: null,
        name: "risk_veto",
        kind: "engine.event" as const,
        started_at: "2026-06-14T10:00:00.000Z",
        finished_at: "2026-06-14T10:00:00.000Z",
        status: "ok" as const,
        attributes: { engine_event_kind: "risk_veto" },
      },
    ];
    const { result } = renderHook(() =>
      useSpanFilter({ runId: "run_engine_evt", spans: spansWithEngineEvent }),
    );
    act(() => result.current.toggleKind("model"));
    // The MODEL chip narrows the span rows, but the engine.event row stays.
    expect(result.current.filtered.some((s) => s.span_id === "ee_x")).toBe(true);
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

  test("status=all (default) passes all spans", () => {
    const { result } = renderHook(() => useSpanFilter({ runId, spans: allSpans }));
    expect(result.current.status).toBe("all");
    expect(result.current.filtered).toHaveLength(allSpans.length);
  });

  test("status filter narrows to spans with matching status", () => {
    const errorSpans = MOCK_RUN_ERROR.spans;
    const errorRunId = MOCK_RUN_ERROR.summary.run_id;
    const { result } = renderHook(() => useSpanFilter({ runId: errorRunId, spans: errorSpans }));
    act(() => result.current.setStatus("red"));
    expect(result.current.filtered.length).toBeGreaterThan(0);
    expect(result.current.filtered.every((s) => s.status === "error")).toBe(true);
  });

  test("switching runId loads that run's filters instead of persisting stale state", () => {
    const liveRunId = MOCK_RUN_LIVE.summary.run_id;
    localStorage.setItem(
      `xvn.agent-runs.filter.${liveRunId}`,
      JSON.stringify({ q: "", k: [], s: "blue", d: "all" }),
    );

    const { result, rerender } = renderHook(
      ({ id, spans }) => useSpanFilter({ runId: id, spans }),
      { initialProps: { id: runId, spans: allSpans } },
    );

    act(() => result.current.setQuery("model:gpt-5"));
    expect(result.current.query).toBe("model:gpt-5");

    rerender({ id: liveRunId, spans: MOCK_RUN_LIVE.spans });

    expect(result.current.query).toBe("");
    expect(result.current.status).toBe("blue");
    expect(result.current.filtered.every((s) => s.status === "in_progress")).toBe(true);
    expect(localStorage.getItem(`xvn.agent-runs.filter.${liveRunId}`)).not.toContain("model:gpt-5");
  });
});
