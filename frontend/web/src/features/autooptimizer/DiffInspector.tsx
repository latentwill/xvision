// DiffInspector — shows details for a single mutation/experiment node.
// Accepts :hash param from the /autooptimizer/diff/:hash route.
// Displays node metadata plus the stored strategy artifact for the experiment.

import { type ReactNode, useState } from "react";
import { Link, useParams } from "react-router-dom";
import { useQueryClient } from "@tanstack/react-query";
import { Card, CardHeader } from "@/components/primitives/Card";
import { ApiError } from "@/api/client";
import {
  useLineageNode,
  formatLineageStatus,
  formatGateVerdict,
  useBlob,
  useRetireLineageNode,
  autooptimizerKeys,
} from "./api";

export function DiffInspector() {
  const { hash } = useParams<{ hash: string }>();

  if (!hash) {
    return (
      <div className="text-[13px] text-text-3 py-4">
        No experiment hash specified.
      </div>
    );
  }

  return <DiffInspectorContent hash={hash} />;
}

function DiffInspectorContent({ hash }: { hash: string }) {
  const { data: node, isPending, isError } = useLineageNode(hash);
  const { data: childBlob, isPending: blobPending, isError: blobError } = useBlob(hash);
  const { data: parentBlob } = useBlob(node?.parent_hash);

  if (isPending) {
    return (
      <div className="text-[13px] text-text-3 py-4">Loading experiment…</div>
    );
  }

  if (isError || !node) {
    return (
      <div className="text-[13px] text-red-500 py-4">
        Experiment not found.{" "}
        <Link
          to="/autooptimizer"
          className="underline text-text-3 hover:text-text"
        >
          Back to genealogy
        </Link>
      </div>
    );
  }

  const statusLabel = formatLineageStatus(node.status);
  const verdictLabel = formatGateVerdict(node.gate_verdict);
  const paramChanges = buildMechanicalParamChanges(parentBlob, childBlob);
  const statusCls =
    node.status === "active"
      ? "bg-green-500/10 text-green-600 dark:text-green-400 border-green-500/30"
      : node.status === "quarantined"
        ? "bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 border-yellow-500/30"
        : "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/30";

  return (
    <div className="space-y-4">
      <div className="flex items-center gap-2">
        <Link
          to="/autooptimizer"
          className="text-[12px] text-text-3 hover:text-text transition-colors"
        >
          ← Genealogy
        </Link>
      </div>

      <Card>
        <CardHeader
          title={
            <span className="font-mono text-[18px]">{hash.slice(0, 16)}…</span>
          }
        />
        <div className="px-5 pb-5 space-y-4">
          {/* Metadata grid */}
          <div className="grid grid-cols-2 gap-x-8 gap-y-3 text-[13px]">
            <MetaRow label="Bundle hash">
              <span className="font-mono text-[12px] text-text break-all">
                {node.bundle_hash}
              </span>
            </MetaRow>

            {node.parent_hash && (
              <MetaRow label="Parent hash">
                <Link
                  to={`/autooptimizer/diff/${encodeURIComponent(node.parent_hash)}`}
                  className="font-mono text-[12px] text-text hover:underline break-all"
                >
                  {node.parent_hash}
                </Link>
              </MetaRow>
            )}

            <MetaRow label="Status">
              <span
                className={`inline-flex items-center px-1.5 py-0.5 rounded-sm border text-[11px] font-medium ${statusCls}`}
              >
                {statusLabel}
              </span>
            </MetaRow>

            <MetaRow label="Gate verdict">
              <span className="text-text">{verdictLabel}</span>
            </MetaRow>

            {node.cycle_id && (
              <MetaRow label="Cycle">
                <span className="font-mono text-[12px] text-text">
                  {node.cycle_id}
                </span>
              </MetaRow>
            )}

            <MetaRow label="Created">
              <span className="text-text">{formatDateTime(node.created_at)}</span>
            </MetaRow>

            {node.diversity_score != null && (
              <MetaRow label="Diversity score">
                <span className="text-text font-mono">
                  {node.diversity_score.toFixed(4)}
                </span>
              </MetaRow>
            )}

          </div>

          {/* F29: retire (move to Rejected) — dashboard parity for
              `xvn optimizer retire`. */}
          <RetireAction hash={node.bundle_hash} isRejected={node.status === "rejected"} />
        </div>
      </Card>

      <Card>
        <CardHeader title="Experiment diff" />
        <div className="px-5 pb-5 space-y-4">
          {paramChanges.length > 0 && (
            <div className="rounded border border-border overflow-hidden">
              <table className="w-full text-[13px] border-collapse">
                <thead>
                  <tr className="bg-surface-card border-b border-border">
                    <th className="text-left font-medium text-text-3 px-3 py-2">Parameter</th>
                    <th className="text-left font-medium text-text-3 px-3 py-2">Before</th>
                    <th className="text-left font-medium text-text-3 px-3 py-2">After</th>
                  </tr>
                </thead>
                <tbody>
                  {paramChanges.map((change) => (
                    <tr key={change.key} className="border-b border-border last:border-0">
                      <td className="px-3 py-2 font-mono text-[12px] text-text">{change.key}</td>
                      <td className="px-3 py-2 font-mono text-[12px] text-text-3 break-all">
                        {change.before}
                      </td>
                      <td className="px-3 py-2 font-mono text-[12px] text-gold break-all">
                        {change.after}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          {blobPending ? (
            <div className="rounded border border-border bg-surface-elev/40 px-4 py-6 text-[13px] text-text-3 text-center">
              Loading strategy blob…
            </div>
          ) : blobError || !childBlob ? (
            <div className="rounded border border-border bg-surface-elev/40 px-4 py-6 text-[13px] text-danger text-center">
              Strategy blob not found for this experiment.
            </div>
          ) : (
            <pre className="max-h-[520px] overflow-auto rounded border border-border bg-surface-elev/40 p-4 text-[12px] leading-5 text-text font-mono">
              {stableStringify(childBlob)}
            </pre>
          )}
        </div>
      </Card>
    </div>
  );
}

/** F29: retire-this-candidate control. Two-click inline confirm (no popups, per
 *  the dashboard UI rule) → POST /lineage/:hash/retire → invalidate the lineage
 *  views so the node flips to Rejected everywhere. */
function RetireAction({ hash, isRejected }: { hash: string; isRejected: boolean }) {
  const queryClient = useQueryClient();
  const [confirming, setConfirming] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const retire = useRetireLineageNode();

  if (isRejected) {
    return (
      <div className="flex items-center gap-2 border-t border-border pt-3 text-[12px] text-text-3">
        This experiment is retired (Rejected).
      </div>
    );
  }

  const onConfirm = () => {
    setError(null);
    retire.mutate(hash, {
      onSuccess: async () => {
        setConfirming(false);
        await Promise.all([
          queryClient.invalidateQueries({ queryKey: autooptimizerKeys.lineageNode(hash) }),
          queryClient.invalidateQueries({ queryKey: autooptimizerKeys.lineage() }),
          queryClient.invalidateQueries({ queryKey: autooptimizerKeys.cycles() }),
        ]);
      },
      onError: (err) => {
        setConfirming(false);
        setError(err instanceof ApiError ? err.message : "Failed to retire experiment.");
      },
    });
  };

  return (
    <div className="flex flex-wrap items-center gap-3 border-t border-border pt-3">
      {confirming ? (
        <>
          <span className="text-[12px] text-text-2">Retire this experiment?</span>
          <button
            type="button"
            onClick={onConfirm}
            disabled={retire.isPending}
            className="px-3 py-1.5 rounded-sm border border-red-500/30 text-red-600 dark:text-red-400 text-[13px] hover:bg-red-500/10 disabled:opacity-50"
          >
            {retire.isPending ? "Retiring…" : "Confirm retire"}
          </button>
          <button
            type="button"
            onClick={() => setConfirming(false)}
            disabled={retire.isPending}
            className="px-3 py-1.5 rounded-sm border border-border text-text-2 text-[13px] hover:bg-surface-card disabled:opacity-50"
          >
            Cancel
          </button>
        </>
      ) : (
        <button
          type="button"
          onClick={() => setConfirming(true)}
          className="px-3 py-1.5 rounded-sm border border-red-500/30 text-red-600 dark:text-red-400 text-[13px] hover:bg-red-500/10"
        >
          Retire experiment
        </button>
      )}
      {error && <span className="text-[12px] text-red-500">{error}</span>}
    </div>
  );
}

type ParamChange = {
  key: string;
  before: string;
  after: string;
};

function buildMechanicalParamChanges(
  parentBlob: unknown,
  childBlob: unknown,
): ParamChange[] {
  const parentParams = asRecord(asRecord(parentBlob)?.mechanical_params);
  const childParams = asRecord(asRecord(childBlob)?.mechanical_params);
  if (!parentParams || !childParams) return [];
  const keys = new Set([...Object.keys(parentParams), ...Object.keys(childParams)]);
  const changes: ParamChange[] = [];
  for (const key of Array.from(keys).sort()) {
    const before = stableStringify(parentParams[key]);
    const after = stableStringify(childParams[key]);
    if (before !== after) {
      changes.push({ key, before, after });
    }
  }
  return changes;
}

function asRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  return value as Record<string, unknown>;
}

function stableStringify(value: unknown): string {
  if (value === undefined) return "undefined";
  return JSON.stringify(sortJson(value), null, 2);
}

function sortJson(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(sortJson);
  if (!value || typeof value !== "object") return value;
  const out: Record<string, unknown> = {};
  for (const key of Object.keys(value as Record<string, unknown>).sort()) {
    out[key] = sortJson((value as Record<string, unknown>)[key]);
  }
  return out;
}

function MetaRow({
  label,
  children,
}: {
  label: string;
  children: ReactNode;
}) {
  return (
    <div>
      <dt className="text-text-3 text-[12px] mb-0.5">{label}</dt>
      <dd className="m-0">{children}</dd>
    </div>
  );
}

function formatDateTime(ts: string): string {
  try {
    return new Date(ts).toLocaleString(undefined, {
      dateStyle: "medium",
      timeStyle: "short",
    });
  } catch {
    return ts;
  }
}
