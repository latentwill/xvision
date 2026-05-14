import { useNavigate } from "react-router-dom";

import { runInlineAction } from "@/components/chat/cards/actions";
import { InlineChartSvg } from "./InlineChartSvg";
import { SERIES_TONES, toneColors } from "./palette";
import type { InlineChartContentBlock, InlineMetric } from "@/api/chat_rail";

export function InlineChartCard({ payload }: { payload: InlineChartContentBlock }) {
  const navigate = useNavigate();

  return (
    <article
      className="rounded-md border border-border-soft bg-surface-card overflow-hidden"
      aria-label={payload.a11y_summary}
    >
      <div className="px-3 pt-3 pb-2 flex items-start justify-between gap-3">
        <div className="min-w-0">
          <h3 className="m-0 text-[13px] font-semibold text-text truncate">
            {payload.title}
          </h3>
          {payload.subtitle || payload.source?.label ? (
            <div className="mt-0.5 text-[11px] text-text-3 truncate">
              {payload.subtitle ?? payload.source?.label}
            </div>
          ) : null}
        </div>
        {payload.primary_metric ? <PrimaryMetric metric={payload.primary_metric} /> : null}
      </div>

      <div className="px-2">
        <InlineChartSvg payload={payload} />
      </div>

      {payload.kind === "compare" && payload.series.length > 1 ? (
        <Legend payload={payload} />
      ) : null}

      {payload.metrics.length > 0 ? <MetricStrip metrics={payload.metrics} /> : null}

      {payload.actions.length > 0 || payload.downsampled ? (
        <footer className="px-3 py-2 border-t border-border-soft flex items-center gap-2">
          {payload.downsampled ? (
            <span className="text-[10px] text-text-3 font-mono">sampled</span>
          ) : null}
          <div className="ml-auto flex items-center gap-1.5">
            {payload.actions.map((action) => (
              <button
                key={`${action.label}:${action.href ?? action.command ?? ""}`}
                type="button"
                onClick={() => runInlineAction(action, navigate)}
                className="px-2 py-1 rounded border border-border-soft text-[11px] text-text-2 hover:text-text hover:border-border"
              >
                {action.label}
              </button>
            ))}
          </div>
        </footer>
      ) : null}
    </article>
  );
}

function PrimaryMetric({ metric }: { metric: InlineMetric }) {
  const colors = toneColors(metric.tone);
  return (
    <div className={`text-right flex-shrink-0 ${colors.text}`}>
      <div className="text-[14px] font-mono leading-tight">
        {metric.value}
        {metric.unit ?? ""}
      </div>
      <div className="text-[10px] uppercase tracking-wide text-text-3">
        {metric.label}
      </div>
    </div>
  );
}

function MetricStrip({ metrics }: { metrics: InlineMetric[] }) {
  return (
    <div className="grid grid-cols-2 gap-px px-3 pb-3">
      {metrics.slice(0, 4).map((metric) => {
        const colors = toneColors(metric.tone);
        return (
          <div
            key={metric.label}
            className="min-w-0 rounded-sm bg-surface-elev px-2 py-1.5"
          >
            <div className="text-[10px] text-text-3 truncate">{metric.label}</div>
            <div className={`text-[12px] font-mono truncate ${colors.text}`}>
              {metric.value}
              {metric.unit ?? ""}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function Legend({ payload }: { payload: InlineChartContentBlock }) {
  return (
    <div className="px-3 pb-2 flex flex-wrap gap-x-3 gap-y-1">
      {payload.series.slice(0, 4).map((series, index) => {
        const tone = series.tone ?? SERIES_TONES[index % SERIES_TONES.length];
        const colors = toneColors(tone);
        return (
          <div
            key={series.id}
            className="min-w-0 flex items-center gap-1.5 text-[11px] text-text-3"
          >
            <span
              className="w-2 h-2 rounded-full flex-shrink-0"
              style={{ backgroundColor: colors.stroke }}
            />
            <span className="truncate">{series.label}</span>
          </div>
        );
      })}
    </div>
  );
}
