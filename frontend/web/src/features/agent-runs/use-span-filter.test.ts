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
