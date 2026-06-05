// writer-provenance.ts
// Ported from LadderWithProvenance.tsx groupNodesByModel — kept identical so
// both surfaces produce the same grouping. See the jsdoc there for the honest
// caveat: LineageNode does not carry provider/model; this is positional/heuristic.

import type { LineageNode, MutatorScore } from "../api";

export type WriterGroup = {
  key: string;
  model: string;
  provider: string;
  prompt_version: string;
  nodes: LineageNode[];
};

/**
 * Groups lineage nodes by the provider+model combination of the ladder entry
 * whose rank position covers them. Since LineageNode has no explicit
 * provider/model field, nodes are distributed round-robin across sorted ladder
 * entries (a heuristic). Future API revisions carrying provider/model per node
 * will allow more precise grouping.
 */
export function groupNodesByWriter(
  nodes: LineageNode[],
  sorted: MutatorScore[],
): WriterGroup[] {
  if (sorted.length === 0) {
    return [
      {
        key: "all",
        model: "All",
        provider: "",
        prompt_version: "",
        nodes: [...nodes].sort(
          (a, b) =>
            new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
        ),
      },
    ];
  }

  // Sort nodes most-recent-first before distributing.
  const sortedNodes = [...nodes].sort(
    (a, b) =>
      new Date(b.created_at).getTime() - new Date(a.created_at).getTime(),
  );

  return sorted
    .map((score, i) => ({
      key: `${score.provider}/${score.model}/${score.prompt_version}`,
      model: score.model,
      provider: score.provider,
      prompt_version: score.prompt_version,
      nodes: sortedNodes.filter((_, j) => j % sorted.length === i),
    }))
    .filter((g) => g.nodes.length > 0);
}
