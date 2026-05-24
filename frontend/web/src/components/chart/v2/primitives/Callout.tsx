/**
 * Callout — one AI-annotation card rendered above the candle pane.
 * ~210px wide. Gold border by default; switches to red when
 * `danger: true`.
 *
 * Inner layout: eyebrow (.callout-head = type · confidence) + Geist
 * title 14px + body 11.5px in text-2 + foot row (idx · N + action).
 *
 * Positioning is owned by the AnnotationOverlay parent; this component
 * is layout-agnostic (absolute parent positions it via inline style).
 */
import type { ReactElement } from "react";

import type { Annotation } from "../types";

export const CALLOUT_WIDTH = 210;

export interface CalloutProps {
  annotation: Annotation;
}

function fmtConfPct(conf: number): string {
  return `${Math.round(conf * 100)}%`;
}

export function Callout({ annotation: a }: CalloutProps): ReactElement {
  const isDanger = a.danger === true;
  const border = isDanger
    ? "rgba(200,68,58,0.32)"
    : "rgba(212,165,71,0.32)";
  const accent = isDanger ? "var(--danger)" : "var(--gold)";

  return (
    <div
      data-testid={`callout-${a.idx}`}
      className="rounded-card bg-surface-card overflow-hidden"
      style={{
        width: CALLOUT_WIDTH,
        border: `1px solid ${border}`,
        boxShadow: "0 2px 6px rgba(0,0,0,0.18)",
      }}
    >
      <header className="px-3 pt-2 pb-1 flex items-center justify-between">
        <span className="caps" style={{ color: accent }}>
          {a.type}
        </span>
        <span
          className="text-[10.5px] text-text-3"
          style={{ fontFamily: 'Geist Mono, ui-monospace, monospace' }}
        >
          conf {fmtConfPct(a.conf)}
        </span>
      </header>
      <div
        className="px-3 text-[14px] leading-tight text-text"
        style={{ fontFamily: 'Geist, sans-serif' }}
      >
        {a.title}
      </div>
      <p className="px-3 mt-1 mb-1 text-[11.5px] leading-snug text-text-2">
        {a.body}
      </p>
      <footer className="px-3 pb-2 flex items-center justify-between text-[10.5px] text-text-3">
        <span style={{ fontFamily: 'Geist Mono, ui-monospace, monospace' }}>
          idx · {a.idx}
        </span>
        <span style={{ color: accent }}>{a.action}</span>
      </footer>
    </div>
  );
}
