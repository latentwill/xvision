import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";

import { ApiError } from "@/api/client";
import { downloadEvalRunExport } from "@/api/eval";
import { ReviewPanel } from "@/features/eval-runs/review";
import { RunSummaryError as RunSummaryPanel } from "@/features/eval-runs/RunSummary";
import {
  derivePriorSideByDecision,
  type PositionSide,
} from "@/features/decisions/positions";
import { isInflightRunStatus } from "@/lib/run-status";
import type { EvalRunLabels } from "@/lib/run-display";
import { formatCostUsd } from "@/lib/format";
import { drawdownMetricTone } from "@/lib/metric-tone";
import type { Agent } from "@/api/agents";
import type {
  DecisionRowDto,
  EquityPoint,
  RunDetail,
  RunSummary,
} from "@/api/types.gen";

// ── design constants (mirrors docs/design/mobile/XVN/mobile-screens.jsx) ──

type Tab = "SUMMARY" | "DECISIONS" | "TRACE" | "REVIEW";
type StripState = "blue" | "green" | "amber" | "red";

const TABS: Tab[] = ["SUMMARY", "DECISIONS", "TRACE", "REVIEW"];

const STRIP: Record<StripState, { dot: string; label: string; ring: string; bg: string; bd: string }> = {
  blue: {
    dot: "var(--info)",
    label: "LIVE",
    ring: "0 0 0 3px rgba(111,143,184,0.25)",
    bg: "rgba(111,143,184,0.06)",
    bd: "rgba(111,143,184,0.25)",
  },
  green: {
    dot: "var(--gold)",
    label: "COMPLETED",
    ring: "0 0 0 3px var(--gold-bg)",
    bg: "transparent",
    bd: "var(--border-soft)",
  },
  amber: {
    dot: "var(--warn)",
    label: "WARN",
    ring: "0 0 0 3px rgba(219,146,48,0.18)",
    bg: "rgba(219,146,48,0.06)",
    bd: "rgba(219,146,48,0.25)",
  },
  red: {
    dot: "var(--danger)",
    label: "ERROR",
    ring: "0 0 0 3px rgba(200,68,58,0.22)",
    bg: "rgba(200,68,58,0.06)",
    bd: "rgba(200,68,58,0.30)",
  },
};

const MONO_TINY = "font-mono text-[9px] tracking-[0.18em]";
const MONO_LBL = "font-mono text-[10px] tracking-[0.18em]";

// ── main entry ────────────────────────────────────────────────────────

export function MobileEvalRunDetail({
  detail,
  labels,
  disambiguator,
  agents,
  agentsAll,
  totalCostUsd,
  onCancel,
  cancelling,
  onRetry,
  retrying,
  onDelete,
  deleting,
}: {
  detail: RunDetail;
  labels: EvalRunLabels;
  disambiguator: string;
  agents: { agent_id: string; role: string }[];
  agentsAll: Agent[];
  totalCostUsd: number | null;
  onCancel: () => void;
  cancelling: boolean;
  onRetry: () => void;
  retrying: boolean;
  onDelete: () => void;
  deleting: boolean;
}) {
  const { summary } = detail;
  const isLive = isInflightRunStatus(summary.status);
  const stripState = mapStripState(summary.status);
  const liveDuration = useLiveDuration(summary);
  const [tab, setTab] = useState<Tab>(() =>
    summary.status === "failed" || summary.status === "cancelled"
      ? "SUMMARY"
      : "SUMMARY",
  );

  return (
    <div className="-mx-4 -mt-4 flex flex-col min-h-0">
      <Link
        to="/eval-runs"
        data-testid="mobile-detail-back"
        className="px-4 pt-3 pb-1 inline-flex items-center gap-1 text-[12px] text-text-3 hover:text-text transition-colors"
      >
        <span aria-hidden>←</span>
        <span>Back to runs</span>
      </Link>
      <LiveStrip
        state={stripState}
        isLive={isLive}
        liveDuration={liveDuration}
        summary={summary}
        labels={labels}
        onHalt={onCancel}
        cancelling={cancelling}
      />
      <TabBar tabs={TABS} active={tab} onChange={setTab} />
      <div className="flex-1 min-h-0 px-3">
        {tab === "SUMMARY" && (
          <SummaryTab
            detail={detail}
            labels={labels}
            disambiguator={disambiguator}
            agents={agents}
            agentsAll={agentsAll}
            totalCostUsd={totalCostUsd}
            isLive={isLive}
            liveDuration={liveDuration}
            onRetry={onRetry}
            retrying={retrying}
            onDelete={onDelete}
            deleting={deleting}
          />
        )}
        {tab === "DECISIONS" && <DecisionsTab decisions={detail.decisions} />}
        {tab === "TRACE" && (
          <TraceTab summary={summary} labels={labels} spanCount={detail.decisions.length} />
        )}
        {tab === "REVIEW" && (
          <ReviewTab summary={summary} />
        )}
      </div>
    </div>
  );
}

