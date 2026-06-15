// Number / date / currency formatters used across the SPA. Kept centralised so
// the prototype's tabular-nums conventions stay consistent.

/**
 * USD cost display formatter. Designed for per-call LLM costs which can
 * range from ~$1e-7 (cheap model on a tiny prompt) up to a few dollars per
 * agent run, with rare per-eval aggregates climbing to the low thousands.
 *
 * The old `toFixed(4)` rule rendered "$0.0000" for anything below half a
 * cent, hiding real cost from operators. This formatter widens precision
 * at small magnitudes and uses a `<$0.0001` floor for the truly tiny.
 * Pair the display string with `formatCostUsdPrecise` in a `title=` to
 * surface the underlying number on hover.
 */
export function formatCostUsd(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  if (value === 0) return "$0.00";

  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";

  if (abs < 0.0001) return "<$0.0001";

  if (abs >= 1000) {
    return `${sign}$${abs.toLocaleString("en-US", {
      minimumFractionDigits: 2,
      maximumFractionDigits: 2,
    })}`;
  }
  if (abs >= 1) return `${sign}$${abs.toFixed(2)}`;
  if (abs >= 0.01) return `${sign}$${abs.toFixed(4)}`;
  return `${sign}$${abs.toFixed(6)}`;
}

/**
 * Spec alias used by callers that follow the
 * `formatUsdCost(value)` naming from the QA round-7 intake. Delegates to
 * `formatCostUsd` so the precision rules stay defined in one place.
 */
export const formatUsdCost = formatCostUsd;

/**
 * Token-cost ("spend") display for aggregate run/eval spend. Unlike
 * `formatCostUsd`, a value of `0` (or null / negative) renders as "—"
 * rather than "$0.00": across the cost path a zero aggregate means "no
 * priced model call was recorded" — i.e. spend is *unknown*, not a
 * genuine $0.00. Showing "—" matches the backend's "zero pricing =
 * unknown" convention (`compute_token_cost_usd`,
 * `aggregate_eval_run_inference_cost`) and avoids advertising a
 * misleadingly precise zero. Any positive amount delegates to
 * `formatCostUsd` so the precision rules stay in one place.
 */
export function formatSpendUsd(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value) || value <= 0) return "—";
  return formatCostUsd(value);
}

/**
 * Full-precision USD string for tooltips. Renders up to 12 fraction
 * digits with trailing zeros trimmed so the operator can confirm the
 * exact paid amount on hover (e.g. "$0.00000123" rather than guessing
 * what "<$0.0001" hides).
 */
export function formatCostUsdPrecise(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "";
  if (value === 0) return "$0.00";

  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";
  const raw = abs.toFixed(12);
  const trimmed = raw.replace(/(\.\d*?)0+$/, "$1").replace(/\.$/, "");
  return `${sign}$${trimmed}`;
}

/**
 * Marketplace USD display formatter.
 * Intended for amounts in the hundreds-to-millions range (buyer payments,
 * lifetime earnings, clones upstream).
 * - < $1 000: "$420"
 * - ≥ $1 000 and < $1 000 000: "$4,820"
 * - ≥ $1 000 000: "$1.2M"
 * Returns "—" for null/undefined/non-finite inputs.
 */
export function formatUsd(value: number | null | undefined): string {
  if (value == null || !Number.isFinite(value)) return "—";
  const abs = Math.abs(value);
  const sign = value < 0 ? "-" : "";
  if (abs >= 1_000_000) {
    return `${sign}$${(abs / 1_000_000).toFixed(1)}M`;
  }
  if (abs >= 1_000) {
    return `${sign}$${abs.toLocaleString("en-US", { maximumFractionDigits: 0 })}`;
  }
  return `${sign}$${Math.round(abs)}`;
}

export function formatCadence(minutes: number): string {
  if (!Number.isFinite(minutes) || minutes <= 0) {
    return "—";
  }

  if (minutes < 60) {
    return `${minutes}m`;
  }

  const hours = Math.floor(minutes / 60);
  const remainderMinutes = minutes % 60;

  if (remainderMinutes === 0) {
    return `${hours}h`;
  }

  return `${hours}h ${remainderMinutes}m`;
}

/**
 * Percentage display for performance figures (returns, win rate, drawdown).
 * Rounds to `digits` (default 2) and strips trailing zeros, so "47.20" reads
 * as "47.2" while a raw "0.19077721054834548" collapses to "0.19" — fixing the
 * marketplace stat boxes that overflowed when full-precision floats were
 * interpolated straight into the cell. `signed` (default true) prefixes a "+"
 * on positive values for return-style metrics; pass `signed: false` for win
 * rate / drawdown where the sign is implied or already carried by the value.
 * Null / undefined / non-finite → "—".
 */
export function formatPercent(
  value: number | null | undefined,
  opts?: { digits?: number; signed?: boolean },
): string {
  if (value == null || !Number.isFinite(value)) return "—";
  const digits = opts?.digits ?? 2;
  const signed = opts?.signed ?? true;
  const rounded = Number(value.toFixed(digits));
  const sign = signed && rounded > 0 ? "+" : "";
  return `${sign}${rounded}%`;
}

/**
 * Ratio display for Sharpe and other unitless performance ratios. Same
 * round-and-strip rule as `formatPercent` but without the "%" unit; defaults to
 * a signed "+" on positive values (matching the listing cards). Null /
 * undefined / non-finite → "—".
 */
export function formatSharpe(
  value: number | null | undefined,
  opts?: { digits?: number; signed?: boolean },
): string {
  if (value == null || !Number.isFinite(value)) return "—";
  const digits = opts?.digits ?? 2;
  const signed = opts?.signed ?? true;
  const rounded = Number(value.toFixed(digits));
  const sign = signed && rounded > 0 ? "+" : "";
  return `${sign}${rounded}`;
}
