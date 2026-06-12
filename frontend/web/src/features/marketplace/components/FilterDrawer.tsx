// src/features/marketplace/components/FilterDrawer.tsx
// Inline filter accordion (spec 3.1C, QA4). NOT an absolute overlay / Dialog /
// Sheet / Popover — it renders in normal document flow and pushes the list down
// (no "stuck open" problem, no click-outside trap). Height is animated via a
// grid-template-rows 0fr → 1fr transition. The owner (BrowseRoute) wires an
// Escape-to-close useEffect and a "Done" button.
import type { ReactNode } from "react";

export function FilterDrawer({
  open,
  title = "Filter strategies",
  children,
}: {
  open: boolean;
  /** Optional accessible label for the in-flow region. */
  title?: string;
  children: ReactNode;
}) {
  return (
    <section
      aria-label={title}
      data-filter-accordion
      data-open={open || undefined}
      className="grid transition-[grid-template-rows] duration-200 ease-out border-b border-ink-rule"
      style={{ gridTemplateRows: open ? "1fr" : "0fr" }}
    >
      <div className="overflow-hidden min-h-0">{open && children}</div>
    </section>
  );
}
