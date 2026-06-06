// Bottom dock — Layer 2 — XVN folio-dark, with compact Logfire-style filter bar.
const { useState: useDS, useEffect: useDE, useMemo: useDM } = React;

const KIND_DEF = [
  { k: "agent",      label: "AGENT", color: "#a39a85" },
  { k: "model",      label: "MODEL", color: "#7dd3fc" },
  { k: "tool",       label: "TOOL",  color: "#6ee7b7" },
  { k: "supervisor", label: "SUPER", color: "#00e676" },
  { k: "artifact",   label: "ARTIF", color: "#a78bfa" },
];

// Decision jump — type a number + Enter, or step prev/next. Designed for thousands of decisions.
function DecisionJump({ value, onChange, decisions }) {
  const [draft, setDraft] = useDS("");
  const ids = decisions.map(d => d.i);
  const active = value !== "all";
  const curIdx = active ? ids.indexOf(parseInt(value, 10)) : -1;

  // keep draft in sync when external value changes
  useDE(() => { setDraft(active ? String(value) : ""); }, [value, active]);

  const commit = (raw) => {
    const n = parseInt(String(raw).replace(/[^0-9]/g, ""), 10);
    if (!Number.isFinite(n)) return;
    if (ids.includes(n)) onChange(String(n));
    else {
      // snap to nearest
      const nearest = ids.reduce((a, b) => Math.abs(b - n) < Math.abs(a - n) ? b : a, ids[0]);
      onChange(String(nearest));
    }
  };

  const step = (delta) => {
    if (curIdx === -1) { onChange(String(ids[0])); return; }
    const next = Math.min(ids.length - 1, Math.max(0, curIdx + delta));
    onChange(String(ids[next]));
  };

  return (
    <div className="flex items-center gap-1 h-6 rounded-sm2 pl-1.5 pr-0.5"
      style={{ background: "var(--bg)", border: `1px solid ${active ? "var(--gold-soft)" : "var(--border)"}` }}>
      <span className="text-[10px] font-mono tracking-[0.16em] whitespace-nowrap"
        style={{ color: active ? "var(--gold-soft)" : "var(--text-4)" }}>DECISION&nbsp;#</span>
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value.replace(/[^0-9]/g, ""))}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit(draft);
          else if (e.key === "ArrowUp")   { e.preventDefault(); step(+1); }
          else if (e.key === "ArrowDown") { e.preventDefault(); step(-1); }
          else if (e.key === "Escape" && active) onChange("all");
        }}
        onBlur={() => { if (draft) commit(draft); }}
        placeholder="—"
        className="w-9 h-full bg-transparent text-[11px] font-mono tabular-nums outline-none"
        style={{ color: active ? "var(--gold)" : "var(--text)" }}
      />
      <button onClick={() => step(-1)} title="Prev decision"
        className="h-full w-5 flex items-center justify-center text-text-3 hover:text-text">
        <svg width="9" height="9" viewBox="0 0 16 16" fill="none"><path d="M10 3l-5 5 5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
      </button>
      <button onClick={() => step(+1)} title="Next decision"
        className="h-full w-5 flex items-center justify-center text-text-3 hover:text-text">
        <svg width="9" height="9" viewBox="0 0 16 16" fill="none"><path d="M6 3l5 5-5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
      </button>
      <span className="text-[10px] font-mono text-text-4 px-1 tabular-nums whitespace-nowrap leading-none">
        {active ? `${curIdx + 1}/${ids.length}` : `of ${ids.length}`}
      </span>
      {active && (
        <button onClick={() => onChange("all")} title="Clear decision filter"
          className="h-full w-5 flex items-center justify-center text-text-3 hover:text-danger text-[12px] leading-none">×</button>
      )}
    </div>
  );
}

const STATUS_DEF = [
  { k: "green", glyph: "✓",  tint: "var(--gold)",   bg: "var(--gold-bg)",        bd: "var(--gold-soft)" },
  { k: "blue",  glyph: "▶",  tint: "var(--info)",   bg: "rgba(111,143,184,0.14)", bd: "rgba(111,143,184,0.45)" },
  { k: "amber", glyph: "⚠",  tint: "var(--warn)",   bg: "rgba(255, 176, 32, 0.10)",  bd: "rgba(255, 176, 32, 0.45)" },
  { k: "red",   glyph: "✕",  tint: "var(--danger)", bg: "rgba(255, 77, 77, 0.10)",   bd: "rgba(255, 77, 77, 0.45)" },
];

