import { useEffect, useMemo, useState, type ReactNode } from "react";
import { useMutation, useQueryClient } from "@tanstack/react-query";

import {
  setStrategyRoute,
  strategyKeys,
  validateStrategyRoute,
  type AgentRef,
  type PipelineDef,
  type RouteContextField,
  type RouteDefinition,
  type RouteGraphEdge,
  type RouteReadiness,
  type RouteTraceMode,
} from "@/api/strategies";
import type { Agent } from "@/api/agents";
import { Card } from "@/components/primitives/Card";
import {
  SignalCheckboxMenu,
  SignalSearchableSelectMenu,
  SignalSelectMenu,
  type SearchableSelectOption,
} from "@/components/primitives/SignalMenu";
import { RouteContextFields } from "./RouteContextFields";
import { RoutePreview } from "./RoutePreview";
import { RouteReadinessPanel } from "./RouteReadinessPanel";

const EMPTY_ROUTE: RouteDefinition = {
  router_role: "",
  branches: [],
  graph_edges: [],
  context_fields: [],
  trace_mode: "compact",
};

const CONDITION_OPERATORS = [
  { value: "eq", label: "equals" },
  { value: "neq", label: "does not equal" },
  { value: "gte", label: "greater than or equal" },
  { value: "lte", label: "less than or equal" },
] as const;

type ConditionOperator = (typeof CONDITION_OPERATORS)[number]["value"];

type EditableCondition = {
  operator: ConditionOperator;
  field: string;
  value: string;
};

