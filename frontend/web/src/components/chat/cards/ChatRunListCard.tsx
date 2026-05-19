import { useNavigate } from "react-router-dom";

import { InlineSparkline } from "@/components/chat/inline-chart/InlineSparkline";
import type { RunListContentBlock } from "@/api/chat_rail";
import {
  displayScenarioName,
  displayStrategyName,
} from "@/lib/run-display";
import { runInlineAction } from "./actions";

export function ChatRunListCard({ payload }: { payload: RunListContentBlock }) {
  const navigate = useNavigate();

  return (
    <article
      role="group"
      aria-label={payload.title}
      className="rounded-md border border-border-soft bg-surface-card overflow-hidden"
    >
      <header className="px-3 py-2 border-b border-border-soft">
        <h3 className="m-0 text-[13px] font-semibold text-text">
          {payload.title}
        </h3>
      </header>
      <div className="divide-y divide-border-soft">
        {payload.runs.slice(0, 5).map((run) => {
          const strategy = run.strategy_id
            ? displayStrategyName(run.strategy_id, [])
            : "Eval run";
          const scenario = run.scenario
            ? displayScenarioName(run.scenario, [])
            : null;
          return (
            <button
              key={run.run_id}
              type="button"
              aria-label={`Open run ${run.run_id}`}
              onClick={() => navigate(`/eval-runs/${encodeURIComponent(run.run_id)}`)}
              className="w-full px-3 py-2 text-left hover:bg-surface-hover flex items-center gap-2"
            >
              <span className="w-5 flex-shrink-0 font-mono text-[11px] text-text-3">
                #{run.rank}
              </span>
              <span className="flex-1 min-w-0">
                <span className="block text-[12px] text-text truncate">
                  {strategy}
                </span>
                {scenario ? (
                  <span className="block text-[11px] text-text-3 truncate">
                    {scenario}
                  </span>
                ) : null}
                {/*
                  PR #341 review: the eval-id rule from QA22 is "not
                  truncated anywhere", so the run id renders on its own
                  line with `break-all` (the chat-rail card is narrow,
                  so the ULID wraps across two lines rather than getting
                  CSS-ellipsized).
                */}
                <span className="block font-mono text-[11px] text-text-3 break-all">
                  {run.run_id}
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
                    label: strategy,
                    tone: "gold",
                    points: run.sparkline,
                  }}
                />
              ) : null}
            </button>
          );
        })}
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
