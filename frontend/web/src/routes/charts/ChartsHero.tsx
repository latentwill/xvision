// /charts/hero — B4 Gradient Warm Hero Dashboard.
//
// Reuses the same /api/v2/charts/dashboards/overview payload as B1
// (real builder pairs each Strategy with its latest backtest run —
// follow-up). Per spec §11.3 resolution, B4 mounts only here; the
// /-replacement decision is the B5 review milestone.

import { useQuery } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import { dashboardChartKeys, getDashboardOverview } from "@/api/chart";
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";
import { GradientHeroDashboard } from "@/components/chart/v2/surfaces/GradientHeroDashboard";

export function ChartsHero() {
  const q = useQuery({
    queryKey: dashboardChartKeys.overview(),
    queryFn: () => getDashboardOverview(),
    staleTime: 30_000,
  });

  if (q.isLoading) {
    return (
      <EmptyState
        title="Loading hero dashboard…"
        message="Fetching the multi-strategy equity bundle."
      />
    );
  }

  if (q.isError) {
    const msg =
      q.error instanceof ApiError
        ? `${q.error.code}: ${q.error.message}`
        : "Failed to load the dashboard payload.";
    return <EmptyState title="Hero unavailable" message={msg} />;
  }

  if (!q.data) return null;

  return <GradientHeroDashboard payload={q.data} />;
}
