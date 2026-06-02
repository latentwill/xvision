// DiffInspector — shows details for a single mutation/experiment node.
// Accepts :hash param from the /autooptimizer/diff/:hash route.
// Displays node metadata and changed fields from the blob store.

import type { ReactNode } from "react";
import { Link, useParams } from "react-router-dom";
import { Card, CardHeader } from "@/components/primitives/Card";
import {
  useBlob,
  useLineageNode,
  formatLineageStatus,
  formatGateVerdict,
  type BundleBlob,
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
  const childBlob = useBlob(node?.bundle_hash);
  const parentBlob = useBlob(node?.parent_hash);

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
        </div>
      </Card>

      <Card>
        <CardHeader title="Experiment diff" />
        <BlobDiff
          parentHash={node.parent_hash}
          parent={parentBlob.data}
          child={childBlob.data}
          isLoading={childBlob.isPending || (node.parent_hash ? parentBlob.isPending : false)}
          isError={childBlob.isError || parentBlob.isError}
        />
      </Card>
    </div>
  );
}

function BlobDiff({
  parentHash,
  parent,
  child,
  isLoading,
  isError,
}: {
  parentHash?: string | null;
  parent?: BundleBlob;
  child?: BundleBlob;
  isLoading: boolean;
  isError: boolean;
}) {
  if (!parentHash) {
    return (
      <div className="px-5 pb-5 text-[13px] text-text-3">
        Root experiment. No parent diff is available.
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className="px-5 pb-5 text-[13px] text-text-3">
        Loading blob diff…
      </div>
    );
  }

  if (isError || !parent || !child) {
    return (
      <div className="px-5 pb-5 text-[13px] text-red-500">
        Failed to load blob diff.
      </div>
    );
  }

  const changes = diffBlobs(parent, child);

  if (changes.length === 0) {
    return (
      <div className="px-5 pb-5 text-[13px] text-text-3">
        No field changes detected.
      </div>
    );
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-[13px] border-collapse">
        <thead>
          <tr className="border-b border-border">
            <th className="text-left font-medium text-text-3 px-5 py-3">
              Field
            </th>
            <th className="text-left font-medium text-text-3 px-4 py-3">
              Parent
            </th>
            <th className="text-left font-medium text-text-3 px-5 py-3">
              Experiment
            </th>
          </tr>
        </thead>
        <tbody>
          {changes.map((change) => (
            <tr
              key={change.path}
              className="border-b border-border last:border-0 hover:bg-surface-elev/40"
            >
              <td className="px-5 py-3 font-mono text-[12px] text-text">
                {change.path}
              </td>
              <td className="px-4 py-3 font-mono text-[12px] text-text-3">
                {formatDiffValue(change.before)}
              </td>
              <td className="px-5 py-3 font-mono text-[12px] text-text">
                {formatDiffValue(change.after)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
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

type FlatBlob = Record<string, unknown>;

function diffBlobs(parent: BundleBlob, child: BundleBlob) {
  const parentFlat = flattenBlob(parent);
  const childFlat = flattenBlob(child);
  const paths = new Set([...Object.keys(parentFlat), ...Object.keys(childFlat)]);
  return [...paths]
    .sort()
    .filter((path) => !valuesEqual(parentFlat[path], childFlat[path]))
    .map((path) => ({
      path,
      before: parentFlat[path],
      after: childFlat[path],
    }));
}

function flattenBlob(value: unknown, prefix = ""): FlatBlob {
  if (!isPlainRecord(value)) {
    return prefix ? { [prefix]: value } : {};
  }

  const out: FlatBlob = {};
  for (const [key, nested] of Object.entries(value)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (isPlainRecord(nested)) {
      Object.assign(out, flattenBlob(nested, path));
    } else {
      out[path] = nested;
    }
  }
  return out;
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function valuesEqual(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

function formatDiffValue(value: unknown): string {
  if (value === undefined) return "—";
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}
