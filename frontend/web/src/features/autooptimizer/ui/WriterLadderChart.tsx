/**
 * WriterLadderChart — per-writer accept-rate ladder (Bug 7 rewrite).
 *
 * One horizontal row per experiment writer: the full provider/model name on
 * its own line (middle-truncated, full name in the title attribute), then a
 * horizontal accept-rate bar (gold fill on a bg-surface-elev track) with
 * accepted/proposals and avg ΔSharpe as mono text at the row end.
 *
 * Plain HTML/CSS — long model names no longer overlap vertical axis labels,
 * and every color comes from a theme token (no black-on-black).
 */
import type { MutatorScore } from "../api";

export interface WriterLadderChartProps {
  rows: MutatorScore[];
}

/** Middle-truncate long names: "google/gemini-2.5-flash…experimental". */
export function truncateMiddle(name: string, max = 44): string {
  if (name.length <= max) return name;
  const head = Math.ceil((max - 1) * 0.6);
  const tail = max - 1 - head;
  return `${name.slice(0, head)}…${name.slice(name.length - tail)}`;
}

function fmtDelta(delta: number): string {
  return delta >= 0 ? `+${delta.toFixed(2)}` : `−${Math.abs(delta).toFixed(2)}`;
}

export function WriterLadderChart({ rows }: WriterLadderChartProps) {
  if (rows.length === 0) {
    return (
      <div className="flex items-center justify-center py-6 text-[12px] text-text-3">
        No writer data yet
      </div>
    );
  }

  return (
    <div data-chart="writer-ladder" className="space-y-3">
      {rows.map((r) => {
        const fullName = `${r.provider}/${r.model}`;
        const rate = r.proposals > 0 ? r.accepted / r.proposals : 0;
        return (
          <div
            key={`${r.provider}/${r.model}/${r.prompt_version}`}
            data-testid="writer-ladder-row"
          >
            <div
              title={fullName}
              className="font-mono text-[11px] text-text-2"
            >
              {truncateMiddle(fullName)}
            </div>
            <div className="mt-1 flex items-center gap-3">
              <div className="h-2 min-w-0 flex-1 overflow-hidden rounded-sm bg-surface-elev">
                <div
                  data-testid="accept-rate-bar"
                  className="h-full rounded-sm bg-gold"
                  style={{ width: `${rate * 100}%` }}
                />
              </div>
              <span className="shrink-0 font-mono text-[11px] tabular-nums text-text-3">
                {r.accepted}/{r.proposals}
              </span>
              <span
                className={`w-14 shrink-0 text-right font-mono text-[11px] tabular-nums ${
                  r.avg_delta_sharpe >= 0 ? "text-gold" : "text-danger"
                }`}
              >
                Δ {fmtDelta(r.avg_delta_sharpe)}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
