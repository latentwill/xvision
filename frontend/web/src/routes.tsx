import { lazy, Suspense, type ReactNode } from "react";
import { createBrowserRouter, Navigate } from "react-router-dom";
import { Layout } from "@/components/shell/Layout";

const HomeRoute = lazy(() => import("./routes/home").then((m) => ({ default: m.HomeRoute })));
const StrategiesRoute = lazy(() => import("./routes/strategies").then((m) => ({ default: m.StrategiesRoute })));
const ScenariosRoute = lazy(() => import("./routes/scenarios").then((m) => ({ default: m.ScenariosRoute })));
const ScenariosNewRoute = lazy(() => import("./routes/scenarios-new").then((m) => ({ default: m.ScenariosNewRoute })));
const ScenariosDetailRoute = lazy(() => import("./routes/scenarios-detail").then((m) => ({ default: m.ScenariosDetailRoute })));
const StrategiesNewRoute = lazy(() => import("./routes/strategies-new").then((m) => ({ default: m.StrategiesNewRoute })));
const AgentsRoute = lazy(() => import("./routes/agents").then((m) => ({ default: m.AgentsRoute })));
const AgentsEditRoute = lazy(() => import("./routes/agents-edit").then((m) => ({ default: m.AgentsEditRoute })));
const AuthoringRoute = lazy(() => import("./routes/authoring").then((m) => ({ default: m.AuthoringRoute })));
const EvalRunsRoute = lazy(() => import("./routes/eval-runs").then((m) => ({ default: m.EvalRunsRoute })));
const EvalRunDetailRoute = lazy(() => import("./routes/eval-runs-detail").then((m) => ({ default: m.EvalRunDetailRoute })));
const EvalCompareRoute = lazy(() => import("./routes/eval-compare").then((m) => ({ default: m.EvalCompareRoute })));
const LiveRoute = lazy(() => import("./routes/live").then((m) => ({ default: m.LiveRoute })));
const SetupRoute = lazy(() => import("./routes/setup").then((m) => ({ default: m.SetupRoute })));
const SettingsLayout = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsLayout })));
const SettingsBrokersRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsBrokersRoute })));
const SettingsDangerRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsDangerRoute })));
const SettingsProvidersRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsProvidersRoute })));
const SettingsSkillsRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsSkillsRoute })));

function page(element: ReactNode) {
  return (
    <Suspense fallback={<div className="px-4 py-6 text-[13px] text-text-3">Loading…</div>}>
      {element}
    </Suspense>
  );
}

export const router = createBrowserRouter([
  {
    path: "/",
    element: <Layout />,
    children: [
      { index: true, element: page(<HomeRoute />) },
      { path: "strategies", element: page(<StrategiesRoute />) },
      { path: "strategies/new", element: page(<StrategiesNewRoute />) },
      { path: "agents", element: page(<AgentsRoute />) },
      { path: "agents/skills", element: page(<SettingsSkillsRoute />) },
      { path: "agents/new", element: page(<AgentsEditRoute />) },
      { path: "agents/:id", element: page(<AgentsEditRoute />) },
      { path: "scenarios", element: page(<ScenariosRoute />) },
      { path: "scenarios/new", element: page(<ScenariosNewRoute />) },
      { path: "scenarios/:id", element: page(<ScenariosDetailRoute />) },
      { path: "authoring", element: page(<AuthoringRoute />) },
      { path: "authoring/:id", element: page(<AuthoringRoute />) },
      { path: "eval-runs", element: page(<EvalRunsRoute />) },
      { path: "eval-runs/:runId", element: page(<EvalRunDetailRoute />) },
      { path: "eval-runs/compare", element: page(<EvalCompareRoute />) },
      { path: "live/:id", element: page(<LiveRoute />) },
      { path: "setup", element: page(<SetupRoute />) },
      {
        path: "settings",
        element: page(<SettingsLayout />),
        children: [
          { index: true, element: <Navigate to="providers" replace /> },
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
