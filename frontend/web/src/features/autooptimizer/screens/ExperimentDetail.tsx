import { useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import { useExperimentDetail, formatGateVerdict } from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";
import { GateScorecard } from "../panels/GateScorecard";
import { FindingsList } from "../panels/FindingsList";
import { RegimeCards } from "../panels/RegimeCards";

function SectionHeader({ title, hint }: { title: string; hint?: string }) {
  return (
    <div className="space-y-0.5">
      <h2 className="m-0 text-[15px] font-semibold tracking-tight">{title}</h2>
      {hint && <p className="m-0 text-[11px] text-text-3">{hint}</p>}
    </div>
  );
}

/**
 * Standalone "researcher report" for a single optimizer experiment (lineage
 * node). Five inline, full-width sections — no right sidebar (the chat rail
 * owns the right edge) and no popups. Backed by the wired
 * `/api/autooptimizer/experiments/:hash/detail` endpoint via
 * {@link useExperimentDetail}.
 *
 * Section → wired-field map:
 *   1. Why tested      → `rationale`
 *   2. What happened    → `regime_results`
 *   3. Gate scorecard   → `gate_record`
 *   4. Decision         → `lineage_node.gate_verdict` + `status`
 *   5. Reviewer notes   → `findings`
 */
export function ExperimentDetail() {
  const { hash = "" } = useParams<{ hash: string }>();
  const { data, isLoading, isError } = useExperimentDetail(hash);

  return (
    <>
      <Topbar
        title="Optimizer"
        sub="Experiment report"
        back={{ to: "/optimizer", label: "Back to Optimizer" }}
      />
      <div className="space-y-5">
        <Breadcrumb
          items={[
            { label: "OPTIMIZER", to: "/optimizer" },
            { label: "experiment" },
            { label: hash.slice(0, 10) },
          ]}
        />

        {isLoading ? (
          <p className="text-[12px] text-text-3">Loading experiment…</p>
        ) : isError || !data ? (
          <p className="text-[12px] text-danger">
            Couldn't load this experiment.
          </p>
        ) : (
          <>
            {/* ── Hero ─────────────────────────────────────────────────────── */}
            <section className="flex items-start gap-4 rounded-md border border-border bg-surface-card p-5">
              <HashSigil hash={hash} size={72} />
              <div className="min-w-0 flex-1">
                <div className="mb-1 flex items-center gap-2">
                  <span className="text-[8.5px] uppercase tracking-widest text-text-3">
                    Optimizer · Experiment
                  </span>
                  <GateBadge
                    verdict={formatGateVerdict(data.lineage_node.gate_verdict)}
                    status={data.lineage_node.status}
                  />
                </div>
                <h1 className="m-0 font-mono text-[22px] tracking-tight">
                  {hash.slice(0, 16)}
                </h1>
                <p className="mt-1 font-mono text-[11px] text-text-3">
                  parent{" "}
                  {data.lineage_node.parent_hash
                    ? data.lineage_node.parent_hash.slice(0, 10)
                    : "— (root)"}
                  {data.lineage_node.cycle_id
                    ? ` · cycle ${data.lineage_node.cycle_id}`
                    : ""}
                </p>
              </div>
            </section>

            {/* ── 1. Why tested ────────────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader
                title="Why this was tested"
                hint="The hypothesis the experiment writer set out to validate."
              />
              {data.rationale ? (
                <p className="text-[13px] leading-relaxed text-text-2">
                  {data.rationale}
                </p>
              ) : (
                <p className="text-[12px] text-text-3">
                  No rationale recorded for this experiment.
                </p>
              )}
            </section>

            {/* ── 2. What happened ─────────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader
                title="What happened"
                hint="How the candidate performed across each evaluation regime."
              />
              <RegimeCards results={data.regime_results} />
            </section>

            {/* ── 3. Gate scorecard ────────────────────────────────────────── */}
            <section className="space-y-3">
              <SectionHeader
                title="Gate scorecard"
                hint="Candidate vs baseline on the day and untouched windows, against the min-improvement threshold."
              />
              <GateScorecard gate_record={data.gate_record} />
            </section>

            {/* ── 4. Decision ──────────────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader title="Decision" />
              <div className="flex items-center gap-3">
                <GateBadge
                  verdict={formatGateVerdict(data.lineage_node.gate_verdict)}
                  status={data.lineage_node.status}
                />
                <span className="font-mono text-[12px] text-text-2">
                  {formatGateVerdict(data.lineage_node.gate_verdict)}
                </span>
              </div>
              {data.gate_record?.reason ? (
                <p className="text-[12px] leading-relaxed text-text-3">
                  {data.gate_record.reason}
                </p>
              ) : (
                <p className="text-[12px] text-text-3">
                  No machine decision rationale recorded.
                </p>
              )}
            </section>

            {/* ── 5. Reviewer notes ────────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader
                title="Reviewer notes"
                hint="Findings the judge raised while reviewing this experiment."
              />
              <FindingsList findings={data.findings} />
            </section>
          </>
        )}
      </div>
    </>
  );
}
