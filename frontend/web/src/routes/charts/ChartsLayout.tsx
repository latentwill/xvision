// /charts — Charts dashboard section (chart-rework spec Track B).
//
// Subnav tabs (Overview · Compare · Annotated · Hero) sit at the top of the
// section, mirroring the chart-lab pattern. Per spec §11.4, all four canvases
// are gated behind the staff cookie `xvn.chartv2=1` during the B0–B4 ramp;
// the gate flips off at B-rollout.
//
// B0 mounts placeholder shells for the four routes so the navigation surface
// is testable and deep-linkable before the canvases land. B1–B4 each replace
// their placeholder with the real surface.

import { Navigate, NavLink, Outlet, useLocation } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";

const TABS: { to: string; label: string; end?: boolean }[] = [
  { to: "/charts/overview", label: "Overview" },
  { to: "/charts/compare", label: "Compare" },
  { to: "/charts/annotated", label: "Annotated" },
  { to: "/charts/hero", label: "Hero" },
];

export function ChartsLayout() {
  const { pathname } = useLocation();
  if (pathname === "/charts" || pathname === "/charts/") {
    return <Navigate to="/charts/overview" replace />;
  }

  return (
    <div className="px-4 py-4">
      <Topbar
        title="Charts"
        sub="Multi-strategy dashboards (chart-rework Track B)."
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
