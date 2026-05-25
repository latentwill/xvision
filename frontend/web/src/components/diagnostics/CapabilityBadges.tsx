// CapabilityBadges — compact capability chips for an agent.
//
// Used on the agent-list row (derived from the agent's slot `capabilities`
// arrays — no per-row diagnostics fetch, avoiding an N+1) and anywhere a
// terse "what can this agent do" summary is wanted. Trader/Filter (the
// optimizable, runtime-supported capabilities) read gold; the rest read
// muted so an operator sees at a glance which capabilities can actually
// launch today.

import { Pill } from "@/components/primitives/Pill";
import type { Capability } from "@/api/agents";

// Mirror of `xvision_engine::diagnostics::is_runtime_supported` /
// `OPTIMIZABLE_CAPABILITIES`: Trader + Filter are the live, optimizable
// capabilities; Critic/Intern/Router persist but have no runtime handler
// yet. Kept as a hand-maintained mirror, like the engine's dspy-free const.
const LIVE_CAPABILITIES: ReadonlySet<Capability> = new Set([
  "trader",
  "filter",
]);

const CAP_LABEL: Record<Capability, string> = {
  trader: "Trader",
  filter: "Filter",
  critic: "Critic",
  intern: "Intern",
  router: "Router",
};

export function CapabilityBadges({
  capabilities,
  className = "",
}: {
  capabilities: Capability[];
  className?: string;
}) {
  if (capabilities.length === 0) {
    return (
      <span className="text-text-3 text-[11px]" data-testid="cap-badges-empty">
        no capabilities
      </span>
    );
  }
  // De-dupe while preserving a stable order (trader, filter, …).
  const order: Capability[] = ["trader", "filter", "critic", "intern", "router"];
  const present = order.filter((c) => capabilities.includes(c));
  return (
    <span
      className={`inline-flex flex-wrap items-center gap-1 ${className}`}
      data-testid="cap-badges"
    >
      {present.map((c) => (
        <Pill
          key={c}
          tone={LIVE_CAPABILITIES.has(c) ? "gold" : "default"}
          title={
            LIVE_CAPABILITIES.has(c)
              ? `${CAP_LABEL[c]} — supported at runtime`
              : `${CAP_LABEL[c]} — no runtime handler yet`
          }
          data-testid={`cap-badge-${c}`}
        >
          {CAP_LABEL[c]}
        </Pill>
      ))}
    </span>
  );
}
