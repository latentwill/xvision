import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import { getHealth, healthKeys } from "@/api/health";
import { evalKeys, listRuns } from "@/api/eval";
import { chartKeys, getRunChart } from "@/api/chart";
import { strategyKeys, listStrategies } from "@/api/strategies";
import { agentKeys, listAgents } from "@/api/agents";
import {
  getBrokers,
  getIdentity,
  listProviders,
  settingsKeys,
} from "@/api/settings";
import { RunChart } from "@/components/chart/RunChart";
import type {
  BrokerEntry,
  BrokersReport,
  HealthReport,
  HealthStatus,
  Probe,
  ProviderRow,
  RunSummary,
} from "@/api/types.gen";

const STATUS_TONE: Record<string, "gold" | "warn" | "danger" | "default" | "info"> = {
  completed: "gold",
  running: "info",
  queued: "default",
  failed: "danger",
  cancelled: "warn",
};

export function HomeRoute() {
  const health = useQuery({ queryKey: healthKeys.report(), queryFn: getHealth });
  const runs = useQuery({ queryKey: evalKeys.runs(), queryFn: listRuns });
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
  const identity = useQuery({
    queryKey: settingsKeys.identity(),
    queryFn: getIdentity,
  });

  const attention = buildAttention({
    health: health.data,
    runs: runs.data,
    providers: providers.data?.providers,
    brokers: brokers.data,
  });

  const recent = (runs.data ?? []).slice(0, 5);
  const latestRunId = recent[0]?.id ?? "";
  const latestChart = useQuery({
    queryKey: chartKeys.run(latestRunId),
    queryFn: () => getRunChart(latestRunId),
    enabled: !!latestRunId,
  });
  const strategyCount = strategies.data?.length ?? 0;
  const agentCount = agents.data?.length ?? 0;
  const identityNote = identity.data?.note;
  const identityActive = identity.data?.feature_compiled_in ?? false;
  const defaultProvider = providers.data?.providers.find(
    (p) => p.is_default,
  );

  return (
    <>
      <Topbar
        title="Control Tower"
        sub="paper · localhost · workspace status at a glance"
      />

      <div className="grid grid-cols-12 gap-5">
        <div className="col-span-12 lg:col-span-8 space-y-5">
          <AttentionCard
            items={attention}
            loading={
              health.isPending ||
              runs.isPending ||
              providers.isPending ||
              brokers.isPending
            }
          />

          <RecentRunsCard
            runs={recent}
            loading={runs.isPending}
            error={runs.error}
          />

          <ControlChartCard
            hasRuns={recent.length > 0}
            loadingRuns={runs.isPending}
            loadingChart={latestChart.isPending}
            chartError={latestChart.error}
            chart={latestChart.data}
          />
        </div>

        <div className="col-span-12 lg:col-span-4 space-y-5">
          <HealthCard report={health.data} loading={health.isPending} />

          <CountCard
            label="Strategies"
            value={strategies.isPending ? "…" : String(strategyCount)}
            link={{ to: "/strategies", label: "manage" }}
            sub={
              strategyCount === 0
                ? "draft your first bundle in the Setup wizard"
                : strategies.data?.[0]?.template
            }
          />

          <CountCard
            label="Agents"
            value={agents.isPending ? "…" : String(agentCount)}
            link={{
              to: agentCount === 0 ? "/agents/new" : "/agents",
              label: agentCount === 0 ? "create" : "manage",
            }}
            sub={
              agentCount === 0
                ? "reusable templates that compose into strategies"
                : agents.data?.[0]?.name
            }
          />

          <CountCard
            label="Providers"
            value={
              providers.isPending
                ? "…"
                : String(providers.data?.providers.length ?? 0)
            }
            link={{ to: "/settings/providers", label: "configure" }}
            sub={defaultProvider ? `default → ${defaultProvider.name}` : undefined}
          />

          <Card className="p-5">
            <div className="text-text-3 text-[11px] uppercase tracking-wider mb-2">
              On-chain identity
            </div>
            <div className="flex items-center gap-2">
              {identityActive ? (
                <Pill tone="gold">compiled in</Pill>
              ) : (
                <Pill>not compiled</Pill>
              )}
            </div>
            {identityNote ? (
              <p className="m-0 mt-2 text-text-3 text-[11px] leading-snug">
                {identityNote}
              </p>
            ) : null}
          </Card>
        </div>
      </div>
    </>
  );
}

// ─── cards ─────────────────────────────────────────────────────────────────

