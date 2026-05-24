/**
 * Topbar — dashboard chrome row: tracked-uppercase eyebrow + Cormorant
 * headline + optional Cormorant-italic tagline, with a right-side
 * action slot for env pills, timeframe toggles, export buttons, etc.
 *
 * Used by all four Track-B canvases (B1/B2/B3/B4). Plain DOM — no
 * uPlot or chart-lib dependencies.
 *
 * Note: there's a `Topbar` already at `@/components/shell/Topbar` used
 * by `chart-lab`. That one is the SHELL topbar (different sizing).
 * Renamed here to `ChartsTopbar` would be clearer in callers, but the
 * primitive name follows the design-handoff terminology — callers
 * import via the namespaced `@/components/chart/v2/primitives` path so
 * the shell `Topbar` is reachable via its own path.
 */
import type { ReactElement, ReactNode } from "react";

export interface ChartsTopbarProps {
  /** `.caps` eyebrow. Optional — omit for chart-only frames (B3 header). */
  eyebrow?: string;
  /** Headline in Cormorant. Required. */
  headline: ReactNode;
  /** Italic tagline below the headline, also Cormorant. Optional. */
  tagline?: ReactNode;
  /** Right-side action cluster (pills, toggles, buttons). */
  actions?: ReactNode;
}

export function ChartsTopbar({
  eyebrow,
  headline,
  tagline,
  actions,
}: ChartsTopbarProps): ReactElement {
  return (
    <header className="flex items-start justify-between gap-6 pb-4">
      <div className="min-w-0 flex-1">
        {eyebrow && <div className="caps mb-1">{eyebrow}</div>}
        <h1
          className="text-[30px] leading-[1.1] tracking-[-0.015em] text-text font-medium"
          style={{ fontFamily: '"Cormorant Garamond", serif' }}
        >
          {headline}
        </h1>
        {tagline != null && (
          <p
            className="mt-1 text-[15px] text-text-2 italic"
            style={{ fontFamily: '"Cormorant Garamond", serif' }}
          >
            {tagline}
          </p>
        )}
      </div>
      {actions != null && (
        <div className="flex items-center gap-2 shrink-0">{actions}</div>
      )}
    </header>
  );
}
