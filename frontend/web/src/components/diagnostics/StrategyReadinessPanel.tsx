import { useQuery } from "@tanstack/react-query";

import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  diagnosticsKeys,
  getStrategyDiagnostics,
  type StrategyAgentDiagnostics,
  type StrategyDiagnostics,
} from "@/api/diagnostics";

export function StrategyReadinessPanel({
  strategyId,
  className = "",
}: {
  strategyId: string;
  className?: string;
}) {
  const q = useQuery<StrategyDiagnostics, ApiError>({
    queryKey: diagnosticsKeys.strategy(strategyId),
    queryFn: () => getStrategyDiagnostics(strategyId),
    enabled: strategyId.length > 0,
  });

  if (q.isPending) {
    return <Card className={`px-5 py-6 text-[13px] text-text-3 ${className}`}>Loading readiness...</Card>;
  }
  if (q.isError || !q.data) {
    return (
      <Card className={`px-5 py-6 text-[13px] text-danger ${className}`} role="alert">
        {q.error instanceof ApiError ? q.error.message : "Could not load readiness."}
      </Card>
    );
  }

  const d = q.data;
  return (
    <div className={`flex flex-col gap-4 ${className}`} data-testid="strategy-readiness">
      <Card className="px-5 py-4">
        <div className="flex flex-wrap items-center gap-3">
          <Pill tone={d.launchable ? "info" : "danger"}>
            {d.launchable ? "Ready to launch" : "Cannot launch"}
          </Pill>
          <span className="text-[13px] text-text-2">
            {d.has_decision_path
              ? "At least one slot can submit decisions."
              : "No slot grants submit_decision."}
          </span>
        </div>
        {d.unregistered_tools.length > 0 ? (
          <ul className="mt-3 flex flex-col gap-2 m-0 p-0 list-none">
            {d.unregistered_tools.map((u) => (
              <li key={`${u.role}-${u.tool}`} className="rounded border border-danger/30 bg-danger/5 px-3 py-2">
                <span className="text-[13px] font-medium text-text">{u.role}</span>
                <span className="ml-2 font-mono text-[12px] text-text-2">{u.tool}</span>
              </li>
            ))}
          </ul>
        ) : null}
      </Card>

      {(d.per_agent ?? []).map((agent, i) => (
        <AgentReadinessCard key={`${agent.role}-${agent.agent_id}-${i}`} agent={agent} />
      ))}
    </div>
  );
}

function AgentReadinessCard({ agent }: { agent: StrategyAgentDiagnostics }) {
  return (
    <Card className="px-5 py-4" data-testid={`agent-readiness-${agent.role}`}>
      <div className="flex flex-wrap items-center justify-between gap-2 mb-3">
        <div className="flex items-center gap-2">
          <h3 className="m-0 text-[14px] font-medium text-text">{agent.role}</h3>
          <span className="text-[12px] text-text-3">
            {agent.agent_name ?? agent.agent_id}
          </span>
        </div>
        {agent.agent_resolved ? null : <Pill tone="danger">agent missing</Pill>}
      </div>
      <ul className="flex flex-col gap-2 m-0 p-0 list-none">
        {agent.tools.map((tool) => (
          <li key={tool.name} className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-[13px] text-text">{tool.name}</span>
            <Pill tone={tool.registered ? "info" : "danger"}>
              {tool.registered ? "registered" : "unregistered"}
            </Pill>
            {tool.description ? (
              <span className="text-[12px] text-text-3">{tool.description}</span>
            ) : null}
          </li>
        ))}
      </ul>
    </Card>
  );
}
