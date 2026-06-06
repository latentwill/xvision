// Shared chrome for blockchain surfaces — Signal theme
// Sidebar, top status bar (with wallet pill), pills, icons, mono helpers

const Icon = ({ name, size = 16, color = "currentColor", sw = 1.5 }) => {
  const paths = {
    home:    <path d="M3 9.5L10 4l7 5.5V16a1 1 0 01-1 1h-3v-5H9v5H4a1 1 0 01-1-1V9.5z"/>,
    chart:   <path d="M3 16h14M5 13l3-4 3 2 4-6"/>,
    play:    <><circle cx="10" cy="10" r="7"/><path d="M8 7l5 3-5 3V7z" fill="currentColor" stroke="none"/></>,
    bars:    <path d="M4 16V8M8 16V5M12 16v-6M16 16v-9"/>,
    book:    <path d="M4 4h5a3 3 0 013 3v9a2 2 0 00-2-2H4V4zM16 4h-5a3 3 0 00-3 3v9a2 2 0 012-2h6V4z"/>,
    db:      <><ellipse cx="10" cy="5" rx="6" ry="2"/><path d="M4 5v10c0 1.1 2.7 2 6 2s6-.9 6-2V5M4 10c0 1.1 2.7 2 6 2s6-.9 6-2"/></>,
    cog:     <><circle cx="10" cy="10" r="2.5"/><path d="M10 2v2M10 16v2M16.4 6l-1.4.8M5 13.2L3.6 14M18 10h-2M4 10H2M16.4 14L15 13.2M5 6.8L3.6 6"/></>,
    market:  <path d="M3 7l7-3 7 3v6l-7 3-7-3V7z M3 7l7 3 7-3M10 10v7"/>,
    plus:    <path d="M10 4v12M4 10h12"/>,
    chevR:   <path d="M8 5l5 5-5 5"/>,
    chevD:   <path d="M5 8l5 5 5-5"/>,
    ext:     <path d="M11 5h4v4M9 11l6-6M14 12v3a1 1 0 01-1 1H5a1 1 0 01-1-1V7a1 1 0 011-1h3"/>,
    copy:    <><rect x="6" y="6" width="9" height="9" rx="1"/><path d="M11 6V5a1 1 0 00-1-1H5a1 1 0 00-1 1v6a1 1 0 001 1h1"/></>,
    lock:    <><rect x="4" y="9" width="12" height="8" rx="1.5"/><path d="M7 9V7a3 3 0 016 0v2"/></>,
    check:   <><circle cx="10" cy="10" r="7"/><path d="M7 10l2 2 4-4"/></>,
    diamond: <path d="M10 3l5 6-5 8-5-8 5-6z"/>,
    branch:  <><circle cx="6" cy="5" r="1.5"/><circle cx="6" cy="15" r="1.5"/><circle cx="14" cy="9" r="1.5"/><path d="M6 6.5v7M7.5 9h2A4.5 4.5 0 0014 9v-1.5"/></>,
    wallet:  <><rect x="3" y="6" width="14" height="10" rx="1.5"/><path d="M3 8h14M13 12h1"/></>,
    shield:  <path d="M10 3l6 2v4c0 4-3 7-6 8-3-1-6-4-6-8V5l6-2z"/>,
    nft:     <><rect x="4" y="4" width="12" height="12" rx="1.5"/><path d="M4 8h12M8 4v12"/></>,
    radio:   <><circle cx="10" cy="10" r="2"/><path d="M5.5 5.5a6 6 0 010 9M14.5 5.5a6 6 0 010 9M7.5 7.5a3 3 0 010 5M12.5 7.5a3 3 0 010 5"/></>,
    spark:   <path d="M3 13l3-3 2 2 4-5 2 3 3-2"/>,
    info:    <><circle cx="10" cy="10" r="7"/><path d="M10 9v4M10 7v0.01"/></>,
    search:  <><circle cx="9" cy="9" r="5"/><path d="M13 13l4 4"/></>,
    arrowU:  <path d="M10 16V4M5 9l5-5 5 5"/>,
    arrowD:  <path d="M10 4v12M5 11l5 5 5-5"/>,
    bolt:    <path d="M11 3L4 12h5l-1 5 7-9h-5l1-5z"/>,
    eye:     <><path d="M2 10s3-5 8-5 8 5 8 5-3 5-8 5-8-5-8-5z"/><circle cx="10" cy="10" r="2"/></>,
    paste:   <><rect x="5" y="4" width="10" height="13" rx="1"/><path d="M7 4V3a1 1 0 011-1h4a1 1 0 011 1v1"/></>,
    gas:     <><rect x="4" y="4" width="9" height="13" rx="1"/><path d="M13 8h2v6a1.5 1.5 0 01-3 0V10"/></>,
  };
  return (
    <svg width={size} height={size} viewBox="0 0 20 20" fill="none"
      stroke={color} strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round"
      style={{flexShrink:0, display:"block"}}>{paths[name]}</svg>
  );
};

