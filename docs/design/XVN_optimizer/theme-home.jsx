// xvn — Themed Control Tower (Robinhood base, 3 color/font variants)

const T_NAV = [
  { key: 'home', label: 'Home', icon: 'home' },
  { key: 'strategies', label: 'Strategies', icon: 'chart' },
  { key: 'live', label: 'Live', icon: 'play' },
  { key: 'eval', label: 'Eval', icon: 'bars' },
  { key: 'journal', label: 'Journal', icon: 'book' },
  { key: 'data', label: 'Data', icon: 'db' },
  { key: 'settings', label: 'Settings', icon: 'cog' },
];

// Per-variant config: brand text, chart colors
const VARIANTS = {
  'rh-signal': {
    brand: 'xvn',
    accent: '#00E676',
    chartStroke: '#00E676',
    chartFill: '#00E676',
    chartFillOp: 0.10,
    chartLineW: 1.6,
    gridStroke: '#141414',
  },
  'rh-atlas': {
    brand: 'xvn',
    accent: '#5EEAD4',
    chartStroke: '#5EEAD4',
    chartFill: '#5EEAD4',
    chartFillOp: 0.08,
    chartLineW: 1.5,
    gridStroke: '#14171B',
  },
  'rh-terminal': {
    brand: 'xvn',
    accent: '#FB923C',
    chartStroke: '#4ADE80',  // chart stays green to match P&L semantics
    chartFill: '#4ADE80',
    chartFillOp: 0.0,
    chartLineW: 1.4,
    gridStroke: '#16161C',
  },
};

// Equity curve
const EQ = [
  0, 0.05, -0.1, 0.05, -0.05, -0.15, -0.05, 0.1, 0.15, 0.05, -0.1, -0.05,
  0.15, 0.25, 0.2, 0.35, 0.5, 0.45, 0.6, 0.7, 0.55, 0.75, 0.85, 0.9,
  1.05, 1.1, 1.0, 1.15, 1.25, 1.35, 1.3, 1.4, 1.42, 1.45, 1.5, 1.55, 1.5
];

const EquityChart = ({ variant }) => {
  const W = 700, H = 200;
  const min = Math.min(...EQ), max = Math.max(...EQ);
  const range = max - min || 1;
  const pts = EQ.map((v, i) => {
    const x = (i / (EQ.length - 1)) * W;
    const y = H - ((v - min) / range) * (H - 12) - 6;
    return [x, y];
  });
  const linePath = "M" + pts.map(p => p.join(",")).join(" L");
  const areaPath = linePath + ` L${W},${H} L0,${H} Z`;
  const cursorIdx = 27;
  const xC = pts[cursorIdx][0];
  const v = VARIANTS[variant];
  const gradId = `eq-grad-${variant}`;

  return (
    <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" style={{display: "block"}}>
      <defs>
        <linearGradient id={gradId} x1="0" x2="0" y1="0" y2="1">
          <stop offset="0%" stopColor={v.chartFill} stopOpacity={v.chartFillOp}/>
          <stop offset="100%" stopColor={v.chartFill} stopOpacity="0"/>
        </linearGradient>
      </defs>
      {[0, 0.25, 0.5, 0.75, 1].map((t, i) => (
        <line key={i} x1="0" x2={W} y1={t * H} y2={t * H} stroke={v.gridStroke} strokeDasharray="1 3" strokeWidth="0.5"/>
      ))}
      {v.chartFillOp > 0 && <path d={areaPath} fill={`url(#${gradId})`}/>}
      <path d={linePath} fill="none" stroke={v.chartStroke} strokeWidth={v.chartLineW} strokeLinejoin="round" strokeLinecap="round"/>
      <line x1={xC} x2={xC} y1={0} y2={H} stroke={v.chartStroke} strokeDasharray="2 3" strokeWidth="0.5" opacity="0.4"/>
      <circle cx={pts[cursorIdx][0]} cy={pts[cursorIdx][1]} r="3.2" fill={v.chartStroke}/>
      <circle cx={pts[cursorIdx][0]} cy={pts[cursorIdx][1]} r="7" fill={v.chartStroke} opacity="0.18"/>
    </svg>
  );
};

