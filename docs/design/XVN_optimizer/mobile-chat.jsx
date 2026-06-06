// xvn — Mobile chat screens (default + active conversation + drawer + sheet)

// Sample equity data
const equityToday = [0, 0.1, -0.05, 0.05, 0.18, 0.32, 0.25, 0.38, 0.52, 0.48, 0.65, 0.78, 0.72, 0.88, 1.05, 1.12, 1.08, 1.25, 1.32, 1.42];
const equityMR = [0, 0.05, 0.12, 0.08, 0.22, 0.35, 0.48, 0.55, 0.62, 0.71, 0.84, 0.91];
const equityBTC = [0, 0.1, 0.05, 0.18, 0.12, 0.28, 0.31, 0.42, 0.38, 0.51];
const equityStable = [0, 0.02, 0.04, 0.03, 0.06, 0.08, 0.07, 0.09, 0.11, 0.12];

// ============================================================
// 1. CHAT — HOME / EMPTY
// ============================================================
const MobileChatHome = () => (
  <div className="m-frame">
    <MobileTopBar />

    <div className="m-thread">
      <DayDivider label="Tue · May 14 · 09:42" />

      <AgentMsg time="09:42">
        <div className="m-text serif" style={{fontFamily: "Geist, sans-serif", fontSize: 22, lineHeight: 1.25, color: "var(--text)", marginBottom: 2}}>
          Good morning, Alex.
        </div>
        <div className="m-text mute">
          Overnight: <span className="up mono">eth-mr-v3</span> filled 3 trades on Alpaca paper · <span className="up mono">+$91.24</span>. One eval finished — <span className="mono">01J2P9R</span> on chop-q2-25 came in at <span className="up mono">+3.1%</span> / Sharpe <span className="mono">0.41</span>. Want a look?
        </div>
        <ChatChartCard
          title="Combined equity (24h)"
          meta="3 strategies · paper"
          ret="+1.42%"
          retClass="up"
          data={equityToday}
          kpis={[["P&L", "+$142.30"], ["Sharpe", "1.62"], ["Trades", "8"]]}
          ctaLeft="ETH · BTC · SOL"
          ctaRight="See breakdown →"
        />
        <div className="m-chips" style={{marginTop: 2}}>
          <div className="m-chip gold">Open 01J2P9R</div>
          <div className="m-chip">Compare with 01H8N7Z</div>
          <div className="m-chip">Draft a variant</div>
        </div>
      </AgentMsg>
    </div>

    <QuickRail items={["Run a backtest", "Today's P&L", "Pause eth-mr-v3", "New strategy", "Journal"]} />
    <Composer />
  </div>
);

// ============================================================
// 2. CHAT — ACTIVE CONVERSATION (eval inline)
// ============================================================
const MobileChatEval = () => (
  <div className="m-frame">
    <MobileTopBar context="eth-mr-v3" />

    <div className="m-thread">
      <DayDivider label="14:36" />

      <UserMsg>How did my last eval run go? Show me the chart.</UserMsg>

      <AgentMsg time="14:36">
        <div className="m-text">
          <span className="mono">01H8N7Z</span> finished 14 minutes ago — <span className="mono">eth-mr-v3</span> on <span className="mono">bull-q1-25</span>. Total return <span className="up mono">+18.4%</span> vs benchmark <span className="mono">+11.2%</span>. Sharpe came in at <span className="mono">1.62</span>.
        </div>
        <ChatChartCard
          title="Equity curve · 01H8N7Z"
          meta="Jan 1 → Mar 31 · backtest"
          ret="+18.4%"
          retClass="up"
          data={[0,0.5,0.8,1.2,2.1,2.8,3.6,4.8,5.4,7.2,8.4,9.2,10.4,12.4,13.6,14.2,15.4,16.8,18.4]}
          kpis={[["Sharpe", "1.62"], ["Max DD", "−6.2%"], ["Win rate", "61%"]]}
          ctaLeft="vs Buy & Hold +11.2%"
          ctaRight="Open run →"
        />
        <div className="m-text mute" style={{fontSize: 13.5}}>
          Three findings extracted — one critical: the strategy underperforms in chop regimes (Sharpe &lt; 0.4 in 8 of 12 chop windows).
        </div>
        <div className="m-chips">
          <div className="m-chip gold">Draft variant from finding</div>
          <div className="m-chip">Re-run on chop-q2-25</div>
          <div className="m-chip">Trade ledger</div>
          <div className="m-chip">Findings (3)</div>
        </div>
      </AgentMsg>
    </div>

    <QuickRail items={["Compare runs", "Edit strategy", "Promote to paper", "Re-run"]} />
    <Composer value="Pin the chop-regime finding…" />
  </div>
);

