import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";
import { compareRuns, evalKeys } from "@/api/eval";
import { listStrategies, strategyKeys } from "@/api/strategies";
import { EmptyState } from "@/components/chart/v2/primitives/EmptyState";
import { ComparisonABDashboard } from "@/components/chart/v2/surfaces/ComparisonABDashboard";
import { useCompareSelection } from "@/components/chart/v2/hooks/useCompareSelection";

export function ChartsCompare() {
  const selection = useCompareSelection();
  const ids = selection.selectedIds;
  const compare = useQuery({
    queryKey: evalKeys.compare(ids),
    queryFn: () => compareRuns(ids),
    enabled: ids.length >= 2,
  });
  const strategies = useQuery({
    queryKey: strategyKeys.list(),
    queryFn: listStrategies,
  });

  if (ids.length < 2) {
    return (
      <EmptyState
        title="Compare needs two runs"
        message="Select two or more eval runs from the run list, or open this route with ?ids=<run-a>,<run-b>."
      />
    );
  }

  if (compare.isPending) {
    return (
      <div className="rounded-card border border-border bg-surface-card p-6">
        <div className="mb-3 h-6 w-80 animate-pulse rounded-sm bg-surface-elev" />
        <div className="h-4 w-56 animate-pulse rounded-sm bg-surface-elev" />
      </div>
    );
  }

  if (compare.isError || !compare.data) {
    return (
      <EmptyState
        title="Compare unavailable"
        message={compare.error instanceof Error ? compare.error.message : "The compare report could not be loaded."}
      />
    );
  }

  return (
    <div>
      <ComparisonABDashboard
        report={compare.data}
        selection={selection}
        strategies={strategies.data ?? []}
      />
      <div className="mt-4 text-[12px] text-text-3">
        <Link to="/eval-runs" className="hover:text-text hover:underline">
          Manage eval runs
        </Link>
      </div>
    </div>
  );
}
