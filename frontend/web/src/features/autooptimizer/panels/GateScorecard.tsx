import type { GateRecord } from "../api";

function fmt2(n: number | null | undefined): string {
  return n != null && Number.isFinite(n) ? n.toFixed(2) : "—";
}

function formatDelta(n: number | null | undefined): string {
  if (n == null || !Number.isFinite(n)) return "—";
  return n >= 0 ? `+${n.toFixed(2)}` : n.toFixed(2);
}

/** Horizontal bar showing parent vs child score with an optional epsilon marker. */
function ScoreBar({
  label,
  parentScore,
  childScore,
  delta,
  epsilon,
}: {
  label: string;
  parentScore: number | null;
  childScore: number | null;
  delta: number | null;
  epsilon: number | null;
}) {
  // Compute relative bar widths for a visual comparison.
  // We clamp to [0, 1] relative to the larger of the two scores.
  const max = Math.max(Math.abs(parentScore ?? 0), Math.abs(childScore ?? 0), 0.001);
  const parentPct = Math.min(100, Math.max(0, ((parentScore ?? 0) / max) * 100));
  const childPct = Math.min(100, Math.max(0, ((childScore ?? 0) / max) * 100));

  // Epsilon threshold marker position: parent + epsilon, relative to max
  const epsilonPct =
    epsilon != null && parentScore != null
      ? Math.min(100, Math.max(0, (((parentScore + epsilon) / max) * 100)))
      : null;

  const deltaPositive = delta != null && delta > 0;
  const deltaClass = deltaPositive ? "text-gold" : delta != null && delta < 0 ? "text-danger" : "text-text-3";

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex items-center justify-between">
        <span className="text-[11px] font-medium text-text-2">{label}</span>
        <span className={`font-mono text-[12px] font-semibold ${deltaClass}`}>
          {formatDelta(delta)}
        </span>
      </div>
      {/* Bar row */}
      <div className="flex flex-col gap-1">
        {/* Baseline (parent) bar */}
        <div className="relative flex h-4 w-full items-center rounded-sm bg-surface-elev overflow-visible">
          <div
            className="h-full rounded-sm bg-text-3/40"
            style={{ width: `${parentPct}%` }}
            title={`Baseline: ${fmt2(parentScore)}`}
          />
          {/* epsilon marker */}
          {epsilonPct != null && (
            <div
              className="absolute top-0 h-full w-px bg-warn"
              style={{ left: `${epsilonPct}%` }}
              title={`Min-improvement threshold (+${fmt2(epsilon)})`}
            />
          )}
          <span className="absolute right-1 text-[9px] font-mono text-text-3">
            {fmt2(parentScore)}
          </span>
        </div>
        {/* Candidate (child) bar */}
        <div className="relative flex h-4 w-full items-center rounded-sm bg-surface-elev overflow-visible">
          <div
            className={`h-full rounded-sm ${deltaPositive ? "bg-gold/50" : "bg-text-2/30"}`}
            style={{ width: `${childPct}%` }}
            title={`Candidate: ${fmt2(childScore)}`}
          />
          {/* epsilon marker — same position, repeated so the visual is aligned */}
          {epsilonPct != null && (
            <div
              className="absolute top-0 h-full w-px bg-warn"
              style={{ left: `${epsilonPct}%` }}
            />
          )}
          <span className="absolute right-1 text-[9px] font-mono text-text-3">
            {fmt2(childScore)}
          </span>
        </div>
        {/* Legend: baseline / candidate labels */}
        <div className="flex justify-between">
          <span className="text-[9px] uppercase tracking-wider text-text-3">Baseline</span>
          <span className="text-[9px] uppercase tracking-wider text-text-3">Candidate</span>
        </div>
      </div>
    </div>
  );
}

export function GateScorecard({ gate_record }: { gate_record: GateRecord | null }) {
  if (!gate_record) {
    return (
      <div className="rounded-md border border-border bg-surface-card p-5">
        <p className="text-[12px] text-text-3">Gate data not recorded</p>
      </div>
    );
  }

  const {
    parent_day_score,
    child_day_score,
    parent_holdout_score,
    child_holdout_score,
    gate_epsilon,
    delta_day,
    delta_holdout,
    drawdown_ratio,
  } = gate_record;

  return (
    <div className="rounded-md border border-border bg-surface-card p-5 space-y-4">
      <ScoreBar
        label="Today's window"
        parentScore={parent_day_score}
        childScore={child_day_score}
        delta={delta_day}
        epsilon={gate_epsilon}
      />
      <ScoreBar
        label="Untouched period"
        parentScore={parent_holdout_score}
        childScore={child_holdout_score}
        delta={delta_holdout}
        epsilon={gate_epsilon}
      />
      {/* Min-improvement legend & drawdown */}
      <div className="flex flex-wrap items-center gap-4 border-t border-border-soft pt-3">
        <div className="flex items-center gap-1.5">
          <div className="h-3 w-px bg-warn" />
          <span className="text-[10px] text-text-3">Min-improvement threshold</span>
        </div>
        {drawdown_ratio != null && (
          <div className="flex items-center gap-1.5">
            <span className="text-[10px] text-text-3">Drawdown ratio</span>
            <span className="font-mono text-[11px] text-text-2">{drawdown_ratio.toFixed(2)}</span>
          </div>
        )}
      </div>
    </div>
  );
}
