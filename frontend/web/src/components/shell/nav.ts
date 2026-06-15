// Shared primary navigation — the single source of truth for BOTH the left
// Sidebar and the ⌘K command palette. Keeping one list means the palette can
// never drift out of sync with the sidebar again (operator QA: the palette was
// missing Agents / Charts / Live / Marketplace / Optimizer / Docs, so the
// Optimizer page could not be reached from ⌘K at all).
import { type IconName } from "@/components/primitives/Icon";

export type NavItem = {
  to: string;
  label: string;
  icon: IconName;
  /** One-line description; surfaced as the command-palette result subtitle. */
  summary: string;
};

// Charts section (chart-rework Track B) is unconditional after B-rollout — the
// `xvn.chartv2` cookie gate was removed once B0–B4 shipped (see
// docs/superpowers/plans/2026-05-23-charts-section-b5-hero-default-review.md).
export const PRIMARY_NAV: NavItem[] = [
  { to: "/", label: "Dashboard", icon: "home", summary: "Workspace status at a glance" },
  { to: "/strategies", label: "Strategies", icon: "chart", summary: "Manage strategies" },
  { to: "/agents", label: "Agents", icon: "user", summary: "Reusable agent templates" },
  { to: "/scenarios", label: "Scenarios", icon: "list", summary: "Browse and create eval scenarios" },
  { to: "/charts", label: "Charts", icon: "chartPie", summary: "Market charts & chart lab" },
  { to: "/eval-runs", label: "Eval", icon: "bars", summary: "Backtests and paper-trade runs" },
  { to: "/live", label: "Live Trading", icon: "play", summary: "Live trading console" },
  { to: "/marketplace", label: "Marketplace", icon: "bag", summary: "Buy and sell strategies as on-chain agents" },
  { to: "/optimizer", label: "Optimizer", icon: "pulse", summary: "Autonomous strategy optimization cycles" },
  { to: "/docs", label: "Docs", icon: "book", summary: "Product manual & guides" },
  { to: "/settings", label: "Settings", icon: "sliders", summary: "Providers, brokers & danger zone" },
];
