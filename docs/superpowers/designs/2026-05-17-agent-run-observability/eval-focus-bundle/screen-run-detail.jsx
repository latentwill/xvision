// xvn — Eval run detail
const RunDetailScreen = () => {
  const [decFilter, setDecFilter] = React.useState("all");
  // Per-bar trade decisions emitted by the strategy.
  // [time, action, symbol, price, qty, conviction, reasoning]
  const decisions = [
    ["01-08 14:22:00", "BUY",   "ETH/USD", "2,851.50", "0.05", 0.78, "RSI 27 + bb_lower touch; ADX 24 (trending)"],
    ["01-08 15:00:00", "HOLD",  "ETH/USD", "2,863.10", "—",    0.42, "RSI rising, no exit signal"],
    ["01-08 18:00:00", "SELL",  "ETH/USD", "2,902.30", "0.05", 0.71, "Target reached; conviction decayed below 0.5"],
    ["01-09 02:00:00", "HOLD",  "ETH/USD", "2,895.40", "—",    0.18, "Regime: chop; standing aside"],
    ["01-12 09:15:00", "BUY",   "ETH/USD", "2,810.20", "0.05", 0.82, "RSI 24 + volume confirmation; bb_lower"],
    ["01-12 14:30:00", "HOLD",  "ETH/USD", "2,838.00", "—",    0.61, "Mid-trade, trailing stop unchanged"],
    ["01-12 19:45:00", "CLOSE", "ETH/USD", "2,875.80", "0.05", 0.55, "Trailing stop hit at +2.33%"],
    ["01-15 03:48:00", "BUY",   "ETH/USD", "2,902.30", "0.05", 0.51, "Weak signal — RSI 31, no bb_lower"],
    ["01-15 09:10:00", "CLOSE", "ETH/USD", "2,887.10", "0.05", 0.39, "Stop-loss triggered (−0.52%)"],
    ["01-19 11:02:00", "BUY",   "ETH/USD", "2,755.80", "0.05", 0.88, "Strong reversal setup; multi-timeframe agree"],
    ["01-19 17:30:00", "HOLD",  "ETH/USD", "2,798.40", "—",    0.74, "Trailing stop tightened"],
    ["01-20 02:15:00", "SELL",  "ETH/USD", "2,838.40", "0.05", 0.81, "Take-profit T2 hit"],
    ["01-22 16:30:00", "BUY",   "ETH/USD", "2,820.60", "0.05", 0.55, "Borderline; RSI 29.4"],
    ["01-22 23:00:00", "CLOSE", "ETH/USD", "2,801.50", "0.05", 0.41, "Time-stop after 6h, marginal loss"],
    ["01-26 08:04:00", "BUY",   "ETH/USD", "2,710.30", "0.05", 0.84, "Sharp dip + funding negative"],
    ["01-26 14:18:00", "SELL",  "ETH/USD", "2,761.20", "0.05", 0.69, "Conviction decayed; +1.88%"],
    ["01-29 21:11:00", "BUY",   "ETH/USD", "2,668.90", "0.05", 0.76, "Bb_lower + RSI 26"],
    ["02-02 12:47:00", "CLOSE", "ETH/USD", "2,652.40", "0.05", 0.32, "Stop-loss at −0.62%"],
    ["02-05 17:22:00", "BUY",   "ETH/USD", "2,612.10", "0.05", 0.79, "Capitulation candle"],
    ["02-08 09:36:00", "CLOSE", "ETH/USD", "2,580.80", "0.05", 0.22, "Drawdown stop fired (−1.20%)"],
  ];
  const actionMeta = {
    BUY:   { color: "var(--gold)",   bg: "rgba(212,165,71,0.12)", border: "rgba(212,165,71,0.4)" },
    SELL:  { color: "var(--info)",   bg: "rgba(111,143,184,0.12)", border: "rgba(111,143,184,0.45)" },
    HOLD:  { color: "var(--text-2)", bg: "rgba(163,154,133,0.08)", border: "var(--border)" },
    CLOSE: { color: "var(--danger)", bg: "rgba(200,68,58,0.10)",  border: "rgba(200,68,58,0.4)" },
  };
  const counts = {
    all:   decisions.length,
    BUY:   decisions.filter(d => d[1] === "BUY").length,
    SELL:  decisions.filter(d => d[1] === "SELL").length,
    HOLD:  decisions.filter(d => d[1] === "HOLD").length,
    CLOSE: decisions.filter(d => d[1] === "CLOSE").length,
  };
  const filtered = decFilter === "all" ? decisions : decisions.filter(d => d[1] === decFilter);
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
      <main className="main xvn-scroll" style={{overflowY: "auto"}}>
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
          <div className="card" style={{display: "flex", flexDirection: "column", maxHeight: 380}}>
            <div className="card-h">
              <h2>Decisions <span className="pill" style={{marginLeft: 8}}>{decisions.length}</span></h2>
              <button className="btn ghost" style={{padding: "4px 10px", fontSize: 12}}>Export ledger</button>
            </div>
            <div className="dec-filter">
              {[
                ["all",   "All",   "muted",     "var(--text-2)"],
                ["BUY",   "Buy",   "gold",      "var(--gold)"],
                ["SELL",  "Sell",  "info",      "var(--info)"],
                ["HOLD",  "Hold",  "muted",     "var(--text-3)"],
                ["CLOSE", "Close", "danger",    "var(--danger)"],
              ].map(([k, label, col]) => (
                <button
                  key={k}
                  className={"dec-pill" + (decFilter === k ? " active" : "")}
                  onClick={() => setDecFilter(k)}
                >
                  <span className={`dot ${col}`} style={{marginRight: 0}}/>
                  <span>{label}</span>
                  <span className="n">{counts[k]}</span>
                </button>
              ))}
            </div>
            <div className="xvn-scroll xvn-scroll--always" style={{flex: 1, overflowY: "scroll", padding: 0}}>
              <table className="tbl" style={{margin: 0}}>
                <thead style={{position: "sticky", top: 0, background: "var(--surface-card)", zIndex: 2}}>
                  <tr>
                    <th style={{paddingLeft: 20}}>Time</th>
                    <th>Action</th>
                    <th>Symbol</th>
                    <th style={{textAlign: "right"}}>Price</th>
                    <th style={{textAlign: "right"}}>Qty</th>
                    <th style={{textAlign: "right"}}>Conv.</th>
                    <th style={{paddingRight: 20}}>Reasoning</th>
                  </tr>
                </thead>
                <tbody>
                  {filtered.length === 0 && (
                    <tr><td colSpan="7" className="mute" style={{padding: "16px 20px"}}>No decisions match this filter.</td></tr>
                  )}
                  {filtered.map(([t, action, sym, price, qty, conv, reason], i) => {
                    const m = actionMeta[action];
                    return (
                      <tr key={i}>
                        <td style={{paddingLeft: 20, whiteSpace: "nowrap"}} className="mono mute">{t}</td>
                        <td>
                          <span style={{
                            display: "inline-block",
                            padding: "2px 8px",
                            borderRadius: 3,
                            border: `1px solid ${m.border}`,
                            background: m.bg,
                            color: m.color,
                            fontFamily: "JetBrains Mono, monospace",
                            fontSize: 11,
                            fontWeight: 500,
                            letterSpacing: "0.04em",
                            minWidth: 48,
                            textAlign: "center",
                          }}>{action}</span>
                        </td>
                        <td className="mono">{sym}</td>
                        <td className="mono" style={{textAlign: "right"}}>{price}</td>
                        <td className="mono mute" style={{textAlign: "right"}}>{qty}</td>
                        <td className="mono" style={{textAlign: "right", color: conv >= 0.7 ? "var(--gold)" : conv >= 0.4 ? "var(--text)" : "var(--text-3)"}}>
                          {conv.toFixed(2)}
                        </td>
                        <td className="mute" style={{paddingRight: 20, fontSize: 12, maxWidth: 260, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis"}}>
                          {reason}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          </div>

          <div className="card" style={{display: "flex", flexDirection: "column", maxHeight: 380}}>
            <div className="card-h">
              <h2>Trade ledger <span className="pill" style={{marginLeft: 8}}>184</span></h2>
              <span className="mute" style={{fontSize: 12}}>Showing 12 of 184</span>
            </div>
            <div className="xvn-scroll xvn-scroll--always" style={{flex: 1, overflowY: "scroll"}}>
              <table className="tbl" style={{margin: 0}}>
                <thead style={{position: "sticky", top: 0, background: "var(--surface-card)", zIndex: 2}}>
                  <tr><th style={{paddingLeft: 20}}>Time</th><th>Side</th><th style={{textAlign: "right"}}>Qty</th><th style={{textAlign: "right"}}>Entry</th><th style={{textAlign: "right"}}>Exit</th><th style={{textAlign: "right", paddingRight: 20}}>PnL</th></tr>
                </thead>
                <tbody>
                  {[
                    ["01-08 14:22", "Long", "0.05", "2,851.50", "2,902.30", "+$2.54", "up"],
                    ["01-12 09:15", "Long", "0.05", "2,810.20", "2,875.80", "+$3.28", "up"],
                    ["01-15 03:48", "Long", "0.05", "2,902.30", "2,887.10", "−$0.76", "down"],
                    ["01-19 11:02", "Long", "0.05", "2,755.80", "2,838.40", "+$4.13", "up"],
                    ["01-22 16:30", "Long", "0.05", "2,820.60", "2,801.50", "−$0.96", "down"],
                    ["01-26 08:04", "Long", "0.05", "2,710.30", "2,761.20", "+$2.55", "up"],
                    ["01-29 21:11", "Long", "0.05", "2,668.90", "2,712.40", "+$2.17", "up"],
                    ["02-02 12:47", "Long", "0.05", "2,668.10", "2,652.40", "−$0.79", "down"],
                    ["02-05 17:22", "Long", "0.05", "2,612.10", "2,668.30", "+$2.81", "up"],
                    ["02-08 09:36", "Long", "0.05", "2,612.10", "2,580.80", "−$1.56", "down"],
                    ["02-11 15:08", "Long", "0.05", "2,594.20", "2,648.40", "+$2.71", "up"],
                    ["02-14 11:55", "Long", "0.05", "2,652.80", "2,712.00", "+$2.96", "up"],
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
        </div>
      </main>
    </div>
  );
};
window.RunDetailScreen = RunDetailScreen;
