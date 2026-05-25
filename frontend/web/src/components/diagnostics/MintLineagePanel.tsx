// MintLineagePanel — marketplace-mint refusal barrier + lineage surface.
//
// Routed/docked panel (NO popup): rendered inline on the optimization-run
// detail once a child agent has been accepted with recorded lineage. It is
// the operator-facing face of the engine's pure mint gate
// (`check_marketplace_mint`): a mint REQUIRES (a) optimization lineage,
// (b) an eval proof, (c) no UNWAIVED overfit warning, and (d) the
// capability's required-metric coverage. The gate REFUSES with a typed
// `mint_*` code; this panel turns each refusal into operator copy so the
// operator knows exactly what is missing before anything mints.
//
// The accept→swap reversible flow itself lives on the run-detail actions
// card (accept-as-child / reject-revert); this panel layers the marketplace
// gate on top of an already-accepted child and shows the attested decision
// on success.

import { useState } from "react";
import { useMutation } from "@tanstack/react-query";

import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  MINT_REFUSAL_CODES,
  mintOptimization,
  type MintDecision,
} from "@/api/optimizations";

// Mirror of the engine's per-capability required-metric battery
// (`xvision_engine::mint::metrics`). Hand-maintained, like the engine's
// dspy-free mirrors — used only to pre-fill the metrics field so the
// operator can see what coverage the gate expects.
const REQUIRED_METRICS: Record<string, string[]> = {
  trader: [
    "forward_return_agreement",
    "sharpe",
    "max_drawdown",
    "profit_factor",
    "calibration",
    "action_validity",
    "selectivity",
    "net_of_cost",
  ],
  filter: [
    "precision",
    "recall",
    "f1",
    "auroc",
    "wake_rate",
    "token_savings",
    "false_suppression",
  ],
};

function refusalRemediation(err: ApiError, childAgentId: string): string {
  switch (err.code) {
    case MINT_REFUSAL_CODES.missingLineage:
      return `Child ${childAgentId} has no optimization lineage for this run. Accept the run's candidate as a child first.`;
    case MINT_REFUSAL_CODES.missingEvalProof:
      return "Provide an eval run id — the marketplace listing needs scored evidence.";
    case MINT_REFUSAL_CODES.unwaivedOverfit:
      return "This snapshot has an unwaived overfit warning. Waive it (with a recorded reason) on the holdout before minting.";
    case MINT_REFUSAL_CODES.incompleteMetrics:
      return "The holdout proof is missing required metrics for this capability. Run a holdout that covers the full battery.";
    default:
      return err.message;
  }
}

export function MintLineagePanel({
  runId,
  capability,
  childAgentId,
  className = "",
}: {
  runId: string;
  capability: string;
  childAgentId: string;
  className?: string;
}) {
  const [evalRunId, setEvalRunId] = useState("");
  const [evalMetric, setEvalMetric] = useState("sharpe");
  const [decision, setDecision] = useState<MintDecision | null>(null);

  const required = REQUIRED_METRICS[capability] ?? [];

  const mintMut = useMutation({
    mutationFn: () =>
      mintOptimization(runId, {
        childAgentId,
        evalRunId: evalRunId.trim(),
        evalMetric: evalMetric.trim() || "sharpe",
        // Submit the capability's full required battery as the holdout
        // coverage claim. The gate validates it against the recorded
        // holdout proof server-side.
        metricsPresent: required,
      }),
    onSuccess: (res) => setDecision(res.decision),
    onError: () => setDecision(null),
  });

  const refusal =
    mintMut.error instanceof ApiError ? mintMut.error : null;
  const genericError =
    mintMut.error && !(mintMut.error instanceof ApiError)
      ? String((mintMut.error as Error).message ?? mintMut.error)
      : null;

  return (
    <Card className={className} data-testid="mint-lineage-panel">
      <div className="px-5 pt-4 pb-2 flex items-center justify-between">
        <h2 className="m-0 text-[15px] font-medium">Marketplace mint</h2>
        <span className="text-[12px] text-text-3">
          lineage · eval proof · no unwaived overfit · metric coverage
        </span>
      </div>

      <div className="px-5 pb-4 text-[13px] text-text-2">
        <div className="mb-3">
          Minting <span className="font-medium text-text">{childAgentId}</span>{" "}
          ({capability}) to marketplace metadata requires it to pass the engine
          mint gate. Supply the eval run that scored this child as the proof of
          performance.
        </div>

        <div className="flex flex-wrap items-end gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-[11px] text-text-3">Eval run id (proof)</span>
            <input
              type="text"
              value={evalRunId}
              onChange={(e) => setEvalRunId(e.target.value)}
              placeholder="eval run id"
              data-testid="mint-eval-run-id"
              className="w-64 rounded border border-border bg-surface-elev px-2 py-1 text-[13px] text-text focus:border-border-strong focus:outline-none"
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-[11px] text-text-3">Eval metric</span>
            <input
              type="text"
              value={evalMetric}
              onChange={(e) => setEvalMetric(e.target.value)}
              data-testid="mint-eval-metric"
              className="w-40 rounded border border-border bg-surface-elev px-2 py-1 text-[13px] text-text focus:border-border-strong focus:outline-none"
            />
          </label>
          <button
            type="button"
            disabled={mintMut.isPending || evalRunId.trim().length === 0}
            onClick={() => mintMut.mutate()}
            data-testid="mint-button"
            className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent hover:opacity-90 disabled:opacity-50"
          >
            {mintMut.isPending ? "Checking…" : "Check & mint"}
          </button>
        </div>

        {required.length > 0 ? (
          <div className="mt-2 text-[11px] text-text-3">
            Required holdout metrics for {capability}: {required.join(", ")}
          </div>
        ) : null}
      </div>

      {refusal ? (
        <div
          className="mx-5 mb-4 rounded border border-danger/30 bg-danger/5 dark:bg-danger/10 px-4 py-3"
          role="alert"
          data-testid="mint-refusal"
        >
          <div className="flex items-center gap-2 mb-1">
            <Pill tone="danger">Mint refused</Pill>
            <span className="font-mono text-[11px] text-text-3">
              {refusal.code}
            </span>
          </div>
          <div className="text-[13px] text-text-2">
            {refusalRemediation(refusal, childAgentId)}
          </div>
        </div>
      ) : null}

      {genericError ? (
        <div
          className="mx-5 mb-4 rounded border border-danger/30 bg-danger/5 dark:bg-danger/10 px-4 py-3"
          role="alert"
          data-testid="mint-generic-error"
        >
          <div className="flex items-center gap-2 mb-1">
            <Pill tone="danger">Mint check failed</Pill>
          </div>
          <div className="text-[13px] text-text-2">{genericError}</div>
        </div>
      ) : null}

      {decision ? (
        <div
          className="mx-5 mb-4 rounded border border-success/30 bg-success/5 dark:bg-success/10 px-4 py-3"
          data-testid="mint-decision"
        >
          <div className="flex items-center gap-2 mb-1">
            <Pill tone="info">Mint allowed</Pill>
            {decision.overfit_waived ? (
              <Pill tone="warn">overfit waived</Pill>
            ) : null}
          </div>
          <div className="text-[13px] text-text-2">
            Provenance attested: eval run{" "}
            <span className="font-mono text-text">{decision.eval_run_id}</span>
            {decision.holdout_snapshot_id ? (
              <>
                {" "}
                · holdout snapshot{" "}
                <span className="font-mono text-text">
                  {decision.holdout_snapshot_id}
                </span>
              </>
            ) : null}
            .
          </div>
        </div>
      ) : null}
    </Card>
  );
}
