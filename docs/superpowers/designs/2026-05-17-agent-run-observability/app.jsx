// Main app — XVN folio-dark. /eval-runs/abc1234de7f6 with trace observability layers.
const { useState: uS, useEffect: uE, useMemo: uM, useRef } = React;

// ---------- Topbar ----------
function TopBar({ isLive, setLive }) {
  return (
    <div className="h-12 px-4 flex items-center gap-3 shrink-0" style={{ background: "var(--surface-sidebar)", borderBottom: "1px solid var(--border)" }}>
      <div className="flex items-center gap-2">
        <span className="font-serif italic text-[22px] text-gold leading-none" style={{ fontWeight: 600 }}>xvn</span>
      </div>
      <span className="text-text-4 mx-1">/</span>
      <span className="text-[11px] font-mono tracking-[0.18em] text-text-3 uppercase">eval runs</span>
      <span className="text-text-4">/</span>
      <code className="text-[12px] font-mono text-text-2">abc1234…</code>

      <div className="w-px h-5 mx-2" style={{ background: "var(--border)" }}></div>

      <div className="flex items-baseline gap-2">
        <span className="font-serif italic text-[20px] text-text leading-none" style={{ fontWeight: 500 }}>Run abc1234…</span>
        <span className="text-text-4 text-[12px]">·</span>
        <span className="text-[12px] text-text-2">scenario</span>
        <code className="text-[12px] font-mono text-text">flash-crash-2024-08</code>
      </div>

      <div className="ml-2 flex items-center gap-1.5 px-2 py-0.5 rounded-sm2"
        style={{
          background: isLive ? "rgba(111,143,184,0.12)" : "var(--gold-bg)",
          border: `1px solid ${isLive ? "rgba(111,143,184,0.40)" : "var(--gold-soft)"}`,
        }}>
        <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: isLive ? "var(--info)" : "var(--gold)" }}></span>
        <span className="text-[10px] font-mono tracking-[0.16em]" style={{ color: isLive ? "var(--info)" : "var(--gold)" }}>
          {isLive ? "RUNNING" : "COMPLETED"}
        </span>
      </div>

      <div className="ml-auto flex items-center gap-1 p-0.5 rounded-sm2"
        style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}>
        <button onClick={() => setLive(false)}
          className="h-6 px-2.5 text-[10px] font-mono tracking-[0.16em] rounded-sm2"
          style={{
            background: !isLive ? "var(--gold-bg)" : "transparent",
            color: !isLive ? "var(--gold)" : "var(--text-3)",
          }}>
          POST-HOC
        </button>
        <span className="text-text-4 text-[10px] px-0.5">⇄</span>
        <button onClick={() => setLive(true)}
          className="h-6 px-2.5 text-[10px] font-mono tracking-[0.16em] rounded-sm2 flex items-center gap-1.5"
          style={{
            background: isLive ? "rgba(111,143,184,0.18)" : "transparent",
            color: isLive ? "#bcd1ea" : "var(--text-3)",
            border: isLive ? "1px solid rgba(111,143,184,0.45)" : "1px solid transparent",
          }}>
          {isLive && <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: "var(--info)" }}></span>}
          LIVE
        </button>
      </div>

      <button className="h-7 px-2 ml-1 text-[11px] font-mono text-text-2 rounded-sm2"
        style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}>
        ⌘K
      </button>
    </div>
  );
}

function Card({ title, sub, right, children, className = "" }) {
  return (
    <div className={`bg-surface border border-default rounded-card ${className}`}>
      {(title || right) && (
        <div className="flex items-center justify-between px-5 pt-4 pb-3" style={{ borderBottom: "1px solid var(--border-soft)" }}>
          <div className="flex items-baseline gap-3">
            <h2 className="m-0 font-serif italic text-[22px] tracking-tight text-text" style={{ fontWeight: 500 }}>{title}</h2>
            {sub && <span className="text-[11px] font-mono text-text-3">{sub}</span>}
          </div>
          {right && <div className="flex items-center gap-2">{right}</div>}
        </div>
      )}
      {children}
    </div>
  );
}

function Stat({ label, value, sub, tone }) {
  const colors = {
    pos:  "#7ab97c",
    neg:  "var(--danger)",
    neu:  "var(--text)",
    gold: "var(--gold)",
  };
  return (
    <div className="px-5 py-4" style={{ borderRight: "1px solid var(--border-soft)" }}>
      <div className="text-[10px] font-mono tracking-[0.18em] text-text-3">{label}</div>
      <div className="mt-1 text-[24px] font-mono tabular-nums leading-tight" style={{ color: colors[tone] || colors.neu, fontWeight: 500 }}>{value}</div>
      {sub && <div className="text-[10px] font-mono text-text-3 mt-0.5">{sub}</div>}
    </div>
  );
}

