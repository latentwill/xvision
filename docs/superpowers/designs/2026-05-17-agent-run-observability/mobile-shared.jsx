// xvn — Mobile shared primitives
// Reuses Icon + Sparkline from shared.jsx (loaded earlier)

// ============================================================
// MINI EQUITY CHART — used inside chat cards & dashboards
// ============================================================
const MiniChart = ({ data, width = 320, height = 90, color = "var(--gold)", showFill = true, axisLabels = false }) => {
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const pad = 4;
  const pts = data.map((v, i) => {
    const x = (i / (data.length - 1)) * width;
    const y = height - ((v - min) / range) * (height - pad * 2) - pad;
    return [x, y];
  });
  const line = "M" + pts.map(p => p.join(",")).join(" L");
  const area = line + ` L${width},${height} L0,${height} Z`;
  const id = "mc-" + Math.random().toString(36).slice(2, 8);
  return (
    <svg width="100%" height={height} viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" style={{display: "block"}}>
      <defs>
        <linearGradient id={id} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor="#D4A547" stopOpacity="0.22"/>
          <stop offset="100%" stopColor="#D4A547" stopOpacity="0"/>
        </linearGradient>
      </defs>
      {[0.25, 0.5, 0.75].map((t, i) => (
        <line key={i} x1="0" x2={width} y1={t * height} y2={t * height} stroke="#2A2618" strokeDasharray="2 4" strokeWidth="0.5"/>
      ))}
      {showFill && <path d={area} fill={`url(#${id})`}/>}
      <path d={line} fill="none" stroke={color} strokeWidth="1.5"/>
    </svg>
  );
};

// Status bar overlay (iOS faux time/battery) — used inside chat for "now"
const ChatTimestamp = ({ time = "Now" }) => (
  <div style={{textAlign: "center", color: "var(--text-3)", fontSize: 11, fontFamily: "JetBrains Mono, monospace", letterSpacing: "0.04em"}}>{time}</div>
);

// Avatar (small)
const Avatar = ({ initials = "A", size = 26, gold = false }) => (
  <div style={{
    width: size, height: size,
    borderRadius: "50%",
    background: gold ? "rgba(212,165,71,0.18)" : "var(--surface-panel)",
    border: "1px solid " + (gold ? "rgba(212,165,71,0.35)" : "var(--border)"),
    display: "flex", alignItems: "center", justifyContent: "center",
    color: gold ? "var(--gold)" : "var(--text)",
    fontFamily: gold ? "Cormorant Garamond, serif" : "Inter, sans-serif",
    fontStyle: gold ? "italic" : "normal",
    fontSize: size * 0.46,
    fontWeight: 500,
    flexShrink: 0
  }}>{initials}</div>
);

// User message bubble
const UserMsg = ({ children }) => (
  <div className="m-msg-user">
    <div className="m-bubble">{children}</div>
  </div>
);

// Agent message (with optional inline children below text)
const AgentMsg = ({ time = "now", children, name = "xvn" }) => (
  <div className="m-msg-agent">
    <div className="m-av">x</div>
    <div className="m-body">
      <div className="m-head"><span className="name">{name}</span><span className="time">{time}</span></div>
      {children}
    </div>
  </div>
);

// Day divider
const DayDivider = ({ label }) => (
  <div className="m-divider">{label}</div>
);

// ============================================================
// INLINE RICH CARDS
// ============================================================

// Eval chart card — appears inline in chat
const ChatChartCard = ({
  title = "Equity (paper)",
  meta = "Today · combined",
  ret = "+1.42%",
  retClass = "up",
  data,
  kpis = [["P&L", "+$142.30"], ["Sharpe", "1.62"], ["Trades", "8"]],
  ctaLeft = "eth-mr-v3 · btc-mom-v1 · stable-flow",
  ctaRight = "Open run →",
}) => (
  <div className="m-card">
    <div className="m-card-head">
      <div>
        <div className="title">{title}</div>
        <div className="meta">{meta}</div>
      </div>
      <div className={"mono " + retClass} style={{fontSize: 17, fontFamily: "Cormorant Garamond, serif", fontWeight: 500}}>{ret}</div>
    </div>
    <div style={{padding: "0 14px 12px"}}>
      <MiniChart data={data} height={86} />
    </div>
    <div className="m-kpi-strip">
      {kpis.map(([l, v]) => (
        <div key={l}><div className="l">{l}</div><div className="v">{v}</div></div>
      ))}
    </div>
    <div className="m-card-foot">
      <span className="left mono" style={{fontSize: 11}}>{ctaLeft}</span>
      <a>{ctaRight}</a>
    </div>
  </div>
);

// Run list card — agent returns a short ranked list inline
const ChatRunListCard = ({ title = "Top runs (30d)", rows, footer = "View all 47 runs →" }) => (
  <div className="m-card">
    <div className="m-card-head">
      <div className="title">{title}</div>
      <span className="meta">SORTED · SHARPE</span>
    </div>
    {rows.map(([id, strat, ret, retCls, sharpe], i) => (
      <div key={id} className="m-run-row">
        <div className="ix">{i + 1}</div>
        <div className="id">
          {id}
          <span className="sub">{strat}</span>
        </div>
        <div style={{textAlign: "right"}}>
          <div className={"ret " + retCls}>{ret}</div>
          <div className="mono" style={{fontSize: 10.5, color: "var(--text-3)", marginTop: 2}}>SH {sharpe}</div>
        </div>
      </div>
    ))}
    <div className="m-card-foot">
      <span className="left"><Icon name="bars" size={12} color="var(--text-3)"/> 47 total</span>
      <a>{footer}</a>
    </div>
  </div>
);

