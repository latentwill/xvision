// src/features/marketplace/components/RemovableChip.tsx
import type { ReactNode } from "react";

export function RemovableChip({ children, onRemove }: { children: ReactNode; onRemove: () => void }) {
  return (
    <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-sm border border-border-strong bg-surface-elev text-[11px] text-text-2">
      {children}
      <button
        type="button"
        aria-label="remove filter"
        onClick={onRemove}
        className="ml-0.5 leading-none text-text-3 hover:text-text"
      >
        ×
      </button>
    </span>
  );
}
