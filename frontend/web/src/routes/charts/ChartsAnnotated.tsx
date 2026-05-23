// /charts/annotated — B3 AI Annotation Chart.
//
// Source switching per spec §11.2 resolution:
//   ?source=run&run_id=...   → /api/v2/charts/annotated/:run_id
//   ?source=live&symbol=...  → /api/v2/charts/annotated/live/:symbol
//   default                  → ?source=run with `run_id=demo` (B3 stub)
//
// When source=live and the response has no annotations, the surface
// renders an EmptyState explaining the producer is not configured
// (live producer is out of scope per spec §9).

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

  return <AIAnnotationDashboard payload={q.data} />;
}
