// frontend/web/src/routes/home.tsx
//
// Home dashboard — "quant mission-control, calm density" (dashboard
// redesign, docs/design/README.md). Bento sections in one center column,
// revealed with a single orchestrated stagger:
//
//   0. Safety banner       — global safety-pause state.
//   0b. Deploy readiness    — slim safety GATE band (e17): keys / broker /
//                            no-blocking-eval. Mounted under the safety
//                            banner, above the outcome/pulse content because
//                            it answers "can I deploy?", not "what's nagging?".
//   1. Pulse band          — equity + drawdown hero, KPI numerals, honest
//                            execution-state chip, freshness stamp.
//   2. Attention band      — honest live counts, in-flight tasks, critical
//                            findings (incl. suspicious failed runs), config
//                            + stale-infra-failure nags.
//   3. Optimizer panel     — experiments accepted/rejected, writer ladder,
//                            cycle trend, cumulative spend.
//   4. Strategy leaderboard — top strategies by latest completed eval.
//
// The Topbar subtitle is the honest "since you were last here" delta (jlm):
// runs/findings completed since the previous visit. Counts are eval facts —
// never live-money / P&L / capital (honesty mandate §8.1/§8.9).
//
// All ranking/aggregation logic lives in tested selectors under
// `features/home/`; this route only wires queries to components.