// ── LIVE strip (sticky) ──────────────────────────────────────────────

function LiveStrip({
  state,
  isLive,
  liveDuration,
  summary,
  labels,
  onHalt,
  cancelling,
}: {
  state: StripState;
  isLive: boolean;
  liveDuration: number;
  summary: RunSummary;
  labels: EvalRunLabels;
  onHalt: () => void;
  cancelling: boolean;
}) {
  const conf = STRIP[state];
  const totalDur = totalDurationLabel(summary, liveDuration);
  return (
    <div
      className="flex items-center gap-2 px-3 min-h-[34px] py-1.5 border-b flex-shrink-0 overflow-hidden sticky top-0 z-10 backdrop-blur"
      style={{ background: conf.bg, borderColor: conf.bd }}
    >
      <span
        className={isLive ? "animate-pulse" : ""}
        style={{
          width: 6,
          height: 6,
          borderRadius: 6,
          background: conf.dot,
          boxShadow: conf.ring,
          flexShrink: 0,
        }}
      />
      <span
        className={`${MONO_TINY} flex-shrink-0`}
        style={{ color: conf.dot }}
      >
        {conf.label}
      </span>
      <span className="font-mono text-[11px] text-text-3 tabular-nums flex-shrink-0">
        {totalDur}
      </span>
      <span
        className="w-px h-3 flex-shrink-0"
        style={{ background: "var(--border)" }}
      />
      <span className="flex-1 min-w-0 font-mono text-[11px] text-text truncate">
        <span className="text-text-3">EVAL&nbsp;·&nbsp;</span>
        {labels.strategyName}
      </span>
      {isLive && (
        <button
          type="button"
          onClick={onHalt}
          disabled={cancelling}
          aria-label={`Halt eval run ${summary.id}`}
          className={`${MONO_TINY} h-[22px] px-2 rounded-sm flex-shrink-0 disabled:opacity-50`}
          style={{
            color: "var(--danger)",
            fontWeight: 600,
            background: "rgba(200,68,58,0.10)",
            border: "1px solid rgba(200,68,58,0.55)",
          }}
        >
          ◼ {cancelling ? "STOPPING" : "HALT"}
        </button>
      )}
    </div>
  );
}

// ── tab bar (sticky) ─────────────────────────────────────────────────

function TabBar({
  tabs,
  active,
  onChange,
}: {
  tabs: Tab[];
  active: Tab;
  onChange: (t: Tab) => void;
}) {
  return (
    <div
      role="tablist"
      aria-label="Eval run sections"
      className="flex px-1 border-b border-border-soft bg-bg flex-shrink-0 sticky top-[34px] z-10"
    >
      {tabs.map((t) => {
        const on = t === active;
        return (
          <button
            key={t}
            role="tab"
            aria-selected={on}
            type="button"
            onClick={() => onChange(t)}
            className={`flex-1 py-3 px-1 ${MONO_LBL} bg-transparent border-b-2 -mb-px ${
              on
                ? "text-gold border-gold font-semibold"
                : "text-text-3 border-transparent"
            }`}
          >
            {t}
          </button>
        );
      })}
    </div>
  );
}

// ── SUMMARY tab ──────────────────────────────────────────────────────

