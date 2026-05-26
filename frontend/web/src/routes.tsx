import { lazy, Suspense, useEffect, type ReactNode } from "react";
import { createBrowserRouter, Navigate } from "react-router-dom";
import { Layout } from "@/components/shell/Layout";
import { AppErrorBoundary } from "@/components/AppErrorBoundary";
import { noteSuccessfulPageLoad } from "@/lib/chunk-reload";

const LoginRoute = lazy(() => import("./routes/login").then((m) => ({ default: m.LoginRoute })));

const HomeRoute = lazy(() => import("./routes/home").then((m) => ({ default: m.HomeRoute })));
const StrategiesRoute = lazy(() => import("./routes/strategies").then((m) => ({ default: m.StrategiesRoute })));
const ScenariosRoute = lazy(() => import("./routes/scenarios").then((m) => ({ default: m.ScenariosRoute })));
const ScenariosNewRoute = lazy(() => import("./routes/scenarios-new").then((m) => ({ default: m.ScenariosNewRoute })));
const ScenariosDetailRoute = lazy(() => import("./routes/scenarios-detail").then((m) => ({ default: m.ScenariosDetailRoute })));
const StrategiesNewRoute = lazy(() => import("./routes/strategies-new").then((m) => ({ default: m.StrategiesNewRoute })));
// StrategiesFolderRoute: the standalone `/strategies-folder` route
// is kept for backward compatibility but resolves as a redirect to
// `/strategies?view=folder`. Lazy import kept so existing deep-links
// (bookmarks, shared URLs) continue to land on the folder view.
const StrategiesFolderRoute = lazy(() => import("./routes/strategies-folder").then((m) => ({ default: m.StrategiesFolderRoute })));
const AgentsRoute = lazy(() => import("./routes/agents").then((m) => ({ default: m.AgentsRoute })));
const AgentsEditRoute = lazy(() => import("./routes/agents-edit").then((m) => ({ default: m.AgentsEditRoute })));
const AgentsFlywheelRoute = lazy(() => import("./routes/agents-flywheel").then((m) => ({ default: m.AgentsFlywheelRoute })));
const OptimizationDetailRoute = lazy(() => import("./routes/optimizations-detail").then((m) => ({ default: m.OptimizationDetailRoute })));
const AgentDiagnosticsRoute = lazy(() => import("./routes/diagnostics-mobile").then((m) => ({ default: m.AgentDiagnosticsRoute })));
const StrategyDiagnosticsRoute = lazy(() => import("./routes/diagnostics-mobile").then((m) => ({ default: m.StrategyDiagnosticsRoute })));
const AuthoringRoute = lazy(() => import("./routes/authoring").then((m) => ({ default: m.AuthoringRoute })));
const EvalRunsRoute = lazy(() => import("./routes/eval-runs").then((m) => ({ default: m.EvalRunsRoute })));
const EvalRunDetailRoute = lazy(() => import("./routes/eval-runs-detail").then((m) => ({ default: m.EvalRunDetailRoute })));
const AgentRunDetailRoute = lazy(() => import("./routes/agent-runs-detail").then((m) => ({ default: m.AgentRunDetailRoute })));
const EvalCompareRoute = lazy(() => import("./routes/eval-compare").then((m) => ({ default: m.EvalCompareRoute })));
const LiveRoute = lazy(() => import("./routes/live").then((m) => ({ default: m.LiveRoute })));
const SetupRoute = lazy(() => import("./routes/setup").then((m) => ({ default: m.SetupRoute })));
const DocsRoute = lazy(() => import("./routes/docs").then((m) => ({ default: m.DocsRoute })));
const MemoryPage = lazy(() => import("./features/memory/MemoryPage").then((m) => ({ default: m.MemoryPage })));
const SettingsLayout = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsLayout })));
const SettingsBrokersRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsBrokersRoute })));
const SettingsDangerRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsDangerRoute })));
const SettingsGeneralRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsGeneralRoute })));
const SettingsProvidersRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsProvidersRoute })));
const SettingsSkillsRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsSkillsRoute })));
const SafetyRoute = lazy(() => import("./routes/safety").then((m) => ({ default: m.SafetyRoute })));
const ChartLabLayout = lazy(() => import("./routes/chart-lab").then((m) => ({ default: m.ChartLabLayout })));
const ChartLabOverview = lazy(() => import("./routes/chart-lab/ChartLabOverview").then((m) => ({ default: m.ChartLabOverview })));
const ChartLabPrimitives = lazy(() => import("./routes/chart-lab/ChartLabPrimitives").then((m) => ({ default: m.ChartLabPrimitives })));
const ChartLabSurfacesIndex = lazy(() => import("./routes/chart-lab/ChartLabSurfaces").then((m) => ({ default: m.ChartLabSurfacesIndex })));
const ChartLabSurfaceDetail = lazy(() => import("./routes/chart-lab/ChartLabSurfaces").then((m) => ({ default: m.ChartLabSurfaceDetail })));
const ChartLabTokens = lazy(() => import("./routes/chart-lab/ChartLabTokens").then((m) => ({ default: m.ChartLabTokens })));
const ChartLabDashboards = lazy(() => import("./routes/chart-lab/ChartLabDashboards").then((m) => ({ default: m.ChartLabDashboards })));
const ChartLabDashboardOverview = lazy(() => import("./routes/chart-lab/ChartLabDashboardOverview").then((m) => ({ default: m.ChartLabDashboardOverview })));
const ChartLabDashboardCompare = lazy(() => import("./routes/chart-lab/ChartLabDashboardCompare").then((m) => ({ default: m.ChartLabDashboardCompare })));
const ChartLabDashboardAnnotated = lazy(() => import("./routes/chart-lab/ChartLabDashboardAnnotated").then((m) => ({ default: m.ChartLabDashboardAnnotated })));
const ChartLabDashboardHero = lazy(() => import("./routes/chart-lab/ChartLabDashboardHero").then((m) => ({ default: m.ChartLabDashboardHero })));
const ChartsLayout = lazy(() => import("./routes/charts/ChartsLayout").then((m) => ({ default: m.ChartsLayout })));
const ChartsOverview = lazy(() => import("./routes/charts/ChartsOverview").then((m) => ({ default: m.ChartsOverview })));
const ChartsCompare = lazy(() => import("./routes/charts/ChartsCompare").then((m) => ({ default: m.ChartsCompare })));
const ChartsAnnotated = lazy(() => import("./routes/charts/ChartsAnnotated").then((m) => ({ default: m.ChartsAnnotated })));
const ChartsHero = lazy(() => import("./routes/charts/ChartsHero").then((m) => ({ default: m.ChartsHero })));
const MarketplaceLayout = lazy(() => import("./features/marketplace/routes/MarketplaceLayout").then((m) => ({ default: m.MarketplaceLayout })));
const MarketplaceBrowseStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceBrowseStub })));
const MarketplaceLeaderboardStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceLeaderboardStub })));
const MarketplaceLineageStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceLineageStub })));
const MarketplaceCreatorStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceCreatorStub })));
const MarketplaceSellStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceSellStub })));
const MarketplaceReceiptStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceReceiptStub })));