// Brand mark — BRKT lockup (bracketed XVN wordmark)
// Locked 24:7 aspect; pass height via `size`.
const BrandMark = ({size = 14, brackets = "var(--gold)"}) => {
  const w = (48 / 14) * size;
  return (
    <svg
      width={w} height={size} viewBox="0 0 48 14"
      xmlns="http://www.w3.org/2000/svg" aria-label="XVN"
      style={{display:"block", overflow:"visible"}}
    >
      <g stroke={brackets} strokeWidth="1.4" fill="none" strokeLinecap="square">
        <path d="M4 1 H1 V13 H4"/>
        <path d="M44 1 H47 V13 H44"/>
      </g>
      <text
        x="24" y="7" fill="currentColor"
        fontFamily="Geist Mono, ui-monospace, monospace"
        fontSize="13" fontWeight="700"
        letterSpacing="0.14em"
        dominantBaseline="central" textAnchor="middle"
      >XVN</text>
    </svg>
  );
};

// Sidebar — adds Marketplace nav item below Journal when `marketplaceVisible`
const SideNav = ({ active = "marketplace", marketplaceVisible = true }) => {
  const top = [
    { key:"home",       label:"Home",       icon:"home" },
    { key:"strategies", label:"Strategies", icon:"chart" },
    { key:"live",       label:"Live",       icon:"play" },
    { key:"eval",       label:"Eval",       icon:"bars" },
    { key:"journal",    label:"Journal",    icon:"book" },
  ];
  if (marketplaceVisible) top.push({ key:"marketplace", label:"Marketplace", icon:"market" });
  top.push({ key:"data", label:"Data", icon:"db" });
  top.push({ key:"settings", label:"Settings", icon:"cog" });

  return (
    <aside style={{
      background:"var(--surface-sidebar)", borderRight:"1px solid var(--border-soft)",
      display:"flex", flexDirection:"column", padding:"22px 0 14px", width:200,
    }}>
      <div style={{padding:"0 22px 24px"}}><BrandMark/></div>

      <nav style={{display:"flex", flexDirection:"column", flex:1}}>
        {top.map(i => {
          const isActive = i.key === active;
          return (
            <div key={i.key} style={{
              display:"flex", alignItems:"center", gap:12,
              padding:"9px 22px",
              color: isActive ? "var(--text)" : "var(--text-2)",
              borderLeft: `2px solid ${isActive ? "var(--gold)" : "transparent"}`,
              fontSize:13.5, fontWeight:500, cursor:"pointer",
            }}>
              <Icon name={i.icon} size={16} color={isActive ? "var(--gold)" : "currentColor"}/>
              <span>{i.label}</span>
            </div>
          );
        })}
      </nav>

      {/* Wallet block at bottom of sidebar */}
      <div style={{
        margin:"0 14px 14px", padding:"12px 12px",
        border:"1px solid var(--border)", borderRadius:6,
      }}>
        <div style={{display:"flex", alignItems:"center", gap:8, marginBottom:8}}>
          <span className="pulse" style={{width:6, height:6, borderRadius:"50%", background:"var(--gold)"}}/>
          <span className="ulabel" style={{fontSize:9.5, letterSpacing:"0.16em"}}>WALLET</span>
        </div>
        <div className="mono" style={{fontSize:11.5, color:"var(--text)", marginBottom:4}}>0xa83e…f12d4</div>
        <div className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>0.42 ETH · Mantle</div>
      </div>

      <div style={{
        display:"flex", alignItems:"center", gap:10,
        padding:"12px 16px", borderTop:"1px solid var(--border-soft)",
      }}>
        <div style={{
          width:30, height:30, borderRadius:"50%",
          background:"var(--surface-panel)", border:"1px solid var(--border)",
          display:"flex", alignItems:"center", justifyContent:"center",
          fontSize:10.5, fontWeight:600,
        }}>AK</div>
        <div style={{flex:1, minWidth:0}}>
          <div style={{fontSize:12.5}}>Alex Kim</div>
          <div style={{fontSize:10.5, color:"var(--text-3)"}}>operator</div>
        </div>
        <Icon name="chevR" size={13} color="var(--text-3)"/>
      </div>
    </aside>
  );
};