function SummaryTab({
  detail,
  labels,
  disambiguator,
  agents,
  agentsAll,
  totalCostUsd,
  isLive,
  liveDuration,
  onRetry,
  retrying,
  onDelete,
  deleting,
}: {
  detail: RunDetail;
  labels: EvalRunLabels;
  disambiguator: string;
  agents: { agent_id: string; role: string }[];
  agentsAll: Agent[];
  totalCostUsd: number | null;
  isLive: boolean;
  liveDuration: number;
  onRetry: () => void;
  retrying: boolean;
  onDelete: () => void;
  deleting: boolean;
}) {
  const { summary, decisions, equity_curve } = detail;
  return (
    <div className="flex flex-col gap-3 py-3 pb-24">
      {/* Hero */}
      <div>
        <div className="font-serif italic text-[28px] leading-none text-text font-medium">
          {labels.strategyName}
        </div>
        <div
          data-testid="mobile-eval-run-id"
          className="mt-1 font-mono text-[11px] text-text-3 break-all select-all"
          aria-label={`Eval run id ${summary.id}`}
        >
          {summary.id}
        </div>
        <div className="text-[14px] text-text-2 mt-1 truncate">
          {labels.scenarioName}
        </div>
        <div
          data-testid="mobile-eval-run-meta"
          className="mt-1.5 flex flex-wrap gap-x-2 font-mono text-[10px] text-text-3"
        >
          <span>{summary.mode}</span>
          <span className="text-text-4">·</span>
          <span>{summary.status}</span>
          <span className="text-text-4">·</span>
          <span className="text-text-2">{disambiguator}</span>
        </div>
      </div>

      <MobileContextStrip
        strategyId={summary.agent_id}
        strategyName={labels.strategyName}
        scenarioId={summary.scenario_id}
        scenarioName={labels.scenarioName}
        agents={agents}
        agentsAll={agentsAll}
      />

      {isLive && (
        <ActivityCard
          liveDuration={liveDuration}
          tokensIn={summary.actual_input_tokens}
        />
      )}

      <RunSummaryPanel error={summary.error} />

      {/* KPI grid */}
      <div className="grid grid-cols-2 gap-2">
        <Stat
          label="PNL"
          value={fmtPnlUsd(totalPnlUsd(equity_curve))}
          sub={`${fmtPct(summary.total_return_pct)}${summary.completed_at ? ` · as of ${fmtDate(summary.completed_at)}` : " · in progress"}`}
          tone={pctTone(summary.total_return_pct)}
        />
        <Stat
          label="MAX DD"
          value={fmtPct(summary.max_drawdown_pct)}
          sub={fmtTokensSub(summary)}
          tone={drawdownMetricTone(summary.max_drawdown_pct)}
        />
        <Stat
          label="SHARPE"
          value={fmtNumber(summary.sharpe)}
          sub="annualized"
        />
        <Stat
          label="WIN RATE"
          value={winRate(decisions).value}
          sub={winRate(decisions).sub}
        />
        <Stat
          label="COST"
          value={formatCostUsd(totalCostUsd)}
          sub="inference"
        />
      </div>

      <EquityCard equity={equity_curve} pct={summary.total_return_pct} />

      <MetaCard summary={summary} labels={labels} />

      <RunActions
        summary={summary}
        onRetry={onRetry}
        retrying={retrying}
        onDelete={onDelete}
        deleting={deleting}
      />
    </div>
  );
}

function Stat({
  label,
  value,
  sub,
  tone,
}: {
  label: string;
  value: string;
  sub?: string;
  tone?: "pos" | "neg" | "gold";
}) {
  const color =
    tone === "pos"
      ? "text-[#7ab97c]"
      : tone === "neg"
        ? "text-danger"
        : tone === "gold"
          ? "text-gold"
          : "text-text";
  return (
    <div className="rounded-card border border-border bg-surface px-3 py-3">
      <div className={`${MONO_TINY} text-text-3`}>{label}</div>
      <div
        className={`mt-1 font-mono text-[22px] font-medium tabular-nums leading-none ${color}`}
      >
        {value}
      </div>
      {sub && (
        <div className={`mt-1 ${MONO_TINY} text-text-3 tabular-nums`}>{sub}</div>
      )}
    </div>
  );
}

function ActivityCard({
  liveDuration,
  tokensIn,
}: {
  liveDuration: number;
  tokensIn: number | null;
}) {
  return (
    <div
      className="rounded-card px-3 py-3 relative overflow-hidden"
      style={{
        background: "rgba(111,143,184,0.06)",
        border: "1px solid rgba(111,143,184,0.30)",
      }}
    >
      <div className="flex items-center gap-1.5 mb-2">
        <span
          className="animate-pulse"
          style={{
            width: 5,
            height: 5,
            borderRadius: 5,
            background: "var(--info)",
            boxShadow: "0 0 0 3px rgba(111,143,184,0.25)",
          }}
        />
        <span className={`${MONO_TINY} text-info`}>
          CURRENTLY · {liveDuration}s
        </span>
      </div>
      <div className="font-mono text-[11px] text-text-3">
        {tokensIn != null ? `${tokensIn.toLocaleString()} tok in` : "streaming"}
        <span className="mx-1">·</span>
        <span className="text-info">live</span>
      </div>
      <div
        className="mt-2 h-[3px] rounded-sm overflow-hidden"
        style={{ background: "var(--border)" }}
      >
        <div
          className="h-full"
          style={{
            width: "62%",
            background: "linear-gradient(90deg, var(--info) 0%, var(--gold) 100%)",
          }}
        />
      </div>
    </div>
  );
}