export function RouteBuilderCard({
  strategyId,
  attached,
  pipeline,
  agentById,
}: {
  strategyId: string;
  attached: AgentRef[];
  pipeline: PipelineDef;
  agentById: Map<string, Agent>;
}) {
  const queryClient = useQueryClient();
  const initialRoute = useMemo(
    () => routeFromPipeline(pipeline, attached),
    [pipeline, attached],
  );
  const [route, setRoute] = useState<RouteDefinition>(initialRoute);
  const [advancedOpen, setAdvancedOpen] = useState(false);
  const [jsonText, setJsonText] = useState(() => JSON.stringify(initialRoute));
  const [jsonError, setJsonError] = useState<string | null>(null);
  const [validatedJsonRoute, setValidatedJsonRoute] = useState<RouteDefinition | null>(null);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [readiness, setReadiness] = useState<RouteReadiness | null>(null);

  useEffect(() => {
    setRoute(initialRoute);
    setJsonText(JSON.stringify(initialRoute));
    setJsonError(null);
    setValidatedJsonRoute(null);
    setSaveError(null);
    setReadiness(null);
  }, [initialRoute]);

  const routerOptions = useMemo(
    () => attached.filter(isRouterRef).map((ref) => routeOption(ref, agentById)),
    [attached, agentById],
  );
  const targetOptions = useMemo(
    () => attached.filter(isTraderTargetRef).map((ref) => ({
      value: ref.role,
      label: displayRole(ref.role),
    })),
    [attached],
  );
  const graphSourceOptions = useMemo(() => {
    const selectedRouter = routerOptions.find((option) => option.value === route.router_role);
    return selectedRouter ? [selectedRouter] : [];
  }, [routerOptions, route.router_role]);
  const graphTargetOptions = useMemo(
    () => attached.map((ref) => routeOption(ref, agentById)),
    [attached, agentById],
  );

  const unsupportedGraphMessage = unsupportedGraphCopy(pipeline);
  const disabledCopy = readinessCopy({
    attached,
    routerOptions,
    targetOptions,
    route,
    unsupportedGraphMessage,
  });

  const saveMut = useMutation({
    mutationFn: (body: RouteDefinition) => setStrategyRoute(strategyId, body),
    onMutate: () => {
      setSaveError(null);
    },
    onSuccess: async (out) => {
      setReadiness(out.readiness);
      await invalidateStrategy(queryClient, strategyId);
    },
    onError: async (error) => {
      setSaveError(errorMessage(error));
      await invalidateStrategy(queryClient, strategyId);
    },
  });
  const saveDisabled = Boolean(disabledCopy) || saveMut.isPending;

  const validateMut = useMutation({
    mutationFn: (body: RouteDefinition) => validateStrategyRoute(strategyId, body),
    onSuccess: (out, body) => {
      setReadiness(out.readiness);
      setValidatedJsonRoute(body);
      setJsonError(null);
    },
    onError: (error) => {
      setJsonError(errorMessage(error));
      setValidatedJsonRoute(null);
    },
  });

  function updateRoute(next: RouteDefinition) {
    setRoute(next);
    if (!advancedOpen) {
      setJsonText(JSON.stringify(next, null, 2));
      setValidatedJsonRoute(null);
    }
  }

  function setRouter(routerRole: string) {
    updateRoute({
      ...route,
      router_role: routerRole,
      graph_edges: (route.graph_edges ?? []).map((edge) => ({
        ...edge,
        from_role: routerRole,
      })),
    });
  }

  function setBranches(targetRoles: string[]) {
    updateRoute({
      ...route,
      branches: targetRoles.map((target_role) => ({ target_role })),
    });
  }

  function addGraphEdge() {
    const target = route.branches[0]?.target_role ?? targetOptions[0]?.value ?? "";
    updateRoute({
      ...route,
      graph_edges: [
        ...(route.graph_edges ?? []),
        {
          from_role: route.router_role,
          to_role: target,
          condition: edgeCondition({ operator: "eq", field: "", value: "" }),
        },
      ],
    });
  }

  function updateEdge(index: number, next: RouteGraphEdge) {
    updateRoute({
      ...route,
      graph_edges: (route.graph_edges ?? []).map((edge, i) =>
        i === index ? { ...next, from_role: route.router_role || next.from_role } : edge,
      ),
    });
  }

  function setContextFields(context_fields: RouteContextField[]) {
    updateRoute({ ...route, context_fields });
  }

  function setTraceMode(trace_mode: RouteTraceMode) {
    updateRoute({ ...route, trace_mode });
  }

  function parseJsonRoute(): RouteDefinition | null {
    try {
      const parsed = JSON.parse(jsonText) as RouteDefinition;
      const normalized = normalizeRoute(parsed);
      setJsonError(null);
      return normalized;
    } catch (error) {
      setJsonError(errorMessage(error));
      setValidatedJsonRoute(null);
      return null;
    }
  }

  function validateJson() {
    const parsed = parseJsonRoute();
    if (parsed) validateMut.mutate(parsed);
  }

  function saveJson() {
    const parsed = validatedJsonRoute ?? parseJsonRoute();
    if (!parsed) return;
    saveMut.mutate(parsed);
  }

  return (
    <Card>
      <header className="border-b border-border-soft px-5 pb-3 pt-4">
        <div className="flex flex-wrap items-start justify-between gap-3">
          <div>
            <div className="text-[12px] uppercase tracking-wide text-text-3">
              Route Builder
            </div>
            <div className="mt-0.5 text-[12px] text-text-2">
              Choose a router, branch targets, context, and conditioned graph
              routes in product language.
            </div>
          </div>
          <button
            type="button"
            onClick={() => setAdvancedOpen((open) => !open)}
            className="inline-flex h-8 items-center rounded-sm border border-border px-2.5 text-[12.5px] text-text transition-colors hover:border-text-3 focus:outline-none focus-visible:ring-1 focus-visible:ring-gold/45"
          >
            Advanced JSON
          </button>
        </div>
      </header>
      <div className="space-y-4 px-5 pb-5 pt-4">
        <RouteReadinessPanel
          message={disabledCopy}
          readiness={readiness}
          error={saveError}
        />

        <div className="grid grid-cols-1 gap-3 lg:grid-cols-[minmax(0,1.15fr)_minmax(320px,0.85fr)]">
          <div className="space-y-4">
            <div className="rounded border border-border-soft bg-surface-card p-3">
              <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
                <RouteField label="Router" hint="AgentRef that chooses the router path.">
                  <SignalSearchableSelectMenu
                    ariaLabel="Router"
                    value={route.router_role}
                    options={routerOptions}
                    onChange={setRouter}
                    placeholder="Select router"
                    emptyHint="No router-capable attached roles"
                    className="w-full justify-between"
                  />
                </RouteField>
                <RouteField label="Branch targets" hint="Trader targets available for branch selection.">
                  <SignalCheckboxMenu
                    label="Branch targets"
                    selected={route.branches.map((branch) => branch.target_role)}
                    options={targetOptions}
                    onChange={setBranches}
                    onClear={() => setBranches([])}
                    triggerLabel={route.branches.length ? "Selected" : "Choose targets"}
                  />
                </RouteField>
              </div>
            </div>

            <div className="space-y-3 rounded border border-border-soft bg-surface-card p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <div>
                  <div className="text-[12px] uppercase tracking-wide text-text-3">
                    Graph routes
                  </div>
                  <div className="text-[12px] text-text-2">
                    Optional forward conditioned routes into a branch target.
                  </div>
                </div>
                <button
                  type="button"
                  onClick={addGraphEdge}
                  disabled={Boolean(unsupportedGraphMessage) || !route.router_role}
                  className="inline-flex h-8 items-center rounded-sm border border-border px-2.5 text-[12.5px] text-text transition-colors hover:border-text-3 focus:outline-none focus-visible:ring-1 focus-visible:ring-gold/45 disabled:cursor-not-allowed disabled:opacity-50"
                >
                  Add graph route
                </button>
              </div>
              {unsupportedGraphMessage ? null : null}
              {(route.graph_edges ?? []).length === 0 ? (
                <p className="m-0 text-[12px] text-text-3">
                  No conditioned graph routes yet. The router can still choose a
                  branch target directly.
                </p>
              ) : (
                <div className="space-y-3">
                  {(route.graph_edges ?? []).map((edge, index) => (
                    <GraphRouteRow
                      key={index}
                      edge={edge}
                      index={index}
                      sourceOptions={graphSourceOptions}
                      targetOptions={graphTargetOptions}
                      onChange={(next) => updateEdge(index, next)}
                    />
                  ))}
                </div>
              )}
            </div>

            <RouteContextFields
              selected={route.context_fields}
              onChange={setContextFields}
            />

            <RouteField label="Trace mode" hint="Compact traces summarize router choices; full traces keep richer route spans.">
              <SignalSelectMenu
                ariaLabel="Trace mode"
                value={route.trace_mode}
                options={[
                  { value: "compact", label: "compact" },
                  { value: "full", label: "full" },
                ]}
                onChange={(value) => setTraceMode(value as RouteTraceMode)}
              />
            </RouteField>

            <div className="flex flex-wrap items-center gap-3">
              <button
                type="button"
                onClick={() => saveMut.mutate(routeWithSelectedRouterSources(route))}
                disabled={saveDisabled}
                className="inline-flex items-center gap-2 rounded bg-gold px-3.5 py-2 text-[13px] font-medium text-bg transition-colors hover:bg-gold-soft focus:outline-none focus-visible:ring-1 focus-visible:ring-gold/45 disabled:cursor-not-allowed disabled:opacity-50"
              >
                {saveMut.isPending ? "Saving route…" : "Save route"}
              </button>
              <span className="text-[12px] text-text-3">
                Saved routes share the same readiness gate across launch modes.
              </span>
            </div>
          </div>

          <RoutePreview route={routeWithSelectedRouterSources(route)} pipeline={pipeline} />
        </div>

        {advancedOpen ? (
          <section className="space-y-3 rounded border border-border-soft bg-surface-card p-3">
            <div>
              <div className="text-[12px] uppercase tracking-wide text-text-3">
                Advanced JSON
              </div>
              <div className="text-[12px] text-text-2">
                Validate pasted route JSON before saving. Guided controls remain
                the default authoring surface.
              </div>
            </div>
            <label className="block">
              <span className="mb-1 block text-[12px] text-text-2">Route JSON</span>
              <textarea
                aria-label="Route JSON"
                value={jsonText}
                onChange={(event) => {
                  setJsonText(event.target.value);
                  setValidatedJsonRoute(null);
                }}
                spellCheck={false}
                className="min-h-[220px] w-full rounded border border-border bg-surface-elev px-3 py-2 font-mono text-[12px] text-text outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45"
              />
            </label>
            {jsonError ? <div className="text-[13px] text-danger">{jsonError}</div> : null}
            <div className="flex flex-wrap items-center gap-2">
              <button
                type="button"
                onClick={validateJson}
                disabled={validateMut.isPending}
                className="inline-flex h-8 items-center rounded-sm border border-border px-2.5 text-[12.5px] text-text transition-colors hover:border-text-3 disabled:opacity-50"
              >
                {validateMut.isPending ? "Validating…" : "Validate JSON"}
              </button>
              <button
                type="button"
                onClick={saveJson}
                disabled={saveMut.isPending || Boolean(jsonError)}
                className="inline-flex h-8 items-center rounded-sm border border-border px-2.5 text-[12.5px] text-text transition-colors hover:border-text-3 disabled:opacity-50"
              >
                Save JSON route
              </button>
            </div>
          </section>
        ) : null}
      </div>
    </Card>
  );
}

