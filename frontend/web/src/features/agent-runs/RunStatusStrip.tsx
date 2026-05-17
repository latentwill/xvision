// frontend/web/src/features/agent-runs/RunStatusStrip.tsx
//
// Phase 1.1 — floating bottom-centre pill.
// Ported pixel-perfect from docs/superpowers/designs/2026-05-17-agent-run-observability/strip.jsx.

import type { AgentRunSummary } from "@/api/types-agent-runs";

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

const TONE: Record<StripTone, { dot: string; label: string; pulse: boolean; glow: string }> = {
  completed: { dot: "var(--gold)",   label: "COMPLETED", pulse: false, glow: "0 0 0 3px var(--gold-bg)" },
  live:      { dot: "var(--info)",   label: "LIVE",      pulse: true,  glow: "0 0 0 3px rgba(111,143,184,0.25)" },
  warn:      { dot: "var(--warn)",   label: "WARNINGS",  pulse: false, glow: "0 0 0 3px rgba(219,146,48,0.20)" },
  error:     { dot: "var(--danger)", label: "ERROR",     pulse: false, glow: "0 0 0 3px rgba(200,68,58,0.25)" },
};

// ── Helpers ───────────────────────────────────────────────────────────────────

function fmtPostHoc(ms: number | null): string {
  if (ms == null) return "—";
  return `${(ms / 1000).toFixed(1)}s`;
}

// ── Component ─────────────────────────────────────────────────────────────────

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

  const dur = isLive
    ? `0:${String(liveDurationSec).padStart(2, "0")}`
    : fmtPostHoc(summary.duration_ms);

  return (
    <div
      data-testid="run-status-strip"
      data-tone={tone}
      onClick={onExpand}
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
        boxShadow: "0 14px 40px rgba(0,0,0,0.55), 0 0 0 1px rgba(0,0,0,0.4)",
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
            boxShadow: conf.glow,
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
        <span style={{ color: "rgba(212,165,71,0.95)" }}>▓</span>
        <span style={{ color: "rgba(212,165,71,0.70)" }}>▓</span>
        <span style={{ color: "rgba(212,165,71,0.55)" }}>▓</span>
        <span style={{ color: "rgba(212,165,71,0.40)" }}>▒</span>
        <span style={{ color: "rgba(212,165,71,0.28)" }}>▒</span>
        <span style={{ color: "rgba(212,165,71,0.18)" }}>░</span>
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
        <span style={{ fontVariantNumeric: "tabular-nums" }}>${summary.total_cost_usd.toFixed(4)}</span>
      </div>

      {/* CurrentSpan chip */}
      {currentSpan != null && (
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
                  background: currentSpan.color,
                  boxShadow: `0 0 0 3px ${currentSpan.color}22`,
                }}
              />
            ) : (
              <span
                style={{
                  display: "inline-block",
                  width: 3,
                  height: 12,
                  flexShrink: 0,
                  background: currentSpan.color,
                }}
              />
            )}
            <span
              style={{
                fontFamily: "var(--font-mono, ui-monospace, monospace)",
                fontSize: 10,
                letterSpacing: "0.16em",
                color: currentSpan.color,
                flexShrink: 0,
              }}
            >
              {currentSpan.label}
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
              {currentSpan.name}
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
              {currentSpan.elapsedMs}ms
            </span>
          </div>
        </>
      )}

      {/* Error pill */}
      {tone === "error" && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "2px 6px",
            borderRadius: 999,
            background: "rgba(200,68,58,0.14)",
            border: "1px solid rgba(200,68,58,0.45)",
            flexShrink: 0,
          }}
        >
          <span
            style={{
              display: "inline-block",
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: "var(--danger)",
            }}
          />
          <span
            style={{
              fontFamily: "var(--font-mono, ui-monospace, monospace)",
              fontSize: 10,
              letterSpacing: "0.05em",
              color: "var(--danger)",
            }}
          >
            1 error
          </span>
        </div>
      )}

      {/* Warn pill */}
      {tone === "warn" && (
        <div
          style={{
            display: "flex",
            alignItems: "center",
            gap: 6,
            padding: "2px 6px",
            borderRadius: 999,
            background: "rgba(219,146,48,0.10)",
            border: "1px solid rgba(219,146,48,0.40)",
            flexShrink: 0,
          }}
        >
          <span
            style={{
              fontFamily: "var(--font-mono, ui-monospace, monospace)",
              fontSize: 10,
              letterSpacing: "0.05em",
              color: "var(--warn)",
            }}
          >
            2 warnings
          </span>
        </div>
      )}

      {/* Right divider + icon buttons */}
      <div style={{ width: 1, height: 14, background: "var(--border)", flexShrink: 0 }} />

      <div style={{ display: "flex", alignItems: "center", gap: 2, paddingRight: 4, flexShrink: 0 }}>
        {/* Expand button */}
        <button
          onClick={(e) => { e.stopPropagation(); onExpand(); }}
          title="Expand trace dock (F12)"
          style={{
            height: 24,
            width: 28,
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
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
            <path d="M3 10l5-5 5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          </svg>
        </button>

        {/* Pop-out button */}
        <button
          aria-label="open dedicated trace view"
          onClick={(e) => { e.stopPropagation(); onPopOut(); }}
          title="Open in dedicated route"
          style={{
            height: 24,
            width: 28,
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
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none">
            <path d="M6 3h7v7M13 3l-7 7M3 8v5h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </button>
      </div>
    </div>
  );
}
