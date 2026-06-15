// Strategy-strip status derivation + selection helpers.
//
// "Live" means LIVE MONEY (xvision-9pi): an agent run only counts as live
// when the backend's `is_live_money` discriminator is set — i.e. its parent
// eval run has `mode = live` AND that eval run is non-terminal. Agent runs
// stuck in `running` whose parent eval run already finished are STALE
// orphans (dead recorder rows), never live.

import type { AgentRunSummary, RunStatus } from "@/api/types-agent-runs";

export type StripStatus = "ACTIVE" | "PAUSED" | "STOPPED" | "STALE";

/** Coarse liveness class for honest home/live page counts. */
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
 * True when a run belongs to a LIVE deployment lineage (its parent eval run
 * was started in `mode = live`), regardless of its current state. This is the
 * gate for what may appear ANYWHERE on the Live Trading page: active, paused,
 * stopped, and stale-orphan live runs all qualify; backtest/paper eval runs
 * and parentless runs do NOT. It exists so the strip never lists the dozens of
 * finished backtest evals as "STOPPED live strategies" — those have a home on
 * the eval-runs page, not here.
 *
 * `eval_mode` is populated by the agent-runs list endpoint from the parent
 * eval run's mode ("live" | "backtest"); absent ⇒ not a live deployment.
 */
export function isLiveLineage(run: AgentRunSummary): boolean {
  return run.eval_mode === "live";
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

/** Honest aggregate counts over a non-terminal agent-run population. `live`
 * splits into `liveActive`/`livePaused` via the per-run pause flag. The home
 * Pulse band and LiveSummaryStrip both consume this so the numbers can never
 * diverge. */
export interface LivenessCounts {
  liveActive: number;
  livePaused: number;
  paper: number;
  stale: number;
}

export function livenessCounts(runs: AgentRunSummary[]): LivenessCounts {
  const counts: LivenessCounts = {
    liveActive: 0,
    livePaused: 0,
    paper: 0,
    stale: 0,
  };
  for (const run of runs) {
    switch (classifyRunLiveness(run)) {
      case "live":
        if (deriveStripStatus(run) === "PAUSED") counts.livePaused += 1;
        else counts.liveActive += 1;
        break;
      case "paper":
        counts.paper += 1;
        break;
      case "stale":
        counts.stale += 1;
        break;
      case "done":
        break;
    }
  }
  return counts;
}

// ─── Strategy-strip status filter (chips) ────────────────────────────────────

/** Filter chip values for the strategy strip. `ALL` is a meta-filter. */
export type StripFilter = "ALL" | "LIVE" | "PAUSED" | "STOPPED";

/** Chip render order. */
export const STRIP_FILTERS: readonly StripFilter[] = [
  "ALL",
  "LIVE",
  "PAUSED",
  "STOPPED",
];

/**
 * Bucket a run for the strip's status filter chips.
 *
 *   LIVE    — live money and not paused (`isLiveRun` && ACTIVE). The ONLY
 *             bucket allowed to present a run as live.
 *   PAUSED  — live money but per-run paused.
 *   STOPPED — everything else: terminal runs, stale orphans (parent eval
 *             run already terminal), and non-live (backtest/paper or
 *             parentless) children. None of these are moving real money,
 *             so on the live page they all read as "not live".
 *
 * STALE deliberately folds into STOPPED (no separate chip): an orphaned
 * recorder row is operationally a dead run, and the pill's own status
 * badge still says STALE so the distinction isn't lost.
 */
export function stripFilterBucket(
  run: AgentRunSummary,
): Exclude<StripFilter, "ALL"> {
  if (isLiveRun(run)) {
    return run.paused === true ? "PAUSED" : "LIVE";
  }
  return "STOPPED";
}

/** Runs matching a filter chip. `ALL` returns the input unchanged. */
export function filterRunsForStrip(
  runs: AgentRunSummary[],
  filter: StripFilter,
): AgentRunSummary[] {
  if (filter === "ALL") return runs;
  return runs.filter((run) => stripFilterBucket(run) === filter);
}

export type StripFilterCounts = Record<StripFilter, number>;

/** Per-chip counts. `ALL` is the total; the other three partition it. */
export function stripFilterCounts(
  runs: AgentRunSummary[],
): StripFilterCounts {
  const counts: StripFilterCounts = {
    ALL: runs.length,
    LIVE: 0,
    PAUSED: 0,
    STOPPED: 0,
  };
  for (const run of runs) {
    counts[stripFilterBucket(run)] += 1;
  }
  return counts;
}

/**
 * Pick the run the live page should auto-select when no `:id` is supplied:
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

/**
 * Strict live-only auto-selection for the bare `/live` route's VIEWPORT.
 *
 * Returns the most recently started genuinely-live-money run, or null when
 * none exists. Unlike `pickDefaultRun`, this NEVER falls back to a
 * backtest/paper/stale/terminal run — so when nothing is actually trading
 * live, the viewport shows the honest "No active live deployments" empty
 * state instead of auto-loading some old eval run's equity curve and chart.
 *
 * Deep links (`/live/:id`) and explicit row clicks still view any run; this
 * only governs what loads automatically with no selection.
 */
export function pickDefaultLiveRun(
  runs: AgentRunSummary[],
): AgentRunSummary | null {
  if (runs.length === 0) return null;
  const byStartedDesc = (a: AgentRunSummary, b: AgentRunSummary) =>
    new Date(b.started_at).getTime() - new Date(a.started_at).getTime();
  const live = runs.filter(isLiveRun).sort(byStartedDesc);
  return live[0] ?? null;
}
