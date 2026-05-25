import type { CSSProperties } from "react";

// XVN · BRKT lockup — the canonical brand mark.
//
// Square brackets enclosing the XVN wordmark. Approved logo direction v1.0
// (see "XVN Logo Handoff.html", rev 2026.05.25 — the bracketed wordmark chosen
// from the variations exploration). Reads as code / array / citation and sits
// cleanly in dense operator chrome.
//
// Geometry is locked to a 24:7 viewBox (48×14). Pass `height` only — the width
// derives from the ratio so the lockup can never be stretched or reflowed.
// Brackets default to the Signal token (`--gold`) and the wordmark to `--text`;
// both flip automatically between dark/light themes. Override `brackets` /
// `wordmark` for the handoff's all-green, all-white, or light-surface variants.
//
// Approved scale ladder (handoff §02): 14 favicon · 20 app default
// (sidebars / headers / login) · 32 display · 64 presentation. Interpolate
// between these — do not redraw.

type BrandMarkProps = {
  /** Target height in px; width derives from the locked 24:7 ratio. */
  height?: number;
  /** Bracket stroke color. Defaults to the Signal token `--gold`. */
  brackets?: string;
  /** Wordmark fill. Defaults to `--text` so it reads correctly on any theme. */
  wordmark?: string;
  className?: string;
  style?: CSSProperties;
  /** Accessible label. */
  title?: string;
};

export function BrandMark({
  height = 14,
  brackets = "var(--gold)",
  wordmark = "var(--text)",
  className,
  style,
  title = "XVN",
}: BrandMarkProps = {}) {
  const width = (48 / 14) * height;
  return (
    <svg
      width={width}
      height={height}
      viewBox="0 0 48 14"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label={title}
      className={className}
      style={{ display: "block", overflow: "visible", ...style }}
    >
      <title>{title}</title>
      <g
        stroke={brackets}
        strokeWidth={1.4}
        fill="none"
        strokeLinecap="square"
      >
        <path d="M4 1 H1 V13 H4" />
        <path d="M44 1 H47 V13 H44" />
      </g>
      <text
        x={24}
        y={7}
        fill={wordmark}
        fontFamily="'Geist Mono', ui-monospace, SFMono-Regular, Menlo, monospace"
        fontSize={13}
        fontWeight={700}
        letterSpacing="0.14em"
        dominantBaseline="central"
        textAnchor="middle"
      >
        XVN
      </text>
    </svg>
  );
}
