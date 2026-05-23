/**
 * StrategyCardGrid + columnCountForN — CSS-grid wrapper that scales
 * the column count with the number of selected strategies.
 *
 * Per spec §4A.1 (B2):
 *   n ≤ 2 → 2 cols
 *   n ≤ 4 → 4 cols
 *   n ≤ 6 → 3 cols
 *   n ≥ 7 → 4 cols
 *
 * Column count is computed in JS (not via Tailwind breakpoints)
 * because it depends on the runtime selection, not the viewport.
 * Tailwind `grid-cols-*` is hard to template dynamically; we drop to
 * an inline `gridTemplateColumns` style instead.
 */
import type { ReactElement, ReactNode } from "react";

/** Exported for unit tests. */
export function columnCountForN(n: number): number {
  if (n <= 2) return 2;
  if (n <= 4) return 4;
  if (n <= 6) return 3;
  return 4;
}

export interface StrategyCardGridProps {
  count: number;
  children: ReactNode;
}

export function StrategyCardGrid({
  count,
  children,
}: StrategyCardGridProps): ReactElement {
  const cols = columnCountForN(Math.max(0, count));
  return (
    <div
      className="grid gap-3"
      style={{ gridTemplateColumns: `repeat(${cols}, minmax(0, 1fr))` }}
      data-testid="strategy-card-grid"
      data-cols={cols}
    >
      {children}
    </div>
  );
}