// Strategy card
const ChatStrategyCard = ({ name = "eth-mr-v3", state = "Live · paper", pnl = "+$91.24", data }) => (
  <div className="m-card">
    <div className="m-card-head">
      <div>
        <div className="title mono" style={{fontFamily: "JetBrains Mono, monospace", fontSize: 14, color: "var(--text)"}}>{name}</div>
        <div className="meta"><span className="dot gold" style={{marginRight: 4}}/>{state}</div>
      </div>
      <div className="mono up" style={{fontFamily: "Cormorant Garamond, serif", fontWeight: 500, fontSize: 20}}>{pnl}</div>
    </div>
    <div style={{padding: "0 14px 10px"}}>
      <MiniChart data={data} height={54} />
    </div>
    <div style={{display: "flex", gap: 6, padding: "0 14px 12px", flexWrap: "wrap"}}>
      <span className="pill gold">Paper</span>
      <span className="pill">Alpaca</span>
      <span className="pill">3 positions</span>
    </div>
    <div className="m-card-foot">
      <span className="left">Sharpe <span className="mono" style={{color: "var(--text)"}}>1.62</span></span>
      <a>Open strategy →</a>
    </div>
  </div>
);

// Action confirmation card (e.g. "Run scheduled")
const ChatActionCard = ({ kind = "check", title = "Run 01M7B1 started", sub = "eth-mr-v3 · paper · bull-q1-25", actions = ["View progress →", "Cancel"] }) => (
  <div className="m-card" style={{display: "flex", alignItems: "center", gap: 12, padding: "12px 14px"}}>
    <div style={{
      width: 32, height: 32, borderRadius: "50%",
      border: "1px solid rgba(212,165,71,0.35)",
      background: "var(--gold-bg)",
      color: "var(--gold)",
      display: "flex", alignItems: "center", justifyContent: "center",
      flexShrink: 0
    }}><Icon name={kind} size={15} /></div>
    <div style={{flex: 1, minWidth: 0}}>
      <div style={{fontSize: 13.5, color: "var(--text)"}}>{title}</div>
      <div className="mono" style={{fontSize: 11, color: "var(--text-3)", marginTop: 2}}>{sub}</div>
    </div>
    <a style={{color: "var(--gold)", fontSize: 12, textDecoration: "none", whiteSpace: "nowrap"}}>{actions[0]}</a>
  </div>
);

// ============================================================
// SHARED CHROME
// ============================================================
const MobileTopBar = ({ onMenu, context = null, right = null, title = null }) => (
  <div className="m-topbar">
    <button className="m-icon-btn"><Icon name="list" size={18}/></button>
    {title ? (
      <div style={{flex: 1, fontFamily: "Cormorant Garamond, serif", fontSize: 22, fontWeight: 500}}>{title}</div>
    ) : (
      <div className="m-ctx">
        {context ? (
          <div className="m-ctx-chip">
            <span className="dot gold" style={{margin: 0}}/>
            <span className="mono">{context}</span>
            <Icon name="chevR" size={12} color="var(--text-3)" />
          </div>
        ) : (
          <span className="m-brand">xvn</span>
        )}
      </div>
    )}
    <button className="m-icon-btn has-dot">
      <Icon name="pulse" size={18} />
      <span className="badge"></span>
    </button>
  </div>
);

const QuickRail = ({ items }) => (
  <div className="m-quick">
    {items.map((label, i) => (
      <div key={i} className={"m-chip" + (i === 0 ? " gold" : "")}>{label}</div>
    ))}
  </div>
);

const Composer = ({ value = "", placeholder = "Ask xvn anything…" }) => (
  <div className="m-composer">
    <div className="m-composer-inner">
      <button className="m-c-btn"><Icon name="plus" size={18}/></button>
      <div className={"field" + (value ? "" : " empty")}>{value || placeholder}</div>
      <button className="m-c-btn"><Icon name="bars" size={16}/></button>
      <button className={"m-c-btn send" + (value ? "" : " dim")}><Icon name="arrow" size={16} color={value ? "#0F0E0C" : "currentColor"}/></button>
    </div>
  </div>
);

window.MiniChart = MiniChart;
window.ChatTimestamp = ChatTimestamp;
window.Avatar = Avatar;
window.UserMsg = UserMsg;
window.AgentMsg = AgentMsg;
window.DayDivider = DayDivider;
window.ChatChartCard = ChatChartCard;
window.ChatRunListCard = ChatRunListCard;
window.ChatStrategyCard = ChatStrategyCard;
window.ChatActionCard = ChatActionCard;
window.MobileTopBar = MobileTopBar;
window.QuickRail = QuickRail;
window.Composer = Composer;
