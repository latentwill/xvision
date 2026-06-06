// Main app — XVN folio-dark. /eval-runs/abc1234de7f6 with trace observability layers.
const { useState: uS, useEffect: uE, useMemo: uM, useRef } = React;

// Tweak defaults — let the user demo different concurrent-eval counts and states.
const TWEAK_DEFAULTS = /*EDITMODE-BEGIN*/{
  "siblingCount": 3,
  "siblingState": "mixed"
}/*EDITMODE-END*/;

// Mock pool of concurrent evals running in parallel on the cluster.
// `short` is the operator-readable identifier — strategy abbreviation · scenario abbreviation.
// Slug (hex id) is kept for the URL/route but never shown as the primary handle in UI.
const SIBLING_POOL = [
  { id: "b22a91", slug: "b22a91", short: "mom·opex",   strategy: "momentum-breakout-v2", scenario: "opex-2023-09",        status: "eval",   elapsed: "0:38", spans: 31, cost: "$0.11" },
  { id: "c781ef", slug: "c781ef", short: "pair·china", strategy: "pairs-stat-arb-v7",    scenario: "china-2015-08",       status: "eval",   elapsed: "1:22", spans: 64, cost: "$0.34" },
  { id: "d041aa", slug: "d041aa", short: "vol·vix",    strategy: "vol-target-v1",        scenario: "vix-spike-2018-02",   status: "warn",   elapsed: "0:51", spans: 42, cost: "$0.19" },
  { id: "e9183c", slug: "e9183c", short: "mr·calm",    strategy: "mean-reversion-v3",    scenario: "calm-monday-2024-06", status: "pass",   elapsed: "3.4s", spans: 47, cost: "$0.18" },
  { id: "f5520b", slug: "f5520b", short: "liq·fed",    strategy: "liquidity-vwap-v4",    scenario: "fed-decision-2023-09",status: "error",  elapsed: "0:12", spans: 8,  cost: "$0.04" },
  { id: "g8f33d", slug: "g8f33d", short: "mom·aapl",   strategy: "momentum-breakout-v2", scenario: "earnings-aapl-q3",    status: "eval",   elapsed: "0:07", spans: 4,  cost: "$0.02" },
  { id: "h2c019", slug: "h2c019", short: "mr·2010",    strategy: "mean-reversion-v3",    scenario: "flash-crash-2010-05", status: "queued", elapsed: "—",   spans: 0,  cost: "$0.00" },
  { id: "j6ab47", slug: "j6ab47", short: "pair·opex",  strategy: "pairs-stat-arb-v7",    scenario: "opex-2024-03",        status: "eval",   elapsed: "0:29", spans: 22, cost: "$0.09" },
];

// ---------- Topbar ----------
// Brand mark: filled square + wordmark, aligned with the breadcrumb baseline.
function BrandMark() {
  return (
    <div className="flex items-center gap-2 leading-none">
      <span
        aria-hidden="true"
        className="inline-block"
        style={{ width: 14, height: 14, background: "var(--gold)", borderRadius: 2 }}
      />
      <span
        className="font-mono text-text"
        style={{ fontSize: 14, fontWeight: 700, letterSpacing: "0.18em" }}
      >
        XVN
      </span>
    </div>
  );
}

function TopBar({ isLive }) {
  return (
    <div className="h-12 px-4 flex items-center gap-3 shrink-0" style={{ background: "var(--surface-sidebar)", borderBottom: "1px solid var(--border)" }}>
      <BrandMark/>
      <span className="text-text-4 mx-1">/</span>
      <span className="text-[11px] font-mono tracking-[0.18em] text-text-3 uppercase">eval runs</span>
      <span className="text-text-4">/</span>
      <code className="text-[12px] font-mono text-text-2 tracking-normal">abc1234de7f6</code>

      <div className="ml-auto flex items-center gap-1.5 px-2.5 py-1 rounded-sm2"
        style={{
          background: isLive ? "rgba(95,168,255,0.12)" : "var(--gold-bg)",
          border: `1px solid ${isLive ? "rgba(95,168,255,0.40)" : "var(--gold-soft)"}`,
        }}>
        <span className="w-1.5 h-1.5 rounded-full animate-pulse" style={{ background: isLive ? "var(--info)" : "var(--gold)" }}></span>
        <span className="text-[10px] font-mono tracking-[0.16em]" style={{ color: isLive ? "var(--info)" : "var(--gold)" }}>
          {isLive ? "EVAL RUNNING" : "EVAL COMPLETED"}
        </span>
      </div>
    </div>
  );
}

