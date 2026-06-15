// StrategyRequirementChip.tsx — a single model/skill/tool requirement on
// the Strategy detail page, rendered against the buyer's local config.
//
// Unlike the marketplace `RequirementChip` (which is deliberately neutral —
// the manifest says what a strategy needs, not what the viewer has), this
// chip carries a satisfied / missing tone because the engine has resolved
// each requirement against the operator's machine. Missing MODEL requirements
// gate the eval/go-live action; missing skills warn; tools are informational.
import type { Requirement } from "@/api/strategies";

export function StrategyRequirementChip({
  requirement,
}: {
  requirement: Requirement;
}) {
  const satisfied = requirement.satisfied;
  const tone = satisfied
    ? "border-border bg-surface-elev text-text-2"
    : "border-amber-500/40 bg-amber-500/10 text-amber-700 dark:border-amber-400/30 dark:bg-amber-400/10 dark:text-amber-300";

  return (
    <span
      data-testid="strategy-requirement-chip"
      data-kind={requirement.kind}
      data-satisfied={satisfied ? "true" : "false"}
      title={requirement.hint ?? undefined}
      className={`inline-flex items-center gap-1.5 rounded-sm border px-2 py-0.5 font-mono text-[10.5px] ${tone}`}
    >
      <span aria-hidden="true">{satisfied ? "✓" : "⚠"}</span>
      {requirement.name}
      <span className="text-[9px] uppercase tracking-[0.14em] text-text-3">
        {requirement.kind}
      </span>
    </span>
  );
}
