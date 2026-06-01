// DiffInspector — shows details for a single mutation/experiment node.
// Accepts :hash param from the /autoresearch/diff/:hash route.
// Displays node metadata; diff content itself requires a blob-store
// endpoint that lands in a follow-up PR.

import type { ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import { Card, CardHeader } from "@/components/primitives/Card";
import { useLineageNode, formatLineageStatus, formatGateVerdict } from "./api";

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
          to="/autoresearch"
          className="underline text-text-3 hover:text-text"
        >
          Back to genealogy
        </Link>
      </div>
    );
  }

  const statusLabel = formatLineageStatus(node.status);
  const verdictLabel = formatGateVerdict(node.gate_verdict);
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
          to="/autoresearch"
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
                  to={`/autoresearch/diff/${encodeURIComponent(node.parent_hash)}`}
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

            {node.diff_hash && (
              <MetaRow label="Diff hash">
                <span className="font-mono text-[12px] text-text break-all">
                  {node.diff_hash}
                </span>
              </MetaRow>
            )}
          </div>
        </div>
      </Card>

      {/* Diff content placeholder — requires blob-store endpoint (follow-up PR) */}
      <Card>
        <CardHeader title="Experiment diff" />
        <div className="px-5 pb-5">
          <div className="rounded border border-border bg-surface-elev/40 px-4 py-6 text-[13px] text-text-3 text-center">
            Diff content loads from the blob store. Available in the follow-up
            PR that wires{" "}
            <code className="font-mono text-[12px]">
              GET /api/autoresearch/blobs/:hash
            </code>
            .
          </div>
        </div>
      </Card>
    </div>
  );
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