function MobileContextStrip({
  strategyId,
  strategyName,
  scenarioId,
  scenarioName,
  agents,
  agentsAll,
}: {
  strategyId: string;
  strategyName: string;
  scenarioId: string;
  scenarioName: string;
  agents: { agent_id: string; role: string }[];
  agentsAll: Agent[];
}) {
  const agentNameById = new Map(agentsAll.map((a) => [a.agent_id, a.name]));
  return (
    <div
      data-testid="mobile-eval-inspector-context-strip"
      className="flex flex-wrap items-center gap-1.5 rounded-card border border-border-soft bg-surface px-2.5 py-2"
    >
      <MobileContextPill
        kind="Strategy"
        to={`/strategies/${encodeURIComponent(strategyId)}`}
        label={strategyName}
        idForAria={strategyId}
      />
      <MobileContextPill
        kind="Scenario"
        to={`/scenarios/${encodeURIComponent(scenarioId)}`}
        label={scenarioName}
        idForAria={scenarioId}
      />
      {agents.map((ref) => (
        <MobileContextPill
          key={`${ref.agent_id}:${ref.role}`}
          kind={ref.role}
          to={`/agents/${encodeURIComponent(ref.agent_id)}`}
          label={agentNameById.get(ref.agent_id) ?? ref.agent_id}
          idForAria={ref.agent_id}
        />
      ))}
    </div>
  );
}

function MobileContextPill({
  kind,
  to,
  label,
  idForAria,
}: {
  kind: string;
  to: string;
  label: string;
  idForAria: string;
}) {
  return (
    <Link
      to={to}
      aria-label={`Open ${kind} ${label} (${idForAria})`}
      className="inline-flex max-w-full items-center gap-1 rounded-sm border border-border-soft px-2 py-1 font-mono text-[10px] text-text-2 hover:border-gold/50 hover:text-text"
    >
      <span className="uppercase tracking-[0.16em] text-text-3">{kind}</span>
      <span className="break-all">{label}</span>
    </Link>
  );
}

function MetaCard({
  summary,
  labels,
}: {
  summary: RunSummary;
  labels: EvalRunLabels;
}) {
  const rows: [string, string][] = [
    ["strategy", labels.strategyName],
    ["scenario", labels.scenarioName],
    ["run id", summary.id],
    ["mode", summary.mode],
    ["started", fmtTime(summary.started_at)],
    [
      "completed",
      summary.completed_at ? fmtTime(summary.completed_at) : "—",
    ],
    ["tokens", fmtTokensTotal(summary)],
  ];
  return (
    <div className="rounded-card border border-border bg-surface px-3 py-2.5">
      <div className={`${MONO_TINY} text-text-3 mb-1.5`}>META</div>
      {rows.map(([k, v]) => (
        <div
          key={k}
          className="flex py-1 border-b border-border-soft last:border-b-0 font-mono text-[11px]"
        >
          <span
            className={`w-[78px] text-text-3 ${MONO_TINY} uppercase pt-0.5 flex-shrink-0`}
          >
            {k}
          </span>
          <span className="flex-1 text-text tabular-nums break-all">{v}</span>
        </div>
      ))}
    </div>
  );
}