function EquityCurve({ isLive }) {
  const pts = useRef(null);
  if (!pts.current) {
    const n = 80; const arr = []; let v = 100;
    for (let i = 0; i < n; i++) {
      const shock = (i > 32 && i < 44) ? -1.4 : 0;
      v += (Math.sin(i / 4) * 0.6) + (Math.cos(i / 9) * 0.4) + shock + 0.15;
      arr.push(v);
    }
    pts.current = arr;
  }
  const arr = pts.current;
  const min = Math.min(...arr), max = Math.max(...arr);
  const w = 100, h = 100;
  const path = arr.map((y, i) => {
    const x = (i / (arr.length - 1)) * w;
    const yy = h - ((y - min) / (max - min)) * h;
    return `${i === 0 ? "M" : "L"}${x.toFixed(2)},${yy.toFixed(2)}`;
  }).join(" ");

  return (
    <div className="relative h-[140px] overflow-hidden" style={{ background: "var(--bg)" }}>
      <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" className="absolute inset-0 w-full h-full">
        {[20, 40, 60, 80].map(y => (
          <line key={y} x1="0" x2={w} y1={y} y2={y} stroke="#2a2618" strokeWidth="0.3"/>
        ))}
        <rect x={(32/(arr.length-1))*w} width={((44-32)/(arr.length-1))*w} y="0" height={h} fill="#c8443a" opacity="0.10"/>
        <path d={path} fill="none" stroke="#d4a547" strokeWidth="0.9" vectorEffect="non-scaling-stroke"/>
        <path d={`${path} L${w},${h} L0,${h} Z`} fill="url(#g)" opacity="0.30"/>
        <defs>
          <linearGradient id="g" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#d4a547" stopOpacity="0.5"/>
            <stop offset="100%" stopColor="#d4a547" stopOpacity="0"/>
          </linearGradient>
        </defs>
        {isLive && <circle cx={w-0.5} cy={h - ((arr[arr.length-1] - min) / (max - min)) * h} r="1.2" fill="#d4a547"><animate attributeName="r" values="1.2;2.4;1.2" dur="1.4s" repeatCount="indefinite"/></circle>}
      </svg>
      <div className="absolute top-2 left-4 text-[10px] font-mono tracking-[0.18em] text-text-3">EQUITY · pnl%</div>
      <div className="absolute top-2 right-4 text-[10px] font-mono text-text-3 tabular-nums">10:14:14 — flash event ↓</div>
      <div className="absolute bottom-2 left-4 text-[10px] font-mono text-text-3 tabular-nums">−0.0%</div>
      <div className="absolute bottom-2 right-4 text-[10px] font-mono tabular-nums text-gold">+6.42%</div>
    </div>
  );
}

function SummaryCard({ isLive }) {
  return (
    <Card
      title="Summary"
      sub="run · abc1234de7f6"
      right={
        <div className="flex items-center gap-2">
          <span className="text-[10px] font-mono text-text-3 tracking-[0.16em]">STRATEGY</span>
          <code className="text-[11px] font-mono text-text">mean-reversion-v3</code>
          <span className="px-1.5 py-0.5 text-[9px] font-mono tracking-[0.18em] rounded-sm2"
            style={{ color: "var(--gold)", background: "var(--gold-bg)", border: "1px solid var(--gold-soft)" }}>PASS</span>
        </div>
      }
    >
      <EquityCurve isLive={isLive}/>
      <div className="grid grid-cols-4" style={{ borderTop: "1px solid var(--border-soft)" }}>
        <Stat label="PNL"           value="+$8,420"   sub="+6.42%"      tone="gold"/>
        <Stat label="MAX DRAWDOWN"  value="−2.81%"    sub="@ 10:14:14"  tone="neg"/>
        <Stat label="SHARPE"        value="2.14"      sub="annualized"  tone="neu"/>
        <Stat label="WIN RATE"      value="62.5%"     sub="5/8 trades"  tone="neu"/>
      </div>
    </Card>
  );
}

function ActionPill({ a }) {
  const styles = {
    BUY:  { color: "var(--gold)",   bg: "var(--gold-bg)", bd: "var(--gold-soft)" },
    SELL: { color: "var(--danger)", bg: "rgba(200,68,58,0.10)", bd: "rgba(200,68,58,0.45)" },
    HOLD: { color: "var(--text-3)", bg: "transparent", bd: "var(--border)" },
  }[a];
  return (
    <span className="px-1.5 py-0.5 text-[10px] font-mono tracking-[0.18em] rounded-sm2"
      style={{ color: styles.color, background: styles.bg, border: `1px solid ${styles.bd}` }}>
      {a}
    </span>
  );
}

