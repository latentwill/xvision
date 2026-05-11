// /agents/new and /agents/:id — single route component handling both
// modes (matches the Inspector pattern from /authoring + /authoring/:id).

import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { AgentForm } from "@/components/agent/AgentForm";

export function AgentsEditRoute() {
  const params = useParams<{ id?: string }>();
  const agentId = params.id;
  const isNew = !agentId;

  return (
    <>
      <Topbar
        title={isNew ? "New agent" : "Agent"}
        sub={isNew ? "Single-slot draft" : agentId}
      />
      <AgentForm agentId={agentId} />
    </>
  );
}
