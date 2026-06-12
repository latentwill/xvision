// src/features/marketplace/routes/browse/SliceChips.tsx
// Slice navigation: a horizontal row of chips, one per slice with a real
// count > 0. Replaces the deleted LeaderboardRail. The strip renders only when
// at least one slice has count > 0 (with a small, complete marketplace it
// simply does not appear — by design, spec 3.1D / QA6).
import type { Slice, SliceId } from "@/features/marketplace/data/types";

interface SliceChipsProps {
  slices: Slice[];
  activeSliceId: SliceId | undefined;
  onSliceClick: (id: SliceId) => void;
}

export function SliceChips({ slices, activeSliceId, onSliceClick }: SliceChipsProps) {
  // Only show slices that have a real, live count.
  const visible = slices.filter((s) => s.count > 0);
  // Gate: render nothing unless at least one slice has a real count.
  if (visible.length === 0) return null;

  return (
    <div
      data-slice-chips
      className="px-4 sm:px-7 pb-3 pt-1 flex items-center gap-2 flex-wrap"
    >
      <span className="font-mono text-[9px] tracking-[0.18em] uppercase text-text-3 mr-0.5">
        Collections
      </span>
      {visible.map((s) => {
        const isActive = s.id === activeSliceId;
        return (
          <button
            key={s.id}
            type="button"
            data-testid={`slice-chip-${s.id}`}
            aria-pressed={isActive}
            title={s.hint}
            onClick={() => onSliceClick(s.id)}
            className={[
              "inline-flex items-center gap-1.5 px-2.5 py-1 rounded border",
              "text-[11.5px] transition-colors cursor-pointer",
              isActive
                ? "border-border-strong bg-surface-elev text-text"
                : "border-border bg-transparent text-text-2 hover:border-border-strong hover:text-text",
            ].join(" ")}
          >
            <span>{s.label}</span>
            <span className="font-mono text-text-3">{s.count.toLocaleString()}</span>
          </button>
        );
      })}
    </div>
  );
}
