import { Link } from "react-router-dom";
import type { CycleNodeDetail } from "../api";

// ─── Cell state ───────────────────────────────────────────────────────────────

export type HeatCellState = "done" | "testing" | "queued" | "failed";

const CELL_STYLE: Record<
  HeatCellState,
  { bg: string; border: string; label: string | null; labelColor?: string }
> = {
  done: { bg: "var(--gold-bg-strong)", border: "var(--gold-soft)", label: null },
  testing: {
    bg: "rgba(95,168,255,0.18)",
    border: "rgba(95,168,255,0.50)",
    label: "…",
    labelColor: "var(--info)",
  },
  queued: { bg: "var(--surface-elev)", border: "var(--border)", label: null },
  failed: {
    bg: "rgba(255,77,77,0.16)",
    border: "rgba(255,77,77,0.50)",
    label: "×",
    labelColor: "var(--danger)",
  },
};

/** One backtest cell. `testing` cells animate an info-blue shimmer. */
export function HeatCell({
  state,
  title,
}: {
  state: HeatCellState;
  title?: string;
}) {
  const s = CELL_STYLE[state];
  return (
    <div
      title={title}
      data-cell-state={state}
      className="relative flex h-[18px] items-center justify-center overflow-hidden rounded-[2px]"
      style={{ background: s.bg, border: `1px solid ${s.border}` }}
    >
      {state === "testing" && (
        <div
          aria-hidden
          className="xvn-heat-flow absolute inset-0"
          style={{
            background:
              "linear-gradient(90deg, transparent, rgba(95,168,255,0.35), transparent)",
          }}
        />
      )}
      {s.label && (
        <span
          className="relative font-mono text-[9px] font-bold"
          style={{ color: s.labelColor, letterSpacing: "0.1em" }}
        >
          {s.label}
        </span>
      )}
    </div>
  );
}

function LegendDot({ color, label }: { color: string; label: string }) {
  return (
    <span className="flex items-center gap-1.5">
      <span
        className="inline-block h-2 w-2 rounded-[2px]"
        style={{ background: color }}
      />
      <span className="font-mono text-[9.5px] text-text-3">{label}</span>
    </span>
  );
}

// ─── Heatmap ────────────────────────────────────────────────────────────────

/**
 * Live experiments × regimes heatmap. Each row is one experiment (lineage node),
 * each column one regime; each cell is one backtest.
 *
 * Cell state is derived from real cycle-detail data:
 *  - `done`    — a `regime_result` exists for (experiment, regime)
 *  - `testing` — result missing AND the optimizer is running (shimmer)
 *  - `queued`  — result missing AND the optimizer is idle
 *
 * The SSE stream carries no per-cell ticks, so cells flip to `done` as the
 * polled cycle detail (`useCycleRun`) refetches new regime results.
 */
export function LiveEvalHeatmap({
  nodes,
  isRunning,
}: {
  nodes: CycleNodeDetail[];
  isRunning: boolean;
}) {
  // Stable first-seen union of regime labels across all nodes.
  const regimeLabels: string[] = [];
  const seen = new Set<string>();
  for (const node of nodes) {
    for (const r of node.regime_results) {
      if (!seen.has(r.regime_label)) {
        seen.add(r.regime_label);
        regimeLabels.push(r.regime_label);
      }
    }
  }

  const hasGrid = nodes.length > 0 && regimeLabels.length > 0;

  let evalsDone = 0;
  const evalsTotal = nodes.length * regimeLabels.length;
  for (const node of nodes) {
    evalsDone += node.regime_results.length;
  }

  const gridTemplate = `minmax(96px, 132px) repeat(${regimeLabels.length}, minmax(0, 1fr))`;

  return (
    <section className="flex min-w-0 flex-col rounded-md border border-border bg-surface-card p-5">
      <div className="mb-2.5 flex items-baseline justify-between gap-3">
        <div>
          <h2 className="m-0 text-[13.5px] font-semibold tracking-tight text-text">
            Live progress · experiments × regimes
          </h2>
          <p className="mt-0.5 font-mono text-[10.5px] text-text-3">
            {hasGrid
              ? "each cell is one backtest · testing cells animate"
              : "lights up when the optimizer runs across a configured regime set"}
          </p>
        </div>
        {hasGrid && (
          <div className="flex shrink-0 items-center gap-2.5">
            <LegendDot color="var(--gold)" label="done" />
            <LegendDot color="var(--info)" label="testing" />
            <LegendDot color="var(--text-4)" label="queued" />
          </div>
        )}
      </div>

      {hasGrid && (
        <>
          {/* Regime column headers */}
          <div
            className="mb-1.5 grid gap-1.5"
            style={{ gridTemplateColumns: gridTemplate }}
          >
            <div />
            {regimeLabels.map((label) => (
              <div
                key={label}
                className="truncate text-center font-mono text-[9.5px] uppercase tracking-[0.04em] text-text-3"
                title={label}
              >
                {label}
              </div>
            ))}
          </div>

          {/* Rows */}
          <div className="flex flex-col gap-[3px]">
            {nodes.map((node) => {
              const byLabel = new Map(
                node.regime_results.map((r) => [r.regime_label, r]),
              );
              return (
                <div
                  key={node.bundle_hash}
                  className="grid items-center gap-1.5"
                  style={{ gridTemplateColumns: gridTemplate }}
                >
                  <Link
                    to={`/optimizer/experiment/${encodeURIComponent(node.bundle_hash)}`}
                    className="truncate font-mono text-[10.5px] font-semibold text-text-2 hover:text-gold"
                    title={node.bundle_hash}
                  >
                    {node.bundle_hash.slice(0, 10)}
                  </Link>
                  {regimeLabels.map((label) => {
                    const r = byLabel.get(label);
                    const state: HeatCellState = r
                      ? "done"
                      : isRunning
                        ? "testing"
                        : "queued";
                    const title = r
                      ? `${label} · Δ-Sharpe ${r.delta_sharpe >= 0 ? "+" : ""}${r.delta_sharpe.toFixed(2)}`
                      : `${label} · ${state}`;
                    return <HeatCell key={label} state={state} title={title} />;
                  })}
                </div>
              );
            })}
          </div>

          {/* Progress footer */}
          <div className="mt-2.5 flex items-center gap-3 border-t border-border-soft pt-2">
            <div className="h-1 flex-1 overflow-hidden rounded-full bg-surface-elev">
              <div
                className="h-full rounded-full bg-gold transition-[width] duration-500"
                style={{
                  width: evalsTotal > 0 ? `${(evalsDone / evalsTotal) * 100}%` : "0%",
                }}
              />
            </div>
            <span className="whitespace-nowrap font-mono text-[10.5px] text-text-3">
              {evalsDone} / {evalsTotal} evals{isRunning ? " · live" : ""}
            </span>
          </div>
        </>
      )}
    </section>
  );
}
