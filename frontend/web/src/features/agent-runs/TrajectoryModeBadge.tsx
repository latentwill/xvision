// frontend/web/src/features/agent-runs/TrajectoryModeBadge.tsx
//
// Inline trajectory-mode badge + replay-metrics row, surfacing
// the Stage-1-declared fields from migration 039:
//   trajectory_mode, replay_hit_ratio, dropped_events, recovery_reason.
//
// Renders defensively: absent or `"live"` fields are either omitted or
// shown with reduced emphasis so pre-migration runs look unchanged.
//
// Hard project rule: NO popups / modals / popovers. Everything inline.

import type { AgentRunSummary, TrajectoryMode } from "@/api/types-agent-runs";

// ── Per-mode visual config ─────────────────────────────────────────────────────

type ModeConfig = {
  label: string;
  /** CSS color token or literal for the badge text and border. */
  color: string;
  /** Low-opacity background so dark-mode works without harsh white/full-sat. */
  bg: string;
  border: string;
};

const MODE_CONFIG: Record<TrajectoryMode, ModeConfig> = {
  live: {
    label: "LIVE",
    color: "var(--info)",
    bg: "rgba(111,143,184,0.10)",
    border: "rgba(111,143,184,0.35)",
  },
  record: {
    label: "RECORD",
    color: "var(--gold)",
    bg: "rgba(219,176,48,0.10)",
    border: "rgba(219,176,48,0.35)",
  },
  replay: {
    label: "REPLAY",
    color: "var(--text)",
    bg: "var(--surface-elev)",
    border: "var(--border-strong)",
  },
};

// ── Recovery-reason display text ───────────────────────────────────────────────

function recoveryReasonLabel(reason: string): string {
  switch (reason) {
    case "replay_divergence":
      return "replay diverged";
    case "replay_frames_exhausted":
      return "frames exhausted";
    default:
      return reason;
  }
}

// ── Sub-components ─────────────────────────────────────────────────────────────

/**
 * Inline chip for the trajectory mode label: `LIVE`, `RECORD`, or `REPLAY`.
 * Omitted entirely when `trajectory_mode` is absent (pre-migration runs).
 */
export function TrajectoryModePill({ mode }: { mode: TrajectoryMode }) {
  const conf = MODE_CONFIG[mode];
  return (
    <span
      data-testid="trajectory-mode-pill"
      data-mode={mode}
      style={{
        display: "inline-flex",
        alignItems: "center",
        padding: "1px 6px",
        borderRadius: 999,
        background: conf.bg,
        border: `1px solid ${conf.border}`,
        fontFamily: "var(--font-mono, ui-monospace, monospace)",
        fontSize: 10,
        letterSpacing: "0.08em",
        color: conf.color,
        flexShrink: 0,
      }}
    >
      {conf.label}
    </span>
  );
}

/**
 * Inline recovery-reason warning chip. Shown only when `recovery_reason`
 * is a non-empty string. Uses amber/warn tones with `dark:` equivalents
 * via CSS vars (no `border-white` / `#fff`).
 */
function RecoveryReasonChip({ reason }: { reason: string }) {
  return (
    <span
      data-testid="recovery-reason-chip"
      data-reason={reason}
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "1px 6px",
        borderRadius: 999,
        background: "rgba(219,146,48,0.10)",
        border: "1px solid rgba(219,146,48,0.40)",
        fontFamily: "var(--font-mono, ui-monospace, monospace)",
        fontSize: 10,
        letterSpacing: "0.06em",
        color: "var(--warn)",
        flexShrink: 0,
      }}
    >
      {/* Warning glyph */}
      <span aria-hidden="true" style={{ fontSize: 9 }}>!</span>
      {recoveryReasonLabel(reason)}
    </span>
  );
}

// ── Main export ────────────────────────────────────────────────────────────────

/**
 * Composite inline badge group for the agent-run detail header card.
 * Renders:
 *   - Trajectory-mode pill (`LIVE` / `RECORD` / `REPLAY`) — always when
 *     `trajectory_mode` is present.
 *   - Replay hit-ratio label — only in `replay` mode.
 *   - Dropped-events count — only when > 0.
 *   - Recovery-reason chip (amber warning) — only when set.
 *
 * All inline — no popups, no popovers.
 */
export function TrajectoryModeBadge({ summary }: { summary: AgentRunSummary }) {
  const mode = summary.trajectory_mode;

  // Pre-migration runs have no trajectory_mode — render nothing so the
  // existing header layout is unaffected.
  if (!mode) return null;

  const isReplay = mode === "replay";
  const hitRatio = summary.replay_hit_ratio;
  const droppedEvents = summary.dropped_events;
  const recoveryReason = summary.recovery_reason;

  return (
    <span
      data-testid="trajectory-mode-badge"
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 6,
        flexShrink: 0,
      }}
    >
      <TrajectoryModePill mode={mode} />

      {isReplay && hitRatio != null && (
        <span
          data-testid="replay-hit-ratio"
          style={{
            fontFamily: "var(--font-mono, ui-monospace, monospace)",
            fontSize: 11,
            color: "var(--text-2)",
            flexShrink: 0,
          }}
          title="Fraction of model-call steps served from recorded frames"
        >
          hit {(hitRatio * 100).toFixed(0)}%
        </span>
      )}

      {droppedEvents != null && droppedEvents > 0 && (
        <span
          data-testid="dropped-events"
          style={{
            fontFamily: "var(--font-mono, ui-monospace, monospace)",
            fontSize: 11,
            color: "var(--danger)",
            flexShrink: 0,
          }}
          title="Events dropped due to buffer pressure"
        >
          {droppedEvents} dropped
        </span>
      )}

      {recoveryReason ? (
        <RecoveryReasonChip reason={recoveryReason} />
      ) : null}
    </span>
  );
}
