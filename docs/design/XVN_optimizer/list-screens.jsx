// xvn — Strategies screen with standardized ListCard
const StrategiesScreenV2 = () => (
  <div className="shell">
    <Sidebar active="strategies" />
    <main className="main">
      <Topbar title="Strategies" sub="8 drafts · 5 validated · 2 archived" />
      <StrategiesList />
    </main>
  </div>
);

// xvn — Eval runs with standardized ListCard
const EvalRunsScreenV2 = () => (
  <div className="shell">
    <Sidebar active="eval" />
    <main className="main">
      <Topbar title="Eval runs" sub="47 runs · 32 completed · 15 in progress" />
      <div style={{display: "flex", gap: 24, alignItems: "center", marginBottom: 18, borderBottom: "1px solid var(--border-soft)"}}>
        {["All", "Mine", "Published evals"].map((t, i) => (
          <div key={t} style={{
            padding: "8px 0", fontSize: 14,
            color: i === 0 ? "var(--text)" : "var(--text-2)",
            borderBottom: i === 0 ? "2px solid var(--gold)" : "2px solid transparent",
            marginBottom: -1, cursor: "pointer"
          }}>{t}</div>
        ))}
      </div>
      <EvalRunsList />
    </main>
  </div>
);

// xvn — Run detail with two compact lists side by side
const RunDetailScreenV2 = () => (
  <div className="shell">
    <Sidebar active="eval" />
    <main className="main">
      <div style={{display: "flex", justifyContent: "space-between", alignItems: "flex-start", marginBottom: 18}}>
        <div>
          <div style={{fontSize: 11, color: "var(--text-3)", letterSpacing: "0.08em", textTransform: "uppercase", marginBottom: 4}}>Eval / Runs / 01H8N7Z</div>
          <h1 className="serif" style={{margin: 0, fontSize: 34}}>Run 01H8N7Z</h1>
          <div className="mute" style={{fontSize: 13, marginTop: 2}}>
            <span className="mono">eth-mr-v3</span> · <span className="mono">bull-q1-25</span> ·{" "}
            <span className="pill" style={{marginLeft: 6}}>Backtest</span> ·{" "}
            <span className="dot gold"/>Completed
          </div>
        </div>
        <div style={{display: "flex", gap: 8}}>
          <button className="btn ghost">Download tape</button>
          <button className="btn ghost">Re-run</button>
          <button className="btn primary">Draft variant →</button>
        </div>
      </div>

      <div className="grid" style={{gridTemplateColumns: "repeat(4, 1fr)", marginBottom: 18}}>
        {[
          ["Total return", "+18.4%", "up"],
          ["Sharpe", "1.62", ""],
          ["Max drawdown", "−6.2%", "down"],
          ["Win rate", "61%", ""],
        ].map(([l, v, c]) => (
          <div className="kpi" key={l}>
            <div className="kpi-label">{l}</div>
            <div className={"kpi-value tnum " + c}>{v}</div>
          </div>
        ))}
      </div>

      <div className="grid" style={{gridTemplateColumns: "1fr 1fr", gap: 18}}>
        <DecisionsList />
        <TradeLedgerList />
      </div>
    </main>
  </div>
);

