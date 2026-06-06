// Floating capsule — single component. Collapsed = focused eval only + others-toggle.
// Expanded = capsule grows upward, one row per concurrent eval, each row formatted like the focused row.
// Click any non-focused row → swaps focus.

const { useEffect: stE, useState: stS, useRef: stR } = React;

// Status palette per eval lifecycle stage.
const ST = {
  eval:   { tint: "var(--info)",   label: "RUNNING",   pulse: true  },
  pass:   { tint: "var(--gold)",   label: "COMPLETED", pulse: false },
  warn:   { tint: "var(--warn)",   label: "WARN",      pulse: false },
  error:  { tint: "var(--danger)", label: "ERROR",     pulse: true  },
  queued: { tint: "var(--text-3)", label: "QUEUED",    pulse: false },
};

// ── One row in the capsule — used for the focused eval AND for every sibling when expanded. ──
// Same shape, same fields; the focused one gets a subtle background + left rule, click-disabled.
function EvalLine({ run, focused, onClick, currentSpan, isLive }) {
  const c = ST[run.status] || ST.eval;
  return (
    <button
      onClick={onClick}
      disabled={focused}
      className="h-9 w-full flex items-center gap-3 px-3 text-left transition-colors"
      style={{
        background: focused ? "rgba(0, 230, 118, 0.06)" : "transparent",
        borderLeft: `2px solid ${focused ? "var(--gold)" : "transparent"}`,
        cursor: focused ? "default" : "pointer",
      }}
      onMouseEnter={(e) => { if (!focused) e.currentTarget.style.background = "var(--surface-hover)"; }}
      onMouseLeave={(e) => { if (!focused) e.currentTarget.style.background = "transparent"; }}
    >
      {/* status dot */}
      <span
        className={`inline-block w-1.5 h-1.5 rounded-full shrink-0 ${c.pulse ? "animate-pulse" : ""}`}
        style={{ background: c.tint, boxShadow: `0 0 0 3px ${c.tint}22` }}
      ></span>

      {/* EVAL · name · status */}
      <span className="flex items-center gap-2 shrink-0">
        <span className="text-[10px] font-mono tracking-[0.18em] text-text-3">EVAL</span>
        <span className="text-[11px] font-mono" style={{ color: c.tint }}>{run.short}</span>
        <span className="text-[10px] font-mono tracking-[0.18em] text-text-3">· {c.label}</span>
      </span>

      <span className="w-px h-4 shrink-0" style={{ background: "var(--border)" }}></span>

      {/* spans · elapsed · cost */}
      <span className="text-text font-mono text-[11px] shrink-0">
        <span className="text-text-3">spans </span><span className="tabular-nums">{run.spans}</span>
        <span className="text-text-4 mx-2">·</span>
        <span className="tabular-nums">{run.elapsed}</span>
        <span className="text-text-4 mx-2">·</span>
        <span className="tabular-nums">{run.cost}</span>
      </span>

      {/* current span (focused only — others would need their own trace stream) */}
      {focused && currentSpan && (
        <>
          <span className="w-px h-4 shrink-0" style={{ background: "var(--border)" }}></span>
          <span className="flex items-center gap-1.5 min-w-0 max-w-[200px]">
            {isLive
              ? <span className="w-1.5 h-1.5 rounded-full animate-pulse shrink-0" style={{ background: currentSpan.color, boxShadow: `0 0 0 3px ${currentSpan.color}22` }}></span>
              : <span className="inline-block w-[3px] h-3 shrink-0" style={{ background: currentSpan.color }}></span>
            }
            <span className="text-[10px] font-mono tracking-[0.16em] shrink-0" style={{ color: currentSpan.color }}>{currentSpan.label}</span>
            <span className="text-[11px] font-mono text-text truncate">{currentSpan.name}</span>
            {currentSpan.elapsed != null && (
              <span className="text-[10px] font-mono tabular-nums text-text-3 shrink-0">{currentSpan.elapsed}</span>
            )}
          </span>
        </>
      )}

      {/* non-focused row gets a hint at the right edge */}
      {!focused && (
        <span className="ml-auto text-[9px] font-mono tracking-[0.18em] text-text-4 shrink-0">SWITCH →</span>
      )}
    </button>
  );
}

