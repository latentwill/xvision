// StrategyReadinessPanel — agent-readiness panel for a strategy detail page.
//
// Renders the dashboard's `GET /api/strategy/:id/diagnostics` payload. It
// answers WHY a strategy can't launch BEFORE launching: a single
// `launchable` verdict up top, then the per-agent capability breakdown with
// every unmet REQUIRED capability surfaced as a typed blocker + remediation.
// Read-only, no popups; the cards stack on a phone.
//
// This is the launch gate's UI face — the operator should see the unmet
// requirements here rather than discovering them when a launch fails.

import { useQuery } from "@tanstack/react-query";

import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { agentKeys, listAgents } from "@/api/agents";
import {
  diagnosticsKeys,
  getStrategyDiagnostics,
  isBlocker,
  remediationFor,
  type StrategyAgentDiagnostics,
  type StrategyDiagnostics,
} from "@/api/diagnostics";
import { CapabilityStatusBadge } from "./CapabilityStatusBadge";

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
  // QA31: surface each agent's primary slot model alongside the
  // readiness rows. The diagnostics endpoint doesn't carry the model
  // (it's strictly about capability satisfaction), so we fetch the
  // workspace agent list — cached aggressively because names + models
  // rarely change — and project a (agent_id → model display) map.
  const agentsQ = useQuery({
    queryKey: agentKeys.list(undefined),
    queryFn: () => listAgents(),
    staleTime: 60_000,
  });
  const agentModelById = new Map<string, string>();
  const agentProviderById = new Map<string, string>();
  for (const a of agentsQ.data ?? []) {
    const firstSlot = a.slots[0];
    if (firstSlot?.model) agentModelById.set(a.agent_id, firstSlot.model);
    if (firstSlot?.provider) agentProviderById.set(a.agent_id, firstSlot.provider);
  }

  if (q.isPending) {
    return (
      <Card
        className={`px-5 py-6 text-[13px] text-text-3 ${className}`}
        data-testid="strategy-readiness-loading"
      >
        Loading readiness…
      </Card>
    );
  }
  if (q.isError || !q.data) {
    return (
      <Card
        className={`px-5 py-6 text-[13px] text-danger ${className}`}
        data-testid="strategy-readiness-error"
        role="alert"
      >
        {q.error instanceof ApiError
          ? q.error.message
          : "Could not load readiness."}
      </Card>
    );
  }

  const d = q.data;

  return (
    <div
      className={`flex flex-col gap-4 ${className}`}
      data-testid="strategy-readiness"
    >
      <Card className="px-5 py-4">
        <div className="flex flex-wrap items-center gap-3">
          {d.launchable ? (
            <Pill tone="info" data-testid="strategy-launchable">
              Ready to launch
            </Pill>
          ) : (
            <Pill tone="danger" data-testid="strategy-not-launchable">
              Cannot launch
            </Pill>
          )}
          <span className="text-[13px] text-text-2">
            {d.launchable
              ? "Every required capability in this strategy's pipeline is satisfied."
              : `${d.required_unmet.length} required capabilit${
                  d.required_unmet.length === 1 ? "y is" : "ies are"
                } unmet — resolve them before launching.`}
          </span>
        </div>

        {d.required_unmet.length > 0 ? (
          <ul
            className="mt-3 flex flex-col gap-2 m-0 p-0 list-none"
            data-testid="strategy-unmet"
          >
            {d.required_unmet.map((u, i) => (
              <li
                key={`${u.role}-${u.capability}-${i}`}
                className="rounded border border-danger/30 bg-danger/5 dark:bg-danger/10 px-3 py-2"
                data-testid={`unmet-${u.capability}`}
              >
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[13px] font-medium text-text">
                    {u.role}
                  </span>
                  <span className="text-[12px] text-text-3 capitalize">
                    {u.capability}
                  </span>
                  <CapabilityStatusBadge status={u.status} />
                </div>
                <div className="text-[12px] text-text-2 mt-1">
                  {remediationFor(u.status, u.capability)}
                </div>
              </li>
            ))}
          </ul>
        ) : null}

        {d.optimizable.length > 0 ? (
          <div className="mt-3 text-[12px] text-text-3">
            Optimizable now:{" "}
            <span className="text-text-2">{d.optimizable.join(", ")}</span>.
          </div>
        ) : null}
      </Card>

      {d.per_agent.map((a, i) => (
        <AgentReadinessCard
          key={`${a.role}-${a.agent_id}-${i}`}
          agent={a}
          model={agentModelById.get(a.agent_id) ?? null}
          provider={agentProviderById.get(a.agent_id) ?? null}
        />
      ))}
    </div>
  );
}

function AgentReadinessCard({
  agent,
  model,
  provider,
}: {
  agent: StrategyAgentDiagnostics;
  model: string | null;
  provider: string | null;
}) {
  return (
    <Card
      className="px-5 py-4"
      data-testid={`agent-readiness-${agent.role}`}
    >
      <div className="flex flex-wrap items-center justify-between gap-2 mb-3">
        <div className="flex items-center gap-2">
          <h3 className="m-0 text-[14px] font-medium text-text">
            {agent.role}
          </h3>
          <span className="text-[12px] text-text-3">
            {agent.agent_name ?? agent.agent_id}
          </span>
        </div>
        {agent.agent_resolved ? null : (
          <Pill tone="danger" data-testid="agent-unresolved">
            agent missing
          </Pill>
        )}
      </div>
      {/*
        QA31: surface the agent's bound model as a first-class row at the
        top of the card. Previously the model lived only inside the
        agent editor — operators had to navigate away to see what the
        strategy was actually about to run. The "MODEL" pill style
        deliberately stands out so it's the first thing the eye lands
        on when scanning the readiness section.
      */}
      {model ? (
        <div
          className="mb-3 flex items-center gap-2 rounded border border-gold/40 bg-gold/[0.06] px-3 py-2"
          data-testid={`agent-model-${agent.role}`}
        >
          <span className="text-[10px] uppercase tracking-[0.14em] font-semibold text-gold">
            Model
          </span>
          <span className="text-[13px] font-mono text-text">{model}</span>
          {provider ? (
            <span className="text-[11px] text-text-3">· {provider}</span>
          ) : null}
        </div>
      ) : null}
      <ul className="flex flex-col gap-2 m-0 p-0 list-none">
        {agent.capabilities.map((line) => {
          const blocked = line.required && isBlocker(line.status);
          return (
            <li
              key={line.capability}
              className="flex flex-col gap-1 border-b border-border-soft pb-2 last:border-b-0 last:pb-0"
              data-testid={`strat-cap-line-${line.capability}`}
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-[13px] font-medium text-text capitalize">
                  {line.capability}
                </span>
                {line.required ? (
                  <span className="text-[10px] uppercase tracking-wide text-text-3">
                    required
                  </span>
                ) : null}
                <CapabilityStatusBadge status={line.status} />
                {line.required_tools.length > 0 ? (
                  <span className="text-[11px] text-text-3">
                    needs: {line.required_tools.join(", ")}
                  </span>
                ) : null}
              </div>
              {blocked ? (
                <span className="text-[12px] text-text-2">
                  {remediationFor(line.status, line.capability)}
                </span>
              ) : null}
            </li>
          );
        })}
      </ul>
    </Card>
  );
}
