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
