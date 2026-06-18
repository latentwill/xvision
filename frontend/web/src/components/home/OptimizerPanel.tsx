// frontend/web/src/components/home/OptimizerPanel.tsx
//
// Home "is the machine doing good work?" panel for the autooptimizer
// subsystem. Operator-facing terms only (terminology lock, CLAUDE.md):
// "Optimizer", "Experiments", "Experiment writers", "Rejected" (overfit),
// "Suspect". Code identifiers stay `autooptimizer`.
//
// Shows: accepted-vs-rejected experiment meter, the writer-model
// mini-ladder (top 3 by avg ΔSharpe), the kept/suspect/dropped trend over
// recent cycles, and cumulative spend. The idle state names the last cycle
// and its result — NEVER "Waiting for connection…".

import { Link } from "react-router-dom";

import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import {
  useLadder,
  useOptimizerStats,
  useOptimizerStatus,
} from "@/features/autooptimizer/api";
import { useAutoresearchRuns } from "@/api/nanochat";
import {
  cumulativeSpendUsd,
  cycleTrend,
  ladderTotals,
  lastCycle,
  shortModelName,
  topWriters,
} from "@/features/home/optimizer-summary";
import { formatRelativeTime } from "@/features/home/pulse";
import { OptimizerDigestStrip } from "./OptimizerDigestStrip";

// ─── sub-components ──────────────────────────────────────────────────────────

function AcceptanceMeter({
  accepted,
  rejected,
}: {
  accepted: number;
  rejected: number;
}) {
  const total = accepted + rejected;
  const acceptedPct = total > 0 ? (accepted / total) * 100 : 0;
  return (
    <div data-testid="optimizer-acceptance" className="space-y-2">
      <div className="flex items-baseline gap-2">
        <span className="font-mono tabular-nums text-[24px] leading-none font-semibold text-text">
          {accepted}
          <span className="text-[14px] text-text-3">/{total}</span>
        </span>
        <span className="text-[11px] text-text-3">accepted</span>
      </div>
      <div
        className="flex h-1.5 w-full overflow-hidden rounded-full bg-surface-elev"
        role="img"
        aria-label={`${accepted} experiments accepted, ${rejected} rejected as overfit`}
      >
        {total > 0 ? (
          <>
            <span
              className="h-full bg-gold"
              style={{ width: `${acceptedPct}%` }}
            />
            <span
              className="h-full bg-danger/40"
              style={{ width: `${100 - acceptedPct}%` }}
            />
          </>
        ) : null}
      </div>
      <p className="text-[11px] text-text-4">
        {rejected} rejected (overfit)
      </p>
    </div>
  );
}

function WriterLadder({
  writers,
}: {
  writers: ReturnType<typeof topWriters>;
}) {
  return (
    <ol data-testid="optimizer-ladder" className="space-y-1.5">
      {writers.map((w) => (
        <li
          key={`${w.provider}/${w.model}/${w.prompt_version}`}
          className="flex items-baseline gap-2 min-w-0"
        >
          <span
            className="flex-1 truncate text-[12px] text-text-2"
            title={`${w.provider}/${w.model}`}
          >
            {shortModelName(w.model)}
          </span>
          <span className="font-mono tabular-nums text-[12px] text-text-3">
            {w.accepted}/{w.proposals}
          </span>
          <span
            className={`w-14 text-right font-mono tabular-nums text-[12px] ${
              w.avg_delta_sharpe > 0 ? "text-gold" : "text-text-4"
            }`}
          >
            {w.avg_delta_sharpe > 0 ? "+" : ""}
            {w.avg_delta_sharpe.toFixed(2)}
          </span>
        </li>
      ))}
    </ol>
  );
}

