import { useEffect, useRef, useState } from "react";
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
  // Open state is lifted here (Bug 6) so an expanded card's grid cell can span
  // the full row instead of staying a skinny 1/3-width column.
  //
  // Model: each card defaults open when expandBoard is set or its hash matches
  // defaultOpenHash (this also covers cards that arrive asynchronously after
  // mount); user toggles are stored as per-hash overrides on top.
  const [overrides, setOverrides] = useState<Map<string, boolean>>(
    () => new Map(),
  );

  // A ?exp= change on the same page should re-open the newly targeted card
  // even if the user previously collapsed it.
  const prevDefault = useRef(defaultOpenHash);
  useEffect(() => {
    if (defaultOpenHash !== undefined && defaultOpenHash !== prevDefault.current) {
      setOverrides((prev) => {
        if (!prev.has(defaultOpenHash)) return prev;
        const next = new Map(prev);
        next.delete(defaultOpenHash);
        return next;
      });
    }
    prevDefault.current = defaultOpenHash;
  }, [defaultOpenHash]);

  if (cards.length === 0) return null;

  const defaultOpenFor = (hash: string) =>
    expandBoard === true || hash === defaultOpenHash;

  const isOpenFor = (hash: string) =>
    overrides.get(hash) ?? defaultOpenFor(hash);

  const toggle = (hash: string) =>
    setOverrides((prev) => {
      const next = new Map(prev);
      next.set(hash, !isOpenFor(hash));
      return next;
    });

  // mobile: no grid-cols class at base breakpoint → single-column list.
  // sm: two columns, lg: three columns.
  return (
    <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
      {cards.map((c) => {
        const isOpen = isOpenFor(c.hash);

        const summaryChip = stateChip[c.state];
        const summaryLabel = stateLabel[c.state];
        const deltaStr = c.delta != null ? ` ${fmtDelta(c.delta)}` : "";

        return (
          <div key={c.hash} className={isOpen ? "col-span-full" : undefined}>
            <ExpandableArtifact
              hash={c.hash}
              open={isOpen}
              onToggle={() => toggle(c.hash)}
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
          </div>
        );
      })}
    </div>
  );
}