// Metadata chip — reused for strategy / scenario / agent under the H1.
// "label · value [→]" — uppercase tracked label, monospaced value, optional trailing chevron.
function MetaChip({ label, value, tone = "neutral", onClick, chevron = true }) {
  const toneStyles = {
    neutral: { color: "var(--text)",  bd: "var(--border)",       bg: "var(--surface-elev)", lbl: "var(--text-3)" },
    gold:    { color: "var(--gold)",  bd: "var(--gold-soft)",    bg: "var(--gold-bg)",      lbl: "var(--gold-soft)" },
    info:    { color: "var(--info)",  bd: "rgba(95,168,255,0.40)", bg: "rgba(95,168,255,0.10)", lbl: "var(--info)" },
  }[tone] || {};
  return (
    <button
      onClick={onClick}
      className="inline-flex items-center gap-2 transition-colors"
      style={{
        height: 28,
        padding: "0 10px",
        background: toneStyles.bg,
        border: `1px solid ${toneStyles.bd}`,
        borderRadius: 4,
        cursor: onClick ? "pointer" : "default",
      }}
    >
      <span className="font-mono uppercase" style={{ fontSize: 10, letterSpacing: "0.16em", color: toneStyles.lbl, fontWeight: 600 }}>
        {label}
      </span>
      <span className="font-mono" style={{ fontSize: 12, color: toneStyles.color, fontWeight: 500 }}>
        {value}
      </span>
      {chevron && (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden="true" style={{ opacity: 0.5, marginLeft: 1 }}>
          <path d="M4.5 2.5L8 6l-3.5 3.5" stroke={toneStyles.color} strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
      )}
    </button>
  );
}

