// xvn — Responsive Tablet (768) and Desktop (1280) views
// Demonstrates how the chat rail scales up

// ============================================================
// TABLET — 768×1024
// Chat occupies a left column (360px), context panel on the right
// ============================================================
const TabletSplit = () => {
  const equity = [0,0.2,0.5,0.3,0.8,1.2,0.9,1.5,2.1,2.8,2.4,3.1,3.8,4.2,3.6,4.8,5.4,6.1,7.2,8.4,9.2,10.4,12.4,14.2,16.8,18.4];
  return (
    <div className="t-shell">
      {/* Left: chat */}
      <div className="left">
        <div className="m-topbar" style={{padding: "16px 18px 12px"}}>
          <button className="m-icon-btn"><Icon name="list" size={18}/></button>
          <span className="m-brand" style={{fontSize: 26}}>xvn</span>
          <div style={{flex: 1}}/>
          <button className="m-icon-btn"><Icon name="plus" size={18}/></button>
        </div>

        <div className="m-thread m-thread--rail" style={{padding: "20px 18px"}}>
          <DayDivider label="14:36" />
          <UserMsg>How did 01H8N7Z compare to the chop run?</UserMsg>

          <AgentMsg time="14:36">
            <div className="m-text">
              In bull-q1-25 it returned <span className="up mono">+18.4%</span> at Sharpe <span className="mono">1.62</span>. In chop-q2-25 (01J2P9R) it managed only <span className="up mono">+3.1%</span> at Sharpe <span className="mono">0.41</span> — and that's with the same parameter set.
            </div>
            <ChatChartCard
              title="01H8N7Z vs 01J2P9R"
              meta="Same strategy · different regimes"
              ret="Δ +15.3%"
              retClass="up"
              data={equity}
              kpis={[["Bull", "+18.4%"], ["Chop", "+3.1%"], ["Δ Sharpe", "1.21"]]}
              ctaLeft="eth-mr-v3"
              ctaRight="Open compare →"
            />
            <div className="m-text mute">The chop windows are where the regime_fit_mismatch finding bites. Want me to draft a variant gated on ADX &gt; 20?</div>
            <div className="m-chips">
              <div className="m-chip gold">Draft ADX-gated variant</div>
              <div className="m-chip">Show chop trades</div>
            </div>
          </AgentMsg>
        </div>

        <QuickRail items={["Re-run", "Draft variant", "Compare more"]} />
        <Composer />
      </div>

      {/* Right: dashboard context */}
      <div className="right">
        <div className="d-topbar">
          <div>
            <h1>Run 01H8N7Z</h1>
            <div className="sub"><span className="mono">eth-mr-v3</span> · <span className="mono">bull-q1-25</span> · <span className="dot gold"/>Completed</div>
          </div>
          <div className="row gap-8">
            <button className="btn ghost" style={{padding: "6px 12px", fontSize: 12}}>Tape</button>
            <button className="btn primary" style={{padding: "6px 12px", fontSize: 12}}>Draft variant</button>
          </div>
        </div>

        <div className="xvn-scroll" style={{padding: "18px 22px", flex: 1, overflowY: "auto", display: "flex", flexDirection: "column", gap: 16}}>
          <div className="grid" style={{gridTemplateColumns: "repeat(4, 1fr)", gap: 10}}>
            {[
              ["Return", "+18.4%", "up", "vs +11.2%"],
              ["Sharpe", "1.62", "", "ann."],
              ["Max DD", "−6.2%", "down", "Feb 8"],
              ["Win", "61%", "", "112/184"],
            ].map(([l, v, c, sub]) => (
              <div className="kpi" key={l} style={{padding: "12px 14px"}}>
                <div className="kpi-label" style={{marginBottom: 6, fontSize: 11}}>{l}</div>
                <div className={"kpi-value tnum " + c} style={{fontSize: 22, marginBottom: 2}}>{v}</div>
                <div className="kpi-foot" style={{fontSize: 11}}>{sub}</div>
              </div>
            ))}
          </div>

          <div className="card">
            <div className="card-h" style={{padding: "14px 18px 8px"}}>
              <h2 style={{fontSize: 19}}>Equity curve</h2>
              <div className="m-segment">
                <button className="active">1M</button>
                <button>3M</button>
                <button>All</button>
              </div>
            </div>
            <div style={{padding: "0 18px 14px"}}>
              <MiniChart data={equity} height={180} width={520} />
              <div className="row" style={{justifyContent: "space-between", fontFamily: "JetBrains Mono, monospace", fontSize: 10.5, color: "var(--text-3)", marginTop: 6}}>
                <span>Jan 1</span><span>Jan 30</span><span>Feb 28</span><span>Mar 31</span>
              </div>
            </div>
          </div>

          <div className="card">
            <div className="card-h" style={{padding: "14px 18px 8px"}}>
              <h2 style={{fontSize: 19}}>Findings <span className="pill" style={{marginLeft: 6, fontSize: 10}}>3</span></h2>
              <a style={{color: "var(--gold)", fontSize: 12, textDecoration: "none"}}>Re-extract</a>
            </div>
            <div style={{padding: "0 18px 16px", display: "flex", flexDirection: "column", gap: 12}}>
              {[
                ["danger", "regime_fit_mismatch", "Underperforms in chop — 8 of 12 windows < 0.4 Sharpe."],
                ["warn", "stop_loss_clustering", "47% of stops within 2× ATR of entry."],
                ["info", "long_holding_outperforms", "Trades held > 4h returned 2.3× short-hold."],
              ].map(([col, kind, sum], i) => (
                <div key={i} className="row" style={{alignItems: "flex-start", gap: 10}}>
                  <span className={`dot ${col}`} style={{marginTop: 6}}/>
                  <div style={{flex: 1}}>
                    <div className="mono" style={{fontSize: 12, color: "var(--text)"}}>{kind}</div>
                    <div className="mute" style={{fontSize: 12.5, marginTop: 2, lineHeight: 1.5}}>{sum}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

// ============================================================
// DESKTOP — 1280×800
// Full three-pane: nav | dashboard | chat rail
// ============================================================
const DesktopThreePane = () => {
  const equityData = [0, 0.05, -0.1, 0.05, -0.05, -0.15, -0.05, 0.1, 0.15, 0.05, -0.1, -0.05, 0.15, 0.25, 0.2, 0.35, 0.5, 0.45, 0.6, 0.7, 0.55, 0.75, 0.85, 0.9, 1.05, 1.1, 1.0, 1.15, 1.25, 1.35, 1.3, 1.4, 1.42, 1.45, 1.5, 1.55, 1.5];
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
    <div className="d-shell">
      {/* Nav */}
      <aside className="nav-col">
        <div style={{padding: "0 22px 24px", fontFamily: "Cormorant Garamond, serif", fontStyle: "italic", fontWeight: 500, fontSize: 32}}>xvn</div>
        <nav className="nav" style={{flex: 1}}>
          {items.map(i => (
            <div key={i.key} className={"nav-item" + (i.key === "home" ? " active" : "")}>
              <Icon name={i.icon} size={16} color={i.key === "home" ? "var(--gold)" : "currentColor"} />
              <span>{i.label}</span>
            </div>
          ))}
        </nav>
        <div className="user-row">
          <div className="avatar">AK</div>
          <div style={{flex: 1, minWidth: 0}}>
            <div style={{fontSize: 12.5, color: "var(--text)"}}>Alex Kim</div>
            <div style={{fontSize: 10.5, color: "var(--text-3)"}}>alex@xvn.dev</div>
          </div>
        </div>
      </aside>

      {/* Main dashboard */}
      <main className="main-col">
        <div className="d-topbar">
          <div>
            <h1>Good morning, Alex.</h1>
            <div className="sub">Here's what's happening across your strategies.</div>
          </div>
          <div className="cmdk" style={{width: 280}}>
            <span className="kbd">⌘K</span>
            <span style={{flex: 1}}>Jump to anything…</span>
          </div>
        </div>

        <div className="xvn-scroll" style={{flex: 1, overflowY: "auto", padding: "20px 24px"}}>
          <div className="grid" style={{gridTemplateColumns: "repeat(4, 1fr)", gap: 12, marginBottom: 14}}>
            {[
              ["Live deployments", "3", "", "2 running · 1 paused"],
              ["P&L today (paper)", "+$142.30", "up", "+1.42% vs SOD"],
              ["Open positions", "3", "", "ETH · BTC · SOL"],
              ["Eval runs (30d)", "47", "", "32 done · 15 wip"],
            ].map(([l, v, c, sub]) => (
              <div className="kpi" key={l} style={{padding: "14px 16px"}}>
                <div className="kpi-label" style={{marginBottom: 8, fontSize: 11}}>{l}</div>
                <div className={"kpi-value tnum " + c} style={{fontSize: 26, marginBottom: 4}}>{v}</div>
                <div className="kpi-foot" style={{fontSize: 11}}>{sub}</div>
              </div>
            ))}
          </div>

          <div className="card" style={{marginBottom: 14}}>
            <div className="card-h" style={{padding: "14px 18px 8px"}}>
              <h2 style={{fontSize: 19}}>Equity (paper combined)</h2>
              <div className="m-segment">
                <button className="active">1D</button>
                <button>7D</button>
                <button>30D</button>
                <button>All</button>
              </div>
            </div>
            <div style={{padding: "0 18px 16px"}}>
              <MiniChart data={equityData} height={180} width={620} />
            </div>
          </div>

          <div className="grid" style={{gridTemplateColumns: "1.3fr 1fr", gap: 14}}>
            <div className="card">
              <div className="card-h" style={{padding: "14px 18px 8px"}}>
                <h2 style={{fontSize: 19}}>Recent runs</h2>
                <a style={{color: "var(--gold)", fontSize: 12, textDecoration: "none"}}>View all →</a>
              </div>
              <table className="tbl">
                <thead>
                  <tr>
                    <th style={{paddingLeft: 18}}>Run</th><th>Strategy</th><th>Status</th>
                    <th style={{textAlign: "right"}}>Sharpe</th><th style={{textAlign: "right", paddingRight: 18}}>Return</th>
                  </tr>
                </thead>
                <tbody>
                  {[
                    ["01H8N7Z", "eth-mr-v3", "Completed", "1.62", "+18.4%", "up"],
                    ["01J2P9R", "eth-mr-v3", "Completed", "0.41", "+3.1%", "up"],
                    ["01K9R5T", "btc-mom-v1", "Completed", "−0.18", "−2.4%", "down"],
                    ["01M7B1", "eth-mr-v3", "Running 42%", "—", "+1.2%", "up"],
                  ].map(([id, s, st, sh, r, c]) => (
                    <tr key={id}>
                      <td style={{paddingLeft: 18}} className="mono">{id}</td>
                      <td className="mono">{s}</td>
                      <td><span className={"dot " + (st.includes("Running") ? "warn" : "gold")}/>{st}</td>
                      <td className="mono" style={{textAlign: "right"}}>{sh}</td>
                      <td className={"mono " + c} style={{textAlign: "right", paddingRight: 18}}>{r}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>

            <div className="card">
              <div className="card-h" style={{padding: "14px 18px 8px"}}>
                <h2 style={{fontSize: 19}}>Top by P&L</h2>
              </div>
              <table className="tbl">
                <tbody>
                  {[
                    ["eth-mr-v3", "+$91.24", "up"],
                    ["btc-momentum-v1", "+$38.12", "up"],
                    ["stablecoin-flow-v1", "+$12.03", "up"],
                    ["sol-trend-follow-v1", "−$4.11", "down"],
                  ].map(([n, v, c]) => (
                    <tr key={n}>
                      <td style={{paddingLeft: 18}} className="mono">{n}</td>
                      <td className={"mono " + c} style={{textAlign: "right", paddingRight: 18}}>{v}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </main>

      {/* Right chat rail */}
      <aside className="chat-col">
        <div className="m-topbar" style={{padding: "16px 18px 12px", background: "transparent"}}>
          <span className="serif" style={{fontFamily: "Cormorant Garamond, serif", fontSize: 22, fontWeight: 500}}>Agent</span>
          <span style={{flex: 1, fontSize: 11, color: "var(--text-2)", marginLeft: 8}}><span className="dot gold"/>Online</span>
          <button className="m-icon-btn"><Icon name="plus" size={16}/></button>
        </div>

        <div className="m-thread m-thread--rail" style={{padding: "16px 16px"}}>
          <DayDivider label="14:36" />

          <AgentMsg time="14:36">
            <div className="m-text">
              Run <span className="mono">01H8N7Z</span> finished — <span className="up mono">+18.4%</span> at Sharpe <span className="mono">1.62</span>. Three findings extracted, one critical.
            </div>
            <ChatChartCard
              title="01H8N7Z"
              meta="bull-q1-25 · backtest"
              ret="+18.4%"
              retClass="up"
              data={[0,0.5,0.8,1.2,2.1,2.8,3.6,4.8,5.4,7.2,8.4,9.2,10.4,12.4,13.6,14.2,15.4,16.8,18.4]}
              kpis={[["Sharpe", "1.62"], ["Max DD", "−6.2%"], ["Win", "61%"]]}
              ctaLeft="vs B&H +11.2%"
              ctaRight="Open →"
            />
            <div className="m-chips">
              <div className="m-chip gold">Draft variant</div>
              <div className="m-chip">Re-run</div>
            </div>
          </AgentMsg>

          <UserMsg>Pin the chop-regime finding to the strategy.</UserMsg>

          <AgentMsg time="14:38">
            <div className="m-text">
              Pinned <span className="mono">regime_fit_mismatch</span> to <span className="mono">eth-mr-v3</span>. It'll surface on the strategy header until you address it or dismiss.
            </div>
            <ChatActionCard
              kind="check"
              title="Finding pinned"
              sub="regime_fit_mismatch → eth-mr-v3"
              actions={["View →"]}
            />
          </AgentMsg>
        </div>

        <QuickRail items={["Draft variant", "Compare runs", "Today's P&L"]} />
        <Composer />
      </aside>
    </div>
  );
};

window.TabletSplit = TabletSplit;
window.DesktopThreePane = DesktopThreePane;
