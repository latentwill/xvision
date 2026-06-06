// xvn — Eval runs list
const EvalRunsScreen = () => {
  const rows = [
    ["01H8N7Z", "eth-mr-v3", "bull-q1-25", "Backtest", ["Completed", "gold"], "1.62", "+18.4%", "−6.2%", "61%", "184", "53.5k", "14m ago"],
    ["01J2P9R", "eth-mr-v3", "chop-q2-25", "Backtest", ["Completed", "gold"], "0.41", "+3.1%", "−4.8%", "48%", "201", "53.5k", "1h ago"],
    ["01K9R5T", "btc-momentum-v1", "bear-q3-24", "Backtest", ["Completed", "gold"], "−0.18", "−2.4%", "−12.1%", "44%", "97", "42.0k", "2h ago"],
    ["01L5A2", "eth-mr-v3", "flash-crash-24-08", "Backtest", ["Completed", "gold"], "−0.92", "−28.7%", "−31.4%", "38%", "44", "53.5k", "3h ago"],
    ["01M7B1", "eth-mr-v3", "bull-q1-25", "Paper", ["Running 42%", "warn"], "—", "+1.2%", "−0.4%", "—", "12", "21.4k", "live"],
    ["01N3Q8", "stablecoin-flow-v1", "carry-90d", "Backtest", ["Completed", "gold"], "0.55", "+5.1%", "−1.2%", "78%", "412", "31.0k", "5h ago"],
    ["01P2K4", "btc-momentum-v1", "bull-q1-25", "Backtest", ["Completed", "gold"], "0.92", "+12.3%", "−5.4%", "55%", "143", "42.0k", "7h ago"],
    ["01Q1A7", "sol-trend-follow-v1", "bear-q3-24", "Backtest", ["Failed", "danger"], "—", "—", "—", "—", "—", "0", "8h ago"],
    ["01R7H3", "eth-mr-v2", "chop-q2-25", "Backtest", ["Completed", "gold"], "0.81", "+6.8%", "−3.7%", "52%", "198", "48.2k", "12h ago"],
    ["01S5T9", "eth-mr-v3", "bear-q3-24", "Backtest", ["Queued", "muted"], "—", "—", "—", "—", "—", "—", "—"],
  ];
  return (
    <div className="shell">
      <Sidebar active="eval" />
      <main className="main">
        <Topbar title="Eval runs" sub="47 runs · 32 completed · 15 in progress" />
        <div style={{display: "flex", gap: 24, alignItems: "center", marginBottom: 16, borderBottom: "1px solid var(--border-soft)", paddingBottom: 0}}>
          {["All", "Mine", "Published evals"].map((t, i) => (
            <div key={t} style={{
              padding: "6px 0", fontSize: 14,
              color: i === 0 ? "var(--text)" : "var(--text-2)",
              borderBottom: i === 0 ? "2px solid var(--gold)" : "2px solid transparent",
              marginBottom: -1, cursor: "pointer"
            }}>{t}</div>
          ))}
          <div style={{marginLeft: "auto", display: "flex", gap: 8}}>
            <button className="btn ghost">Compare selected (0)</button>
            <button className="btn primary"><Icon name="plus" size={13} color="#001A0A"/> New run</button>
          </div>
        </div>

        <div style={{display: "flex", gap: 8, alignItems: "center", marginBottom: 16, flexWrap: "wrap"}}>
          <span className="pill gold">Strategy: eth-mr-v3 ×</span>
          <span className="pill">Scenario: bull-q1-25, chop-q2-25 ×</span>
          <select className="input"><option>Mode: All</option></select>
          <select className="input"><option>Status: All</option></select>
          <select className="input"><option>Started: any</option></select>
          <select className="input" style={{marginLeft: "auto"}}><option>Sort: Most recent</option></select>
        </div>

        <div className="card">
          <table className="tbl">
            <thead>
              <tr>
                <th style={{paddingLeft: 20, width: 28}}><input type="checkbox"/></th>
                <th>Run ID</th><th>Strategy</th><th>Scenario</th><th>Mode</th><th>Status</th>
                <th style={{textAlign: "right"}}>Sharpe</th>
                <th style={{textAlign: "right"}}>Return</th>
                <th style={{textAlign: "right"}}>Max DD</th>
                <th style={{textAlign: "right"}}>Win rate</th>
                <th style={{textAlign: "right"}}>Trades</th>
                <th style={{textAlign: "right"}}>Tokens</th>
                <th style={{paddingRight: 20}}>Started</th>
              </tr>
            </thead>
            <tbody>
              {rows.map(([id, st, sc, m, [stat, c], sh, ret, dd, wr, n, tk, t], i) => {
                const retClass = ret.includes("+") ? "up" : ret.includes("−") ? "down" : "mute";
                return (
                  <tr key={id} className="hover">
                    <td style={{paddingLeft: 20}}><input type="checkbox"/></td>
                    <td className="mono" style={{color: "var(--text)"}}>{id}</td>
                    <td className="mono">{st}</td>
                    <td className="mono mute">{sc}</td>
                    <td>{m}</td>
                    <td><span className={`dot ${c}`}/>{stat}</td>
                    <td className="mono" style={{textAlign: "right"}}>{sh}</td>
                    <td className={"mono " + retClass} style={{textAlign: "right"}}>{ret}</td>
                    <td className="mono down" style={{textAlign: "right"}}>{dd}</td>
                    <td className="mono" style={{textAlign: "right"}}>{wr}</td>
                    <td className="mono" style={{textAlign: "right"}}>{n}</td>
                    <td className="mono mute" style={{textAlign: "right"}}>{tk}</td>
                    <td className="mute" style={{paddingRight: 20}}>{t}</td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      </main>
    </div>
  );
};
window.EvalRunsScreen = EvalRunsScreen;