function Card({ title, sub, right, children, className = "" }) {
  return (
    <div className={`bg-surface border border-default rounded-card ${className}`}>
      {(title || right) && (
        <div className="flex items-center justify-between px-5 pt-4 pb-3" style={{ borderBottom: "1px solid var(--border-soft)" }}>
          <div className="flex items-baseline gap-3">
            <h2 className="m-0 font-serif text-[22px] tracking-tight text-text" style={{ fontWeight: 500 }}>{title}</h2>
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
        <rect x={(32/(arr.length-1))*w} width={((44-32)/(arr.length-1))*w} y="0" height={h} fill="#ff4d4d" opacity="0.10"/>
        <path d={path} fill="none" stroke="#00e676" strokeWidth="0.9" vectorEffect="non-scaling-stroke"/>
        <path d={`${path} L${w},${h} L0,${h} Z`} fill="url(#g)" opacity="0.30"/>
        <defs>
          <linearGradient id="g" x1="0" x2="0" y1="0" y2="1">
            <stop offset="0%" stopColor="#00e676" stopOpacity="0.5"/>
            <stop offset="100%" stopColor="#00e676" stopOpacity="0"/>
          </linearGradient>
        </defs>
        {isLive && <circle cx={w-0.5} cy={h - ((arr[arr.length-1] - min) / (max - min)) * h} r="1.2" fill="#00e676"><animate attributeName="r" values="1.2;2.4;1.2" dur="1.4s" repeatCount="indefinite"/></circle>}
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
      right={
        <span className="px-1.5 py-0.5 text-[9px] font-mono tracking-[0.18em] rounded-sm2"
          style={{ color: "var(--gold)", background: "var(--gold-bg)", border: "1px solid var(--gold-soft)" }}>PASS</span>
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
  const meta = {
    BUY:  {
      label: "BUY",
      fg: "#001A0A",
      bg: "var(--gold)",
      bd: "var(--gold)",
      glyph: (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <path d="M6 9.5V2.5M6 2.5L2.5 6M6 2.5L9.5 6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
      ),
    },
    SELL: {
      label: "SELL",
      fg: "#1A0000",
      bg: "var(--danger)",
      bd: "var(--danger)",
      glyph: (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <path d="M6 2.5V9.5M6 9.5L2.5 6M6 9.5L9.5 6" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round"/>
        </svg>
      ),
    },
    HOLD: {
      label: "HOLD",
      fg: "var(--text-2)",
      bg: "transparent",
      bd: "var(--border-strong)",
      glyph: (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <path d="M3 6H9" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
        </svg>
      ),
    },
    CLOSE: {
      label: "CLOSE",
      fg: "var(--warn)",
      bg: "rgba(255, 176, 32, 0.10)",
      bd: "rgba(255, 176, 32, 0.45)",
      glyph: (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" aria-hidden="true">
          <path d="M3.5 3.5L8.5 8.5M8.5 3.5L3.5 8.5" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round"/>
        </svg>
      ),
    },
  }[a] || {
    label: a, fg: "var(--text-2)", bg: "transparent", bd: "var(--border-strong)", glyph: null,
  };
  return (
    <span
      className="inline-flex items-center gap-1.5 font-mono"
      style={{
        color: meta.fg,
        background: meta.bg,
        border: `1px solid ${meta.bd}`,
        padding: "3px 7px 3px 6px",
        borderRadius: 3,
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.1em",
        lineHeight: 1,
        minWidth: 50,
        justifyContent: "center",
      }}
    >
      {meta.glyph}
      <span>{meta.label}</span>
    </span>
  );
}

function PhaseChip({ phase }) {
  if (phase === "filtered") {
    return (
      <span
        className="inline-flex items-center gap-1.5 font-mono"
        style={{
          color: "var(--text-3)",
          background: "transparent",
          border: "1px solid var(--border-strong)",
          padding: "3px 8px",
          borderRadius: 3,
          fontSize: 10,
          fontWeight: 500,
          letterSpacing: "0.12em",
          lineHeight: 1,
        }}
      >
        <span
          aria-hidden="true"
          style={{
            width: 5, height: 5, borderRadius: "50%",
            border: "1px solid var(--text-3)",
            background: "transparent",
          }}
        />
        FILTERED
      </span>
    );
  }
  return (
    <span
      className="inline-flex items-center gap-1.5 font-mono"
      style={{
        color: "var(--text)",
        background: "var(--surface-elev)",
        border: "1px solid var(--border-strong)",
        padding: "3px 8px",
        borderRadius: 3,
        fontSize: 10,
        fontWeight: 600,
        letterSpacing: "0.12em",
        lineHeight: 1,
      }}
    >
      <span
        aria-hidden="true"
        style={{
          width: 5, height: 5, borderRadius: "50%",
          background: "var(--gold)",
        }}
      />
      ENGAGED
    </span>
  );
}

function DecisionsTable({ decisions, onJump, focusedIdx }) {
  const [search, setSearch] = uS("");
  const [actionFilter, setActionFilter] = uS("all"); // all | BUY | SELL | HOLD | FILTERED
  const [sortKey, setSortKey] = uS("time-asc");

  // Counts per action + filtered phase
  const counts = uM(() => ({
    all:      decisions.length,
    BUY:      decisions.filter(d => d.action === "BUY").length,
    SELL:     decisions.filter(d => d.action === "SELL").length,
    HOLD:     decisions.filter(d => d.action === "HOLD").length,
    FILTERED: decisions.filter(d => d.phase === "filtered").length,
  }), [decisions]);

  const filteredView = uM(() => {
    let out = decisions;
    if (actionFilter === "FILTERED") {
      out = out.filter(d => d.phase === "filtered");
    } else if (actionFilter !== "all") {
      out = out.filter(d => d.action === actionFilter);
    }
    const q = (search || "").toLowerCase().trim();
    if (q) {
      out = out.filter(d => {
        const hay = [String(d.i), d.t || "", d.phase || "", d.action || "", d.just || ""].join(" ").toLowerCase();
        return hay.includes(q);
      });
    }
    const cp = [...out];
    if (sortKey === "time-asc")  cp.sort((a, b) => a.i - b.i);
    if (sortKey === "time-desc") cp.sort((a, b) => b.i - a.i);
    if (sortKey === "conv-desc") cp.sort((a, b) => (b.conv || 0) - (a.conv || 0));
    if (sortKey === "pnl-desc")  cp.sort((a, b) => (b.pnl || 0) - (a.pnl || 0));
    return cp;
  }, [decisions, search, actionFilter, sortKey]);

  const engagedCount = decisions.filter(d => d.phase !== "filtered").length;

  // Filter pill row — colored dot + label + count (from eval-focus design language).
  const PILLS = [
    { k: "all",      label: "All",      dotColor: "var(--text-2)",  activeBg: "var(--surface-elev)", activeBd: "var(--border-strong)", activeFg: "var(--text)",   filled: true  },
    { k: "BUY",      label: "Buy",      dotColor: "var(--gold)",    activeBg: "var(--gold-bg)",      activeBd: "var(--gold-soft)",     activeFg: "var(--gold)",   filled: true  },
    { k: "SELL",     label: "Sell",     dotColor: "var(--danger)",  activeBg: "rgba(255,77,77,0.10)",activeBd: "rgba(255,77,77,0.45)", activeFg: "var(--danger)", filled: true  },
    { k: "HOLD",     label: "Hold",     dotColor: "var(--text-3)",  activeBg: "var(--surface-elev)", activeBd: "var(--border-strong)", activeFg: "var(--text)",   filled: true  },
    { k: "FILTERED", label: "Filtered", dotColor: "var(--text-3)",  activeBg: "transparent",         activeBd: "var(--text-3)",        activeFg: "var(--text-2)", filled: false },
  ];

  return (
    <Card title="Decisions" sub={`${filteredView.length} of ${decisions.length} steps · ${engagedCount} engaged`}
      right={<span className="text-[10px] font-mono text-text-3">click row → filter dock</span>}>

      {/* Toolbar — search + sort */}
      <div className="px-5 pt-4 pb-3 flex items-center gap-3" style={{ borderBottom: "1px solid var(--border-soft)" }}>
        <div className="flex items-center gap-2 px-3 h-8 rounded-sm2 flex-1 max-w-[320px]"
          style={{ background: "var(--surface-elev)", border: "1px solid var(--border)" }}>
          <Icon name="search" size={13} color="var(--text-3)" />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search decisions… (id, justification, action)"
            spellCheck={false}
            className="flex-1 bg-transparent border-none outline-none text-text text-[12.5px] font-mono"
            style={{ minWidth: 0 }}
          />
          {search && (
            <button onClick={() => setSearch("")} className="text-text-3 hover:text-text text-[14px] leading-none px-1" aria-label="Clear">×</button>
          )}
        </div>

        <div className="ml-auto flex items-center gap-2">
          <span className="text-[10px] font-mono tracking-[0.16em] text-text-3 uppercase">Sort</span>
          <select
            value={sortKey}
            onChange={(e) => setSortKey(e.target.value)}
            className="h-8 px-2 text-[12px] font-mono text-text rounded-sm2"
            style={{ background: "var(--surface-elev)", border: "1px solid var(--border)", outline: "none" }}
          >
            <option value="time-asc">Time ↑ (oldest first)</option>
            <option value="time-desc">Time ↓ (newest first)</option>
            <option value="conv-desc">Conviction high → low</option>
            <option value="pnl-desc">PnL high → low</option>
          </select>
        </div>
      </div>

      {/* Filter pill row */}
      <div className="px-5 py-3 flex items-center gap-2 flex-wrap" style={{ borderBottom: "1px solid var(--border-soft)" }}>
        {PILLS.map(p => {
          const isActive = actionFilter === p.k;
          return (
            <button
              key={p.k}
              onClick={() => setActionFilter(p.k)}
              className="inline-flex items-center gap-2 h-7 px-2.5 rounded-full text-[11.5px] font-mono transition-colors"
              style={{
                background: isActive ? p.activeBg : "transparent",
                border: `1px solid ${isActive ? p.activeBd : "var(--border)"}`,
                color: isActive ? p.activeFg : "var(--text-2)",
              }}
            >
              <span
                aria-hidden="true"
                style={{
                  width: 6, height: 6, borderRadius: "50%",
                  background: p.filled ? p.dotColor : "transparent",
                  border: p.filled ? "none" : `1px solid ${p.dotColor}`,
                }}
              />
              <span>{p.label}</span>
              <span
                className="px-1.5 h-[16px] inline-flex items-center justify-center rounded-sm tabular-nums"
                style={{
                  fontSize: 10,
                  color: isActive ? p.activeFg : "var(--text-3)",
                  background: "rgba(0,0,0,0.35)",
                }}
              >{counts[p.k]}</span>
            </button>
          );
        })}
      </div>

      {/* Decision timeline strip — one square per decision, with timestamp ticks */}
      <DecisionTimeline
        decisions={decisions}
        focusedIdx={focusedIdx}
        onJump={onJump}
        activeFilter={actionFilter}
      />

      <div className="overflow-hidden">
        <table className="w-full text-[11px] font-mono">
          <thead style={{ background: "var(--surface-elev)" }}>
            <tr className="text-left text-text-3">
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-10">#</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-32">TIMESTAMP</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-24">PHASE</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-16">ACTION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-28">CONVICTION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px]">JUSTIFICATION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-24 text-right">PNL</th>
            </tr>
          </thead>
          <tbody>
            {filteredView.length === 0 ? (
              <tr>
                <td colSpan="7" className="px-4 py-8 text-center text-text-3">
                  No decisions match these filters.
                </td>
              </tr>
            ) : filteredView.map(d => {
              const focus = d.i === focusedIdx;
              const isFiltered = d.phase === "filtered";
              return (
                <tr key={d.i} onClick={() => onJump(d.i)}
                  className="cursor-pointer transition-colors"
                  style={{
                    borderTop: "1px solid var(--border-soft)",
                    background: focus ? "var(--gold-bg)" : "transparent",
                    opacity: isFiltered ? 0.78 : 1,
                  }}
                  onMouseEnter={(e) => { if (!focus) e.currentTarget.style.background = "var(--surface-hover)"; }}
                  onMouseLeave={(e) => { if (!focus) e.currentTarget.style.background = "transparent"; }}>
                  <td className="px-4 py-2 tabular-nums text-text-3">{d.i}</td>
                  <td className="px-4 py-2 tabular-nums text-text-2">{d.t}</td>
                  <td className="px-4 py-2"><PhaseChip phase={d.phase}/></td>
                  <td className="px-4 py-2">
                    {isFiltered ? <span className="text-text-4">—</span> : <ActionPill a={d.action}/>}
                  </td>
                  <td className="px-4 py-2 tabular-nums text-text">
                    {isFiltered ? (
                      <span className="text-text-4">—</span>
                    ) : (
                      <div className="flex items-center gap-2">
                        <span className="w-9 text-right">{(d.conv * 100).toFixed(0)}%</span>
                        <span className="flex-1 h-1 rounded-full overflow-hidden max-w-[70px]" style={{ background: "var(--border)" }}>
                          <span className="block h-full" style={{ width: `${d.conv * 100}%`, background: "var(--gold)" }}></span>
                        </span>
                      </div>
                    )}
                  </td>
                  <td className="px-4 py-2 text-text-2 truncate max-w-[1px]">
                    {isFiltered ? <span className="text-text-4">—</span> : d.just}
                  </td>
                  <td className="px-4 py-2 tabular-nums text-right"
                    style={{ color: isFiltered ? "var(--text-4)" : (d.pnl > 0 ? "var(--gold)" : d.pnl < 0 ? "var(--danger)" : "var(--text-4)") }}>
                    {isFiltered ? "—" : (d.pnl === 0 ? "—" : (d.pnl > 0 ? `+$${d.pnl.toLocaleString()}` : `−$${Math.abs(d.pnl).toLocaleString()}`))}
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

// Decision density strip — scales to thousands of decisions.
// One thin column per decision, ordered by index. Color = action; filtered = short cool-gray
// half-height tick at the bottom (visually quiet, no-error read).
// Below: sparse time-axis ticks. Click → jump. Hover → tooltip with timestamp + action + i.
function DecisionTimeline({ decisions, focusedIdx, onJump, activeFilter }) {
  const containerRef = useRef(null);
  const [hover, setHover] = uS(null); // {x, i, t, action, phase}
  const [width, setWidth] = uS(800);

  uE(() => {
    if (!containerRef.current) return;
    const ro = new ResizeObserver(entries => {
      for (const e of entries) setWidth(e.contentRect.width);
    });
    ro.observe(containerRef.current);
    setWidth(containerRef.current.clientWidth);
    return () => ro.disconnect();
  }, []);

  if (!decisions.length) return null;
  const sorted = uM(() => [...decisions].sort((a, b) => a.i - b.i), [decisions]);
  const n = sorted.length;

  // Minimum tick width is 1px; cap visually around 6px so they don't look like big chunks at low counts.
  const tickW = Math.min(6, Math.max(1, Math.floor(width / n)));
  const gap   = tickW >= 4 ? 1 : 0;
  const slot  = tickW + gap;

  // Color mapping
  const COLOR_OPAQUE = {
    BUY:  "var(--gold)",
    SELL: "var(--danger)",
    HOLD: "var(--text-2)",
  };
  const COLOR_DIM = "var(--border-strong)";

  const isDim = (d) => {
    if (activeFilter === "all") return false;
    if (activeFilter === "FILTERED") return d.phase !== "filtered";
    return d.action !== activeFilter || d.phase === "filtered";
  };

  // Axis labels: pick ~6 evenly spaced indices, show seconds.ms portion only (data shares the same minute window).
  const labelIndices = uM(() => {
    const target = Math.min(6, n);
    if (target < 2) return [0];
    const step = (n - 1) / (target - 1);
    return Array.from({ length: target }, (_, k) => Math.round(k * step));
  }, [n]);

  const parse = (t) => {
    if (!t) return { prefix: "—", ss: "—" };
    const [hms, ms] = t.split(".");
    const parts = (hms || "").split(":");
    return {
      prefix: parts.slice(0, 2).join(":"),
      ss: `:${parts[2] || "—"}.${(ms || "").slice(0, 3)}`,
    };
  };

  const firstStamp = parse(sorted[0].t);
  const lastStamp  = parse(sorted[n - 1].t);

  const focusedSlot = uM(() => {
    const idx = sorted.findIndex(d => d.i === focusedIdx);
    return idx < 0 ? null : idx;
  }, [sorted, focusedIdx]);

  return (
    <div className="px-5 pt-4 pb-3" style={{ borderBottom: "1px solid var(--border-soft)" }}>
      <div className="flex items-center justify-between mb-2.5">
        <div className="flex items-baseline gap-2.5">
          <span className="text-[10px] font-mono tracking-[0.18em] text-text-3 uppercase">Density</span>
          <span className="text-[10.5px] font-mono text-text-3">
            <span className="text-text-2 tabular-nums">{n}</span> steps · <span className="tabular-nums">{firstStamp.prefix}</span> window
          </span>
        </div>
        <div className="flex items-center gap-3 text-[10px] font-mono text-text-3">
          <span className="inline-flex items-center gap-1.5">
            <span style={{ width: 9, height: 9, background: "var(--gold)" }}/>buy
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span style={{ width: 9, height: 9, background: "var(--danger)" }}/>sell
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span style={{ width: 9, height: 9, background: "var(--text-2)" }}/>hold
          </span>
          <span className="inline-flex items-center gap-1.5">
            <span style={{ width: 9, height: 4, background: "var(--border-strong)", alignSelf: "flex-end" }}/>filtered
          </span>
        </div>
      </div>

      {/* Strip */}
      <div
        ref={containerRef}
        className="relative"
        style={{ height: 36, background: "var(--surface-elev)", border: "1px solid var(--border-soft)", borderRadius: 3 }}
        onMouseLeave={() => setHover(null)}
      >

        {sorted.map((d, idx) => {
          const isFiltered = d.phase === "filtered";
          const dim = isDim(d);
          const color = dim ? COLOR_DIM : (isFiltered ? "var(--text-3)" : (COLOR_OPAQUE[d.action] || "var(--text-2)"));
          const isFocus = d.i === focusedIdx;
          // Full-height transparent hit target; the visible tick lives inside.
          // Engaged → full-height filled. Filtered → short stub anchored at bottom (quiet),
          // but the surrounding hit area is still full-height/clickable.
          return (
            <div
              key={d.i}
              onClick={() => onJump(d.i)}
              onMouseEnter={() => {
                setHover({
                  x: idx * slot + tickW / 2,
                  i: d.i, t: d.t, action: d.action, phase: d.phase, conv: d.conv, just: d.just,
                });
              }}
              style={{
                position: "absolute",
                left: idx * slot,
                top: 0,
                width: tickW,
                height: 36,
                cursor: "pointer",
                opacity: dim ? 0.45 : 1,
                transition: "opacity 0.15s",
              }}
            >
              {/* Visible tick */}
              <div
                style={{
                  position: "absolute",
                  left: 0,
                  width: tickW,
                  bottom: isFiltered ? 1 : 2,
                  height: isFiltered ? 10 : 32,
                  background: color,
                  boxShadow: isFocus ? "0 0 0 1.5px var(--gold), 0 0 0 3px var(--gold-bg)" : "none",
                  pointerEvents: "none",
                }}
              />
              {/* Hover lift — subtle background on parent on hover */}
            </div>
          );
        })}

        {/* Focused decision marker (chevron above) */}
        {focusedSlot != null && (
          <div
            style={{
              position: "absolute",
              left: focusedSlot * slot + tickW / 2 - 5,
              top: -6,
              width: 0, height: 0,
              borderLeft: "5px solid transparent",
              borderRight: "5px solid transparent",
              borderTop: "5px solid var(--gold)",
            }}
          />
        )}

        {/* Hover tooltip */}
        {hover && (
          <div
            className="pointer-events-none absolute z-10 px-2 py-1.5 rounded-sm2 font-mono text-[10.5px] whitespace-nowrap"
            style={{
              left: Math.min(Math.max(hover.x, 80), width - 80),
              transform: "translate(-50%, calc(-100% - 10px))",
              top: 0,
              background: "var(--surface-card)",
              border: "1px solid var(--border-strong)",
              color: "var(--text)",
              boxShadow: "0 8px 20px rgba(0,0,0,0.5)",
            }}
          >
            <div className="flex items-center gap-2 mb-0.5">
              <span className="text-text-3">#</span>
              <span className="tabular-nums">{hover.i}</span>
              <span className="text-text-4">·</span>
              <span className="tabular-nums text-text-2">{hover.t}</span>
              <span className="text-text-4">·</span>
              <span style={{
                color: hover.phase === "filtered" ? "var(--text-3)" :
                  hover.action === "BUY" ? "var(--gold)" :
                  hover.action === "SELL" ? "var(--danger)" : "var(--text)"
              }}>{hover.phase === "filtered" ? "FILTERED" : hover.action}</span>
              {hover.conv != null && hover.phase !== "filtered" && (
                <>
                  <span className="text-text-4">·</span>
                  <span className="tabular-nums text-text-2">{(hover.conv * 100).toFixed(0)}%</span>
                </>
              )}
            </div>
            {hover.just && hover.phase !== "filtered" && (
              <div className="text-text-3 max-w-[280px] truncate">{hover.just}</div>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

function ReviewPanel() {
  return (
    <Card title="Review" sub="supervisor · claude-haiku-4-5"
      right={<span className="px-1.5 py-0.5 text-[9px] font-mono tracking-[0.18em] rounded-sm2"
        style={{ color: "var(--warn)", background: "rgba(255, 176, 32, 0.10)", border: "1px solid rgba(255, 176, 32, 0.40)" }}>2 NOTES</span>}>
      <div className="p-5 space-y-3 text-[13px] leading-relaxed text-text-2">
        <p className="font-serif text-[15px] text-text" style={{ fontWeight: 400 }}>
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
  supervisor: { c: "#00e676", l: "SUPER" },
  artifact:   { c: "#a78bfa", l: "ARTIF" },
};

function App() {
  const { spans, decisions } = window.MOCK;
  const [tweaks, setTweak] = useTweaks(TWEAK_DEFAULTS);

  // Filter the sibling pool by tweak count + override status based on demo mode.
  const siblings = uM(() => {
    const base = SIBLING_POOL.slice(0, tweaks.siblingCount);
    if (tweaks.siblingState === "all-eval")  return base.map(s => ({ ...s, status: s.status === "queued" ? "queued" : "eval" }));
    if (tweaks.siblingState === "one-error") return base.map((s, i) => ({ ...s, status: i === 0 ? "error" : (s.status === "queued" ? "queued" : "eval") }));
    if (tweaks.siblingState === "finished")  return base.map(s => ({ ...s, status: "pass" }));
    return base; // "mixed" — use the pool's natural mix
  }, [tweaks.siblingCount, tweaks.siblingState]);

  const onSwitchEval = (run) => {
    toast(`Would navigate → /eval-runs/${run.id} (${run.strategy})`);
  };

  const [isLive, setLive]          = uS(true);
  const [dockOpen, setDockOpen]    = uS(false);
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
      <TopBar isLive={isLive}/>

      {/* Page body */}
      <div className="flex-1 min-h-0 overflow-auto px-6 py-6">
        <div className="max-w-[1400px] mx-auto">
          <div className="mb-5">
            <div className="flex items-baseline gap-4">
              <h1 className="font-mono text-[28px] leading-none text-text tracking-tight tabular-nums" style={{ fontWeight: 500 }}>
                abc1234de7f6
              </h1>
              <div className="text-[12px] font-mono text-text-3">
                started <span className="text-text-2">2026-05-17 10:13:31Z</span>
                <span className="text-text-4 mx-2">·</span>
                budget <span className="text-text-2">$0.18 / $1.00</span>
                <span className="text-text-4 mx-2">·</span>
                commit <span className="text-text-2">7f2b1ad</span>
              </div>
            </div>
            <div className="mt-4 flex items-center gap-2 flex-wrap">
              <MetaChip label="Strategy" value="mean-reversion-v3" tone="gold" onClick={() => {}}/>
              <MetaChip label="Scenario" value="flash-crash-2024-08" onClick={() => {}}/>
              <MetaChip label="Agent" value="trader-v2 · claude-sonnet-4-5" tone="info" onClick={() => {}}/>
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
                    ["seed",       "0x9c44a1"],
                    ["mode",       "paper · alpaca"],
                    ["region",     "us-east"],
                    ["budget",     "$0.18 / $1.00 (18%)"],
                    ["commit",     "7f2b1ad"],
                    ["started",    "2026-05-17 10:13:31Z"],
                    ["duration",   "00:00:43"],
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
          focusedShort="mr·flash"
          siblings={siblings}
          onSwitchEval={onSwitchEval}
          onExpand={expandDock} onPopOut={popOut}/>
      )}

      <ToastStack toasts={toasts}/>

      <TweaksPanel title="Tweaks · concurrent evals">
        <TweakSection label="Cluster"/>
        <TweakSlider label="Other evals running" value={tweaks.siblingCount} min={0} max={8} step={1}
          onChange={(v) => setTweak('siblingCount', v)}/>
        <TweakRadio label="Demo state" value={tweaks.siblingState}
          options={['mixed', 'all-eval', 'one-error', 'finished']}
          onChange={(v) => setTweak('siblingState', v)}/>
        <TweakButton label="Open dock" onClick={() => expandDock()}/>
        <TweakButton label="Close dock (show capsule)" onClick={() => minimizeDock()}/>
      </TweaksPanel>
    </div>
  );
}

ReactDOM.createRoot(document.getElementById("root")).render(<App/>);
