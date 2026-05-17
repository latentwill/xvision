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
