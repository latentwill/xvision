// Mobile screens for the eval-run-detail observability view.
// Translates the three desktop observability layers (Strip, Dock, Inspector) into a phone-appropriate pattern:
//   - Persistent LIVE status row replaces the floating strip
//   - Sticky tab bar with TRACE tab replaces the bottom dock
//   - Bottom sheets replace the inline inspector
//
// Same color tokens + data as the desktop (Eval Run Detail.html) so the visual identity carries.

const { useState: mS, useEffect: mE, useRef: mR, useMemo: mM } = React;

const KIND = {
  agent:      { c: "#a39a85", l: "AGENT" },
  model:      { c: "#7dd3fc", l: "MODEL" },
  tool:       { c: "#6ee7b7", l: "TOOL"  },
  supervisor: { c: "#00e676", l: "SUPER" },
  artifact:   { c: "#a78bfa", l: "ARTIF" },
};

const hexA = (hex, a) => {
  const h = hex.replace("#",""); const r = parseInt(h.slice(0,2),16), g = parseInt(h.slice(2,4),16), b = parseInt(h.slice(4,6),16);
  return `rgba(${r},${g},${b},${a})`;
};

// ─────────────────────────────────────────────────────────────
// Mobile top bar — XVN house pattern (hamburger / serif title / pulse icon)
// ─────────────────────────────────────────────────────────────
function MTopBar({ title, isLive }) {
  return (
    <div style={{
      height: 44, display: "flex", alignItems: "center", gap: 4, padding: "0 8px",
      borderBottom: "1px solid var(--border-soft)", background: "var(--bg)", flexShrink: 0,
    }}>
      <button style={iconBtn(36)}>
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
          <path d="M3 7h18M3 12h18M3 17h12" stroke="var(--text-2)" strokeWidth="1.8" strokeLinecap="round"/>
        </svg>
      </button>
      <div style={{ flex: 1, textAlign: "center", fontFamily: "'Geist', sans-serif", fontSize: 21, fontStyle: "normal", fontWeight: 500, color: "var(--text)" }}>
        {title}
      </div>
      <button style={{ ...iconBtn(36), position: "relative" }}>
        <svg width="17" height="17" viewBox="0 0 24 24" fill="none">
          <path d="M3 12h4l2-5 4 10 2-5h6" stroke={isLive ? "var(--info)" : "var(--text-2)"} strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
        {isLive && <span style={{ position: "absolute", top: 7, right: 8, width: 6, height: 6, borderRadius: 3, background: "var(--info)", border: "1.5px solid var(--bg)" }} />}
      </button>
    </div>
  );
}

const iconBtn = (size) => ({
  width: size, height: size, borderRadius: 999,
  display: "flex", alignItems: "center", justifyContent: "center",
  background: "transparent", border: "none", color: "var(--text-2)", cursor: "pointer",
});