window.Strip = function Strip({
  state, liveDuration, isLive, currentSpan, onExpand, onPopOut,
  focusedShort = "mr·flash",
  siblings = [],
  onSwitchEval,
}) {
  const focusedSt = ST[
    state === "green" ? "pass" :
    state === "blue"  ? "eval" :
    state === "amber" ? "warn" :
    state === "red"   ? "error" : "eval"
  ];

  // The focused eval is synthesized as its own "run" so it shares the EvalLine layout.
  const focusedRun = {
    id: "focused", short: focusedShort, status:
      state === "green" ? "pass" :
      state === "blue"  ? "eval" :
      state === "amber" ? "warn" :
      state === "red"   ? "error" : "eval",
    spans: 47,
    elapsed: isLive ? `0:${String(liveDuration).padStart(2,"0")}` : "3.4s",
    cost: "$0.18",
  };

  // Sort: errored siblings first (always promoted), then the rest in given order.
  const errored = siblings.filter(s => s.status === "error");
  const others  = siblings.filter(s => s.status !== "error");
  const ordered = [...errored, ...others];

  const [expanded, setExpanded] = stS(false);

  // If a sibling has an error and the capsule is collapsed, draw attention via the toggle pill border.
  const anyError = ordered.some(s => s.status === "error") || state === "red";
  const anyWarn  = ordered.some(s => s.status === "warn")  || state === "amber";
  const borderColor = anyError ? "var(--danger)" : anyWarn ? "var(--warn)" : "var(--border-strong)";

  // Auto-pop the capsule open when an error appears (assertive but appropriate for trading workflows).
  // Once user collapses, respect that until a NEW error fires.
  const lastErrorCount = stR(errored.length);
  stE(() => {
    if (errored.length > lastErrorCount.current) setExpanded(true);
    lastErrorCount.current = errored.length;
  }, [errored.length]);

  const hasSiblings = ordered.length > 0;
  const errorCount  = errored.length;

  return (
    <div
      className="fixed left-1/2 -translate-x-1/2 z-40 select-none whitespace-nowrap flex flex-col overflow-hidden"
      style={{
        bottom: 14,
        background: "var(--surface-elev)",
        border: `1px solid ${borderColor}`,
        borderRadius: expanded ? 12 : 999,    // pill when collapsed; rounded card when stacked
        boxShadow: "0 14px 40px rgba(0,0,0,0.55), 0 0 0 1px rgba(0,0,0,0.4)",
        backdropFilter: "blur(8px)",
        maxWidth: "calc(100vw - 32px)",
        minWidth: 520,
        transition: "border-radius 180ms ease",
      }}
    >
      {/* ── Header row (always the focused eval) ── */}
      <div className="flex items-stretch" style={{ borderBottom: expanded ? "1px solid var(--border)" : "none" }}>
        <EvalLine run={focusedRun} focused={true} currentSpan={currentSpan} isLive={isLive} onClick={() => {}}/>

        {/* trailing controls */}
        <div className="flex items-center gap-0.5 pr-1 shrink-0">
          {hasSiblings && (
            <button
              onClick={(e) => { e.stopPropagation(); setExpanded(v => !v); }}
              title={expanded ? "Collapse" : `Show ${ordered.length} other eval${ordered.length === 1 ? "" : "s"}`}
              className="h-7 px-2 mx-0.5 flex items-center gap-1.5 rounded-full text-[10px] font-mono tracking-[0.16em]"
              style={{
                background: anyError ? "rgba(255, 77, 77, 0.10)" : expanded ? "var(--gold-bg)" : "var(--surface-card)",
                border: `1px solid ${anyError ? "var(--danger)" : expanded ? "var(--gold-soft)" : "var(--border-strong)"}`,
                color: anyError ? "var(--danger)" : expanded ? "var(--gold)" : "var(--text-2)",
              }}
            >
              <span>+{ordered.length}</span>
              <span className="text-text-4">·</span>
              <span>{ordered.length === 1 ? "OTHER" : "OTHERS"}</span>
              {errorCount > 0 && !expanded && (
                <>
                  <span className="text-text-4">·</span>
                  <span style={{ color: "var(--danger)" }}>{errorCount} ERR</span>
                </>
              )}
            </button>
          )}

          <button
            onClick={(e) => { e.stopPropagation(); onExpand(); }}
            title="Expand trace dock (F12)"
            className="h-7 w-7 flex items-center justify-center text-text-3 hover:text-gold rounded-full">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M3 10l5-5 5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
          </button>
          <button
            onClick={(e) => { e.stopPropagation(); onPopOut(); }}
            title="Open in dedicated route"
            className="h-7 w-7 flex items-center justify-center text-text-3 hover:text-text rounded-full">
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M6 3h7v7M13 3l-7 7M3 8v5h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>
          </button>
        </div>
      </div>

      {/* ── Expanded body — one row per sibling, formatted like the focused row ── */}
      {expanded && hasSiblings && (
        <div className="flex flex-col">
          {ordered.map((r, i) => (
            <div key={r.id} style={{ borderTop: i === 0 ? "none" : "1px solid var(--border-soft)" }}>
              <EvalLine
                run={r}
                focused={false}
                onClick={() => onSwitchEval(r)}
                isLive={false}
              />
            </div>
          ))}

          {/* Footer hint */}
          <div className="px-3 py-1.5 text-[9px] font-mono tracking-[0.18em] text-text-4 flex items-center justify-between"
            style={{ borderTop: "1px solid var(--border-soft)", background: "var(--surface-card)" }}>
            <span>{ordered.length} EVAL{ordered.length === 1 ? "" : "S"} RUNNING ON CLUSTER</span>
            <span>CLICK ROW → SWITCH FOCUS</span>
          </div>
        </div>
      )}
    </div>
  );
};
