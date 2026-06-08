import { Link } from "react-router-dom";
import { useSessionList, type SessionListItem } from "@/features/autooptimizer/api";

// Extend SessionListItem to include suspect_count which the API returns but
// the current TypeScript interface does not yet declare.
type SessionListItemFull = SessionListItem & {
  suspect_count?: number;
};

/**
 * OptimizerDigestStrip — compact one-liner on the home page showing the last
 * optimizer run outcome.
 *
 * Sits between LiveStrategiesSection and CriticalFindingsRow.
 * S1-merge: move between LiveStrategiesSection and CriticalFindingsRow
 *
 * Terminology (LOCKED — see CLAUDE.md):
 *   - "Honesty check"  (NOT "canary" or "null-result canary")
 *   - "kept"           (NOT "passed")
 *   - "suspect"        (NOT "quarantined")
 *   - "dropped"        (NOT "rejected")
 */
export function OptimizerDigestStrip() {
  const { data: sessions } = useSessionList();

  // Hidden when loading (undefined) or no runs recorded yet.
  if (!sessions || sessions.length === 0) {
    return null;
  }

  const session = sessions[0] as SessionListItemFull;

  const costLabel =
    session.cost_usd != null ? `$${session.cost_usd.toFixed(2)}` : "$?";

  // suspect_count is not currently in SessionListItem's TypeScript type, but is
  // present in the API response. Render a dash until the type is updated.
  // TODO: add suspect_count to SessionListItem interface when the API type is extended.
  const suspectLabel =
    session.suspect_count != null ? `${session.suspect_count} suspect` : "— suspect";

  return (
    <div
      data-testid="optimizer-digest-strip"
      className="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground border-b border-border/50"
    >
      <span className="font-medium text-foreground/70">Last run:</span>
      <span>
        {session.cycles_completed} experiments · {session.kept_count} kept ·{" "}
        {suspectLabel} · Honesty check — · {costLabel}
      </span>
      <Link
        to={`/optimizer/run/${session.session_id}`}
        className="ml-auto shrink-0 text-xs underline-offset-2 hover:underline"
      >
        View run →
      </Link>
    </div>
  );
}
