import { useMemo } from "react";

// Module-scope counter — each surface mount gets its own unique sync key,
// so two ChartV2 instances on the same page do not share a cursor band.
let counter = 0;

/**
 * Returns a stable uPlot sync key for all panes that belong to the same
 * surface instance. Pass the returned string to `uPlot({ sync: { key } })`.
 *
 * The key is stable across re-renders of the same mount. It changes only
 * when `surface` changes (i.e. the component switched to a different
 * dataset), at which point a fresh key is minted so the old cursor group
 * is abandoned.
 */
export function useChart2Sync(surface: string): string {
  // eslint-disable-next-line react-hooks/exhaustive-deps
  return useMemo(() => `chart2:${surface}:${++counter}`, [surface]);
}
