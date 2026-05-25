import { Navigate, useParams } from "react-router-dom";

import { FlywheelPanel } from "@/features/memory/MemorySurface";
import { Topbar } from "@/components/shell/Topbar";

export function AgentsFlywheelRoute() {
  const { id } = useParams<{ id: string }>();
  if (!id) {
    return <Navigate to="/agents" replace />;
  }

  return (
    <>
      <Topbar
        title="Flywheel"
        sub={id}
        back={{ to: `/agents/${encodeURIComponent(id)}`, label: "Back to agent" }}
      />
      <FlywheelPanel mode="agent" agentId={id} fullHistory />
    </>
  );
}
