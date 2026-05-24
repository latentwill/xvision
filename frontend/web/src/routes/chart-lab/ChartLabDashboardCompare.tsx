// /chart-lab/dashboards/compare — fixture render of B2's
// ComparisonABDashboard. Independent of backend uptime.
//
// MemoryRouter wraps the surface so `useChart2Roster` (which reads/
// writes `?ids=`) has a router context inside chart-lab without
// affecting the real router's URL.

import { MemoryRouter } from "react-router-dom";

import fixture from "@/components/chart/v2/__fixtures__/multi-strategy-equity.json";
import { ComparisonABDashboard } from "@/components/chart/v2/surfaces/ComparisonABDashboard";
import type { MultiStrategyEquityBundle } from "@/components/chart/v2/types";

export function ChartLabDashboardCompare() {
  const payload = fixture as unknown as MultiStrategyEquityBundle;
  return (
    <div className="space-y-4">
      <div className="text-[12px] text-text-3">
        Rendered against{" "}
        <code className="text-text-2">multi-strategy-equity.json</code> in a
        scoped MemoryRouter. Production route at{" "}
        <code className="text-text-2">/charts/compare</code> uses the real
        URL for <code>?ids=</code> selection state.
      </div>
      <MemoryRouter initialEntries={["/charts/compare"]}>
        <ComparisonABDashboard payload={payload} />
      </MemoryRouter>
    </div>
  );
}
