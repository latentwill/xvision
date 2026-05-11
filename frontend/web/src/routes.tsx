import { createBrowserRouter, Navigate } from "react-router-dom";
import { Layout } from "@/components/shell/Layout";
import { HomeRoute } from "./routes/home";
import { StrategiesRoute } from "./routes/strategies";
import { AuthoringRoute } from "./routes/authoring";
import { EvalRunsRoute } from "./routes/eval-runs";
import { EvalRunDetailRoute } from "./routes/eval-runs-detail";
import { EvalCompareRoute } from "./routes/eval-compare";
import { SetupRoute } from "./routes/setup";
import {
  SettingsLayout,
  SettingsBrokersRoute,
  SettingsDangerRoute,
  SettingsIdentityRoute,
  SettingsProvidersRoute,
} from "./routes/settings";

export const router = createBrowserRouter([
  {
    path: "/",
    element: <Layout />,
    children: [
      { index: true, element: <HomeRoute /> },
      { path: "strategies", element: <StrategiesRoute /> },
      { path: "authoring", element: <AuthoringRoute /> },
      { path: "authoring/:id", element: <AuthoringRoute /> },
      { path: "eval-runs", element: <EvalRunsRoute /> },
      { path: "eval-runs/:runId", element: <EvalRunDetailRoute /> },
      { path: "eval-runs/compare", element: <EvalCompareRoute /> },
      { path: "setup", element: <SetupRoute /> },
      {
        path: "settings",
        element: <SettingsLayout />,
        children: [
          { index: true, element: <Navigate to="providers" replace /> },
          { path: "providers", element: <SettingsProvidersRoute /> },
          { path: "brokers", element: <SettingsBrokersRoute /> },
          { path: "identity", element: <SettingsIdentityRoute /> },
          { path: "danger", element: <SettingsDangerRoute /> },
        ],
      },
      { path: "*", element: <Navigate to="/" replace /> },
    ],
  },
]);
