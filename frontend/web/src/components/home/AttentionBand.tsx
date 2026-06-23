// frontend/web/src/components/home/AttentionBand.tsx
//
// Home "live & attention" band (dashboard redesign §2): calm card with
// live-trading summary, active deployments, and config nags. Rows that
// have nothing to say render nothing, so the band shrinks gracefully.

import type { LiveDeploymentSummary } from "@/api/types.gen";
import { Card } from "@/components/primitives/Card";
import { ActiveTasksStrip } from "./ActiveTasksStrip";
import { LiveSummaryStrip } from "./LiveSummaryStrip";
import { NagStrip, type AttentionItem } from "./NagStrip";

export interface AttentionBandProps {
  /** Config + stale-infra-failure nags. */
  nagItems: AttentionItem[];
  /** Live/paper deployment rows from the home route's 5s poll. */
  deployments?: LiveDeploymentSummary[];
}

export function AttentionBand({
  nagItems,
  deployments,
}: AttentionBandProps) {
  return (
    <section data-testid="attention-band" aria-label="Live and attention">
      <Card className="p-0 overflow-hidden xvn-card-hover">
        <div className="divide-y divide-border-soft">
          <LiveSummaryStrip />
          <ActiveTasksStrip deployments={deployments} />
          <NagStrip items={nagItems} />
        </div>
      </Card>
    </section>
  );
}
