// /charts/compare — B2 Comparison AB Scalable.
//
// Reuses the B0 backend stub /api/v2/charts/dashboards/overview as the
// data source (same MultiStrategyEquityBundle). Selection state is
// URL-synced via useChart2Roster: deep-link with `?ids=fib,ema,brk`.

import { useQuery } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { dashboardChartKeys, getDashboardOverview } from "@/api/chart";
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";
import { ComparisonABDashboard } from "@/components/chart/v2/surfaces/ComparisonABDashboard";

export function ChartsCompare() {
  const q = useQuery({
    queryKey: dashboardChartKeys.overview(),
    queryFn: () => getDashboardOverview(),
    staleTime: 30_000,
  });

  if (q.isLoading) {
    return (
      <EmptyState
        title="Loading comparison…"
        message="Fetching the multi-strategy equity bundle."
      />
    );
  }

  if (q.isError) {
    const msg =
      q.error instanceof ApiError
        ? `${q.error.code}: ${q.error.message}`
        : "Failed to load the comparison payload.";
    return <EmptyState title="Comparison unavailable" message={msg} />;
  }

  if (!q.data) return null;

  return <ComparisonABDashboard payload={q.data} />;
}
