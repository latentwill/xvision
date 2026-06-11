import type { BoardCard } from "../selectors/buildBoardState";
import { ExpandableArtifact } from "./ExpandableArtifact";

const stateChip: Record<BoardCard["state"], string> = {
  queued: "text-text-4",
  evaluating: "text-warn animate-pulse",
  kept: "text-gold",
  rejected: "text-danger",
  suspect: "text-warn",
};

const stateLabel: Record<BoardCard["state"], string> = {
  queued: "queued",
  evaluating: "evaluating…",
  kept: "kept",
  rejected: "rejected",
  suspect: "suspect",
};

function fmtDelta(delta: number): string {
  // Use unicode minus (−) for negative, + for non-negative
  return delta >= 0 ? `+${delta.toFixed(2)}` : `−${Math.abs(delta).toFixed(2)}`;
}

export function ExperimentBoard({
  cards,
  defaultOpenHash,
  expandBoard,
}: {
  cards: BoardCard[];
  /** Task 14: open the card whose hash matches this value on mount */
  defaultOpenHash?: string;
  /** Task 14: open all cards on mount when true */
  expandBoard?: boolean;
}) {
  if (cards.length === 0) return null;

  // mobile: no grid-cols class at base breakpoint → single-column list.
  // sm: two columns, lg: three columns.
  return (
    <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
      {cards.map((c) => {
        const isDefaultOpen =
          expandBoard === true || (defaultOpenHash !== undefined && c.hash === defaultOpenHash);

        const summaryChip = stateChip[c.state];
        const summaryLabel = stateLabel[c.state];
        const deltaStr = c.delta != null ? ` ${fmtDelta(c.delta)}` : "";

        return (
          <ExpandableArtifact
            key={c.hash}
            hash={c.hash}
            defaultOpen={isDefaultOpen}
            writerModel={c.writer}
            summary={
              <span className="flex items-center gap-2 font-mono text-[12px]">
                <span className="text-text-2">{c.hash.slice(0, 8)}</span>
                {c.label && (
                  <span className="truncate text-text-3">{c.label}</span>
                )}
                <span className={summaryChip}>
                  {summaryLabel}
                  {deltaStr}
                </span>
              </span>
            }
          />
        );
      })}
    </div>
  );
}
