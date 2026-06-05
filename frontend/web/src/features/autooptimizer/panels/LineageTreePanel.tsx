import { Link } from "react-router-dom";
import { useLineageNodes, formatGateVerdict, type LineageNode } from "../api";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";

// ─── Tree building ─────────────────────────────────────────────────────────────

type TreeNode = {
  node: LineageNode;
  children: TreeNode[];
};

function buildTree(nodes: LineageNode[]): TreeNode[] {
  const byHash = new Map<string, TreeNode>();
  for (const n of nodes) {
    byHash.set(n.bundle_hash, { node: n, children: [] });
  }

  const roots: TreeNode[] = [];
  for (const n of nodes) {
    const treeNode = byHash.get(n.bundle_hash)!;
    const parentHash = n.parent_hash ?? null;
    if (parentHash && byHash.has(parentHash)) {
      byHash.get(parentHash)!.children.push(treeNode);
    } else {
      roots.push(treeNode);
    }
  }
  return roots;
}

// ─── Recursive row renderer ────────────────────────────────────────────────────

function TreeRow({ treeNode, depth }: { treeNode: TreeNode; depth: number }) {
  const { node } = treeNode;
  return (
    <>
      <li
        key={node.bundle_hash}
        className="flex items-center gap-2 py-1.5"
        style={{ paddingLeft: `${depth * 20}px` }}
      >
        {depth > 0 && (
          <span className="mr-1 text-text-3 text-[11px] select-none">└</span>
        )}
        <HashSigil hash={node.bundle_hash} size={24} />
        <Link
          to={`/optimizer/experiment/${encodeURIComponent(node.bundle_hash)}`}
          className="font-mono text-[12px] text-text hover:text-gold"
        >
          {node.bundle_hash.slice(0, 10)}
        </Link>
        <span className="ml-auto">
          <GateBadge verdict={formatGateVerdict(node.gate_verdict)} status={node.status} />
        </span>
      </li>
      {treeNode.children.map((child) => (
        <TreeRow key={child.node.bundle_hash} treeNode={child} depth={depth + 1} />
      ))}
    </>
  );
}

// ─── Panel ────────────────────────────────────────────────────────────────────

export function LineageTreePanel({ cycleId }: { cycleId: string }) {
  const { data, isLoading, isError } = useLineageNodes({ cycleId });
  const nodes: LineageNode[] = data ?? [];

  return (
    <section className="rounded-md border border-border bg-surface-card p-5">
      <div className="mb-2">
        <h2 className="m-0 text-[15px] font-semibold tracking-tight">Lineage tree</h2>
        <p className="mt-0.5 text-[12px] text-text-3">parent → child genealogy for this cycle</p>
      </div>

      {isLoading ? (
        <p className="text-[12px] text-text-3">Loading…</p>
      ) : isError ? (
        <p className="text-[12px] text-danger">Couldn't load lineage.</p>
      ) : nodes.length === 0 ? (
        <p className="text-[12px] text-text-3">No lineage recorded for this cycle.</p>
      ) : (
        <ul className="mt-1 divide-y divide-border-soft">
          {buildTree(nodes).map((root) => (
            <TreeRow key={root.node.bundle_hash} treeNode={root} depth={0} />
          ))}
        </ul>
      )}
    </section>
  );
}
