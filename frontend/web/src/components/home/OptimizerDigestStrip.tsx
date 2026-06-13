import { Link } from "react-router-dom";
import { useSessionList, useOptimizerStats } from "@/features/autooptimizer/api";
import {
  bestHoldoutDelta,
  costAnomaly,
  rollingAcceptanceRate,
} from "@/features/home/optimizer-summary";

/**
 * OptimizerDigestStrip — compact one-liner on the home page showing the last
 * optimizer run outcome plus a few FE-derivable health slices (zn2).
 *
 * Sits between LiveSummaryStrip and CriticalFindingsRow.
 * S1-merge: move between LiveSummaryStrip and CriticalFindingsRow
 *
 * Data sources (both honest — counts / Sharpe-style deltas / token cost; NO
 * live-money, P&L, capital, or budget cap is fabricated here):
 *   - useSessionList()    → the newest session's experiments/kept/suspect/honesty.
 *   - useOptimizerStats() → per-cycle StatsRow[] driving the zn2 segments:
 *       · 30d rolling acceptance rate (with a degradation tone),
 *       · best holdout Δ across cycles,
 *       · client-side cost-anomaly tint (latest cost vs trailing-cycle median).
 *
 * Honesty check (terminology lock): a degraded acceptance rate is not always
 * bad. The optimizer periodically runs a *sabotaged null-result* honesty test
 * (developer-surface codename "null-result canary"); when that runs, the
 * machine *correctly* degrades — a healthy signal. We document that on the
 * segment `title`/`aria` so the operator understands a warn tone may be the
 * machine passing its own honesty check, NOT a regression. The word "canary"
 * is developer-only and never appears in visible copy.
 *
 * Budget denominator (deferred to bead 8wn): the literal "$X / $Y today" cap
 * needs a persisted daily budget cap that does not exist yet. We render the
 * spend numerator honestly and an em-dash placeholder for the denominator —
 * never a faked cap.
 *
 * Terminology (LOCKED — see CLAUDE.md):
 *   - "Honesty check"  (NOT "canary" or "null-result canary")
 *   - "kept"           (NOT "passed")
 *   - "suspect"        (NOT "quarantined")
 *   - "dropped"        (NOT "rejected")
 */

const HONESTY_TITLE =
  "30-day acceptance rate (kept ÷ all candidates). A drop can be the machine " +
  "correctly degrading under a sabotaged null-result honesty check — not " +
  "always a regression.";

export function OptimizerDigestStrip() {
  const { data: sessions } = useSessionList();
  const { data: stats } = useOptimizerStats();

  // Hidden when loading (undefined) or no runs recorded yet.
  if (!sessions || sessions.length === 0) {
    return null;
  }

  const session = sessions[0];
  const rows = stats ?? [];

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

  // Newest cycle's lineage edge over the random baseline (S0). undefined → "—".
  // > 0 means the accepted lineage still beats a no-intelligence random agent.
  const edge = session.latest_parent_edge;
  const edgeLabel =
    edge == null ? "Edge vs random —" : `Edge vs random ${edge >= 0 ? "+" : ""}${edge.toFixed(2)}`;

  // ─── zn2 FE-derivable slices (off StatsRow[]) ──────────────────────────────

  // 30-day rolling acceptance rate + degradation signal.
  const acceptance = rollingAcceptanceRate(rows);
  const acceptanceLabel =
    acceptance.rate === null
      ? "— accepted (30d)"
      : `${Math.round(acceptance.rate * 100)}% accepted (30d)`;
  // Degradation = saturated text tint (warn), never a side-stripe. Gold when
  // healthy and we have a real rate; muted when there's no in-window data.
  const acceptanceTone = acceptance.degraded
    ? "text-warn"
    : acceptance.rate !== null
      ? "text-gold"
      : undefined;

  // Best holdout Δ across cycles (max of finite best_delta_holdout).
  const holdout = bestHoldoutDelta(rows);
  const holdoutLabel =
    holdout === null
      ? "Best holdout Δ —"
      : `Best holdout Δ ${holdout >= 0 ? "+" : ""}${holdout.toFixed(2)}`;
  const holdoutTone =
    holdout === null ? undefined : holdout >= 0 ? "text-gold" : "text-warn";

  // Cost-anomaly tint: latest cycle cost vs trailing-cycle median.
  const cost = costAnomaly(rows);
  const costTone = cost.anomalous ? "text-warn" : undefined;
  const costTitle = cost.anomalous
    ? `Latest cycle cost ($${cost.currentUsd?.toFixed(2) ?? "—"}) is well above the` +
      ` trailing-cycle median ($${cost.medianUsd?.toFixed(2) ?? "—"}).`
    : "Latest optimizer session spend. Daily budget cap pending.";
  // Spend numerator is honest; the cap denominator is deferred to bead 8wn —
  // render an em-dash placeholder, never a faked cap.
  const spendLabel = `${costLabel} / —`;

  return (
    <div
      data-testid="optimizer-digest-strip"
      className="flex items-center gap-2 px-5 py-2.5 text-[12px] text-text-3 border-t border-border-soft"
    >
      <span className="font-medium text-text-2">Last run:</span>
      <span>
        <span className="font-mono tabular-nums">{session.cycles_completed}</span>{" "}
        experiments ·{" "}
        <span className="font-mono tabular-nums">{session.kept_count}</span> kept ·{" "}
        {suspectLabel} ·{" "}
        <span
          className={
            session.honesty_passed === false ? "text-amber-600 dark:text-amber-400" : undefined
          }
        >
          {honestyLabel}
        </span>{" "}
        ·{" "}
        <span
          className={
            edge != null && edge < 0 ? "text-amber-600 dark:text-amber-400" : undefined
          }
          title="Newest cycle's accepted-lineage edge over a fixed-seed random agent (parent − random)"
        >
          {edgeLabel}
        </span>{" "}
        ·{" "}
        <span
          data-testid="optimizer-digest-acceptance"
          className={acceptanceTone}
          title={HONESTY_TITLE}
          aria-label={HONESTY_TITLE}
        >
          {acceptanceLabel}
        </span>{" "}
        ·{" "}
        <span
          data-testid="optimizer-digest-holdout"
          className={`font-mono tabular-nums ${holdoutTone ?? ""}`}
          title="Best holdout-window Sharpe delta across recent cycles (best candidate − baseline on the untouched window)."
        >
          {holdoutLabel}
        </span>{" "}
        ·{" "}
        <span
          data-testid="optimizer-digest-cost"
          className={`font-mono tabular-nums ${costTone ?? ""}`}
          title={costTitle}
        >
          {spendLabel}
        </span>
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
