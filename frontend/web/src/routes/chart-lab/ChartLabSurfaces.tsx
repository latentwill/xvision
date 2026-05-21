import { Link, Outlet, useParams } from "react-router-dom";
import {
  CompareChartV2,
  LiveChartV2,
  RunChartV2,
  ScenarioChartV2,
  StrategyChartV2,
  WizardPreviewChartV2,
} from "@/components/chart/v2/surfaces";
import { getChart2Fixture } from "@/components/chart/v2/hooks/useChart2Fixture";

const SURFACES = ["run", "compare", "scenario", "strategy", "live", "wizard"] as const;
type Slug = (typeof SURFACES)[number];

export function ChartLabSurfacesIndex() {
  return (
    <div className="grid gap-2 max-w-xl">
      <p className="text-[13px] text-text-2">
        Pick a surface to render full-bleed against its fixture.
      </p>
      <ul className="grid gap-2">
        {SURFACES.map((slug) => (
          <li key={slug}>
            <Link
              to={`/chart-lab/surfaces/${slug}`}
              className="block border border-border rounded-card px-3 py-2 text-[13px] text-text hover:bg-surface-hover"
            >
              {slug}
            </Link>
          </li>
        ))}
      </ul>
      <Outlet />
    </div>
  );
}

export function ChartLabSurfaceDetail() {
  const { surface } = useParams<{ surface: Slug }>();
  if (!surface || !SURFACES.includes(surface as Slug)) {
    return <div className="text-[13px] text-text-3">Unknown surface: {surface}</div>;
  }
  switch (surface) {
    case "run":
      return <RunChartV2 payload={getChart2Fixture("run")} />;
    case "compare":
      return <CompareChartV2 payload={getChart2Fixture("compare")} />;
    case "scenario":
      return <ScenarioChartV2 payload={getChart2Fixture("scenario")} />;
    case "strategy":
      return <StrategyChartV2 payload={getChart2Fixture("strategy")} />;
    case "live":
      return <LiveChartV2 payload={getChart2Fixture("live")} />;
    case "wizard":
      return <WizardPreviewChartV2 payload={getChart2Fixture("wizard")} />;
  }
}
