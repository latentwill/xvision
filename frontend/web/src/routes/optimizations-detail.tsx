// /agents/:id/optimizations/:runId — optimizer run detail.
//
// Routed detail surface (NOT a popup): renders the candidate table, a
// before/after prompt DIFF (parent slot prompt vs. selected candidate
// instruction), the metric delta, the holdout split column, and the
// accept-as-child / revert actions plus an evidence export link.
//
// Long-running optimizations must not freeze the surface: the query polls on a
// background interval while the run is pending/running and the UI stays
// interactive. Failed optimizations preserve partial evidence — the detail
// endpoint returns whatever candidates were persisted, and this view renders
// them with a clear "failed" banner rather than an empty/error state.

import { useMemo, useState } from "react";
import { useParams, useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { PromptDiff } from "@/components/diagnostics/PromptDiff";
import { MintLineagePanel } from "@/components/diagnostics/MintLineagePanel";
import { ApiError } from "@/api/client";
import { agentKeys, getAgent } from "@/api/agents";
import {
  acceptOptimization,
  getOptimization,
  optimizationKeys,
  recordOptimizationHoldout,
  revertOptimization,
  waiveOptimizationOverfit,
  type OptimizationCandidate,
  type RunDetail,
} from "@/api/optimizations";

const INFLIGHT = new Set(["pending", "running"]);

function statusTone(status: string): "info" | "warn" | "danger" | "default" {
  if (status === "completed") return "info";
  if (status === "failed") return "danger";
  if (INFLIGHT.has(status)) return "warn";
  return "default";
}

function fmtMetric(v: number | null): string {
  if (v === null || v === undefined) return "—";
  return v.toFixed(4);
}

/// The metric delta = selected candidate metric − baseline (candidate_index 0)
/// metric. Returns null when either is missing.
function metricDelta(candidates: OptimizationCandidate[]): number | null {
  const selected = candidates.find((c) => c.selected);
  const baseline =
    candidates.find((c) => c.candidate_index === 0) ?? candidates[0];
  if (!selected || !baseline) return null;
  if (selected.metric_value === null || baseline.metric_value === null)
    return null;
  return selected.metric_value - baseline.metric_value;
}

export function OptimizationDetailRoute() {
  const { id: agentId, runId } = useParams<{ id: string; runId: string }>();
  const rid = runId ?? "";
  const aid = agentId ?? "";
  const qc = useQueryClient();
  const navigate = useNavigate();

  // Advanced detail (MIPRO/GEPA internals) collapsed by default — operator-
  // friendly summary is the default surface.
  const [showAdvanced, setShowAdvanced] = useState(false);
  const [trainMetric, setTrainMetric] = useState("");
  const [holdoutMetric, setHoldoutMetric] = useState("");
  const [overrideReason, setOverrideReason] = useState("");
  const [waiverReason, setWaiverReason] = useState("");

  const q = useQuery<RunDetail, ApiError>({
    queryKey: optimizationKeys.detail(rid),
    queryFn: () => getOptimization(rid),
    enabled: rid.length > 0,
    // Background-poll while in-flight so a long optimization streams its
    // candidates in without the operator refreshing — and stop once terminal.
    refetchInterval: (query) =>
      query.state.data && INFLIGHT.has(query.state.data.run.status)
        ? 3_000
        : false,
  });

  const detail = q.data;
  const run = detail?.run;

  // The parent agent gives us the "before" prompt for the diff.
  const parentQ = useQuery({
    queryKey: agentKeys.detail(run?.agent_id ?? aid),
    queryFn: () => getAgent(run?.agent_id ?? aid),
    enabled: Boolean(run?.agent_id ?? aid),
  });

  const selected = useMemo(
    () => detail?.candidates.find((c) => c.selected) ?? null,
    [detail],
  );

  const beforePrompt = useMemo(() => {
    if (!parentQ.data || !run) return "";
    const slot = parentQ.data.slots.find((s) => s.name === run.slot_name);
    return slot?.system_prompt ?? "";
  }, [parentQ.data, run]);

  const afterPrompt = selected?.instruction ?? "";

  // The accepted snapshot (if any) determines accept vs. revert affordance.
  const acceptedSnapshot = detail?.snapshots.find((s) => s.accepted) ?? null;
  const latestSnapshot = detail?.snapshots[0] ?? null;
  const lineageChild = detail?.lineage[0] ?? null;
  const latestHoldout =
    latestSnapshot && detail
      ? (detail.holdouts ?? []).find((h) => h.snapshot_id === latestSnapshot.id) ??
        null
      : null;
  const acceptedHoldout =
    acceptedSnapshot && detail
      ? (detail.holdouts ?? []).find(
          (h) => h.snapshot_id === acceptedSnapshot.id,
        ) ?? null
      : null;
  const canAccept =
    Boolean(latestSnapshot && selected) &&
    (Boolean(latestHoldout) || overrideReason.trim().length > 0);

  const acceptMut = useMutation({
    mutationFn: () => {
      if (!latestSnapshot) throw new Error("no snapshot to accept");
      return acceptOptimization(
        rid,
        latestSnapshot.id,
        undefined,
        latestHoldout ? undefined : overrideReason.trim(),
      );
    },
    onSuccess: () => {
      setOverrideReason("");
      qc.invalidateQueries({ queryKey: optimizationKeys.detail(rid) });
      qc.invalidateQueries({ queryKey: agentKeys.all });
    },
  });

  const revertMut = useMutation({
    mutationFn: () => {
      if (!acceptedSnapshot) throw new Error("nothing accepted to revert");
      if (!lineageChild) throw new Error("no lineage child to revert");
      return revertOptimization(
        rid,
        acceptedSnapshot.id,
        lineageChild.child_agent_id,
      );
    },
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: optimizationKeys.detail(rid) });
      qc.invalidateQueries({ queryKey: agentKeys.all });
    },
  });

  const holdoutMut = useMutation({
    mutationFn: () => {
      if (!latestSnapshot) throw new Error("no snapshot to score");
      const train = Number(trainMetric);
      const holdout = Number(holdoutMetric);
      if (!Number.isFinite(train) || !Number.isFinite(holdout)) {
        throw new Error("train and holdout metrics must be numbers");
      }
      return recordOptimizationHoldout(rid, latestSnapshot.id, {
        metric: run?.metric ?? "metric",
        trainMetricValue: train,
        holdoutMetricValue: holdout,
      });
    },
    onSuccess: () => {
      setTrainMetric("");
      setHoldoutMetric("");
      qc.invalidateQueries({ queryKey: optimizationKeys.detail(rid) });
    },
  });

  const waiveMut = useMutation({
    mutationFn: () => {
      if (!latestSnapshot) throw new Error("no snapshot to waive");
      const reason = waiverReason.trim();
      if (!reason) throw new Error("waiver reason is required");
      return waiveOptimizationOverfit(rid, latestSnapshot.id, reason);
    },
    onSuccess: () => {
      setWaiverReason("");
      qc.invalidateQueries({ queryKey: optimizationKeys.detail(rid) });
    },
  });

  if (q.isLoading) {
    return (
      <div className="max-w-5xl mx-auto" data-testid="optimization-loading">
        <Topbar title="Improve agent" sub="Loading optimization run…" />
        <Card className="px-5 py-8 text-sm text-text-3">Loading…</Card>
      </div>
    );
  }

  if (q.isError || !detail || !run) {
    const msg =
      q.error instanceof ApiError ? q.error.message : "Run not found.";
    return (
      <div className="max-w-5xl mx-auto" data-testid="optimization-error">
        <Topbar
          title="Improve agent"
          sub="Could not load this optimization run."
          back={{ to: `/agents/${aid}`, label: "Back to agent" }}
        />
        <Card className="px-5 py-6 text-sm text-danger">{msg}</Card>
      </div>
    );
  }

  const delta = metricDelta(detail.candidates);
  const failed = run.status === "failed";

  return (
    <div className="max-w-5xl mx-auto" data-testid="optimization-detail">
      <Topbar
        title="Improve this agent"
        sub={
          <span>
            Reviewing an optimization run for slot{" "}
            <span className="font-medium text-text">{run.slot_name}</span>.
          </span>
        }
        back={{ to: `/agents/${run.agent_id}`, label: "Back to agent" }}
      />

      {/* Summary card — operator-friendly, optimizer name behind "advanced". */}
      <Card className="mb-6">
        <div className="px-5 py-4 flex flex-wrap items-center gap-4">
          <Pill tone={statusTone(run.status)} data-testid="opt-status">
            {run.status}
          </Pill>
          <div className="text-sm">
            <span className="text-text-3">Result metric</span>{" "}
            <span className="font-mono text-text" data-testid="opt-metric">
              {fmtMetric(selected?.metric_value ?? null)}
            </span>
          </div>
          <div className="text-sm">
            <span className="text-text-3">Improvement</span>{" "}
            <span
              className={
                delta !== null && delta > 0
                  ? "font-mono text-success"
                  : delta !== null && delta < 0
                    ? "font-mono text-danger"
                    : "font-mono text-text-2"
              }
              data-testid="opt-delta"
            >
              {delta === null
                ? "—"
                : `${delta > 0 ? "+" : ""}${delta.toFixed(4)}`}
            </span>
          </div>
          <button
            type="button"
            className="ml-auto text-[12px] text-text-3 hover:text-text underline-offset-2 hover:underline"
            onClick={() => setShowAdvanced((v) => !v)}
            data-testid="opt-advanced-toggle"
          >
            {showAdvanced ? "Hide advanced" : "Advanced detail"}
          </button>
        </div>

        {showAdvanced ? (
          <div
            className="px-5 pb-4 grid grid-cols-2 gap-x-6 gap-y-1 text-[12px] text-text-2 border-t border-border pt-3"
            data-testid="opt-advanced"
          >
            <div>
              <span className="text-text-3">Optimizer</span> {run.optimizer}
            </div>
            <div>
              <span className="text-text-3">Metric</span> {run.metric}
            </div>
            <div>
              <span className="text-text-3">Capability</span> {run.capability}
            </div>
            <div>
              <span className="text-text-3">Seed</span> {run.rng_seed}
            </div>
            <div>
              <span className="text-text-3">Model</span>{" "}
              {run.model_provider ?? "—"}/{run.model_name ?? "—"}
            </div>
            <div className="truncate">
              <span className="text-text-3">Corpus</span> {run.corpus_query}
            </div>
            {run.optimizer_version ? (
              <div>
                <span className="text-text-3">Version</span>{" "}
                {run.optimizer_version}
              </div>
            ) : null}
            {run.signature_hash ? (
              <div className="truncate">
                <span className="text-text-3">Signature</span>{" "}
                <span className="font-mono">{run.signature_hash}</span>
              </div>
            ) : null}
          </div>
        ) : null}
      </Card>

      {failed ? (
        <Card className="mb-6 px-5 py-3 border-danger/30 bg-danger/5 dark:bg-danger/10 text-sm text-danger">
          This optimization run failed. The candidates below are the partial
          evidence captured before it stopped — review them, but the result may
          be incomplete.
        </Card>
      ) : null}

      {/* Before / after prompt diff (shared PromptDiff component). */}
      <PromptDiff
        className="mb-6"
        before={beforePrompt}
        after={afterPrompt}
        beforeLabel={`Current (${run.slot_name})`}
        afterLabel="Optimized"
        caption="before → after (selected candidate)"
      />

      {/* Mint / lineage panel — surfaces the marketplace-mint refusals and
          the accepted child's lineage. The accept→swap reversible flow lives
          in the actions card below. */}
      {acceptedSnapshot && lineageChild ? (
        <MintLineagePanel
          runId={rid}
          capability={run.capability}
          childAgentId={lineageChild.child_agent_id}
          className="mb-6"
        />
      ) : null}

      {/* Candidate table. */}
      <Card className="mb-6">
        <div className="px-5 pt-4 pb-2">
          <h2 className="m-0 text-[15px] font-medium">Candidates</h2>
        </div>
        <div className="overflow-x-auto">
          <table className="w-full text-[13px]" data-testid="candidate-table">
            <thead>
              <tr className="text-text-3 text-left border-b border-border">
                <th className="px-5 py-2 font-medium">#</th>
                <th className="px-3 py-2 font-medium">Instruction</th>
                <th className="px-3 py-2 font-medium">Metric</th>
                <th className="px-3 py-2 font-medium">Split</th>
                <th className="px-5 py-2 font-medium">Selected</th>
              </tr>
            </thead>
            <tbody>
              {detail.candidates.length === 0 ? (
                <tr>
                  <td
                    colSpan={5}
                    className="px-5 py-6 text-center text-text-3"
                  >
                    No candidates recorded yet.
                  </td>
                </tr>
              ) : (
                detail.candidates.map((c) => (
                  <tr
                    key={c.id}
                    className={
                      c.selected
                        ? "border-b border-border bg-success/5 dark:bg-success/10"
                        : "border-b border-border"
                    }
                    data-testid={`candidate-row-${c.candidate_index}`}
                  >
                    <td className="px-5 py-2 font-mono text-text-2">
                      {c.candidate_index}
                    </td>
                    <td className="px-3 py-2 max-w-md">
                      <span className="line-clamp-2 text-text-2">
                        {c.instruction}
                      </span>
                    </td>
                    <td className="px-3 py-2 font-mono">
                      {fmtMetric(c.metric_value)}
                    </td>
                    <td className="px-3 py-2">
                      <span className="text-text-3">{c.split}</span>
                    </td>
                    <td className="px-5 py-2">
                      {c.selected ? (
                        <Pill tone="gold">winner</Pill>
                      ) : (
                        <span className="text-text-3">—</span>
                      )}
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      </Card>

      {/* Actions. */}
      <Card className="mb-10">
        <div className="px-5 py-4 flex flex-col gap-4">
          {acceptedSnapshot ? (
            <div className="flex flex-wrap items-center gap-3">
              <Pill tone="info">Accepted</Pill>
              {acceptedHoldout ? (
                <Pill tone={acceptedHoldout.overfit_warning ? "warn" : "info"}>
                  holdout {fmtMetric(acceptedHoldout.holdout_metric_value)}
                </Pill>
              ) : null}
              {lineageChild ? (
                <button
                  type="button"
                  className="text-[13px] text-accent hover:underline"
                  onClick={() =>
                    navigate(`/agents/${lineageChild.child_agent_id}`)
                  }
                  data-testid="view-child-agent"
                >
                  View child agent →
                </button>
              ) : null}
              <button
                type="button"
                className="ml-auto px-3 py-1.5 rounded border border-danger/40 text-danger text-[13px] hover:bg-danger/5 disabled:opacity-50"
                disabled={revertMut.isPending || !lineageChild}
                onClick={() => revertMut.mutate()}
                data-testid="revert-button"
              >
                {revertMut.isPending ? "Reverting…" : "Reject / revert"}
              </button>
            </div>
          ) : (
            <>
              <div className="flex flex-wrap items-center gap-3">
                <span className="text-[13px] text-text-2">
                  Accept the winning candidate as a new child agent. Your
                  current agent stays unchanged.
                </span>
                {latestHoldout ? (
                  <Pill tone={latestHoldout.overfit_warning ? "warn" : "info"}>
                    holdout {fmtMetric(latestHoldout.holdout_metric_value)}
                  </Pill>
                ) : (
                  <Pill tone="warn">holdout missing</Pill>
                )}
                <button
                  type="button"
                  className="ml-auto px-3 py-1.5 rounded bg-accent text-on-accent text-[13px] font-medium hover:opacity-90 disabled:opacity-50"
                  disabled={acceptMut.isPending || !canAccept}
                  onClick={() => acceptMut.mutate()}
                  data-testid="accept-button"
                >
                  {acceptMut.isPending ? "Accepting…" : "Accept as child agent"}
                </button>
              </div>

              {latestSnapshot && !latestHoldout ? (
                <div className="grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_minmax(0,1fr)_auto] gap-2">
                  <input
                    className="rounded border border-border bg-surface-elev px-3 py-2 text-[13px] text-text"
                    inputMode="decimal"
                    placeholder="Train metric"
                    value={trainMetric}
                    onChange={(e) => setTrainMetric(e.target.value)}
                    data-testid="holdout-train-input"
                  />
                  <input
                    className="rounded border border-border bg-surface-elev px-3 py-2 text-[13px] text-text"
                    inputMode="decimal"
                    placeholder="Holdout metric"
                    value={holdoutMetric}
                    onChange={(e) => setHoldoutMetric(e.target.value)}
                    data-testid="holdout-value-input"
                  />
                  <button
                    type="button"
                    className="px-3 py-2 rounded border border-border text-[13px] hover:bg-surface-2 disabled:opacity-50"
                    disabled={holdoutMut.isPending}
                    onClick={() => holdoutMut.mutate()}
                    data-testid="record-holdout-button"
                  >
                    {holdoutMut.isPending ? "Recording…" : "Record holdout"}
                  </button>
                  <input
                    className="md:col-span-3 rounded border border-border bg-surface-elev px-3 py-2 text-[13px] text-text"
                    placeholder="Override reason"
                    value={overrideReason}
                    onChange={(e) => setOverrideReason(e.target.value)}
                    data-testid="holdout-override-input"
                  />
                </div>
              ) : null}

              {latestSnapshot &&
              latestHoldout?.overfit_warning &&
              !latestHoldout.overfit_waiver_reason ? (
                <div className="grid grid-cols-1 md:grid-cols-[minmax(0,1fr)_auto] gap-2">
                  <input
                    className="rounded border border-border bg-surface-elev px-3 py-2 text-[13px] text-text"
                    placeholder="Overfit waiver reason"
                    value={waiverReason}
                    onChange={(e) => setWaiverReason(e.target.value)}
                    data-testid="overfit-waiver-input"
                  />
                  <button
                    type="button"
                    className="px-3 py-2 rounded border border-warn/50 text-warn text-[13px] hover:bg-warn/5 disabled:opacity-50"
                    disabled={waiveMut.isPending || !waiverReason.trim()}
                    onClick={() => waiveMut.mutate()}
                    data-testid="waive-overfit-button"
                  >
                    {waiveMut.isPending ? "Waiving…" : "Waive overfit"}
                  </button>
                </div>
              ) : null}
            </>
          )}
          <a
            href={`/api/optimizations/${encodeURIComponent(rid)}`}
            target="_blank"
            rel="noreferrer"
            className="text-[12px] text-text-3 hover:text-text underline-offset-2 hover:underline"
            data-testid="evidence-export"
          >
            Export evidence (JSON)
          </a>
        </div>
        {(acceptMut.isError ||
          revertMut.isError ||
          holdoutMut.isError ||
          waiveMut.isError) && (
          <div
            className="px-5 pb-4 text-[13px] text-danger"
            data-testid="action-error"
          >
            {(acceptMut.error as Error)?.message ??
              (revertMut.error as Error)?.message ??
              (holdoutMut.error as Error)?.message ??
              (waiveMut.error as Error)?.message}
          </div>
        )}
      </Card>
    </div>
  );
}