function DecisionsTable({ decisions, onJump, focusedIdx }) {
  return (
    <Card title="Decisions" sub={`${decisions.length} steps · 5 trades`}
      right={<span className="text-[10px] font-mono text-text-3">click row → filter dock</span>}>
      <div className="overflow-hidden">
        <table className="w-full text-[11px] font-mono">
          <thead style={{ background: "var(--surface-elev)" }}>
            <tr className="text-left text-text-3">
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-10">#</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-32">TIMESTAMP</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-16">ACTION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-28">CONVICTION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px]">JUSTIFICATION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-24 text-right">PNL</th>
            </tr>
          </thead>
          <tbody>
            {decisions.map(d => {
              const focus = d.i === focusedIdx;
              return (
                <tr key={d.i} onClick={() => onJump(d.i)}
                  className="cursor-pointer transition-colors"
                  style={{
                    borderTop: "1px solid var(--border-soft)",
                    background: focus ? "var(--gold-bg)" : "transparent",
                  }}
                  onMouseEnter={(e) => { if (!focus) e.currentTarget.style.background = "var(--surface-hover)"; }}
                  onMouseLeave={(e) => { if (!focus) e.currentTarget.style.background = "transparent"; }}>
                  <td className="px-4 py-2 tabular-nums text-text-3">{d.i}</td>
                  <td className="px-4 py-2 tabular-nums text-text-2">{d.t}</td>
                  <td className="px-4 py-2"><ActionPill a={d.action}/></td>
                  <td className="px-4 py-2 tabular-nums text-text">
                    <div className="flex items-center gap-2">
                      <span className="w-9 text-right">{(d.conv * 100).toFixed(0)}%</span>
                      <span className="flex-1 h-1 rounded-full overflow-hidden max-w-[70px]" style={{ background: "var(--border)" }}>
                        <span className="block h-full" style={{ width: `${d.conv * 100}%`, background: "var(--gold)" }}></span>
                      </span>
                    </div>
                  </td>
                  <td className="px-4 py-2 text-text-2 truncate max-w-[1px]">{d.just}</td>
                  <td className="px-4 py-2 tabular-nums text-right"
                    style={{ color: d.pnl > 0 ? "var(--gold)" : d.pnl < 0 ? "var(--danger)" : "var(--text-4)" }}>
                    {d.pnl === 0 ? "—" : (d.pnl > 0 ? `+$${d.pnl.toLocaleString()}` : `−$${Math.abs(d.pnl).toLocaleString()}`)}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </Card>
  );
}

function ReviewPanel() {
  return (
    <Card title="Review" sub="supervisor · claude-haiku-4-5"
      right={<span className="px-1.5 py-0.5 text-[9px] font-mono tracking-[0.18em] rounded-sm2"
        style={{ color: "var(--warn)", background: "rgba(219,146,48,0.10)", border: "1px solid rgba(219,146,48,0.40)" }}>2 NOTES</span>}>
      <div className="p-5 space-y-3 text-[13px] leading-relaxed text-text-2">
        <p className="font-serif italic text-[15px] text-text" style={{ fontWeight: 400 }}>
          Strategy executed mean-reversion logic correctly under the 14:14 liquidity shock, scaling into the dislocation at half-Kelly. Sharpe (2.14) within bounds; drawdown contained.
        </p>
        <div className="pt-3 mt-3 grid grid-cols-[60px_1fr] gap-x-3 gap-y-2 text-[12px] font-mono" style={{ borderTop: "1px solid var(--border-soft)" }}>
          <span className="text-text-3 tracking-[0.16em] text-[10px] pt-0.5">NOTE 1</span>
          <span className="text-text">Decision <span className="text-gold">#14</span> used stale book snapshot (Δ 320ms). Consider tighter freshness gate.</span>
          <span className="text-text-3 tracking-[0.16em] text-[10px] pt-0.5">NOTE 2</span>
          <span className="text-text">Tool <code className="text-warn">run_backtest</code> called twice in adjacent steps — possible cache miss.</span>
        </div>
      </div>
    </Card>
  );
}

function ToastStack({ toasts }) {
  return (
    <div className="fixed top-16 right-3 z-[60] flex flex-col gap-1 items-end pointer-events-none">
      {toasts.map(t => (
        <div key={t.id} className="px-3 py-1.5 rounded-sm2 text-[11px] font-mono text-text"
          style={{
            background: "var(--surface-elev)",
            border: `1px solid ${t.tone === "danger" ? "var(--danger)" : "var(--border-strong)"}`,
            boxShadow: "0 14px 40px rgba(0,0,0,0.5)",
            color: t.tone === "danger" ? "#ec9a92" : "var(--text)",
          }}>
          <span className="mr-2" style={{ color: t.tone === "danger" ? "var(--danger)" : "var(--gold)" }}>{t.tone === "danger" ? "◼" : "›"}</span>{t.msg}
        </div>
      ))}
    </div>
  );
}

// ---------- App ----------
const KIND_TINTS = {
  agent:      { c: "#a39a85", l: "AGENT" },
  model:      { c: "#7dd3fc", l: "MODEL" },
  tool:       { c: "#6ee7b7", l: "TOOL"  },
  supervisor: { c: "#d4a547", l: "SUPER" },
  artifact:   { c: "#a78bfa", l: "ARTIF" },
};

function App() {
  const { spans, decisions } = window.MOCK;

  const [isLive, setLive]          = uS(true);
  const [dockOpen, setDockOpen]    = uS(true);
  const [height, setHeight]        = uS("working");
  const [lastHeight, setLastHeight]= uS("working");
  const [selected, setSelected]    = uS("s6");
  const [stripState, setStripState]= uS("blue");
  const [autoScroll, setAutoScroll]= uS(true);
  const [liveDur, setLiveDur]      = uS(43);
  const [toasts, setToasts]        = uS([]);
  const [focusedDecision, setFD]   = uS(14);

  // Filter state
  const [query, setQuery]          = uS("");
  const [kinds, setKinds]          = uS(() => new Set()); // empty = all
  const [decisionFilter, setDF]    = uS("all");

  uE(() => { setStripState(isLive ? "blue" : "green"); }, [isLive]);

  uE(() => {
    if (!isLive) return;
    const id = setInterval(() => setLiveDur(d => d + 1), 1000);
    return () => clearInterval(id);
  }, [isLive]);

  uE(() => {
    const h = (e) => {
      if (e.key === "F12") { e.preventDefault(); setDockOpen(o => !o); }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, []);

  const toast = (msg, tone) => {
    const id = Math.random().toString(36).slice(2);
    setToasts(t => [...t, { id, msg, tone }]);
    setTimeout(() => setToasts(t => t.filter(x => x.id !== id)), 2800);
  };

  const expandDock   = () => { setDockOpen(true); setHeight(lastHeight); };
  const minimizeDock = () => { setLastHeight(height); setDockOpen(false); };
  const changeHeight = (k) => { setHeight(k); setLastHeight(k); if (!dockOpen) setDockOpen(true); };
  const popOut = () => toast("Would navigate to /eval-runs/abc1234de7f6 (dedicated route)");

  const toggleKind = (k) => setKinds(prev => {
    const next = new Set(prev);
    next.has(k) ? next.delete(k) : next.add(k);
    return next;
  });

  // Apply filters
  const filteredSpans = uM(() => {
    const q = query.trim().toLowerCase();
    return spans.filter(s => {
      if (kinds.size > 0 && !kinds.has(s.kind)) return false;
      if (decisionFilter !== "all" && String(s.decision_idx || "") !== String(decisionFilter)) return false;
      if (!q) return true;
      // pseudo-syntax: title:x  model:x  tool:x
      const tokens = q.split(/\s+/);
      return tokens.every(tok => {
        if (tok.startsWith("title:"))    return s.name.toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("model:"))    return (s.model||"").toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("tool:"))     return s.kind === "tool" && s.name.toLowerCase().includes(tok.slice(5));
        if (tok.startsWith("agent:"))    return s.kind === "agent" && s.name.toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("decision:")) return String(s.decision_idx||"") === tok.slice(9);
        return s.name.toLowerCase().includes(tok)
            || (s.model||"").toLowerCase().includes(tok)
            || (s.provider||"").toLowerCase().includes(tok)
            || String(s.decision_idx||"").includes(tok);
      });
    });
  }, [spans, query, kinds, decisionFilter, stripState]);

  // Strip state acts as a soft display filter on the operator's mental model — keep state separate from kind/title filters.
  // (Switching it tints the strip + flame-graph axis line; it does NOT remove rows.)

  const handleHalt = () => {
    toast("HALT signal sent · all open orders cancelled · strategy paused", "danger");
    setStripState("red");
    setLive(false);
  };

  // Sync decision filter <-> focused decision (clicking a decision row filters the dock to its spans)
  const onJumpDecision = (i) => {
    setFD(i);
    setDF(String(i));
    if (!dockOpen) expandDock();
    toast(`Filtered dock to decision #${i}`);
  };

  // Current span shown on the floating strip — in LIVE mode it's the newest in-flight leaf;
  // in post-hoc mode it tracks the selected span so operators see what they're inspecting.
  const currentSpan = uM(() => {
    let s;
    if (isLive) {
      const t = liveDur * 70 + 200;
      const candidates = spans.filter(x => x.start <= t && x.start + x.dur > t);
      // prefer deepest (leaf) — the actual work being done
      s = candidates.sort((a, b) => b.depth - a.depth || b.start - a.start)[0];
      if (!s) s = spans.filter(x => x.start <= t).sort((a, b) => b.start - a.start)[0];
    } else {
      s = spans.find(x => x.id === selected);
    }
    if (!s) return null;
    const k = KIND_TINTS[s.kind] || KIND_TINTS.agent;
    const elapsed = isLive ? `${Math.max(0, (liveDur * 70 + 200) - s.start)}ms` : `${s.dur}ms`;
    return { name: s.name, color: k.c, label: k.l, elapsed };
  }, [isLive, liveDur, spans, selected]);

  return (
    <div className="h-screen w-screen flex flex-col overflow-hidden bg-bg text-text">
      <TopBar isLive={isLive} setLive={setLive}/>

      {/* Page body */}
      <div className="flex-1 min-h-0 overflow-auto px-6 py-6">
        <div className="max-w-[1400px] mx-auto">
          <div className="mb-6 flex items-baseline gap-4">
            <h1 className="font-serif text-[34px] leading-none text-text tracking-tight" style={{ fontWeight: 500 }}>
              <span className="italic">Run</span> <span className="font-mono text-[26px] text-text-2 tracking-normal">abc1234de7f6</span>
            </h1>
            <div className="text-[12px] font-mono text-text-3">
              started <span className="text-text-2">2026-05-17 10:13:31Z</span>
              <span className="text-text-4 mx-2">·</span>
              budget <span className="text-text-2">$0.18 / $1.00</span>
              <span className="text-text-4 mx-2">·</span>
              commit <span className="text-text-2">7f2b1ad</span>
            </div>
          </div>

          <div className="grid grid-cols-12 gap-5">
            <div className="col-span-12 lg:col-span-8 space-y-5">
              <SummaryCard isLive={isLive}/>
              <DecisionsTable decisions={decisions} focusedIdx={focusedDecision} onJump={onJumpDecision}/>
            </div>

            <div className="col-span-12 lg:col-span-4 space-y-5">
              <Card title="Meta" sub="run config">
                <div className="p-4 text-[11px] font-mono space-y-1.5">
                  {[
                    ["run.id",     "abc1234de7f6"],
                    ["strategy",   "mean-reversion-v3"],
                    ["scenario",   "flash-crash-2024-08"],
                    ["seed",       "0x9c44a1"],
                    ["mode",       "paper · alpaca"],
                    ["region",     "us-east"],
                    ["budget",     "$0.18 / $1.00 (18%)"],
                  ].map(([k, v]) => (
                    <div key={k} className="flex items-baseline gap-3">
                      <span className="w-[80px] shrink-0 text-[10px] uppercase tracking-[0.14em] text-text-3">{k}</span>
                      <span className="text-text tabular-nums break-all">{v}</span>
                    </div>
                  ))}
                </div>
              </Card>

              <ReviewPanel/>
            </div>
          </div>

          {/* breathing room above the floating strip / dock */}
          <div style={{ height: dockOpen ? 8 : 64 }}></div>
        </div>
      </div>

      {/* Dock — renders at bottom when open; otherwise the floating Strip shows. */}
      {dockOpen ? (
        <Dock
          height={height} onHeight={changeHeight}
          onMinimize={minimizeDock} onPopOut={popOut}
          isLive={isLive} liveDuration={liveDur}
          spans={spans} selected={selected} onSelect={setSelected}
          autoScroll={autoScroll} setAutoScroll={setAutoScroll}
          onToast={toast} decisions={decisions}
          query={query} setQuery={setQuery}
          kinds={kinds} toggleKind={toggleKind}
          status={stripState} setStatus={setStripState}
          decisionFilter={decisionFilter} setDecisionFilter={setDF}
          filteredSpans={filteredSpans}
          onHalt={handleHalt}
        />
      ) : (
        <Strip state={stripState} liveDuration={liveDur} isLive={isLive}
          currentSpan={currentSpan}
          onExpand={expandDock} onPopOut={popOut}/>
      )}

      <ToastStack toasts={toasts}/>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App/>);
