// Signal action chip used in the Decisions table + density-strip legend.
// Filled/loud for BUY (green) and SELL (red); outlined-quiet for HOLD; warn
// outline for CLOSE. README §5 / Task B4 step 3.
//
// Tokens (`--gold`, `--danger`, `--text-2`, `--border-strong`, `--warn`) are
// referenced as CSS vars so the pill inherits the Signal palette. The
// foreground-on-fill colors (`#001A0A`, `#1A0000`) are intentional fixed
// near-black tints chosen for contrast on the green/red fills and are NOT
// theme tokens.

import type { ReactNode } from "react";

export type ActionPillAction = "LONG" | "SELL" | "SHORT" | "HOLD" | "CLOSE";

type Variant = {
  label: string;
  fg: string;
  bg: string;
  bd: string;
  glyph: ReactNode;
};

const UP_ARROW = (
  <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden>
    <path
      d="M6 9.5V2.5M6 2.5L2.5 6M6 2.5L9.5 6"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const DOWN_ARROW = (
  <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden>
    <path
      d="M6 2.5V9.5M6 9.5L2.5 6M6 9.5L9.5 6"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    />
  </svg>
);

const BAR = (
  <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden>
    <path d="M3 6H9" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  </svg>
);

const CROSS = (
  <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden>
    <path
      d="M3.5 3.5L8.5 8.5M8.5 3.5L3.5 8.5"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
    />
  </svg>
);

const VARIANTS: Record<ActionPillAction, Variant> = {
  LONG: { label: "LONG", fg: "#001A0A", bg: "var(--gold)", bd: "var(--gold)", glyph: UP_ARROW },
  SELL: { label: "SELL", fg: "#1A0000", bg: "var(--danger)", bd: "var(--danger)", glyph: DOWN_ARROW },
  SHORT: { label: "SHORT", fg: "#1A0000", bg: "rgba(200,0,0,0.85)", bd: "rgba(200,0,0,0.85)", glyph: DOWN_ARROW },
  HOLD: {
    label: "HOLD",
    fg: "var(--text-2)",
    bg: "transparent",
    bd: "var(--border-strong)",
    glyph: BAR,
  },
  CLOSE: {
    label: "CLOSE",
    fg: "var(--warn)",
    bg: "rgba(255, 176, 32, 0.10)",
    bd: "rgba(255, 176, 32, 0.45)",
    glyph: CROSS,
  },
};

export function ActionPill({ action }: { action: ActionPillAction }) {
  const v = VARIANTS[action];
  return (
    <span
      className="inline-flex items-center gap-1.5 font-mono"
      style={{
        color: v.fg,
        background: v.bg,
        border: `1px solid ${v.bd}`,
        padding: "3px 7px 3px 6px",
        borderRadius: 3,
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.1em",
        lineHeight: 1,
        minWidth: 50,
        justifyContent: "center",
      }}
    >
      {v.glyph}
      <span>{v.label}</span>
    </span>
  );
}
