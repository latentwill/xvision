/**
 * MonthlyReturnsHeatmap — N strategies × M months cell grid.
 *
 * Spec §4A.1 (B1): per-cell color is gold-for-positive,
 * danger-red-for-negative; alpha scales linearly with the magnitude
 * of the return out to a configurable ceiling (default 15%), then
 * clamps. Legend bar runs `-ceiling%  ↔  +ceiling%` along the bottom.
 *
 * Pure SVG/DOM — no chart library. The cell-color computation is
 * exported as `cellAlpha` for unit testing.
 */
import type { ReactElement } from "react";

export interface MonthlyRow {
  id: string;
  label: string;
  /** `cells.length` must be the same across every row in the same chart. */
  cells: Array<{ year: number; month: number; value: number }>;
}

export interface MonthlyReturnsHeatmapProps {
  rows: MonthlyRow[];
  /** Max-magnitude reference for alpha scaling. Defaults to 15%. */
  ceiling?: number;
  /** Floor alpha so faint cells stay visible (default 0.10). */
  minAlpha?: number;
  /** Ceiling alpha (default 0.65). */
  maxAlpha?: number;
}

/**
 * Compute the cell alpha for a value, clamped between `minAlpha` and
 * `maxAlpha`. Exported for unit tests.
 *
 * `value` and `ceiling` are in the same units (e.g. percent: 0.10 = 10%
 * — note these are absolute returns, not basis points).
 */
export function cellAlpha(
  value: number,
  ceiling: number,
  minAlpha: number,
  maxAlpha: number,
): number {
  if (!Number.isFinite(value)) return minAlpha;
  const mag = Math.abs(value) / ceiling;
  const scaled = minAlpha + (maxAlpha - minAlpha) * Math.min(1, Math.max(0, mag));
  return Math.min(maxAlpha, Math.max(minAlpha, scaled));
}

function formatMonthShort(year: number, month: number): string {
  // 1-indexed month → "Jan", "Feb", …
  const names = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun",
    "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
  ];
  return `${names[Math.max(0, Math.min(11, month - 1))]} '${String(year).slice(2)}`;
}

export function MonthlyReturnsHeatmap({
  rows,
  ceiling = 0.15,
  minAlpha = 0.10,
  maxAlpha = 0.65,
}: MonthlyReturnsHeatmapProps): ReactElement {
  // Use the first row to drive the column headers. All rows must have
  // the same `cells.length` (asserted in dev via a console.warn rather
  // than a throw so a bad fixture doesn't crash the whole dashboard).
  const months = rows[0]?.cells ?? [];
  if (
    process.env.NODE_ENV !== "production" &&
    rows.some((r) => r.cells.length !== months.length)
  ) {
    // eslint-disable-next-line no-console
    console.warn(
      "[MonthlyReturnsHeatmap] inconsistent cells.length across rows",
      rows.map((r) => ({ id: r.id, n: r.cells.length })),
    );
  }

  return (
    <div className="border border-border rounded-card bg-surface-card p-4">
      <header className="caps mb-3">Monthly Returns</header>

      <div
        className="grid gap-x-px gap-y-px text-[10.5px] text-text-3"
        style={{
          gridTemplateColumns: `minmax(80px, max-content) repeat(${months.length}, minmax(28px, 1fr))`,
          fontFamily: 'Geist Mono, ui-monospace, monospace',
        }}
        role="table"
        aria-label="Monthly returns heatmap"
      >
        {/* Header row: empty corner + month labels */}
        <div role="rowheader" />
        {months.map((m) => (
          <div
            key={`${m.year}-${m.month}`}
            role="columnheader"
            className="text-center px-1 py-1 text-text-3"
          >
            {formatMonthShort(m.year, m.month)}
          </div>
        ))}

        {/* Body rows: strategy label + N cells */}
        {rows.map((r) => (
          <div key={r.id} className="contents">
            <div
              role="rowheader"
              className="pr-3 py-1 text-text-2 text-[12px] truncate"
            >
              {r.label}
            </div>
            {r.cells.map((c, i) => {
              const alpha = cellAlpha(c.value, ceiling, minAlpha, maxAlpha);
              const baseHex = c.value >= 0 ? "0,230,118" : "255,77,77";
              return (
                <div
                  key={`${r.id}-${i}`}
                  role="cell"
                  title={`${formatMonthShort(c.year, c.month)} · ${(c.value * 100).toFixed(2)}%`}
                  className="text-center py-1 text-[10.5px] tabular-nums text-text"
                  style={{ backgroundColor: `rgba(${baseHex}, ${alpha.toFixed(3)})` }}
                >
                  {(c.value * 100).toFixed(0)}
                </div>
              );
            })}
          </div>
        ))}
      </div>

      <footer className="mt-3 flex items-center justify-between gap-3 text-[10.5px] text-text-3">
        <span>−{(ceiling * 100).toFixed(0)}%</span>
        <div
          className="h-2 flex-1 rounded-sm"
          style={{
            background:
              "linear-gradient(to right, rgba(255,77,77,0.55), rgba(255,77,77,0.10), rgba(0,230,118,0.10), rgba(0,230,118,0.55))",
          }}
          aria-hidden="true"
        />
        <span>+{(ceiling * 100).toFixed(0)}%</span>
      </footer>
    </div>
  );
}
