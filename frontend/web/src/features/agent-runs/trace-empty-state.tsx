// frontend/web/src/features/agent-runs/trace-empty-state.tsx
//
// Honest, actionable empty state for the trace tree. The tree can read as
// empty for FIVE distinct reasons, but the legacy message always blamed
// "the current filter" — wrong (and dead-ends the operator) when no filter
// is active. The common real-world case: a sticky URL/localStorage filter
// (`?q=kind:…`) leaks across runs and hides every span, with no obvious
// recovery. This component names the actual reason and offers the matching
// escape hatch (Clear filters / Show Advanced).

export type EmptyTreeReason =
  | "live-waiting" // a running run that hasn't emitted spans yet
  | "no-spans" // a finished run that recorded no spans at all
  | "filtered" // spans exist but the q/kind/status/decision filter hides them
  | "simple-hidden" // spans survived the filter but Simple view hides them all
  | "all-hidden"; // remaining spans are internal markers, hidden in every view

/**
 * Decide WHY the trace tree is empty so the dock can show an honest message
 * instead of always claiming a filter is responsible. Returns `null` when
 * there ARE rows to render (the caller renders the tree).
 *
 * @param sourceCount  total spans on the run, pre-filter
 * @param filteredCount spans after the q/kind/status/decision filter
 * @param displayCount  spans after the view-mode hidden-kinds (what the tree gets)
 * @param filterActive  whether any q/kind/status/decision filter is set
 * @param isLive        the run is currently streaming
 * @param advancedView  the dock is in Advanced (vs Simple) view
 */
export function emptyTreeReason(args: {
  sourceCount: number;
  filteredCount: number;
  displayCount: number;
  filterActive: boolean;
  isLive: boolean;
  advancedView: boolean;
}): EmptyTreeReason | null {
  const { sourceCount, filteredCount, displayCount, filterActive, isLive, advancedView } = args;
  if (displayCount > 0) return null;
  if (sourceCount === 0) return isLive ? "live-waiting" : "no-spans";
  // Source spans exist but none reach the tree.
  if (filterActive && filteredCount === 0) return "filtered";
  // Spans survived the q/kind filter but the view-mode hides them all.
  return advancedView ? "all-hidden" : "simple-hidden";
}

const ACTION_BTN_CLASS =
  "font-mono text-[11px] px-2 py-1 rounded border border-border text-text-2 " +
  "hover:text-text hover:bg-surface-elev transition-colors";

export function TraceEmptyState({
  reason,
  hiddenCount,
  onClearFilters,
  onShowAdvanced,
}: {
  reason: EmptyTreeReason;
  /** Count of spans the Simple view is hiding (for the simple-hidden / all-hidden copy). */
  hiddenCount: number;
  onClearFilters: () => void;
  onShowAdvanced: () => void;
}) {
  const plural = hiddenCount === 1 ? "span" : "spans";
  const verb = hiddenCount === 1 ? "is" : "are";

  let message: string;
  let action: React.ReactNode = null;

  switch (reason) {
    case "live-waiting":
      message = "Waiting for spans…";
      break;
    case "no-spans":
      message = "No spans were recorded for this run.";
      break;
    case "filtered":
      message = "No spans match the current filter.";
      action = (
        <button type="button" className={ACTION_BTN_CLASS} onClick={onClearFilters}>
          Clear filters
        </button>
      );
      break;
    case "simple-hidden":
      message = `${hiddenCount} instrumentation ${plural} ${verb} hidden in Simple view.`;
      action = (
        <button type="button" className={ACTION_BTN_CLASS} onClick={onShowAdvanced}>
          Show Advanced
        </button>
      );
      break;
    case "all-hidden":
      message = `${hiddenCount} internal marker ${plural} (no inspectable detail).`;
      break;
  }

  return (
    <div
      className="font-mono text-[12px] text-text-3 p-3 flex flex-col items-start gap-2"
      aria-label="no spans"
      data-empty-reason={reason}
    >
      <span>{message}</span>
      {action}
    </div>
  );
}
