// frontend/web/src/routes/home.tsx
//
// Home dashboard — "quant mission-control, calm density" (dashboard
// redesign, docs/design/README.md). Four bento sections in one center
// column, revealed with a single orchestrated stagger:
//
//   1. Pulse band      — equity + drawdown hero, KPI numerals, honest
//                        execution-state chip, freshness stamp.
//   2. Attention band  — honest live counts, in-flight tasks, critical
//                        findings, awaiting-first-eval action, config nags.
//   3. Optimizer panel — experiments accepted/rejected, writer ladder,
//                        cycle trend, cumulative spend ("is the machine
//                        doing good work?").
//   4. Strategy leaderboard — top strategies by latest completed eval,
//                        with sample-size honesty and the coverage footer.
//
// All ranking/aggregation logic lives in tested selectors under
// `features/home/`; this route only wires queries to components.

import { useEffect } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";

import { Topbar } from "@/components/shell/Topbar";
import { SafetyPauseBanner } from "@/components/home/SafetyPauseBanner";
import { PulseBand } from "@/components/home/PulseBand";
import { AttentionBand } from "@/components/home/AttentionBand";
import { OptimizerPanel } from "@/components/home/OptimizerPanel";
import { StrategyLeaderboard } from "@/components/home/StrategyLeaderboard";
import type { AttentionItem } from "@/components/home/NagStrip";
import { chartKeys, getRunChart } from "@/api/chart";
import { evalKeys, listRuns } from "@/api/eval";
import { strategyKeys, listStrategies } from "@/api/strategies";
import { getBrokers, listProviders, settingsKeys } from "@/api/settings";
import { agentRunKeys, listAgentRuns } from "@/api/agent-runs";
import { livenessCounts } from "@/features/live/strip-status";
import { pickHeroRun } from "@/features/home/pulse";
import type {
  BrokerEntry,
  BrokersReport,
  ProviderRow,
} from "@/api/types.gen";

// One page of recent runs feeds the pulse KPIs, leaderboard, and coverage
// join. 100 keeps the hero/leaderboard meaningful on busy nodes (the server
// caps at 200) without pulling the full ledger.
const RUNS_PAGE = { limit: 100 } as const;

// Same population (and cache entry) as LiveSummaryStrip: liveness is derived
// from non-terminal agent runs only.
const LIVENESS_PARAMS = { status: "running,queued", limit: 100 } as const;

export function HomeRoute() {
  const runs = useQuery({
    queryKey: evalKeys.runs(RUNS_PAGE),
    queryFn: () => listRuns(RUNS_PAGE),
  });
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

  const attentionItems = buildAttention({
    providers: providers.data?.providers,
    brokers: brokers.data,
  });

  const strategyCount = strategies.data?.length ?? 0;
  const liveness = agentRuns.data ? livenessCounts(agentRuns.data) : null;

  return (
    <>
      <Topbar
        title="Dashboard"
        sub={`${strategyCount} strategies`}
      />

      <div className="space-y-5">
        <SafetyPauseBanner />

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
            strategies={strategies.data ?? []}
            nagItems={attentionItems}
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
  providers: ProviderRow[] | undefined;
  brokers: BrokersReport | undefined;
}): AttentionItem[] {
  const out: AttentionItem[] = [];

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
