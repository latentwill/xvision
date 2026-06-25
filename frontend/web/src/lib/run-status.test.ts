import { describe, expect, it } from "vitest";
import { isInflightRunStatus, isRetryableRunStatus, isTerminalRunStatus } from "./run-status";

describe("run status helpers", () => {
  it("treats disconnected as terminal and retryable", () => {
    expect(isTerminalRunStatus("disconnected")).toBe(true);
    expect(isRetryableRunStatus("disconnected")).toBe(true);
    expect(isInflightRunStatus("disconnected")).toBe(false);
  });

  it("keeps queued and running as non-terminal inflight states", () => {
    for (const status of ["queued", "running"]) {
      expect(isInflightRunStatus(status)).toBe(true);
      expect(isTerminalRunStatus(status)).toBe(false);
      expect(isRetryableRunStatus(status)).toBe(false);
    }
  });
});
