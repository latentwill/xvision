// src/features/marketplace/components/FilterDrawer.tsx
// Inline filter accordion (spec 3.1C, QA4). NOT an absolute overlay / Dialog /
// Sheet / Popover — it renders in normal document flow and pushes the list down
// (no "stuck open" problem, no click-outside trap). The reveal is INSTANT, not
// height-animated: transitions on grid-template-rows fr values or max-height
// freeze at their start value (panel reads as permanently collapsed) under
// animation-paused environments like CDP screenshot tooling, and the crisp
// state change keeps the reveal predictable. The content caps itself
// at 60vh (FilterDrawerContent) with a sticky footer. The owner (BrowseRoute)
// wires an Escape-to-close useEffect, a scroll-into-view on open, and a
// "Done" button.
import type { ReactNode, Ref } from "react";

export function FilterDrawer({
  open,
  title = "Filter strategies",
  children,
  sectionRef,
}: {
  open: boolean;
  /** Optional accessible label for the in-flow region. */
  title?: string;
  children: ReactNode;
  /** Optional ref to the section element (e.g. for scroll-into-view on open). */
  sectionRef?: Ref<HTMLElement>;
}) {
  return (
    <section
      ref={sectionRef}
      aria-label={title}
      data-filter-accordion
      data-open={open || undefined}
      className={open ? "shrink-0 border-b border-border" : "shrink-0"}
    >
      {open && <div className="overflow-hidden">{children}</div>}
    </section>
  );
}
