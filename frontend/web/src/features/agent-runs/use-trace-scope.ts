// frontend/web/src/features/agent-runs/use-trace-scope.ts
import { useLocation } from "react-router-dom";
import type { TraceScope } from "@/stores/trace-dock";

/**
 * Map a router pathname onto its {@link TraceScope}.
 *
 * Pure + exported so the mapping is unit-testable without a Router.
 * Rule (locked by WS-2, extended by WS-11a): anything under `/live` is the
 * live surface; anything under `/optimizer` is the `opti` surface (the
 * autooptimizer cycle trace); everything else — including `/eval-runs/*`
 * and the standalone `/agent-runs/:runId` — is the eval surface. The floating
 * capsule and the trace dock derive which per-scope run slice to read from
 * this.
 */
export function scopeForPath(pathname: string): TraceScope {
  if (pathname.startsWith("/live")) return "live";
  if (pathname.startsWith("/optimizer")) return "opti";
  return "eval";
}

/**
 * The {@link TraceScope} for the current route. Shell components
 * (StripDockSlot / TraceDock / SpanInspector) call this to pick the
 * per-scope dock slice, so the capsule only ever renders for the
 * surface the operator is actually looking at.
 */
export function useCurrentTraceScope(): TraceScope {
  const { pathname } = useLocation();
  return scopeForPath(pathname);
}
