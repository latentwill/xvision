import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { SafetyPauseBanner } from "@/components/home/SafetyPauseBanner";
import { HomeOutcomeStrip } from "@/components/home/HomeOutcomeStrip";
import { ActiveTasksStrip } from "@/components/home/ActiveTasksStrip";
import { evalKeys, listRuns } from "@/api/eval";
import { chartKeys, getRunChart } from "@/api/chart";
import { strategyKeys, listStrategies } from "@/api/strategies";
import { scenarioKeys, listScenarios } from "@/api/scenarios";
import { agentKeys, listAgents } from "@/api/agents";
import { getBrokers, listProviders, settingsKeys } from "@/api/settings";
import { isInflightRunStatus } from "@/lib/run-status";
import { LiveSummaryStrip } from "@/components/home/LiveSummaryStrip";
import { OptimizerDigestStrip } from "@/components/home/OptimizerDigestStrip";
import { CriticalFindingsRow } from "@/components/home/CriticalFindingsRow";
import { StrategyOutcomesSummary } from "@/components/home/StrategyOutcomesSummary";
import { NagStrip } from "@/components/home/NagStrip";
import type { AttentionItem } from "@/components/home/NagStrip";
import type {
  BrokerEntry,
  BrokersReport,
  ProviderRow,
  RunSummary,
} from "@/api/types.gen";

// suppress unused-import lint — these are referenced by buildAttention keep-compat
void isInflightRunStatus;

export function HomeRoute() {
  const runs = useQuery({ queryKey: evalKeys.runs(), queryFn: () => listRuns() });
  const strategies = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });
  const agents = useQuery({
    queryKey: agentKeys.list(),
    queryFn: () => listAgents(),
  });
  const providers = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });
  const brokers = useQuery({
    queryKey: settingsKeys.brokers(),
    queryFn: getBrokers,
  });

  const recent = (runs.data ?? []).slice(0, 5);
  const latestChartableRun = recent.find(isChartableRun);
  const latestRunId = latestChartableRun?.id ?? "";
  // keep chart query alive so hook count stays stable across renders
  useQuery({
    queryKey: chartKeys.run(latestRunId),
    queryFn: () => getRunChart(latestRunId),
    enabled: !!latestRunId,
  });
  // scenario query kept for hook-count stability; will be used by W7 NagStrip
  useQuery({
    queryKey: scenarioKeys.list(),
    queryFn: () => listScenarios(),
  });

  const attentionItems = buildAttention({
    providers: providers.data?.providers,
    brokers: brokers.data,
  });

  const strategyCount = strategies.data?.length ?? 0;
  // agentCount kept alive for future use
  void agents.data;

  return (
    <>
      <Topbar
        title="Dashboard"
        sub={`${strategyCount} strategies`}
      />

      <div className="space-y-5">
        <SafetyPauseBanner />
        <HomeOutcomeStrip strategies={strategies.data ?? []} runs={runs.data ?? []} />
        <ActiveTasksStrip />
        <LiveSummaryStrip />
        <OptimizerDigestStrip />
        <CriticalFindingsRow runs={runs.data ?? []} />
        <StrategyOutcomesSummary strategies={strategies.data ?? []} runs={runs.data ?? []} />
        <NagStrip items={attentionItems} />
      </div>
    </>
  );
}

// ─── helpers ───────────────────────────────────────────────────────────────

function isChartableRun(run: RunSummary): boolean {
  return run.mode !== "live" && run.scenario_id.trim().length > 0;
}

// ─── attention rollup (nag items only — perf-drop/eval-failure moved to other sections) ──

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