const ThemedHome = ({ theme = 'rh-signal' }) => {
  const v = VARIANTS[theme];
  const accentColor = v.accent;

  return (
    <div className={`theme theme-${theme}`}>
      <div className="shell">
        {/* Sidebar */}
        <aside className="sidebar">
          <div className="brand">{v.brand}</div>
          <nav className="nav">
            {T_NAV.map(i => (
              <div key={i.key} className={"nav-item" + (i.key === 'home' ? ' active' : '')}>
                <Icon name={i.icon} size={17} color={i.key === 'home' ? accentColor : 'currentColor'} />
                <span>{i.label}</span>
              </div>
            ))}
          </nav>
          <div className="sidebar-card">
            <h4>Setup agent</h4>
            <p>Add an LLM key to begin building strategies with xvn.</p>
            <button className="btn primary" style={{width: "100%", justifyContent: "center"}}>Add LLM key</button>
          </div>
          <div className="user-row">
            <div className="avatar">AK</div>
            <div style={{flex: 1, minWidth: 0}}>
              <div style={{fontSize: 13}}>Alex Kim</div>
              <div style={{fontSize: 11, color: 'var(--text-3)'}}>alex@xvn.dev</div>
            </div>
            <Icon name="chevR" size={14} color="var(--text-3)" />
          </div>
        </aside>

        {/* Main */}
        <main className="main">
          {/* Hero portfolio number */}
          <div className="hero">
            <div style={{fontSize: 13, color: 'var(--text-2)', marginBottom: 4}}>Portfolio value (paper)</div>
            <div className="hero-num tnum">$10,142.30</div>
            <div className="hero-sub">
              <span className="up" style={{fontWeight: 600}}>+$142.30 (+1.42%)</span>
              <span style={{color: 'var(--text-3)'}}>Today</span>
            </div>
          </div>

          <div className="topbar">
            <div>
              <h1>Good morning, Alex.</h1>
              <div className="sub">3 deployments · 5 runs queued</div>
            </div>
            <div className="cmdk">
              <Icon name="search" size={14} color="var(--text-3)" />
              <span style={{flex: 1}}>Jump to anything...</span>
              <span className="kbd">⌘K</span>
            </div>
          </div>

          {/* KPIs row */}
          <div className="grid" style={{
            gridTemplateColumns: 'repeat(4, 1fr)',
            marginBottom: 18,
            gap: 0,
            border: '1px solid var(--border)',
            borderRadius: 6,
            padding: 18
          }}>
            {[
              ['Live deployments', '3', <span><span className="up">2 running</span> <span className="muted">·</span> 1 paused</span>],
              ['P&L today (paper)', <span className="up">+$142.30</span>, <span><span className="up">+1.42%</span> vs open</span>],
              ['Open positions', '3', <span className="mute">ETH · BTC · SOL · all long</span>],
              ['Eval runs (30d)', '47', <span><span className="up">32 done</span> <span className="muted">·</span> 15 active</span>],
            ].map(([label, value, foot], i) => (
              <div key={i} className="kpi" style={i > 0 ? {borderLeft: '1px solid var(--border)', paddingLeft: 20} : {}}>
                <div className="kpi-label">{label}</div>
                <div className="kpi-value tnum">{value}</div>
                <div className="kpi-foot">{foot}</div>
              </div>
            ))}
          </div>

          {/* Charts row */}
          <div className="grid" style={{gridTemplateColumns: "1.4fr 1fr", marginBottom: 18}}>
            <div className="card">
              <div className="card-h">
                <h2>Equity (paper combined)</h2>
                <div className="toggle-row">
                  <button className="active">1D</button>
                  <button>7D</button>
                  <button>30D</button>
                  <button>90D</button>
                  <button>All</button>
                </div>
              </div>
              <div style={{padding: "0 20px 20px", position: "relative"}}>
                <EquityChart variant={theme} />
                <div className="mono" style={{display: "flex", justifyContent: "space-between", color: "var(--text-3)", fontSize: 10.5, marginTop: 8, paddingLeft: 4}}>
                  <span>00:00</span><span>04:00</span><span>08:00</span><span>12:00</span><span>16:00</span><span>20:00</span><span>24:00</span>
                </div>
                <div className="mono" style={{
                  position: "absolute", left: "55%", top: 50,
                  background: 'var(--surface-elev)',
                  border: '1px solid var(--border-strong)',
                  padding: "8px 12px",
                  borderRadius: 4,
                  fontSize: 11.5,
                }}>
                  <div style={{color: "var(--text-2)"}}>14:37</div>
                  <div className="up" style={{margin: "2px 0", fontWeight: 600}}>+1.42%</div>
                  <div style={{color: "var(--text)"}}>$10,142.30</div>
                </div>
              </div>
            </div>

            <div className="card">
              <div className="card-h">
                <h2>Top strategies (today)</h2>
              </div>
              <table className="tbl">
                <thead><tr><th style={{paddingLeft: 20}}>Strategy</th><th style={{textAlign: "right", paddingRight: 20}}>P&L (paper)</th></tr></thead>
                <tbody>
                  {[
                    ["eth-mr-v3", "+$91.24", "up"],
                    ["btc-momentum-v1", "+$38.12", "up"],
                    ["stablecoin-flow-v1", "+$12.03", "up"],
                    ["sol-trend-follow-v1", "−$4.11", "down"],
                    ["arb-revert-v1", "−$7.42", "down"],
                  ].map(([n, val, c]) => (
                    <tr key={n}><td style={{paddingLeft: 20}} className="mono">{n}</td><td style={{textAlign: "right", paddingRight: 20}} className={"mono " + c}>{val}</td></tr>
                  ))}
                </tbody>
              </table>
              <div style={{padding: "12px 20px"}}>
                <a className="link">View all strategies →</a>
              </div>
            </div>
          </div>

          {/* Bottom row */}
          <div className="grid" style={{gridTemplateColumns: "1.4fr 1fr", marginBottom: 18}}>
            <div className="card">
              <div className="card-h">
                <h2>Recent runs</h2>
                <a className="link">View all →</a>
              </div>
              <table className="tbl">
                <thead>
                  <tr>
                    <th style={{paddingLeft: 20}}>Run ID</th><th>Strategy</th><th>Scenario</th><th>Mode</th><th>Status</th>
                    <th style={{textAlign: "right"}}>Sharpe</th><th style={{textAlign: "right", paddingRight: 20}}>Return</th>
                  </tr>
                </thead>
                <tbody>
                  {[
                    ["01H8N7Z", "eth-mr-v3", "bull-q1-25", "Backtest", "done", "1.62", "+18.4%", "up"],
                    ["01J2P9R", "eth-mr-v3", "chop-q2-25", "Backtest", "done", "0.41", "+3.1%", "up"],
                    ["01K9R5T", "btc-momentum-v1", "bear-q3-24", "Backtest", "done", "−0.18", "−2.4%", "down"],
                    ["01L5A2", "eth-mr-v3", "flash-24-08", "Backtest", "done", "−0.92", "−28.7%", "down"],
                    ["01M7B1", "eth-mr-v3", "bull-q1-25", "Paper", "running", "—", "+1.2%", "up"],
                  ].map(([id, st, sc, m, stat, sh, ret, c]) => (
                    <tr key={id}>
                      <td style={{paddingLeft: 20}} className="mono">{id}</td>
                      <td className="mono">{st}</td>
                      <td className="mono mute">{sc}</td>
                      <td>{m}</td>
                      <td>
                        {stat === 'done' ? (
                          <><span className="dot up"/>Completed</>
                        ) : (
                          <><span className="dot warn"/>Running <span className="mono" style={{color: 'var(--warn)'}}>42%</span>
                            <span style={{display: "inline-block", width: 50, height: 3, background: "var(--border)", borderRadius: 2, marginLeft: 8, verticalAlign: "middle", overflow: "hidden"}}>
                              <span style={{display: "block", width: "42%", height: "100%", background: "var(--warn)"}}/>
                            </span>
                          </>
                        )}
                      </td>
                      <td className="mono" style={{textAlign: "right"}}>{sh}</td>
                      <td className={"mono " + c} style={{textAlign: "right", paddingRight: 20}}>{ret}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            <div className="card">
              <div className="card-h">
                <h2>Open positions</h2>
                <a className="link">View all →</a>
              </div>
              <table className="tbl">
                <thead><tr><th style={{paddingLeft: 20}}>Symbol</th><th>Side</th><th style={{textAlign: "right"}}>Size</th><th style={{textAlign: "right"}}>Mark</th><th style={{textAlign: "right", paddingRight: 20}}>uPnL</th></tr></thead>
                <tbody>
                  {[
                    ["ETH/USD", "Long", "0.05", "2,851.50", "+0.22%", "up"],
                    ["BTC/USD", "Long", "0.01", "67,421.10", "+0.11%", "up"],
                    ["SOL/USD", "Long", "1.20", "152.31", "−0.08%", "down"],
                  ].map(([s, side, sz, mk, p, c]) => (
                    <tr key={s}>
                      <td style={{paddingLeft: 20}} className="mono">{s}</td>
                      <td className="up">{side}</td>
                      <td className="mono" style={{textAlign: "right"}}>{sz}</td>
                      <td className="mono" style={{textAlign: "right"}}>{mk}</td>
                      <td className={"mono " + c} style={{textAlign: "right", paddingRight: 20}}>{p}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <div style={{padding: "12px 20px", color: "var(--text-3)", fontSize: 12}}>All paper · Alpaca paper</div>
            </div>
          </div>

          {/* Quick start */}
          <div className="card">
            <div className="card-h" style={{paddingBottom: 8}}><h2>Quick start</h2></div>
            <div style={{display: "grid", gridTemplateColumns: "repeat(4, 1fr)", paddingBottom: 6, width: '100%'}}>
              {[
                ["code", "Create a new strategy", "Start from scratch or a template"],
                ["play", "Run a backtest", "Test in a scenario"],
                ["chart", "Go live (paper)", "Deploy to paper trading"],
                ["book", "Add to journal", "Capture a finding or note"],
              ].map(([ic, t, s]) => (
                <div key={t} className="qa">
                  <div className="qa-icon"><Icon name={ic} size={14} /></div>
                  <div>
                    <div className="qa-title">{t}</div>
                    <div className="qa-sub">{s}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </main>

        {/* Right rail */}
        <aside className="rail">
          <div className="card" style={{padding: 16}}>
            <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 12}}>
              <span style={{fontSize: 15, fontWeight: 600, color: 'var(--text)'}}>Agent</span>
              <span style={{fontSize: 11.5, color: 'var(--text-2)', display: 'flex', alignItems: 'center'}}>
                <span className="dot accent"/>Online
              </span>
            </div>
            <div style={{fontSize: 13, color: 'var(--text-2)', lineHeight: 1.55, marginBottom: 14}}>
              I can help analyze runs, draft variants, or set up new strategies.
            </div>
            <button className="btn primary" style={{width: "100%", justifyContent: "center"}}>Open xvn agent</button>
          </div>

          <div>
            <h3>Recent activity</h3>
            <div style={{display: "flex", flexDirection: "column", gap: 12}}>
              {[
                ["check", "up", "Run 01H8N7Z completed", "eth-mr-v3 · bull-q1-25", "14m"],
                ["findingDot", "info", "New finding extracted", "regime_fit_mismatch", "22m"],
                ["play", "muted", "Deployment started", "eth-mr-v3 (paper)", "1h"],
                ["branch", "muted", "Draft forked from journal", "funding rate negative", "2h"],
                ["check", "up", "Run 01J2P9R completed", "eth-mr-v3 · chop-q2-25", "3h"],
              ].map(([ic, col, t, s, time], i) => {
                const colorMap = { up: 'var(--up)', info: 'var(--info)', accent: 'var(--accent)', muted: 'var(--text-3)' };
                return (
                  <div key={i} style={{display: "flex", gap: 10, fontSize: 12}}>
                    <div style={{marginTop: 2}}><Icon name={ic} size={14} color={colorMap[col]} /></div>
                    <div style={{flex: 1, minWidth: 0}}>
                      <div style={{color: "var(--text)", fontSize: 13}}>{t}</div>
                      <div className="mono" style={{color: "var(--text-3)", fontSize: 11, marginTop: 2}}>{s}</div>
                    </div>
                    <div className="mono" style={{color: "var(--text-3)", fontSize: 11, whiteSpace: "nowrap"}}>{time}</div>
                  </div>
                );
              })}
            </div>
            <div style={{marginTop: 14}}><a className="link">View all activity →</a></div>
          </div>

          <div>
            <h3>System status</h3>
            <div style={{display: "flex", flexDirection: "column", gap: 8}}>
              {[
                ["Alpaca paper", "Operational"],
                ["Market data", "Operational"],
                ["LLM service", "Operational"],
                ["Backtest engine", "Operational"],
              ].map(([n, s]) => (
                <div key={n} style={{display: "flex", justifyContent: "space-between", fontSize: 12, color: 'var(--text-2)'}}>
                  <span>{n}</span>
                  <span style={{color: 'var(--text-2)'}}><span className="dot up"/>{s}</span>
                </div>
              ))}
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
};

window.ThemedHome = ThemedHome;
