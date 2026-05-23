// /charts/annotated — Chart 03 AI Annotation Chart.
// B0: placeholder shell. B3 replaces with the real AIAnnotationDashboard
// surface: KlineCandlePane + AnnotationOverlay + InsightLog.
//
// See docs/superpowers/plans/2026-05-23-charts-section-b3-ai-annotation.md.

import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";

export function ChartsAnnotated() {
  return (
    <EmptyState
      title="B3: Annotated — coming soon"
      message="The AI Annotation chart (Chart 03) lands in milestone B3: candle pane with EMA(21) overlay, AI callouts anchored to candles, collapsible insight log, and live/run source switching."
    />
  );
}
