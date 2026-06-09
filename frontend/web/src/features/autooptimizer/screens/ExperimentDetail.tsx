import { Link, useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import {
  useLineageNode,
  useExperimentRegimeResults,
  useExperimentDetail,
  formatGateVerdict,
} from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";
import { EmptyPanel } from "../ui/EmptyPanel";
import { ParentDiffPanel } from "../panels/ParentDiffPanel";
import { RegimeCards } from "../panels/RegimeCards";
import { GateScorecard } from "../panels/GateScorecard";
import { FindingsList } from "../panels/FindingsList";

function SectionHeader({ title }: { title: string }) {
  return (
    <h2 className="m-0 text-[15px] font-semibold tracking-tight">{title}</h2>
  );
}

export function ExperimentDetail() {
  const { hash = "" } = useParams<{ hash: string }>();
  const { data: node, isLoading, isError } = useLineageNode(hash);
  const { results: regimeResults, isLoading: regimeLoading } =
    useExperimentRegimeResults(hash, node?.cycle_id ?? undefined);
  // Detail endpoint — may not be available in older backend versions; fails gracefully.
  const { data: detail } = useExperimentDetail(hash);

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
            {/* ── Hero card ─────────────────────────────────────────────────── */}
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
                <Link
                  to={`/optimizer/strategy/${encodeURIComponent(node.bundle_hash)}`}
                  className="mt-2 inline-block text-[11px] text-brand underline hover:opacity-80"
                >
                  View strategy →
                </Link>
              </div>
            </section>

            {/* ── Section 1: Why tested ──────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader title="Why tested" />
              {detail?.rationale ? (
                <p className="text-[13px] text-text-2 leading-relaxed">{detail.rationale}</p>
              ) : (
                <p className="text-[12px] text-text-3">No rationale recorded</p>
              )}
              {/* ParentDiffPanel computes client-side diff — kept unchanged per spec */}
              <ParentDiffPanel childHash={node.bundle_hash} parentHash={node.parent_hash} />
            </section>

            {/* ── Section 2: What happened ───────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader title="What happened" />
              <p className="text-[12px] text-text-3">
                Phase timeline — lights up once event linkage per experiment ships.
              </p>
            </section>

            {/* ── Section 3: The numbers ─────────────────────────────────────── */}
            <section className="space-y-3">
              <SectionHeader title="The numbers" />
              <GateScorecard gate_record={detail?.gate_record ?? null} />
            </section>

            {/* ── Section 4: Decision ────────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader title="Decision" />
              <div className="flex items-center gap-3">
                <GateBadge verdict={formatGateVerdict(node.gate_verdict)} status={node.status} />
                {detail?.gate_record?.reason ? (
                  <p className="text-[13px] text-text-2">{detail.gate_record.reason}</p>
                ) : (
                  <p className="text-[12px] text-text-3">
                    {formatGateVerdict(node.gate_verdict) === "Pending"
                      ? "Gate evaluation pending"
                      : "No detailed reason recorded"}
                  </p>
                )}
              </div>
            </section>

            {/* ── Section 5: Reviewer notes ──────────────────────────────────── */}
            <section className="space-y-3">
              <SectionHeader title="Reviewer notes" />
              <FindingsList findings={detail?.findings ?? []} />
            </section>

            {/* ── Existing sections (kept) ───────────────────────────────────── */}
            <RegimeCards results={regimeResults} isLoading={regimeLoading} />
            <EmptyPanel title="Flight recorder" phase={3} hint="The structured trace (intern → trader → risk → execution) for this experiment, once trace linkage ships." />
            <EmptyPanel title="Sign-off receipts" phase={4} hint="Attester endorsements and the sign-off decision, once attesters ship." />
          </>
        )}
      </div>
    </>
  );
}
