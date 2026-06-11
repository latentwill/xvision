// frontend/web/src/components/home/CriticalFindingsRow.tsx
//
// Shows the top 5 severity=critical findings from the 3 most recent
// completed eval reviews. Always renders — never returns null.

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { listCriticalFindings } from "@/api/eval-review";
import { Pill } from "@/components/primitives/Pill";
import type { RunSummary } from "@/api/types.gen";
import type { CriticalFinding } from "@/api/eval-review";

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

function FindingChip({ finding }: { finding: CriticalFinding }) {
  return (
    <div className="flex-shrink-0 w-64 rounded border border-danger/30 bg-danger/5 p-2.5 flex flex-col gap-1.5">
      {/* Header row: severity pill + strategy name */}
      <div className="flex items-center gap-2 flex-wrap">
        <Pill tone="danger">critical</Pill>
        {finding.strategyName && (
          <span className="text-[11px] text-text-3 truncate max-w-[120px]">
            {finding.strategyName}
          </span>
        )}
      </div>

      {/* Summary — 2-line clamp */}
      <p className="text-[12px] text-text line-clamp-2 leading-snug flex-1">
        {finding.title ?? finding.summary}
      </p>

      {/* Action link */}
      <Link
        to={`/eval-runs/${finding.runId}`}
        className="text-[11px] text-danger hover:underline font-medium self-start"
      >
        Draft variant →
      </Link>
    </div>
  );
}

function EmptyState() {
  return (
    <p className="text-[12px] text-text-4">
      No critical findings in recent runs
    </p>
  );
}

// ---------------------------------------------------------------------------
// Main component
// ---------------------------------------------------------------------------

export interface CriticalFindingsRowProps {
  runs: RunSummary[];
}

export function CriticalFindingsRow({ runs }: CriticalFindingsRowProps) {
  const runIds = runs.slice(0, 3).map((r) => r.id);

  const { data, isPending } = useQuery({
    queryKey: ["critical-findings", runIds],
    queryFn: () => listCriticalFindings(runs),
    enabled: runs.length > 0,
  });

  return (
    <section data-testid="critical-findings-row" className="px-5 py-2.5">
      {/* Header */}
      <div className="mb-1.5 flex items-baseline gap-2">
        <span className="caps">Critical findings</span>
        <span className="text-[11px] text-text-4">
          from 3 most recent reviews
        </span>
      </div>

      {/* Body */}
      {isPending ? (
        // Loading skeleton — minimal, non-intrusive
        <div
          data-testid="critical-findings-loading"
          className="animate-pulse flex gap-3"
          aria-label="Loading critical findings"
        >
          <div className="h-20 w-64 rounded bg-surface-elev" />
          <div className="h-20 w-64 rounded bg-surface-elev" />
        </div>
      ) : !data || data.length === 0 ? (
        <EmptyState />
      ) : (
        <div className="overflow-x-auto xvn-scroll">
          <div className="flex gap-3 pb-1">
            {data.map((finding) => (
              <FindingChip key={finding.id} finding={finding} />
            ))}
          </div>
        </div>
      )}
    </section>
  );
}
