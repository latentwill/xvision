import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { useLineageNode, formatGateVerdict } from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";
import { EmptyPanel } from "../ui/EmptyPanel";
import { ParentDiffPanel } from "../panels/ParentDiffPanel";

export function ExperimentDetail() {
  const { hash = "" } = useParams<{ hash: string }>();
  const { data: node, isLoading, isError } = useLineageNode(hash);

  return (
    <>
      <Topbar title="Optimizer" sub="Experiment detail" back={{ to: "/optimizer", label: "Back to Optimizer" }} />
      <div className="space-y-5">
        <Breadcrumb
          items={[
            { label: "OPTIMIZER", to: "/optimizer" },
            { label: "cycle", to: node?.cycle_id ? `/optimizer/cycle/${encodeURIComponent(node.cycle_id)}` : undefined },
            { label: hash.slice(0, 10) },
          ]}
        />

        {isLoading ? (
          <p className="text-[12px] text-text-3">Loading experiment…</p>
        ) : isError || !node ? (
          <p className="text-[12px] text-danger">Couldn't load this experiment.</p>
        ) : (
          <>
            <section className="flex items-start gap-4 rounded-md border border-border bg-surface-card p-5">
              <HashSigil hash={node.bundle_hash} size={72} />
              <div className="min-w-0">
                <div className="mb-1 flex items-center gap-2">
                  <span className="text-[8.5px] uppercase tracking-widest text-text-3">Optimizer · Experiment</span>
                  <GateBadge verdict={formatGateVerdict(node.gate_verdict)} status={node.status} />
                </div>
                <h1 className="m-0 font-mono text-[22px] tracking-tight">{node.bundle_hash.slice(0, 16)}</h1>
                <p className="mt-1 font-mono text-[11px] text-text-3">
                  parent {node.parent_hash ? node.parent_hash.slice(0, 10) : "— (root)"} · cycle {node.cycle_id ?? "—"}
                </p>
              </div>
            </section>

            <ParentDiffPanel childHash={node.bundle_hash} parentHash={node.parent_hash} />

            <EmptyPanel title="Per-regime evaluation" phase={2} hint="Lights up when the regime matrix runs — Δ-Sharpe, return, drawdown, win-rate and an equity curve per regime." />
            <EmptyPanel title="Flight recorder" phase={3} hint="The structured trace (intern → trader → risk → execution) for this experiment, once trace linkage ships." />
            <EmptyPanel title="Sign-off receipts" phase={4} hint="Attester endorsements and the sign-off decision, once attesters ship." />
          </>
        )}
      </div>
    </>
  );
}
