// /chart-lab — staff-only sandbox for the chart-v2 primitives & surfaces.
//
// Tabs (Overview · Primitives · Surfaces · Tokens) are routed via the same
// URL prefix so the operator can deep-link to a specific tab or to a specific
// surface composition.

import { NavLink, Outlet } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";

const TABS: { to: string; label: string; end?: boolean }[] = [
  { to: "/chart-lab", label: "Overview", end: true },
  { to: "/chart-lab/primitives", label: "Primitives" },
  { to: "/chart-lab/surfaces", label: "Surfaces" },
  { to: "/chart-lab/tokens", label: "Tokens" },
];

export function ChartLabLayout() {
  return (
    <div className="px-4 py-4">
      <Topbar
        title="Chart Lab"
        sub="Staff-only sandbox for chart-v2 (klinecharts + uPlot)."
      />
      <nav className="mt-3 flex items-center gap-1 border-b border-border">
        {TABS.map((tab) => (
          <NavLink
            key={tab.to}
            to={tab.to}
            end={tab.end}
            className={({ isActive }) =>
              `px-3 py-1.5 text-[13px] -mb-px border-b-2 ${
                isActive
                  ? "border-gold text-text"
                  : "border-transparent text-text-3 hover:text-text-2"
              }`
            }
          >
            {tab.label}
          </NavLink>
        ))}
      </nav>
      <div className="mt-4">
        <Outlet />
      </div>
    </div>
  );
}
