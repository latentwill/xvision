// Flame graph + Inspector — XVN folio-dark
const { useState: useS, useEffect: useE, useRef: useR, useMemo } = React;

const KIND_COLORS = {
  agent:      { bar: "#a39a85", label: "AGENT" },
  model:      { bar: "#7dd3fc", label: "MODEL" },
  tool:       { bar: "#6ee7b7", label: "TOOL"  },
  supervisor: { bar: "#00e676", label: "SUPER" },
  artifact:   { bar: "#a78bfa", label: "ARTIF" },
};

const hexA = (hex, a) => {
  const h = hex.replace("#",""); const r = parseInt(h.slice(0,2),16), g = parseInt(h.slice(2,4),16), b = parseInt(h.slice(4,6),16);
  return `rgba(${r},${g},${b},${a})`;
};

window.FlameGraph = function FlameGraph({ spans, selected, onSelect, autoScroll, isLive, currentTime }) {
  const total = 3400;
  const containerRef = useR(null);
  const [tip, setTip] = useS(null);

  useE(() => {
    if (!isLive || !autoScroll || !containerRef.current) return;
    containerRef.current.scrollLeft = containerRef.current.scrollWidth;
  }, [isLive, autoScroll, currentTime]);

  const widthPct = (ms) => `${(ms / total) * 100}%`;
  const leftPct  = (ms) => `${(ms / total) * 100}%`;
  const ticks = [0, 500, 1000, 1500, 2000, 2500, 3000];

  return (
    <div className="flex-1 min-w-0 flex flex-col" style={{ background: "var(--bg)", borderRight: "1px solid var(--border)" }}>
      {/* axis */}
      <div className="relative h-6 text-[10px] font-mono text-text-3 shrink-0" style={{ borderBottom: "1px solid var(--border)" }}>
        <div className="absolute inset-y-0 left-[200px] right-0">
          {ticks.map(t => (
            <div key={t} className="absolute top-0 bottom-0" style={{ left: leftPct(t), borderLeft: "1px solid var(--border-soft)" }}>
              <span className="absolute top-1 left-1 tabular-nums">{(t/1000).toFixed(1)}s</span>
            </div>
          ))}
          {isLive && (
            <div className="absolute top-0 bottom-0 w-px"
              style={{ left: leftPct(Math.min(currentTime, total)), background: "var(--info)", boxShadow: `0 0 8px ${hexA("#6f8fb8", 0.7)}` }}></div>
          )}
        </div>
        <div className="absolute left-3 top-1 text-text-4 tracking-[0.18em]">SPAN</div>
      </div>

      <div ref={containerRef} className="flex-1 overflow-auto">
        {spans.map((s, i) => {
          const c = KIND_COLORS[s.kind] || KIND_COLORS.agent;
          const isSel = selected === s.id;
          const inflight = isLive && (s.start + s.dur) > currentTime && s.start < currentTime;
          const future = isLive && s.start > currentTime;
          const effDur = inflight ? Math.max(20, currentTime - s.start) : s.dur;
          return (
            <div key={s.id}
              className="relative h-6 flex items-center cursor-pointer"
              style={{
                borderBottom: "1px solid var(--border-soft)",
                background: isSel ? "var(--gold-bg)" : "transparent",
              }}
              onMouseEnter={(e) => { if (!isSel) e.currentTarget.style.background = "var(--surface-hover)"; setTip({ id: s.id, x: e.clientX, y: e.clientY }); }}
              onMouseLeave={(e) => { if (!isSel) e.currentTarget.style.background = "transparent"; setTip(null); }}
              onMouseMove={(e) => setTip(t => t && t.id === s.id ? { ...t, x: e.clientX, y: e.clientY } : t)}
              onClick={() => onSelect(s.id)}
            >
              {/* name column */}
              <div className="w-[200px] shrink-0 pl-3 pr-2 flex items-center gap-1.5 truncate"
                style={{ borderRight: isSel ? "1px solid var(--gold)" : "1px solid var(--border-soft)" }}>
                <span style={{ paddingLeft: s.depth * 10 }}></span>
                <span className="inline-block w-[3px] h-3" style={{ background: c.bar }}></span>
                <span className={`text-[11px] font-mono truncate ${isSel ? "text-text" : "text-text-2"}`}>{s.name}</span>
              </div>

              {/* bar lane */}
              <div className="relative flex-1 h-full">
                <div
                  className="absolute top-1 bottom-1 flex items-center"
                  style={{
                    left: leftPct(s.start),
                    width: widthPct(effDur),
                    background: future ? hexA(c.bar, 0.10) : hexA(c.bar, 0.55),
                    border: isSel ? `1px solid var(--gold)` : `1px solid ${hexA(c.bar, 0.7)}`,
                    boxShadow: isSel ? `0 0 0 2px var(--gold-bg)` : "none",
                    borderRadius: 2,
                  }}
                >
                  {inflight && <div className="absolute inset-0 animate-pulse" style={{ background: hexA(c.bar, 0.25) }}></div>}
                  <span className="absolute inset-0 px-1.5 flex items-center text-[10px] font-mono tabular-nums truncate" style={{ color: "rgba(15,14,12,0.85)" }}>
                    {effDur > 220 ? `${effDur.toFixed(0)}ms` : ""}
                  </span>
                </div>
              </div>
            </div>
          );
        })}
      </div>

      {tip && (() => {
        const s = spans.find(x => x.id === tip.id);
        if (!s) return null;
        return (
          <div className="pointer-events-none fixed z-50 px-2 py-1.5 rounded-sm2 text-[10px] font-mono text-text"
            style={{ left: tip.x + 12, top: tip.y + 12, background: "var(--surface-elev)", border: "1px solid var(--border-strong)", boxShadow: "0 10px 30px rgba(0,0,0,0.5)" }}>
            <div>{s.name}</div>
            <div className="text-text-3">dur <span className="text-text tabular-nums">{s.dur}ms</span> · cost <span className="text-text tabular-nums">${(s.cost||0).toFixed(3)}</span></div>
          </div>
        );
      })()}
    </div>
  );
};

