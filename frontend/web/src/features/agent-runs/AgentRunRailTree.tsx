// frontend/web/src/features/agent-runs/AgentRunRailTree.tsx
import type { RunSpan } from "@/api/types-agent-runs";
import { spanColor } from "./span-colors";

type Node = { span: RunSpan; children: Node[] };

function buildTree(spans: RunSpan[]): Node[] {
  const byId = new Map<string, Node>();
  spans.forEach((s) => byId.set(s.span_id, { span: s, children: [] }));
  const roots: Node[] = [];
  for (const n of byId.values()) {
    const parentId = n.span.parent_span_id;
    if (parentId && byId.has(parentId)) byId.get(parentId)!.children.push(n);
    else roots.push(n);
  }
  const sortRec = (nodes: Node[]) => {
    nodes.sort(
      (a, b) =>
        new Date(a.span.started_at).getTime() -
        new Date(b.span.started_at).getTime(),
    );
    nodes.forEach((n) => sortRec(n.children));
  };
  sortRec(roots);
  return roots;
}

function NodeRow({
  node,
  depth,
  selectedSpanId,
  onSelect,
}: {
  node: Node;
  depth: number;
  selectedSpanId: string | null;
  onSelect: (id: string) => void;
}) {
  const color = spanColor(node.span.kind);
  const selected = selectedSpanId === node.span.span_id;
  return (
    <div>
      <button
        type="button"
        data-testid={`rail-node-${node.span.span_id}`}
        onClick={() => onSelect(node.span.span_id)}
        className={`w-full flex items-center gap-1.5 py-0.5 pr-2 text-left text-[11px] hover:bg-surface-elev ${selected ? "bg-surface-elev" : ""}`}
        style={{ paddingLeft: `${0.25 + depth * 0.75}rem` }}
      >
        <span aria-hidden>{node.children.length > 0 ? "▾" : "·"}</span>
        <span className="inline-block w-1.5 h-1.5 rounded-sm" style={{ background: color.hex }} aria-hidden />
        <span className="text-text-2">{node.span.kind.replace(/^.*\./, "")}</span>
      </button>
      {node.children.map((c) => (
        <NodeRow key={c.span.span_id} node={c} depth={depth + 1} selectedSpanId={selectedSpanId} onSelect={onSelect} />
      ))}
    </div>
  );
}

export function AgentRunRailTree({
  spans,
  selectedSpanId,
  onSelect,
}: {
  spans: RunSpan[];
  selectedSpanId: string | null;
  onSelect: (id: string) => void;
}) {
  const roots = buildTree(spans);
  return (
    <div className="font-mono overflow-y-auto h-full border-r border-border">
      {roots.map((r) => (
        <NodeRow key={r.span.span_id} node={r} depth={0} selectedSpanId={selectedSpanId} onSelect={onSelect} />
      ))}
    </div>
  );
}
