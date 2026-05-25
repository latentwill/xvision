// PromptDiff — a before/after slot-prompt diff card.
//
// Extracted from `routes/optimizations-detail.tsx` so the slot
// prompt-diff / optimized-snapshot view can be reused outside the
// optimization-run surface (e.g. an agent-detail slot inspector). The
// "after" pane reads green to signal the proposed/optimized value; the
// "before" pane is neutral. No diff library — a side-by-side full-text
// view keeps the dependency surface small and survives a mobile column
// stack (the grid collapses to one column below `md`).

import { Card } from "@/components/primitives/Card";

export function PromptDiff({
  before,
  after,
  beforeLabel = "Current",
  afterLabel = "Optimized",
  title = "Prompt change",
  caption,
  className = "",
}: {
  before: string;
  after: string;
  beforeLabel?: string;
  afterLabel?: string;
  title?: string;
  caption?: string;
  className?: string;
}) {
  return (
    <Card className={className}>
      <div className="px-5 pt-4 pb-2 flex items-center justify-between">
        <h2 className="m-0 text-[15px] font-medium">{title}</h2>
        {caption ? (
          <span className="text-[12px] text-text-3">{caption}</span>
        ) : null}
      </div>
      <div className="px-5 pb-5 grid grid-cols-1 md:grid-cols-2 gap-4">
        <div data-testid="prompt-before">
          <div className="text-[12px] text-text-3 mb-1">{beforeLabel}</div>
          <pre className="whitespace-pre-wrap break-words text-[12px] leading-relaxed bg-surface-elev border border-border rounded p-3 text-text-2 max-h-72 overflow-auto">
            {before || "—"}
          </pre>
        </div>
        <div data-testid="prompt-after">
          <div className="text-[12px] text-text-3 mb-1">{afterLabel}</div>
          <pre className="whitespace-pre-wrap break-words text-[12px] leading-relaxed bg-success/5 dark:bg-success/10 border border-success/30 rounded p-3 text-text max-h-72 overflow-auto">
            {after || "—"}
          </pre>
        </div>
      </div>
    </Card>
  );
}
