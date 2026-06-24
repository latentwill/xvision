// Signal decision-density strip — README §8 + Implementation notes / Task B4
// step 6.
//
// A horizontal density bar that scales from ~10 to ~1000+ decisions without
// changing layout. One thin column per decision, ordered by index. Renders with
// absolute positioning inside a relative container (NOT flex) so the pixel math
// is exact: `tickW = min(6, max(1, floor(width / n)))`, gap 1px when tickW ≥ 4
// else 0, slot = tickW + gap.
//
// Per decision the hit-area is the FULL 36px column height (so filtered ticks
// stay clickable); the visible "ink" inside is phase-dependent — engaged ticks
// fill 32px from the top colored by action, filtered ticks are a 10px stub
// anchored to the bottom in `--text-3`. Click → onJump(i). Hover → a non-
// interactive absolutely-positioned tooltip (allowed under the no-popups rule —
// it does not steal focus). Active filter dims non-matching ticks (does not
// remove them). The focused decision gets a small down-triangle marker above.

import { useEffect, useMemo, useRef, useState } from "react";
import type { ActionFilter } from "./decision-table-state";
import { matchesActionFilter } from "./decision-table-state";
import { fmtStepStamp, type TimelineDecision } from "./decision-view";

const COLOR_BY_ACTION: Record<string, string> = {
  LONG: "var(--gold)",
  SELL: "var(--danger)",
  SHORT: "var(--danger)",
  CLOSE: "var(--danger)",
  HOLD: "var(--text-2)",
};

const DIM_COLOR = "var(--border-strong)";

type HoverState = {
  x: number;
  d: TimelineDecision;
};

