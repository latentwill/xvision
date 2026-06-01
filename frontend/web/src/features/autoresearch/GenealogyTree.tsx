// GenealogyTree — renders the experiment genealogy tree.
// Fetches all lineage nodes, groups by cycle_id, and shows a simple
// flat list (no D3 dependency — plain HTML rows with parent arrows).
// Clicking a node navigates to /autoresearch/diff/:hash.

import { useNavigate } from "react-router-dom";
import { Card, CardHeader } from "@/components/primitives/Card";
import {
  useLineageNodes,
  formatLineageStatus,
  formatGateVerdict,
  type LineageNode,
} from "./api";

export function GenealogyTree() {
  const { data: nodes, isPending, isError } = useLineageNodes();

  if (isPending) {
    return (
      <div className="text-[13px] text-text-3 py-4">
        Loading genealogy…
      </div>
    );
  }

  if (isError) {
    return (
      <div className="text-[13px] text-red-500 py-4">
        Failed to load genealogy data.
      </div>
    );
  }

  if (!nodes || nodes.length === 0) {
    return (
      <div className="text-[13px] text-text-3 py-4">
        No experiments yet.
      </div>
    );
  }

  // Group nodes by cycle_id (undefined/null → "ungrouped")
  const grouped = groupByCycle(nodes);

  return (
    <div className="space-y-4">
      {grouped.map(({ cycleId, items }) => (
        <Card key={cycleId ?? "__none__"}>
          <CardHeader
            title={
              cycleId ? (
                <span>
                  <span className="text-text-3 font-normal text-[13px] mr-2">
                    Cycle
                  </span>
                  <span className="font-mono text-[13px]">{cycleId}</span>
                </span>
              ) : (
                "No cycle assigned"
              )
            }
          />
          <div className="px-5 pb-4 space-y-1">
            {items.map((node) => (
              <NodeRow key={node.bundle_hash} node={node} />
            ))}
          </div>
        </Card>
      ))}
    </div>
  );
}

function NodeRow({ node }: { node: LineageNode }) {
  const navigate = useNavigate();
  const shortHash = node.bundle_hash.slice(0, 8);

  return (
    <button
      type="button"
      onClick={() =>
        navigate(`/autoresearch/diff/${encodeURIComponent(node.bundle_hash)}`)
      }
      className="w-full flex items-center gap-3 px-3 py-2.5 rounded border border-border hover:border-text-3 hover:bg-surface-elev/40 transition-colors text-left"
    >
      {/* Parent arrow */}
      {node.parent_hash ? (
        <span className="text-text-3 text-[11px] font-mono shrink-0">
          ↳ {node.parent_hash.slice(0, 8)}
        </span>
      ) : (
        <span className="text-text-3 text-[11px] font-mono shrink-0">root</span>
      )}

      {/* Hash */}
      <span className="font-mono text-[13px] text-text shrink-0">{shortHash}</span>

      {/* Status badge */}
      <StatusBadge status={node.status} />

      {/* Gate verdict */}
      <span className="text-[12px] text-text-3 shrink-0">
        {formatGateVerdict(node.gate_verdict)}
      </span>

      {/* Diversity score */}
      {node.diversity_score != null && (
        <span className="ml-auto text-[12px] text-text-3 shrink-0">
          div {node.diversity_score.toFixed(3)}
        </span>
      )}

      {/* Created at */}
      <span className="ml-auto text-[12px] text-text-3 shrink-0">
        {formatShortDate(node.created_at)}
      </span>
    </button>
  );
}

function StatusBadge({ status }: { status: string }) {
  const label = formatLineageStatus(status as "active" | "rejected" | "quarantined");
  const cls =
    status === "active"
      ? "bg-green-500/10 text-green-600 dark:text-green-400 border-green-500/30"
      : status === "quarantined"
        ? "bg-yellow-500/10 text-yellow-600 dark:text-yellow-400 border-yellow-500/30"
        : "bg-red-500/10 text-red-600 dark:text-red-400 border-red-500/30";
  return (
    <span
      className={`inline-flex items-center px-1.5 py-0.5 rounded-sm border text-[11px] font-medium shrink-0 ${cls}`}
    >
      {label}
    </span>
  );
}

function formatShortDate(ts: string): string {
  try {
    return new Date(ts).toLocaleDateString(undefined, {
      month: "short",
      day: "numeric",
    });
  } catch {
    return ts;
  }
}

type CycleGroup = {
  cycleId: string | null;
  items: LineageNode[];
};

function groupByCycle(nodes: LineageNode[]): CycleGroup[] {
  const map = new Map<string, LineageNode[]>();
  for (const node of nodes) {
    const key = node.cycle_id ?? "__none__";
    const arr = map.get(key) ?? [];
    arr.push(node);
    map.set(key, arr);
  }
  const result: CycleGroup[] = [];
  for (const [key, items] of map.entries()) {
    result.push({
      cycleId: key === "__none__" ? null : key,
      // Most recent first within each cycle
      items: [...items].sort(
        (a, b) =>
          new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
      ),
    });
  }
  // Sort cycles: most recent cycle first (by the max created_at in the group)
  result.sort((a, b) => {
    const aMax = Math.max(...a.items.map((n) => new Date(n.created_at).getTime()));
    const bMax = Math.max(...b.items.map((n) => new Date(n.created_at).getTime()));
    return bMax - aMax;
  });
  return result;
}