function RouteField({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: ReactNode;
}) {
  return (
    <label className="block">
      <span className="mb-1 block text-[12px] text-text-2">{label}</span>
      {children}
      {hint ? <span className="mt-1 block text-[11.5px] text-text-3">{hint}</span> : null}
    </label>
  );
}

function GraphRouteRow({
  edge,
  index,
  sourceOptions,
  targetOptions,
  onChange,
}: {
  edge: RouteGraphEdge;
  index: number;
  sourceOptions: SearchableSelectOption[];
  targetOptions: SearchableSelectOption[];
  onChange: (edge: RouteGraphEdge) => void;
}) {
  const editable = editableCondition(edge);
  function setCondition(next: EditableCondition) {
    onChange({ ...edge, condition: edgeCondition(next) });
  }

  return (
    <div className="rounded border border-border-soft bg-surface-elev p-3">
      <div className="mb-2 font-mono text-[11px] uppercase tracking-wide text-text-3">
        Route {index + 1}
      </div>
      <div className="grid grid-cols-1 gap-3 md:grid-cols-2">
        <RouteField label="Route source">
          <SignalSearchableSelectMenu
            ariaLabel="Route source"
            value={edge.from_role}
            options={sourceOptions}
            onChange={(value) => onChange({ ...edge, from_role: value })}
            placeholder="Source role"
            className="w-full justify-between"
          />
        </RouteField>
        <RouteField label="Route target">
          <SignalSearchableSelectMenu
            ariaLabel="Route target"
            value={edge.to_role}
            options={targetOptions}
            onChange={(value) => onChange({ ...edge, to_role: value })}
            placeholder="Target role"
            className="w-full justify-between"
          />
        </RouteField>
      </div>
      <div className="mt-3 grid grid-cols-1 gap-3 md:grid-cols-[minmax(0,1fr)_180px_minmax(0,1fr)]">
        <RouteField label="Condition field">
          <input
            aria-label="Condition field"
            value={editable.field}
            onChange={(event) => setCondition({ ...editable, field: event.target.value })}
            className="h-8 w-full rounded-sm border border-border bg-surface-card px-2.5 font-mono text-[12px] text-text outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45"
          />
        </RouteField>
        <RouteField label="Condition operator">
          <SignalSelectMenu
            ariaLabel="Condition operator"
            value={editable.operator}
            options={CONDITION_OPERATORS}
            onChange={(value) => setCondition({ ...editable, operator: value as ConditionOperator })}
            className="w-full justify-between"
          />
        </RouteField>
        <RouteField label="Condition value">
          <input
            aria-label="Condition value"
            value={editable.value}
            onChange={(event) => setCondition({ ...editable, value: event.target.value })}
            className="h-8 w-full rounded-sm border border-border bg-surface-card px-2.5 font-mono text-[12px] text-text outline-none focus:border-gold/45 focus-visible:ring-1 focus-visible:ring-gold/45"
          />
        </RouteField>
      </div>
    </div>
  );
}

