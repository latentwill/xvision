import { Link } from "react-router-dom";
import { useSessionList } from "@/features/autooptimizer/api";

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

  const session = sessions[0];

  const costLabel =
    session.cost_usd != null ? `$${session.cost_usd.toFixed(2)}` : "$?";

  // suspect_count is now part of SessionListItem (S0 / O1a) — render the real
  // value, falling back to a dash only when the field is genuinely absent.
  const suspectLabel =
    session.suspect_count != null ? `${session.suspect_count} suspect` : "— suspect";

  // Honesty check outcome of the session's newest cycle (S0 / O1b).
  // undefined → "—" (no honesty check ran yet).
  const honestyLabel =
    session.honesty_passed == null
      ? "Honesty check —"
      : session.honesty_passed
        ? "Honesty check ✓"
        : "Honesty check ✗ failed";

  return (
    <div
      data-testid="optimizer-digest-strip"
      className="flex items-center gap-2 px-3 py-2 text-sm text-muted-foreground border-b border-border/50"
    >
      <span className="font-medium text-foreground/70">Last run:</span>
      <span>
        {session.cycles_completed} experiments · {session.kept_count} kept ·{" "}
        {suspectLabel} ·{" "}
        <span
          className={
            session.honesty_passed === false ? "text-amber-600 dark:text-amber-400" : undefined
          }
        >
          {honestyLabel}
        </span>{" "}
        · {costLabel}
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
