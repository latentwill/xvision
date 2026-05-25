// Mobile-safe READ-ONLY diagnostics routes.
//
// `/agents/:id/diagnostics` and `/strategies/:id/diagnostics` render the
// agent / strategy diagnostics surfaces as standalone, full-width,
// read-only pages. The underlying views (`AgentDiagnosticsView`,
// `StrategyReadinessPanel`) are card stacks that collapse cleanly to a
// single column on a phone, so these routes are the deep-linkable,
// share-friendly read-only face of the diagnostics data — no editing
// affordances, no popups. Deep-links here let an operator open just the
// readiness verdict on a small screen without loading the full editor.

import { useParams, Navigate, Link } from "react-router-dom";

import { Topbar } from "@/components/shell/Topbar";
import { AgentDiagnosticsView } from "@/components/diagnostics/AgentDiagnosticsView";
import { StrategyReadinessPanel } from "@/components/diagnostics/StrategyReadinessPanel";

export function AgentDiagnosticsRoute() {
  const { id } = useParams<{ id: string }>();
  if (!id) return <Navigate to="/agents" replace />;
  return (
    <div
      className="mx-auto w-full max-w-2xl px-3 sm:px-0"
      data-testid="agent-diagnostics-route"
    >
      <Topbar
        title="Agent diagnostics"
        sub="Read-only capability readiness"
        back={{ to: `/agents/${encodeURIComponent(id)}`, label: "Back to agent" }}
      />
      <AgentDiagnosticsView agentId={id} />
      <div className="mt-4 text-[12px] text-text-3">
        <Link
          to={`/agents/${encodeURIComponent(id)}?tab=diagnostics`}
          className="hover:text-text underline-offset-2 hover:underline"
        >
          Open in agent editor →
        </Link>
      </div>
    </div>
  );
}

export function StrategyDiagnosticsRoute() {
  const { id } = useParams<{ id: string }>();
  if (!id) return <Navigate to="/strategies" replace />;
  return (
    <div
      className="mx-auto w-full max-w-2xl px-3 sm:px-0"
      data-testid="strategy-diagnostics-route"
    >
      <Topbar
        title="Strategy readiness"
        sub="Read-only launch readiness"
        back={{
          to: `/strategies/${encodeURIComponent(id)}`,
          label: "Back to strategy",
        }}
      />
      <StrategyReadinessPanel strategyId={id} />
    </div>
  );
}
