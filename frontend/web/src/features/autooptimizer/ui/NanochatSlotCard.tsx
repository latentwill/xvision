// NanochatSlotCard — inline checkpoint picker for the strategy authoring page.
//
// Design constraints (CLAUDE.md):
//  - INLINE only: renders as a full-width card within the page's center column.
//  - NO Dialog / Sheet / Popover / overlay of any kind.
//  - NO right-sidebar / col-span-4 / grid-cols-12 layout.
//  - Dark-mode borders: border-border / border-border-soft, never border-white.

import { useEffect, useMemo, useRef } from "react";
import { useSearchParams } from "react-router-dom";
import {
  useNanochatCheckpoints,
  parseInputSpec,
  type NanochatCheckpoint,
} from "@/api/nanochat";
import { NanochatBacktestFlow } from "./NanochatBacktestFlow";
import { SignalSearchableSelectMenu } from "@/components/primitives/SignalMenu";

export type NanochatSlotCardProps = {
  strategyId: string;
  agentRefRole: string;
  /** Indicator names currently available in the strategy's tool registry. */
  availableIndicators: string[];
  /** Currently selected checkpoint model_id, or null if none selected. */
  checkpointModelId: string | null;
  /** Veto mode: true = hard gate (block trade on NEUTRAL), false = advisory. */
  veto: boolean;
  onCheckpointChange: (model_id: string | null) => void;
  onVetoChange: (veto: boolean) => void;
  /** Called whenever the compatibility state changes.
   *  `false` = save should be blocked (required indicators missing). */
  onCompatibilityChange?: (compatible: boolean) => void;
};

/** Returns the list of required indicators missing from `available`.
 *  Reads `checkpoint.input_spec` via `parseInputSpec` — never accesses it
 *  as a direct object (it is a raw JSON string on the wire). */
function getMissingIndicators(
  checkpoint: NanochatCheckpoint | null | undefined,
  available: string[],
): string[] {
  if (!checkpoint) return [];
  const spec = parseInputSpec(checkpoint.input_spec);
  if (!spec) return [];
  const avail = new Set(available);
  return spec.indicators.filter((ind) => !avail.has(ind));
}

