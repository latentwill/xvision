// AgentDiagnosticsView — read-only per-slot capability diagnostics for one
// agent.
//
// Renders the dashboard's `GET /api/agents/:id/diagnostics` payload: an
// agent-readiness verdict plus, per slot, the typed status of every
// declared capability with remediation copy for the blockers. Pure read —
// no mutations, no popups. The layout is a vertical stack of cards that
// collapses cleanly on a phone (so the mobile read-only view reuses it
// verbatim).

import { useQuery } from "@tanstack/react-query";

import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  diagnosticsKeys,
  getAgentDiagnostics,
  isBlocker,
  remediationFor,
  type AgentDiagnostics,
  type AgentSlotDiagnostics,
} from "@/api/diagnostics";
import { CapabilityStatusBadge } from "./CapabilityStatusBadge";

export function AgentDiagnosticsView({ agentId }: { agentId: string }) {
  const q = useQuery<AgentDiagnostics, ApiError>({
    queryKey: diagnosticsKeys.agent(agentId),
    queryFn: () => getAgentDiagnostics(agentId),
    enabled: agentId.length > 0,
  });

  if (q.isPending) {
    return (
      <Card className="px-5 py-6 text-[13px] text-text-3" data-testid="agent-diag-loading">
        Loading diagnostics…
      </Card>
    );
  }
  if (q.isError || !q.data) {
    return (
      <Card
        className="px-5 py-6 text-[13px] text-danger"
        data-testid="agent-diag-error"
        role="alert"
      >
        {q.error instanceof ApiError
          ? q.error.message
          : "Could not load diagnostics."}
      </Card>
    );
  }

  const d = q.data;

  return (
    <div className="flex flex-col gap-4" data-testid="agent-diagnostics">
      <Card className="px-5 py-4">
        <div className="flex flex-wrap items-center gap-3">
          {d.agent_ready ? (
            <Pill tone="info" data-testid="agent-ready">
              Agent ready
            </Pill>
          ) : (
            <Pill tone="danger" data-testid="agent-not-ready">
              Not ready
            </Pill>
          )}
          <span className="text-[13px] text-text-2">
            {d.agent_ready
              ? "Every declared capability has a prompt + model binding and a runtime handler. A strategy may still need tool grants."
              : "One or more declared capabilities are blocked — see the slots below."}
          </span>
        </div>
        {d.optimizable_capabilities.length > 0 ? (
          <div className="mt-3 text-[12px] text-text-3">
            Optimizable now:{" "}
            <span className="text-text-2">
              {d.optimizable_capabilities.join(", ")}
            </span>{" "}
            — these positions have a dspy optimizer signature.
          </div>
        ) : null}
      </Card>

      {d.slots.map((slot) => (
        <SlotDiagnosticsCard key={slot.slot_name} slot={slot} />
      ))}
    </div>
  );
}

function SlotDiagnosticsCard({ slot }: { slot: AgentSlotDiagnostics }) {
  return (
    <Card className="px-5 py-4" data-testid={`slot-diag-${slot.slot_name}`}>
      <div className="flex flex-wrap items-center justify-between gap-2 mb-3">
        <h3 className="m-0 text-[14px] font-medium text-text">
          {slot.slot_name}
        </h3>
        <div className="flex items-center gap-2">
          <Pill tone={slot.prompt_present ? "info" : "danger"}>
            {slot.prompt_present ? "prompt" : "no prompt"}
          </Pill>
          <Pill tone={slot.model_bound ? "info" : "danger"}>
            {slot.model_bound ? "model bound" : "no model"}
          </Pill>
        </div>
      </div>
      <ul className="flex flex-col gap-2 m-0 p-0 list-none">
        {slot.capabilities.map((line) => {
          const blocked = isBlocker(line.status);
          return (
            <li
              key={line.capability}
              className="flex flex-col gap-1 border-b border-border-soft pb-2 last:border-b-0 last:pb-0"
              data-testid={`cap-line-${line.capability}`}
            >
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-[13px] font-medium text-text capitalize">
                  {line.capability}
                </span>
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
