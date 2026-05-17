// Shared icons and primitives for xvn screens
const Icon = ({ name, size = 16, color = "currentColor", strokeWidth = 1.5 }) => {
  const s = size;
  const sw = strokeWidth;
  const paths = {
    home: <><path d="M3 9.5L10 4l7 5.5V16a1 1 0 01-1 1h-3v-5H9v5H4a1 1 0 01-1-1V9.5z"/></>,
    chart: <><path d="M3 16h14M5 13l3-4 3 2 4-6"/></>,
    play: <><circle cx="10" cy="10" r="7"/><path d="M8 7l5 3-5 3V7z" fill="currentColor" stroke="none"/></>,
    bars: <><path d="M4 16V8M8 16V5M12 16v-6M16 16v-9"/></>,
    book: <><path d="M4 4h5a3 3 0 013 3v9a2 2 0 00-2-2H4V4zM16 4h-5a3 3 0 00-3 3v9a2 2 0 012-2h6V4z"/></>,
    db: <><ellipse cx="10" cy="5" rx="6" ry="2"/><path d="M4 5v10c0 1.1 2.7 2 6 2s6-.9 6-2V5M4 10c0 1.1 2.7 2 6 2s6-.9 6-2"/></>,
    cog: <><circle cx="10" cy="10" r="2.5"/><path d="M10 2v2M10 16v2M16.4 6l-1.4.8M5 13.2L3.6 14M18 10h-2M4 10H2M16.4 14L15 13.2M5 6.8L3.6 6"/></>,
    pulse: <><path d="M3 10h3l2-5 4 10 2-5h3"/></>,
    dollar: <><circle cx="10" cy="10" r="7"/><path d="M10 6v8M12.5 7.5h-3.5a1.5 1.5 0 000 3h2a1.5 1.5 0 010 3H7.5"/></>,
    bag: <><path d="M5 7h10l-1 10H6L5 7zM7 7V5a3 3 0 016 0v2"/></>,
    barchart: <><path d="M3 17h14M6 17V9M10 17V5M14 17v-6"/></>,
    diamond: <><path d="M10 3l5 6-5 8-5-8 5-6z"/></>,
    code: <><path d="M7 6l-4 4 4 4M13 6l4 4-4 4"/></>,
    arrow: <><path d="M4 10h12M12 6l4 4-4 4"/></>,
    check: <><circle cx="10" cy="10" r="7"/><path d="M7 10l2 2 4-4"/></>,
    findingDot: <><circle cx="10" cy="10" r="7"/><circle cx="10" cy="10" r="3" fill="currentColor" stroke="none"/></>,
    branch: <><circle cx="6" cy="5" r="1.5"/><circle cx="6" cy="15" r="1.5"/><circle cx="14" cy="9" r="1.5"/><path d="M6 6.5v7M7.5 9h2A4.5 4.5 0 0014 9v-1.5"/></>,
    plus: <><path d="M10 4v12M4 10h12"/></>,
    search: <><circle cx="9" cy="9" r="5"/><path d="M13 13l4 4"/></>,
    chevR: <><path d="M8 5l5 5-5 5"/></>,
    settings: <><circle cx="10" cy="10" r="2.5"/><path d="M3 10h2M15 10h2M10 3v2M10 15v2M5 5l1.5 1.5M13.5 13.5L15 15M5 15l1.5-1.5M13.5 6.5L15 5"/></>,
    box: <><path d="M3 7l7-3 7 3v6l-7 3-7-3V7z M3 7l7 3 7-3M10 10v7"/></>,
    user: <><circle cx="10" cy="7" r="3"/><path d="M4 17c0-3 2.5-5 6-5s6 2 6 5"/></>,
    list: <><path d="M3 6h14M3 10h14M3 14h14"/></>,
    flame: <><path d="M10 17c3 0 5-2 5-5 0-3-3-4-3-7-2 1-3 3-3 4-1-1-1.5-2-1.5-3-2 1.5-2.5 4-2.5 6 0 3 2 5 5 5z"/></>,
    sliders: <><path d="M4 6h6M14 6h2M4 10h2M10 10h6M4 14h10M16 14h0"/><circle cx="12" cy="6" r="1.5"/><circle cx="8" cy="10" r="1.5"/><circle cx="14" cy="14" r="1.5"/></>,
  };
  const p = paths[name];
  return (
    <svg width={s} height={s} viewBox="0 0 20 20" fill="none" stroke={color} strokeWidth={sw} strokeLinecap="round" strokeLinejoin="round" style={{flexShrink: 0}}>
      {p}
    </svg>
  );
};

// Sidebar — reused across all routes
const Sidebar = ({ active = "home" }) => {
  const items = [
    { key: "home", label: "Home", icon: "home" },
    { key: "strategies", label: "Strategies", icon: "chart" },
    { key: "live", label: "Live", icon: "play" },
    { key: "eval", label: "Eval", icon: "bars" },
    { key: "journal", label: "Journal", icon: "book" },
    { key: "data", label: "Data", icon: "db" },
    { key: "settings", label: "Settings", icon: "cog" },
  ];
  return (
    <aside className="sidebar">
      <div className="brand">xvn</div>
      <nav className="nav">
        {items.map(i => (
          <div key={i.key} className={"nav-item" + (i.key === active ? " active" : "")}>
            <Icon name={i.icon} size={17} color={i.key === active ? "var(--gold)" : "currentColor"} />
            <span>{i.label}</span>
          </div>
        ))}
      </nav>
      <div className="sidebar-card">
        <h4>Setup agent</h4>
        <p>Add an LLM key to begin building strategies with xvn.</p>
        <button className="btn primary" style={{width: "100%", justifyContent: "center", padding: "8px"}}>Add LLM key</button>
      </div>
      <div className="user-row">
        <div className="avatar">AK</div>
        <div style={{flex: 1, minWidth: 0}}>
          <div style={{fontSize: 13, color: "var(--text)"}}>Alex Kim</div>
          <div style={{fontSize: 11, color: "var(--text-3)"}}>alex@xvn.dev</div>
        </div>
        <Icon name="chevR" size={14} color="var(--text-3)" />
      </div>
    </aside>
  );
};

// Topbar — title + cmdk
const Topbar = ({ title, sub, cmdkPlaceholder = "Jump to anything..." }) => (
  <div className="topbar">
    <div>
      <h1>{title}</h1>
      {sub && <div className="sub">{sub}</div>}
    </div>
    <div className="cmdk">
      <span className="kbd">⌘K</span>
      <span style={{flex: 1}}>{cmdkPlaceholder}</span>
    </div>
  </div>
);

// Sparkline SVG generator
const Sparkline = ({ data, width = 80, height = 22, color = "var(--gold)" }) => {
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const pts = data.map((v, i) => {
    const x = (i / (data.length - 1)) * width;
    const y = height - ((v - min) / range) * (height - 2) - 1;
    return `${x},${y}`;
  }).join(" ");
  return (
    <svg className="spark" width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
      <polyline points={pts} fill="none" stroke={color} strokeWidth="1.2" />
    </svg>
  );
};

window.Icon = Icon;
window.Sidebar = Sidebar;
window.Topbar = Topbar;
window.Sparkline = Sparkline;
