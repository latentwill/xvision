// xvn — Home (Control Tower)
const HomeScreen = () => {
  // Equity curve data
  const equityData = [
    0, 0.05, -0.1, 0.05, -0.05, -0.15, -0.05, 0.1, 0.15, 0.05, -0.1, -0.05,
    0.15, 0.25, 0.2, 0.35, 0.5, 0.45, 0.6, 0.7, 0.55, 0.75, 0.85, 0.9,
    1.05, 1.1, 1.0, 1.15, 1.25, 1.35, 1.3, 1.4, 1.42, 1.45, 1.5, 1.55, 1.5
  ];
  const W = 700, H = 200;
  const min = Math.min(...equityData), max = Math.max(...equityData);
  const range = max - min || 1;
  const pts = equityData.map((v, i) => {
    const x = (i / (equityData.length - 1)) * W;
    const y = H - ((v - min) / range) * (H - 8) - 4;
    return [x, y];
  });
  const linePath = "M" + pts.map(p => p.join(",")).join(" L");
  const areaPath = linePath + ` L${W},${H} L0,${H} Z`;
  const xCrosshair = pts[27][0];

  return (
    <div className="shell with-rail">
      <Sidebar active="home" />
      <main className="main">
        <Topbar title="Good morning, Alex." sub="Here's what's happening across your strategies." />

        {/* KPI row */}
        <div className="grid" style={{gridTemplateColumns: "repeat(4, 1fr)", marginBottom: 18}}>
          <div className="kpi">
            <div className="kpi-label"><Icon name="pulse" size={16} color="var(--text-2)" /> Live deployments</div>
            <div className="kpi-value tnum">3</div>
            <div className="kpi-foot"><span className="up">2 running</span> <span className="muted">·</span> 1 paused</div>
          </div>
          <div className="kpi">
            <div className="kpi-label"><span className="kpi-icon">$</span> P&L today (paper) <Icon name="diamond" size={14} color="var(--gold)" style={{marginLeft: "auto"}} /></div>
            <div className="kpi-value tnum up">+$142.30</div>
            <div className="kpi-foot"><span className="up">+1.42%</span> vs start of day</div>
          </div>
          <div className="kpi">
            <div className="kpi-label"><Icon name="bag" size={16} color="var(--text-2)" /> Open positions</div>
            <div className="kpi-value tnum">3</div>
            <div className="kpi-foot mute">ETH long · BTC long · SOL long</div>
          </div>
          <div className="kpi">
            <div className="kpi-label"><Icon name="barchart" size={16} color="var(--text-2)" /> Eval runs (30d)</div>
            <div className="kpi-value tnum">47</div>
            <div className="kpi-foot"><span className="up">32 completed</span> <span className="muted">·</span> 15 in progress</div>
          </div>
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
              <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" style={{display: "block"}}>
                <defs>
                  <linearGradient id="eqGrad" x1="0" x2="0" y1="0" y2="1">
                    <stop offset="0%" stopColor="#00E676" stopOpacity="0.18"/>
                    <stop offset="100%" stopColor="#00E676" stopOpacity="0"/>
                  </linearGradient>
                </defs>
                {[0, 0.25, 0.5, 0.75, 1].map((t, i) => (
                  <line key={i} x1="0" x2={W} y1={t * H} y2={t * H} stroke="#2A2618" strokeDasharray="2 4" strokeWidth="0.5"/>
                ))}
                <path d={areaPath} fill="url(#eqGrad)"/>
                <path d={linePath} fill="none" stroke="#00E676" strokeWidth="1.5"/>
                <line x1={xCrosshair} x2={xCrosshair} y1={0} y2={H} stroke="#A39A85" strokeDasharray="2 3" strokeWidth="0.5" opacity="0.6"/>
                <circle cx={pts[27][0]} cy={pts[27][1]} r="3" fill="#00E676"/>
                <circle cx={pts[27][0]} cy={pts[27][1]} r="6" fill="#00E676" opacity="0.25"/>
              </svg>
              {/* Y axis labels */}
              <div style={{position: "absolute", left: 24, top: 4, fontSize: 10, color: "var(--text-3)", fontFamily: "monospace"}}>+2.0%</div>
              <div style={{position: "absolute", left: 24, top: 54, fontSize: 10, color: "var(--text-3)", fontFamily: "monospace"}}>+1.0%</div>
              <div style={{position: "absolute", left: 24, top: 104, fontSize: 10, color: "var(--text-3)", fontFamily: "monospace"}}>0%</div>
              <div style={{position: "absolute", left: 24, top: 154, fontSize: 10, color: "var(--text-3)", fontFamily: "monospace"}}>-1.0%</div>
              <div style={{position: "absolute", left: 24, top: 204, fontSize: 10, color: "var(--text-3)", fontFamily: "monospace"}}>-2.0%</div>
              {/* Tooltip */}
              <div style={{position: "absolute", left: "55%", top: 70, background: "var(--surface-elev)", border: "1px solid var(--border)", padding: "10px 14px", borderRadius: 4, fontSize: 12, fontFamily: "JetBrains Mono, monospace"}}>
                <div style={{color: "var(--text-2)"}}>14:37</div>
                <div className="up" style={{margin: "2px 0"}}>+1.42%</div>
                <div style={{color: "var(--text)"}}>$10,142.30</div>
              </div>
              {/* X labels */}
              <div style={{display: "flex", justifyContent: "space-between", color: "var(--text-3)", fontSize: 11, fontFamily: "JetBrains Mono, monospace", marginTop: 8, paddingLeft: 8}}>
                <span>00:00</span><span>04:00</span><span>08:00</span><span>12:00</span><span>16:00</span><span>20:00</span><span>24:00</span>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="card-h">
              <h2>Top strategies by P&amp;L (today)</h2>
            </div>
            <table className="tbl" style={{margin: 0}}>
              <thead><tr><th style={{paddingLeft: 20}}>Strategy</th><th style={{textAlign: "right", paddingRight: 20}}>P&L (paper)</th></tr></thead>
              <tbody>
                {[
                  ["eth-mr-v3", "+$91.24", "up"],
                  ["btc-momentum-v1", "+$38.12", "up"],
                  ["stablecoin-flow-v1", "+$12.03", "up"],
                  ["sol-trend-follow-v1", "−$4.11", "down"],
                  ["arb-revert-v1", "−$7.42", "down"],
                ].map(([n, v, c]) => (
                  <tr key={n}><td style={{paddingLeft: 20}} className="mono">{n}</td><td style={{textAlign: "right", paddingRight: 20}} className={"mono " + c}>{v}</td></tr>
                ))}
              </tbody>
            </table>
            <div style={{padding: "12px 20px"}}>
              <a style={{color: "var(--gold)", textDecoration: "none", fontSize: 13}}>View all strategies →</a>
            </div>
          </div>
        </div>

        {/* Bottom row: recent runs + open positions */}
        <div className="grid" style={{gridTemplateColumns: "1.4fr 1fr", marginBottom: 18}}>
          <div className="card">
            <div className="card-h">
              <h2>Recent runs</h2>
              <a style={{color: "var(--gold)", textDecoration: "none", fontSize: 13}}>View all runs →</a>
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
                  ["01H8N7Z", "eth-mr-v3", "bull-q1-25", "Backtest", ["Completed", "gold"], "1.62", "+18.4%", "up"],
                  ["01J2P9R", "eth-mr-v3", "chop-q2-25", "Backtest", ["Completed", "gold"], "0.41", "+3.1%", "up"],
                  ["01K9R5T", "btc-momentum-v1", "bear-q3-24", "Backtest", ["Completed", "gold"], "−0.18", "−2.4%", "down"],
                  ["01L5A2", "eth-mr-v3", "flash-crash-24-08", "Backtest", ["Completed", "gold"], "−0.92", "−28.7%", "down"],
                  ["01M7B1", "eth-mr-v3", "bull-q1-25", "Paper", ["Running 42%", "warn"], "—", "+1.2%", "up"],
                ].map(([id, st, sc, m, [stat, sc2], sh, ret, c], i) => (
                  <tr key={id}>
                    <td style={{paddingLeft: 20}} className="mono">{id}</td>
                    <td className="mono">{st}</td>
                    <td className="mono mute">{sc}</td>
                    <td>{m}</td>
                    <td>
                      <span className={`dot ${sc2}`}/>
                      {stat.includes("Running") ? (
                        <>Running <span className="mono" style={{color: "var(--warn)"}}>42%</span>
                          <div style={{display: "inline-block", width: 60, height: 3, background: "var(--border)", borderRadius: 2, marginLeft: 8, verticalAlign: "middle", overflow: "hidden"}}>
                            <div style={{width: "42%", height: "100%", background: "var(--warn)"}}/>
                          </div>
                        </>
                      ) : "Completed"}
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
              <a style={{color: "var(--gold)", textDecoration: "none", fontSize: 13}}>View all →</a>
            </div>
            <table className="tbl">
              <thead><tr><th style={{paddingLeft: 20}}>Symbol</th><th>Side</th><th style={{textAlign: "right"}}>Size</th><th style={{textAlign: "right"}}>Mark</th><th style={{textAlign: "right", paddingRight: 20}}>PnL (unrealized)</th></tr></thead>
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
          <div style={{display: "grid", gridTemplateColumns: "repeat(4, 1fr)", paddingBottom: 6}}>
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

      <aside className="rail">
        <div className="card" style={{padding: 16}}>
          <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 14}}>
            <span className="serif" style={{fontSize: 22}}>Agent</span>
            <span style={{fontSize: 12, color: "var(--text-2)"}}><span className="dot gold"/>Online <Icon name="chevR" size={12} color="var(--text-3)" /></span>
          </div>
          <div style={{fontSize: 13, color: "var(--text-2)", lineHeight: 1.55, marginBottom: 12}}>
            I can help you analyze runs, draft variants, or set up new strategies.
          </div>
          <div style={{fontSize: 13, color: "var(--gold)", marginBottom: 16}}>What would you like to do?</div>
          <button className="btn primary" style={{width: "100%", justifyContent: "center"}}>Open xvn agent</button>
        </div>

        <div>
          <h3>Recent activity</h3>
          <div style={{display: "flex", flexDirection: "column", gap: 14}}>
            {[
              ["check", "var(--gold)", "Run 01H8N7Z completed", "eth-mr-v3 · bull-q1-25", "14m ago"],
              ["findingDot", "var(--info)", "New finding extracted", "regime_fit_mismatch from 01H8N7Z", "22m ago"],
              ["play", "var(--text-2)", "Deployment eth-mr-v3", "started (paper)", "1h ago"],
              ["branch", "var(--text-2)", "Draft forked from journal", "note: funding rate negative", "2h ago"],
              ["check", "var(--gold)", "Run 01J2P9R completed", "eth-mr-v3 · chop-q2-25", "3h ago"],
            ].map(([ic, col, t, s, time], i) => (
              <div key={i} style={{display: "flex", gap: 10, fontSize: 12}}>
                <div style={{marginTop: 2}}><Icon name={ic} size={14} color={col} /></div>
                <div style={{flex: 1, minWidth: 0}}>
                  <div style={{color: "var(--text)", fontSize: 13}}>{t}</div>
                  <div style={{color: "var(--text-3)", fontSize: 11, fontFamily: "JetBrains Mono, monospace", marginTop: 2}}>{s}</div>
                </div>
                <div style={{color: "var(--text-3)", fontSize: 11, whiteSpace: "nowrap"}}>{time}</div>
              </div>
            ))}
          </div>
          <div style={{marginTop: 14}}><a style={{color: "var(--gold)", fontSize: 13, textDecoration: "none"}}>View all activity →</a></div>
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
              <div key={n} style={{display: "flex", justifyContent: "space-between", fontSize: 12, color: "var(--text-2)"}}>
                <span>{n}</span>
                <span><span className="dot gold"/>{s}</span>
              </div>
            ))}
          </div>
          <div style={{marginTop: 12}}><a style={{color: "var(--gold)", fontSize: 13, textDecoration: "none"}}>View status page →</a></div>
        </div>
      </aside>
    </div>
  );
};

window.HomeScreen = HomeScreen;
