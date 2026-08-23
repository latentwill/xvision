import { afterEach, describe, expect, it, vi } from "vitest";
import * as client from "@/api/client";
import type { RouteBranch as GeneratedRouteBranch } from "@/api/types.gen/RouteBranch";
import type { RouteContextField as GeneratedRouteContextField } from "@/api/types.gen/RouteContextField";
import type { RouteDefinition as GeneratedRouteDefinition } from "@/api/types.gen/RouteDefinition";
import type { RouteGraphEdge as GeneratedRouteGraphEdge } from "@/api/types.gen/RouteGraphEdge";
import type { StrategyDiagnostics } from "@/api/diagnostics";
import {
  setStrategyRoute,
  validateStrategyRoute,
  type RouteBranch,
  type RouteContextField,
  type RouteDefinition,
  type RouteGraphEdge,
  type RouteReadiness,
  type StrategyRouteOut,
} from "@/api/strategies";

afterEach(() => vi.restoreAllMocks());

type IsExact<Actual, Expected> =
  (<T>() => T extends Actual ? 1 : 2) extends
  (<T>() => T extends Expected ? 1 : 2)
    ? (<T>() => T extends Expected ? 1 : 2) extends
        (<T>() => T extends Actual ? 1 : 2)
      ? true
      : false
    : false;

type Assert<T extends true> = T;

const routeTypeContractChecks: [
  Assert<IsExact<RouteDefinition, GeneratedRouteDefinition>>,
  Assert<IsExact<RouteBranch, GeneratedRouteBranch>>,
  Assert<IsExact<RouteGraphEdge, GeneratedRouteGraphEdge>>,
  Assert<IsExact<RouteContextField, GeneratedRouteContextField>>,
] = [true, true, true, true];

void routeTypeContractChecks;

const routeReadinessWithLaunchReasons: RouteReadiness = {
  routed: true,
  context_fields: ["available_targets"],
  launchable: false,
  reasons: [
    {
      code: "route.unreachable_provider_model",
      message: "Bind each branch target to a model that can run before launching.",
      blocking: true,
    },
  ],
};

const strategyDiagnosticsWithRoute: StrategyDiagnostics = {
  strategy_id: "strat-1",
  per_agent: [],
  unregistered_tools: [],
  has_decision_path: true,
  launchable: false,
  route: routeReadinessWithLaunchReasons,
};

void routeReadinessWithLaunchReasons;
void strategyDiagnosticsWithRoute;

// @ts-expect-error RouteContextField must not accept Rust-excluded fields.
const strategyStateContextField: RouteContextField = "strategy_state";
void strategyStateContextField;

// @ts-expect-error RouteBranch must only accept the generated target_role shape.
const routeBranchWithExtraProperties: RouteBranch = { target_role: "analyst", label: "Analyst branch" };
void routeBranchWithExtraProperties;

// @ts-expect-error RouteGraphEdge must not accept source_role/target_role aliases.
const routeGraphEdgeWithAlias: RouteGraphEdge = { source_role: "analyst", target_role: "trader" };
void routeGraphEdgeWithAlias;

const route: RouteDefinition = {
  router_role: "router",
  branches: [{ target_role: "analyst" }],
  graph_edges: [{ from_role: "analyst", to_role: "trader" }],
  context_fields: ["market_snapshot", "available_targets"],
  trace_mode: "compact",
} satisfies GeneratedRouteDefinition;

const routeWithoutGraphEdges: RouteDefinition = {
  router_role: "router",
  branches: [{ target_role: "analyst" }],
  context_fields: ["market_snapshot", "available_targets"],
  trace_mode: "compact",
} satisfies GeneratedRouteDefinition;

const routeHelperAcceptsRouteWithoutGraphEdges: Parameters<typeof setStrategyRoute>[1] =
  routeWithoutGraphEdges;
const validateHelperAcceptsRouteWithoutGraphEdges: Parameters<typeof validateStrategyRoute>[1] =
  routeWithoutGraphEdges;

void routeHelperAcceptsRouteWithoutGraphEdges;
void validateHelperAcceptsRouteWithoutGraphEdges;

const routeOut = {
  strategy: { manifest: { id: "strat-1" } },
  readiness: {
    routed: true,
    context_fields: ["market_snapshot", "available_targets"],
  },
} as StrategyRouteOut;

describe("Route Builder strategy API", () => {
  it("PUTs setStrategyRoute to the route endpoint with the exact RouteDefinition body", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue(routeOut as never);

    await setStrategyRoute("strat/1", route);

    expect(spy).toHaveBeenCalledWith(
      "/api/strategy/strat%2F1/route",
      expect.objectContaining({ method: "PUT" }),
    );
    const [, options] = spy.mock.calls[0]!;
    expect(JSON.parse((options as RequestInit).body as string)).toEqual(route);
  });

  it("POSTs validateStrategyRoute to the dry-run endpoint without wrapping or dropping readiness fields", async () => {
    const spy = vi.spyOn(client, "apiFetch").mockResolvedValue(routeOut as never);

    await validateStrategyRoute("strat-2", route);

    expect(spy).toHaveBeenCalledWith(
      "/api/strategy/strat-2/route/validate",
      expect.objectContaining({ method: "POST" }),
    );
    const [, options] = spy.mock.calls[0]!;
    expect(JSON.parse((options as RequestInit).body as string)).toEqual(route);
  });
});
