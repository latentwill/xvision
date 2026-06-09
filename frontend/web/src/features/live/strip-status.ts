// Strategy-strip status derivation + selection helpers.

import type { AgentRunSummary, RunStatus } from "@/api/types-agent-runs";

export type StripStatus = "ACTIVE" | "PAUSED" | "STOPPED";

// Statuses that mean the run is no longer live (terminal).
const TERMINAL_STATUSES: ReadonlySet<RunStatus> = new Set([
  "completed",
  "failed",
  "cancelled",
  "interrupted",
  "agent_failure",
]);

/**
 * Derive the strip status pill from a run summary (spec §2.4):
 *   STOPPED — run status is terminal (cancelled / completed / failed / …).
 *   PAUSED  — run is live but `paused` (per-run pause flag).
 *   ACTIVE  — live and not paused.
 *
 * `paused` is read defensively: the field may be absent on endpoints that
 * don't yet join the eval run record.
 */
export function deriveStripStatus(run: AgentRunSummary): StripStatus {
  if (TERMINAL_STATUSES.has(run.status)) return "STOPPED";
  if (run.paused === true) return "PAUSED";
  return "ACTIVE";
}

/** True when the run is still live (running or queued), regardless of pause. */
export function isLiveRun(run: AgentRunSummary): boolean {
  return !TERMINAL_STATUSES.has(run.status);
}

/**
 * Pick the run the cockpit should auto-select when no `:id` is supplied:
 * the most recently STARTED live run. Falls back to the most recently
 * started run of any status when none are live (so the viewport still
 * has something to show). Returns null for an empty list.
 */
export function pickDefaultRun(runs: AgentRunSummary[]): AgentRunSummary | null {
  if (runs.length === 0) return null;
  const byStartedDesc = (a: AgentRunSummary, b: AgentRunSummary) =>
    new Date(b.started_at).getTime() - new Date(a.started_at).getTime();
  const live = runs.filter(isLiveRun).sort(byStartedDesc);
  if (live.length > 0) return live[0]!;
  return [...runs].sort(byStartedDesc)[0]!;
}