function RunActions({
  summary,
  onRetry,
  retrying,
  onDelete,
  deleting,
}: {
  summary: RunSummary;
  onRetry: () => void;
  retrying: boolean;
  onDelete: () => void;
  deleting: boolean;
}) {
  // Cancelled runs are eligible for retry alongside failed runs — see
  // the desktop SummaryCard comment for the rationale.
  const canRetry =
    summary.status === "failed" || summary.status === "cancelled";
  const terminal = isTerminalStatus(summary.status);
  const [downloading, setDownloading] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);

  async function handleDownload() {
    setDownloadError(null);
    setDownloading(true);
    try {
      await downloadEvalRunExport(summary.id);
    } catch (err) {
      setDownloadError(err instanceof Error ? err.message : String(err));
    } finally {
      setDownloading(false);
    }
  }

  return (
    <div className="flex flex-col gap-2">
      {/*
        `min-w-[16ch]` on each button gives a column floor that matches
        the widest natural label ("Preparing JSON…" with padding) so
        Retry / Download render at the same visual width on mobile.
        Replaces a `grid auto-cols-fr` shell that didn't actually
        equalize widths in the unconstrained inline-grid case. See
        `qa-eval-inspector-buttons-actually-uniform`.
      */}
      <div className="flex items-center gap-2">
        {canRetry && (
          <button
            type="button"
            aria-label={`Retry eval run ${summary.id}`}
            onClick={onRetry}
            disabled={retrying}
            className="min-w-[16ch] rounded-sm border border-info/40 bg-info/[0.08] px-2.5 py-1.5 text-[12px] text-info hover:border-info/70 hover:text-text disabled:opacity-50"
          >
            {retrying ? "Retrying..." : "Retry"}
          </button>
        )}
        {terminal && (
          <button
            type="button"
            aria-label={`Download eval run ${summary.id} as JSON`}
            onClick={handleDownload}
            disabled={downloading}
            className="min-w-[16ch] rounded-sm border border-border-soft bg-surface-elev px-2.5 py-1.5 text-[12px] text-text-2 hover:border-gold/40 hover:text-text disabled:opacity-50"
          >
            {downloading ? "Preparing JSON…" : "Download JSON"}
          </button>
        )}
        <button
          type="button"
          aria-label={`Delete eval run ${summary.id}`}
          onClick={onDelete}
          disabled={deleting}
          className="min-w-[16ch] rounded-sm border border-danger/40 bg-danger/[0.06] px-2.5 py-1.5 text-[12px] text-danger hover:border-danger/70 hover:bg-danger/[0.12] hover:text-text disabled:opacity-50"
        >
          {deleting ? "Deleting…" : "Delete"}
        </button>
      </div>
      {downloadError && (
        <div className="rounded-sm border border-danger/30 bg-danger/[0.06] px-2 py-1 text-[12px] text-danger">
          Download failed: {downloadError}
        </div>
      )}
    </div>
  );
}

// ── Equity sparkline ────────────────────────────────────────────────

