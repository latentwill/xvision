import { useParams, useSearchParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { useCycleRun, type CycleRunDetail } from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { ConsoleModule } from "../ui/ConsoleModule";
import { EditorialHeadline } from "../ui/EditorialHeadline";
import { EmptyPanel } from "../ui/EmptyPanel";
import { CycleExperimentsTable } from "../panels/CycleExperimentsTable";
import { LineageTreePanel } from "../panels/LineageTreePanel";
import { GateBuckets } from "../panels/GateBuckets";
import { EvalMatrix } from "../panels/EvalMatrix";

/** The cycle's best kept node: highest gate delta among `active` nodes (null-safe). */
function bestKeptFind(cycle: CycleRunDetail): { hash: string; delta: number } | null {
  let best: { hash: string; delta: number } | null = null;
  for (const n of cycle.nodes ?? []) {
    if (n.status !== "active") continue;
    const delta = (n as Record<string, unknown>).delta_day;
    if (typeof delta !== "number") continue;
    if (best === null || delta > best.delta) best = { hash: n.bundle_hash, delta };
  }
  return best;
}

function fmtDelta(d: number): string {
  return `${d >= 0 ? "+" : "−"}${Math.abs(d).toFixed(2)}`;
}

function headlineFor(cycle: CycleRunDetail): { title: string; subtitle: string } {
  const best = bestKeptFind(cycle);
  const bestClause = best
    ? ` — best find ${best.hash.slice(0, 8)}, ΔSharpe ${fmtDelta(best.delta)}.`
    : ".";
  const spend = cycle.cost_usd == null ? "$—" : `$${cycle.cost_usd.toFixed(2)}`;
  return {
    title: `Cycle ${cycle.cycle_id} kept ${cycle.active_count} of ${cycle.node_count} experiments${bestClause}`,
    subtitle: `${spend} · ${cycle.node_count} experiments`,
  };
}

export function CycleDetail() {
  const { cycleId = "" } = useParams<{ cycleId: string }>();
  const [searchParams] = useSearchParams();
  const exp = searchParams.get("exp") ?? undefined;
  const { data: cycle, isLoading, isError } = useCycleRun(cycleId);

  return (
    <>
      <Topbar title="Optimizer" sub="Cycle detail" back={{ to: "/optimizer", label: "Back to Optimizer" }} />
      <div className="space-y-5">
        <Breadcrumb items={[{ label: "OPTIMIZER", to: "/optimizer" }, { label: "cycle" }, { label: cycleId }]} />

        {isLoading ? (
          <p className="text-[12px] text-text-3">Loading cycle…</p>
        ) : isError || !cycle ? (
          <p className="text-[12px] text-danger">Couldn't load this cycle.</p>
        ) : (
          <EditorialHeadline headline={headlineFor(cycle)} digest={null} />
        )}

        <ConsoleModule
          cycleId={cycleId}
          expandBoard
          defaultOpenHash={exp}
          feedMaxItems={Number.POSITIVE_INFINITY}
        />

        {cycle ? (
          <>
            <GateBuckets kept={cycle.active_count} suspect={cycle.suspect_count ?? 0} dropped={cycle.rejected_count} />
            <EvalMatrix nodes={cycle.nodes ?? []} />
          </>
        ) : (
          <>
            <EmptyPanel title="Anti-overfit gate" phase={2} hint="Kept / Suspect / Dropped buckets appear once experiments are gated across the regime set." />
            <EmptyPanel title="Eval matrix" phase={2} hint="Experiments × regimes heat-map of Δ-Sharpe — lights up when the regime matrix runs." />
          </>
        )}

        <CycleExperimentsTable cycleId={cycleId} />

        <LineageTreePanel cycleId={cycleId} />

        <EmptyPanel title="Attester activity" phase={4} hint="Local attester sign-offs (endorse / question / reject) per experiment, once attesters ship." />
        <EmptyPanel title="Evening summary preview" phase={4} hint="The local, unpublished nightly summary of kept experiments, once sign-off ships." />
      </div>
    </>
  );
}