import { useEffect, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { Topbar } from "@/components/shell/Topbar";
import { SafetyPauseBanner } from "@/components/home/SafetyPauseBanner";
import { CapitalRiskStrip } from "@/components/home/CapitalRiskStrip";
import { HomeDeltaSubtitle } from "@/components/home/HomeDeltaSubtitle";
import { HomeOutcomeStrip } from "@/components/home/HomeOutcomeStrip";
import {
  TimeWindowPills,
  sinceForWindow,
  type TimeWindow,
} from "@/components/home/TimeWindowPills";
import { PulseBand } from "@/components/home/PulseBand";
import { AttentionBand } from "@/components/home/AttentionBand";
import { OptimizerPanel } from "@/components/home/OptimizerPanel";
import { CostRollupStrip } from "@/components/home/CostRollupStrip";
import { StrategyLeaderboard } from "@/components/home/StrategyLeaderboard";
import type { AttentionItem } from "@/components/home/NagStrip";
import { chartKeys, getRunChart } from "@/api/chart";
import { costKeys, getCostBudget, getCostRollup } from "@/api/cost";
import { evalKeys, listRuns } from "@/api/eval";
import { deploymentKeys, listDeployments } from "@/api/live-deployments";
import { strategyKeys, listStrategies } from "@/api/strategies";
import { getBrokers, settingsKeys } from "@/api/settings";
import { agentRunKeys, listAgentRuns } from "@/api/agent-runs";
import { listCriticalFindings } from "@/api/eval-review";
import { livenessCounts } from "@/features/live/strip-status";
import { pickHeroRun } from "@/features/home/pulse";
import { aggregateCapitalRisk } from "@/features/home/capital-risk";
import {
  computeSinceDelta,
  persistVisitOnce,
  snapshotLastVisit,
} from "@/features/home/last-visit";
import type {
  BrokerEntry,
  BrokersReport,
} from "@/api/types.gen";

// One page of recent runs feeds the pulse KPIs, leaderboard, and coverage
// join. 100 keeps the hero/leaderboard meaningful on busy nodes (the server
// caps at 200) without pulling the full ledger.
const RUNS_PAGE = { limit: 100 } as const;

// Same population (and cache entry) as LiveSummaryStrip: liveness is derived
// from non-terminal agent runs only.
const LIVENESS_PARAMS = { status: "running,queued", limit: 100 } as const;

// n0k/awm (CT5 §9): active live/paper deployments for the ActiveTasksStrip live
// rows. Capital / P&L / drawdown come from THIS 5s poll (per-tick capital
// streaming is deferred; CT5 §4), filtered to the active window.
const DEPLOYMENTS_PARAMS = { status: "running,paused" } as const;

export function HomeRoute() {
  const runs = useQuery({
    queryKey: evalKeys.runs(RUNS_PAGE),
    queryFn: () => listRuns(RUNS_PAGE),
  });

  // bead-008: the time-window pills scope ONLY the outcomes + findings
  // surfaces. The pulse hero, leaderboard, and last-visit delta stay on the
  // unscoped `runs` query above. Default 'all' => sinceForWindow returns
  // undefined => the windowed params equal RUNS_PAGE and collapse onto the
  // same cache key, so first paint issues NO extra fetch.
  const [window, setWindow] = useState<TimeWindow>("all");
  const since = sinceForWindow(window);
  const scopedParams = since ? { ...RUNS_PAGE, since } : RUNS_PAGE;
  const scopedRuns = useQuery({
    queryKey: evalKeys.runs(scopedParams),
    queryFn: () => listRuns(scopedParams),
  });
  // When 'all', reuse the unscoped page so the outcomes/findings surfaces stay
  // pixel-identical to today's first paint while the windowed fetch (if any)
  // resolves.
  const windowedRuns = since ? (scopedRuns.data ?? []) : (runs.data ?? []);
  const strategies = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const brokers = useQuery({
    queryKey: settingsKeys.brokers(),
    queryFn: getBrokers,
  });
  const agentRuns = useQuery({
    queryKey: agentRunKeys.list(LIVENESS_PARAMS),
    queryFn: () => listAgentRuns(LIVENESS_PARAMS),
    refetchInterval: 10_000,
  });


  // jlm: read-before-write the last-visit boundary. snapshotLastVisit() freezes
  // the PREVIOUS boundary at module scope on the first render this page load —
  // so this paint (and any in-session remount / StrictMode double-invoke) shows
  // the delta since that prior visit — then persistVisitOnce (below) writes
  // "now" a single time so the NEXT page load measures from here. bead s78.2
  // reads it HERE (above the deployments poll) so the same boundary feeds the
  // deployments `?since` risk-veto count.
  const lastVisitIso = snapshotLastVisit();

  // n0k/awm: live/paper deployments for the ActiveTasksStrip live rows. 5s
  // poll matches the CT5 contract (§3) — list membership AND the honest
  // capital/P&L/drawdown fields both ride this poll.
  // bead s78.2: pass the jlm last-visit boundary (the SAME boundary the home
  // delta + cost rollup window off) as `?since` so each deployment carries a
  // REAL count of risk vetoes recorded since the operator was last here. On a
  // first visit there is no boundary, so `since` is omitted → the backend
  // leaves `risk_veto_count_since_last_visit` null and the chip shows "—".
  const deploymentsParams = lastVisitIso
    ? { ...DEPLOYMENTS_PARAMS, since: lastVisitIso }
    : DEPLOYMENTS_PARAMS;
  const deployments = useQuery({
    queryKey: deploymentKeys.list(deploymentsParams),
    queryFn: () => listDeployments(deploymentsParams),
    refetchInterval: 5_000,
  });

  // jlm: findings for the "since you were last here" delta. Shares the exact
  // query CriticalFindingsRow uses (same key) so this is a cache hit, not a
  // second fan-out. Findings counts are eval facts (honest), never money.
  const recentRunsForFindings = (runs.data ?? []).slice(0, 3);
  const criticalFindings = useQuery({
    queryKey: ["critical-findings", recentRunsForFindings.map((r) => r.id)],
    queryFn: () => listCriticalFindings(runs.data ?? []),
    enabled: (runs.data?.length ?? 0) > 0,
  });

  // Kill the runs→chart waterfall: start the slim hero-chart fetch the
  // moment the runs page lands, before PulseBand mounts its own query.
  const queryClient = useQueryClient();
  useEffect(() => {
    const hero = pickHeroRun(runs.data ?? []);
    if (!hero) return;
    void queryClient.prefetchQuery({
      queryKey: chartKeys.run(hero.id, ["equity"]),
      queryFn: () => getRunChart(hero.id, ["equity"]),
      staleTime: 30_000,
    });
  }, [runs.data, queryClient]);

  // jlm: persist "now" as the new boundary a single time per page load so the
  // NEXT page load measures from here. The PREVIOUS boundary was already frozen
  // above (snapshotLastVisit, read before this write) and drives both the delta
  // subtitle and the deployments `?since` risk-veto count this render.
  useEffect(() => {
    persistVisitOnce(new Date().toISOString());
  }, []);

  const delta = computeSinceDelta({
    runs: runs.data ?? [],
    findings: criticalFindings.data ?? [],
    lastVisitIso,
  });

  // 8wn: cost rollup strip — spend SINCE LAST VISIT and THIS WEEK vs the
  // operator cap. Two read-only windowed rollups + the persisted cap. Both
  // windows scope on a stable boundary frozen per page-load so they don't
  // refetch on every render:
  //   - since-last-visit uses the SAME LAST_VISIT_LS boundary as the home
  //     delta subtitle (the two surfaces must agree). On a first visit there
  //     is no boundary → skip the query and show an honest "first visit".
  //   - this-week = now - 7d, frozen once via useState so the cache key is
  //     stable across re-renders.
  const firstVisit = delta.firstVisit;
  const [weekSince] = useState(
    () => new Date(Date.now() - 7 * 24 * 60 * 60 * 1000).toISOString(),
  );
  const costSinceVisit = useQuery({
    queryKey: costKeys.rollup(lastVisitIso ?? ""),
    queryFn: () => getCostRollup({ since: lastVisitIso ?? undefined }),
    enabled: !firstVisit && lastVisitIso != null,
    staleTime: 30_000,
    retry: false,
  });
  const costThisWeek = useQuery({
    queryKey: costKeys.rollup(weekSince),
    queryFn: () => getCostRollup({ since: weekSince }),
    staleTime: 30_000,
    retry: false,
  });
  const costBudget = useQuery({
    queryKey: costKeys.budget(),
    queryFn: getCostBudget,
    staleTime: 60_000,
    retry: false,
  });
  const attentionItems = buildAttention({
    brokers: brokers.data,
  });

  const liveness = agentRuns.data ? livenessCounts(agentRuns.data) : null;

  // 8s4: capital-risk strip aggregate. REUSES the live-deployments poll above
  // (no second fetch). With zero live deployments we say nothing — never imply
  // live capital exists when none does; the strip mounts only when there is at
  // least one active deployment (its own below-floor "insufficient data" state
  // covers the deployed-but-no-fills-yet case honestly).
  const liveDeployments = deployments.data ?? [];
  const capitalRisk = aggregateCapitalRisk(liveDeployments);

  return (
    <>
      <Topbar
        title="Dashboard"
        sub={<HomeDeltaSubtitle delta={delta} />}
      />

      <div className="space-y-5">
        <SafetyPauseBanner />

        {/* Deploy readiness gate (e17) removed — unused queries dropped */}

        {/* 8s4: capital-risk safety strip — slim top band in the safety-gate
            area, under SafetyPauseBanner (which keeps its top precedence) and
            DeployReadinessStrip. Mounts only when there is at least one active
            live/paper deployment, so an idle node says nothing rather than
            implying live capital exists. */}
        {liveDeployments.length > 0 && <CapitalRiskStrip agg={capitalRisk} />}

        {/* bead-008: inline, full-width window selector scoping the outcomes +
            findings surfaces below it. It does NOT scope the pulse hero,
            leaderboard, or the last-visit delta. */}
        <div className="flex flex-wrap items-center justify-between gap-2">
          <span className="text-[10px] uppercase tracking-wide text-text-3">
            Window
          </span>
          <TimeWindowPills value={window} onChange={setWindow} />
        </div>

        <HomeOutcomeStrip runs={windowedRuns} strategies={strategies.data ?? []} />

        <div className="xvn-card-in" style={{ animationDelay: "0ms" }}>
          <PulseBand
            runs={runs.data ?? []}
            strategies={strategies.data ?? []}
            liveness={liveness}
            runsPending={runs.isPending}
          />
        </div>

        <div className="xvn-card-in" style={{ animationDelay: "70ms" }}>
          <AttentionBand
            nagItems={attentionItems}
            deployments={deployments.data ?? []}
          />
        </div>

        <div className="xvn-card-in" style={{ animationDelay: "140ms" }}>
          <OptimizerPanel />
        </div>

        <div className="xvn-card-in" style={{ animationDelay: "175ms" }}>
          <CostRollupStrip
            sinceLastVisit={costSinceVisit.data ?? null}
            thisWeek={costThisWeek.data ?? null}
            dailyCapUsd={costBudget.data?.daily_cap_usd ?? null}
            firstVisit={firstVisit}
          />
        </div>

        <div className="xvn-card-in" style={{ animationDelay: "210ms" }}>
          <StrategyLeaderboard
            strategies={strategies.data ?? []}
            runs={runs.data ?? []}
          />
        </div>
      </div>
    </>
  );
}

// ─── attention rollup (nag items only — perf-drop/eval-failure live in other sections) ──

function buildAttention(input: {
  brokers: BrokersReport | undefined;
}): AttentionItem[] {
  const out: AttentionItem[] = [];

  // Only live-trading-specific issues belong here — general config
  // nags and eval failures are surfaced elsewhere on the dashboard.
  if (input.brokers && !input.brokers.alpaca.configured) {
    out.push(brokerAttention(input.brokers.alpaca));
  }

  return out;
}

function brokerAttention(b: BrokerEntry): AttentionItem {
  return {
    tone: "warn",
    title: `${b.name} credentials not set`,
    detail: b.credentials
      .filter((c) => !c.is_set)
      .map((c) => c.env_var)
      .join(", "),
    link: { to: "/settings/brokers", label: "set up" },
  };
}
