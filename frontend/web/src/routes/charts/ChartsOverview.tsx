// /charts/overview — Chart 01 Dark Minimal Strategy Dashboard.
// B0: placeholder shell. B1 replaces with the real DarkMinimalDashboard
// surface composed of MultiStrategyEquityPane + DrawdownCard +
// MonthlyReturnsHeatmap + KpiCard×5 + Topbar.
//
// See docs/superpowers/plans/2026-05-23-charts-section-b1-overview-dashboard.md.

import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";

export function ChartsOverview() {
  return (
    <EmptyState
      title="B1: Overview — coming soon"
      message="The Dark Minimal Strategy Dashboard (Chart 01) lands in milestone B1: multi-strategy equity overlay, drawdown card, monthly-returns heatmap, and a 5-up KPI row."
    />
  );
}
