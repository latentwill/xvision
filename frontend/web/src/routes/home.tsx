import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { SafetyPauseBanner } from "@/components/home/SafetyPauseBanner";
import { HomeOutcomeStrip } from "@/components/home/HomeOutcomeStrip";
import { ActiveTasksStrip } from "@/components/home/ActiveTasksStrip";
import { RunChartV2 } from "@/components/chart/v2/surfaces/RunChartV2";
import { runChartPayloadToV2 } from "@/components/chart/v2/adapters/run-chart-payload";
import { evalKeys, listRuns } from "@/api/eval";
import { chartKeys, getRunChart } from "@/api/chart";
import { strategyKeys, listStrategies } from "@/api/strategies";
import { scenarioKeys, listScenarios } from "@/api/scenarios";
import { agentKeys, listAgents } from "@/api/agents";
import { getBrokers, listProviders, settingsKeys } from "@/api/settings";
import { isInflightRunStatus } from "@/lib/run-status";
import { displayStrategyName } from "@/lib/run-display";
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

void isInflightRunStatus; // used in isChartableRun guard below

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
  const latestChart = useQuery({
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

  const hasRuns = (runs.data?.length ?? 0) > 0;
  const latestStrategyName = latestChartableRun
    ? displayStrategyName(latestChartableRun.agent_id ?? "", strategies.data ?? [])
    : "";

  return (
    <>
      <Topbar
        title="Dashboard"
        sub={`cockpit · ${strategyCount} strategies`}
      />

      <div className="space-y-5">
        <SafetyPauseBanner />
        <HomeOutcomeStrip strategies={strategies.data ?? []} runs={runs.data ?? []} />
        <ActiveTasksStrip />

        {/* Latest run equity chart — surfaced from the eval list */}
        {hasRuns ? (
          <Card className="p-0 overflow-hidden">
            <div className="flex items-center justify-between px-4 py-3 border-b border-border-soft">
              <div>
                <span className="text-[13px] font-medium text-text">
                  Latest run
                </span>
                {latestStrategyName ? (
                  <span className="ml-2 text-[12px] text-text-3">
                    {latestStrategyName}
                  </span>
                ) : null}
              </div>
              {latestRunId ? (
                <Link
                  to={`/eval-runs/${latestRunId}`}
                  className="text-[12px] text-text-3 hover:text-text"
                >
                  View run →
                </Link>
              ) : null}
            </div>
            <div className="p-4">
              {runs.isPending ? (
                <div className="text-[13px] text-text-3 text-center py-6">
                  Loading…
                </div>
              ) : !latestRunId ? (
                <div className="text-[13px] text-text-3 text-center py-6">
                  No chartable runs on this page.
                </div>
              ) : latestChart.isPending ? (
                <div className="text-[13px] text-text-3 text-center py-6">
                  Loading chart…
                </div>
              ) : latestChart.isError ? (
                <div className="text-[13px] text-text-3 text-center py-6">
                  Chart unavailable.
                </div>
              ) : latestChart.data ? (
                <RunChartV2 payload={runChartPayloadToV2(latestChart.data)} />
              ) : null}
            </div>
          </Card>
        ) : null}

        {/* Recent activity — last 5 eval runs */}
        {hasRuns ? (
          <Card className="p-0 overflow-hidden">
            <div className="flex items-center justify-between px-4 py-3 border-b border-border-soft">
              <span className="text-[13px] font-medium text-text">
                Recent runs
              </span>
              <Link
                to="/eval-runs"
                className="text-[12px] text-text-3 hover:text-text"
              >
                View all →
              </Link>
            </div>
            <div className="divide-y divide-border-soft">
              {recent.map((run) => (
                <Link
                  key={run.id}
                  to={`/eval-runs/${run.id}`}
                  className="flex items-center gap-3 px-4 py-2.5 hover:bg-surface-hover transition-colors"
                >
                  <div className="flex-1 min-w-0">
                    <div className="text-[13px] text-text truncate">
                      {displayStrategyName(run.agent_id ?? "", strategies.data ?? [])}
                    </div>
                    <div className="text-[11px] text-text-3 font-mono">
                      {run.id.slice(0, 8)}
                    </div>
                  </div>
                  <Pill tone={runTone(run.status)} animated={isInflightRunStatus(run.status)}>
                    {run.status}
                  </Pill>
                  <span className={`text-[13px] font-mono tabular-nums w-16 text-right ${signedToneClass(run.total_return_pct)}`}>
                    {fmtPct(run.total_return_pct)}
                  </span>
                </Link>
              ))}
            </div>
          </Card>
        ) : (
          /* Zero-state CTA — guides first-time users */
          <Card className="p-6 text-center space-y-3">
            <p className="text-[15px] font-medium text-text">
              Run your first eval
            </p>
            <p className="text-[13px] text-text-3 max-w-sm mx-auto">
              Pick a strategy and scenario to backtest, or start a live paper
              deployment.
            </p>
            <div className="flex items-center justify-center gap-3">
              <Link
                to="/eval-runs?start=1"
                className="inline-flex items-center gap-1.5 rounded bg-gold px-3.5 py-1.5 text-[13px] font-medium text-bg hover:bg-gold-soft transition-colors"
              >
                Start eval
              </Link>
              <Link
                to="/strategies"
                className="text-[13px] text-text-3 hover:text-text"
              >
                Browse strategies →
              </Link>
            </div>
          </Card>
        )}

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

function runTone(status: string): "gold" | "info" | "warn" | "danger" | "default" {
  switch (status) {
    case "completed": return "gold";
    case "running":
    case "queued": return "info";
    case "failed": return "danger";
    case "cancelled": return "warn";
    default: return "default";
  }
}

function signedToneClass(n: number | null | undefined): string {
  if (n == null || n === 0) return "text-text";
  return n > 0 ? "text-gold" : "text-danger";
}

function fmtPct(n: number | null | undefined): string {
  if (n == null) return "—";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(2)}%`;
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