// ─────────────────────────────────────────────────────────────
// LIVE status row — sticky, replaces the floating desktop strip
// ─────────────────────────────────────────────────────────────
function MLiveStrip({ state, isLive, liveDuration, currentSpan, onHalt }) {
  const conf = {
    green: { dot: "var(--gold)",   label: "COMPLETED", glow: "0 0 0 3px var(--gold-bg)", bg: "transparent", bd: "var(--border-soft)" },
    blue:  { dot: "var(--info)",   label: "LIVE",      glow: "0 0 0 3px rgba(111,143,184,0.25)", bg: "rgba(111,143,184,0.06)", bd: "rgba(111,143,184,0.25)" },
    amber: { dot: "var(--warn)",   label: "WARN",      glow: "0 0 0 3px rgba(255, 176, 32, 0.18)",  bg: "rgba(255, 176, 32, 0.06)",  bd: "rgba(255, 176, 32, 0.25)" },
    red:   { dot: "var(--danger)", label: "ERROR",     glow: "0 0 0 3px rgba(255, 77, 77, 0.22)",   bg: "rgba(255, 77, 77, 0.06)",   bd: "rgba(255, 77, 77, 0.30)" },
  }[state];

  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 8,
      padding: "6px 12px", minHeight: 34,
      background: conf.bg, borderBottom: `1px solid ${conf.bd}`, flexShrink: 0, overflow: "hidden",
    }}>
      <span style={{
        width: 6, height: 6, borderRadius: 6, background: conf.dot,
        boxShadow: conf.glow, animation: isLive ? "pulse 1.4s infinite" : "none", flexShrink: 0,
      }} />
      <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: conf.dot, flexShrink: 0 }}>{conf.label}</span>
      <span style={{ fontSize: 11, fontFamily: "'Geist Mono', monospace", color: "var(--text-3)", flexShrink: 0, fontVariantNumeric: "tabular-nums" }}>
        {isLive ? `0:${String(liveDuration).padStart(2,"0")}` : "3.4s"}
      </span>

      <div style={{ width: 1, height: 12, background: "var(--border)", flexShrink: 0 }} />

      {currentSpan ? (
        <div style={{ flex: 1, display: "flex", alignItems: "center", gap: 5, minWidth: 0 }}>
          <span style={{ width: 3, height: 10, background: currentSpan.color, flexShrink: 0 }} />
          <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.14em", color: currentSpan.color, flexShrink: 0 }}>{currentSpan.label}</span>
          <span style={{ fontSize: 11, fontFamily: "'Geist Mono', monospace", color: "var(--text)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", minWidth: 0 }}>{currentSpan.name}</span>
        </div>
      ) : (
        <div style={{ flex: 1 }} />
      )}

      {isLive && (
        <button onClick={onHalt} style={{
          height: 22, padding: "0 8px", fontSize: 9, fontFamily: "'Geist Mono', monospace",
          letterSpacing: "0.18em", color: "var(--danger)", fontWeight: 600,
          background: "rgba(255, 77, 77, 0.10)", border: "1px solid rgba(255, 77, 77, 0.55)",
          borderRadius: 4, flexShrink: 0, cursor: "pointer",
        }}>◼ HALT</button>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Tab bar
// ─────────────────────────────────────────────────────────────
function MTabs({ tabs, active }) {
  return (
    <div style={{
      display: "flex", padding: "0 4px",
      borderBottom: "1px solid var(--border-soft)", background: "var(--bg)",
      flexShrink: 0,
    }}>
      {tabs.map(t => {
        const on = t === active;
        return (
          <button key={t} style={{
            flex: 1,
            padding: "11px 4px", fontSize: 10, fontFamily: "'Geist Mono', monospace",
            letterSpacing: "0.18em", color: on ? "var(--gold)" : "var(--text-3)",
            background: "transparent", border: "none",
            borderBottom: on ? "2px solid var(--gold)" : "2px solid transparent",
            marginBottom: -1, fontWeight: on ? 600 : 400, cursor: "pointer",
          }}>{t}</button>
        );
      })}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Summary tab
// ─────────────────────────────────────────────────────────────
function MStat({ label, value, sub, tone }) {
  const colors = { pos: "#7ab97c", neg: "var(--danger)", neu: "var(--text)", gold: "var(--gold)" };
  return (
    <div style={{ padding: "12px 12px", background: "var(--surface-card)", border: "1px solid var(--border)", borderRadius: 6 }}>
      <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)" }}>{label}</div>
      <div style={{ marginTop: 4, fontSize: 22, fontFamily: "'Geist Mono', monospace", fontVariantNumeric: "tabular-nums", color: colors[tone] || colors.neu, fontWeight: 500, lineHeight: 1 }}>{value}</div>
      {sub && <div style={{ marginTop: 3, fontSize: 9, fontFamily: "'Geist Mono', monospace", color: "var(--text-3)" }}>{sub}</div>}
    </div>
  );
}

function MEquity() {
  const arr = mM(() => {
    const n = 64, out = []; let v = 100;
    for (let i = 0; i < n; i++) {
      const shock = (i > 26 && i < 36) ? -1.4 : 0;
      v += (Math.sin(i / 4) * 0.6) + (Math.cos(i / 9) * 0.4) + shock + 0.18;
      out.push(v);
    }
    return out;
  }, []);
  const min = Math.min(...arr), max = Math.max(...arr);
  const w = 100, h = 60;
  const path = arr.map((y, i) => {
    const x = (i / (arr.length - 1)) * w;
    const yy = h - ((y - min) / (max - min)) * h;
    return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${yy.toFixed(2)}`;
  }).join(" ");

  return (
    <div style={{ position: "relative", height: 96, background: "var(--surface-card)", border: "1px solid var(--border)", borderRadius: 6, overflow: "hidden" }}>
      <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{ position: "absolute", inset: 0, width: "100%", height: "100%" }}>
        <rect x={(26/(arr.length-1))*w} width={((36-26)/(arr.length-1))*w} y="0" height={h} fill="#ff4d4d" opacity="0.10"/>
        <path d={path} fill="none" stroke="#00e676" strokeWidth="0.9" vectorEffect="non-scaling-stroke"/>
        <path d={`${path} L${w},${h} L0,${h} Z`} fill="url(#mg)" opacity="0.30"/>
        <defs>
          <linearGradient id="mg" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#00e676" stopOpacity="0.5"/>
            <stop offset="100%" stopColor="#00e676" stopOpacity="0"/>
          </linearGradient>
        </defs>
      </svg>
      <div style={{ position: "absolute", top: 8, left: 12, fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)" }}>EQUITY · pnl%</div>
      <div style={{ position: "absolute", bottom: 8, right: 12, fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--gold)", fontVariantNumeric: "tabular-nums" }}>+6.42%</div>
    </div>
  );
}

function MActivityCard({ liveDuration, span }) {
  return (
    <div style={{
      padding: "12px 12px", background: "rgba(111,143,184,0.06)",
      border: "1px solid rgba(111,143,184,0.30)", borderRadius: 6, position: "relative", overflow: "hidden",
    }}>
      <div style={{ display: "flex", alignItems: "center", gap: 6, marginBottom: 8 }}>
        <span style={{ width: 5, height: 5, borderRadius: 5, background: "var(--info)", boxShadow: "0 0 0 3px rgba(111,143,184,0.25)", animation: "pulse 1.4s infinite" }} />
        <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--info)" }}>CURRENTLY · {liveDuration}s</span>
      </div>
      <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
        <span style={{ width: 3, height: 12, background: span.color, flexShrink: 0 }} />
        <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.14em", color: span.color }}>{span.label}</span>
        <span style={{ fontSize: 12, fontFamily: "'Geist Mono', monospace", color: "var(--text)" }}>{span.name}</span>
      </div>
      <div style={{ marginTop: 6, fontSize: 11, fontFamily: "'Geist Mono', monospace", color: "var(--text-3)" }}>
        decision <span style={{ color: "var(--gold)" }}>#14</span> · 3,820 tok in · streaming
      </div>
      <div style={{ marginTop: 8, height: 3, background: "var(--border)", borderRadius: 2, overflow: "hidden" }}>
        <div style={{ width: "62%", height: "100%", background: "linear-gradient(90deg, var(--info) 0%, var(--gold) 100%)" }} />
      </div>
    </div>
  );
}

function MSummaryTab({ isLive, liveDuration, currentSpan }) {
  return (
    <div style={{ padding: "14px 12px 96px", display: "flex", flexDirection: "column", gap: 12 }}>
      {/* Hero */}
      <div>
        <div style={{ fontFamily: "'Geist', sans-serif", fontSize: 28, fontStyle: "normal", color: "var(--text)", lineHeight: 1, fontWeight: 500 }}>Run</div>
        <div style={{ fontFamily: "'Geist Mono', monospace", fontSize: 14, color: "var(--text-2)", marginTop: 2 }}>abc1234de7f6</div>
        <div style={{ marginTop: 6, fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--text-3)", display: "flex", flexWrap: "wrap", gap: 8 }}>
          <span>flash-crash-2024-08</span>
          <span style={{ color: "var(--text-4)" }}>·</span>
          <span>mean-reversion-v3</span>
          <span style={{ color: "var(--text-4)" }}>·</span>
          <span>commit 7f2b1ad</span>
        </div>
      </div>

      {isLive && <MActivityCard liveDuration={liveDuration} span={currentSpan} />}

      {/* KPI grid */}
      <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8 }}>
        <MStat label="PNL"      value="+$8,420" sub="+6.42%"      tone="gold"/>
        <MStat label="MAX DD"   value="−2.81%"  sub="@ 10:14:14"  tone="neg"/>
        <MStat label="SHARPE"   value="2.14"    sub="annualized"  tone="neu"/>
        <MStat label="WIN RATE" value="62.5%"   sub="5/8 trades"  tone="neu"/>
      </div>

      <MEquity/>

      {/* Meta */}
      <div style={{ background: "var(--surface-card)", border: "1px solid var(--border)", borderRadius: 6, padding: "10px 12px" }}>
        <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)", marginBottom: 6 }}>META</div>
        {[
          ["scenario",   "flash-crash-2024-08"],
          ["strategy",   "mean-reversion-v3"],
          ["mode",       "paper · alpaca"],
          ["budget",     "$0.18 / $1.00 (18%)"],
          ["seed",       "0x9c44a1"],
        ].map(([k, v]) => (
          <div key={k} style={{ display: "flex", padding: "3px 0", borderBottom: "1px solid var(--border-soft)", fontSize: 11, fontFamily: "'Geist Mono', monospace" }}>
            <span style={{ width: 70, color: "var(--text-3)", fontSize: 9, letterSpacing: "0.14em", textTransform: "uppercase", paddingTop: 2 }}>{k}</span>
            <span style={{ flex: 1, color: "var(--text)", fontVariantNumeric: "tabular-nums" }}>{v}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Trace tab — replaces the desktop dock
// ─────────────────────────────────────────────────────────────
function MFilterChips({ kinds, decisionIdx, onMoreFilters }) {
  return (
    <div style={{
      display: "flex", alignItems: "center", gap: 6,
      padding: "8px 12px", borderBottom: "1px solid var(--border-soft)",
      background: "var(--surface-card)", overflowX: "auto", flexShrink: 0,
      scrollbarWidth: "none",
    }}>
      <button onClick={onMoreFilters} style={{
        height: 24, padding: "0 8px", display: "flex", alignItems: "center", gap: 4,
        background: "var(--bg)", border: "1px solid var(--border)", borderRadius: 4,
        fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--text-2)",
        flexShrink: 0, cursor: "pointer",
      }}>
        <svg width="10" height="10" viewBox="0 0 16 16" fill="none"><circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4"/><path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>
        filter
      </button>

      {Object.keys(KIND).map(k => {
        const on = kinds.has(k);
        const c = KIND[k].c;
        return (
          <button key={k} style={{
            height: 24, padding: "0 8px", display: "flex", alignItems: "center", gap: 4,
            background: on ? "var(--surface-card)" : "transparent",
            border: `1px solid ${on ? c : "var(--border)"}`, borderRadius: 4,
            fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.14em",
            color: on ? c : "var(--text-3)", flexShrink: 0, cursor: "pointer",
          }}>
            <span style={{ width: 5, height: 5, background: c, opacity: on ? 1 : 0.5 }} />
            {KIND[k].l}
          </button>
        );
      })}

      <div style={{ width: 1, height: 16, background: "var(--border)", flexShrink: 0, margin: "0 2px" }} />

      <div style={{
        display: "flex", alignItems: "center", gap: 2, height: 24, padding: "0 4px 0 8px",
        background: "var(--bg)", border: `1px solid ${decisionIdx != null ? "var(--gold-soft)" : "var(--border)"}`,
        borderRadius: 4, flexShrink: 0,
      }}>
        <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.14em", color: decisionIdx != null ? "var(--gold-soft)" : "var(--text-4)" }}>DECISION&nbsp;#</span>
        <span style={{ fontSize: 11, fontFamily: "'Geist Mono', monospace", color: decisionIdx != null ? "var(--gold)" : "var(--text)", fontVariantNumeric: "tabular-nums", paddingRight: 4 }}>
          {decisionIdx != null ? decisionIdx : "—"}
        </span>
        <button style={{ width: 20, height: "100%", background: "transparent", border: "none", color: "var(--text-3)", cursor: "pointer" }}>
          <svg width="9" height="9" viewBox="0 0 16 16" fill="none"><path d="M10 3l-5 5 5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
        </button>
        <button style={{ width: 20, height: "100%", background: "transparent", border: "none", color: "var(--text-3)", cursor: "pointer" }}>
          <svg width="9" height="9" viewBox="0 0 16 16" fill="none"><path d="M6 3l5 5-5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
        </button>
      </div>
    </div>
  );
}

function MSpanRow({ s, selected, isLive, currentTime, guides, isAncestorOfSelected, onTap }) {
  const c = KIND[s.kind] || KIND.agent;
  const inflight = isLive && (s.start + s.dur) > currentTime && s.start < currentTime;
  const future   = isLive && s.start > currentTime;
  const effDur   = inflight ? Math.max(20, currentTime - s.start) : s.dur;
  const isSel    = selected === s.id;

  // duration formatting — switch to seconds past 1000ms so the trailing column stays narrow
  const durStr = effDur >= 1000
    ? `${(effDur / 1000).toFixed(2)}s`
    : `${effDur}ms`;

  return (
    <div onClick={onTap} style={{
      display: "flex", alignItems: "stretch", minHeight: 32,
      background: isSel ? "var(--gold-bg)" : "transparent",
      borderLeft: isSel ? "2px solid var(--gold)" : "2px solid transparent",
      borderBottom: "1px solid var(--border-soft)",
      cursor: "pointer",
      opacity: future ? 0.45 : 1,
    }}>
      <TreeGuide guides={guides} highlight={isAncestorOfSelected || isSel} />

      <div style={{ flex: 1, display: "flex", alignItems: "center", gap: 6, minWidth: 0, padding: "0 10px 0 4px" }}>
        {/* kind tag */}
        <span style={{ width: 3, height: 12, background: c.c, flexShrink: 0 }} />
        <span style={{
          fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.14em",
          color: c.c, flexShrink: 0, width: 36, // fixed width so names align across rows
        }}>{c.l}</span>

        {/* name */}
        <span style={{
          fontSize: 12, fontFamily: "'Geist Mono', monospace",
          color: isSel ? "var(--text)" : (s.depth === 0 ? "var(--text)" : "var(--text-2)"),
          fontWeight: s.depth === 0 ? 500 : 400,
          overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap", flex: 1, minWidth: 0,
        }}>{s.name}</span>

        {/* inflight pulse */}
        {inflight && (
          <span style={{
            width: 5, height: 5, borderRadius: 5, background: "var(--info)",
            boxShadow: "0 0 0 2px rgba(111,143,184,0.30)",
            animation: "pulse 1.4s infinite", flexShrink: 0,
          }} />
        )}

        {/* duration */}
        <span style={{
          fontSize: 10, fontFamily: "'Geist Mono', monospace",
          color: inflight ? "var(--info)" : "var(--text-3)",
          fontVariantNumeric: "tabular-nums", flexShrink: 0,
          minWidth: 44, textAlign: "right",
        }}>{durStr}</span>
      </div>
    </div>
  );
}

// Tree connectors — one cell per ancestor depth + one for the row's own connector.
function TreeGuide({ guides, highlight }) {
  const base   = "var(--border)";
  const strong = "var(--gold-soft)";
  return (
    <div style={{ display: "flex", flexShrink: 0, alignSelf: "stretch" }}>
      {guides.map((g, i) => {
        const isOwn = i === guides.length - 1 && (g === "branch" || g === "last");
        const lineColor = highlight && isOwn ? strong : base;
        return (
          <div key={i} style={{ width: 14, position: "relative" }}>
            {/* full-height vertical line for ancestors that still have siblings below */}
            {g === "through" && (
              <div style={{ position: "absolute", left: 6, top: 0, bottom: 0, width: 1, background: base }} />
            )}
            {/* row's own connector — ├ or └ */}
            {g === "branch" && (
              <>
                <div style={{ position: "absolute", left: 6, top: 0, bottom: 0, width: 1, background: lineColor }} />
                <div style={{ position: "absolute", left: 6, top: "50%", width: 7, height: 1, background: lineColor }} />
              </>
            )}
            {g === "last" && (
              <>
                <div style={{ position: "absolute", left: 6, top: 0, height: "50%", width: 1, background: lineColor }} />
                <div style={{ position: "absolute", left: 6, top: "50%", width: 7, height: 1, background: lineColor }} />
              </>
            )}
          </div>
        );
      })}
    </div>
  );
}

// Pre-compute hierarchy meta for the span list.
function computeGuides(spans) {
  return spans.map((s, i) => {
    const guides = [];
    // ancestor columns — vertical line continues if the ancestor at depth d still has another sibling coming
    for (let d = 0; d < s.depth; d++) {
      let hasMore = false;
      for (let j = i + 1; j < spans.length; j++) {
        if (spans[j].depth < d) break;
        if (spans[j].depth === d) { hasMore = true; break; }
      }
      guides.push(hasMore ? "through" : "none");
    }
    // row's own connector to parent
    if (s.depth > 0) {
      let hasMore = false;
      for (let j = i + 1; j < spans.length; j++) {
        if (spans[j].depth < s.depth) break;
        if (spans[j].depth === s.depth) { hasMore = true; break; }
      }
      guides.push(hasMore ? "branch" : "last");
    }
    return guides;
  });
}

// Return set of span ids that are ancestors of `selectedId`.
function ancestorsOf(spans, selectedId) {
  const idx = spans.findIndex(s => s.id === selectedId);
  if (idx === -1) return new Set();
  const target = spans[idx];
  const out = new Set();
  let depth = target.depth - 1;
  for (let j = idx - 1; j >= 0 && depth >= 0; j--) {
    if (spans[j].depth === depth) { out.add(spans[j].id); depth--; }
  }
  return out;
}

function MTraceTab({ spans, selected, onSelectSpan, isLive, liveDuration, kinds, decisionFilter, onMoreFilters, filtered }) {
  const currentTime = liveDuration * 70 + 200;
  const guidesAll   = mM(() => computeGuides(filtered), [filtered]);
  const ancestors   = mM(() => ancestorsOf(filtered, selected), [filtered, selected]);
  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <MFilterChips kinds={kinds} decisionIdx={decisionFilter} onMoreFilters={onMoreFilters} />

      <div style={{
        display: "flex", alignItems: "center", justifyContent: "space-between",
        padding: "6px 12px", background: "var(--surface-elev)", borderBottom: "1px solid var(--border-soft)",
        fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--text-3)", flexShrink: 0,
      }}>
        <span><span style={{ color: "var(--text)" }}>{filtered.length}</span><span style={{ color: "var(--text-4)" }}>/</span>{spans.length} spans · 3.4s total</span>
        <button style={{ background: "transparent", border: "none", color: "var(--text-3)", fontSize: 10, fontFamily: "'Geist Mono', monospace" }}>
          ⤴ flame view
        </button>
      </div>

      <div style={{ flex: 1, overflow: "auto", paddingBottom: 80 }}>
        {filtered.map((s, i) => (
          <MSpanRow
            key={s.id}
            s={s}
            selected={selected}
            isLive={isLive}
            currentTime={currentTime}
            guides={guidesAll[i]}
            isAncestorOfSelected={ancestors.has(s.id)}
            onTap={() => onSelectSpan(s.id)}
          />
        ))}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Decisions tab
// ─────────────────────────────────────────────────────────────
function MActionPill({ a }) {
  const styles = {
    BUY:  { color: "var(--gold)",   bg: "var(--gold-bg)",         bd: "var(--gold-soft)" },
    SELL: { color: "var(--danger)", bg: "rgba(255, 77, 77, 0.10)",   bd: "rgba(255, 77, 77, 0.45)" },
    HOLD: { color: "var(--text-3)", bg: "transparent",            bd: "var(--border)" },
  }[a];
  return (
    <span style={{
      padding: "2px 6px", fontSize: 9, fontFamily: "'Geist Mono', monospace",
      letterSpacing: "0.18em", color: styles.color, background: styles.bg,
      border: `1px solid ${styles.bd}`, borderRadius: 3,
    }}>{a}</span>
  );
}

function MDecisionsTab({ decisions, focused }) {
  return (
    <div style={{ padding: "12px 12px 96px", display: "flex", flexDirection: "column", gap: 8 }}>
      <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)" }}>
        {decisions.length} STEPS · 5 TRADES
      </div>
      {decisions.map(d => {
        const focus = d.i === focused;
        return (
          <div key={d.i} style={{
            padding: "10px 12px",
            background: focus ? "var(--gold-bg)" : "var(--surface-card)",
            border: `1px solid ${focus ? "var(--gold-soft)" : "var(--border)"}`,
            borderRadius: 6,
          }}>
            <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
              <span style={{ fontSize: 11, fontFamily: "'Geist Mono', monospace", color: focus ? "var(--gold)" : "var(--text-3)", fontVariantNumeric: "tabular-nums", fontWeight: 500 }}>#{d.i}</span>
              <MActionPill a={d.action}/>
              <span style={{ fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--text-3)", fontVariantNumeric: "tabular-nums" }}>{d.t}</span>
              <span style={{
                marginLeft: "auto", fontSize: 11, fontFamily: "'Geist Mono', monospace",
                fontVariantNumeric: "tabular-nums",
                color: d.pnl > 0 ? "var(--gold)" : d.pnl < 0 ? "var(--danger)" : "var(--text-4)",
              }}>
                {d.pnl === 0 ? "—" : (d.pnl > 0 ? `+$${d.pnl.toLocaleString()}` : `−$${Math.abs(d.pnl).toLocaleString()}`)}
              </span>
            </div>
            <div style={{ marginTop: 8, display: "flex", alignItems: "center", gap: 6 }}>
              <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.14em", color: "var(--text-4)", width: 50 }}>CONV</span>
              <span style={{ fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--text)", fontVariantNumeric: "tabular-nums", width: 32 }}>{(d.conv * 100).toFixed(0)}%</span>
              <span style={{ flex: 1, height: 3, background: "var(--border)", borderRadius: 2, overflow: "hidden" }}>
                <span style={{ display: "block", width: `${d.conv * 100}%`, height: "100%", background: "var(--gold)" }} />
              </span>
            </div>
            <div style={{ marginTop: 6, fontSize: 12, color: "var(--text-2)", lineHeight: 1.4 }}>
              {d.just}
            </div>
          </div>
        );
      })}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Review tab
// ─────────────────────────────────────────────────────────────
function MReviewTab() {
  return (
    <div style={{ padding: "14px 12px 96px", display: "flex", flexDirection: "column", gap: 12 }}>
      <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div>
          <div style={{ fontFamily: "'Geist', sans-serif", fontSize: 24, fontStyle: "normal", color: "var(--text)", lineHeight: 1, fontWeight: 500 }}>Review</div>
          <div style={{ fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--text-3)", marginTop: 3 }}>supervisor · claude-haiku-4-5</div>
        </div>
        <span style={{ padding: "2px 6px", fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--warn)", background: "rgba(255, 176, 32, 0.10)", border: "1px solid rgba(255, 176, 32, 0.40)", borderRadius: 3 }}>2 NOTES</span>
      </div>

      <p style={{ margin: 0, fontFamily: "'Geist', sans-serif", fontStyle: "normal", fontSize: 16, color: "var(--text)", lineHeight: 1.4 }}>
        Strategy executed mean-reversion logic correctly under the 14:14 liquidity shock, scaling into the dislocation at half-Kelly. Sharpe (2.14) within bounds; drawdown contained.
      </p>

      {[
        { n: "NOTE 1", body: <>Decision <span style={{ color: "var(--gold)" }}>#14</span> used stale book snapshot (Δ 320ms). Consider tighter freshness gate.</> },
        { n: "NOTE 2", body: <>Tool <code style={{ color: "var(--warn)", fontFamily: "'Geist Mono', monospace" }}>run_backtest</code> called twice in adjacent steps — possible cache miss.</> },
      ].map(({ n, body }) => (
        <div key={n} style={{ padding: "10px 12px", background: "var(--surface-card)", border: "1px solid var(--border)", borderRadius: 6 }}>
          <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)", marginBottom: 4 }}>{n}</div>
          <div style={{ fontSize: 12, color: "var(--text)", lineHeight: 1.4, fontFamily: "'Geist Mono', monospace" }}>{body}</div>
        </div>
      ))}
    </div>
  );
}

// ─────────────────────────────────────────────────────────────
// Bottom sheets
// ─────────────────────────────────────────────────────────────
function MSheet({ title, children, height = 560 }) {
  return (
    <div style={{ position: "absolute", inset: 0, zIndex: 30 }}>
      <div style={{ position: "absolute", inset: 0, background: "rgba(0,0,0,0.55)" }} />
      <div style={{
        position: "absolute", bottom: 0, left: 0, right: 0, height,
        background: "var(--surface-card)",
        borderTopLeftRadius: 18, borderTopRightRadius: 18,
        borderTop: "1px solid var(--border-strong)",
        display: "flex", flexDirection: "column",
        boxShadow: "0 -20px 60px rgba(0,0,0,0.6)",
      }}>
        <div style={{ height: 18, display: "flex", alignItems: "center", justifyContent: "center", flexShrink: 0 }}>
          <div style={{ width: 36, height: 4, borderRadius: 2, background: "var(--border-strong)" }} />
        </div>
        {title && (
          <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", padding: "0 14px 10px", flexShrink: 0 }}>
            {title}
          </div>
        )}
        <div style={{ flex: 1, overflow: "auto" }}>{children}</div>
      </div>
    </div>
  );
}

function PullQuote({ label, body, accent = "var(--gold)", glyph = "“", italic = false, streaming = false }) {
  return (
    <div style={{ marginTop: 0 }}>
      <div style={{ display: "flex", justifyContent: "space-between", marginBottom: 4 }}>
        <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)" }}>{label}</span>
        {streaming && <span style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.16em", color: "var(--info)", animation: "pulse 1.4s infinite" }}>● STREAMING</span>}
      </div>
      <div style={{
        position: "relative", padding: "8px 10px 8px 12px",
        background: "var(--surface-elev)", borderRadius: 4, borderLeft: `2px solid ${accent}`,
      }}>
        <span style={{ position: "absolute", top: -2, left: 4, fontSize: 22, fontFamily: "'Geist', sans-serif", color: accent, opacity: 0.45, lineHeight: 1, userSelect: "none" }}>{glyph}</span>
        <div style={{
          paddingLeft: 12, fontSize: 12, lineHeight: 1.45, color: "var(--text)",
          fontFamily: italic ? "'Geist', sans-serif" : "'Geist Mono', monospace",
          fontStyle: italic ? "italic" : "normal",
        }}>
          {typeof body === "string" ? body : body}
          {streaming && <span style={{ display: "inline-block", width: 4, height: 12, background: "var(--info)", marginLeft: 4, verticalAlign: "middle", animation: "pulse 1.4s infinite" }} />}
        </div>
      </div>
    </div>
  );
}

function MSpanSheet({ span }) {
  if (!span) return null;
  const c = KIND[span.kind] || KIND.agent;

  const Row = ({ k, v, tone }) => (
    <div style={{ display: "flex", padding: "5px 0", borderBottom: "1px solid var(--border-soft)" }}>
      <span style={{ width: 90, fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.14em", textTransform: "uppercase", color: "var(--text-3)", paddingTop: 2 }}>{k}</span>
      <span style={{ flex: 1, fontSize: 11, fontFamily: "'Geist Mono', monospace", color: tone === "gold" ? "var(--gold)" : "var(--text)", fontVariantNumeric: "tabular-nums", wordBreak: "break-all" }}>{v}</span>
    </div>
  );

  const title = (
    <>
      <div style={{ display: "flex", alignItems: "center", gap: 6, minWidth: 0 }}>
        <span style={{
          padding: "2px 6px", fontSize: 9, fontFamily: "'Geist Mono', monospace",
          letterSpacing: "0.16em", color: c.c, background: hexA(c.c, 0.08),
          border: `1px solid ${hexA(c.c, 0.4)}`, borderRadius: 3, flexShrink: 0,
        }}>{c.l}</span>
        <span style={{ fontSize: 12, fontFamily: "'Geist Mono', monospace", color: "var(--text)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>{span.name}</span>
      </div>
      <button style={iconBtn(28)}>
        <svg width="14" height="14" viewBox="0 0 16 16" fill="none"><path d="M3 3l10 10M13 3L3 13" stroke="var(--text-3)" strokeWidth="1.5" strokeLinecap="round"/></svg>
      </button>
    </>
  );

  return (
    <MSheet title={title} height={620}>
      <div style={{ padding: "0 14px 14px", display: "flex", flexDirection: "column", gap: 10 }}>
        {span.prompt && (
          <PullQuote label="PROMPT" body={span.prompt} accent={c.c} glyph="›" />
        )}
        {span.response && (
          <PullQuote label="RESPONSE" body={span.response} accent="var(--gold)" glyph="“" italic />
        )}
        {span.response_partial && (
          <PullQuote label="RESPONSE (PARTIAL)" body={span.response_partial} accent="var(--info)" glyph="“" italic streaming />
        )}
        {span.args && (
          <PullQuote label="TOOL ARGS" accent={c.c} glyph="›"
            body={<pre style={{ margin: 0, fontFamily: "'Geist Mono', monospace", fontSize: 11, whiteSpace: "pre-wrap", color: "var(--text-2)" }}>{JSON.stringify(span.args, null, 2)}</pre>} />
        )}
        {span.result && (
          <PullQuote label="TOOL RESULT" accent="var(--gold)" glyph="←"
            body={<pre style={{ margin: 0, fontFamily: "'Geist Mono', monospace", fontSize: 11, whiteSpace: "pre-wrap", color: "var(--text)" }}>{JSON.stringify(span.result, null, 2)}</pre>} />
        )}

        <div style={{ marginTop: 6 }}>
          <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)", marginBottom: 4 }}>FIELDS</div>
          <Row k="duration" v={`${span.dur} ms`} />
          <Row k="start"    v={`+${span.start} ms`} />
          {span.provider && <Row k="provider" v={span.provider} />}
          {span.model    && <Row k="model"    v={span.model} tone="gold" />}
          {span.tokens_in  !== undefined && <Row k="tokens.in"  v={span.tokens_in.toLocaleString()} />}
          {span.tokens_out !== undefined && <Row k="tokens.out" v={span.tokens_out.toLocaleString()} />}
          <Row k="cost" v={`$${(span.cost ?? 0).toFixed(4)}`} />
          {span.hash && <Row k="prompt.hash" v={span.hash} />}
          {span.decision_idx && <Row k="decision" v={`#${span.decision_idx}`} tone="gold" />}
        </div>

        <div style={{ display: "flex", flexDirection: "column", gap: 6, marginTop: 8 }}>
          {[
            ["↧", "Jump to decision #14", false],
            ["↻", "Rerun from here",       true],  // locked because LIVE
            ["⧉", "Copy span JSON",        false],
          ].map(([gly, label, locked]) => (
            <button key={label} disabled={locked} style={{
              height: 36, padding: "0 10px", display: "flex", alignItems: "center", gap: 8,
              background: "var(--surface-elev)", border: "1px solid var(--border)", borderRadius: 4,
              fontSize: 12, fontFamily: "'Geist Mono', monospace",
              color: locked ? "var(--text-4)" : "var(--text)", cursor: locked ? "not-allowed" : "pointer",
              opacity: locked ? 0.6 : 1, textAlign: "left",
            }}>
              <span style={{ color: locked ? "var(--text-4)" : "var(--gold)", width: 14 }}>{gly}</span>
              {label}
              {locked && <span style={{ marginLeft: "auto", fontSize: 9, letterSpacing: "0.16em", color: "var(--text-4)" }}>LIVE</span>}
            </button>
          ))}
        </div>
      </div>
    </MSheet>
  );
}

function MFilterSheet({ kinds, decisionFilter, filteredCount }) {
  const STATUS_DEF = [
    { k: "any",   glyph: "·",  tint: "var(--text)",    bg: "var(--surface-elev)", bd: "var(--border)" },
    { k: "green", glyph: "✓",  tint: "var(--gold)",    bg: "var(--gold-bg)",      bd: "var(--gold-soft)" },
    { k: "blue",  glyph: "▶",  tint: "var(--info)",    bg: "rgba(111,143,184,0.14)", bd: "rgba(111,143,184,0.45)" },
    { k: "amber", glyph: "⚠",  tint: "var(--warn)",    bg: "rgba(255, 176, 32, 0.10)",  bd: "rgba(255, 176, 32, 0.45)" },
    { k: "red",   glyph: "✕",  tint: "var(--danger)",  bg: "rgba(255, 77, 77, 0.10)",   bd: "rgba(255, 77, 77, 0.45)" },
  ];
  return (
    <MSheet height={520} title={
      <>
        <span style={{ fontFamily: "'Geist', sans-serif", fontStyle: "normal", fontSize: 22, color: "var(--text)", fontWeight: 500 }}>Filter trace</span>
        <button style={{ fontSize: 11, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.16em", color: "var(--text-3)", background: "transparent", border: "none", cursor: "pointer" }}>CLEAR ALL</button>
      </>
    }>
      <div style={{ padding: "0 14px 14px", display: "flex", flexDirection: "column", gap: 14 }}>
        {/* search */}
        <div>
          <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)", marginBottom: 6 }}>SEARCH</div>
          <div style={{ display: "flex", alignItems: "center", gap: 6, padding: "0 10px", height: 36, background: "var(--bg)", border: "1px solid var(--border)", borderRadius: 4 }}>
            <svg width="11" height="11" viewBox="0 0 16 16" fill="none" style={{ color: "var(--text-3)" }}><circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4"/><path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round"/></svg>
            <input defaultValue="model:gpt-5" style={{
              flex: 1, fontSize: 13, fontFamily: "'Geist Mono', monospace",
              color: "var(--text)", background: "transparent", border: "none", outline: "none",
            }}/>
          </div>
          <div style={{ marginTop: 4, fontSize: 9, fontFamily: "'Geist Mono', monospace", color: "var(--text-4)" }}>
            try: title:agent.plan · model:gpt-5 · tool:run_backtest · decision:14
          </div>
        </div>

        {/* kind */}
        <div>
          <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)", marginBottom: 6 }}>KIND</div>
          <div style={{ display: "grid", gridTemplateColumns: "1fr 1fr 1fr", gap: 6 }}>
            {Object.keys(KIND).map(k => {
              const on = kinds.has(k);
              const c = KIND[k].c;
              return (
                <button key={k} style={{
                  height: 34, padding: "0 8px", display: "flex", alignItems: "center", gap: 6, justifyContent: "center",
                  background: on ? "var(--surface-elev)" : "transparent",
                  border: `1px solid ${on ? c : "var(--border)"}`, borderRadius: 4,
                  fontSize: 10, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.16em",
                  color: on ? c : "var(--text-3)",
                }}>
                  <span style={{ width: 6, height: 6, background: c, opacity: on ? 1 : 0.5 }} />
                  {KIND[k].l}
                </button>
              );
            })}
          </div>
        </div>

        {/* status */}
        <div>
          <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)", marginBottom: 6 }}>STATUS</div>
          <div style={{ display: "flex", gap: 6 }}>
            {STATUS_DEF.map(s => {
              const on = s.k === "blue";
              return (
                <button key={s.k} style={{
                  flex: 1, height: 34,
                  background: on ? s.bg : "transparent",
                  border: `1px solid ${on ? s.bd : "var(--border)"}`, borderRadius: 4,
                  fontSize: 11, fontFamily: "'Geist Mono', monospace",
                  color: on ? s.tint : "var(--text-3)",
                  display: "flex", alignItems: "center", justifyContent: "center", gap: 4,
                }}>
                  <span style={{ fontSize: 13 }}>{s.glyph}</span>
                  <span style={{ fontSize: 9, letterSpacing: "0.16em" }}>{s.k === "any" ? "ANY" : s.k.toUpperCase()}</span>
                </button>
              );
            })}
          </div>
        </div>

        {/* decision */}
        <div>
          <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.18em", color: "var(--text-3)", marginBottom: 6 }}>JUMP TO DECISION</div>
          <div style={{
            display: "flex", alignItems: "center", height: 38,
            background: "var(--bg)", border: `1px solid ${decisionFilter != null ? "var(--gold-soft)" : "var(--border)"}`,
            borderRadius: 4, padding: "0 6px 0 10px",
          }}>
            <span style={{ fontSize: 10, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.16em", color: "var(--gold-soft)" }}>DECISION&nbsp;#</span>
            <input defaultValue={decisionFilter ?? ""} placeholder="—" style={{
              width: 60, marginLeft: 4, background: "transparent", border: "none", outline: "none",
              fontSize: 16, fontFamily: "'Geist Mono', monospace", color: "var(--gold)",
              fontVariantNumeric: "tabular-nums",
            }} />
            <button style={{ width: 30, height: 30, background: "transparent", border: "none", color: "var(--text-3)", cursor: "pointer" }}>
              <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M10 3l-5 5 5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
            </button>
            <button style={{ width: 30, height: 30, background: "transparent", border: "none", color: "var(--text-3)", cursor: "pointer" }}>
              <svg width="11" height="11" viewBox="0 0 16 16" fill="none"><path d="M6 3l5 5-5 5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round"/></svg>
            </button>
            <span style={{ marginLeft: "auto", fontSize: 10, fontFamily: "'Geist Mono', monospace", color: "var(--text-4)", paddingRight: 8 }}>
              {decisionFilter != null ? `${decisionFilter > 0 ? 4 : 0}/8` : "of 8"}
            </span>
          </div>
        </div>

        {/* apply */}
        <button style={{
          marginTop: 6, height: 44, background: "var(--gold)", border: "none", borderRadius: 4,
          fontFamily: "'Geist Mono', monospace", fontSize: 12, letterSpacing: "0.18em",
          color: "#0f0e0c", fontWeight: 600, cursor: "pointer",
        }}>
          SHOW {filteredCount} SPANS
        </button>
      </div>
    </MSheet>
  );
}

// ─────────────────────────────────────────────────────────────
// App — composes one phone screen for a given state
// ─────────────────────────────────────────────────────────────
window.MobileApp = function MobileApp({ scenario }) {
  const { spans, decisions } = window.MOCK;

  // Defaults
  const tab            = scenario.tab            || "TRACE";
  const isLive         = scenario.isLive         !== undefined ? scenario.isLive : true;
  const stripState     = scenario.stripState     || (isLive ? "blue" : "green");
  const liveDuration   = scenario.liveDuration   ?? 43;
  const selectedSpan   = scenario.selectedSpan   || "s6";
  const sheet          = scenario.sheet          || null;     // 'span' | 'filter' | null
  const kinds          = new Set(scenario.kinds  || []);
  const decisionFilter = scenario.decisionFilter ?? null;
  const focusedDec     = scenario.focusedDec     ?? 14;

  // Derive current span for the LIVE strip
  const currentSpan = mM(() => {
    const t = liveDuration * 70 + 200;
    const candidates = spans.filter(x => x.start <= t && x.start + x.dur > t);
    const s = candidates.sort((a, b) => b.depth - a.depth || b.start - a.start)[0]
           || spans.find(x => x.id === selectedSpan);
    if (!s) return null;
    const k = KIND[s.kind] || KIND.agent;
    return { name: s.name, color: k.c, label: k.l };
  }, [spans, liveDuration, selectedSpan]);

  // Filtered spans
  const filtered = mM(() => spans.filter(s => {
    if (kinds.size > 0 && !kinds.has(s.kind)) return false;
    if (decisionFilter != null && String(s.decision_idx || "") !== String(decisionFilter)) return false;
    return true;
  }), [spans, kinds, decisionFilter]);

  // Page content per tab
  let body;
  if (tab === "SUMMARY") {
    body = <div style={{ flex: 1, overflow: "auto" }}><MSummaryTab isLive={isLive} liveDuration={liveDuration} currentSpan={currentSpan} /></div>;
  } else if (tab === "DECISIONS") {
    body = <div style={{ flex: 1, overflow: "auto" }}><MDecisionsTab decisions={decisions} focused={focusedDec} /></div>;
  } else if (tab === "REVIEW") {
    body = <div style={{ flex: 1, overflow: "auto" }}><MReviewTab/></div>;
  } else { // TRACE
    body = <div style={{ flex: 1, minHeight: 0, display: "flex" }}>
      <MTraceTab
        spans={spans} selected={selectedSpan} onSelectSpan={() => {}}
        isLive={isLive} liveDuration={liveDuration}
        kinds={kinds} decisionFilter={decisionFilter}
        onMoreFilters={() => {}}
        filtered={filtered}
      />
    </div>;
  }

  return (
    <div style={{ position: "absolute", inset: 0, paddingTop: 46, background: "var(--bg)", display: "flex", flexDirection: "column" }}>
      <MTopBar title="Eval run" isLive={isLive}/>
      <MLiveStrip state={stripState} isLive={isLive} liveDuration={liveDuration} currentSpan={currentSpan} onHalt={() => {}} />
      <MTabs tabs={["SUMMARY", "DECISIONS", "TRACE", "REVIEW"]} active={tab}/>
      {body}

      {/* Floating chat pill — only when no sheet */}
      {!sheet && (
        <div style={{
          position: "absolute", left: 12, right: 12, bottom: 46, height: 50,
          background: "var(--surface-card)", border: "1px solid var(--border-strong)",
          borderRadius: 999, display: "flex", alignItems: "center", gap: 10, padding: "0 8px",
          boxShadow: "0 14px 40px rgba(0,0,0,0.5)",
        }}>
          <span style={{
            width: 32, height: 32, borderRadius: 999, flexShrink: 0,
            background: "var(--gold-bg)", border: "1px solid var(--gold-soft)",
            display: "flex", alignItems: "center", justifyContent: "center", color: "var(--gold)",
          }}>
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none"><path d="M3 12h4l2-5 4 10 2-5h6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/></svg>
          </span>
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 9, fontFamily: "'Geist Mono', monospace", letterSpacing: "0.16em", color: "var(--text-3)" }}>RUN abc1234…</div>
            <div style={{ fontSize: 12, color: "var(--text-2)", overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>Ask the supervisor about this run…</div>
          </div>
          <span style={{ width: 28, height: 28, borderRadius: 14, background: "var(--surface-elev)", border: "1px solid var(--border)", display: "flex", alignItems: "center", justifyContent: "center", color: "var(--text-2)", flexShrink: 0 }}>
            <svg width="12" height="12" viewBox="0 0 16 16" fill="none"><path d="M6 3l5 5-5 5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/></svg>
          </span>
        </div>
      )}

      {/* Sheets */}
      {sheet === "span"   && <MSpanSheet span={spans.find(s => s.id === selectedSpan)} />}
      {sheet === "filter" && <MFilterSheet kinds={kinds} decisionFilter={decisionFilter} filteredCount={filtered.length} />}
    </div>
  );
};