/**
 * Marker that only mounts after its parent Suspense has resolved
 * (i.e., the lazy route chunk loaded successfully). Used as a sibling
 * of the route element to clear the reload-attempted guard only once
 * we have proof that the current bundle's chunks are reachable. See
 * PR #317 review (P1).
 */
function RouteLoaded() {
  useEffect(() => {
    noteSuccessfulPageLoad();
  }, []);
  return null;
}

function page(element: ReactNode) {
  // AppErrorBoundary wraps the Suspense boundary so chunk-load errors
  // (Vite-after-deploy: stale `index.html` referencing a hash that no
  // longer exists) are caught here and routed through
  // `attemptChunkReload` rather than crashing to the global error UI.
  // Non-chunk errors fall through unchanged.
  return (
    <AppErrorBoundary>
      <Suspense fallback={<div className="px-4 py-6 text-[13px] text-text-3">Loading…</div>}>
        <RouteLoaded />
        {element}
      </Suspense>
    </AppErrorBoundary>
  );
}

export const router = createBrowserRouter([
  // Full-screen auth gate — lives outside the Layout shell so it occupies
  // the whole viewport without sidebar/topbar chrome.
  {
    path: "/login",
    element: page(<LoginRoute />),
  },
  {
    path: "/",
    element: <Layout />,
    children: [
      { index: true, element: page(<HomeRoute />) },
      { path: "strategies", element: page(<StrategiesRoute />) },
      { path: "strategies-folder", element: page(<StrategiesFolderRoute />) },
      { path: "strategies/new", element: page(<StrategiesNewRoute />) },
      { path: "strategies/:id", element: page(<AuthoringRoute />) },
      {
        path: "strategies/:id/diagnostics",
        element: page(<StrategyDiagnosticsRoute />),
      },
      { path: "agents", element: page(<AgentsRoute />) },
      { path: "agents/memory", element: page(<MemoryPage />) },
      { path: "agents/skills", element: page(<SettingsSkillsRoute />) },
      { path: "agents/new", element: page(<AgentsEditRoute />) },
      { path: "agents/:id/flywheel", element: page(<AgentsFlywheelRoute />) },
      { path: "agents/:id", element: page(<AgentsEditRoute />) },
      {
        path: "agents/:id/diagnostics",
        element: page(<AgentDiagnosticsRoute />),
      },
      {
        path: "agents/:id/optimizations/:runId",
        element: page(<OptimizationDetailRoute />),
      },
      { path: "scenarios", element: page(<ScenariosRoute />) },
      { path: "scenarios/new", element: page(<ScenariosNewRoute />) },
      { path: "scenarios/:id", element: page(<ScenariosDetailRoute />) },
      { path: "authoring", element: page(<AuthoringRoute />) },
      { path: "authoring/:id", element: page(<AuthoringRoute />) },
      { path: "eval-runs", element: page(<EvalRunsRoute />) },
      { path: "eval-runs/:runId", element: page(<EvalRunDetailRoute />) },
      { path: "agent-runs/:runId", element: page(<AgentRunDetailRoute />) },
      { path: "eval-runs/compare", element: page(<EvalCompareRoute />) },
      // Backward-compat alias: deep links to /memory continue to work.
      { path: "memory", element: <Navigate to="/agents/memory" replace /> },
      { path: "live/:id", element: page(<LiveRoute />) },
      { path: "setup", element: page(<SetupRoute />) },
      { path: "safety", element: page(<SafetyRoute />) },
      {
        path: "chart-lab",
        element: page(<ChartLabLayout />),
        children: [
          { index: true, element: page(<ChartLabOverview />) },
          { path: "primitives", element: page(<ChartLabPrimitives />) },
          {
            path: "surfaces",
            element: page(<ChartLabSurfacesIndex />),
          },
          {
            path: "surfaces/:surface",
            element: page(<ChartLabSurfaceDetail />),
          },
          { path: "tokens", element: page(<ChartLabTokens />) },
          { path: "dashboards", element: page(<ChartLabDashboards />) },
          { path: "dashboards/overview", element: page(<ChartLabDashboardOverview />) },
          { path: "dashboards/compare", element: page(<ChartLabDashboardCompare />) },
          { path: "dashboards/annotated", element: page(<ChartLabDashboardAnnotated />) },
          { path: "dashboards/hero", element: page(<ChartLabDashboardHero />) },
        ],
      },
      // Charts dashboard section (chart-rework spec Track B). B0 mounts
      // the route topology with placeholder shells; B1–B4 each replace
      // a shell with the real surface. Sidebar entry is cookie-gated;
      // the routes themselves are reachable by deep-link even without
      // the cookie (lower friction for QA + the chart-lab dashboards tab).
      {
        path: "charts",
        element: page(<ChartsLayout />),
        children: [
          { index: true, element: <Navigate to="overview" replace /> },
          { path: "overview", element: page(<ChartsOverview />) },
          { path: "compare", element: page(<ChartsCompare />) },
          { path: "annotated", element: page(<ChartsAnnotated />) },
          { path: "hero", element: page(<ChartsHero />) },
        ],
      },
      {
        path: "marketplace",
        element: page(<MarketplaceLayout />),
        children: [
          { index: true, element: page(<MarketplaceBrowseStub />) },
          { path: "leaderboard", element: page(<MarketplaceLeaderboardStub />) },
          { path: "leaderboard/:sliceId", element: page(<MarketplaceLeaderboardStub />) },
          { path: "lineage/:name", element: page(<MarketplaceLineageStub />) },
          { path: "creator/:handleOrAddr", element: page(<MarketplaceCreatorStub />) },
          { path: "sell", element: page(<MarketplaceSellStub />) },
          { path: "receipts/:tx", element: page(<MarketplaceReceiptStub />) },
        ],
      },
      { path: "docs", element: page(<DocsRoute />) },
      {
        path: "settings",
        element: page(<SettingsLayout />),
        children: [
          { index: true, element: <Navigate to="general" replace /> },
          { path: "general", element: page(<SettingsGeneralRoute />) },
          { path: "providers", element: page(<SettingsProvidersRoute />) },
          { path: "brokers", element: page(<SettingsBrokersRoute />) },
          { path: "skills", element: <Navigate to="/agents/skills" replace /> },
          { path: "danger", element: page(<SettingsDangerRoute />) },
        ],
      },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
]);
