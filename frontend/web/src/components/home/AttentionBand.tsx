// frontend/web/src/components/home/AttentionBand.tsx
//
// Home "live & attention" band (dashboard redesign §2): one calm card that
// unifies the honest live-trading counts, in-flight tasks, critical findings,
// the awaiting-first-eval next action, and config nags. Every row is a routed
// drilldown phrased as the next action — never a dead-end nag. Rows that have
// nothing to say render nothing (their components return null), so the band
// shrinks instead of stacking empty placeholders.

import { Link } from "react-router-dom";

import type { LiveDeploymentSummary, RunSummary } from "@/api/types.gen";
import type { StrategyListItem } from "@/api/strategies";
import { Card } from "@/components/primitives/Card";
import {
  coverageCounts,
  strategyEvalCoverage,
} from "@/features/strategies/coverage";
import type { FailedRunFinding } from "@/features/home/failed-runs";
import { ActiveTasksStrip } from "./ActiveTasksStrip";
import { CriticalFindingsRow } from "./CriticalFindingsRow";
import { LiveSummaryStrip } from "./LiveSummaryStrip";
import { NagStrip, type AttentionItem } from "./NagStrip";

export interface AttentionBandProps {
  runs: RunSummary[];
  strategies: StrategyListItem[];
  /** Config + stale-infra-failure nags. Stale infra failures lead, config
   * nags after; empty for none. */
  nagItems: AttentionItem[];
  /** Suspicious failed-run findings (bead xvision-1zs) — merged into the
   * Recent Findings surface after human-reviewed criticals. */
  failedRunFindings?: FailedRunFinding[];
  /** Runs that scope the Recent Findings surface only (bead-008 time window).
   * Coverage / awaiting-eval counts keep using the unscoped `runs`; only the
   * CriticalFindingsRow rescopes. Defaults to `runs` when unset. */
  findingsRuns?: RunSummary[];
  /** n0k/awm: live/paper deployment rows from the home route's 5s poll,
   * forwarded straight to ActiveTasksStrip. Empty/undefined => no live group. */
  deployments?: LiveDeploymentSummary[];
}

export function AttentionBand({
  runs,
  strategies,
  nagItems,
  failedRunFindings = [],
  findingsRuns,
  deployments,
}: AttentionBandProps) {
  const counts = coverageCounts(strategyEvalCoverage(strategies, runs));
  const findingsScope = findingsRuns ?? runs;

  return (
    <section data-testid="attention-band" aria-label="Live and attention">
      <Card className="p-0 overflow-hidden xvn-card-hover">
        <div className="divide-y divide-border-soft">
          <LiveSummaryStrip />
          <ActiveTasksStrip deployments={deployments} />
          <CriticalFindingsRow
            runs={findingsScope}
            failedRunFindings={failedRunFindings}
          />
          {counts.userAwaitingFirstEval > 0 ? (
            <div
              data-testid="awaiting-eval-action"
              className="flex flex-wrap items-center gap-x-2 gap-y-1 px-5 py-2.5 text-[12px]"
            >
              <span
                className="h-1.5 w-1.5 shrink-0 rounded-full bg-warn"
                aria-hidden
              />
              <Link
                to="/eval-runs"
                className="font-medium text-text hover:underline"
              >
                Evaluate {counts.userAwaitingFirstEval} user{" "}
                {counts.userAwaitingFirstEval === 1 ? "strategy" : "strategies"}{" "}
                awaiting first eval →
              </Link>
              {counts.optimizerLineage > 0 ? (
                <span className="text-text-4">
                  · {counts.optimizerLineage} optimizer-generated (evaluated in
                  lineage)
                </span>
              ) : null}
            </div>
          ) : null}
          <NagStrip items={nagItems} />
        </div>
      </Card>
    </section>
  );
}
