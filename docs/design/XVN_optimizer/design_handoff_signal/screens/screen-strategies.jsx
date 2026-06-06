// xvn — Strategies list
const StrategiesScreen = () => {
  const rows = [
    ["eth-mr-v3", "mean_reversion", "—", ["Validated", "gold"], "1.62 · bull-q1-25", "53.5k", "14m ago"],
    ["eth-mr-v2", "mean_reversion", "eth-mr-v1", ["Validated", "gold"], "0.81 · chop-q2-25", "48.2k", "2h ago"],
    ["btc-momentum-v1", "trend_follower", "—", ["Validated", "gold"], "0.92 · bull-q1-25", "42.0k", "5h ago"],
    ["sol-trend-follow-v1", "trend_follower", "—", ["Draft", "muted"], "—", "—", "1d ago"],
    ["arb-revert-v1", "stat_arb", "—", ["Warnings", "warn"], "−0.18 · bear-q3-24", "67.4k", "1d ago"],
    ["stablecoin-flow-v1", "carry", "—", ["Validated", "gold"], "0.55 · 90d-paper", "31.0k", "2d ago"],
    ["eth-mr-v1", "mean_reversion", "—", ["Archived", "muted"], "0.42 · bull-q1-25", "44.8k", "1w ago"],
    ["btc-momentum-v0", "trend_follower", "—", ["Archived", "muted"], "0.31 · bull-q1-25", "40.2k", "2w ago"],
  ];
  return (
    <div className="shell">
      <Sidebar active="strategies" />
      <main className="main">
        <Topbar title="Strategies" sub="8 drafts · 5 validated · 2 archived" />
        <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 16}}>
          <div style={{display: "flex", gap: 10}}>
            <div style={{display: "flex", alignItems: "center", gap: 8, padding: "8px 12px", background: "var(--surface-elev)", border: "1px solid var(--border)", borderRadius: 4, width: 280}}>
              <Icon name="search" size={14} color="var(--text-3)" />
              <input className="input" placeholder="Filter by name..." style={{padding: 0, border: "none", background: "transparent", flex: 1, color: "var(--text)"}}/>
            </div>
            <select className="input"><option>All status</option><option>Draft</option><option>Validated</option></select>
            <select className="input"><option>All templates</option></select>
          </div>
          <div style={{display: "flex", gap: 10}}>
            <button className="btn ghost"><Icon name="plus" size={13}/> New from template</button>
            <button className="btn primary"><Icon name="plus" size={13} color="#001A0A"/> New strategy</button>
          </div>
        </div>
        <div className="card">
          <table className="tbl">
            <thead>
              <tr>
                <th style={{paddingLeft: 20, width: 28}}><input type="checkbox"/></th>
                <th>Name</th><th>Template</th><th>Forked from</th><th>Status</th>
                <th>Last eval</th>
                <th style={{textAlign: "right"}}>Tokens / run</th>
                <th>Updated</th>
                <th style={{paddingRight: 20, width: 40}}></th>
              </tr>
            </thead>
            <tbody>
              {rows.map(([n, tpl, fk, [stat, c], le, tk, up], i) => (
                <tr key={i} className="hover">
                  <td style={{paddingLeft: 20}}><input type="checkbox"/></td>
                  <td className="mono" style={{color: "var(--text)"}}>{n}</td>
                  <td className="mute">{tpl}</td>
                  <td className="mute mono">{fk}</td>
                  <td><span className={`dot ${c}`}/>{stat}</td>
                  <td className="mono mute">{le}</td>
                  <td className="mono" style={{textAlign: "right"}}>{tk}</td>
                  <td className="mute">{up}</td>
                  <td style={{paddingRight: 20, color: "var(--text-3)"}}>⋯</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </main>
    </div>
  );
};
window.StrategiesScreen = StrategiesScreen;
