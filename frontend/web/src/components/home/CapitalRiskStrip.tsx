// frontend/web/src/components/home/CapitalRiskStrip.tsx
//
// Home capital-risk strip (bead 8s4, CT5 §9.3 — "deployed capital · drawdown ·
// daily-loss-limit buffer", flagged "non-negotiable for live money"). A slim,
// full-width safety strip that surfaces the aggregate capital exposure of the
// active live/paper deployments at a glance, and routes into /live for
// per-deployment detail.
//
// REUSE, NOT RE-FETCH: this component is presentational. It takes the already-
// computed `CapitalRiskAggregate` (built from the route's existing Wave-3b
// live-deployments 5s poll via `aggregateCapitalRisk`). It never fetches.
//
// HONESTY MANDATE (CT5 §8.1/§8.9). Every value originates from broker/execution
// truth on `LiveDeploymentSummary` and may legitimately be null:
//   - a null individual field renders "—", NEVER a fabricated $0;
//   - below the data floor (no live deployments, or every field null) the strip
//     renders an explicit "insufficient data — no live capital deployed yet"
//     state, NOT a calm green zero grid;
//   - the risk-veto chip (bead s78.2) renders the REAL summed count of recorded
//     risk-veto supervisor notes since the operator's last visit. It is "—" when
//     the count is null (no `?since` boundary → can't count since an unknown
//     time), and the number INCLUDING a real `0` when known ("0 vetoes since you
//     were last here" is an honest fact). Never a fabricated 0.
// The daily-loss BUFFER is color-coded (healthy → warn → danger) via theme
// tokens as it shrinks toward the enforced kill line.
//
// NO POPUP: inline, full-width strip — no dialog/sheet/overlay.
// LAYOUT: single full-width row of metric chips; no right-side 4th column.

import { Link } from "react-router-dom";

import { Card } from "@/components/primitives/Card";
import {
  bufferTone,
  type BufferTone,
  type CapitalRiskAggregate,
} from "@/features/home/capital-risk";

export interface CapitalRiskStripProps {
  agg: CapitalRiskAggregate;
}

const DASH = "—";

/** Plain unsigned USD with thousands separators (e.g. `$1,200`). Whole dollars:
 * a capital-exposure overview reads cleaner without cents at the aggregate. */
function fmtUsd(n: number | null): string {
  if (n == null) return DASH;
  return `$${n.toLocaleString("en-US", { maximumFractionDigits: 0 })}`;
}

/** Drawdown percent (always a loss magnitude, so unsigned with a leading
 * minus). `—` when null. */
function fmtDrawdown(n: number | null): string {
  if (n == null) return DASH;
  if (n === 0) return "0.0%";
  return `−${Math.abs(n).toFixed(1)}%`;
}

// Buffer tone → a NON-color-alone status cue (WCAG 1.4.1) on the safety-critical
// metric: a shape glyph + a word carry the state, with hue as a third, redundant
// signal. The dollar VALUE renders in neutral high-contrast text (so it always
// clears 4.5:1 in both themes); only the small glyph carries hue (a graphical
// object, ≥3:1). A null tone (no honest ratio) shows no cue — never a fabricated
// "healthy" green.
const BUFFER_TONE: Record<
  BufferTone,
  { glyph: string; word: string; color: string }
> = {
  healthy: { glyph: "✓", word: "healthy", color: "text-gold" },
  warn: { glyph: "⚠", word: "low", color: "text-warn" },
  danger: { glyph: "✕", word: "critical", color: "text-danger" },
};

