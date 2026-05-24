/**
 * LeadCardChrome — HOC wrapper around a StrategyCard. When `lead=true`
 * the wrapped card gets the gold-tinted backdrop + border + 1px top
 * gradient line that distinguishes the lead strategy from the rest of
 * the grid (per handoff §02).
 *
 * Children render inside the chrome regardless; the chrome itself is
 * styling-only.
 */
import type { ReactElement, ReactNode } from "react";

export interface LeadCardChromeProps {
  lead: boolean;
  children: ReactNode;
}

export function LeadCardChrome({ lead, children }: LeadCardChromeProps): ReactElement {
  if (!lead) {
    return (
      <div className="relative overflow-hidden border border-border rounded-card bg-surface-card">
        {children}
      </div>
    );
  }
  return (
    <div
      className="relative overflow-hidden border rounded-card"
      style={{
        background:
          "linear-gradient(180deg, rgba(0,230,118,0.04), var(--surface-card) 38%)",
        borderColor: "rgba(0,230,118,0.28)",
      }}
      data-testid="lead-card-chrome"
    >
      <span
        aria-hidden="true"
        className="absolute top-0 left-0 right-0 h-px"
        style={{
          background:
            "linear-gradient(to right, transparent, rgba(0,230,118,0.7), transparent)",
        }}
      />
      {children}
    </div>
  );
}
