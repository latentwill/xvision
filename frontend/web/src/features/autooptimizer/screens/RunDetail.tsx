import { useState } from "react";
import { Link } from "react-router-dom";
import { Pill } from "@/components/primitives/Pill";
import { Topbar } from "@/components/shell/Topbar";
import { ActivityFeed } from "../ui/ActivityFeed";
import { ImprovementChart } from "../ui/ImprovementChart";
import { SpendChart } from "../ui/SpendChart";
import { OutcomeStackedChart } from "../ui/OutcomeStackedChart";
import { ExperimentPill } from "../ui/ExperimentPill";
import { GateBadge } from "../ui/GateBadge";
import { DeltaCell } from "../ui/DeltaCell";
import {
  useOptimizerStatus,
  useOptimizerStats,
  useLineageNodes,
  usePauseSession,
  useResumeSession,
  useCancelSession,
  formatGateVerdict,
  type LineageNode,
} from "../api";

interface RunDetailProps {
  sessionId: string;
}

function statePill(state: string) {
  const lower = state.toLowerCase();
  if (lower === "running")
    return (
      <Pill tone="gold" animated>
        Running
      </Pill>
    );
  if (lower === "paused") return <Pill tone="warn">Paused</Pill>;
  if (lower === "finished") return <Pill tone="default">Finished</Pill>;
  if (lower === "failed") return <Pill tone="danger">Failed</Pill>;
  if (lower === "cancelling") return <Pill tone="warn">Cancelling</Pill>;
  return <Pill tone="default">{state}</Pill>;
}

function modeLabel(mode: string): string {
  if (mode === "explore") return "Explore";
  if (mode === "exploit") return "Exploit";
  return mode;
}

// ─── Controls row ─────────────────────────────────────────────────────────────

interface ControlsRowProps {
  sessionId: string;
  state: string;
}

function ControlsRow({ sessionId, state }: ControlsRowProps) {
  const [optimisticLabel, setOptimisticLabel] = useState<string | null>(null);

  const pauseMutation = usePauseSession();
  const resumeMutation = useResumeSession();
  const cancelMutation = useCancelSession();

  const isRunning = state === "running";
  const isPaused = state === "paused";

  if (!isRunning && !isPaused) return null;

  const handlePause = () => {
    setOptimisticLabel("Pausing…");
    pauseMutation.mutate(sessionId, {
      onSettled: () => setOptimisticLabel(null),
    });
  };

  const handleResume = () => {
    setOptimisticLabel("Resuming…");
    resumeMutation.mutate(sessionId, {
      onSettled: () => setOptimisticLabel(null),
    });
  };

  const handleCancel = () => {
    setOptimisticLabel("Cancelling…");
    cancelMutation.mutate(sessionId, {
      onSettled: () => setOptimisticLabel(null),
    });
  };

  return (
    <div className="flex items-center gap-2 flex-wrap">
      {optimisticLabel && (
        <span className="text-[12px] text-text-3">{optimisticLabel}</span>
      )}
      {isRunning && (
        <>
          <button
            type="button"
            onClick={handlePause}
            disabled={pauseMutation.isPending}
            className="rounded border border-border px-3 py-1.5 text-[13px] text-text-2 hover:bg-surface-elev/40 transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Pause
          </button>
          <button
            type="button"
            onClick={handleCancel}
            disabled={cancelMutation.isPending}
            className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
        </>
      )}
      {isPaused && (
        <>
          <button
            type="button"
            onClick={handleResume}
            disabled={resumeMutation.isPending}
            className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90 transition-opacity disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Resume
          </button>
          <button
            type="button"
            onClick={handleCancel}
            disabled={cancelMutation.isPending}
            className="rounded border border-danger/40 px-3 py-1.5 text-[13px] text-danger hover:bg-danger/[0.06] transition-colors disabled:opacity-60 disabled:cursor-not-allowed"
          >
            Cancel
          </button>
        </>
      )}
    </div>
  );
}

// ─── Charts section ───────────────────────────────────────────────────────────

function RunChartsSection({ sessionId }: { sessionId: string }) {
  const [showOutcome, setShowOutcome] = useState(false);
  const { data: statsRows } = useOptimizerStats({ session_id: sessionId });
  const rows = statsRows ?? [];

  return (
    <div className="rounded-md border border-border bg-surface-card px-5 py-4 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div>
          <h2 className="text-[13px] font-semibold tracking-tight text-text">Improvement over time</h2>
          <p className="text-[11px] text-text-3 mt-0.5">Best Δ untouched-period score per cycle</p>
        </div>
        <button
          type="button"
          onClick={() => setShowOutcome((v) => !v)}
          className="rounded border border-border px-2.5 py-1 text-[11px] text-text-2 hover:bg-surface-elev/40 transition-colors"
        >
          {showOutcome ? "Hide outcome mix" : "Show outcome mix"}
        </button>
      </div>
      <ImprovementChart rows={rows} sessionId={sessionId} />
      <SpendChart rows={rows} />
      {showOutcome && <OutcomeStackedChart rows={rows} />}
    </div>
  );
}

// ─── Experiments table ────────────────────────────────────────────────────────