function Metric({
  label,
  value,
  testid,
  valueClass = "text-text",
  tone,
  title,
}: {
  label: string;
  value: string;
  testid: string;
  valueClass?: string;
  tone?: BufferTone | null;
  title?: string;
}) {
  const t = tone ? BUFFER_TONE[tone] : null;
  return (
    <div className="flex flex-col gap-0.5 min-w-0" title={title}>
      <span className="text-[10px] uppercase tracking-wide text-text-3">
        {label}
      </span>
      <span className="flex items-baseline gap-1">
        {t && (
          // Shape + hue = the non-color-alone cue (glyph is a graphical object).
          <span
            className={`shrink-0 font-mono text-[12px] ${t.color}`}
            aria-hidden="true"
          >
            {t.glyph}
          </span>
        )}
        <span
          data-testid={testid}
          data-tone={tone ?? undefined}
          className={`font-mono tabular-nums text-[15px] font-semibold ${valueClass}`}
        >
          {value}
        </span>
        {t && (
          // Word = the readable, screen-reader-friendly state (not hue-alone).
          <span className="shrink-0 text-[11px] text-text-3">{t.word}</span>
        )}
      </span>
    </div>
  );
}

export function CapitalRiskStrip({ agg }: CapitalRiskStripProps) {
  const tone = bufferTone(
    agg.tightestDailyLossBufferUsd,
    agg.tightestDailyLossBudgetUsd,
  );

  return (
    <section data-testid="capital-risk-strip" aria-label="Capital risk">
      <Card className="px-5 py-3">
        {!agg.hasData ? (
          // Below the data floor: honest insufficient-data state. NOT a $0 grid.
          // When live/paper runs exist but no real capital is deployed, surface
          // the paper-trading status instead of a generic "no data" message.
          <div
            data-testid="capital-risk-empty"
            className="flex items-center gap-2 text-[12px] leading-5"
          >
            <span className="shrink-0 font-mono text-text-4" aria-hidden="true">
              {DASH}
            </span>
            <span className="text-text-3">
              {agg.liveCount > 0
                ? "Capital risk — paper trading active; no live capital deployed."
                : "Capital risk — insufficient data; no live capital deployed yet."}
            </span>
            <Link
              to="/live"
              className="ml-auto shrink-0 text-[12px] text-text-3 underline-offset-2 hover:text-text hover:underline"
            >
              Live trading →
            </Link>
          </div>
        ) : (
          <div className="flex flex-wrap items-center gap-x-6 gap-y-3">
            <span className="text-[12px] font-medium text-text-2">
              Capital risk
            </span>

            <Metric
              label="Deployed capital"
              value={fmtUsd(agg.deployedCapitalUsd)}
              testid="capital-risk-deployed"
              title="Σ open-position notional across active deployments"
            />

            <Metric
              label="Worst drawdown"
              value={fmtDrawdown(agg.worstDrawdownPct)}
              testid="capital-risk-drawdown"
              valueClass={agg.worstDrawdownPct == null ? "text-text-2" : "text-text"}
              title="Largest peak-to-current drawdown across active deployments"
            />

            <Metric
              label="Daily-loss buffer"
              value={fmtUsd(agg.tightestDailyLossBufferUsd)}
              testid="capital-risk-buffer"
              valueClass={
                agg.tightestDailyLossBufferUsd == null
                  ? "text-text-2"
                  : "text-text"
              }
              tone={tone}
              title="Tightest headroom before the enforced daily-loss kill fires"
            />

            {/* bead s78.2: risk vetoes since last visit — a REAL count of
                recorded risk-veto supervisor notes since the operator's last
                visit. "—" when null (no boundary → can't count since an unknown
                time); the number INCLUDING a real 0 when known. Never a
                fabricated 0. */}
            <div className="flex flex-col gap-0.5">
              <span className="text-[10px] uppercase tracking-wide text-text-3">
                Risk vetoes
              </span>
              <span
                data-testid="capital-risk-veto"
                className={`font-mono tabular-nums text-[15px] font-semibold ${
                  agg.riskVetoCount == null ? "text-text-2" : "text-text"
                }`}
                title={
                  agg.riskVetoCount == null
                    ? "Risk vetoes since last visit (no last-visit boundary yet)"
                    : "Risk vetoes recorded since your last visit"
                }
              >
                {agg.riskVetoCount == null ? DASH : String(agg.riskVetoCount)}
              </span>
            </div>

            <Link
              to="/live"
              className="ml-auto shrink-0 text-[12px] text-text-3 underline-offset-2 hover:text-text hover:underline"
            >
              Live trading →
            </Link>
          </div>
        )}
      </Card>
    </section>
  );
}
