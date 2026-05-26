// src/features/marketplace/components/X402Badge.tsx
import { AgentIcon } from "./AgentIcon";

export function X402Badge({ "data-testid": testId }: { "data-testid"?: string } = {}) {
  return (
    <span
      data-testid={testId}
      title="Accepts agent-paid auto-purchase (x402)"
      className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-gold/40 text-gold text-[10px] font-medium"
    >
      <AgentIcon size={10} />
      x402
    </span>
  );
}
