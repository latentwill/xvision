// Strategy-strip status derivation + selection helpers.
//
// "Live" means LIVE MONEY (xvision-9pi): an agent run only counts as live
// when the backend's `is_live_money` discriminator is set — i.e. its parent
// eval run has `mode = live` AND that eval run is non-terminal. Agent runs
// stuck in `running` whose parent eval run already finished are STALE
// orphans (dead recorder rows), never live.

import type { AgentRunSummary, RunStatus } from "@/api/types-agent-runs";

export type StripStatus = "ACTIVE" | "PAUSED" | "STOPPED" | "STALE";

/** Coarse liveness class for honest home/cockpit counts. */
export type RunLiveness = "live" | "paper" | "stale" | "done";

// Statuses that mean the agent run itself is no longer running (terminal).
const TERMINAL_STATUSES: ReadonlySet<RunStatus> = new Set([
  "completed",
  "failed",
  "cancelled",
  "interrupted",
  "agent_failure",
]);

// Terminal statuses of the PARENT eval run (`eval_runs.status`).
const TERMINAL_EVAL_STATUSES: ReadonlySet<string> = new Set([
  "completed",
  "failed",
  "cancelled",
]);

/**
 * True when the agent run is a stale orphan: its own status is still
 * non-terminal (`queued`/`running`) but the parent eval run is already
 * terminal. These rows are recorder leftovers from dead processes — they
 * must render as STALE/interrupted, never as live.
 *
 * Defensive: `eval_run_status` is absent on endpoints that don't join the
 * eval run record; absent ⇒ not provably stale.
 */
export function isStaleRun(run: AgentRunSummary): boolean {
  if (TERMINAL_STATUSES.has(run.status)) return false;
  return (
    typeof run.eval_run_status === "string" &&
    TERMINAL_EVAL_STATUSES.has(run.eval_run_status)
  );
}

/**
 * THE liveness selector: true only when real money is moving right now.
 *
 * Requires ALL of:
 *  - the backend live-money signal (`is_live_money === true`, i.e. parent
 *    eval run `mode = live` and non-terminal),
 *  - the agent run itself is non-terminal,
 *  - the run is not a stale orphan (defensive re-check of
 *    `eval_run_status` against the terminal set).
 *
 * Anything else — backtest/paper children, parentless runs, orphans of
 * finished live runs — is NOT live, regardless of `status === "running"`.
 */
export function isLiveRun(run: AgentRunSummary): boolean {
  return (
    run.is_live_money === true &&
    !TERMINAL_STATUSES.has(run.status) &&
    !isStaleRun(run)
  );
}

/**
 * Classify a run for honest counts:
 *   done  — agent run reached a terminal status.
 *   stale — non-terminal agent run whose parent eval run is terminal.
 *   live  — live money (see `isLiveRun`).
 *   paper — non-terminal but not live money (backtest/sim or no parent).
 */
export function classifyRunLiveness(run: AgentRunSummary): RunLiveness {
  if (TERMINAL_STATUSES.has(run.status)) return "done";
  if (isStaleRun(run)) return "stale";
  if (isLiveRun(run)) return "live";
  return "paper";
}

/**
 * Derive the strip status pill from a run summary (spec §2.4):
 *   STOPPED — run status is terminal (cancelled / completed / failed / …).
 *   STALE   — run claims to be running but its parent eval run is terminal
 *             (orphaned recorder row; treated as interrupted, never live).
 *   PAUSED  — run is live but `paused` (per-run pause flag).
 *   ACTIVE  — live and not paused.
 *
 * `paused` is read defensively: the field may be absent on endpoints that
 * don't yet join the eval run record.
 */
export function deriveStripStatus(run: AgentRunSummary): StripStatus {
  if (TERMINAL_STATUSES.has(run.status)) return "STOPPED";
  if (isStaleRun(run)) return "STALE";
  if (run.paused === true) return "PAUSED";
  return "ACTIVE";
}

/**
 * Pick the run the cockpit should auto-select when no `:id` is supplied:
 * the most recently STARTED live-money run. Falls back to the most
 * recently started paper (non-terminal, non-stale) run, then to the most
 * recently started run of any status (so the viewport still has something
 * to show). Stale orphans are never preferred over genuinely-running work.
 * Returns null for an empty list.
 */
export function pickDefaultRun(runs: AgentRunSummary[]): AgentRunSummary | null {
  if (runs.length === 0) return null;
  const byStartedDesc = (a: AgentRunSummary, b: AgentRunSummary) =>
    new Date(b.started_at).getTime() - new Date(a.started_at).getTime();
  const live = runs.filter(isLiveRun).sort(byStartedDesc);
  if (live.length > 0) return live[0]!;
  const paper = runs
    .filter((r) => classifyRunLiveness(r) === "paper")
    .sort(byStartedDesc);
  if (paper.length > 0) return paper[0]!;
  return [...runs].sort(byStartedDesc)[0]!;
}
