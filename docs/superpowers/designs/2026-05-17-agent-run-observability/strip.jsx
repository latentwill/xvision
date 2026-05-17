// Status-line strip — floats at bottom; visible when dock is closed.
const { useEffect, useState, useRef } = React;

window.Strip = function Strip({ state, liveDuration, isLive, currentSpan, onExpand, onPopOut }) {
  const conf = {
    green: { dot: "var(--gold)",   label: "COMPLETED", pulse: false, glow: "0 0 0 3px var(--gold-bg)" },
    blue:  { dot: "var(--info)",   label: "LIVE",      pulse: true,  glow: "0 0 0 3px rgba(111,143,184,0.25)" },
    amber: { dot: "var(--warn)",   label: "WARNINGS",  pulse: false, glow: "0 0 0 3px rgba(219,146,48,0.20)" },
    red:   { dot: "var(--danger)", label: "ERROR",     pulse: false, glow: "0 0 0 3px rgba(200,68,58,0.25)" },
  }[state];

  const dur = isLive
    ? `0:${String(liveDuration).padStart(2,"0")}`
    : "3.4s";

  return (
    <div
      className="fixed left-1/2 -translate-x-1/2 z-40 h-8 flex items-center gap-3 px-3 select-none cursor-pointer whitespace-nowrap"
      style={{
        bottom: 14,
        background: "var(--surface-elev)",
        border: "1px solid var(--border-strong)",
        borderRadius: 999,
        boxShadow: "0 14px 40px rgba(0,0,0,0.55), 0 0 0 1px rgba(0,0,0,0.4)",
        backdropFilter: "blur(8px)",
        maxWidth: "calc(100vw - 32px)",
      }}
      onClick={onExpand}
      title="Click to expand the trace dock (F12)"
    >
      <div className="flex items-center gap-2 shrink-0 pl-1">
        <span
          className={`inline-block w-1.5 h-1.5 rounded-full ${conf.pulse ? "animate-pulse" : ""}`}
          style={{ background: conf.dot, boxShadow: conf.glow }}
        ></span>
        <span className="text-text-3 tracking-[0.18em] text-[10px] font-mono">{conf.label}</span>
      </div>

      <div className="w-px h-3.5" style={{ background: "var(--border)" }}></div>

      <div className="density-glyph text-[11px] leading-none tracking-tight">
        <span style={{ color: "rgba(212,165,71,0.95)" }}>▓</span>
        <span style={{ color: "rgba(212,165,71,0.70)" }}>▓</span>
        <span style={{ color: "rgba(212,165,71,0.55)" }}>▓</span>
        <span style={{ color: "rgba(212,165,71,0.40)" }}>▒</span>
        <span style={{ color: "rgba(212,165,71,0.28)" }}>▒</span>
        <span style={{ color: "rgba(212,165,71,0.18)" }}>░</span>
        <span style={{ color: "var(--text-4)" }}>░</span>
      </div>

      <div className="text-text font-mono text-[11px]">
        <span className="text-text-3">spans </span><span className="tabular-nums">47</span>
        <span className="text-text-4 mx-2">·</span>
        <span className="text-text-3">model </span><span className="tabular-nums">12</span>
        <span className="text-text-4 mx-2">·</span>
        <span className="tabular-nums">{dur}</span>
        <span className="text-text-4 mx-2">·</span>
        <span className="tabular-nums">$0.18</span>
      </div>

      {currentSpan && (
        <>
          <div className="w-px h-3.5" style={{ background: "var(--border)" }}></div>
          <div className="flex items-center gap-1.5 max-w-[260px] min-w-0">
            {isLive
              ? <span className="w-1.5 h-1.5 rounded-full animate-pulse shrink-0" style={{ background: currentSpan.color, boxShadow: `0 0 0 3px ${currentSpan.color}22` }}></span>
              : <span className="inline-block w-[3px] h-3 shrink-0" style={{ background: currentSpan.color }}></span>
            }
            <span className="text-[10px] font-mono tracking-[0.16em] shrink-0" style={{ color: currentSpan.color }}>{currentSpan.label}</span>
            <span className="text-[11px] font-mono text-text truncate">{currentSpan.name}</span>
            {currentSpan.elapsed != null && (
              <span className="text-[10px] font-mono tabular-nums text-text-3 shrink-0">{currentSpan.elapsed}</span>
            )}
          </div>
        </>
      )}

      {state === "red" && (
        <div className="flex items-center gap-1.5 px-1.5 py-0.5 rounded-full" style={{ background: "rgba(200,68,58,0.14)", border: "1px solid rgba(200,68,58,0.45)" }}>
          <span className="w-1.5 h-1.5 rounded-full" style={{ background: "var(--danger)" }}></span>
          <span className="text-danger text-[10px] tracking-wide font-mono">1 error</span>
        </div>
      )}
      {state === "amber" && (
        <div className="flex items-center gap-1.5 px-1.5 py-0.5 rounded-full" style={{ background: "rgba(219,146,48,0.10)", border: "1px solid rgba(219,146,48,0.40)" }}>
          <span className="text-warn text-[10px] tracking-wide font-mono">2 warnings</span>
        </div>
      )}

      <div className="w-px h-3.5" style={{ background: "var(--border)" }}></div>

      <div className="flex items-center gap-0.5 pr-1">
        <button
          onClick={(e) => { e.stopPropagation(); onExpand(); }}
          title="Expand trace dock (F12)"
          className="h-6 w-7 flex items-center justify-center text-text-3 hover:text-gold rounded-full">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M3 10l5-5 5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
        </button>
        <button
          onClick={(e) => { e.stopPropagation(); onPopOut(); }}
          title="Open in dedicated route"
          className="h-6 w-7 flex items-center justify-center text-text-3 hover:text-text rounded-full">
          <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M6 3h7v7M13 3l-7 7M3 8v5h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>
        </button>
      </div>
    </div>
  );
};
