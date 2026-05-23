// /chart-lab/dashboards — Track B dashboard surface review tab.
//
// Lists the four B-milestone canvases. Each row links to the production
// route (`/charts/<slug>`); once a milestone lands, the row becomes a
// real preview. Until then, the production route shows a placeholder
// EmptyState and clicking through is just verifying that the route
// topology is wired.
//
// See docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md §4A.7.

import { Link } from "react-router-dom";

const DASHBOARDS: { slug: string; label: string; status: string }[] = [
  {
    slug: "overview",
    label: "Overview · Dark Minimal Strategy Dashboard (Chart 01)",
    status: "B1 — coming soon",
  },
  {
    slug: "compare",
    label: "Compare · Comparison AB Scalable (Chart 02)",
    status: "B2 — coming soon",
  },
  {
    slug: "annotated",
    label: "Annotated · AI Annotation Chart (Chart 03)",
    status: "B3 — coming soon",
  },
  {
    slug: "hero",
    label: "Hero · Gradient Warm Hero Dashboard (Chart 05)",
    status: "B4 — coming soon",
  },
];

export function ChartLabDashboards() {
  return (
    <div className="space-y-4">
      <header className="space-y-1">
        <h2 className="serif text-[20px] leading-tight text-text">Dashboards</h2>
        <p className="text-[13px] text-text-3 max-w-2xl">
          Track B canvases of the chart-rework spec, rendered against the
          production route. Each becomes a real preview once its B-milestone
          lands; until then the production route shows a placeholder. Chart 04
          (Liquidation Heatmap) is parked as the F-CHART-LIQHEAT followup.
        </p>
      </header>

      <ul className="border border-border rounded-card divide-y divide-border bg-surface-card">
        {DASHBOARDS.map((d) => (
          <li key={d.slug} className="flex items-center justify-between gap-4 px-4 py-3">
            <Link
              to={`/charts/${d.slug}`}
              className="text-text hover:text-gold text-[14px] flex-1"
            >
              {d.label}
            </Link>
            <span className="caps">{d.status}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}
