// frontend/web/src/features/agent-runs/FlameGraph.tsx
import { useMemo } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { spanColorForSpan, withAlpha } from "./span-colors";

type LayoutRow = {
  span: RunSpan;
  depth: number;
  leftPct: number;
  widthPct: number;
};

// `topAndDepth` walks parent pointers within the filtered span set to find
// the span's top-level ancestor (the one whose parent is missing from the
// set) and the depth relative to that ancestor. Returns `null` only if the
// walk cycles, which would be a malformed fixture.
function topAndDepth(
  span: RunSpan,
  byId: Map<string, RunSpan>,
): { topId: string; depth: number } {
  let depth = 0;
  let cur: RunSpan = span;
  while (cur.parent_span_id && byId.has(cur.parent_span_id)) {
    const parent = byId.get(cur.parent_span_id)!;
    depth += 1;
    cur = parent;
    if (depth > 32) break;
  }
  return { topId: cur.span_id, depth };
}

function layout(spans: RunSpan[]): LayoutRow[] {
  if (spans.length === 0) return [];
  const byId = new Map(spans.map((s) => [s.span_id, s]));
  const ts = (iso: string) => new Date(iso).getTime();
  const starts = spans.map((s) => ts(s.started_at));
  const ends = spans.map((s) => (s.finished_at ? ts(s.finished_at) : Date.now()));
  const t0 = Math.min(...starts);
  const tN = Math.max(...ends);
  const totalSpan = Math.max(1, tN - t0);

  // Top-level spans are parentless within the filtered set (either truly
  // root or filter removed their parent). Each gets its own vertical lane
  // so sibling top-levels don't overdraw row 0.
  const topLevels = spans
    .filter((s) => !s.parent_span_id || !byId.has(s.parent_span_id))
    .sort((a, b) => ts(a.started_at) - ts(b.started_at));

  // Map every span to its top-level ancestor + intra-lane depth.
  const ancestry = new Map<string, { topId: string; depth: number }>();
  for (const s of spans) ancestry.set(s.span_id, topAndDepth(s, byId));

  // Lane offset = sum of (max_intra_depth + 1) for preceding lanes, so a
  // 3-deep tree followed by a 1-deep tree lays out as rows 0..3 then 4..5.
  const maxDepthByTop = new Map<string, number>();
  for (const { topId, depth } of ancestry.values()) {
    maxDepthByTop.set(topId, Math.max(maxDepthByTop.get(topId) ?? 0, depth));
  }
  const laneOffset = new Map<string, number>();
  let runningOffset = 0;
  for (const top of topLevels) {
    laneOffset.set(top.span_id, runningOffset);
    runningOffset += (maxDepthByTop.get(top.span_id) ?? 0) + 1;
  }

  // A single-span run paints as a fixed chip instead of the full dock
  // width — otherwise the lone bar is visually indistinguishable from
  // the background.
  const isSingleSpan = spans.length === 1;

  return spans
    .map((s) => {
      const start = ts(s.started_at);
      const end = s.finished_at ? ts(s.finished_at) : Date.now();
      const a = ancestry.get(s.span_id)!;
      const finalDepth = (laneOffset.get(a.topId) ?? 0) + a.depth;
      const leftPct = isSingleSpan ? 0 : ((start - t0) / totalSpan) * 100;
      const widthPct = isSingleSpan
        ? 40
        : Math.max(0.5, ((end - start) / totalSpan) * 100);
      return { span: s, depth: finalDepth, leftPct, widthPct };
    })
    .sort((a, b) => a.depth - b.depth || a.leftPct - b.leftPct);
}

export function FlameGraph({
  spans,
  selectedSpanId,
  onSelect,
}: {
  spans: RunSpan[];
  selectedSpanId: string | null;
  onSelect: (spanId: string) => void;
}) {
  const rows = useMemo(() => layout(spans), [spans]);
  const ROW_H = 18;
  const maxDepth = rows.reduce((m, r) => Math.max(m, r.depth), 0);
  const totalH = (maxDepth + 1) * ROW_H;

  return (
    <div className="scrollbar-stable relative w-full overflow-x-auto h-full" role="figure" aria-label="span flame graph">
      <div className="relative" style={{ height: totalH, minWidth: "100%" }}>
        {rows.map((r) => {
          const color = spanColorForSpan(r.span);
          const selected = r.span.span_id === selectedSpanId;
          // Real export-backed model.call spans land on `span.cost` (see
          // normalisation in api/agent-runs.ts); the legacy `attributes.cost_usd`
          // path is preserved for mock fixtures and any older event shapes.
          const cost = r.span.cost ?? (r.span.attributes as { cost_usd?: number }).cost_usd;
          const costDisplay = cost != null ? ` · ${formatCostUsd(cost)}` : "";
          const costPrecise = cost != null ? ` (${formatCostUsdPrecise(cost)})` : "";
          const pulseClass = r.span.status === "in_progress" ? "animate-pulse" : "";
          const errorClass = r.span.status === "error" ? "outline outline-1 outline-red-400" : "";
          const selectedClass = selected ? "ring-2 ring-gold" : "";
          return (
            <button
              key={r.span.span_id}
              type="button"
              data-testid={`flame-bar-${r.span.span_id}`}
              onClick={() => onSelect(r.span.span_id)}
              title={`${r.span.kind} · ${r.span.name}${costDisplay}${costPrecise}`}
              className={`absolute text-[10px] font-mono leading-[16px] px-1.5 truncate text-left text-white ${selectedClass} ${pulseClass} ${errorClass}`}
              style={{
                left: `${r.leftPct}%`,
                width: `${r.widthPct}%`,
                top: r.depth * ROW_H,
                height: ROW_H - 2,
                background: withAlpha(color.hex, 0.55),
                border: `1px solid ${withAlpha(color.hex, 0.7)}`,
                color: "rgba(15,14,12,0.9)",
                borderRadius: 2,
              }}
            >
              {r.span.name}
              {costDisplay}
            </button>
          );
        })}
      </div>
    </div>
  );
}
