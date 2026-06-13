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
import { DeployReadinessStrip } from "@/components/home/DeployReadinessStrip";
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
import { StrategyLeaderboard } from "@/components/home/StrategyLeaderboard";
import type { AttentionItem } from "@/components/home/NagStrip";
import { chartKeys, getRunChart } from "@/api/chart";
import { evalKeys, listRuns } from "@/api/eval";
import { strategyKeys, listStrategies } from "@/api/strategies";
import { getBrokers, listProviders, settingsKeys, testAlpacaConnection } from "@/api/settings";
import { agentRunKeys, listAgentRuns } from "@/api/agent-runs";
import { getSafetyState, safetyKeys } from "@/api/safety";
import { listCriticalFindings } from "@/api/eval-review";
import { livenessCounts } from "@/features/live/strip-status";
import { pickHeroRun } from "@/features/home/pulse";
import { buildDeployReadiness } from "@/features/home/deploy-readiness";
import { failedRunFindings, failedRunNags } from "@/features/home/failed-runs";
import {
  computeSinceDelta,
  persistVisitOnce,
  snapshotLastVisit,
} from "@/features/home/last-visit";
import type {
  BrokerEntry,
  BrokersReport,
  ProviderRow,
  RunSummary,
} from "@/api/types.gen";

// One page of recent runs feeds the pulse KPIs, leaderboard, and coverage
// join. 100 keeps the hero/leaderboard meaningful on busy nodes (the server
// caps at 200) without pulling the full ledger.
const RUNS_PAGE = { limit: 100 } as const;

// Same population (and cache entry) as LiveSummaryStrip: liveness is derived
// from non-terminal agent runs only.
const LIVENESS_PARAMS = { status: "running,queued", limit: 100 } as const;

// In-flight eval runs for the deploy-readiness "no blocking eval" check —
// same shape ActiveTasksStrip uses, so the stuck-run story stays consistent.
const INFLIGHT_PARAMS = { status: "queued,running" } as const;

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
  const providers = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
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

  // e17: safety state for the deploy-readiness "no blocking eval" check.
  // Reuses safetyKeys.state so SafetyPauseBanner and the readiness gate share
  // one cache entry rather than double-fetching.
  const safety = useQuery({
    queryKey: safetyKeys.state(),
    queryFn: getSafetyState,
  });

  // e17: broker reachability — configured-but-unreachable is a deploy-blocker.
  // Only probe when Alpaca is actually configured; an unconfigured broker is
  // not "unreachable", it's just "not set up yet" (handled by the selector).
  const brokerConfigured = brokers.data?.alpaca.configured ?? false;
  const brokerTest = useQuery({
    queryKey: [...settingsKeys.brokers(), "test-connection"],
    queryFn: testAlpacaConnection,
    enabled: brokerConfigured,
  });

  // e17: the in-flight eval list (queued|running) feeds the stuck-run check.
  const inflightRuns = useQuery({
    queryKey: evalKeys.runs(INFLIGHT_PARAMS),
    queryFn: () => listRuns(INFLIGHT_PARAMS),
    refetchInterval: 10_000,
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

  // jlm: read-before-write the last-visit boundary. snapshotLastVisit() freezes
  // the PREVIOUS boundary at module scope on the first render this page load —
  // so this paint (and any in-session remount / StrictMode double-invoke) shows
  // the delta since that prior visit — then persistVisitOnce writes "now" a
  // single time so the NEXT page load measures from here.
  const lastVisitIso = snapshotLastVisit();
  useEffect(() => {
    persistVisitOnce(new Date().toISOString());
  }, []);

  const delta = computeSinceDelta({
    runs: runs.data ?? [],
    findings: criticalFindings.data ?? [],
    lastVisitIso,
  });

  const attentionItems = buildAttention({
    runs: runs.data ?? [],
    providers: providers.data?.providers,
    brokers: brokers.data,
  });

  const liveness = agentRuns.data ? livenessCounts(agentRuns.data) : null;

  const readinessChecks = buildDeployReadiness({
    providers: providers.data?.providers,
    brokers: brokers.data,
    brokerTest: brokerTest.data,
    safety: safety.data,
    inflightRuns: inflightRuns.data ?? [],
  });

  return (
    <>
      <Topbar
        title="Dashboard"
        sub={<HomeDeltaSubtitle delta={delta} />}
      />

      <div className="space-y-5">
        <SafetyPauseBanner />

        <DeployReadinessStrip checks={readinessChecks} />

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
            runs={runs.data ?? []}
            findingsRuns={windowedRuns}
            strategies={strategies.data ?? []}
            nagItems={attentionItems}
            failedRunFindings={failedRunFindings(windowedRuns)}
          />
        </div>

        <div className="xvn-card-in" style={{ animationDelay: "140ms" }}>
          <OptimizerPanel />
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
  runs: RunSummary[];
  providers: ProviderRow[] | undefined;
  brokers: BrokersReport | undefined;
}): AttentionItem[] {
  const out: AttentionItem[] = [];

  // 1zs: stale-infra-failure nags lead — an upstream-broke run still sitting
  // failed is more actionable than a missing config key. Config nags follow
  // (lowest priority per §6). failedRunNags already returns AttentionItem-
  // compatible rows (tone/title/detail/link).
  out.push(...failedRunNags(input.runs));

  const missingKeys = (input.providers ?? []).filter(
    (p) => !p.synthetic && p.api_key_env !== "" && !p.api_key_set,
  );
  if (missingKeys.length > 0) {
    out.push({
      tone: "warn",
      title: `${missingKeys.length} provider${missingKeys.length === 1 ? "" : "s"} missing API key`,
      detail: missingKeys
        .map((p) => `${p.name} → ${p.api_key_env}`)
        .join(", "),
      link: { to: "/settings/providers", label: "configure" },
    });
  }

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
