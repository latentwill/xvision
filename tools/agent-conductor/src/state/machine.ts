// Phase-1 state machine. ONLY these transitions are acted on:
//   READY → CLAIMED
//   CLAIMED → CODING
//   CODING → PR_OPEN
//   MERGED → ARCHIVED
//
// Every other observed transition is a no-op-with-log: surfaced in the
// digest but never executed. Phase-2/3 routing lands in separate contracts.

import type { BoardTask, TaskStatus } from "../types.js";

export type TransitionKind =
  | "claim" // READY → CLAIMED
  | "begin-coding" // CLAIMED → CODING
  | "pr-open" // CODING → PR_OPEN
  | "archive" // MERGED → ARCHIVED
  | "observe-only" // Phase-2/3 transition; log only
  | "noop"; // nothing to do

export interface PlannedTransition {
  kind: TransitionKind;
  track: string;
  from: TaskStatus;
  to: TaskStatus;
  reason: string;
}

// Observed reality used to decide CLAIMED→CODING and CODING→PR_OPEN.
// Kept abstract so tests can drive the decisions without a real filesystem
// or remote.
export interface ObservedReality {
  worktreeExists: boolean;
  branchExistsLocal: boolean;
  branchPushed: boolean;
  hasCommitsAhead: boolean;
  prNumber: number | null;
  prMerged: boolean;
}

const PHASE_1_ACTING: ReadonlySet<TaskStatus> = new Set<TaskStatus>([
  "READY",
  "CLAIMED",
  "CODING",
  "MERGED",
]);

export function planTransition(
  task: BoardTask,
  observed: ObservedReality,
): PlannedTransition {
  const base = { track: task.track, from: task.status };

  if (!PHASE_1_ACTING.has(task.status)) {
    // Phase-2/3 statuses (REVIEWING, CHANGES_REQUESTED, FIXING, APPROVED,
    // MERGE_READY, DEPLOYED, ARCHIVED, BACKLOG, PR_OPEN-already): observe only.
    return {
      ...base,
      kind: task.status === "PR_OPEN" ? "noop" : "observe-only",
      to: task.status,
      reason: `Phase-1 daemon does not act on ${task.status}`,
    };
  }

  switch (task.status) {
    case "READY":
      return {
        ...base,
        kind: "claim",
        to: "CLAIMED",
        reason: "READY task eligible for claim",
      };
    case "CLAIMED":
      if (observed.worktreeExists && observed.branchExistsLocal) {
        return {
          ...base,
          kind: "begin-coding",
          to: "CODING",
          reason: "worktree + branch present; worker has begun coding",
        };
      }
      return {
        ...base,
        kind: "noop",
        to: "CLAIMED",
        reason: "waiting for worker to establish worktree + branch",
      };
    case "CODING":
      if (observed.prNumber !== null) {
        return {
          ...base,
          kind: "pr-open",
          to: "PR_OPEN",
          reason: `PR #${observed.prNumber} opened`,
        };
      }
      return {
        ...base,
        kind: "noop",
        to: "CODING",
        reason: observed.hasCommitsAhead
          ? "commits present, no PR yet"
          : "no commits yet",
      };
    case "MERGED":
      return {
        ...base,
        kind: "archive",
        to: "ARCHIVED",
        reason: "PR merged; ready to archive worktree + queue marker",
      };
    default:
      // Exhaustiveness guard — should never hit because of PHASE_1_ACTING.
      return {
        ...base,
        kind: "noop",
        to: task.status,
        reason: "unhandled status",
      };
  }
}
