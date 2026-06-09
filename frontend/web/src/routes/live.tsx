import { useParams } from "react-router-dom";

import { LiveCockpit } from "@/features/live/LiveCockpit";

/**
 * `/live` and `/live/:id` both render the Live Trading cockpit
 * (`LiveCockpit`). With no `:id` the cockpit auto-selects the most
 * recently started live run; with an `:id` it preselects that run.
 *
 * The old `/live` run-list (`live-list.tsx`) was absorbed into the
 * cockpit's strategy strip — it reuses the same `listAgentRuns` polling.
 */
export function LiveRoute() {
  const { id } = useParams();
  return <LiveCockpit runId={id || undefined} />;
}