const TradeLedgerList = () => {
  const trades = [
    { t: "01-08 14:22", side: "Long", q: "0.05", e: "2,851.50", x: "2,902.30", p: 2.54, added: 1 },
    { t: "01-12 09:15", side: "Long", q: "0.05", e: "2,810.20", x: "2,875.80", p: 3.28, added: 2 },
    { t: "01-15 03:48", side: "Long", q: "0.05", e: "2,902.30", x: "2,887.10", p: -0.76, added: 3 },
    { t: "01-19 11:02", side: "Long", q: "0.05", e: "2,755.80", x: "2,838.40", p: 4.13, added: 4 },
    { t: "01-22 16:30", side: "Long", q: "0.05", e: "2,820.60", x: "2,801.50", p: -0.96, added: 5 },
    { t: "01-26 08:04", side: "Long", q: "0.05", e: "2,710.30", x: "2,761.20", p: 2.55, added: 6 },
    { t: "01-29 21:11", side: "Long", q: "0.05", e: "2,668.90", x: "2,712.40", p: 2.17, added: 7 },
    { t: "02-02 12:47", side: "Long", q: "0.05", e: "2,668.10", x: "2,652.40", p: -0.79, added: 8 },
    { t: "02-05 17:22", side: "Long", q: "0.05", e: "2,612.10", x: "2,668.30", p: 2.81, added: 9 },
    { t: "02-08 09:36", side: "Long", q: "0.05", e: "2,612.10", x: "2,580.80", p: -1.56, added: 10 },
  ];
  const list = useListState({
    rows: trades,
    filters: [
      { id: "outcome", label: "Outcome", options: [
        { value: "all", label: "All" }, { value: "win", label: "Wins" }, { value: "loss", label: "Losses" },
      ]},
    ],
    sortOptions: [
      { value: "added", label: "Recent" },
      { value: "pnl-desc", label: "PnL (high → low)" },
      { value: "pnl-asc", label: "PnL (low → high)" },
    ],
    filterFn: (r, q, f) => {
      const qq = q.trim().toLowerCase();
      if (qq && !r.t.toLowerCase().includes(qq)) return false;
      if (f.outcome === "win" && r.p < 0) return false;
      if (f.outcome === "loss" && r.p >= 0) return false;
      return true;
    },
    sortFn: (rows, k) =>
      k === "pnl-desc" ? rows.sort((a,b) => b.p - a.p) :
      k === "pnl-asc"  ? rows.sort((a,b) => a.p - b.p) :
      rows.sort((a,b) => a.added - b.added),
  });

  return (
    <ListCard
      title="Trade ledger"
      count={184}
      density="compact"
      toolbar={{
        ...list,
        search: { ...list.search, placeholder: "Date…" },
      }}
      columns={[
        { key: "t", label: "Time" }, { key: "side", label: "Side" },
        { key: "q", label: "Qty", align: "right" },
        { key: "e", label: "Entry", align: "right" },
        { key: "x", label: "Exit", align: "right" },
        { key: "p", label: "PnL", align: "right" },
      ]}
      rows={list.rows}
      renderRow={(r, i) => (
        <tr key={i}>
          <td className="mono mute">{r.t}</td>
          <td className="up">{r.side}</td>
          <td className="mono" style={{textAlign: "right"}}>{r.q}</td>
          <td className="mono" style={{textAlign: "right"}}>{r.e}</td>
          <td className="mono" style={{textAlign: "right"}}>{r.x}</td>
          <td className={"mono " + (r.p >= 0 ? "up" : "down")} style={{textAlign: "right"}}>
            {(r.p >= 0 ? "+$" : "−$") + Math.abs(r.p).toFixed(2)}
          </td>
        </tr>
      )}
      footer={<><span>Showing {list.rows.length} of 184</span><a style={{color: "var(--gold)", textDecoration: "none"}}>Load more →</a></>}
    />
  );
};

// Home with compact lists
const HomeScreenV2 = () => (
  <div className="shell with-rail">
    <Sidebar active="home" />
    <main className="main">
      <Topbar title="Good morning, Alex." sub="Here's what's happening across your strategies." />
      <div className="grid" style={{gridTemplateColumns: "repeat(4, 1fr)", marginBottom: 18}}>
        {[
          ["Live deployments", "3", ""],
          ["P&L today", "+$142.30", "up"],
          ["Open positions", "3", ""],
          ["Eval runs (30d)", "47", ""],
        ].map(([l, v, c]) => (
          <div className="kpi" key={l}>
            <div className="kpi-label">{l}</div>
            <div className={"kpi-value tnum " + c}>{v}</div>
          </div>
        ))}
      </div>
      <div className="grid" style={{gridTemplateColumns: "1.4fr 1fr", gap: 18}}>
        <RecentRunsList />
        <PositionsList />
      </div>
    </main>
    <aside className="rail">
      <h3>Activity</h3>
      <div style={{color: "var(--text-2)", fontSize: 12, lineHeight: 1.6}}>
        Right rail shown for context. The same compact pattern works here too — see Positions to the left.
      </div>
    </aside>
  </div>
);

window.StrategiesScreenV2 = StrategiesScreenV2;
window.EvalRunsScreenV2 = EvalRunsScreenV2;
window.RunDetailScreenV2 = RunDetailScreenV2;
window.HomeScreenV2 = HomeScreenV2;
window.TradeLedgerList = TradeLedgerList;