function CycleTrendBars({
  trend,
}: {
  trend: ReturnType<typeof cycleTrend>;
}) {
  const maxTotal = Math.max(
    1,
    ...trend.map((t) => t.kept + t.suspect + t.dropped),
  );
  return (
    <div
      data-testid="optimizer-cycle-trend"
      className="flex h-9 items-end gap-1"
      role="img"
      aria-label="Kept, suspect, and dropped experiments per recent cycle"
    >
      {trend.map((t) => {
        const total = t.kept + t.suspect + t.dropped;
        const scale = total > 0 ? (total / maxTotal) * 36 : 2;
        const seg = (n: number) =>
          total > 0 ? Math.max(n > 0 ? 2 : 0, (n / total) * scale) : 0;
        return (
          <div
            key={t.cycleId}
            className="flex w-2.5 flex-col-reverse overflow-hidden rounded-[2px]"
            style={{ height: `${Math.max(scale, 2)}px` }}
            title={`${t.kept} kept · ${t.suspect} suspect · ${t.dropped} dropped`}
          >
            {total === 0 ? (
              <span className="h-[2px] bg-surface-panel" />
            ) : (
              <>
                <span className="w-full bg-text-4/50" style={{ height: seg(t.dropped) }} />
                <span className="w-full bg-warn" style={{ height: seg(t.suspect) }} />
                <span className="w-full bg-gold" style={{ height: seg(t.kept) }} />
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}

export function OptimizerPanel() {
  const ladder = useLadder();
  const stats = useOptimizerStats();
  const status = useOptimizerStatus();
  const arRuns = useAutoresearchRuns();

  const scores = ladder.data ?? [];
  const rows = stats.data ?? [];
  const totals = ladderTotals(scores);
  const writers = topWriters(scores, 3);
  const trend = cycleTrend(rows, 12);
  const spend = cumulativeSpendUsd(rows);
  const last = lastCycle(rows);
  const session = status?.active_session ?? null;
  const arRunsList = arRuns.data ?? [];
  const arActive = arRunsList.find((r) => r.status === "running") ?? null;
  const isLoading = ladder.isPending && stats.isPending;
  const hasData = scores.length > 0 || rows.length > 0;

  return (
    <section data-testid="optimizer-panel" aria-label="Optimizer">
      <Card className="p-0 overflow-hidden xvn-card-hover">
        {/* Header */}
        <div className="flex flex-wrap items-center justify-between gap-2 px-5 pt-4 pb-3 border-b border-border-soft">
          <div className="flex items-center gap-3 min-w-0">
            <span className="text-[15px] font-medium text-text">Optimizer</span>
            {session ? (
              <Pill
                tone={session.state === "paused" ? "warn" : "gold"}
                animated={session.state === "running"}
                data-testid="optimizer-status-pill"
              >
                {session.state === "paused" ? "paused" : "running"} ·{" "}
                {session.cycles_completed} experiments
              </Pill>
            ) : last ? (
              <span data-testid="optimizer-idle" className="text-[12px] text-text-3">
                Idle · last cycle {formatRelativeTime(last.ts)} — {last.kept}{" "}
                kept · {last.suspect} suspect · {last.dropped} dropped
              </span>
            ) : null}
          </div>
          <Link
            to="/optimizer"
            className="text-[12px] text-text-3 hover:text-text"
          >
            Open Optimizer →
          </Link>
        </div>

        {/* Autoresearcher status strip */}
        {arActive && (
          <div className="flex items-center gap-3 px-5 py-2 border-b border-border-soft bg-surface-panel/50">
            <span className="text-[12px] text-text-3">Autoresearcher</span>
            <Pill tone="gold" animated data-testid="optimizer-ar-status-pill">
              running · {arActive.run_tag}
            </Pill>
            {arActive.source_strategy_id && (
              <span className="text-[12px] text-text-4 font-mono truncate max-w-[16rem]">
                {arActive.source_strategy_id}
              </span>
            )}
          </div>
        )}

        {/* Body */}
        {isLoading ? (
          <div className="grid grid-cols-1 sm:grid-cols-3 divide-y sm:divide-y-0 sm:divide-x divide-border-soft animate-pulse">
            {[0, 1, 2].map((i) => (
              <div key={i} className="px-5 py-4">
                <div className="h-16 rounded bg-surface-elev" />
              </div>
            ))}
          </div>
        ) : !hasData ? (
          <div data-testid="optimizer-empty" className="px-5 py-8 text-center">
            <p className="text-[13px] text-text-3">
              No optimizer cycles recorded yet.
            </p>
            <p className="mt-1 text-[12px] text-text-4">
              Start a cycle from the Optimizer page to begin generating and
              gating experiments.
            </p>
          </div>
        ) : (
          <div className="grid grid-cols-1 sm:grid-cols-3 divide-y sm:divide-y-0 sm:divide-x divide-border-soft">
            <div className="px-5 py-4">
              <p className="caps mb-2">Experiments</p>
              <AcceptanceMeter
                accepted={totals.accepted}
                rejected={totals.rejectedOverfit}
              />
            </div>
            <div className="px-5 py-4 min-w-0">
              <p className="caps mb-2">Experiment writers · avg ΔSharpe</p>
              {writers.length > 0 ? (
                <WriterLadder writers={writers} />
              ) : (
                <p className="text-[12px] text-text-4">No proposals yet.</p>
              )}
            </div>
            <div className="px-5 py-4">
              <p className="caps mb-2">Recent cycles · kept / suspect / dropped</p>
              {trend.length > 0 ? (
                <CycleTrendBars trend={trend} />
              ) : (
                <p className="text-[12px] text-text-4">No cycle stats yet.</p>
              )}
              <p className="mt-2 text-[11px] text-text-4">
                Σ spend{" "}
                <span
                  data-testid="optimizer-spend"
                  className="font-mono tabular-nums text-text-3"
                >
                  {spend !== null ? `$${spend.toFixed(2)}` : "$ —"}
                </span>
              </p>
            </div>
          </div>
        )}

        {/* Last-session digest (renders nothing when no sessions exist). */}
        <OptimizerDigestStrip />
      </Card>
    </section>
  );
}
