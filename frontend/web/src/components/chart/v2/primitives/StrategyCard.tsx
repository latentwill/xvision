import { Link } from "react-router-dom";
import type { DrawdownPoint, EquityPoint } from "../types";
import { LeadCardChrome } from "./LeadCardChrome";
import { MiniSparkline } from "./MiniSparkline";

export type StrategyCardArm = {
  id: string;
  runId: string;
  label: string;
  shortId: string;
  kind: string;
  color: string;
  status: string;
  equity: EquityPoint[];
  drawdown: DrawdownPoint[];
  metrics: {
    returnPct?: number | null;
    sharpe?: number | null;
    maxDrawdownPct?: number | null;
    decisions?: number | null;
  };
};

type Props = {
  arm: StrategyCardArm;
  lead?: boolean;
  removable?: boolean;
  onRemove?: (id: string) => void;
  onSetLead?: (id: string) => void;
};

export function StrategyCard({
  arm,
  lead = false,
  removable = true,
  onRemove,
  onSetLead,
}: Props) {
  const body = (
    <article
      className={`h-full rounded-card border bg-surface-card p-3 ${
        lead ? "border-transparent" : "border-border"
      }`}
      data-testid={`strategy-card-${arm.runId}`}
    >
      <div className="flex items-start gap-2">
        <span
          className="mt-1 h-2.5 w-2.5 shrink-0 rounded-full"
          style={{ background: arm.color }}
          aria-hidden
        />
        <div className="min-w-0">
          <Link
            to={`/eval-runs/${encodeURIComponent(arm.runId)}`}
            className="block truncate font-sans font-medium text-[18px] leading-tight text-text hover:underline"
            title={arm.id}
          >
            {arm.label}
          </Link>
          <div className="mt-0.5 font-mono text-[10px] uppercase tracking-normal text-text-3">
            {arm.kind} / {arm.shortId}
          </div>
        </div>
        {lead ? (
          <span className="ml-auto rounded-sm border border-gold/40 px-1.5 py-0.5 font-mono text-[10px] uppercase text-gold">
            Lead
          </span>
        ) : (
          <button
            type="button"
            onClick={() => onSetLead?.(arm.runId)}
            className="ml-auto rounded-sm border border-border px-1.5 py-0.5 font-mono text-[10px] uppercase text-text-3 hover:text-text"
          >
            Lead
          </button>
        )}
        <button
          type="button"
          onClick={() => onRemove?.(arm.runId)}
          disabled={!removable}
          aria-label={`Remove ${arm.label}`}
          title={removable ? `Remove ${arm.id}` : "Compare requires at least two runs"}
          className="rounded-sm px-1.5 py-0.5 font-mono text-[12px] text-text-3 hover:text-danger disabled:cursor-not-allowed disabled:opacity-35"
        >
          x
        </button>
      </div>

      <div className="mt-3">
        <MiniSparkline points={arm.equity} color={arm.color} />
      </div>

      <div className="mt-3 grid grid-cols-2 gap-2 text-[12px]">
        <Metric label="Return" value={fmtPct(arm.metrics.returnPct)} tone={signTone(arm.metrics.returnPct)} />
        <Metric label="Sharpe" value={fmtNumber(arm.metrics.sharpe, 2)} />
        <Metric label="Max DD" value={fmtPct(arm.metrics.maxDrawdownPct)} tone="text-danger" />
        <Metric label="Decisions" value={fmtInt(arm.metrics.decisions)} />
      </div>

      <div className="mt-3 flex flex-wrap gap-1">
        {indicatorChips(arm).map((chip) => (
          <span
            key={chip}
            className="rounded-sm border border-border-soft bg-surface-elev px-1.5 py-0.5 font-mono text-[10px] text-text-3"
          >
            {chip}
          </span>
        ))}
      </div>
    </article>
  );

  return lead ? <LeadCardChrome>{body}</LeadCardChrome> : body;
}

function Metric({ label, value, tone = "text-text" }: { label: string; value: string; tone?: string }) {
  return (
    <div className="rounded-sm bg-surface-elev px-2 py-1.5">
      <div className="text-[10px] uppercase text-text-3">{label}</div>
      <div className={`mt-0.5 font-mono ${tone}`}>{value}</div>
    </div>
  );
}

function indicatorChips(arm: StrategyCardArm): string[] {
  const latest = arm.equity[arm.equity.length - 1]?.value ?? 0;
  const first = arm.equity[0]?.value ?? latest;
  const direction = latest >= first ? "up" : "down";
  const dd = arm.metrics.maxDrawdownPct == null ? "DD" : `DD ${Math.abs(arm.metrics.maxDrawdownPct).toFixed(1)}%`;
  return [`EQ ${direction}`, dd, arm.status.toUpperCase(), "EMA"];
}

function fmtNumber(n: number | null | undefined, digits = 2): string {
  return n == null ? "-" : n.toFixed(digits);
}

function fmtPct(n: number | null | undefined): string {
  if (n == null) return "-";
  const sign = n > 0 ? "+" : "";
  return `${sign}${n.toFixed(2)}%`;
}

function fmtInt(n: number | null | undefined): string {
  return n == null ? "-" : String(n);
}

function signTone(n: number | null | undefined): string {
  if (n == null) return "text-text";
  if (n > 0) return "text-gold";
  if (n < 0) return "text-danger";
  return "text-text-2";
}
