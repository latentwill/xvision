import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { useCycleRun } from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { EmptyPanel } from "../ui/EmptyPanel";
import { ProgressDial } from "../ui/ProgressDial";
import { CycleExperimentsTable } from "../panels/CycleExperimentsTable";
import { LineageTreePanel } from "../panels/LineageTreePanel";
import { GateBuckets } from "../panels/GateBuckets";
import { EvalMatrix } from "../panels/EvalMatrix";

function stat(label: string, value: string, tone = "text-text") {
  return (
    <div className="flex flex-col">
      <span className="text-[8.5px] uppercase tracking-widest text-text-3">{label}</span>
      <span className={`font-mono text-[20px] ${tone}`}>{value}</span>
    </div>
  );
}

export function CycleDetail() {
  const { cycleId = "" } = useParams<{ cycleId: string }>();
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
          <section className="rounded-md border border-border bg-surface-card p-5">
            <span className="text-[8.5px] uppercase tracking-widest text-text-3">Cycle</span>
            <h1 className="m-0 mb-3 font-mono text-[22px] tracking-tight">{cycle.cycle_id}</h1>
            <div className="flex flex-wrap items-center gap-6">
              <ProgressDial
                value={cycle.node_count > 0 ? cycle.active_count / cycle.node_count : 0}
                label="KEPT"
                size={64}
              />
              <div className="grid grid-cols-2 gap-4 sm:grid-cols-4">
                {stat("Experiments", String(cycle.node_count))}
                {stat("Kept", String(cycle.active_count), "text-gold")}
                {stat("Dropped", String(cycle.rejected_count), "text-text-2")}
                {stat("$ spend", cycle.cost_usd == null ? "—" : `$${cycle.cost_usd.toFixed(2)}`)}
              </div>
            </div>
          </section>
        )}

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