export function NanochatSlotCard({
  strategyId,
  agentRefRole: _agentRefRole,
  availableIndicators,
  checkpointModelId,
  veto,
  onCheckpointChange,
  onVetoChange,
  onCompatibilityChange,
}: NanochatSlotCardProps) {
  const [params] = useSearchParams();
  const appliedAttachIdRef = useRef<string | null>(null);
  const checkpointsQ = useNanochatCheckpoints({ promoted_only: true });
  const checkpoints: NanochatCheckpoint[] = checkpointsQ.data ?? [];

  // Deep-link: ?attach_checkpoint=<model_id> pre-selects the checkpoint.
  // Only fires once when checkpointModelId is null (nothing already selected).
  useEffect(() => {
    const attachId = params.get("attach_checkpoint");
    if (
      attachId &&
      checkpointModelId == null &&
      appliedAttachIdRef.current !== attachId
    ) {
      appliedAttachIdRef.current = attachId;
      onCheckpointChange(attachId);
    }
  }, [params, checkpointModelId, onCheckpointChange]);

  const selectedCheckpoint = useMemo(
    () => checkpoints.find((c) => c.model_id === checkpointModelId) ?? null,
    [checkpoints, checkpointModelId],
  );

  const missing = getMissingIndicators(selectedCheckpoint, availableIndicators);
  const compatible = missing.length === 0;

  // Notify parent whenever compatibility changes so it can block save.
  useEffect(() => {
    onCompatibilityChange?.(compatible);
  }, [compatible, onCompatibilityChange]);

  const isCandidate =
    selectedCheckpoint != null && !selectedCheckpoint.live_approved;

  return (
    <div className="space-y-3 rounded-md border border-border bg-card p-4">
      {/* Section label — matches sibling card conventions in this directory */}
      <div className="text-[11px] font-semibold uppercase tracking-[0.18em] text-text-3">
        Nanochat filter slot
      </div>

      {/* Checkpoint picker — promoted models only */}
      <div className="flex flex-col gap-1">
        <div className="text-[12px] text-text-3">
          Nanochat model
        </div>
        <SignalSearchableSelectMenu
          ariaLabel="Nanochat model"
          value={checkpointModelId ?? ""}
          options={[
            {
              value: "",
              label: "No checkpoint",
              meta: "Clear nanochat model",
              searchText: "none no checkpoint clear",
            },
            ...checkpoints.map((checkpoint) => ({
              value: checkpoint.model_id,
              label: checkpoint.live_approved
                ? checkpoint.display_name
                : `${checkpoint.display_name} (candidate)`,
              meta: checkpoint.model_id,
              searchText: `${checkpoint.display_name} ${checkpoint.model_id}`,
            })),
          ]}
          onChange={(next) => onCheckpointChange(next || null)}
          placeholder="— select a checkpoint —"
          searchPlaceholder="Search checkpoints…"
          emptyHint="No checkpoints found"
          loading={checkpointsQ.isLoading}
          className="w-full justify-between"
        />
        {checkpointsQ.isLoading && (
          <span className="text-[11px] text-text-3">Loading checkpoints…</span>
        )}
      </div>

      {/* Candidate warning badge — shown when the selected model has not been
          live-approved. Reminds operator to run a backtest before going live. */}
      {isCandidate && (
        <div
          role="status"
          className="inline-flex items-center gap-1.5 rounded-full border border-amber-500/30 bg-amber-500/10 px-2.5 py-1 text-[12px] text-amber-400"
        >
          Candidate — backtest before use
        </div>
      )}

      {/* Veto toggle — hard gate vs. advisory */}
      <label className="flex cursor-pointer items-center gap-2">
        <input
          type="checkbox"
          checked={veto}
          onChange={(e) => onVetoChange(e.target.checked)}
          className="h-4 w-4 rounded border border-border bg-surface-elev accent-accent"
        />
        <span className="text-[13px] text-text-2">
          Block trade on NEUTRAL (hard gate)
        </span>
      </label>

      {/* Compatibility section — only shown when a checkpoint is selected */}
      {selectedCheckpoint && (
        <div className="flex flex-col gap-2">
          {/* Green / red badge */}
          <div className="flex items-center gap-2">
            <span
              className={[
                "inline-flex items-center gap-1 rounded-full px-2.5 py-0.5 text-[12px] font-medium",
                compatible
                  ? "bg-green-500/10 text-green-400"
                  : "bg-danger/10 text-danger",
              ].join(" ")}
            >
              <span
                className={[
                  "h-1.5 w-1.5 rounded-full",
                  compatible ? "bg-green-400" : "bg-danger",
                ].join(" ")}
              />
              {compatible ? "Compatible" : "Incompatible"}
            </span>
          </div>

          {/* Incompatibility detail — lists missing indicators and three
              remediation steps inline (no popup, per CLAUDE.md). */}
          {!compatible && (
            <div className="space-y-2 rounded-md border border-danger/20 bg-danger/[0.05] p-3 text-[12px]">
              <p className="font-medium text-danger">
                Missing indicators required by this checkpoint:
              </p>
              <ul className="list-inside list-disc space-y-0.5 text-text-2">
                {missing.map((ind) => (
                  <li key={ind}>
                    <code className="font-mono text-[11px]">{ind}</code>
                  </li>
                ))}
              </ul>
              <p className="mt-2 text-text-3">Remediation — choose one:</p>
              <ol className="list-inside list-decimal space-y-1 text-text-2">
                <li>
                  Add{" "}
                  {missing.map((ind, i) => (
                    <span key={ind}>
                      <code className="font-mono text-[11px]">{ind}</code>
                      {i < missing.length - 1 ? ", " : ""}
                    </span>
                  ))}{" "}
                  to this strategy's tools
                </li>
                <li>Pick a different checkpoint whose indicators match</li>
                <li>Remove the nanochat slot</li>
              </ol>
            </div>
          )}
        </div>
      )}

      {/* Backtest + approve gate — shown inline when a candidate checkpoint is
          selected and the strategy is already compatible (no missing indicators).
          Operator-triggered only: never auto-fires. */}
      {isCandidate && compatible && selectedCheckpoint && (
        <NanochatBacktestFlow
          strategyId={strategyId}
          checkpointModelId={selectedCheckpoint.model_id}
          onApproved={() => {
            // Cache invalidation is handled by useApproveCheckpoint's onSuccess;
            // the checkpoints query will refetch and live_approved flips to true.
          }}
        />
      )}
    </div>
  );
}
