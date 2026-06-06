// xvn — Desktop calendar in-context on the strategies page
// Shows the dual-month popover opened from a filter bar trigger

const StrategiesPageWithCalendar = ({ initialOpen = true }) => {
  const rows = [
    ["eth-mr-v3", "mean_reversion", ["Validated", "gold"], "1.62 · bull-q1-25", "Mar 28, 2025", "53.5k"],
    ["eth-mr-v2", "mean_reversion", ["Validated", "gold"], "0.81 · chop-q2-25", "Feb 14, 2025", "48.2k"],
    ["btc-momentum-v1", "trend_follower", ["Validated", "gold"], "0.92 · bull-q1-25", "Mar 02, 2025", "42.0k"],
    ["sol-trend-follow-v1", "trend_follower", ["Draft", "muted"], "—", "—", "—"],
    ["arb-revert-v1", "stat_arb", ["Warnings", "warn"], "−0.18 · bear-q3-24", "Jan 19, 2025", "67.4k"],
  ];

  return (
    <div style={{
      width: 1280, height: 820,
      background: "var(--bg)",
      color: "var(--text)",
      fontFamily: "Inter, sans-serif",
      display: "grid", gridTemplateColumns: "200px 1fr",
      position: "relative",
      overflow: "hidden",
    }}>
      {/* Sidebar */}
      <aside style={{
        background: "var(--surface-sidebar)",
        borderRight: "1px solid var(--border-soft)",
        padding: "24px 0 16px",
        display: "flex", flexDirection: "column",
      }}>
        <div style={{
          fontFamily: "'Geist', sans-serif", fontStyle: "normal", fontWeight: 500,
          fontSize: 38, color: "var(--text)", padding: "0 24px 32px", letterSpacing: "-0.02em",
        }}>xvn</div>
        <div style={{display: "flex", flexDirection: "column", flex: 1}}>
          {[
            ["Home", "home", false],
            ["Strategies", "chart", true],
            ["Live", "play", false],
            ["Eval", "bars", false],
            ["Journal", "book", false],
            ["Data", "db", false],
            ["Settings", "cog", false],
          ].map(([label, icon, active]) => (
            <div key={label} style={{
              display: "flex", alignItems: "center", gap: 12,
              padding: "10px 24px",
              color: active ? "var(--text)" : "var(--text-2)",
              background: active ? "rgba(0, 230, 118, 0.06)" : "transparent",
              borderLeft: active ? "2px solid var(--gold)" : "2px solid transparent",
              fontSize: 13.5,
            }}>
              <Icon name={icon} size={17} color={active ? "var(--gold)" : "currentColor"}/>
              <span>{label}</span>
            </div>
          ))}
        </div>
      </aside>

      {/* Main */}
      <main style={{padding: "36px 36px 24px", display: "flex", flexDirection: "column", overflow: "hidden"}}>
        <div style={{display: "flex", alignItems: "flex-start", justifyContent: "space-between", marginBottom: 28}}>
          <div>
            <h1 style={{fontFamily: "'Geist', sans-serif", fontWeight: 500, fontSize: 38, margin: "0 0 4px", letterSpacing: "-0.01em"}}>Strategies</h1>
            <div style={{color: "var(--text-2)", fontSize: 14}}>8 drafts · 5 validated · 2 archived</div>
          </div>
          <div style={{
            display: "flex", alignItems: "center", gap: 10, width: 380,
            padding: "9px 12px",
            background: "var(--surface-elev)",
            border: "1px solid var(--border)",
            borderRadius: 4, color: "var(--text-3)", fontSize: 13,
          }}>
            <span style={{
              padding: "2px 6px", border: "1px solid var(--border-strong)", borderRadius: 3,
              fontFamily: "'Geist Mono', monospace", fontSize: 11, color: "var(--text-2)",
            }}>⌘K</span>
            <span style={{flex: 1}}>Jump to anything…</span>
          </div>
        </div>

        {/* Filter row */}
        <div style={{display: "flex", justifyContent: "space-between", alignItems: "center", marginBottom: 14, gap: 12}}>
          <div style={{display: "flex", gap: 10, alignItems: "center"}}>
            <div style={{
              display: "flex", alignItems: "center", gap: 8,
              padding: "8px 12px",
              background: "var(--surface-elev)",
              border: "1px solid var(--border)",
              borderRadius: 4, width: 240,
            }}>
              <Icon name="search" size={14} color="var(--text-3)" />
              <span style={{color: "var(--text-3)", fontSize: 13}}>Filter by name…</span>
            </div>
            <select style={{
              background: "var(--surface-elev)", border: "1px solid var(--border)",
              color: "var(--text-2)", padding: "8px 12px", borderRadius: 4,
              fontFamily: "inherit", fontSize: 13,
            }}><option>All status</option></select>
          </div>
          <div style={{display: "flex", gap: 10}}>
            <button className="btn ghost"><Icon name="plus" size={13}/> New from template</button>
            <button className="btn primary"><Icon name="plus" size={13} color="#000000"/> New strategy</button>
          </div>
        </div>

        {/* Inline swing-out — sits in the page flow, pushes the table down when open */}
        <div style={{marginBottom: 14}}>
          <InlineRangeBar initialOpen={initialOpen} width={"100%"}/>
        </div>

        {/* Table */}
        <div className="card" style={{flex: 1, overflow: "hidden"}}>
          <table className="tbl">
            <thead>
              <tr>
                <th style={{paddingLeft: 20, width: 28}}><input type="checkbox" readOnly/></th>
                <th>Name</th><th>Template</th><th>Status</th>
                <th>Last eval</th>
                <th>Eval date</th>
                <th style={{textAlign: "right"}}>Tokens / run</th>
                <th style={{paddingRight: 20, width: 40}}></th>
              </tr>
            </thead>
            <tbody>
              {rows.map(([n, tpl, [stat, c], le, dt, tk], i) => (
                <tr key={i} className="hover">
                  <td style={{paddingLeft: 20}}><input type="checkbox" readOnly/></td>
                  <td className="mono" style={{color: "var(--text)"}}>{n}</td>
                  <td className="mute">{tpl}</td>
                  <td><span className={`dot ${c}`}/>{stat}</td>
                  <td className="mono mute">{le}</td>
                  <td className="mono mute">{dt}</td>
                  <td className="mono" style={{textAlign: "right"}}>{tk}</td>
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

window.StrategiesPageWithCalendar = StrategiesPageWithCalendar;
