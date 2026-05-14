import { useNavigate } from "react-router-dom";

import { toneColors } from "@/components/chat/inline-chart/palette";
import type { StrategyCardContentBlock } from "@/api/chat_rail";
import { runInlineAction } from "./actions";

export function ChatStrategyCard({
  payload,
}: {
  payload: StrategyCardContentBlock;
}) {
  const navigate = useNavigate();

  return (
    <article
      role="group"
      aria-label={`Strategy ${payload.title}`}
      className="rounded-md border border-border-soft bg-surface-card overflow-hidden"
    >
      <header className="px-3 py-2 border-b border-border-soft">
        <div className="flex items-start justify-between gap-3">
          <div className="min-w-0">
            <h3 className="m-0 text-[13px] font-semibold text-text truncate">
              {payload.title}
            </h3>
            <div className="mt-0.5 font-mono text-[11px] text-text-3 truncate">
              {payload.subtitle ?? payload.strategy_id}
            </div>
          </div>
          {payload.status ? (
            <span className="rounded-full border border-border-soft px-2 py-0.5 text-[10px] text-text-2">
              {payload.status}
            </span>
          ) : null}
        </div>
      </header>

      {payload.metrics.length > 0 ? (
        <div className="grid grid-cols-2 gap-px p-3">
          {payload.metrics.slice(0, 4).map((metric) => {
            const colors = toneColors(metric.tone);
            return (
              <div
                key={metric.label}
                className="rounded-sm bg-surface-elev px-2 py-1.5"
              >
                <div className="text-[10px] text-text-3 truncate">
                  {metric.label}
                </div>
                <div className={`font-mono text-[12px] truncate ${colors.text}`}>
                  {metric.value}
                  {metric.unit ?? ""}
                </div>
              </div>
            );
          })}
        </div>
      ) : null}

      {payload.tags.length > 0 ? (
        <div className="px-3 pb-3 flex flex-wrap gap-1">
          {payload.tags.slice(0, 6).map((tag) => (
            <span
              key={tag}
              className="rounded-full border border-border-soft px-2 py-0.5 text-[10px] text-text-3"
            >
              {tag}
            </span>
          ))}
        </div>
      ) : null}

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
