// xvn — Eval run detail
const RunDetailScreen = () => {
  const equityData = [0, 0.2, 0.5, 0.3, 0.8, 1.2, 0.9, 1.5, 2.1, 2.8, 2.4, 3.1, 3.8, 4.2, 3.6, 4.8, 5.4, 6.1, 7.2, 8.4, 7.8, 9.2, 10.4, 11.8, 12.4, 13.6, 14.2, 15.4, 16.8, 17.2, 18.4];
  const W = 720, H = 200;
  const min = Math.min(...equityData), max = Math.max(...equityData);
  const range = max - min || 1;
  const pts = equityData.map((v, i) => {
    const x = (i / (equityData.length - 1)) * W;
    const y = H - ((v - min) / range) * (H - 8) - 4;
    return [x, y];
  });
  const linePath = "M" + pts.map(p => p.join(",")).join(" L");
  const areaPath = linePath + ` L${W},${H} L0,${H} Z`;
  // Buy & hold baseline
  const bhData = equityData.map((_, i) => i / equityData.length * 11.2);
  const bhPts = bhData.map((v, i) => `${(i/(bhData.length-1))*W},${H - ((v-min)/range)*(H-8) - 4}`).join(" L");
  // Trade markers
  const markers = [3, 7, 12, 18, 22, 25];

  return (
    <div className="shell">
      <Sidebar active="eval" />
      <main className="main">
        <div style={{display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 18}}>
          <div>
            <div style={{fontSize: 11, color: "var(--text-3)", letterSpacing: "0.08em", textTransform: "uppercase", marginBottom: 4}}>Eval / Runs / 01H8N7Z</div>
            <h1 className="serif" style={{margin: 0, fontSize: 34}}>Run 01H8N7Z</h1>
            <div className="mute" style={{fontSize: 13, marginTop: 2}}><span className="mono">eth-mr-v3</span> · <span className="mono">bull-q1-25</span> · <span className="pill" style={{marginLeft: 6}}>Backtest</span> · <span className="dot gold"/>Completed</div>
          </div>
          <div style={{display: "flex", gap: 8}}>
            <button className="btn ghost">Download tape</button>
            <button className="btn ghost">Compare with…</button>
            <button className="btn ghost">Re-run</button>
            <button className="btn primary">Draft variant from this →</button>
          </div>
        </div>

        {/* KPI tiles */}
        <div className="grid" style={{gridTemplateColumns: "repeat(4, 1fr)", marginBottom: 18}}>
          {[
            ["Total return", "+18.4%", "up", "vs benchmark +11.2%"],
            ["Sharpe", "1.62", "", "annualized"],
            ["Max drawdown", "−6.2%", "down", "on 2025-02-08"],
            ["Win rate", "61%", "", "112 / 184 trades"],
          ].map(([l, v, c, sub]) => (
            <div className="kpi" key={l}>
              <div className="kpi-label">{l}</div>
              <div className={"kpi-value tnum " + (c || "")}>{v}</div>
              <div className="kpi-foot">{sub}</div>
            </div>
          ))}
        </div>

        {/* Equity curve */}
        <div className="card" style={{marginBottom: 18}}>
          <div className="card-h">
            <h2>Equity curve</h2>
            <div style={{display: "flex", gap: 16, alignItems: "center"}}>
              <span style={{fontSize: 12, color: "var(--text-2)"}}><span style={{display: "inline-block", width: 10, height: 1.5, background: "var(--gold)", marginRight: 6}}/>This run</span>
              <span style={{fontSize: 12, color: "var(--text-2)"}}><span style={{display: "inline-block", width: 10, height: 1.5, background: "var(--text-3)", marginRight: 6}}/>Buy & hold</span>
              <span style={{fontSize: 12, color: "var(--text-2)"}}><input type="checkbox" defaultChecked style={{marginRight: 4, accentColor: "var(--gold)"}}/>Show drawdown</span>
            </div>
          </div>
          <div style={{padding: "0 20px 20px", position: "relative"}}>
            <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" style={{display: "block"}}>
              <defs>
                <linearGradient id="rdGrad" x1="0" x2="0" y1="0" y2="1">
                  <stop offset="0%" stopColor="#D4A547" stopOpacity="0.18"/>
                  <stop offset="100%" stopColor="#D4A547" stopOpacity="0"/>
                </linearGradient>
              </defs>
              {[0, 0.25, 0.5, 0.75, 1].map((t, i) => (
                <line key={i} x1="0" x2={W} y1={t * H} y2={t * H} stroke="#2A2618" strokeDasharray="2 4" strokeWidth="0.5"/>
              ))}
              <path d={areaPath} fill="url(#rdGrad)"/>
              <path d={linePath} fill="none" stroke="#D4A547" strokeWidth="1.5"/>
              <path d={"M" + bhPts} fill="none" stroke="#6B6553" strokeWidth="1" strokeDasharray="3 3"/>
              {markers.map((mi, i) => (
                <g key={i}>
                  <circle cx={pts[mi][0]} cy={pts[mi][1]} r="3" fill={i % 3 === 2 ? "#C8443A" : "#D4A547"}/>
                </g>
              ))}
            </svg>
            <div style={{display: "flex", justifyContent: "space-between", color: "var(--text-3)", fontSize: 11, fontFamily: "JetBrains Mono, monospace", marginTop: 8}}>
              <span>Jan 1</span><span>Jan 15</span><span>Feb 1</span><span>Feb 15</span><span>Mar 1</span><span>Mar 15</span><span>Mar 31</span>
            </div>
          </div>
        </div>

        {/* Findings + ledger */}
        <div className="grid" style={{gridTemplateColumns: "1fr 1fr", gap: 18}}>
          <div className="card">
            <div className="card-h">
              <h2>Findings <span className="pill" style={{marginLeft: 8}}>3</span></h2>
              <button className="btn ghost" style={{padding: "4px 10px", fontSize: 12}}>Re-extract</button>
            </div>
            <div style={{padding: "0 20px 16px", display: "flex", flexDirection: "column", gap: 14}}>
              {[
                ["danger", "regime_fit_mismatch", "Strategy underperforms in chop regimes — 8 of 12 chop windows produced < 0.4 Sharpe.", "critical"],
                ["warn", "stop_loss_clustering", "47% of stops triggered within 2× ATR of entry; tighten or widen.", "warning"],
                ["info", "long_holding_outperforms", "Trades held > 4h returned 2.3× short-hold trades.", "info"],
              ].map(([col, kind, summary, sev], i) => (
                <div key={i} style={{display: "flex", gap: 12, alignItems: "flex-start"}}>
                  <span className={`dot ${col}`} style={{marginTop: 5}}/>
                  <div style={{flex: 1, minWidth: 0}}>
                    <div style={{fontFamily: "JetBrains Mono, monospace", fontSize: 12, color: "var(--text)"}}>{kind}</div>
                    <div className="mute" style={{fontSize: 12, lineHeight: 1.55, marginTop: 2}}>{summary}</div>
                    <div style={{display: "flex", gap: 12, marginTop: 8}}>
                      <a style={{color: "var(--gold)", fontSize: 12, textDecoration: "none"}}>Draft variant from this →</a>
                      <a style={{color: "var(--text-2)", fontSize: 12, textDecoration: "none"}}>Evidence ↗</a>
                      <a style={{color: "var(--text-2)", fontSize: 12, textDecoration: "none"}}>Add to journal</a>
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          <div className="card">
            <div className="card-h">
              <h2>Trade ledger <span className="pill" style={{marginLeft: 8}}>184</span></h2>
              <span className="mute" style={{fontSize: 12}}>Showing 5 of 184</span>
            </div>
            <table className="tbl">
              <thead><tr><th style={{paddingLeft: 20}}>Time</th><th>Side</th><th style={{textAlign: "right"}}>Qty</th><th style={{textAlign: "right"}}>Entry</th><th style={{textAlign: "right"}}>Exit</th><th style={{textAlign: "right", paddingRight: 20}}>PnL</th></tr></thead>
              <tbody>
                {[
                  ["01-08 14:22", "Long", "0.05", "2,851.50", "2,902.30", "+$2.54", "up"],
                  ["01-12 09:15", "Long", "0.05", "2,810.20", "2,875.80", "+$3.28", "up"],
                  ["01-15 03:48", "Long", "0.05", "2,902.30", "2,887.10", "−$0.76", "down"],
                  ["01-19 11:02", "Long", "0.05", "2,755.80", "2,838.40", "+$4.13", "up"],
                  ["01-22 16:30", "Long", "0.05", "2,820.60", "2,801.50", "−$0.96", "down"],
                ].map(([t, s, q, e, x, p, c], i) => (
                  <tr key={i}>
                    <td style={{paddingLeft: 20}} className="mono mute">{t}</td>
                    <td className="up">{s}</td>
                    <td className="mono" style={{textAlign: "right"}}>{q}</td>
                    <td className="mono" style={{textAlign: "right"}}>{e}</td>
                    <td className="mono" style={{textAlign: "right"}}>{x}</td>
                    <td className={"mono " + c} style={{textAlign: "right", paddingRight: 20}}>{p}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </main>
    </div>
  );
};
window.RunDetailScreen = RunDetailScreen;
