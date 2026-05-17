// frontend/web/src/features/agent-runs/FlameGraph.tsx
import { useMemo } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { spanColor, withAlpha } from "./span-colors";

type LayoutRow = {
  span: RunSpan;
  depth: number;
  leftPct: number;
  widthPct: number;
};

function depthOf(span: RunSpan, byId: Map<string, RunSpan>): number {
  let depth = 0;
  let cur: RunSpan | undefined = span;
  while (cur?.parent_span_id) {
    depth += 1;
    cur = byId.get(cur.parent_span_id);
    if (depth > 32) break;
  }
  return depth;
}

function layout(spans: RunSpan[]): LayoutRow[] {
  if (spans.length === 0) return [];
  const byId = new Map(spans.map((s) => [s.span_id, s]));
  const ts = (iso: string) => new Date(iso).getTime();
  const starts = spans.map((s) => ts(s.started_at));
  const ends = spans.map((s) => (s.finished_at ? ts(s.finished_at) : Date.now()));
  const t0 = Math.min(...starts);
  const tN = Math.max(...ends);
  const span = Math.max(1, tN - t0);
  return spans
    .map((s) => {
      const start = ts(s.started_at);
      const end = s.finished_at ? ts(s.finished_at) : Date.now();
      return {
        span: s,
        depth: depthOf(s, byId),
        leftPct: ((start - t0) / span) * 100,
        widthPct: Math.max(0.5, ((end - start) / span) * 100),
      };
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
    <div className="relative w-full overflow-x-auto overflow-y-auto h-full" role="figure" aria-label="span flame graph">
      <div className="relative" style={{ height: totalH, minWidth: "100%" }}>
        {rows.map((r) => {
          const color = spanColor(r.span.kind);
          const selected = r.span.span_id === selectedSpanId;
          const cost = (r.span.attributes as { cost_usd?: number }).cost_usd;
          const pulseClass = r.span.status === "in_progress" ? "animate-pulse" : "";
          const errorClass = r.span.status === "error" ? "outline outline-1 outline-red-400" : "";
          const selectedClass = selected ? "ring-2 ring-white/80" : "";
          return (
            <button
              key={r.span.span_id}
              type="button"
              data-testid={`flame-bar-${r.span.span_id}`}
              onClick={() => onSelect(r.span.span_id)}
              title={`${r.span.kind} · ${r.span.name}${cost != null ? ` · $${cost}` : ""}`}
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
              {cost != null ? ` · $${cost}` : ""}
            </button>
          );
        })}
      </div>
    </div>
  );
}
