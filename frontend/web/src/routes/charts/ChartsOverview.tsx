// /charts/overview — B1 Dark Minimal Strategy Dashboard.
//
// B0 shipped a placeholder shell here; B1 fetches the
// MultiStrategyEquityBundle from /api/v2/charts/dashboards/overview
// (still the B0 fixture-backed stub on the backend; a follow-up swaps
// in the real builder pairing each Strategy with its latest backtest
// run equity series) and renders <DarkMinimalDashboard>.

import { useQuery } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { dashboardChartKeys, getDashboardOverview } from "@/api/chart";
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";
import { DarkMinimalDashboard } from "@/components/chart/v2/surfaces/DarkMinimalDashboard";

export function ChartsOverview() {
  const q = useQuery({
    queryKey: dashboardChartKeys.overview(),
    queryFn: () => getDashboardOverview(),
    staleTime: 30_000,
  });

  if (q.isLoading) {
    return (
      <EmptyState
        title="Loading dashboard…"
        message="Fetching the multi-strategy equity bundle."
      />
    );
  }

  if (q.isError) {
    const msg =
      q.error instanceof ApiError
        ? `${q.error.code}: ${q.error.message}`
        : "Failed to load the dashboard payload.";
    return (
      <EmptyState
        title="Dashboard unavailable"
        message={msg}
      />
    );
  }

  if (!q.data) return null;

  return <DarkMinimalDashboard payload={q.data} />;
}
