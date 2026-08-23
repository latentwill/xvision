import type { PipelineDef, PipelineEdge, RouteDefinition, RouteGraphEdge } from "@/api/strategies";
import { routeContextLabel } from "./RouteContextFields";

function branchLabel(role: string): string {
  const base = role.replace(/_?trader$/i, "") || role;
  return base
    .replace(/[_-]+/g, " ")
    .trim()
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function conditionLabel(edge: RouteGraphEdge): string {
  const condition = edge.condition;
  if (!condition) return "always";
  if ("eq" in condition) {
    return `${condition.eq.signal_field} = ${String(condition.eq.value)}`;
  }
  if ("neq" in condition) {
    return `${condition.neq.signal_field} != ${String(condition.neq.value)}`;
  }
  if ("gte" in condition) {
    return `${condition.gte.signal_field} >= ${String(condition.gte.value)}`;
  }
  if ("lte" in condition) {
    return `${condition.lte.signal_field} <= ${String(condition.lte.value)}`;
  }
  if ("in" in condition) {
    return `${condition.in.signal_field} in ${condition.in.values.map(String).join(", ")}`;
  }
  return "compound condition";
}

function branchPathLabel(targetRole: string, downstreamEdges: PipelineEdge[]): string {
  const path = downstreamPath(targetRole, downstreamEdges);
  return `${branchLabel(targetRole)} → ${path.join(" → ")}`;
}

function downstreamPath(targetRole: string, downstreamEdges: PipelineEdge[]): string[] {
  const path = [targetRole];
  const visited = new Set(path);
  let current = targetRole;

  while (true) {
    const next = downstreamEdges.find((edge) => edge.from_role === current)?.to_role;
    if (!next || visited.has(next)) return path;
    path.push(next);
    visited.add(next);
    current = next;
  }
}

export function RoutePreview({
  route,
  pipeline,
}: {
  route: RouteDefinition;
  pipeline: PipelineDef;
}) {
  const context = route.context_fields.map(routeContextLabel);
  const visibleContext = context.length > 0 ? context.join(", ") : "available targets";
  const downstreamEdges = pipeline.edges ?? [];
  return (
    <section className="rounded border border-border-soft bg-surface-elev px-3 py-3">
      <div className="mb-2 text-[12px] uppercase tracking-wide text-text-3">
        Selected path
      </div>
      <div className="space-y-2 text-[13px] leading-relaxed text-text-2">
        <div>{`Router: ${route.router_role || "—"}`}</div>
        <div>
          <div className="text-[12px] uppercase tracking-wide text-text-3">
            Branches
          </div>
          <ul className="mt-1 space-y-1">
            {route.branches.length === 0 ? (
              <li className="text-text-3">No branch targets selected.</li>
            ) : (
              route.branches.map((branch) => (
                <li key={branch.target_role}>
                  {branchPathLabel(branch.target_role, downstreamEdges)}
                </li>
              ))
            )}
          </ul>
        </div>
        <div>{`Router can see: ${visibleContext}`}</div>
        <div>
          Router cannot see: credentials, broker secrets, unselected tools,
          hidden memory, or fields not checked above.
        </div>
        <div>
          <div className="text-[12px] uppercase tracking-wide text-text-3">
            Graph routes
          </div>
          <ul className="mt-1 space-y-1 font-mono text-[12px] text-text">
            {(route.graph_edges ?? []).length === 0 ? (
              <li className="font-sans text-text-3">No conditioned graph routes.</li>
            ) : (
              (route.graph_edges ?? []).map((edge, index) => (
                <li key={`${edge.from_role}:${edge.to_role}:${index}`}>
                  {edge.from_role} -- {conditionLabel(edge)} --&gt; {edge.to_role}
                </li>
              ))
            )}
          </ul>
        </div>
        <div>{`Trace: ${route.trace_mode}`}</div>
        <div className="rounded border border-border-soft bg-surface-card px-2 py-2 text-[12px] text-text-2">
          <span className="font-medium text-text">Actual graph semantics:</span> The
          preview shows which router path may run. Forward test and live trading
          use the same route readiness contract before launch.
        </div>
      </div>
    </section>
  );
}
