// frontend/web/src/features/agent-runs/RunStatusStrip.tsx
//
// Phase 1.1 — floating bottom-centre pill.
// Ported pixel-perfect from docs/superpowers/designs/2026-05-17-agent-run-observability/strip.jsx.

import { useEffect, useMemo, useState, type ReactNode } from "react";
import type { AgentRunSummary } from "@/api/types-agent-runs";
import { formatCostUsd, formatCostUsdPrecise } from "@/lib/format";
import { useTraceDock } from "@/stores/trace-dock";
import { spanColor } from "./span-colors";
import { TrajectoryModePill } from "./TrajectoryModeBadge";

// ── Exported types ────────────────────────────────────────────────────────────

export type StripTone = "completed" | "live" | "warn" | "error";

export type CurrentSpanChip = {
  name: string;
  color: string;    // hex from CATEGORY_STYLES[cat].hex
  label: string;    // CATEGORY_STYLES[cat].label
  elapsedMs: number;
};

export type RunStatusStripProps = {
  summary: AgentRunSummary;
  currentSpan: CurrentSpanChip | null;
  isLive: boolean;
  liveDurationSec: number;   // ticking counter shown when isLive
  tone: StripTone;           // injected by parent
  onExpand: () => void;
  onPopOut: () => void;
};

// ── Tone config ───────────────────────────────────────────────────────────────

// `dotGlow` is the small halo around the leading status dot. `capsuleGlow`
// is a larger outer ring on the capsule itself so the floating control
// reads as a discrete element instead of blending into the page surface.
// Per-tone hues match the dot so the capsule visibly carries its state.
const TONE: Record<
  StripTone,
  { dot: string; label: string; pulse: boolean; dotGlow: string; capsuleGlow: string }
