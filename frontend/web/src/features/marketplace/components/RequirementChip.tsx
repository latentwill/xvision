// RequirementChip.tsx — neutral chip for a manifest-declared requirement
// (model or tool). Deliberately carries NO installed/missing tone: the
// manifest tells us what the strategy needs, not what the viewer has.
import type { Requirement } from "../data/bundle";

export function RequirementChip({ requirement }: { requirement: Requirement }) {
  return (
    <span
      data-testid="requirement-chip"
      data-kind={requirement.kind}
      className="inline-flex items-center gap-1.5 px-2 py-0.5 rounded-sm border border-border bg-surface-elev font-mono text-[10.5px] text-text-2"
    >
      {requirement.name}
      <span className="text-[9px] tracking-[0.14em] text-text-3 uppercase">
        {requirement.kind}
      </span>
    </span>
  );
}
