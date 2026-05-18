import { describe, expect, it } from "vitest";
import { planTransition } from "../src/state/machine.js";
import type { BoardTask, TaskStatus } from "../src/types.js";

function task(status: TaskStatus, overrides: Partial<BoardTask> = {}): BoardTask {
  return { status, lane: "leaf", track: "demo", ...overrides };
}

const NO_OBSERVATION = {
  worktreeExists: false,
  branchExistsLocal: false,
  branchPushed: false,
  hasCommitsAhead: false,
  prNumber: null,
  prMerged: false,
};

describe("Phase-1 state machine", () => {
  it("plans READY → CLAIMED", () => {
    const p = planTransition(task("READY"), NO_OBSERVATION);
    expect(p.kind).toBe("claim");
    expect(p.to).toBe("CLAIMED");
  });

  it("plans CLAIMED → CODING when worktree+branch are present", () => {
    const p = planTransition(task("CLAIMED"), {
      ...NO_OBSERVATION,
      worktreeExists: true,
      branchExistsLocal: true,
    });
    expect(p.kind).toBe("begin-coding");
    expect(p.to).toBe("CODING");
  });

  it("CLAIMED stays put until the worker establishes its environment", () => {
    const p = planTransition(task("CLAIMED"), NO_OBSERVATION);
    expect(p.kind).toBe("noop");
    expect(p.to).toBe("CLAIMED");
  });

  it("plans CODING → PR_OPEN when a PR number appears", () => {
    const p = planTransition(task("CODING"), {
      ...NO_OBSERVATION,
      hasCommitsAhead: true,
      prNumber: 123,
    });
    expect(p.kind).toBe("pr-open");
    expect(p.reason).toContain("PR #123");
  });

  it("plans MERGED → ARCHIVED", () => {
    const p = planTransition(task("MERGED"), NO_OBSERVATION);
    expect(p.kind).toBe("archive");
    expect(p.to).toBe("ARCHIVED");
  });

  it.each([
    "REVIEWING",
    "CHANGES_REQUESTED",
    "FIXING",
    "APPROVED",
    "MERGE_READY",
    "DEPLOYED",
    "ARCHIVED",
    "BACKLOG",
  ] as TaskStatus[])(
    "observes %s without acting",
    (status) => {
      const p = planTransition(task(status), NO_OBSERVATION);
      expect(p.kind).toBe("observe-only");
      expect(p.to).toBe(status);
    },
  );

  it("PR_OPEN with no merge yet is a no-op", () => {
    const p = planTransition(task("PR_OPEN"), NO_OBSERVATION);
    expect(p.kind).toBe("noop");
  });
});
