// NanochatBacktestFlow — operator-triggered backtest + approve gate.
//
// Design constraints (CLAUDE.md):
//  - OPERATOR-TRIGGERED only: the "Run backtest" button must be clicked
//    explicitly. No auto-fire on mount or prop change.
//  - INLINE only: no Dialog / Sheet / Popover / overlay of any kind.
//  - Dark-mode borders: border-border / border-border-soft, never border-white.

import { useCallback, useState } from "react";
import { startRun } from "@/api/eval";
import { useApproveCheckpoint } from "@/api/nanochat";

export type NanochatBacktestFlowProps = {
  /** The strategy whose backtest we launch. Maps to `agent_id` on StartRunReq. */
  strategyId: string;
  /** The candidate checkpoint model_id to be approved after a successful backtest. */
  checkpointModelId: string;
  /** Called after the checkpoint has been approved via useApproveCheckpoint. */
  onApproved?: () => void;
};

type BacktestState =
  | { phase: "idle" }
  | { phase: "running" }
  | { phase: "awaiting_confirm"; withRunId: string; baselineRunId: string }
  | { phase: "approving" }
  | { phase: "done" }
  | { phase: "error"; message: string };

export function NanochatBacktestFlow({
  strategyId,
  checkpointModelId,
  onApproved,
}: NanochatBacktestFlowProps) {
  const [state, setState] = useState<BacktestState>({ phase: "idle" });
  const approveMutation = useApproveCheckpoint();

  /** Launch two parallel eval runs:
   *  1. With checkpoint — params_override carries nanochat_checkpoint_model_id.
   *  2. Baseline — same strategy, no checkpoint override.
   *  Uses the existing `startRun` from @/api/eval (POST /api/eval/runs). */
  const handleRunBacktest = useCallback(async () => {
    setState({ phase: "running" });
    try {
      const [withResult, baselineResult] = await Promise.all([
        startRun({
          agent_id: strategyId,
          scenario_id: "",
          mode: "backtest",
          params_override: { nanochat_checkpoint_model_id: checkpointModelId },
        }),
        startRun({
          agent_id: strategyId,
          scenario_id: "",
          mode: "backtest",
          params_override: null,
        }),
      ]);
      setState({
        phase: "awaiting_confirm",
        withRunId: withResult.summary.id,
        baselineRunId: baselineResult.summary.id,
      });
    } catch (err) {
      setState({
        phase: "error",
        message:
          err instanceof Error ? err.message : "Failed to start backtest runs.",
      });
    }
  }, [strategyId, checkpointModelId]);

  const handleConfirm = useCallback(async () => {
    if (state.phase !== "awaiting_confirm") return;
    setState({ phase: "approving" });
    try {
      await approveMutation.mutateAsync(checkpointModelId);
      setState({ phase: "done" });
      onApproved?.();
    } catch (err) {
      setState({
        phase: "error",
        message: err instanceof Error ? err.message : "Approval failed.",
      });
    }
  }, [state, checkpointModelId, approveMutation, onApproved]);

  if (state.phase === "done") {
    return (
      <div className="rounded-md border border-green-500/20 bg-green-500/[0.05] p-3 text-[12px] text-green-400">
        Checkpoint approved. Save is now unblocked.
      </div>
    );
  }

  if (state.phase === "error") {
    return (
      <div className="rounded-md border border-danger/20 bg-danger/[0.05] p-3 text-[12px] text-danger">
        {state.message}
      </div>
    );
  }

  return (
    <div className="space-y-3 rounded-md border border-border bg-card p-4">
      <p className="text-[12px] text-text-2">
        This checkpoint is a candidate and not yet approved for live use. Run a
        backtest comparison (with vs. without this slot), then confirm the result
        to approve.
      </p>

      {state.phase === "idle" && (
        <button
          type="button"
          onClick={() => void handleRunBacktest()}
          className="rounded border border-border px-3 py-1.5 text-[13px] text-text-2 transition-colors hover:bg-surface-elev/40"
        >
          Run backtest
        </button>
      )}

      {state.phase === "running" && (
        <p className="text-[12px] text-text-3">Launching two eval runs…</p>
      )}

      {state.phase === "awaiting_confirm" && (
        <div className="space-y-2">
          <p className="text-[12px] text-text-2">Backtest runs launched:</p>
          <ul className="list-inside list-disc space-y-0.5 font-mono text-[12px] text-text-3">
            <li>
              With nanochat:{" "}
              <a
                href={`/eval-runs/${state.withRunId}`}
                className="text-accent hover:underline"
                target="_blank"
                rel="noopener noreferrer"
              >
                {state.withRunId}
              </a>
            </li>
            <li>
              Baseline:{" "}
              <a
                href={`/eval-runs/${state.baselineRunId}`}
                className="text-accent hover:underline"
                target="_blank"
                rel="noopener noreferrer"
              >
                {state.baselineRunId}
              </a>
            </li>
          </ul>
          <p className="text-[12px] text-text-3">
            Review the results, then confirm to approve this checkpoint.
          </p>
          <button
            type="button"
            onClick={() => void handleConfirm()}
            className="rounded bg-accent px-3 py-1.5 text-[13px] font-medium text-on-accent transition-opacity hover:opacity-90"
          >
            Confirm &amp; approve
          </button>
        </div>
      )}

      {state.phase === "approving" && (
        <p className="text-[12px] text-text-3">Approving…</p>
      )}
    </div>
  );
}
