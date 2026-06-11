// frontend/web/src/components/home/PulseViewSwitcher.tsx
//
// Chip row for the Pulse band chart views. Renders as its own full-width
// sub-row below the band header (the header row is already crowded at small
// breakpoints). No popups; selection is plain buttons with aria-pressed.

import type { ReactElement } from "react";
import type { PulseView } from "@/features/home/pulse";

const VIEW_LABELS: Record<PulseView, string> = {
  return: "Return %",
  trades: "Price + trades",
  hold: "vs Buy & Hold",
  drawdown: "Drawdown",
  field: "All runs",
};

export interface PulseViewSwitcherProps {
  view: PulseView;
  onViewChange: (view: PulseView) => void;
}

export function PulseViewSwitcher({
  view,
  onViewChange,
}: PulseViewSwitcherProps): ReactElement {
  return (
    <div
      data-testid="pulse-view-switcher"
      className="flex flex-wrap items-center gap-1.5 px-5 pb-2"
      role="group"
      aria-label="Chart view"
    >
      {(Object.keys(VIEW_LABELS) as PulseView[]).map((v) => (
        <button
          key={v}
          type="button"
          aria-pressed={view === v}
          onClick={() => onViewChange(v)}
          className={`rounded-sm border px-2.5 py-1 text-[11px] font-medium uppercase tracking-wide transition-colors ${
            view === v
              ? "border-gold/40 text-gold"
              : "border-border-soft text-text-3 hover:text-text"
          }`}
        >
          {VIEW_LABELS[v]}
        </button>
      ))}
    </div>
  );
}
