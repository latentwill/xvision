// frontend/web/src/features/agent-runs/PullQuote.tsx
import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";

/** Clamp to this many lines before the "show more" affordance appears. */
const CLAMP_LINES = 4;

// Collapsed clamp: a `-webkit-box` line-clamp keeps the body to CLAMP_LINES
// visual lines (works alongside `whitespace-pre-wrap`, counting both hard
// newlines and wrapped lines). Inline-styled (not a Tailwind utility) so the
// clamp is bulletproof regardless of the Tailwind line-clamp config.
const CLAMP_STYLE: CSSProperties = {
  display: "-webkit-box",
  WebkitLineClamp: CLAMP_LINES,
  WebkitBoxOrient: "vertical",
  overflow: "hidden",
};

/**
 * Renders a long body (prompt / response) clamped to {@link CLAMP_LINES} lines
 * with a "show more" toggle that expands into a scrollable box. The toggle only
 * appears when the content actually overflows the clamp.
 *
 * `whitespace-pre-wrap` preserves the model's newlines; `[overflow-wrap:anywhere]`
 * forces a long UNBROKEN line (a hash, base64 blob, URL) to wrap so it can never
 * push out of the box.
 */
function ClampedBody({ children }: { children: ReactNode }) {
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Measure against the COLLAPSED clamp (the effect runs on the initial,
  // unexpanded render): if the full content is taller than the clamped box,
  // the body is truncated and the toggle is warranted. Re-checked when the
  // body text changes.
  useLayoutEffect(() => {
    const el = ref.current;
    if (el && !expanded) {
      setOverflows(el.scrollHeight > el.clientHeight + 1);
    }
  }, [children, expanded]);

  return (
    <>
      <div
        ref={ref}
        data-testid="pullquote-body"
        data-expanded={expanded}
        className={`whitespace-pre-wrap [overflow-wrap:anywhere] ${
          expanded ? "max-h-[40vh] overflow-y-auto" : ""
        }`}
        style={expanded ? undefined : CLAMP_STYLE}
      >
        {children}
      </div>
      {overflows ? (
        <button
          type="button"
          onClick={() => setExpanded((e) => !e)}
          aria-expanded={expanded}
          data-testid="pullquote-toggle"
          className="mt-1 text-[9px] font-mono tracking-[0.16em] text-text-3 hover:text-text"
        >
          {expanded ? "▴ SHOW LESS" : "▾ SHOW MORE"}
        </button>
      ) : null}
    </>
  );
}

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
          className="absolute -top-1 left-1 text-[22px] leading-none font-sans font-semibold select-none"
          style={{ color: accent, opacity: 0.45 }}
          aria-hidden
        >
          {glyph}
        </span>
        <div className={`pl-2 text-[12px] leading-relaxed ${italic ? "font-sans font-semibold" : "font-mono"}`} style={{ color: "var(--text)" }}>
          {streaming ? (
            // Live tail: never clamp a streaming body (the operator wants the
            // latest tokens). Still wrap long unbroken lines so they stay boxed.
            <span className="whitespace-pre-wrap [overflow-wrap:anywhere]">{body}</span>
          ) : (
            <ClampedBody>{body}</ClampedBody>
          )}
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
