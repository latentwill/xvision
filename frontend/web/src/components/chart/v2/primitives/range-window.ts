import type { RangePreset } from "./ChartFrame";

/**
 * Visible-window duration (in seconds) for a range preset, or `null` for the
 * "All" preset which means "show the full dataset extent".
 */
export function rangeWindowSeconds(preset: RangePreset): number | null {
  switch (preset) {
    case "1h":
      return 3_600;
    case "4h":
      return 4 * 3_600;
    case "6h":
      return 6 * 3_600;
    case "12h":
      return 12 * 3_600;
    case "1d":
      return 86_400;
    case "1w":
      return 7 * 86_400;
    case "1m":
      return 30 * 86_400;
    case "3m":
      return 90 * 86_400;
    case "All":
      return null;
  }
}

/**
 * Parse a candle granularity string (e.g. "1h", "15m", "1d") into seconds.
 * Returns `null` for unparseable input. Months ("M") are approximated at 30d.
 */
export function granularitySeconds(g: string): number | null {
  const m = /^(\d+)\s*([mhdwM])$/.exec(g.trim());
  if (!m) return null;
  const n = Number(m[1]);
  switch (m[2]) {
    case "m":
      return n * 60;
    case "h":
      return n * 3600;
    case "d":
      return n * 86_400;
    case "w":
      return n * 7 * 86_400;
    case "M":
      return n * 30 * 86_400;
    default:
      return null;
  }
}