function FilterBar({ query, setQuery, kinds, toggleKind, status, setStatus, decisionFilter, setDecisionFilter, decisions, total, filtered }) {
  return (
    <div className="h-9 px-2 flex items-center gap-2 shrink-0 overflow-hidden" style={{ borderBottom: "1px solid var(--border)", background: "var(--surface-elev)" }}>
      {/* search */}
      <div className="flex items-center gap-1.5 h-6 px-2 rounded-sm2 flex-1 min-w-[200px] max-w-[380px]"
        style={{ background: "var(--bg)", border: "1px solid var(--border)" }}>
        <svg width="10" height="10" viewBox="0 0 16 16" fill="none" className="text-text-3"><circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4"/><path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>
        <input
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder='filter   title:agent.plan   model:gpt-5   tool:run_backtest'
          className="flex-1 bg-transparent text-[11px] font-mono text-text outline-none placeholder:text-text-4 min-w-0"
        />
        {query && (
          <button onClick={() => setQuery("")} className="text-text-3 hover:text-text text-[10px] font-mono">×</button>
        )}
      </div>

      <div className="flex items-center gap-0.5 shrink-0">
        {KIND_DEF.map(k => {
          const on = kinds.has(k.k);
          return (
            <button key={k.k} onClick={() => toggleKind(k.k)}
              className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] rounded-sm2 flex items-center gap-1"
              style={{
                background: on ? "var(--surface-card)" : "transparent",
                border: `1px solid ${on ? k.color : "var(--border)"}`,
                color: on ? k.color : "var(--text-3)",
              }}>
              <span className="w-1.5 h-1.5 inline-block" style={{ background: k.color, opacity: on ? 1 : 0.5 }}></span>
              {k.label}
            </button>
          );
        })}
      </div>

      <div className="w-px h-4 shrink-0" style={{ background: "var(--border)" }}></div>

      <div className="flex items-center gap-0.5 shrink-0">
        {STATUS_DEF.map(s => {
          const on = status === s.k;
          return (
            <button key={s.k} onClick={() => setStatus(s.k)} title={s.k.toUpperCase()}
              className="h-6 w-6 text-[10px] font-mono rounded-sm2 flex items-center justify-center"
              style={{
                background: on ? s.bg : "transparent",
                border: `1px solid ${on ? s.bd : "var(--border)"}`,
                color: on ? s.tint : "var(--text-3)",
              }}>
              {s.glyph}
            </button>
          );
        })}
      </div>

      <div className="w-px h-4 shrink-0" style={{ background: "var(--border)" }}></div>

      <DecisionJump value={decisionFilter} onChange={setDecisionFilter} decisions={decisions}/>

      <div className="ml-auto text-[10px] font-mono text-text-3 tabular-nums pr-1 shrink-0 whitespace-nowrap">
        <span className="text-text">{filtered}</span><span className="text-text-4">/</span><span>{total}</span> spans
      </div>
    </div>
  );
}

