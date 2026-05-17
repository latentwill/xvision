// frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx
import type { RunSpan } from "@/api/types-agent-runs";
import { spanColor } from "./span-colors";

function depthOf(span: RunSpan, byId: Map<string, RunSpan>): number {
  let depth = 0;
  let cur: RunSpan | undefined = span;
  while (cur?.parent_span_id) {
    depth += 1;
    cur = byId.get(cur.parent_span_id);
    if (depth > 32) break; // cycle guard
  }
  return depth;
}

function durationMs(s: RunSpan): number | null {
  if (!s.finished_at) return null;
  return new Date(s.finished_at).getTime() - new Date(s.started_at).getTime();
}

export function AgentRunIndentedTimeline({
  spans,
  selectedSpanId,
  onSelect,
}: {
  spans: RunSpan[];
  selectedSpanId: string | null;
  onSelect: (spanId: string) => void;
}) {
  const byId = new Map(spans.map((s) => [s.span_id, s]));
  const ordered = [...spans].sort(
    (a, b) => new Date(a.started_at).getTime() - new Date(b.started_at).getTime(),
  );
  return (
    <div className="font-mono text-[12px] overflow-y-auto">
      {ordered.map((s) => {
        const depth = depthOf(s, byId);
        const ms = durationMs(s);
        const color = spanColor(s.kind);
        const selected = s.span_id === selectedSpanId;
        return (
          <button
            key={s.span_id}
            type="button"
            data-testid={`span-row-${s.span_id}`}
            data-depth={depth}
            data-selected={selected}
            onClick={() => onSelect(s.span_id)}
            className={`w-full flex items-center gap-2 px-2 py-1 text-left hover:bg-surface-elev ${selected ? "bg-surface-elev" : ""}`}
            style={{ paddingLeft: `${0.5 + depth * 1.25}rem` }}
          >
            <span className="inline-block w-2 h-2 rounded-sm" style={{ background: color.hex }} aria-hidden />
            <span className="text-text-2">{s.kind}</span>
            <span className="text-text">{s.name}</span>
            <span className="ml-auto text-text-3">{ms != null ? `${ms}ms` : "…"}</span>
            {s.status === "error" ? <span className="text-red-400">●</span> : null}
            {s.status === "in_progress" ? <span className="text-blue-400 animate-pulse">●</span> : null}
          </button>
        );
      })}
    </div>
  );
}
