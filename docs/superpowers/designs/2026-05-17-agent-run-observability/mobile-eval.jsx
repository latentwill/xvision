// xvn — Mobile dashboards (Eval list, Run detail) with chat pill
// ============================================================
// 5. EVAL RUNS LIST — full-screen dashboard, chat collapses to pill
// ============================================================
const MobileEvalList = () => {
  const rows = [
    ["01H8N7Z", "eth-mr-v3", "bull-q1-25",   "+18.4%", "up",   "1.62",  "61%", "184", "14m ago", "gold",
      [0,0.3,0.6,1.1,1.8,2.6,3.4,4.6,6.1,7.8,9.4,11.2,13.0,15.0,16.4,17.8,18.4]],
    ["01J2P9R", "eth-mr-v3", "chop-q2-25",   "+3.1%",  "up",   "0.41",  "48%", "201", "1h ago",  "gold",
      [0,0.4,-0.2,0.6,-0.1,0.8,1.1,0.6,1.5,2.0,1.4,2.2,2.6,2.1,2.9,3.1]],
    ["01K9R5T", "btc-momentum-v1", "bear-q3-24", "−2.4%", "down", "−0.18","44%", "97",  "2h ago",  "gold",
      [0,-0.3,0.1,-0.5,-0.2,-0.8,-1.2,-0.7,-1.5,-2.0,-1.4,-2.6,-2.4]],
    ["01L5A2", "eth-mr-v3", "flash-crash-24-08", "−28.7%", "down", "−0.92","38%","44","3h ago","gold",
      [0,1.2,2.4,3.6,2.8,-8.2,-14.5,-22.1,-26.4,-28.7]],
    ["01M7B1", "eth-mr-v3", "bull-q1-25",   "+1.2%",  "up",   "—",     "—",   "12",  "live",    "warn",
      [0,0.2,0.4,0.3,0.6,0.9,0.8,1.0,1.2]],
    ["01N3Q8", "stablecoin-flow-v1", "carry-90d", "+5.1%", "up", "0.55", "78%", "412", "5h ago", "gold",
      [0,0.4,0.8,1.4,2.0,2.6,3.2,3.8,4.2,4.6,5.0,5.1]],
  ];
  return (
    <div className="m-frame">
      <MobileTopBar title="Eval runs" />

      <div className="m-dash" style={{paddingBottom: 86}}>
        <div className="row" style={{justifyContent: "space-between"}}>
          <div className="m-segment">
            <button className="active">All · 47</button>
            <button>Mine</button>
            <button>Pinned</button>
          </div>
          <button className="m-icon-btn" style={{border: "1px solid var(--border)", borderRadius: 8, width: 34, height: 34}}>
            <Icon name="sliders" size={15}/>
          </button>
        </div>

        <div className="m-filterbar">
          <span className="m-chip gold">Strategy: eth-mr-v3 ×</span>
          <span className="m-chip">Mode: All</span>
          <span className="m-chip">Status: All</span>
          <span className="m-chip">Sort: Recent</span>
        </div>

        {/* Aggregate inline mini-chart card */}
        <div className="m-card" style={{padding: "14px 14px 12px"}}>
          <div style={{display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 8}}>
            <div>
              <div style={{fontFamily: "Cormorant Garamond, serif", fontSize: 17, fontWeight: 500}}>Returns distribution</div>
              <div className="mono" style={{fontSize: 10.5, color: "var(--text-3)", letterSpacing: "0.05em", marginTop: 2}}>30D · 47 RUNS</div>
            </div>
            <div className="row" style={{gap: 6, fontSize: 11, color: "var(--text-2)", fontFamily: "JetBrains Mono, monospace"}}>
              <span className="up">μ +4.8%</span>
              <span>·</span>
              <span>σ 9.2%</span>
            </div>
          </div>
          <svg width="100%" height="64" viewBox="0 0 320 64" preserveAspectRatio="none" style={{display: "block"}}>
            {/* histogram bars */}
            {[6, 10, 14, 22, 30, 42, 38, 28, 18, 10, 6, 4].map((h, i) => {
              const cls = i < 4 ? "#C8443A" : i < 8 ? "#D4A547" : "#B8862E";
              return <rect key={i} x={i * 26 + 4} y={64 - h} width="20" height={h} fill={cls} opacity={i === 5 || i === 6 ? 0.85 : 0.45}/>;
            })}
            <line x1="160" x2="160" y1="0" y2="64" stroke="#A39A85" strokeDasharray="2 3" strokeWidth="0.5"/>
          </svg>
          <div className="row" style={{justifyContent: "space-between", fontFamily: "JetBrains Mono, monospace", fontSize: 10, color: "var(--text-3)", marginTop: 4}}>
            <span>−30%</span><span>−10%</span><span>0</span><span>+10%</span><span>+30%</span>
          </div>
        </div>

        {/* Run cards */}
        {rows.map(([id, strat, sc, ret, retCls, sh, wr, n, t, stCls, spark]) => (
          <div key={id} className="m-runcard">
            <div className="row1">
              <div style={{minWidth: 0, flex: 1}}>
                <div className="id">{id} <span className={`dot ${stCls}`} style={{marginLeft: 6}}/></div>
                <div className="sub">{strat} · {sc}</div>
              </div>
              <div className={"ret " + retCls}>{ret}</div>
            </div>
            <div className="spark-row">
              <div style={{width: "55%"}}>
                <MiniChart data={spark} width={170} height={38} color={retCls === "up" ? "var(--gold)" : "var(--danger)"} />
              </div>
              <div className="stats">
                <div className="col">
                  <span style={{fontSize: 10, color: "var(--text-3)", textTransform: "uppercase", letterSpacing: "0.05em"}}>Sharpe</span>
                  <span className="v">{sh}</span>
                </div>
                <div className="col">
                  <span style={{fontSize: 10, color: "var(--text-3)", textTransform: "uppercase", letterSpacing: "0.05em"}}>Win</span>
                  <span className="v">{wr}</span>
                </div>
                <div className="col">
                  <span style={{fontSize: 10, color: "var(--text-3)", textTransform: "uppercase", letterSpacing: "0.05em"}}>N</span>
                  <span className="v">{n}</span>
                </div>
              </div>
            </div>
            <div className="row" style={{justifyContent: "space-between", paddingTop: 6, borderTop: "1px solid var(--border-soft)"}}>
              <span className="mono" style={{fontSize: 11, color: "var(--text-3)"}}>{t}</span>
              <a style={{color: "var(--gold)", fontSize: 12, textDecoration: "none"}}>Open run →</a>
            </div>
          </div>
        ))}
      </div>

      {/* Floating chat pill — chat is always reachable */}
      <div className="m-chat-pill">
        <div className="av">x</div>
        <div className="field">Ask xvn about these runs…</div>
        <button className="send"><Icon name="arrow" size={14} color="#0F0E0C"/></button>
      </div>
    </div>
  );
};