function AttentionCard({
  items,
  loading,
}: {
  items: AttentionItem[];
  loading: boolean;
}) {
  return (
    <Card className="p-5">
      <div className="flex items-baseline justify-between mb-3">
        <h2 className="m-0 font-serif font-medium text-[24px] tracking-tight">
          Needs attention
        </h2>
        <span className="text-[12px] text-text-3">
          {loading
            ? "checking…"
            : items.length === 0
              ? "all clear"
              : `${items.length} item${items.length === 1 ? "" : "s"}`}
        </span>
      </div>

      {loading ? (
        <div className="space-y-2">
          <div className="h-4 w-64 bg-surface-elev rounded animate-pulse" />
          <div className="h-4 w-48 bg-surface-elev rounded animate-pulse" />
        </div>
      ) : items.length === 0 ? (
        <p className="m-0 text-text-2 text-[13px] leading-snug">
          Health probes are green, no failed runs in the recent window, every
          declared provider has its API-key env var set, and broker credentials
          are present for the configured executor. You're good to draft a
          bundle.
        </p>
      ) : (
        <ul className="m-0 p-0 list-none space-y-2">
          {items.map((it, i) => (
            <li
              key={i}
              className="flex items-start gap-3 border-t border-border-soft first:border-t-0 pt-2 first:pt-0"
            >
              <span className="mt-1">
                <Pill tone={it.tone}>
                  {it.tone === "danger" ? "blocking" : it.tone === "warn" ? "warn" : "info"}
                </Pill>
              </span>
              <div className="flex-1 min-w-0">
                <div className="text-text text-[13px]">{it.title}</div>
                <div className="text-text-3 text-[12px] mt-0.5 leading-snug">
                  {it.detail}
                </div>
              </div>
              {it.link ? (
                <Link
                  to={it.link.to}
                  className="text-[12px] text-text-2 hover:text-text whitespace-nowrap pt-0.5"
                >
                  {it.link.label} →
                </Link>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}

function RecentRunsCard({
  runs,
  loading,
  error,
}: {
  runs: RunSummary[];
  loading: boolean;
  error: unknown;
}) {
  return (
    <Card className="p-5">
      <div className="flex items-baseline justify-between mb-3">
        <h2 className="m-0 font-serif font-medium text-[24px] tracking-tight">
          Recent runs
        </h2>
        <Link
          to="/eval-runs"
          className="text-[12px] text-text-2 hover:text-text"
        >
          all runs →
        </Link>
      </div>

      {loading ? (
        <div className="space-y-2">
          <div className="h-5 bg-surface-elev rounded animate-pulse" />
          <div className="h-5 w-3/4 bg-surface-elev rounded animate-pulse" />
        </div>
      ) : error ? (
        <p className="m-0 text-danger text-[13px]">
          <code className="font-mono text-[12px]">{errorMessage(error)}</code>
        </p>
      ) : runs.length === 0 ? (
        <p className="m-0 text-text-2 text-[13px] leading-snug">
          No runs yet. Draft a bundle in{" "}
          <Link to="/setup" className="text-gold hover:underline">
            Setup
          </Link>
          , validate it from the{" "}
          <Link to="/strategies" className="text-gold hover:underline">
            Strategies
          </Link>{" "}
          page, then kick off an eval.
        </p>
      ) : (
        <table className="w-full">
          <thead>
            <tr className="text-text-3 text-[11px] uppercase tracking-wider text-left">
              <th className="py-1.5 pr-3 font-normal">Run</th>
              <th className="py-1.5 pr-3 font-normal">Scenario</th>
              <th className="py-1.5 pr-3 font-normal text-right">Sharpe</th>
              <th className="py-1.5 pr-3 font-normal text-right">Return</th>
              <th className="py-1.5 pr-0 font-normal text-right">Status</th>
            </tr>
          </thead>
          <tbody>
            {runs.map((r) => (
              <tr key={r.id} className="border-t border-border-soft">
                <td className="py-2 pr-3">
                  <Link
                    to={`/eval-runs/${r.id}`}
                    className="font-mono text-[12px] text-text hover:text-gold"
                  >
                    {r.id.slice(0, 10)}…
                  </Link>
                </td>
                <td className="py-2 pr-3 text-text-2 text-[12px]">
                  {r.scenario_id}
                </td>
                <td className="py-2 pr-3 text-right font-mono text-[12px]">
                  {fmtNumOrDash(r.sharpe, 2)}
                </td>
                <td
                  className={`py-2 pr-3 text-right font-mono text-[12px] ${returnTone(r.total_return_pct)}`}
                >
                  {fmtPctOrDash(r.total_return_pct)}
                </td>
                <td className="py-2 pr-0 text-right">
                  <Pill tone={STATUS_TONE[r.status] ?? "default"}>
                    {r.status}
                  </Pill>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Card>
  );
}

function ControlChartCard({
  hasRuns,
  loadingRuns,
  loadingChart,
  chartError,
  chart,
}: {
  hasRuns: boolean;
  loadingRuns: boolean;
  loadingChart: boolean;
  chartError: unknown;
  chart: Parameters<typeof RunChart>[0]["payload"] | undefined;
}) {
  return (
    <Card className="p-5">
      <div className="flex items-baseline justify-between mb-3">
        <h2 className="m-0 font-serif font-medium text-[24px] tracking-tight">
          Chart snapshot
        </h2>
        <Link
          to="/eval-runs"
          className="text-[12px] text-text-2 hover:text-text"
        >
          open eval →
        </Link>
      </div>
      {loadingRuns ? (
        <div className="text-text-3 text-[13px] text-center py-6">
          Loading runs…
        </div>
      ) : !hasRuns ? (
        <div className="text-text-3 text-[13px] text-center py-6">
          No runs yet. Start one from Eval.
        </div>
      ) : loadingChart ? (
        <div className="text-text-3 text-[13px] text-center py-6">
          Loading chart…
        </div>
      ) : chartError ? (
        <div className="text-danger text-[13px] text-center py-6">
          Chart unavailable.
        </div>
      ) : chart ? (
        <RunChart payload={chart} />
      ) : null}
    </Card>
  );
}

function HealthCard({
  report,
  loading,
}: {
  report: HealthReport | undefined;
  loading: boolean;
}) {
  return (
    <Card className="p-5">
      <div className="text-text-3 text-[11px] uppercase tracking-wider mb-2">
        Local health
      </div>
      {loading || !report ? (
        <div className="space-y-1.5">
          <div className="h-4 w-32 bg-surface-elev rounded animate-pulse" />
          <div className="h-4 w-24 bg-surface-elev rounded animate-pulse" />
        </div>
      ) : (
        <>
          <div className="mb-3 flex items-center gap-2">
            <HealthDot status={report.status} />
            <span className="text-[13px] text-text capitalize">
              {report.status}
            </span>
          </div>
          <ul className="m-0 p-0 list-none space-y-1">
            {report.probes.map((p) => (
              <ProbeRow key={p.name} probe={p} />
            ))}
          </ul>
        </>
      )}
    </Card>
  );
}

function ProbeRow({ probe }: { probe: Probe }) {
  return (
    <li className="flex items-center justify-between gap-2 text-[12px]">
      <span className="flex items-center gap-2 text-text-2">
        <HealthDot status={probe.status} />
        <code className="font-mono text-text-2">{probe.name}</code>
      </span>
      <span className="text-text-3 truncate">
        {probe.latency_ms != null ? `${probe.latency_ms}ms` : probe.detail ?? ""}
      </span>
    </li>
  );
}

function HealthDot({ status }: { status: HealthStatus }) {
  const cls =
    status === "ok"
      ? "bg-gold"
      : status === "degraded"
        ? "bg-warn"
        : "bg-danger";
  return <span className={`inline-block w-1.5 h-1.5 rounded-full ${cls}`} />;
}
function CountCard({
  label,
  value,
  link,
  sub,
}: {
  label: string;
  value: string;
  link: { to: string; label: string };
  sub?: string;
}) {
  return (
    <Card className="p-5">
      <div className="flex items-baseline justify-between mb-1">
        <div className="text-text-3 text-[11px] uppercase tracking-wider">
          {label}
        </div>
        <Link to={link.to} className="text-[12px] text-text-2 hover:text-text">
          {link.label} →
        </Link>
      </div>
      <div className="font-serif text-[36px] tracking-tight leading-none text-text">
        {value}
      </div>
      {sub ? (
        <div className="text-text-3 text-[12px] mt-2 truncate">{sub}</div>
      ) : null}
    </Card>
  );
}

// ─── attention rollup ──────────────────────────────────────────────────────

type AttentionItem = {
  tone: "warn" | "danger" | "info";
  title: string;
  detail: string;
  link?: { to: string; label: string };
};

function buildAttention(input: {
  health: HealthReport | undefined;
  runs: RunSummary[] | undefined;
  providers: ProviderRow[] | undefined;
  brokers: BrokersReport | undefined;
}): AttentionItem[] {
  const out: AttentionItem[] = [];

  for (const p of input.health?.probes ?? []) {
    if (p.status === "ok") continue;
    out.push({
      tone: p.status === "down" ? "danger" : "warn",
      title: `Probe ${p.name} ${p.status === "down" ? "is down" : "is degraded"}`,
      detail: p.detail ?? "(no detail)",
    });
  }

  const failed = (input.runs ?? []).filter((r) => r.status === "failed");
  if (failed.length > 0) {
    out.push({
      tone: "danger",
      title: `${failed.length} failed eval run${failed.length === 1 ? "" : "s"}`,
      detail:
        failed[0].error ??
        `latest: ${failed[0].scenario_id} (${failed[0].id.slice(0, 10)}…)`,
      link: { to: "/eval-runs", label: "review" },
    });
  }

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

// ─── helpers ───────────────────────────────────────────────────────────────

function fmtPctOrDash(v: number | null | undefined): string {
  if (v == null) return "—";
  const sign = v > 0 ? "+" : "";
  return `${sign}${v.toFixed(2)}%`;
}

function fmtNumOrDash(v: number | null | undefined, digits: number): string {
  if (v == null) return "—";
  return v.toFixed(digits);
}

function returnTone(v: number | null | undefined): string {
  if (v == null) return "text-text-3";
  if (v > 0) return "text-gold";
  if (v < 0) return "text-danger";
  return "text-text-2";
}

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
