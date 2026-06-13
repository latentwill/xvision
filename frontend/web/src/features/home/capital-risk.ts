// frontend/web/src/features/home/capital-risk.ts
//
// Pure capital-risk aggregate for the home capital-risk strip (bead 8s4,
// CT5 §9.3 — "deployed capital · drawdown · daily-loss-limit buffer",
// "non-negotiable for live money"). NO JSX, NO fetch: the route owns the
// (already-running, Wave-3b 5s) live-deployments poll, the component
// (CapitalRiskStrip) owns rendering, this selector owns the math.
//
// HONESTY MANDATE (CT5 §8.1/§8.9). Every field on `LiveDeploymentSummary` is
// already broker/execution-sourced and may legitimately be `null` (no fill yet,
// venue unreachable, no day baseline). This aggregate NEVER coerces a `null`
// into a `0`:
//   - it aggregates ONLY over the non-null source values;
//   - a field whose every source value is null aggregates to `null`;
//   - the data floor (no deployments at all, OR every aggregate null) reports
//     `hasData=false` so the strip renders an explicit "insufficient data —
//     no live capital deployed yet" state, never a calm green zero.
// A real broker-sourced `0` (a flat book) is honest DATA and is kept distinct
// from `null` (no snapshot).

import type { LiveDeploymentSummary } from "@/api/types.gen";

/** Aggregate capital-risk snapshot across the active deployment population. */
export interface CapitalRiskAggregate {
  /** SUM of non-null `deployed_capital_usd`; `null` when none are non-null. */
  deployedCapitalUsd: number | null;
  /** WORST (max) non-null `drawdown_pct`; `null` when none are non-null. */
  worstDrawdownPct: number | null;
  /** TIGHTEST (min) non-null `daily_loss_limit_remaining_usd`; `null` when none. */
  tightestDailyLossBufferUsd: number | null;
  /** Budget paired with the tightest daily-loss buffer; `null` when unsourced. */
  tightestDailyLossBudgetUsd: number | null;
  /**
   * bead s78.2: SUM of the non-null `risk_veto_count_since_last_visit` values —
   * a REAL count of recorded risk-veto supervisor notes since the operator's
   * last visit. `null` when EVERY per-deployment count is null (no `?since`
   * boundary was supplied → can't count "since an unknown time"). A summed `0`
   * is an honest fact ("0 vetoes since you were last here"), kept distinct from
   * `null`; it is NEVER fabricated from null counts.
   */
  riskVetoCount: number | null;
  /** Count of deployments considered (the active live/paper population). */
  liveCount: number;
  /** False below the data floor: no deployments, OR every aggregate is null. */
  hasData: boolean;
}

/** Color tone for the daily-loss buffer as it shrinks toward the kill line. */
export type BufferTone = "healthy" | "warn" | "danger";

/** Buffer/budget ratio bands. The buffer is the headroom before the enforced
 * daily-loss kill fires; as it shrinks relative to the configured daily-loss
 * budget the tone escalates. */
const BUFFER_WARN_RATIO = 0.25; // ≤25% of budget remaining → warn
const BUFFER_HEALTHY_RATIO = 0.5; // >50% remaining → healthy

/** Sum of the non-null members of `xs`; `null` when `xs` has no non-null member. */
function sumNonNull(xs: (number | null)[]): number | null {
  const present = xs.filter((x): x is number => x != null);
  if (present.length === 0) return null;
  return present.reduce((acc, x) => acc + x, 0);
}

/** Max of the non-null members; `null` when none. */
function maxNonNull(xs: (number | null)[]): number | null {
  const present = xs.filter((x): x is number => x != null);
  if (present.length === 0) return null;
  return present.reduce((acc, x) => (x > acc ? x : acc));
}

/** Min of the non-null members; `null` when none. */
function minNonNull(xs: (number | null)[]): number | null {
  const present = xs.filter((x): x is number => x != null);
  if (present.length === 0) return null;
  return present.reduce((acc, x) => (x < acc ? x : acc));
}

/**
 * Aggregate the capital-risk fields across the (already-filtered, active)
 * deployment population. Each field is aggregated independently over its own
 * non-null source values, so a row missing one field still contributes the
 * others. `hasData` is the honest data floor: it is `false` when there are no
 * deployments at all, OR when every aggregate came back `null`.
 */
export function aggregateCapitalRisk(
  deployments: LiveDeploymentSummary[],
): CapitalRiskAggregate {
  const deployedCapitalUsd = sumNonNull(
    deployments.map((d) => d.deployed_capital_usd),
  );
  const worstDrawdownPct = maxNonNull(deployments.map((d) => d.drawdown_pct));
  const tightestDailyLossBufferUsd = minNonNull(
    deployments.map((d) => d.daily_loss_limit_remaining_usd),
  );
  const tightestDailyLossBudgetUsd =
    tightestDailyLossBufferUsd == null
      ? null
      : (deployments.find(
          (d) => d.daily_loss_limit_remaining_usd === tightestDailyLossBufferUsd,
        )?.daily_loss_budget_usd ?? null);

  // bead s78.2: sum the REAL non-null per-deployment veto counts. `sumNonNull`
  // already filters nulls (no fabrication) and returns null when none are
  // non-null — so a summed 0 stays an honest 0, distinct from null.
  const riskVetoCount = sumNonNull(
    deployments.map((d) => d.risk_veto_count_since_last_visit),
  );

  const hasData =
    deployedCapitalUsd != null ||
    worstDrawdownPct != null ||
    tightestDailyLossBufferUsd != null;

  return {
    deployedCapitalUsd,
    worstDrawdownPct,
    tightestDailyLossBufferUsd,
    tightestDailyLossBudgetUsd,
    riskVetoCount,
    liveCount: deployments.length,
    hasData,
  };
}

/**
 * Classify the daily-loss buffer's tone as it approaches 0. Returns `null` when
 * there is no honest ratio to compute (buffer null, or budget null/zero) — the
 * caller renders a neutral tone in that case, never a fabricated "healthy".
 *
 * A non-positive buffer (the kill line is at/over the edge) is always `danger`.
 */
export function bufferTone(
  bufferUsd: number | null,
  budgetUsd: number | null,
): BufferTone | null {
  if (bufferUsd == null) return null;
  if (budgetUsd == null || budgetUsd <= 0) return null;

  // At or past the kill line — no ratio needed, this is the worst case.
  if (bufferUsd <= 0) return "danger";

  const ratio = bufferUsd / budgetUsd;
  if (ratio <= BUFFER_WARN_RATIO) return "warn";
  if (ratio <= BUFFER_HEALTHY_RATIO) return null;
  return "healthy";
}