window.Inspector = function Inspector({ span, isLive, onToast, decisions }) {
  if (!span) {
    return (
      <div className="w-[360px] shrink-0 p-4 text-[11px] font-mono text-text-3">
        Select a span to inspect.
      </div>
    );
  }
  const c = KIND_COLORS[span.kind] || KIND_COLORS.agent;
  const streamingNow = isLive && span.streaming;

  const Row = ({ k, v, mono = true, tone }) => (
    <div className="flex items-baseline gap-3 py-1" style={{ borderBottom: "1px solid var(--border-soft)" }}>
      <div className="w-[100px] shrink-0 text-[10px] uppercase tracking-wider text-text-3">{k}</div>
      <div className={`flex-1 text-[11px] ${mono ? "font-mono tabular-nums" : ""} break-all`}
        style={{ color: tone === "gold" ? "var(--gold)" : "var(--text)" }}>{v}</div>
    </div>
  );

  // First-class pull-quotes for prompt / response / tool args / tool result.
  // These are the things an operator actually wants to read, so they get the visual weight of a quote, not a row.
  const PullQuote = ({ label, body, accent = "var(--gold)", glyph = "“", italic = false, streaming = false }) => (
    <div className="mt-3 first:mt-0">
      <div className="flex items-center justify-between mb-1">
        <span className="text-[9px] font-mono tracking-[0.18em] text-text-3">{label}</span>
        {streaming && (
          <span className="text-[9px] font-mono tracking-[0.16em] animate-pulse"
            style={{ color: "var(--info)" }}>● STREAMING</span>
        )}
      </div>
      <div className="relative pl-3 pr-3 py-2 rounded-sm2"
        style={{ background: "var(--surface-elev)", borderLeft: `2px solid ${accent}` }}>
        <span className="absolute -top-1 left-1 text-[22px] leading-none font-serif select-none" style={{ color: accent, opacity: 0.45 }}>{glyph}</span>
        <div className={`pl-2 text-[12px] leading-relaxed ${italic ? "font-serif" : "font-mono"}`}
          style={{ color: "var(--text)", fontWeight: italic ? 400 : 400 }}>
          {body}{streaming && <span className="inline-block w-1 h-3 align-middle ml-1 animate-pulse" style={{ background: "var(--info)" }}></span>}
        </div>
      </div>
    </div>
  );

  return (
    <div className="w-[400px] shrink-0 flex flex-col" style={{ background: "var(--surface-card)" }}>
      <div className="px-3 py-2 flex items-center gap-2" style={{ borderBottom: "1px solid var(--border)" }}>
        <span className="px-1.5 py-0.5 text-[9px] tracking-[0.16em] font-mono rounded-sm2"
          style={{ color: c.bar, background: hexA(c.bar, 0.08), border: `1px solid ${hexA(c.bar, 0.4)}` }}>{c.label}</span>
        <span className="text-[11px] font-mono text-text truncate">{span.name}</span>
        {streamingNow && (
          <span className="ml-auto px-1.5 py-0.5 text-[9px] tracking-[0.16em] font-mono rounded-sm2 animate-pulse"
            style={{ color: "var(--info)", background: hexA("#6f8fb8", 0.12), border: "1px solid " + hexA("#6f8fb8", 0.5) }}>STREAMING</span>
        )}
      </div>

      <div className="flex-1 overflow-auto px-3 py-3">
        {/* First-class content first */}
        {span.prompt && (
          <PullQuote label="PROMPT" body={span.prompt} accent={c.bar} glyph="›" />
        )}
        {span.response && (
          <PullQuote label="RESPONSE" body={span.response} accent="var(--gold)" glyph="“" italic />
        )}
        {span.response_partial && (
          <PullQuote label="RESPONSE (PARTIAL)" body={span.response_partial} accent="var(--info)" glyph="“" italic streaming />
        )}
        {span.args && (
          <PullQuote label="TOOL ARGS" accent={c.bar} glyph="›"
            body={
              <pre className="m-0 text-[11px] font-mono whitespace-pre-wrap text-text-2">{JSON.stringify(span.args, null, 2)}</pre>
            } />
        )}
        {span.result && (
          <PullQuote label="TOOL RESULT" accent="var(--gold)" glyph="←"
            body={
              <pre className="m-0 text-[11px] font-mono whitespace-pre-wrap text-text">{JSON.stringify(span.result, null, 2)}</pre>
            } />
        )}

        {/* Compact field list below */}
        <div className="mt-4 pt-1">
          <div className="text-[9px] font-mono tracking-[0.18em] text-text-3 mb-1">FIELDS</div>
          <Row k="span.id"    v={span.id} />
          <Row k="kind"       v={span.kind} />
          <Row k="duration"   v={`${span.dur} ms`} />
          <Row k="start"      v={`+${span.start} ms`} />
          {span.provider && <Row k="provider" v={span.provider} />}
          {span.model    && <Row k="model"    v={span.model} tone="gold" />}
          {span.tokens_in  !== undefined && <Row k="tokens.in"  v={span.tokens_in.toLocaleString()} />}
          {span.tokens_out !== undefined && <Row k="tokens.out" v={span.tokens_out.toLocaleString()} />}
          <Row k="cost" v={`$${(span.cost ?? 0).toFixed(4)}`} />
          {span.hash && <Row k="prompt.hash" v={span.hash} />}
          {span.decision_idx && <Row k="decision" v={`#${span.decision_idx}`} tone="gold" />}
        </div>
      </div>

      <div className="p-2 grid grid-cols-1 gap-1" style={{ borderTop: "1px solid var(--border)" }}>
        <button
          onClick={() => onToast(`Would scroll to decision #${span.decision_idx || 14}`)}
          className="h-7 px-2 text-[11px] font-mono text-left text-text rounded-sm2 flex items-center gap-2 transition-colors"
          style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}
          onMouseEnter={(e) => e.currentTarget.style.background = "var(--surface-hover)"}
          onMouseLeave={(e) => e.currentTarget.style.background = "var(--surface-elev)"}>
          <span className="text-gold">↧</span> Jump to decision #{span.decision_idx || 14}
        </button>
        <button
          disabled={isLive}
          onClick={() => onToast("Rerun from span queued")}
          title={isLive ? "Disabled — strategy is currently executing" : ""}
          className="h-7 px-2 text-[11px] font-mono text-left rounded-sm2 flex items-center gap-2"
          style={{
            background: isLive ? "transparent" : "var(--surface-elev)",
            border: "1px solid var(--border)",
            color: isLive ? "var(--text-4)" : "var(--text)",
            cursor: isLive ? "not-allowed" : "pointer",
          }}>
          <span style={{ color: isLive ? "var(--text-4)" : "var(--gold)" }}>↻</span>
          Rerun from here
          {isLive && <span className="ml-auto text-[9px] text-text-4 tracking-wider">LOCKED · LIVE</span>}
        </button>
        <button
          onClick={() => onToast("Span JSON copied")}
          className="h-7 px-2 text-[11px] font-mono text-left text-text rounded-sm2 flex items-center gap-2"
          style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}
          onMouseEnter={(e) => e.currentTarget.style.background = "var(--surface-hover)"}
          onMouseLeave={(e) => e.currentTarget.style.background = "var(--surface-elev)"}>
          <span className="text-gold">⧉</span> Copy span JSON
        </button>
      </div>
    </div>
  );
};