function routeFromPipeline(pipeline: PipelineDef, attached: AgentRef[]): RouteDefinition {
  if (pipeline.route) return normalizeRoute(pipeline.route);
  const router = attached.find(isRouterRef)?.role ?? "";
  return {
    ...EMPTY_ROUTE,
    router_role: router,
    graph_edges: (pipeline.edges ?? []).map((edge) => ({
      from_role: edge.from_role,
      to_role: edge.to_role,
      condition: edge.condition ?? undefined,
    })),
  };
}

function normalizeRoute(route: RouteDefinition): RouteDefinition {
  if (!route || typeof route !== "object") {
    throw new Error("Route JSON must be an object.");
  }
  return {
    router_role: String(route.router_role ?? ""),
    branches: Array.isArray(route.branches) ? route.branches : [],
    graph_edges: Array.isArray(route.graph_edges) ? route.graph_edges : [],
    context_fields: Array.isArray(route.context_fields) ? route.context_fields : [],
    trace_mode: route.trace_mode === "full" ? "full" : "compact",
  };
}

function routeWithSelectedRouterSources(route: RouteDefinition): RouteDefinition {
  if (!route.router_role || (route.graph_edges ?? []).length === 0) return route;
  return {
    ...route,
    graph_edges: (route.graph_edges ?? []).map((edge) => ({
      ...edge,
      from_role: route.router_role,
    })),
  };
}