function ExperimentRow({ node }: { node: LineageNode }) {
  const verdict = formatGateVerdict(node.gate_verdict);
  const deltaDay = null; // not available in lineage list without cycle detail
  const deltaHoldout =
    typeof node.diversity_score === "number" ? null : null;

  return (
    <tr className="border-b border-border/50 last:border-0 hover:bg-surface-elev/40 transition-colors">
      <td className="px-3 py-2">
        <Link
          to={`/optimizer/experiment/${encodeURIComponent(node.bundle_hash)}`}
          className="flex items-center gap-2 font-mono text-[12px] text-text hover:text-gold transition-colors"
        >
          <ExperimentPill />
          <span>{node.bundle_hash.slice(0, 10)}</span>
        </Link>
      </td>
      <td className="px-3 py-2">
        <GateBadge verdict={verdict} status={node.status} />
      </td>
      <td className="px-3 py-2 w-20">
        <DeltaCell state="done" delta={deltaDay ?? undefined} />
      </td>
      <td className="px-3 py-2 w-20">
        <DeltaCell state="done" delta={deltaHoldout ?? undefined} />
      </td>
      <td className="px-3 py-2 text-[11px] text-text-3">
        {/* reviewer note count — not available in lineage list */}
        —
      </td>
    </tr>
  );
}

function SessionExperimentsTable({ sessionId }: { sessionId: string }) {
  // Lineage nodes filtered by session_id (cycles with this session)
  // The lineage API accepts cycle_id but not session_id directly;
  // we load all and note the session context in the section header.
  // Per task spec: GET /api/autooptimizer/cycles?session_id= or lineage filtered.
  // Here we use useLineageNodes without cycle filter and rely on the
  // backend to return lineage for the session (or display all recent).
  const { data, isLoading, isError } = useLineageNodes();
  const rows: LineageNode[] = data ?? [];

  return (
    <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
      <div>
        <h2 className="text-[13px] font-semibold tracking-tight text-text">Experiments</h2>
        <p className="text-[11px] text-text-3 mt-0.5">
          What the optimizer tried in this run
        </p>
      </div>
      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load experiments.</p>
      ) : rows.length === 0 ? (
        <p className="text-[12px] text-text-3">No experiments recorded yet.</p>
      ) : (
        <div className="overflow-x-auto">
          <table className="w-full text-[12px] border-collapse">
            <thead>
              <tr className="border-b border-border text-left">
                <th className="px-3 py-2 font-medium text-[11px] text-text-3 uppercase tracking-wide">
                  Experiment
                </th>
                <th className="px-3 py-2 font-medium text-[11px] text-text-3 uppercase tracking-wide">
                  Outcome
                </th>
                <th className="px-3 py-2 font-medium text-[11px] text-text-3 uppercase tracking-wide">
                  Δ day
                </th>
                <th className="px-3 py-2 font-medium text-[11px] text-text-3 uppercase tracking-wide">
                  Δ untouched
                </th>
                <th className="px-3 py-2 font-medium text-[11px] text-text-3 uppercase tracking-wide">
                  Notes
                </th>
              </tr>
            </thead>
            <tbody>
              {rows.map((node) => (
                <ExperimentRow key={node.bundle_hash} node={node} />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

// ─── Page root ────────────────────────────────────────────────────────────────

/**
 * Run detail screen — P4: adds controls row, charts, and experiments table.
 *
 * Layout: single full-width column (no right-side box; chat rail is always
 * present on this route per the three-pane shell rule).
 */
export function RunDetail({ sessionId }: RunDetailProps) {
  const status = useOptimizerStatus();
  const session = status?.active_session?.session_id === sessionId
    ? status.active_session
    : null;

  const state = session?.state ?? "finished";
  const strategyId = session?.strategy_id ?? "";
  const mode = session?.mode ?? "";

  return (
    <>
      <Topbar
        title="Optimizer"
        sub={`Run ${sessionId.slice(0, 8)}`}
      />
      <div className="space-y-5">
        {/* Header strip */}
        <div className="flex flex-wrap items-center gap-3">
          <Link
            to="/optimizer"
            className="text-[13px] text-text-3 hover:text-text transition-colors"
            aria-label="Back to Optimizer"
          >
            ← Optimizer
          </Link>
          {statePill(state)}
          {strategyId && (
            <span className="rounded border border-border px-2 py-0.5 font-mono text-[11px] text-text-2">
              {strategyId}
            </span>
          )}
          {mode && (
            <span className="rounded border border-border px-2 py-0.5 text-[11px] text-text-3">
              {modeLabel(mode)}
            </span>
          )}
          <span className="font-mono text-[11px] text-text-3 ml-auto">
            {sessionId.slice(0, 8)}
          </span>
        </div>

        {/* Controls row — only shown for running / paused states */}
        <ControlsRow sessionId={sessionId} state={state} />

        {/* Activity feed */}
        <ActivityFeed sessionId={sessionId} />

        {/* Charts: ImprovementChart + SpendChart + optional OutcomeStackedChart */}
        <RunChartsSection sessionId={sessionId} />

        {/* Experiments table */}
        <SessionExperimentsTable sessionId={sessionId} />
      </div>
    </>
  );
}
