// Tone helpers for metric cells.
//
// Drawdown is a loss metric by definition: any non-zero magnitude is bad
// news. Treating it like a signed return (positive = good, negative = bad)
// is a category error, and the original `drawdownToneClass` did exactly
// that — `text-warn` for |dd| < 10, `text-danger` only at |dd| >= 10.
//
// This module exposes a magnitude-only helper that every surface rendering
// `max_drawdown_pct` should import. Backend payload sign convention is
// intentionally untouched (some paths emit positive drawdown, others
// negative); the helper accepts both and keys off magnitude.

/**
 * Tone class for a max-drawdown value.
 *
 * - non-zero (positive or negative magnitude) → `text-danger`
 * - exactly zero or null/undefined → `text-text`
 */
export function drawdownToneClass(n: number | null | undefined): string {
  if (n == null || n === 0) return "text-text";
  return "text-danger";
}

/**
 * `"neg" | undefined` shape for surfaces whose Metric component
 * accepts a `tone` prop (`"pos" | "neg" | "neutral"`). Returns `"neg"`
 * for any non-zero magnitude and `undefined` for zero/null so the
 * Metric falls through to its neutral default.
 */
export function drawdownMetricTone(
  n: number | null | undefined,
): "neg" | undefined {
  if (n == null || n === 0) return undefined;
  return "neg";
}