function routeOption(ref: AgentRef, agentById: Map<string, Agent>): SearchableSelectOption {
  const agent = agentById.get(ref.agent_id);
  const role = displayRole(ref.role);
  return {
    value: ref.role,
    label: agent?.name ?? role,
    meta: ref.role,
    searchText: `${agent?.name ?? ""} ${ref.role} ${ref.agent_id}`,
    badge: ref.activates ?? undefined,
  };
}

function isRouterRef(ref: AgentRef): boolean {
  return ref.activates === "router" || ref.role.toLowerCase().includes("router");
}

function isTraderTargetRef(ref: AgentRef): boolean {
  if (isRouterRef(ref)) return false;
  return ref.activates === "trader" || ref.activates == null || ref.role.toLowerCase().includes("trader");
}

function displayRole(role: string): string {
  return role.replace(/[_-]+/g, " ").replace(/\b\w/g, (char) => char.toUpperCase());
}

function unsupportedGraphCopy(pipeline: PipelineDef): string | null {
  if (pipeline.kind !== "graph" || pipeline.route || (pipeline.edges ?? []).length === 0) return null;
  const unsupported = (pipeline.edges ?? []).some((edge) => !edge.condition);
  return unsupported
    ? "This graph is preserved and exportable. Route Builder can safely edit forward conditioned routes; use JSON import for this unsupported shape."
    : null;
}

function readinessCopy({
  attached,
  routerOptions,
  targetOptions,
  route,
  unsupportedGraphMessage,
}: {
  attached: AgentRef[];
  routerOptions: SearchableSelectOption[];
  targetOptions: Array<{ value: string; label: string }>;
  route: RouteDefinition;
  unsupportedGraphMessage: string | null;
}): string | null {
  if (unsupportedGraphMessage) return unsupportedGraphMessage;
  if (attached.length === 0) {
    return "Attach a router and at least one downstream trader target before building a route.";
  }
  if (attached.length === 1) {
    return "Routing needs a router and at least one branch target.";
  }
  if (routerOptions.length === 0) {
    return "Choose an agent that can route, or mark this attached role as a Router.";
  }
  if (targetOptions.length === 0) {
    return "Every route must reach at least one trader target before it can launch.";
  }
  if (!route.router_role || route.branches.length === 0) {
    return "Choose a router and at least one branch target before saving the route.";
  }
  return null;
}

function editableCondition(edge: RouteGraphEdge): EditableCondition {
  const condition = edge.condition;
  if (condition && "neq" in condition) {
    return {
      operator: "neq",
      field: condition.neq.signal_field,
      value: String(condition.neq.value ?? ""),
    };
  }
  if (condition && "gte" in condition) {
    return {
      operator: "gte",
      field: condition.gte.signal_field,
      value: String(condition.gte.value ?? ""),
    };
  }
  if (condition && "lte" in condition) {
    return {
      operator: "lte",
      field: condition.lte.signal_field,
      value: String(condition.lte.value ?? ""),
    };
  }
  if (condition && "eq" in condition) {
    return {
      operator: "eq",
      field: condition.eq.signal_field,
      value: String(condition.eq.value ?? ""),
    };
  }
  return { operator: "eq", field: "", value: "" };
}

function edgeCondition(condition: EditableCondition): RouteGraphEdge["condition"] {
  const field = condition.field.trim();
  const value = condition.value.trim();
  if (condition.operator === "neq") return { neq: { signal_field: field, value } };
  if (condition.operator === "gte") return { gte: { signal_field: field, value } };
  if (condition.operator === "lte") return { lte: { signal_field: field, value } };
  return { eq: { signal_field: field, value } };
}

async function invalidateStrategy(
  queryClient: ReturnType<typeof useQueryClient>,
  strategyId: string,
) {
  await queryClient.invalidateQueries({ queryKey: strategyKeys.detail(strategyId) });
  await queryClient.invalidateQueries({ queryKey: strategyKeys.validate(strategyId) });
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return String(error);
}