function EquityCard({
  equity,
  pct,
}: {
  equity: EquityPoint[];
  pct: number | null;
}) {
  if (equity.length < 2) {
    return (
      <div className="rounded-card border border-border bg-surface px-3 py-6 text-center text-text-3 text-[12px]">
        No equity data yet.
      </div>
    );
  }
  const W = 100;
  const H = 60;
  const values = equity.map((p) => p.equity_usd);
  const min = Math.min(...values);
  const max = Math.max(...values);
  const range = max - min || 1;
  const path = equity
    .map((p, i) => {
      const x = (i / (equity.length - 1)) * W;
      const y = H - ((p.equity_usd - min) / range) * H;
      return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${y.toFixed(2)}`;
    })
    .join(" ");

  return (
    <div className="relative rounded-card border border-border bg-surface overflow-hidden h-[96px]">
      <svg
        viewBox={`0 0 ${W} ${H}`}
        preserveAspectRatio="none"
        className="absolute inset-0 w-full h-full"
        role="img"
        aria-label="Equity curve"
      >
        <defs>
          <linearGradient id="mEvalRunEquityGrad" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#d4a547" stopOpacity="0.5" />
            <stop offset="100%" stopColor="#d4a547" stopOpacity="0" />
          </linearGradient>
        </defs>
        <path
          d={path}
          fill="none"
          stroke="#d4a547"
          strokeWidth="0.9"
          vectorEffect="non-scaling-stroke"
        />
        <path d={`${path} L${W},${H} L0,${H} Z`} fill="url(#mEvalRunEquityGrad)" opacity="0.3" />
      </svg>
      <div className={`absolute top-2 left-3 ${MONO_TINY} text-text-3`}>
        EQUITY · pnl%
      </div>
      <div className="absolute bottom-2 right-3 font-mono text-[10px] text-gold tabular-nums">
        {fmtPct(pct)}
      </div>
    </div>
  );
}

// ── DECISIONS tab ──────────────────────────────────────────────────

function DecisionsTab({ decisions }: { decisions: DecisionRowDto[] }) {
  const tradeCount = useMemo(
    () => decisions.filter((d) => d.pnl_realized != null).length,
    [decisions],
  );
  // Same prior-side derivation as the desktop view — drives the
  // direction-aware action label on each card (SELL vs COVER for a
  // `flat`, SHORT vs BUY for an open).
  const priorSideByDecision = useMemo(
    () => derivePriorSideByDecision(decisions),
    [decisions],
  );
  if (decisions.length === 0) {
    return (
      <div className="py-10 text-center text-text-2">
        <div className="font-serif italic text-[20px] text-text-3 mb-1">
          no decisions
        </div>
        <p className="m-0 text-[12.5px]">
          This run hasn't recorded any decisions yet.
        </p>
      </div>
    );
  }
  return (
    <div className="flex flex-col gap-2 py-3 pb-24">
      <div className={`${MONO_TINY} text-text-3 px-1`}>
        {decisions.length} STEPS · {tradeCount} TRADES
      </div>
      {decisions.map((d) => (
        <DecisionCard
          key={d.decision_index}
          d={d}
          priorSide={priorSideByDecision.get(d.decision_index) ?? "flat"}
        />
      ))}
    </div>
  );
}

function DecisionCard({ d, priorSide }: { d: DecisionRowDto; priorSide: PositionSide }) {
  const action = actionLabel(d.action, priorSide);
  const pnl = d.pnl_realized;
  const conviction = clamp01(d.conviction);
  return (
    <div className="rounded-card border border-border bg-surface px-3 py-2.5">
      <div className="flex items-center gap-2">
        <span className="font-mono text-[11px] text-text-3 tabular-nums font-medium">
          #{d.decision_index}
        </span>
        <ActionPill action={action} />
        <span className="font-mono text-[10px] text-text-3 tabular-nums">
          {fmtTime(d.timestamp)}
        </span>
        <span
          className={`ml-auto font-mono text-[11px] tabular-nums ${pnlClass(pnl)}`}
        >
          {pnl == null
            ? "—"
            : pnl > 0
              ? `+$${pnl.toFixed(2)}`
              : `−$${Math.abs(pnl).toFixed(2)}`}
        </span>
      </div>
      <div className="mt-2 flex items-center gap-1.5">
        <span
          className={`${MONO_TINY} text-text-4 w-[44px]`}
        >
          CONV
        </span>
        <span className="font-mono text-[10px] text-text tabular-nums w-[34px]">
          {Math.round(conviction * 100)}%
        </span>
        <span
          className="flex-1 h-[3px] rounded-sm overflow-hidden"
          style={{ background: "var(--border)" }}
        >
          <span
            className="block h-full bg-gold"
            style={{ width: `${conviction * 100}%` }}
          />
        </span>
      </div>
      <div className="mt-1.5 text-[12px] text-text-2 leading-snug">
        {decisionReasoning(d)}
      </div>
    </div>
  );
}

type MobileActionLabel = "BUY" | "SHORT" | "SELL" | "COVER" | "HOLD";

function ActionPill({ action }: { action: MobileActionLabel }) {
  const styles: Record<MobileActionLabel, { color: string; bg: string; bd: string }> = {
    BUY: { color: "var(--gold)", bg: "var(--gold-bg)", bd: "var(--gold-soft)" },
    SHORT: {
      color: "var(--danger)",
      bg: "rgba(200,68,58,0.10)",
      bd: "rgba(200,68,58,0.45)",
    },
    SELL: {
      color: "var(--warn)",
      bg: "rgba(212,165,71,0.10)",
      bd: "rgba(212,165,71,0.45)",
    },
    COVER: {
      color: "var(--info)",
      bg: "rgba(111,143,184,0.10)",
      bd: "rgba(111,143,184,0.45)",
    },
    HOLD: { color: "var(--text-3)", bg: "transparent", bd: "var(--border)" },
  };
  const s = styles[action];
  return (
    <span
      className={`px-1.5 py-[1px] ${MONO_TINY} rounded-sm`}
      style={{ color: s.color, background: s.bg, border: `1px solid ${s.bd}` }}
    >
      {action}
    </span>
  );
}

// ── TRACE tab ──────────────────────────────────────────────────────

function TraceTab({
  summary,
  labels,
  spanCount,
}: {
  summary: RunSummary;
  labels: EvalRunLabels;
  spanCount: number;
}) {
  const agentRunId = traceRunId(summary);
  return (
    <div className="flex flex-col gap-3 py-3 pb-24">
      <div className={`${MONO_TINY} text-text-3`}>
        TRACE · {labels.strategyName}
      </div>
      <div
        className="rounded-card border border-border bg-surface px-3 py-3"
        style={{ borderColor: "rgba(111,143,184,0.30)" }}
      >
        <div className="flex items-center gap-1.5 mb-1.5">
          <span
            style={{
              width: 5,
              height: 5,
              borderRadius: 5,
              background: "var(--info)",
              boxShadow: "0 0 0 3px rgba(111,143,184,0.25)",
            }}
          />
          <span className={`${MONO_TINY} text-info`}>OBSERVABILITY</span>
        </div>
        <p className="text-[13px] text-text leading-snug m-0">
          {spanCount > 0
            ? `This run produced ${spanCount} decisions. Open the full trace surface to inspect spans, prompts, tool calls, and supervisor notes.`
            : "Open the full trace surface to inspect agent spans for this run."}
        </p>
        <Link
          to={`/agent-runs/${encodeURIComponent(agentRunId)}`}
          className="mt-3 inline-flex items-center gap-2 px-3 py-2 rounded text-[12px] border border-gold/40 bg-gold/[0.08] text-gold hover:bg-gold/[0.14]"
        >
          View full trace →
        </Link>
      </div>
      <div className={`${MONO_TINY} text-text-4 px-1`}>
        Span tree + bottom-sheet inspectors will surface here once
        <code className="font-mono px-1">summary.agent_run_id</code>
        is wired to RunSummary (
        <code className="font-mono px-1">agent-run-observability-ipc-emission</code>
        ).
      </div>
    </div>
  );
}

// ── REVIEW tab ──────────────────────────────────────────────────────

function ReviewTab({ summary }: { summary: RunSummary }) {
  // `key` resets ReviewPanel state across runs — matches desktop behavior.
  return (
    <div className="flex flex-col gap-2 py-3 pb-24">
      <div className="flex items-baseline justify-between">
        <div>
          <div className="font-serif italic text-[22px] leading-none text-text font-medium">
            Review
          </div>
          <div className="font-mono text-[10px] text-text-3 mt-1">
            supervisor agents
          </div>
        </div>
      </div>
      <ReviewPanel
        key={summary.id}
        runId={summary.id}
        runIsCompleted={summary.status === "completed"}
      />
    </div>
  );
}

// ── loading / error ───────────────────────────────────────────────

export function MobileEvalRunDetailLoading({ id }: { id: string }) {
  return (
    <div className="rounded-card border border-border bg-surface p-5 animate-pulse">
      <div className="h-4 w-32 bg-surface-elev rounded mb-2" />
      <div className="h-3 w-48 bg-surface-elev rounded" />
      <div className="sr-only">Loading run {id}…</div>
    </div>
  );
}

export function MobileEvalRunDetailError({
  err,
  onRetry,
  runId,
}: {
  err: unknown;
  onRetry: () => void;
  runId: string;
}) {
  if (err instanceof ApiError && err.code === "not_found") {
    return (
      <div className="rounded-card border border-border bg-surface px-5 py-10 text-center">
        <div className="font-serif italic text-[22px] text-text-3 mb-2">
          run not found
        </div>
        <p className="m-0 mb-4 text-text-2 text-[13px]">
          No run with id <code className="font-mono text-text">{runId}</code>.
        </p>
        <Link
          to="/eval-runs"
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] border border-border text-text hover:border-text-3"
        >
          ← Back to runs
        </Link>
      </div>
    );
  }
  const message =
    err instanceof ApiError
      ? `${err.code}: ${err.message}`
      : err instanceof Error
        ? err.message
        : String(err);
  return (
    <div className="rounded-card border border-border bg-surface px-5 py-10 text-center">
      <div className="font-serif italic text-[22px] text-danger mb-2">
        couldn't load run
      </div>
      <p className="m-0 mb-4 text-text-2 text-[13px]">
        <code className="font-mono text-[12px] text-danger">{message}</code>
      </p>
      <button
        onClick={onRetry}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] border border-border text-text hover:border-text-3"
      >
        Retry
      </button>
    </div>
  );
}

// ── helpers ─────────────────────────────────────────────────────────

function mapStripState(status: string): StripState {
  if (isInflightRunStatus(status)) return "blue";
  if (status === "completed") return "green";
  if (status === "cancelled") return "amber";
  return "red";
}

function isTerminalStatus(status: string): boolean {
  return status === "completed" || status === "failed" || status === "cancelled";
}

function useLiveDuration(summary: RunSummary): number {
  const isLive = isInflightRunStatus(summary.status);
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    if (!isLive) return;
    const i = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(i);
  }, [isLive]);
  if (!isLive) {
    if (!summary.completed_at) return 0;
    const start = new Date(summary.started_at).getTime();
    const end = new Date(summary.completed_at).getTime();
    if (Number.isNaN(start) || Number.isNaN(end)) return 0;
    return Math.max(0, Math.floor((end - start) / 1000));
  }
  const start = new Date(summary.started_at).getTime();
  if (Number.isNaN(start)) return 0;
  return Math.max(0, Math.floor((now - start) / 1000));
}

function totalDurationLabel(summary: RunSummary, liveDuration: number): string {
  const isLive = isInflightRunStatus(summary.status);
  if (isLive) {
    const m = Math.floor(liveDuration / 60);
    const s = liveDuration % 60;
    return `${m}:${String(s).padStart(2, "0")}`;
  }
  if (!summary.completed_at) return "—";
  const ms =
    new Date(summary.completed_at).getTime() -
    new Date(summary.started_at).getTime();
  if (Number.isNaN(ms) || ms < 0) return "—";
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const m = Math.floor(ms / 60_000);
  const s = Math.floor((ms % 60_000) / 1000);
  return `${m}m ${s}s`;
}

function decisionReasoning(row: DecisionRowDto): string {
  const extended = row as DecisionRowDto & { reasoning?: string | null };
  return extended.reasoning?.trim() || row.justification?.trim() || "—";
}

function actionLabel(action: string, priorSide: PositionSide): MobileActionLabel {
  if (action === "long_open") return "BUY";
  if (action === "short_open") return "SHORT";
  if (action === "flat") {
    if (priorSide === "long") return "SELL";
    if (priorSide === "short") return "COVER";
    return "HOLD";
  }
  return "HOLD";
}

function pnlClass(n: number | null | undefined): string {
  if (n == null || n === 0) return "text-text-4";
  if (n > 0) return "text-gold";
  return "text-danger";
}

function pctTone(n: number | null | undefined): "pos" | "neg" | "gold" | undefined {
  if (n == null) return undefined;
  if (n > 0) return "gold";
  if (n < 0) return "neg";
  return undefined;
}

function clamp01(n: number | null | undefined): number {
  if (n == null || Number.isNaN(n)) return 0;
  return Math.max(0, Math.min(1, n));
}

function winRate(decisions: DecisionRowDto[]): { value: string; sub: string } {
  const settled = decisions.filter((d) => d.pnl_realized != null);
  if (settled.length === 0) return { value: "—", sub: "no trades" };
  const wins = settled.filter((d) => (d.pnl_realized ?? 0) > 0).length;
  const pct = (wins / settled.length) * 100;
  return {
    value: `${pct.toFixed(1)}%`,
    sub: `${wins}/${settled.length} trades`,
  };
}

function fmtNumber(n: number | null | undefined): string {
  if (n == null) return "—";
  return n.toFixed(2);
}

function fmtPct(n: number | null | undefined): string {
  if (n == null) return "—";
  const sign = n > 0 ? "+" : n < 0 ? "−" : "";
  return `${sign}${Math.abs(n).toFixed(2)}%`;
}

// Absolute terminal-PnL from the equity curve (terminal - start). Lives
// here rather than via `summary` because RunSummary only carries the
// % return, not the starting balance. See QA22 /
// `eval-inspector-total-pnl-summary`.
function totalPnlUsd(
  equityCurve: ReadonlyArray<{ equity_usd: number }>,
): number | null {
  if (equityCurve.length < 2) return null;
  const start = equityCurve[0]?.equity_usd;
  const end = equityCurve[equityCurve.length - 1]?.equity_usd;
  if (start == null || end == null) return null;
  return end - start;
}

function fmtPnlUsd(pnl: number | null): string {
  if (pnl == null) return "—";
  const abs = Math.abs(pnl);
  const formatted = abs.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
  if (pnl > 0) return `+$${formatted}`;
  if (pnl < 0) return `−$${formatted}`;
  return `$${formatted}`;
}

function fmtTokensTotal(summary: RunSummary): string {
  const total =
    (summary.actual_input_tokens ?? 0) + (summary.actual_output_tokens ?? 0);
  return total > 0 ? total.toLocaleString() : "—";
}

function fmtTokensSub(summary: RunSummary): string {
  const total = fmtTokensTotal(summary);
  return total === "—" ? "no tokens" : `${total} tok`;
}

function fmtTime(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function fmtDate(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  return d.toLocaleDateString(undefined, { month: "short", day: "numeric" });
}

function traceRunId(summary: RunSummary): string {
  const withTraceId = summary as RunSummary & { agent_run_id?: string | null };
  return withTraceId.agent_run_id ?? summary.id;
}