// Top status bar — global chrome above page content
// Right side shows daemon · LLM · wallet pills
const TopStatus = ({ breadcrumb = [], walletConnected = true, network = "mantle" }) => (
  <div style={{
    height:48, borderBottom:"1px solid var(--border)",
    display:"flex", alignItems:"center", gap:14, padding:"0 18px",
    background:"#000", flexShrink:0,
  }}>
    {/* Breadcrumb */}
    <div style={{display:"flex", alignItems:"center", gap:10, flex:1}}>
      {breadcrumb.map((b, i) => (
        <React.Fragment key={i}>
          {i > 0 && <span style={{color:"var(--text-4)"}}>/</span>}
          <span className={b.mono ? "mono" : "ulabel"} style={{
            fontSize: b.mono ? 12 : 11,
            color: i === breadcrumb.length-1 ? "var(--text)" : "var(--text-3)",
            letterSpacing: b.mono ? "normal" : "0.18em",
          }}>{b.text}</span>
        </React.Fragment>
      ))}
    </div>

    {/* Status pills */}
    <div style={{display:"flex", alignItems:"center", gap:8}}>
      <StatusPill tone="info" pulse>DAEMON</StatusPill>
      <StatusPill tone="neutral">CLAUDE · HAIKU-4-5</StatusPill>
      {walletConnected ? (
        <div style={{
          display:"inline-flex", alignItems:"center", gap:7,
          padding:"4px 9px", border:"1px solid var(--gold-soft)",
          background:"var(--gold-bg)", borderRadius:3,
        }}>
          <span style={{width:6, height:6, borderRadius:"50%", background:"var(--gold)"}}/>
          <span className="mono" style={{fontSize:10.5, color:"var(--gold)", letterSpacing:"0.02em"}}>0xa83e…f12 · 0.42 ETH</span>
        </div>
      ) : (
        <div style={{
          display:"inline-flex", alignItems:"center", gap:7,
          padding:"4px 9px", border:"1px dashed var(--border-strong)",
          borderRadius:3,
        }}>
          <Icon name="wallet" size={11} color="var(--text-3)"/>
          <span className="mono" style={{fontSize:10.5, color:"var(--text-3)", letterSpacing:"0.02em"}}>NO WALLET</span>
        </div>
      )}
    </div>
  </div>
);

// Status pill — generic
const StatusPill = ({ tone = "neutral", pulse = false, dot = true, children }) => {
  const tones = {
    gold:    { bg:"var(--gold-bg)",      bd:"var(--gold-soft)",            fg:"var(--gold)",    dot:"var(--gold)" },
    info:    { bg:"rgba(95,168,255,0.10)", bd:"rgba(95,168,255,0.40)",     fg:"var(--info)",    dot:"var(--info)" },
    warn:    { bg:"rgba(255,176,32,0.10)", bd:"rgba(255,176,32,0.40)",     fg:"var(--warn)",    dot:"var(--warn)" },
    danger:  { bg:"rgba(255,77,77,0.10)",  bd:"rgba(255,77,77,0.40)",      fg:"var(--danger)",  dot:"var(--danger)" },
    neutral: { bg:"var(--surface-elev)",   bd:"var(--border-strong)",      fg:"var(--text-2)",  dot:"var(--text-3)" },
    mute:    { bg:"transparent",           bd:"var(--border-strong)",      fg:"var(--text-3)",  dot:"var(--text-4)" },
  };
  const t = tones[tone] || tones.neutral;
  return (
    <div style={{
      display:"inline-flex", alignItems:"center", gap:7,
      padding:"4px 9px", border:`1px solid ${t.bd}`, background:t.bg, borderRadius:3,
    }}>
      {dot && <span className={pulse ? "pulse" : ""} style={{width:6, height:6, borderRadius:"50%", background:t.dot}}/>}
      <span className="mono" style={{fontSize:10, color:t.fg, letterSpacing:"0.16em", textTransform:"uppercase", fontWeight:500}}>{children}</span>
    </div>
  );
};

