import { useParams } from "react-router-dom";

import { LiveConsole } from "@/features/live/LiveConsole";

/**
 * `/fwd` and `/fwd/:id` both render the Forward Test console
 * (`LiveConsole`). With no `:id` the console auto-selects the most
 * recently started forward-test run; with an `:id` it preselects that run.
 *
 * The old `/live` run-list (`live-list.tsx`) was absorbed into the
 * console's strategy strip — it reuses the same `listAgentRuns` polling.
 */
export function LiveRoute() {
  const { id } = useParams();
  return <LiveConsole runId={id || undefined} />;
}
