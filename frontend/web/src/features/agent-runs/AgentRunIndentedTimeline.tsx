// frontend/web/src/features/agent-runs/AgentRunIndentedTimeline.tsx
import { useMemo } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { categoryOf, spanColorForSpan, withAlpha } from "./span-colors";

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

type Row = {
  span: RunSpan;
  depth: number;
  leftPct: number;
  widthPct: number;
  durationMs: number | null;
};

function buildRows(spans: RunSpan[]): Row[] {
  if (spans.length === 0) return [];
  const byId = new Map(spans.map((s) => [s.span_id, s]));
  const starts = spans.map((s) => ts(s.started_at));
  const ends = spans.map((s) => (s.finished_at ? ts(s.finished_at) : Date.now()));
  const t0 = Math.min(...starts);
  const tN = Math.max(...ends);
  const window = Math.max(1, tN - t0);

  const ordered = [...spans].sort((a, b) => ts(a.started_at) - ts(b.started_at));

  return ordered.map((span) => {
    const start = ts(span.started_at);
    const end = span.finished_at ? ts(span.finished_at) : Date.now();
    const leftPct = ((start - t0) / window) * 100;
    const widthPct = Math.max(0.6, ((end - start) / window) * 100);
    return {
      span,
      depth: depthOf(span, byId),
      leftPct,
      widthPct: Math.min(100 - leftPct, widthPct),
      durationMs: span.finished_at ? end - start : null,
    };
  });
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
  const visibleSpans = useMemo(
    // Hide supervisor-category instrumentation, but keep engine.event rows
    // (WS-8) — they resolve to `unknown` via the bare-kind lookup yet carry
    // first-class lifecycle signals that must render.
    () =>
      spans.filter(
        (span) =>
          span.kind === "engine.event" || categoryOf(span.kind) !== "supervisor",
      ),
    [spans],
  );
  const rows = useMemo(() => buildRows(visibleSpans), [visibleSpans]);

  if (rows.length === 0) {
    return (
      <div className="font-mono text-[12px] text-text-3 p-3" aria-label="no spans">
        no spans match the current filter
      </div>
    );
  }

  return (
    <div
      className="font-mono text-[12px] overflow-y-auto h-full"
      role="tree"
      aria-label="span waterfall"
    >
      {rows.map((row) => {
        const { span, depth, leftPct, widthPct, durationMs } = row;
        const color = spanColorForSpan(span);
        const selected = span.span_id === selectedSpanId;
        const isError = span.status === "error";
        const isLive = span.status === "in_progress";

        return (
          <button
            key={span.span_id}
            type="button"
            data-testid={`span-row-${span.span_id}`}
            data-depth={depth}
            data-selected={selected}
            onClick={() => onSelect(span.span_id)}
            className={`group w-full grid grid-cols-[minmax(0,3fr)_minmax(0,5fr)_64px] items-start gap-3 px-3 py-1.5 text-left border-l-2 hover:bg-surface-elev/60 ${
              selected ? "bg-surface-elev" : ""
            }`}
            style={{
              borderLeftColor: selected ? color.hex : "transparent",
            }}
          >
            {/* Label column: indent + dot + kind chip + full name (wraps).
                IDs are not truncated — model paths like
                `openrouter/deepseek/deepseek-v4-pro` must render in full. */}
            <div
              className="flex items-start gap-2 min-w-0 leading-snug"
              style={{ paddingLeft: `${depth * 1.1}rem` }}
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
              <span
                className="text-text break-all"
                title={`${span.kind} · ${span.name}`}
              >
                {span.name}
              </span>
              {isError ? (
                <span className="text-red-400 text-[10px] shrink-0" aria-label="error">●</span>
              ) : null}
              {isLive ? (
                <span
                  className="text-blue-300 text-[10px] shrink-0 animate-pulse"
                  aria-label="in progress"
                >
                  ●
                </span>
              ) : null}
            </div>

            {/* Waterfall column: a track + a positioned bar */}
            <div className="relative h-3 mt-1 rounded-sm bg-surface-elev/40 overflow-hidden">
              <div
                data-testid={`span-waterfall-bar-${span.span_id}`}
                className={`absolute top-0 bottom-0 rounded-sm ${isLive ? "animate-pulse" : ""}`}
                style={{
                  left: `${leftPct}%`,
                  width: `${widthPct}%`,
                  background: withAlpha(color.hex, 0.7),
                  outline: isError
                    ? `1px solid rgba(239,68,68,0.8)`
                    : selected
                      ? `1px solid ${color.hex}`
                      : `1px solid ${withAlpha(color.hex, 0.9)}`,
                }}
              />
            </div>

            {/* Duration column */}
            <span className="text-text-3 text-right tabular-nums mt-px">
              {formatMs(durationMs)}
            </span>
          </button>
        );
      })}
    </div>
  );
}
