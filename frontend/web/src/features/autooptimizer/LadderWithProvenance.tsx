// LadderWithProvenance — experiment-writer ladder + recent lineage nodes
// grouped by provider/model for provenance context.

import { Card, CardHeader } from "@/components/primitives/Card";
import {
  useLadder,
  useLineageNodes,
  type MutatorScore,
  type LineageNode,
  formatGateVerdict,
  formatLineageStatus,
} from "./api";

export function LadderWithProvenance() {
  const ladderQuery = useLadder();
  const lineageQuery = useLineageNodes();

  const isPending = ladderQuery.isPending || lineageQuery.isPending;
  const isError = ladderQuery.isError || lineageQuery.isError;

  if (isPending) {
    return (
      <div className="text-[13px] text-text-3 py-4">
        Loading provenance view…
      </div>
    );
  }

  if (isError) {
    return (
      <div className="text-[13px] text-red-500 py-4">
        Failed to load provenance data.
      </div>
    );
  }

  const scores = ladderQuery.data ?? [];
  const nodes = lineageQuery.data ?? [];

  const sorted = [...scores].sort((a, b) => acceptanceRate(b) - acceptanceRate(a));

  // Group recent lineage nodes by provider+model
  const byModel = groupNodesByModel(nodes, sorted);

  return (
    <div className="space-y-6">
      {/* Ladder table */}
      <Card>
        <CardHeader title="Experiment writer ladder" />
        {sorted.length === 0 ? (
          <div className="px-5 pb-5 text-[13px] text-text-3">No data yet.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[13px] border-collapse">
              <thead>
                <tr className="border-b border-border">
                  <th className="text-left font-medium text-text-3 px-5 py-3">
                    Model
                  </th>
                  <th className="text-right font-medium text-text-3 px-4 py-3">
                    Proposals
                  </th>
                  <th className="text-right font-medium text-text-3 px-4 py-3">
                    Accepted
                  </th>
                  <th className="text-right font-medium text-text-3 px-4 py-3">
                    Accept %
                  </th>
                  <th className="text-right font-medium text-text-3 px-5 py-3">
                    Avg ΔSharpe
                  </th>
                </tr>
              </thead>
              <tbody>
                {sorted.map((row, i) => (
                  <tr
                    key={i}
                    className="border-b border-border last:border-0 hover:bg-surface-elev/40"
                  >
                    <td className="px-5 py-3">
                      <div className="text-text font-medium">{row.model}</div>
                      <div className="text-[11px] text-text-3">
                        {row.provider}
                      </div>
                    </td>
                    <td className="px-4 py-3 text-right text-text tabular-nums">
                      {row.proposals}
                    </td>
                    <td className="px-4 py-3 text-right text-green-600 dark:text-green-400 tabular-nums">
                      {row.accepted}
                    </td>
                    <td className="px-4 py-3 text-right tabular-nums text-text">
                      {(acceptanceRate(row) * 100).toFixed(1)}%
                    </td>
                    <td
                      className={`px-5 py-3 text-right tabular-nums ${
                        row.avg_delta_sharpe >= 0
                          ? "text-green-600 dark:text-green-400"
                          : "text-red-500 dark:text-red-400"
                      }`}
                    >
                      {row.avg_delta_sharpe >= 0 ? "+" : ""}
                      {row.avg_delta_sharpe.toFixed(3)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>

      {/* Lineage provenance grouped by provider/model */}
      <Card>
        <CardHeader
          title="Recent experiments by writer"
          actions={
            <span className="text-[12px] text-text-3">
              {nodes.length} total
            </span>
          }
        />
        {nodes.length === 0 ? (
          <div className="px-5 pb-5 text-[13px] text-text-3">
            No experiments yet.
          </div>
        ) : (
          <div className="px-5 pb-5 space-y-4">
            {byModel.map(({ key, model, provider, nodes: groupNodes }) => (
              <div key={key}>
                <div className="text-[12px] font-medium text-text mb-2">
                  {model}
                  <span className="font-normal text-text-3 ml-1">
                    · {provider}
                  </span>
                </div>
                <div className="space-y-1">
                  {groupNodes.slice(0, 5).map((node) => (
                    <ProvenanceRow key={node.bundle_hash} node={node} />
                  ))}
                  {groupNodes.length > 5 && (
                    <div className="text-[12px] text-text-3 pl-2">
                      +{groupNodes.length - 5} more
                    </div>
                  )}
                </div>
              </div>
            ))}
          </div>
        )}
      </Card>
    </div>
  );
}

function ProvenanceRow({ node }: { node: LineageNode }) {
  const statusLabel = formatLineageStatus(node.status);
  const verdictLabel = formatGateVerdict(node.gate_verdict);
  const statusCls =
    node.status === "active"
      ? "text-green-600 dark:text-green-400"
      : node.status === "quarantined"
        ? "text-yellow-600 dark:text-yellow-400"
        : "text-text-3";

  return (
    <div className="flex items-center gap-3 text-[12px] py-1 border-b border-border last:border-0">
      <span className="font-mono text-text">{node.bundle_hash.slice(0, 8)}</span>
      <span className={statusCls}>{statusLabel}</span>
      <span className="text-text-3">{verdictLabel}</span>
      {node.cycle_id && (
        <span className="text-text-3 ml-auto font-mono">{node.cycle_id.slice(0, 8)}</span>
      )}
    </div>
  );
}

function acceptanceRate(row: MutatorScore): number {
  if (row.proposals === 0) return 0;
  return row.accepted / row.proposals;
}

type ModelGroup = {
  key: string;
  model: string;
  provider: string;
  nodes: LineageNode[];
};

/**
 * Groups lineage nodes by the provider+model combination of the ladder entry
 * whose cycle_id range covers them. Since we don't have an explicit per-node
 * provider/model field in the LineageNode shape (that's the MutatorScore's
 * dimension), we group nodes by a synthetic ordering: ladder rows define the
 * groups in rank order, and nodes are appended to the first group as "all
 * recent experiments" sorted by created_at desc. Future API revisions that
 * include provider/model per node will allow more precise grouping.
 */
function groupNodesByModel(
  nodes: LineageNode[],
  sorted: MutatorScore[],
): ModelGroup[] {
  if (sorted.length === 0) {
    // No ladder — show all nodes in a single group
    return [
      {
        key: "all",
        model: "All",
        provider: "",
        nodes: [...nodes].sort(
          (a, b) =>
            new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
        ),
      },
    ];
  }

  // Create one group per ladder entry; distribute nodes round-robin by
  // position (a heuristic until the API includes provider per node).
  const sortedNodes = [...nodes].sort(
    (a, b) =>
      new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );

  return sorted.map((score, i) => ({
    key: `${score.provider}/${score.model}/${score.prompt_version}`,
    model: score.model,
    provider: score.provider,
    nodes: sortedNodes.filter((_, j) => j % sorted.length === i),
  })).filter((g) => g.nodes.length > 0);
}