// ============================================================
// 3. NAV DRAWER OPEN
// ============================================================
const MobileDrawer = () => {
  const items = [
    { key: "home", label: "Home", icon: "home" },
    { key: "strategies", label: "Strategies", icon: "chart", count: "8" },
    { key: "live", label: "Live", icon: "play", count: "2" },
    { key: "eval", label: "Eval", icon: "bars", count: "47" },
    { key: "journal", label: "Journal", icon: "book" },
    { key: "data", label: "Data", icon: "db" },
    { key: "settings", label: "Settings", icon: "cog" },
  ];
  return (
    <div className="m-frame">
      {/* Background — chat dimmed */}
      <MobileTopBar />
      <div className="m-thread" style={{opacity: 0.35}}>
        <AgentMsg time="09:42">
          <div className="m-text serif" style={{fontFamily: "Geist, sans-serif", fontSize: 22}}>Good morning, Alex.</div>
          <div className="m-text mute">Overnight: eth-mr-v3 filled 3 trades…</div>
        </AgentMsg>
      </div>
      <Composer />

      <div className="m-overlay"/>

      <aside className="m-drawer">
        <div className="head">
          <span className="m-brand">xvn</span>
          <button className="m-icon-btn"><Icon name="arrow" size={16} color="var(--text-2)" /></button>
        </div>
        <nav className="m-nav">
          {items.map(i => (
            <div key={i.key} className={"m-nav-item" + (i.key === "home" ? " active" : "")}>
              <Icon name={i.icon} size={17} color={i.key === "home" ? "var(--gold)" : "currentColor"} />
              <span>{i.label}</span>
              {i.count && <span className="count">{i.count}</span>}
            </div>
          ))}
        </nav>
        <div className="footer-card">
          <h4>Conversations</h4>
          <p>Resume a past thread or start fresh.</p>
          <button className="btn ghost" style={{width: "100%", justifyContent: "center", padding: "8px"}}>View history →</button>
        </div>
        <div className="user-row">
          <div className="avatar">AK</div>
          <div style={{flex: 1, minWidth: 0}}>
            <div style={{fontSize: 13, color: "var(--text)"}}>Alex Kim</div>
            <div style={{fontSize: 11, color: "var(--text-3)"}}>alex@xvn.dev</div>
          </div>
          <Icon name="settings" size={14} color="var(--text-3)" />
        </div>
      </aside>
    </div>
  );
};

// ============================================================
// 4. FUNCTIONS BOTTOM SHEET (the "+" button)
// ============================================================
const MobileSheet = () => (
  <div className="m-frame">
    <MobileTopBar />
    <div className="m-thread" style={{opacity: 0.35}}>
      <AgentMsg time="14:36">
        <div className="m-text mute">01H8N7Z finished 14 minutes ago…</div>
      </AgentMsg>
    </div>

    <div className="m-overlay"/>
    <div className="m-sheet">
      <div className="grip"/>
      <div className="head">
        <h3>All functions</h3>
        <button className="m-icon-btn"><Icon name="search" size={16}/></button>
      </div>
      <div className="scroller">
        <div className="m-sheet-group">
          <div className="label">Create</div>
          <div className="m-tile-grid">
            <div className="m-tile gold">
              <div className="ic"><Icon name="code" size={14}/></div>
              <div className="t">New strategy</div>
              <div className="s">Start from scratch or a template</div>
            </div>
            <div className="m-tile">
              <div className="ic"><Icon name="branch" size={14}/></div>
              <div className="t">Draft variant</div>
              <div className="s">Fork an existing strategy</div>
            </div>
            <div className="m-tile">
              <div className="ic"><Icon name="play" size={14}/></div>
              <div className="t">Run backtest</div>
              <div className="s">Pick scenario + horizon</div>
            </div>
            <div className="m-tile">
              <div className="ic"><Icon name="book" size={14}/></div>
              <div className="t">Journal note</div>
              <div className="s">Capture a finding</div>
            </div>
          </div>
        </div>

        <div className="m-sheet-group">
          <div className="label">Inspect</div>
          <div className="m-list-row">
            <div className="ic"><Icon name="bars" size={14}/></div>
            <div className="b">
              <div className="t">Open a run</div>
              <div className="s">Chart, ledger, findings</div>
            </div>
            <Icon name="chevR" size={14} color="var(--text-3)"/>
          </div>
          <div className="m-list-row">
            <div className="ic"><Icon name="sliders" size={14}/></div>
            <div className="b">
              <div className="t">Compare runs</div>
              <div className="s">Up to 4 side-by-side</div>
            </div>
            <Icon name="chevR" size={14} color="var(--text-3)"/>
          </div>
          <div className="m-list-row">
            <div className="ic"><Icon name="findingDot" size={14}/></div>
            <div className="b">
              <div className="t">Findings library</div>
              <div className="s">12 unread across 4 strategies</div>
            </div>
            <Icon name="chevR" size={14} color="var(--text-3)"/>
          </div>
        </div>

        <div className="m-sheet-group">
          <div className="label">Live</div>
          <div className="m-list-row">
            <div className="ic" style={{background: "var(--gold-bg)", color: "var(--gold)"}}><Icon name="play" size={14}/></div>
            <div className="b">
              <div className="t">Deploy to paper</div>
              <div className="s">Alpaca paper · 1 strategy ready</div>
            </div>
            <Icon name="chevR" size={14} color="var(--text-3)"/>
          </div>
          <div className="m-list-row">
            <div className="ic"><Icon name="flame" size={14}/></div>
            <div className="b">
              <div className="t">Pause / resume</div>
              <div className="s">2 live deployments</div>
            </div>
            <Icon name="chevR" size={14} color="var(--text-3)"/>
          </div>
        </div>
      </div>
    </div>
  </div>
);

window.MobileChatHome = MobileChatHome;
window.MobileChatEval = MobileChatEval;
window.MobileDrawer = MobileDrawer;
window.MobileSheet = MobileSheet;
