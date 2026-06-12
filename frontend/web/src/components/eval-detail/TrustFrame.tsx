import type { ReactNode } from "react";

type EvidenceGrade = "A" | "B" | "C" | "D" | string;

export type TrustFrameMetrics = {
  status?: string | null;
  error?: string | null;
  total_return_pct?: number | null;
  sharpe?: number | null;
  sharpe_ci_low?: number | null;
  sharpe_ci_high?: number | null;
  return_ci_low?: number | null;
  return_ci_high?: number | null;
  n_trades?: number | null;
  n_decisions?: number | null;
  n_real_decisions?: number | null;
  n_synthesized_decisions?: number | null;
  insufficient_sample?: boolean | null;
  annualization_calendar?: string | null;
  evidence_grade?: EvidenceGrade | null;
};

export function TrustFrameStrip({
  metrics,
  compact = false,
}: {
  metrics: TrustFrameMetrics | null | undefined;
  compact?: boolean;
}) {
  const failed = metrics?.status === "failed" || Boolean(metrics?.error);
  const zeroTrades = metrics?.n_trades === 0;
  const hasMetrics =
    metrics?.total_return_pct != null ||
    metrics?.sharpe != null ||
    metrics?.n_trades != null ||
    metrics?.evidence_grade != null;

  if (!metrics || (!hasMetrics && !failed)) return null;

  const sampleLabel = failed
    ? "failed run"
    : zeroTrades
      ? "zero trades"
      : metrics.insufficient_sample
        ? "insufficient sample"
        : "sample checked";

  return (
    <div
      data-testid="eval-trust-frame"
      className={[
        "border border-border bg-surface/60",
        compact ? "rounded-sm px-3 py-2" : "rounded-md px-4 py-3",
      ].join(" ")}
    >
      <div className="flex flex-wrap items-center gap-2 text-[11px] font-mono tabular-nums">
        <EvidenceGradeChip grade={metrics.evidence_grade} failed={failed} />
        <TrustChip tone={failed || zeroTrades ? "danger" : metrics.insufficient_sample ? "warn" : "muted"}>
          {sampleLabel}
        </TrustChip>
        <TrustChip tone="muted">trades {fmtInt(metrics.n_trades)}</TrustChip>
        <TrustChip tone="muted">
          decisions {fmtInt(metrics.n_real_decisions)}/{fmtInt(metrics.n_synthesized_decisions)} real/synth
        </TrustChip>
        <TrustChip tone={metrics.annualization_calendar ? "muted" : "warn"}>
          {metrics.annualization_calendar ? calendarLabel(metrics.annualization_calendar) : "legacy annualization"}
        </TrustChip>
        <TrustChip tone="muted">fees/slippage scenario model</TrustChip>
      </div>
      <div className="mt-2 grid gap-1 text-[11px] font-mono tabular-nums text-text-3 sm:grid-cols-2">
        <ConfidenceLine
          label="return CI"
          low={metrics.return_ci_low}
          high={metrics.return_ci_high}
          unit="%"
        />
        <ConfidenceLine label="Sharpe CI" low={metrics.sharpe_ci_low} high={metrics.sharpe_ci_high} />
      </div>
    </div>
  );
}

export function EvidenceGradeChip({
  grade,
  failed = false,
}: {
  grade: EvidenceGrade | null | undefined;
  failed?: boolean;
}) {
  const label = failed ? "failed" : grade ? `grade ${grade}` : "grade pending";
  return <TrustChip tone={gradeTone(grade, failed)}>{label}</TrustChip>;
}

export function ConfidenceInline({
  low,
  high,
  unit,
}: {
  low: number | null | undefined;
  high: number | null | undefined;
  unit?: string;
}) {
  if (low == null || high == null || !Number.isFinite(low) || !Number.isFinite(high)) {
    return <span className="text-text-4">CI -</span>;
  }
  return (
    <span className="text-text-4">
      CI {fmtNumber(low)}
      {unit ?? ""}..{fmtNumber(high)}
      {unit ?? ""}
    </span>
  );
}

function ConfidenceLine({
  label,
  low,
  high,
  unit,
}: {
  label: string;
  low: number | null | undefined;
  high: number | null | undefined;
  unit?: string;
}) {
  return (
    <div>
      <span className="text-text-4">{label}</span>{" "}
      <ConfidenceInline low={low} high={high} unit={unit} />
    </div>
  );
}

function TrustChip({ tone, children }: { tone: "gold" | "info" | "warn" | "danger" | "muted"; children: ReactNode }) {
  const toneClass =
    tone === "gold"
      ? "border-gold/30 bg-gold/[0.08] text-gold"
      : tone === "info"
        ? "border-info/30 bg-info/[0.08] text-info"
        : tone === "warn"
          ? "border-warn/30 bg-warn/[0.08] text-warn"
          : tone === "danger"
            ? "border-danger/30 bg-danger/[0.08] text-danger"
            : "border-border bg-surface text-text-3";
  return (
    <span className={`inline-flex items-center rounded-sm border px-1.5 py-0.5 ${toneClass}`}>
      {children}
    </span>
  );
}

function gradeTone(grade: EvidenceGrade | null | undefined, failed: boolean): "gold" | "info" | "warn" | "danger" | "muted" {
  if (failed) return "danger";
  if (grade === "A") return "gold";
  if (grade === "B") return "info";
  if (grade === "C") return "warn";
  if (grade === "D") return "danger";
  return "muted";
}

function fmtInt(value: number | bigint | null | undefined): string {
  if (value == null) return "-";
  const n = Number(value);
  return Number.isFinite(n) ? Math.trunc(n).toLocaleString() : "-";
}

function fmtNumber(value: number): string {
  return Math.abs(value) >= 10 ? value.toFixed(1) : value.toFixed(2);
}

function calendarLabel(value: string): string {
  if (value === "crypto_24_7_365d") return "calendar crypto 24/7";
  if (value === "us_market_252x390m") return "calendar US session";
  return `calendar ${value}`;
}