function windowLabel(t: string): string {
  const date = new Date(t);
  if (Number.isNaN(date.getTime())) return t;
  const hh = String(date.getUTCHours()).padStart(2, "0");
  const mm = String(date.getUTCMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

export function DecisionTimeline({
  decisions,
  focusedIdx,
  onJump,
  activeFilter,
  stepsCount,
}: {
  decisions: TimelineDecision[];
  focusedIdx: number | null;
  onJump: (i: number) => void;
  activeFilter: ActionFilter;
  /** Distinct step count (passed from the parent so the strip header reports
   *  step moments, not per-asset rows — the previous label used
   *  `decisions.length` and read "22 steps" on a 5-step / 5-asset run). */
  stepsCount: number;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [hover, setHover] = useState<HoverState | null>(null);
  const [width, setWidth] = useState(800);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    if (el.clientWidth) setWidth(el.clientWidth);
    // jsdom / SSR lack ResizeObserver; the initial clientWidth measurement is
    // enough for those environments. Only subscribe when the API exists.
    if (typeof ResizeObserver === "undefined") return;
    const ro = new ResizeObserver((entries) => {
      for (const e of entries) setWidth(e.contentRect.width);
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  const sorted = useMemo(() => [...decisions].sort((a, b) => a.i - b.i), [decisions]);
  const n = sorted.length;

  const focusedSlot = useMemo(() => {
    if (focusedIdx == null) return null;
    const idx = sorted.findIndex((d) => d.i === focusedIdx);
    return idx < 0 ? null : idx;
  }, [sorted, focusedIdx]);

  if (n === 0) return null;

  const tickW = Math.min(6, Math.max(1, Math.floor(width / n)));
  const gap = tickW >= 4 ? 1 : 0;
  const slot = tickW + gap;

  const isDim = (d: TimelineDecision): boolean => {
    if (activeFilter === "all") return false;
    return !matchesActionFilter(d, activeFilter);
  };

  const firstWindow = windowLabel(sorted[0]!.t);

  return (
    <div
      className="px-5 pt-4 pb-3"
      style={{ borderBottom: "1px solid var(--border-soft)" }}
      data-testid="decision-density-strip"
    >
      <div className="flex items-center justify-between mb-2.5">
        <div className="flex items-baseline gap-2.5">
          <span className="text-[10px] font-mono tracking-[0.18em] text-text-3 uppercase">
            Density
          </span>
          <span className="text-[10.5px] font-mono text-text-3">
            <span className="text-text-2 tabular-nums">{stepsCount}</span>{" "}
            {stepsCount === 1 ? "step" : "steps"} ·{" "}
            <span className="tabular-nums">{n}</span> trader{" "}
            {n === 1 ? "call" : "calls"} ·{" "}
            <span className="tabular-nums">{firstWindow}</span> window
          </span>
        </div>
        <div className="flex items-center gap-3 text-[10px] font-mono text-text-3">
          <span className="inline-flex items-center gap-1.5">
            <span style={{ width: 9, height: 9, background: "var(--gold)" }} />
            long
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span style={{ width: 9, height: 9, background: "var(--danger)" }} />
            sell
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span style={{ width: 9, height: 9, background: "var(--text-2)" }} />
            hold
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span
              style={{ width: 9, height: 4, background: "var(--border-strong)", alignSelf: "flex-end" }}
            />
            filtered
          </span>
        </div>
      </div>

      <div
        ref={containerRef}
        className="relative"
        style={{
          height: 36,
          background: "var(--surface-elev)",
          border: "1px solid var(--border-soft)",
          borderRadius: 3,
        }}
        onMouseLeave={() => setHover(null)}
      >
        {sorted.map((d, idx) => {
          const isFiltered = d.phase === "filtered";
          const dim = isDim(d);
          const color = dim
            ? DIM_COLOR
            : isFiltered
              ? "var(--text-3)"
              : COLOR_BY_ACTION[d.action ?? "HOLD"] ?? "var(--text-2)";
          const isFocus = d.i === focusedIdx;
          return (
            <div
              key={d.i}
              role="button"
              tabIndex={0}
              aria-label={`Jump to decision ${d.i}`}
              onClick={() => onJump(d.i)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") {
                  e.preventDefault();
                  onJump(d.i);
                }
              }}
              onMouseEnter={() => setHover({ x: idx * slot + tickW / 2, d })}
              style={{
                position: "absolute",
                left: idx * slot,
                top: 0,
                width: tickW,
                height: 36,
                cursor: "pointer",
                opacity: dim ? 0.45 : 1,
                transition: "opacity 0.15s",
              }}
            >
              <div
                style={{
                  position: "absolute",
                  left: 0,
                  width: tickW,
                  bottom: isFiltered ? 1 : 2,
                  height: isFiltered ? 10 : 32,
                  background: color,
                  boxShadow: isFocus
                    ? "0 0 0 1.5px var(--gold), 0 0 0 3px var(--gold-bg)"
                    : "none",
                  pointerEvents: "none",
                }}
              />
            </div>
          );
        })}

        {focusedSlot != null && (
          <div
            aria-hidden
            style={{
              position: "absolute",
              left: focusedSlot * slot + tickW / 2 - 5,
              top: -6,
              width: 0,
              height: 0,
              borderLeft: "5px solid transparent",
              borderRight: "5px solid transparent",
              borderTop: "5px solid var(--gold)",
            }}
          />
        )}

        {hover && (
          <div
            className="pointer-events-none absolute z-10 px-2 py-1.5 font-mono text-[10.5px] whitespace-nowrap"
            style={{
              left: Math.min(Math.max(hover.x, 80), Math.max(width - 80, 80)),
              transform: "translate(-50%, calc(-100% - 10px))",
              top: 0,
              background: "var(--surface-card)",
              border: "1px solid var(--border-strong)",
              borderRadius: 4,
              color: "var(--text)",
              boxShadow: "0 8px 20px rgba(0,0,0,0.5)",
            }}
          >
            <div className="flex items-center gap-2 mb-0.5">
              <span className="text-text-3">#</span>
              <span className="tabular-nums">{hover.d.i}</span>
              <span className="text-text-4">·</span>
              <span className="tabular-nums text-text-2">{fmtStepStamp(hover.d.t)}</span>
              <span className="text-text-4">·</span>
              <span
                style={{
                  color:
                    hover.d.phase === "filtered"
                      ? "var(--text-3)"
                      : hover.d.action === "LONG"
                        ? "var(--gold)"
                        : hover.d.action === "SELL" || hover.d.action === "SHORT" || hover.d.action === "CLOSE"
                          ? "var(--danger)"
                          : "var(--text)",
                }}
              >
                {hover.d.phase === "filtered" ? "NO-OP" : hover.d.action}
              </span>
              {hover.d.conv != null && hover.d.phase !== "filtered" && (
                <>
                  <span className="text-text-4">·</span>
                  <span className="tabular-nums text-text-2">
                    {(hover.d.conv * 100).toFixed(0)}%
                  </span>
                </>
              )}
            </div>
            {hover.d.just && hover.d.phase !== "filtered" && (
              <div className="text-text-3 max-w-[280px] truncate">{hover.d.just}</div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}
