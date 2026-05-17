// frontend/web/src/features/agent-runs/PullQuote.tsx
import type { ReactNode } from "react";

export function PullQuote({
  label,
  body,
  accent = "var(--gold)",
  glyph = "“",
  italic = false,
  streaming = false,
}: {
  label: string;
  body: ReactNode;
  accent?: string;
  glyph?: string;
  italic?: boolean;
  streaming?: boolean;
}) {
  return (
    <div className="mt-3 first:mt-0">
      <div className="flex items-center justify-between mb-1">
        <span className="text-[9px] font-mono tracking-[0.18em] text-text-3">{label}</span>
        {streaming ? (
          <span className="text-[9px] font-mono tracking-[0.16em] animate-pulse" style={{ color: "var(--info)" }}>
            {"●"} STREAMING
          </span>
        ) : null}
      </div>
      <div
        className="relative pl-3 pr-3 py-2"
        style={{ background: "var(--surface-elev)", borderLeft: `2px solid ${accent}`, borderRadius: 4 }}
      >
        <span
          className="absolute -top-1 left-1 text-[22px] leading-none font-serif select-none"
          style={{ color: accent, opacity: 0.45 }}
          aria-hidden
        >
          {glyph}
        </span>
        <div className={`pl-2 text-[12px] leading-relaxed ${italic ? "font-serif italic" : "font-mono"}`} style={{ color: "var(--text)" }}>
          {body}
          {streaming ? (
            <span
              className="inline-block w-1 h-3 align-middle ml-1 animate-pulse"
              style={{ background: "var(--info)" }}
              aria-hidden
            />
          ) : null}
        </div>
      </div>
    </div>
  );
}
