// /charts/annotated — B3 AI Annotation Chart.
//
// Source switching per spec §11.2 resolution:
//   ?source=run&run_id=...   → /api/v2/charts/annotated/:run_id
//   ?source=live&symbol=...  → /api/v2/charts/annotated/live/:symbol
//   default                  → ?source=run with `run_id=demo` (B3 stub)
//
// Empty annotation payloads carry a `note` so the surface can distinguish
// "review not yet run" from live/no-data states.

import { useSearchParams } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import {
  annotatedChartKeys,
  getAnnotatedLive,
  getAnnotatedRun,
  type AnnotatedSource,
} from "@/api/chart";
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";
import { AIAnnotationDashboard } from "@/components/chart/v2/surfaces/AIAnnotationDashboard";

export function ChartsAnnotated() {
  const [params] = useSearchParams();
  const sourceParam = (params.get("source") ?? "run") as AnnotatedSource;
  const runId = params.get("run_id") ?? "demo";
  const symbol = params.get("symbol") ?? "BTC/USDT";

  const isLive = sourceParam === "live";

  const q = useQuery({
    queryKey: isLive
      ? annotatedChartKeys.live(symbol)
      : annotatedChartKeys.run(runId),
    queryFn: () => (isLive ? getAnnotatedLive(symbol) : getAnnotatedRun(runId)),
    staleTime: 30_000,
  });

  if (q.isLoading) {
    return (
      <EmptyState
        title="Loading annotations…"
        message={
          isLive
            ? `Fetching live candles for ${symbol}.`
            : `Fetching annotations for run ${runId}.`
        }
      />
    );
  }

  if (q.isError) {
    const msg =
      q.error instanceof ApiError
        ? `${q.error.code}: ${q.error.message}`
        : "Failed to load the annotation payload.";
    return <EmptyState title="Annotations unavailable" message={msg} />;
  }

  if (!q.data) return null;

  const isDemo = !isLive && runId === "demo";

  return (
    <div className="flex flex-col gap-0">
      {isDemo && (
        <div
          data-testid="demo-data-banner"
          className="flex items-center gap-2 px-4 py-2 bg-warn/10 border-b border-warn/30 text-[12px] text-warn"
          role="note"
          aria-label="Demo data — illustrative annotations"
        >
          <span className="font-semibold">Demo data</span>
          <span className="text-warn/70">—</span>
          <span>Illustrative annotations only, not a real run.</span>
        </div>
      )}
      <AIAnnotationDashboard payload={q.data} />
    </div>
  );
}
