import { useNavigate } from "react-router-dom";

import { InlineSparkline } from "@/components/chat/inline-chart/InlineSparkline";
import type { RunListContentBlock } from "@/api/chat_rail";
import { runInlineAction } from "./actions";

export function ChatRunListCard({ payload }: { payload: RunListContentBlock }) {
  const navigate = useNavigate();

  return (
    <article className="rounded-md border border-border-soft bg-surface-card overflow-hidden">
      <header className="px-3 py-2 border-b border-border-soft">
        <h3 className="m-0 text-[13px] font-semibold text-text">
          {payload.title}
        </h3>
      </header>
      <div className="divide-y divide-border-soft">
        {payload.runs.slice(0, 5).map((run) => (
          <button
            key={run.run_id}
            type="button"
            onClick={() => navigate(`/eval-runs/${encodeURIComponent(run.run_id)}`)}
            className="w-full px-3 py-2 text-left hover:bg-surface-hover flex items-center gap-2"
          >
            <span className="w-5 flex-shrink-0 font-mono text-[11px] text-text-3">
              #{run.rank}
            </span>
            <span className="flex-1 min-w-0">
              <span className="block font-mono text-[12px] text-text truncate">
                {run.run_id}
              </span>
              <span className="block text-[11px] text-text-3 truncate">
                {[run.strategy_id, run.scenario].filter(Boolean).join(" / ")}
              </span>
            </span>
            <span className="flex-shrink-0 text-right">
              <span className="block font-mono text-[12px] text-gold">
                {formatPercent(run.return_pct)}
              </span>
              <span className="block font-mono text-[10px] text-text-3">
                S {formatNumber(run.sharpe)}
              </span>
            </span>
            {run.sparkline && run.sparkline.length > 0 ? (
              <InlineSparkline
                series={{
                  id: `${run.run_id}:spark`,
                  label: run.run_id,
                  tone: "gold",
                  points: run.sparkline,
                }}
              />
            ) : null}
          </button>
        ))}
      </div>
      {payload.actions.length > 0 ? (
        <footer className="px-3 py-2 border-t border-border-soft flex justify-end gap-1.5">
          {payload.actions.map((action) => (
            <button
              key={`${action.label}:${action.href ?? action.command ?? ""}`}
              type="button"
              onClick={() => runInlineAction(action, navigate)}
              className="px-2 py-1 rounded border border-border-soft text-[11px] text-text-2 hover:text-text"
            >
              {action.label}
            </button>
          ))}
        </footer>
      ) : null}
    </article>
  );
}

function formatPercent(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "--";
  const sign = value > 0 ? "+" : "";
  return `${sign}${value.toFixed(1)}%`;
}

function formatNumber(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "--";
  return value.toFixed(2);
}
