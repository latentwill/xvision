// frontend/web/src/components/home/CapitalRiskStrip.tsx
//
// 8s4 Capital-risk strip (CT5 S1). Surfaces per-deployment capital-risk
// metrics for RUNNING simulated (paper/testnet) deployments on the home dashboard.
//
// HONESTY MANDATE: every deployment shown is paper or testnet — NEVER live
// money. The "simulated" qualifier is always visible on the section label.
// The component renders null when there are no running deployments
// ("say nothing when you have nothing to say" — matches DeployReadinessStrip).
//
// NO POPUP: inline full-width strip, no dialog/sheet/overlay (CLAUDE.md).
// NO RIGHT-SIDE BOX: single-column layout (QA30 / CLAUDE.md).
// Color is NEVER the only signal — always paired with glyph + literal value.

import { Card } from "@/components/primitives/Card";
import { VenueBadge } from "@/components/primitives/VenueBadge";
import type { VenueLabel } from "@/api/safety";
import type { LiveDeploymentSummary } from "@/api/types.gen/LiveDeploymentSummary";
import {
  drawdownTone,
  runningPnl,
  dailyLossBufferTone,
  toneGlyph,
  formatUsd,
  formatPct,
  type RiskTone,
} from "@/features/live/deployment-risk";

// ─── Props ────────────────────────────────────────────────────────────────────

export interface CapitalRiskStripProps {
  deployments: LiveDeploymentSummary[];
}

// ─── Tone → Tailwind class ────────────────────────────────────────────────────

function toneClass(tone: RiskTone): string {
  switch (tone) {
    case "gold":    return "text-gold";
    case "warn":    return "text-warn";
    case "danger":  return "text-danger";
    case "neutral": return "";
  }
}

// ─── Metric Cell idiom (mirror HomeOutcomeStrip Cell) ────────────────────────

interface MetricCellProps {
  label: string;
  /** Visible value string (includes glyph prefix if any). */
  value: string;
  tone: RiskTone;
  testId: string;
  /** Additional key that drives the xvn-num-pop CSS mount animation on change. */
  animationKey?: string;
}

function MetricCell({ label, value, tone, testId, animationKey }: MetricCellProps) {
  return (
    <div className="flex flex-col gap-0.5">
      <span className="text-[10px] uppercase tracking-wide text-text-3">
        {label}
      </span>
      <span
        key={animationKey ?? value}
        data-testid={testId}
        data-tone={tone}
        className={`xvn-num-pop text-[15px] font-mono font-semibold tabular-nums ${toneClass(tone)}`}
      >
        {value}
      </span>
    </div>
  );
}

// ─── Per-deployment row ───────────────────────────────────────────────────────

function DeploymentRow({ dep }: { dep: LiveDeploymentSummary }) {
  const id = dep.deployment_id;

  // Drawdown
  const ddTone = drawdownTone(dep.drawdown_pct);
  const ddGlyph = toneGlyph(ddTone);
  const ddValue = dep.drawdown_pct !== null
    ? `${ddGlyph} ${formatPct(dep.drawdown_pct)}`
    : "—";

  // Running P&L
  const pnl = runningPnl(dep);
  const pnlValue = pnl.value !== null
    ? `${pnl.glyph} ${formatUsd(pnl.value)}`
    : "—";

  // Daily-loss buffer
  const bufTone = dailyLossBufferTone(dep.daily_loss_limit_remaining_usd, dep.daily_loss_budget_usd);
  const bufGlyph = toneGlyph(bufTone);
  const bufValue = dep.daily_loss_limit_remaining_usd !== null
    ? `${bufGlyph} ${formatUsd(dep.daily_loss_limit_remaining_usd)}`
    : "—";

  return (
    <div
      data-testid={`capital-risk-row-${id}`}
      className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2 py-2"
    >
      {/* LEFT: identity */}
      <div className="flex min-w-0 items-center gap-2">
        <span className="truncate text-[13px] font-medium text-text-2">
          {dep.strategy_name ?? "—"}
        </span>
        <VenueBadge label={dep.venue_label as VenueLabel} />
      </div>

      {/* RIGHT: metric cells */}
      <div className="flex flex-wrap items-start gap-x-8 gap-y-2">
        <MetricCell
          label="Deployed"
          value={formatUsd(dep.deployed_capital_usd)}
          tone="neutral"
          testId={`deployed-cell-${id}`}
        />
        <MetricCell
          label="Drawdown"
          value={ddValue}
          tone={ddTone}
          testId={`drawdown-cell-${id}`}
          animationKey={String(dep.drawdown_pct)}
        />
        <MetricCell
          label="P&L (today)"
          value={pnlValue}
          tone={pnl.tone}
          testId={`pnl-cell-${id}`}
          animationKey={String(pnl.value)}
        />
        <MetricCell
          label="Daily-loss buffer"
          value={bufValue}
          tone={bufTone}
          testId={`buffer-cell-${id}`}
        />
      </div>
    </div>
  );
}

// ─── Strip ────────────────────────────────────────────────────────────────────

export function CapitalRiskStrip({ deployments }: CapitalRiskStripProps) {
  // Nothing running → render nothing (band shrinks instead of stacking an
  // empty placeholder). Mirrors DeployReadinessStrip's empty-state contract.
  if (deployments.length === 0) return null;

  return (
    <section aria-label="Capital at risk (simulated)">
      <Card data-testid="capital-risk-strip" className="px-5 py-2.5">
        {/* Section header — one quiet label with the simulated qualifier */}
        <div className="mb-2 flex items-center gap-2">
          <span className="text-[10px] uppercase tracking-wide text-text-3">
            Capital at risk
          </span>
          <span className="text-[10px] text-text-4 italic">simulated</span>
        </div>

        <div className="divide-y divide-border-soft/60">
          {deployments.map((dep) => (
            <DeploymentRow key={dep.deployment_id} dep={dep} />
          ))}
        </div>
      </Card>
    </section>
  );
}
