// frontend/web/src/features/agent-runs/SpanTree.tsx
//
// Collapsible nested span tree (WS-16). Renders the trace as a hierarchy
// keyed off `parent_span_id`: a parent span (e.g. a DECISION) can be
// collapsed to a one-line rollup and expanded to its full subtree. Built
// on the same `depthOf` walk as AgentRunIndentedTimeline; collapse state
// lives in the shared trace-dock store (persisted to localStorage) so the
// operator's expand/collapse choices survive a reload.
import { useMemo } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { formatCostUsd } from "@/lib/format";
import { useTraceDock } from "@/stores/trace-dock";
import { spanColor, withAlpha } from "./span-colors";

function ts(iso: string): number {
  return new Date(iso).getTime();
}

function formatMs(ms: number | null): string {
  if (ms == null) return "…";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(ms < 10_000 ? 2 : 1)}s`;
  const total = Math.round(ms / 1000);
  const m = Math.floor(total / 60);
  const s = total % 60;
  return `${m}m${s.toString().padStart(2, "0")}s`;
}

/**
 * Pull a cost figure off a span. Real export-backed spans land on
 * `span.cost`; mock-fixture / older event shapes carry `attributes.cost_usd`.
 */
function costOf(span: RunSpan): number | undefined {
  return span.cost ?? (span.attributes as { cost_usd?: number }).cost_usd;
}

type TreeRow = {
  span: RunSpan;
  depth: number;
  durationMs: number | null;
  /** Number of descendants (transitive) under this node within the set. */
  descendantCount: number;
  hasChildren: boolean;
  collapsed: boolean;
};

/**
 * Build the ordered, depth-annotated row list, then prune any row whose
 * ancestor chain contains a collapsed node. Roots (parentless within the
 * set) and orphans (parent missing) stay top-level. Within a parent,
 * children sort by start time so the tree reads chronologically.
 */
function buildVisibleRows(
  spans: RunSpan[],
  collapsed: Set<string>,
): TreeRow[] {
  if (spans.length === 0) return [];
  const byId = new Map(spans.map((s) => [s.span_id, s]));

  // children-of index, keyed by the EFFECTIVE parent (present-in-set parent
  // or "" for roots/orphans).
  const childrenOf = new Map<string, RunSpan[]>();
  for (const s of spans) {
    const p =
      s.parent_span_id && byId.has(s.parent_span_id) ? s.parent_span_id : "";
    const arr = childrenOf.get(p);
    if (arr) arr.push(s);
    else childrenOf.set(p, [s]);
  }
  for (const arr of childrenOf.values()) {
    arr.sort((a, b) => ts(a.started_at) - ts(b.started_at));
  }

  // Transitive descendant counts (used by the rollup's key metric).
  const descendantCount = new Map<string, number>();
  function countDescendants(id: string): number {
    const cached = descendantCount.get(id);
    if (cached != null) return cached;
    const kids = childrenOf.get(id) ?? [];
    let total = kids.length;
    for (const k of kids) total += countDescendants(k.span_id);
    descendantCount.set(id, total);
    return total;
  }
  for (const s of spans) countDescendants(s.span_id);

  // Pre-order DFS from the synthetic roots ("" key), skipping any subtree
  // whose parent is collapsed. We still EMIT the collapsed node itself.
  const rows: TreeRow[] = [];
  function walk(span: RunSpan, depth: number): void {
    const kids = childrenOf.get(span.span_id) ?? [];
    const hasChildren = kids.length > 0;
    const isCollapsed = hasChildren && collapsed.has(span.span_id);
    rows.push({
      span,
      depth,
      durationMs: span.finished_at
        ? ts(span.finished_at) - ts(span.started_at)
        : null,
      descendantCount: descendantCount.get(span.span_id) ?? 0,
      hasChildren,
      collapsed: isCollapsed,
    });
    if (isCollapsed) return; // hide the entire subtree
    for (const k of kids) walk(k, depth + 1);
  }
  for (const root of childrenOf.get("") ?? []) walk(root, 0);
  return rows;
}

export function SpanTree({
  spans,
  selectedSpanId,
  onSelect,
}: {
  spans: RunSpan[];
  selectedSpanId: string | null;
  onSelect: (spanId: string) => void;
}) {
  const collapsedSpanIds = useTraceDock((s) => s.collapsedSpanIds);
  const toggleSpanCollapsed = useTraceDock((s) => s.toggleSpanCollapsed);
  const collapseAllSpans = useTraceDock((s) => s.collapseAllSpans);
  const expandAllSpans = useTraceDock((s) => s.expandAllSpans);

  const rows = useMemo(
    () => buildVisibleRows(spans, collapsedSpanIds),
    [spans, collapsedSpanIds],
  );

  // The set of nodes that *can* be collapsed — only those with children.
  const collapsibleIds = useMemo(() => {
    const byId = new Map(spans.map((s) => [s.span_id, s]));
    const parents = new Set<string>();
    for (const s of spans) {
      if (s.parent_span_id && byId.has(s.parent_span_id)) {
        parents.add(s.parent_span_id);
      }
    }
    return [...parents];
  }, [spans]);

  if (spans.length === 0) {
    return (
      <div
        className="font-mono text-[12px] text-text-3 p-3"
        aria-label="no spans"
      >
        no spans match the current filter
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center gap-2 px-3 h-7 border-b border-border text-[10px] font-mono text-text-3 shrink-0">
        <span className="tracking-wider">TREE</span>
        <span className="opacity-40">·</span>
        <button
          type="button"
          onClick={() => expandAllSpans()}
          className="px-1.5 h-5 rounded hover:bg-surface-elev hover:text-text"
          title="Expand every node"
        >
          expand all
        </button>
        <button
          type="button"
          onClick={() => collapseAllSpans(collapsibleIds)}
          className="px-1.5 h-5 rounded hover:bg-surface-elev hover:text-text"
          title="Collapse every node that has children"
        >
          collapse all
        </button>
      </div>
      <div
        className="font-mono text-[12px] overflow-y-auto flex-1 min-h-0"
        role="tree"
        aria-label="span tree"
      >
        {rows.map((row) => (
          <SpanTreeRow
            key={row.span.span_id}
            row={row}
            selected={row.span.span_id === selectedSpanId}
            onSelect={onSelect}
            onToggle={toggleSpanCollapsed}
          />
        ))}
      </div>
    </div>
  );
}

function SpanTreeRow({
  row,
  selected,
  onSelect,
  onToggle,
}: {
  row: TreeRow;
  selected: boolean;
  onSelect: (spanId: string) => void;
  onToggle: (spanId: string) => void;
}) {
  const { span, depth, durationMs, descendantCount, hasChildren, collapsed } =
    row;
  const color = spanColor(span.kind);
  const isError = span.status === "error";
  const isLive = span.status === "in_progress";
  const cost = costOf(span);

  return (
    <div
      data-testid={`span-tree-row-${span.span_id}`}
      data-depth={depth}
      data-selected={selected}
      role="treeitem"
      aria-expanded={hasChildren ? !collapsed : undefined}
      className={`group w-full flex items-start gap-1 px-2 py-1 border-l-2 hover:bg-surface-elev/60 ${
        selected ? "bg-surface-elev" : ""
      }`}
      style={{
        borderLeftColor: selected ? color.hex : "transparent",
        paddingLeft: `${0.5 + depth * 1.1}rem`,
      }}
    >
      {/* Disclosure caret — only for nodes with children. Leaves get a
          spacer so labels still line up. */}
      {hasChildren ? (
        <button
          type="button"
          data-testid={`span-tree-caret-${span.span_id}`}
          aria-label={collapsed ? "expand node" : "collapse node"}
          onClick={() => onToggle(span.span_id)}
          className="shrink-0 w-4 h-4 inline-flex items-center justify-center text-text-3 hover:text-text mt-px"
        >
          <svg
            width="9"
            height="9"
            viewBox="0 0 10 10"
            fill="none"
            aria-hidden
            style={{
              transform: collapsed ? "rotate(-90deg)" : "rotate(0deg)",
              transition: "transform 80ms ease",
            }}
          >
            <path
              d="M2 3l3 3 3-3"
              stroke="currentColor"
              strokeWidth="1.4"
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
        </button>
      ) : (
        <span className="shrink-0 w-4 h-4" aria-hidden />
      )}

      {/* Label — clicking selects the span (distinct from the caret toggle). */}
      <button
        type="button"
        data-testid={`span-tree-label-${span.span_id}`}
        onClick={() => onSelect(span.span_id)}
        className="flex items-start gap-2 min-w-0 leading-snug text-left flex-1"
      >
        <span
          className="inline-block w-2 h-2 rounded-sm shrink-0 mt-1"
          style={{ background: color.hex }}
          aria-hidden
        />
        <span
          className="text-[9px] font-semibold tracking-wider px-1.5 py-px rounded-sm shrink-0 mt-px"
          style={{
            color: color.hex,
            background: withAlpha(color.hex, 0.12),
            border: `1px solid ${withAlpha(color.hex, 0.35)}`,
          }}
        >
          {color.label}
        </span>
        <span className="text-text break-all" title={`${span.kind} · ${span.name}`}>
          {span.name}
        </span>
        {isError ? (
          <span className="text-red-400 text-[10px] shrink-0" aria-label="error">
            ●
          </span>
        ) : null}
        {isLive ? (
          <span
            className="text-blue-300 text-[10px] shrink-0 animate-pulse"
            aria-label="in progress"
          >
            ●
          </span>
        ) : null}
      </button>

      {/* Right edge: either the collapsed rollup or the plain duration. */}
      {collapsed ? (
        <span
          data-testid={`span-tree-rollup-${span.span_id}`}
          className="shrink-0 ml-auto flex items-center gap-1.5 text-[10px] text-text-3 tabular-nums"
          title={`${color.label} · ${formatMs(durationMs)} · ${span.status} · ${descendantCount} hidden`}
        >
          <span style={{ color: color.hex }}>{color.label}</span>
          <span className="opacity-40">·</span>
          <span>{formatMs(durationMs)}</span>
          <span className="opacity-40">·</span>
          <span className={isError ? "text-red-400" : ""}>{span.status}</span>
          {cost != null ? (
            <>
              <span className="opacity-40">·</span>
              <span>{formatCostUsd(cost)}</span>
            </>
          ) : null}
          <span className="opacity-40">·</span>
          <span aria-label={`${descendantCount} hidden`}>▾{descendantCount}</span>
        </span>
      ) : (
        <span className="shrink-0 ml-auto text-text-3 text-right tabular-nums text-[10px] mt-px">
          {formatMs(durationMs)}
        </span>
      )}
    </div>
  );
}
