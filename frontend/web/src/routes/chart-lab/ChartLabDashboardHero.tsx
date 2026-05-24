// /chart-lab/dashboards/hero — fixture render of B4's
// GradientHeroDashboard against multi-strategy-equity.json.

import fixture from "@/components/chart/v2/__fixtures__/multi-strategy-equity.json";
import { GradientHeroDashboard } from "@/components/chart/v2/surfaces/GradientHeroDashboard";
import type { MultiStrategyEquityBundle } from "@/components/chart/v2/types";

export function ChartLabDashboardHero() {
  const payload = fixture as unknown as MultiStrategyEquityBundle;
  return (
    <div className="space-y-4">
      <div className="text-[12px] text-text-3">
        Rendered against{" "}
        <code className="text-text-2">multi-strategy-equity.json</code>. Same
        fixture as Overview; this surface is the "hero" variant per spec §3 B4.
      </div>
      <GradientHeroDashboard payload={payload} />
    </div>
  );
}
