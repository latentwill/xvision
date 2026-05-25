// Signal eval-run topbar — 48px chrome row above the run body. README §1 /
// Task B4 step 7.
//
// Layout: BrandMark · "/" · EVAL RUNS (links to the list) · "/" · full run id
// (no truncation) · spacer · status pill on the right.
//
// Status pill is a *display*, not a toggle:
//   - completed → `--gold-bg` / `--gold-soft` border / `--gold` text + solid dot
//   - running   → blue tint + `--info` text + pulsing dot
//   - other terminal states (failed/cancelled/queued) reuse the neutral/warn/
//     danger tones so the bar still communicates the run's real status.
//
// Explicitly REMOVED vs the prior chrome: the POST-HOC⇄EVAL toggle, the ⌘K
// command button, and the duplicate "Run … · scenario …" middle section. The
// run id now lives in exactly two places — this breadcrumb and the body H1.

import { Link } from "react-router-dom";
import { BrandMark } from "./BrandMark";

type PillStyle = {
  label: string;
  bg: string;
  bd: string;
  fg: string;
  dot: string;
  pulse: boolean;
};

function pillFor(status: string): PillStyle {
  switch (status) {
    case "running":
    case "queued":
      return {
        label: `EVAL ${status.toUpperCase()}`,
        bg: "rgba(95,168,255,0.12)",
        bd: "rgba(95,168,255,0.40)",
        fg: "var(--info)",
        dot: "var(--info)",
        pulse: true,
      };
    case "failed":
      return {
        label: "EVAL FAILED",
        bg: "rgba(255,77,77,0.10)",
        bd: "var(--danger)",
        fg: "var(--danger)",
        dot: "var(--danger)",
        pulse: false,
      };
    case "cancelled":
      return {
        label: "EVAL CANCELLED",
        bg: "rgba(255,176,32,0.10)",
        bd: "rgba(255,176,32,0.45)",
        fg: "var(--warn)",
        dot: "var(--warn)",
        pulse: false,
      };
    case "completed":
      return {
        label: "EVAL COMPLETED",
        bg: "var(--gold-bg)",
        bd: "var(--gold-soft)",
        fg: "var(--gold)",
        dot: "var(--gold)",
        pulse: false,
      };
    default:
      return {
        label: status ? `EVAL ${status.toUpperCase()}` : "EVAL UNKNOWN",
        bg: "rgba(153,153,153,0.10)",
        bd: "var(--border)",
        fg: "var(--text-2)",
        dot: "var(--text-3)",
        pulse: false,
      };
  }
}

export function EvalTopBar({ runId, status }: { runId: string; status: string }) {
  const pill = pillFor(status);
  return (
    <div
      data-testid="eval-topbar"
      className="h-12 px-4 flex items-center gap-3 shrink-0"
      style={{ background: "var(--surface-sidebar)", borderBottom: "1px solid var(--border)" }}
    >
      <Link to="/eval-runs" aria-label="XVN home — eval runs">
        <BrandMark />
      </Link>
      <span className="text-text-4 mx-1" aria-hidden>
        /
      </span>
      <Link
        to="/eval-runs"
        className="text-[11px] font-mono tracking-[0.18em] text-text-3 uppercase hover:text-text-2"
      >
        eval runs
      </Link>
      <span className="text-text-4" aria-hidden>
        /
      </span>
      <code
        data-testid="eval-topbar-run-id"
        className="text-[12px] font-mono text-text-2 tracking-normal break-all select-all"
        aria-label={`Eval run id ${runId}`}
      >
        {runId}
      </code>

      <div
        data-testid="eval-topbar-status"
        className="ml-auto flex items-center gap-1.5 px-2.5 py-1"
        style={{ background: pill.bg, border: `1px solid ${pill.bd}`, borderRadius: 4 }}
      >
        <span
          className={`w-1.5 h-1.5 rounded-full ${pill.pulse ? "animate-pulse" : ""}`}
          style={{ background: pill.dot }}
        />
        <span
          className="text-[10px] font-mono tracking-[0.16em] uppercase"
          style={{ color: pill.fg }}
        >
          {pill.label}
        </span>
      </div>
    </div>
  );
}