// ============================================================
// 6. RUN DETAIL — full-screen with chart
// ============================================================
const MobileRunDetail = () => {
  const [decFilter, setDecFilter] = React.useState("all");
  const decisions = [
    ["danger", "regime_fit_mismatch",    "Underperforms in chop regimes — 8 of 12 windows produced < 0.4 Sharpe."],
    ["warn",   "stop_loss_clustering",   "47% of stops triggered within 2× ATR of entry — tighten or widen."],
    ["info",   "long_holding_outperforms","Trades held > 4h returned 2.3× short-hold trades."],
    ["warn",   "session_bias_us_open",   "62% of P&L concentrated in 13:30–15:30 UTC window."],
    ["info",   "entry_slippage_low",     "Median slippage on entry 1.2 bps — well under 5 bps budget."],
    ["danger", "fee_drag_high_freq",     "Maker/taker fees consumed 18% of gross P&L in chop windows."],
    ["info",   "volatility_filter_helps","ATR > 1.4σ filter raises Sharpe to 1.84 on backtest replay."],
  ];
  const counts = {
    all:     decisions.length,
    danger:  decisions.filter(d => d[0] === "danger").length,
    warn:    decisions.filter(d => d[0] === "warn").length,
    info:    decisions.filter(d => d[0] === "info").length,
  };
  const filtered = decFilter === "all" ? decisions : decisions.filter(d => d[0] === decFilter);
  const equityData = [0, 0.5, 0.8, 1.2, 2.1, 2.8, 2.4, 3.1, 3.8, 4.2, 3.6, 4.8, 5.4, 6.1, 7.2, 8.4, 7.8, 9.2, 10.4, 11.8, 12.4, 13.6, 14.2, 15.4, 16.8, 17.2, 18.4];
  const W = 320, H = 140;
  const min = Math.min(...equityData), max = Math.max(...equityData);
  const range = max - min || 1;
  const pad = 6;
  const pts = equityData.map((v, i) => {
    const x = (i / (equityData.length - 1)) * W;
    const y = H - ((v - min) / range) * (H - pad * 2) - pad;
    return [x, y];
  });
  const linePath = "M" + pts.map(p => p.join(",")).join(" L");
  const areaPath = linePath + ` L${W},${H} L0,${H} Z`;
  const bhPts = equityData.map((_, i) => {
    const v = i / equityData.length * 11.2;
    return `${(i/(equityData.length-1))*W},${H - ((v-min)/range)*(H-pad*2) - pad}`;
  }).join(" L");

  return (
    <div className="m-frame">
      <div className="m-topbar">
        <button className="m-icon-btn"><Icon name="arrow" size={18} color="var(--text)"/></button>
        <div style={{flex: 1, minWidth: 0}}>
          <div className="mono" style={{fontSize: 10.5, color: "var(--text-3)", letterSpacing: "0.06em", textTransform: "uppercase"}}>Eval / Runs</div>
          <div className="mono" style={{fontSize: 14, color: "var(--text)"}}>01H8N7Z</div>
        </div>
        <button className="m-icon-btn"><Icon name="sliders" size={18}/></button>
        <button className="m-icon-btn"><Icon name="branch" size={18}/></button>
      </div>

      <div className="m-dash" style={{paddingBottom: 90}}>
        <div className="row gap-8" style={{flexWrap: "wrap", fontSize: 12, color: "var(--text-2)"}}>
          <span className="mono">eth-mr-v3</span><span>·</span>
          <span className="mono">bull-q1-25</span><span>·</span>
          <span className="pill">Backtest</span>
          <span><span className="dot gold"/>Completed</span>
        </div>

        {/* KPI strip */}
        <div className="grid" style={{gridTemplateColumns: "repeat(2, 1fr)", gap: 8}}>
          <div className="kpi" style={{padding: "12px 14px"}}>
            <div className="kpi-label" style={{marginBottom: 6, fontSize: 11}}>Total return</div>
            <div className="kpi-value tnum up" style={{fontSize: 26, marginBottom: 2}}>+18.4%</div>
            <div className="kpi-foot" style={{fontSize: 11}}>vs B&H +11.2%</div>
          </div>
          <div className="kpi" style={{padding: "12px 14px"}}>
            <div className="kpi-label" style={{marginBottom: 6, fontSize: 11}}>Sharpe</div>
            <div className="kpi-value tnum" style={{fontSize: 26, marginBottom: 2}}>1.62</div>
            <div className="kpi-foot" style={{fontSize: 11}}>annualized</div>
          </div>
          <div className="kpi" style={{padding: "12px 14px"}}>
            <div className="kpi-label" style={{marginBottom: 6, fontSize: 11}}>Max DD</div>
            <div className="kpi-value tnum down" style={{fontSize: 26, marginBottom: 2}}>−6.2%</div>
            <div className="kpi-foot" style={{fontSize: 11}}>on 2025-02-08</div>
          </div>
          <div className="kpi" style={{padding: "12px 14px"}}>
            <div className="kpi-label" style={{marginBottom: 6, fontSize: 11}}>Win rate</div>
            <div className="kpi-value tnum" style={{fontSize: 26, marginBottom: 2}}>61%</div>
            <div className="kpi-foot" style={{fontSize: 11}}>112 / 184</div>
          </div>
        </div>

        {/* Equity curve */}
        <div className="card">
          <div className="card-h" style={{padding: "14px 16px 8px"}}>
            <h2 style={{fontSize: 18}}>Equity curve</h2>
            <div className="m-segment">
              <button className="active">1M</button>
              <button>3M</button>
              <button>All</button>
            </div>
          </div>
          <div style={{padding: "0 14px 12px", position: "relative"}}>
            <svg width="100%" height={H} viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" style={{display: "block"}}>
              <defs>
                <linearGradient id="rdGradM" x1="0" x2="0" y1="0" y2="1">
                  <stop offset="0%" stopColor="#D4A547" stopOpacity="0.22"/>
                  <stop offset="100%" stopColor="#D4A547" stopOpacity="0"/>
                </linearGradient>
              </defs>
              {[0, 0.25, 0.5, 0.75, 1].map((t, i) => (
                <line key={i} x1="0" x2={W} y1={t * H} y2={t * H} stroke="#2A2618" strokeDasharray="2 4" strokeWidth="0.5"/>
              ))}
              <path d={areaPath} fill="url(#rdGradM)"/>
              <path d={linePath} fill="none" stroke="#D4A547" strokeWidth="1.5"/>
              <path d={"M" + bhPts} fill="none" stroke="#6B6553" strokeWidth="1" strokeDasharray="3 3"/>
              {[3, 7, 12, 18, 22].map((mi, i) => (
                <circle key={i} cx={pts[mi][0]} cy={pts[mi][1]} r="3" fill={i === 2 ? "#C8443A" : "#D4A547"}/>
              ))}
            </svg>
            <div className="row" style={{justifyContent: "space-between", fontFamily: "JetBrains Mono, monospace", fontSize: 10, color: "var(--text-3)", marginTop: 6}}>
              <span>Jan 1</span><span>Jan 30</span><span>Feb 28</span><span>Mar 31</span>
            </div>
            <div className="row gap-12" style={{marginTop: 10, fontSize: 11, color: "var(--text-2)"}}>
              <span><span style={{display: "inline-block", width: 10, height: 1.5, background: "var(--gold)", marginRight: 5, verticalAlign: "middle"}}/>This run</span>
              <span><span style={{display: "inline-block", width: 10, height: 1.5, background: "var(--text-3)", marginRight: 5, verticalAlign: "middle"}}/>Buy & hold</span>
            </div>
          </div>
        </div>

        {/* Decisions (findings) — internal scroll with themed scrollbar */}
        <div className="card">
          <div className="card-h" style={{padding: "14px 16px 6px"}}>
            <h2 style={{fontSize: 18}}>Decisions <span className="pill" style={{marginLeft: 6, fontSize: 10}}>{decisions.length}</span></h2>
            <a style={{color: "var(--gold)", fontSize: 12, textDecoration: "none"}}>Re-extract</a>
          </div>
          <div style={{padding: "0 12px 8px"}}>
            <div className="dec-filter" style={{padding: "0 4px 6px"}}>
              {[
                ["all",    "All",      "muted"],
                ["danger", "Critical", "danger"],
                ["warn",   "Warning",  "warn"],
                ["info",   "Insight",  "info"],
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
          </div>
          <div className="xvn-scroll" style={{padding: "0 16px 14px", display: "flex", flexDirection: "column", gap: 14, maxHeight: 220, overflowY: "auto"}}>
            {filtered.length === 0 && (
              <div className="mute" style={{fontSize: 12.5, padding: "12px 4px"}}>No decisions match this filter.</div>
            )}
            {filtered.map(([col, kind, summary], i) => (
              <div key={i} style={{display: "flex", gap: 10, alignItems: "flex-start"}}>
                <span className={`dot ${col}`} style={{marginTop: 6}}/>
                <div style={{flex: 1, minWidth: 0}}>
                  <div className="mono" style={{fontSize: 12, color: "var(--text)"}}>{kind}</div>
                  <div className="mute" style={{fontSize: 12.5, lineHeight: 1.5, marginTop: 3}}>{summary}</div>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Trade ledger — internal scroll with themed scrollbar */}
        <div className="card">
          <div className="card-h" style={{padding: "14px 16px 6px"}}>
            <h2 style={{fontSize: 18}}>Trade ledger</h2>
            <span className="mute" style={{fontSize: 11}}>showing 12 of 184</span>
          </div>
          <div className="xvn-scroll" style={{maxHeight: 240, overflowY: "auto"}}>
            <table className="tbl">
              <tbody>
                {[
                  ["01-08 14:22", "Long",  "+$2.54", "up"],
                  ["01-12 09:15", "Long",  "+$3.28", "up"],
                  ["01-15 03:48", "Long",  "−$0.76", "down"],
                  ["01-19 11:02", "Long",  "+$4.13", "up"],
                  ["01-22 16:30", "Long",  "−$0.96", "down"],
                  ["01-26 08:04", "Long",  "+$5.18", "up"],
                  ["01-29 21:11", "Short", "+$1.82", "up"],
                  ["02-02 12:47", "Long",  "−$1.34", "down"],
                  ["02-05 17:22", "Long",  "+$2.91", "up"],
                  ["02-08 09:36", "Long",  "−$6.20", "down"],
                  ["02-11 15:08", "Long",  "+$3.74", "up"],
                  ["02-14 11:55", "Long",  "+$4.46", "up"],
                ].map(([t, s, p, c], i) => (
                  <tr key={i}>
                    <td style={{paddingLeft: 16, fontSize: 12}} className="mono mute">{t}</td>
                    <td className={s === "Short" ? "down" : "up"} style={{fontSize: 12}}>{s}</td>
                    <td className={"mono " + c} style={{textAlign: "right", paddingRight: 16, fontSize: 12.5}}>{p}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* Chat pill */}
      <div className="m-chat-pill">
        <div className="av">x</div>
        <div className="field">Draft a variant from regime_fit_mismatch…</div>
        <button className="send"><Icon name="arrow" size={14} color="#0F0E0C"/></button>
      </div>
    </div>
  );
};

window.MobileEvalList = MobileEvalList;
window.MobileRunDetail = MobileRunDetail;
