import type { PipelineDef } from "./types.gen/PipelineDef";
import type { RouteBranch } from "./types.gen/RouteBranch";
import type { RouteContextField } from "./types.gen/RouteContextField";
import type { RouteDefinition } from "./types.gen/RouteDefinition";
import type { RouteGraphEdge } from "./types.gen/RouteGraphEdge";
import type { RouteTraceMode } from "./types.gen/RouteTraceMode";
import type { SetPipelineReq } from "./types.gen/SetPipelineReq";

const defaultContextFields: RouteContextField[] = [
  "market_snapshot",
  "tool_state",
  "available_targets",
  "regime_summary",
];

const routeBranch: RouteBranch = { target_role: "trader" };
const graphEdges: RouteGraphEdge[] = [];
const traceMode: RouteTraceMode = "compact";

const route: RouteDefinition = {
  router_role: "router",
  branches: [routeBranch],
  graph_edges: graphEdges,
  context_fields: defaultContextFields,
  trace_mode: traceMode,
};

export const generatedPipelineTypeAcceptsRoute: PipelineDef = {
  kind: "sequential",
  edges: [],
  route,
};

export const generatedSetPipelineReqTypeAcceptsRoute: SetPipelineReq = {
  strategy_id: "01HZROUTETYPECONTRACT",
  kind: "sequential",
  edges: [],
  route,
};