// Tx hash chip — clickable mono pill with external-link icon
const TxChip = ({ hash, label, tone = "neutral", style }) => {
  const tones = {
    neutral: { bg:"var(--surface-elev)", fg:"var(--text-2)", bd:"var(--border-strong)" },
    gold:    { bg:"var(--gold-bg)",      fg:"var(--gold)",   bd:"var(--gold-soft)" },
    info:    { bg:"rgba(95,168,255,0.10)", fg:"var(--info)", bd:"rgba(95,168,255,0.40)" },
  };
  const t = tones[tone] || tones.neutral;
  return (
    <span style={{
      display:"inline-flex", alignItems:"center", gap:6,
      padding:"3px 7px", border:`1px solid ${t.bd}`, background:t.bg, borderRadius:3,
      fontFamily:"'Geist Mono', monospace", fontSize:11, color:t.fg,
      cursor:"pointer", ...style,
    }}>
      {label && <span style={{color:"var(--text-3)", letterSpacing:"0.14em", fontSize:9.5, textTransform:"uppercase"}}>{label}</span>}
      <span>{hash}</span>
      <Icon name="ext" size={10} color={t.fg}/>
    </span>
  );
};

// Section card — title + actions header + body
const Card = ({ title, sub, right, children, style, bodyStyle, dense = false }) => (
  <div style={{
    border:"1px solid var(--border)", borderRadius:6,
    background:"transparent", display:"flex", flexDirection:"column",
    ...style,
  }}>
    {(title || right) && (
      <div style={{
        display:"flex", alignItems:"center", justifyContent:"space-between",
        padding: dense ? "12px 14px 10px" : "14px 16px 12px",
        borderBottom: title ? "1px solid var(--border-soft)" : "none",
      }}>
        <div style={{display:"flex", flexDirection:"column", gap:3}}>
          {title && <div style={{fontSize:14, fontWeight:600, letterSpacing:"-0.01em"}}>{title}</div>}
          {sub && <div className="mono" style={{fontSize:10.5, color:"var(--text-3)"}}>{sub}</div>}
        </div>
        {right}
      </div>
    )}
    <div style={{flex:1, minHeight:0, ...bodyStyle}}>{children}</div>
  </div>
);

// Button — primary / ghost / danger
const Btn = ({ variant = "ghost", icon, children, dense = false, style, lock = false, ...p }) => {
  const v = {
    primary: { bg:"var(--gold)", fg:"#001A0A", bd:"var(--gold)" },
    ghost:   { bg:"transparent", fg:"var(--text-2)", bd:"var(--border-strong)" },
    danger:  { bg:"transparent", fg:"var(--danger)", bd:"rgba(255,77,77,0.4)" },
    chip:    { bg:"var(--surface-elev)", fg:"var(--text)", bd:"var(--border)" },
  }[variant];
  return (
    <button style={{
      display:"inline-flex", alignItems:"center", gap:6,
      padding: dense ? "5px 10px" : "7px 12px",
      borderRadius:4, border:`1px solid ${v.bd}`, background:v.bg, color:v.fg,
      fontFamily:"'Geist', sans-serif", fontSize: dense ? 12 : 12.5, fontWeight:600,
      cursor:"pointer", letterSpacing:"-0.005em", lineHeight:1, ...style,
    }} {...p}>
      {lock && <Icon name="lock" size={11} color={v.fg}/>}
      {icon && <Icon name={icon} size={12} color={v.fg}/>}
      {children}
    </button>
  );
};

// Lineage color dot (palette of cool/warm hues for line colors, like chart palette)
const LINEAGE_COLORS = {
  A: "#00E676",  // signal green
  B: "#5FA8FF",  // sky/info
  C: "#A78BFA",  // violet
  D: "#FBBF24",  // yellow
  E: "#F472B6",  // pink
};
const LineageDot = ({ id, size = 8 }) => (
  <span style={{
    width:size, height:size, borderRadius:"50%",
    background: LINEAGE_COLORS[id] || "var(--text-3)",
    display:"inline-block", verticalAlign:"middle",
  }}/>
);

// Frame chrome wrapper — the rectangular page surface (1440×900)
const Frame = ({ children }) => (
  <div className="frame-bg">{children}</div>
);

Object.assign(window, {
  Icon, BrandMark, SideNav, TopStatus, StatusPill, TxChip, Card, Btn,
  LineageDot, LINEAGE_COLORS, Frame,
});