window.Dock = function Dock({
  height, onHeight, onMinimize, onPopOut,
  isLive, liveDuration, spans, selected, onSelect,
  autoScroll, setAutoScroll, onToast, decisions,
  query, setQuery, kinds, toggleKind, status, setStatus, decisionFilter, setDecisionFilter,
  filteredSpans,
  onHalt,
}) {
  const heights = { peek: 240, working: 480, full: "80vh" };
  const dur = isLive ? `0:${String(liveDuration).padStart(2,"0")}` : "3.4s";

  const HeightBtn = ({ k, label }) => {
    const active = height === k;
    return (
      <button onClick={() => onHeight(k)}
        className="px-1.5 h-5 text-[10px] font-mono tracking-wider rounded-sm2"
        style={{
          background: active ? "var(--gold-bg)" : "transparent",
          border: `1px solid ${active ? "var(--gold-soft)" : "var(--border)"}`,
          color: active ? "var(--gold)" : "var(--text-3)",
        }}>
        {label}
      </button>
    );
  };

  return (
    <div className="flex flex-col shrink-0" style={{ height: heights[height], background: "var(--surface-card)", borderTop: "1px solid var(--border)" }}>
      {/* Dock header */}
      <div className="h-8 px-2 flex items-center gap-3 shrink-0" style={{ borderBottom: "1px solid var(--border)" }}>
        <div className="text-[10px] font-mono tracking-[0.18em] text-text-3">TRACE</div>
        <div className="w-px h-4" style={{ background: "var(--border)" }}></div>
        <div className="density-glyph text-[11px] leading-none tracking-tight">
          <span style={{ color: "rgba(0, 230, 118, 0.95)" }}>▓</span>
          <span style={{ color: "rgba(0, 230, 118, 0.70)" }}>▓</span>
          <span style={{ color: "rgba(0, 230, 118, 0.55)" }}>▓</span>
          <span style={{ color: "rgba(0, 230, 118, 0.40)" }}>▒</span>
          <span style={{ color: "rgba(0, 230, 118, 0.28)" }}>▒</span>
          <span style={{ color: "rgba(0, 230, 118, 0.18)" }}>░</span>
          <span style={{ color: "var(--text-4)" }}>░</span>
        </div>
        <div className="text-[11px] font-mono text-text">
          <span className="text-text-3">spans </span><span className="tabular-nums">47</span>
          <span className="text-text-4 mx-2">·</span>
          <span className="text-text-3">model </span><span className="tabular-nums">12</span>
          <span className="text-text-4 mx-2">·</span>
          <span className="tabular-nums">{dur}</span>
          <span className="text-text-4 mx-2">·</span>
          <span className="tabular-nums">$0.18</span>
        </div>

        {isLive && (
          <label className="ml-3 flex items-center gap-1.5 text-[10px] font-mono text-text-2 cursor-pointer select-none">
            <input type="checkbox" checked={autoScroll} onChange={(e) => setAutoScroll(e.target.checked)}
              className="w-3 h-3" style={{ accentColor: "var(--gold)" }}/>
            <span>{autoScroll ? "follow live" : "lock scroll"}</span>
          </label>
        )}

        <div className="ml-auto flex items-center gap-1">
          <div className="flex items-center gap-0.5 mr-2">
            <HeightBtn k="peek"    label="PEEK"    />
            <HeightBtn k="working" label="WORKING" />
            <HeightBtn k="full"    label="FULL"    />
          </div>

          {isLive && (
            <button
              onClick={() => onHalt()}
              className="h-6 px-2 text-[10px] font-mono tracking-[0.18em] rounded-sm2 transition-colors"
              style={{
                color: "var(--danger)",
                background: "rgba(255, 77, 77, 0.10)",
                border: "1px solid rgba(255, 77, 77, 0.55)",
                fontWeight: 600,
              }}
              onMouseEnter={(e) => { e.currentTarget.style.background = "var(--danger)"; e.currentTarget.style.color = "#0f0e0c"; }}
              onMouseLeave={(e) => { e.currentTarget.style.background = "rgba(255, 77, 77, 0.10)"; e.currentTarget.style.color = "var(--danger)"; }}>
              ◼ HALT STRATEGY
            </button>
          )}

          <button onClick={onPopOut} title="Open dedicated route"
            className="h-6 w-6 flex items-center justify-center text-text-3 rounded-sm2"
            onMouseEnter={(e) => { e.currentTarget.style.background = "var(--surface-hover)"; e.currentTarget.style.color = "var(--text)"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-3)"; }}>
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M6 3h7v7M13 3l-7 7M3 8v5h5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/></svg>
          </button>
          <button onClick={onMinimize} title="Minimize to strip (F12)"
            className="h-6 w-6 flex items-center justify-center text-text-3 rounded-sm2"
            onMouseEnter={(e) => { e.currentTarget.style.background = "var(--surface-hover)"; e.currentTarget.style.color = "var(--text)"; }}
            onMouseLeave={(e) => { e.currentTarget.style.background = "transparent"; e.currentTarget.style.color = "var(--text-3)"; }}>
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M3 11h10" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
          </button>
        </div>
      </div>

      {/* Filter bar — Logfire style, compact */}
      <FilterBar
        query={query} setQuery={setQuery}
        kinds={kinds} toggleKind={toggleKind}
        status={status} setStatus={setStatus}
        decisionFilter={decisionFilter} setDecisionFilter={setDecisionFilter}
        decisions={decisions}
        total={spans.length} filtered={filteredSpans.length}
      />

      {/* Body */}
      {height === "peek" ? (
        <div className="flex-1 min-h-0 flex">
          <FlameGraph spans={filteredSpans} selected={selected} onSelect={onSelect}
            autoScroll={autoScroll} isLive={isLive} currentTime={liveDuration * 70 + 200} />
        </div>
      ) : (
        <div className="flex-1 min-h-0 flex">
          <FlameGraph spans={filteredSpans} selected={selected} onSelect={onSelect}
            autoScroll={autoScroll} isLive={isLive} currentTime={liveDuration * 70 + 200} />
          <Inspector span={spans.find(s => s.id === selected)} isLive={isLive} onToast={onToast} decisions={decisions} />
        </div>
      )}
    </div>
  );
};
