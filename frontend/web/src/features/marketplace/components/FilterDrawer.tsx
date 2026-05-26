// src/features/marketplace/components/FilterDrawer.tsx
// Docked right-edge panel. NOT a Dialog/Modal/Sheet/Popover (CLAUDE.md
// no-popups rule). It does not trap focus or paint a full-screen overlay
// owning the page; it docks over the list area while the rail/sidebar stay.
import type { ReactNode } from "react";

export function FilterDrawer({
  open,
  onClose,
  title = "Filter strategies",
  children,
}: {
  open: boolean;
  onClose: () => void;
  title?: string;
  children: ReactNode;
}) {
  if (!open) return null;
  return (
    <aside
      aria-label={title}
      className="absolute right-0 top-0 h-full w-[400px] bg-surface-panel border-l border-border shadow-xl flex flex-col"
    >
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <span className="font-sans font-medium text-[15px]">{title}</span>
        <button type="button" aria-label="close filters" onClick={onClose} className="text-text-3 hover:text-text">
          ×
        </button>
      </div>
      <div className="flex-1 overflow-y-auto px-4 py-3">{children}</div>
    </aside>
  );
}