> = {
  completed: {
    dot: "var(--gold)",
    label: "COMPLETED",
    pulse: false,
    dotGlow: "0 0 0 3px var(--gold-bg)",
    capsuleGlow: "0 0 24px 2px rgba(0,230,118,0.22), 0 0 0 1px rgba(0,230,118,0.35)",
  },
  live: {
    dot: "var(--info)",
    label: "LIVE",
    pulse: true,
    dotGlow: "0 0 0 3px rgba(111,143,184,0.25)",
    capsuleGlow: "0 0 28px 2px rgba(111,143,184,0.30), 0 0 0 1px rgba(111,143,184,0.45)",
  },
  warn: {
    dot: "var(--warn)",
    label: "WARNINGS",
    pulse: false,
    dotGlow: "0 0 0 3px rgba(219,146,48,0.20)",
    capsuleGlow: "0 0 24px 2px rgba(219,146,48,0.24), 0 0 0 1px rgba(219,146,48,0.40)",
  },
  error: {
    dot: "var(--danger)",
    label: "ERROR",
    pulse: false,
    dotGlow: "0 0 0 3px rgba(255,77,77,0.25)",
    capsuleGlow: "0 0 28px 2px rgba(255,77,77,0.32), 0 0 0 1px rgba(255,77,77,0.50)",
  },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtPostHoc(ms: number | null): string {
  if (ms == null) return "—";
  return `${(ms / 1000).toFixed(1)}s`;
}

function TonePill({
  dotColor,
  background,
  border,
  textColor,
  children,
}: {
  dotColor?: string;
  background: string;
  border: string;
  textColor: string;
  children: ReactNode;
}) {
  return (
    <div
      style={{
        display: "flex", alignItems: "center", gap: 6,
        padding: "2px 6px", borderRadius: 999,
        background, border: `1px solid ${border}`, flexShrink: 0,
      }}
    >
      {dotColor != null && (
        <span
          style={{ width: 6, height: 6, borderRadius: 999, background: dotColor }}
        />
      )}
      <span
        style={{
          color: textColor,
          fontSize: 10, letterSpacing: "0.04em",
          fontFamily: "var(--font-mono, ui-monospace, monospace)",
        }}
      >
        {children}
      </span>
    </div>
  );
}

// ── Component ─────────────────────────────────────────────────────────────────

/**
 * Derive a `CurrentSpanChip` from the SSE streaming slice when the
 * parent hasn't computed one yet. Returns the newest active span
 * (highest `started_at`) so a long-running parent doesn't shadow a
 * fresh in-flight leaf. Stable identity per dependency set so React
 * doesn't churn the strip during keep-alive frames.
 */
function useLiveActiveSpanChip(isLive: boolean): CurrentSpanChip | null {
  const activeMeta = useTraceDock((s) => s.streamingState.activeSpanMeta);
  // Local 1-second tick so the chip's elapsed time advances even when
  // the SSE feed is quiet between events. `nowMs` is intentionally a
  // dependency of the memo so the elapsed value recomputes each tick.
  const [nowMs, setNowMs] = useState<number>(() => Date.now());
  const hasActive = isLive && Object.keys(activeMeta).length > 0;
  useEffect(() => {
    if (!hasActive) return;
    setNowMs(Date.now());
    const id = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [hasActive]);

  return useMemo<CurrentSpanChip | null>(() => {
    if (!isLive) return null;
    const ids = Object.keys(activeMeta);
    if (ids.length === 0) return null;
    let best: { id: string; startedMs: number } | null = null;
    for (const id of ids) {
      const meta = activeMeta[id]!;
      const startedMs = new Date(meta.started_at).getTime();
      if (!Number.isFinite(startedMs)) continue;
      if (best == null || startedMs > best.startedMs) {
        best = { id, startedMs };
      }
    }
    if (best == null) return null;
    const meta = activeMeta[best.id]!;
    const color = spanColor(meta.kind);
    return {
      name: meta.name,
      color: color.hex,
      label: color.label,
      elapsedMs: Math.max(0, nowMs - best.startedMs),
    };
  }, [activeMeta, isLive, nowMs]);
}

export function RunStatusStrip({
  summary,
  currentSpan,
  isLive,
  liveDurationSec,
  tone,
  onExpand,
  onPopOut,
}: RunStatusStripProps) {
  const conf = TONE[tone];

  const mins = Math.floor(liveDurationSec / 60);
  const secs = liveDurationSec % 60;
  const liveDurDisplay = `${mins}:${String(secs).padStart(2, "0")}`;
  const dur = isLive ? liveDurDisplay : fmtPostHoc(summary.duration_ms);

  // Acceptance: when a stream is open and any span is active, show the
  // currently-active span (highest `started_at` among active spans)
  // with elapsed time. Existing post-hoc behavior unchanged: parent
  // explicit `currentSpan` wins; we only fill in when prop is null.
  const liveChip = useLiveActiveSpanChip(isLive);
  const effectiveCurrentSpan = currentSpan ?? liveChip;

  return (
    <div
      data-testid="run-status-strip"
      data-tone={tone}
      role="button"
      tabIndex={0}
      aria-label="Expand trace dock"
      onClick={onExpand}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onExpand();
        }
      }}
      title="Click to expand the trace dock (F12)"
      style={{
        position: "fixed",
        bottom: 14,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 40,
        height: 32,
        borderRadius: 999,
        background: "var(--surface-elev)",
        border: "1px solid var(--border-strong)",
        boxShadow: `${conf.capsuleGlow}, 0 14px 40px rgba(0,0,0,0.55), 0 0 0 1px rgba(0,0,0,0.4)`,
        backdropFilter: "blur(8px)",
        maxWidth: "calc(100vw - 32px)",
        cursor: "pointer",
        userSelect: "none",
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "0 12px",
        whiteSpace: "nowrap",
      }}
    >
      {/* Tone block */}
      <div style={{ display: "flex", alignItems: "center", gap: 8, flexShrink: 0, paddingLeft: 4 }}>
        <span
          className={conf.pulse ? "animate-pulse" : ""}
          style={{
            display: "inline-block",
            width: 6,
            height: 6,
            borderRadius: "50%",
            background: conf.dot,
            boxShadow: conf.dotGlow,
          }}
        />
        <span
          style={{
            fontFamily: "var(--font-mono, ui-monospace, monospace)",
            letterSpacing: "0.18em",
            fontSize: 10,
            color: "var(--text-3)",
          }}
        >
          {conf.label}
        </span>
        {/* Trajectory-mode pill — shown only when backend field is present.
            Renders beside the status dot so operators see live/record/replay
            at a glance without reading the detail card. */}
        {summary.trajectory_mode ? (
          <TrajectoryModePill mode={summary.trajectory_mode} />
        ) : null}
      </div>

      {/* Divider */}
      <div style={{ width: 1, height: 14, background: "var(--border)", flexShrink: 0 }} />

      {/* Density-glyph row */}
      <div
        style={{
          fontFamily: "var(--font-mono, ui-monospace, monospace)",
          fontSize: 11,
          lineHeight: 1,
          letterSpacing: -1,
          flexShrink: 0,
        }}
      >
        <span style={{ color: "rgba(0,230,118,0.95)" }}>▓</span>
        <span style={{ color: "rgba(0,230,118,0.70)" }}>▓</span>
        <span style={{ color: "rgba(0,230,118,0.55)" }}>▓</span>
        <span style={{ color: "rgba(0,230,118,0.40)" }}>▒</span>
        <span style={{ color: "rgba(0,230,118,0.28)" }}>▒</span>
        <span style={{ color: "rgba(0,230,118,0.18)" }}>░</span>
        <span style={{ color: "var(--text-4)" }}>░</span>
      </div>

      {/* Aggregates block */}
      <div
        style={{
          fontFamily: "var(--font-mono, ui-monospace, monospace)",
          fontSize: 11,
          color: "var(--text)",
        }}
      >
        <span style={{ color: "var(--text-3)" }}>spans </span>
        <span style={{ fontVariantNumeric: "tabular-nums" }}>{summary.span_count}</span>
        <span style={{ color: "var(--text-4)", margin: "0 8px" }}>·</span>
        <span style={{ color: "var(--text-3)" }}>model </span>
        <span style={{ fontVariantNumeric: "tabular-nums" }}>{summary.model_call_count}</span>
        <span style={{ color: "var(--text-4)", margin: "0 8px" }}>·</span>
        <span style={{ fontVariantNumeric: "tabular-nums" }}>{dur}</span>
        <span style={{ color: "var(--text-4)", margin: "0 8px" }}>·</span>
        <span
          style={{ fontVariantNumeric: "tabular-nums" }}
          title={formatCostUsdPrecise(summary.total_cost_usd)}
        >
          {formatCostUsd(summary.total_cost_usd)}
        </span>
      </div>

      {/* CurrentSpan chip */}
      {effectiveCurrentSpan != null && (
        <>
          <div style={{ width: 1, height: 14, background: "var(--border)", flexShrink: 0 }} />
          <div
            style={{
              display: "flex",
              alignItems: "center",
              gap: 6,
              maxWidth: 260,
              minWidth: 0,
            }}
          >
            {isLive ? (
              <span
                className="animate-pulse"
                style={{
                  display: "inline-block",
                  width: 6,
                  height: 6,
                  borderRadius: "50%",
                  flexShrink: 0,
                  background: effectiveCurrentSpan.color,
                  boxShadow: `0 0 0 3px ${effectiveCurrentSpan.color}22`,
                }}
              />
            ) : (
              <span
                style={{
                  display: "inline-block",
                  width: 3,
                  height: 12,
                  flexShrink: 0,
                  background: effectiveCurrentSpan.color,
                }}
              />
            )}
            <span
              style={{
                fontFamily: "var(--font-mono, ui-monospace, monospace)",
                fontSize: 10,
                letterSpacing: "0.16em",
                color: effectiveCurrentSpan.color,
                flexShrink: 0,
              }}
            >
              {effectiveCurrentSpan.label}
            </span>
            <span
              style={{
                fontFamily: "var(--font-mono, ui-monospace, monospace)",
                fontSize: 11,
                color: "var(--text)",
                overflow: "hidden",
                textOverflow: "ellipsis",
                minWidth: 0,
              }}
            >
              {effectiveCurrentSpan.name}
            </span>
            <span
              style={{
                fontFamily: "var(--font-mono, ui-monospace, monospace)",
                fontSize: 10,
                fontVariantNumeric: "tabular-nums",
                color: "var(--text-3)",
                flexShrink: 0,
              }}
            >
              {effectiveCurrentSpan.elapsedMs}ms
            </span>
          </div>
        </>
      )}

      {/* Error pill */}
      {tone === "error" && (
        <TonePill
          dotColor="var(--danger)"
          background="rgba(255,77,77,0.14)"
          border="rgba(255,77,77,0.45)"
          textColor="var(--danger)"
        >
          {summary.error_count} error{summary.error_count !== 1 ? "s" : ""}
        </TonePill>
      )}

      {/* Warn pill */}
      {tone === "warn" && (
        // TODO: wire to summary.warning_count when the backend adds it
        <TonePill
          background="rgba(219,146,48,0.10)"
          border="rgba(219,146,48,0.40)"
          textColor="var(--warn)"
        >
          2 warnings
        </TonePill>
      )}

      {/* Right divider + icon buttons */}
      <div style={{ width: 1, height: 14, background: "var(--border)", flexShrink: 0 }} />

      <div style={{ display: "flex", alignItems: "center", gap: 2, paddingRight: 4, flexShrink: 0 }}>
        {/* Expand button. Icon bumped from 11px to 14px per qa-operator-2026-05-17 readability fix. */}
        <button
          onClick={(e) => { e.stopPropagation(); onExpand(); }}
          aria-label="Expand trace dock"
          title="Expand trace dock (F12)"
          style={{
            height: 26,
            width: 30,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--text-3)",
            borderRadius: "50%",
            background: "transparent",
            border: "none",
            cursor: "pointer",
            padding: 0,
          }}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
            <path d="M3 10l5-5 5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </button>

        {/* Pop-out button. Icon bumped from 11px to 14px per qa-operator-2026-05-17 readability fix. */}
        <button
          aria-label="open dedicated trace view"
          onClick={(e) => { e.stopPropagation(); onPopOut(); }}
          title="Open in dedicated route"
          style={{
            height: 26,
            width: 30,
            display: "flex",
            alignItems: "center",
            justifyContent: "center",
            color: "var(--text-3)",
            borderRadius: "50%",
            background: "transparent",
            border: "none",
            cursor: "pointer",
            padding: 0,
          }}
        >
          <svg width="14" height="14" viewBox="0 0 16 16" fill="none">
            <path d="M6 3h7v7M13 3l-7 7M3 8v5h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      </div>
    </div>
  );
}
