import { useQuery } from "@tanstack/react-query";

import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  diagnosticsKeys,
  getAgentDiagnostics,
  type AgentDiagnostics,
  type AgentSlotDiagnostics,
} from "@/api/diagnostics";

export function AgentDiagnosticsView({ agentId }: { agentId: string }) {
  const q = useQuery<AgentDiagnostics, ApiError>({
    queryKey: diagnosticsKeys.agent(agentId),
    queryFn: () => getAgentDiagnostics(agentId),
    enabled: agentId.length > 0,
  });

  if (q.isPending) {
    return <Card className="px-5 py-6 text-[13px] text-text-3">Loading diagnostics...</Card>;
  }
  if (q.isError || !q.data) {
    return (
      <Card className="px-5 py-6 text-[13px] text-danger" role="alert">
        {q.error instanceof ApiError ? q.error.message : "Could not load diagnostics."}
      </Card>
    );
  }

  const d = q.data;
  return (
    <div className="flex flex-col gap-4" data-testid="agent-diagnostics">
      <Card className="px-5 py-4">
        <div className="flex flex-wrap items-center gap-3">
          <Pill tone={d.agent_ready ? "info" : "danger"}>
            {d.agent_ready ? "Agent ready" : "Not ready"}
          </Pill>
          <span className="text-[13px] text-text-2">
            {d.agent_ready
              ? "Prompt, model binding, and tool registrations are ready."
              : "One or more slots need a prompt, model, or registered tool."}
          </span>
        </div>
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
        <h3 className="m-0 text-[14px] font-medium text-text">{slot.slot_name}</h3>
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
        {slot.tools.map((tool) => (
          <li
            key={tool.name}
            className="flex flex-wrap items-center gap-2 border-b border-border-soft pb-2 last:border-b-0 last:pb-0"
          >
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
