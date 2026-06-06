// xvn — List component application examples
// Demonstrates the standard ListCard across every list surface in the app.

// ── Data (re-used from existing screens) ────────────────────────────────────
const STRAT_ROWS = [
  { name: "eth-mr-v3",          tpl: "mean_reversion",  forked: "—",          status: "Validated", color: "gold",   lastEval: "1.62 · bull-q1-25",  tokens: "53.5k", tokensN: 53500, updated: "14m ago",  updatedRank: 1,  addedRank: 1 },
  { name: "eth-mr-v2",          tpl: "mean_reversion",  forked: "eth-mr-v1",  status: "Validated", color: "gold",   lastEval: "0.81 · chop-q2-25",  tokens: "48.2k", tokensN: 48200, updated: "2h ago",   updatedRank: 2,  addedRank: 5 },
  { name: "btc-momentum-v1",    tpl: "trend_follower",  forked: "—",          status: "Validated", color: "gold",   lastEval: "0.92 · bull-q1-25",  tokens: "42.0k", tokensN: 42000, updated: "5h ago",   updatedRank: 3,  addedRank: 4 },
  { name: "sol-trend-follow-v1",tpl: "trend_follower",  forked: "—",          status: "Draft",     color: "muted",  lastEval: "—",                  tokens: "—",     tokensN: 0,     updated: "1d ago",   updatedRank: 4,  addedRank: 2 },
  { name: "arb-revert-v1",      tpl: "stat_arb",        forked: "—",          status: "Warnings",  color: "warn",   lastEval: "−0.18 · bear-q3-24", tokens: "67.4k", tokensN: 67400, updated: "1d ago",   updatedRank: 5,  addedRank: 3 },
  { name: "stablecoin-flow-v1", tpl: "carry",           forked: "—",          status: "Validated", color: "gold",   lastEval: "0.55 · 90d-paper",   tokens: "31.0k", tokensN: 31000, updated: "2d ago",   updatedRank: 6,  addedRank: 6 },
  { name: "eth-mr-v1",          tpl: "mean_reversion",  forked: "—",          status: "Archived",  color: "muted",  lastEval: "0.42 · bull-q1-25",  tokens: "44.8k", tokensN: 44800, updated: "1w ago",   updatedRank: 7,  addedRank: 7 },
  { name: "btc-momentum-v0",    tpl: "trend_follower",  forked: "—",          status: "Archived",  color: "muted",  lastEval: "0.31 · bull-q1-25", tokens: "40.2k", tokensN: 40200, updated: "2w ago",   updatedRank: 8,  addedRank: 8 },
];

const RUN_ROWS = [
  { id: "01H8N7Z", strat: "eth-mr-v3",          scen: "bull-q1-25",       mode: "Backtest", status: "Completed",   color: "gold",   sharpe: 1.62,  ret: 18.4,   dd: -6.2,  wr: 61, n: 184, tok: "53.5k", t: "14m ago",  added: 1 },
  { id: "01J2P9R", strat: "eth-mr-v3",          scen: "chop-q2-25",       mode: "Backtest", status: "Completed",   color: "gold",   sharpe: 0.41,  ret: 3.1,    dd: -4.8,  wr: 48, n: 201, tok: "53.5k", t: "1h ago",   added: 2 },
  { id: "01K9R5T", strat: "btc-momentum-v1",    scen: "bear-q3-24",       mode: "Backtest", status: "Completed",   color: "gold",   sharpe: -0.18, ret: -2.4,   dd: -12.1, wr: 44, n: 97,  tok: "42.0k", t: "2h ago",   added: 3 },
  { id: "01L5A2",  strat: "eth-mr-v3",          scen: "flash-crash-24-08",mode: "Backtest", status: "Completed",   color: "gold",   sharpe: -0.92, ret: -28.7,  dd: -31.4, wr: 38, n: 44,  tok: "53.5k", t: "3h ago",   added: 4 },
  { id: "01M7B1",  strat: "eth-mr-v3",          scen: "bull-q1-25",       mode: "Paper",    status: "Running 42%", color: "warn",   sharpe: null,  ret: 1.2,    dd: -0.4,  wr: null,n:12,  tok: "21.4k", t: "live",     added: 0, running: true },
  { id: "01N3Q8",  strat: "stablecoin-flow-v1", scen: "carry-90d",        mode: "Backtest", status: "Completed",   color: "gold",   sharpe: 0.55,  ret: 5.1,    dd: -1.2,  wr: 78, n: 412, tok: "31.0k", t: "5h ago",   added: 5 },
  { id: "01P2K4",  strat: "btc-momentum-v1",    scen: "bull-q1-25",       mode: "Backtest", status: "Completed",   color: "gold",   sharpe: 0.92,  ret: 12.3,   dd: -5.4,  wr: 55, n: 143, tok: "42.0k", t: "7h ago",   added: 6 },
  { id: "01Q1A7",  strat: "sol-trend-follow-v1",scen: "bear-q3-24",       mode: "Backtest", status: "Failed",      color: "danger", sharpe: null,  ret: null,   dd: null,  wr: null,n: null,tok:"0",     t: "8h ago",   added: 7 },
  { id: "01R7H3",  strat: "eth-mr-v2",          scen: "chop-q2-25",       mode: "Backtest", status: "Completed",   color: "gold",   sharpe: 0.81,  ret: 6.8,    dd: -3.7,  wr: 52, n: 198, tok: "48.2k", t: "12h ago",  added: 8 },
  { id: "01S5T9",  strat: "eth-mr-v3",          scen: "bear-q3-24",       mode: "Backtest", status: "Queued",      color: "muted",  sharpe: null,  ret: null,   dd: null,  wr: null,n: null,tok:"—",    t: "—",        added: 9 },
];

// ── Strategies — full ListCard ──────────────────────────────────────────────
const StrategiesList = () => {
  const list = useListState({
    rows: STRAT_ROWS,
    filters: [
      { id: "status", label: "Status",
        options: [
          { value: "all", label: "All status" },
          { value: "Validated", label: "Validated" },
          { value: "Draft", label: "Draft" },
          { value: "Warnings", label: "Warnings" },
          { value: "Archived", label: "Archived" },
        ]},
      { id: "tpl", label: "Template",
        options: [
          { value: "all", label: "All templates" },
          { value: "mean_reversion", label: "mean_reversion" },
          { value: "trend_follower", label: "trend_follower" },
          { value: "stat_arb", label: "stat_arb" },
          { value: "carry", label: "carry" },
        ]},
    ],
    sortOptions: [
      { value: "added",   label: "Recently added" },
      { value: "added-asc", label: "Oldest first" },
      { value: "updated", label: "Recently updated" },
      { value: "name",    label: "Name A → Z" },
      { value: "tokens",  label: "Tokens (high → low)" },
    ],
    filterFn: (r, q, f) => {
      const qq = q.trim().toLowerCase();
      if (qq && !r.name.toLowerCase().includes(qq) && !r.tpl.toLowerCase().includes(qq)) return false;
      if (f.status !== "all" && r.status !== f.status) return false;
      if (f.tpl !== "all" && r.tpl !== f.tpl) return false;
      return true;
    },
    sortFn: (rows, key) => {
      if (key === "added")     return rows.sort((a,b) => a.addedRank - b.addedRank);
      if (key === "added-asc") return rows.sort((a,b) => b.addedRank - a.addedRank);
      if (key === "updated")   return rows.sort((a,b) => a.updatedRank - b.updatedRank);
      if (key === "name")      return rows.sort((a,b) => a.name.localeCompare(b.name));
      if (key === "tokens")    return rows.sort((a,b) => b.tokensN - a.tokensN);
      return rows;
    },
  });

  return (
    <ListCard
      title="Strategies"
      count={STRAT_ROWS.length}
      toolbar={{
        ...list,
        actions: (
          <>
            <button className="btn ghost"><Icon name="plus" size={13}/> New from template</button>
            <button className="btn primary"><Icon name="plus" size={13} color="#001A0A"/> New strategy</button>
          </>
        ),
      }}
      columns={[
        { key: "chk",     label: "", width: 28 },
        { key: "name",    label: "Name" },
        { key: "tpl",     label: "Template" },
        { key: "forked",  label: "Forked from" },
        { key: "status",  label: "Status" },
        { key: "lastEval",label: "Last eval" },
        { key: "tokens",  label: "Tokens / run", align: "right" },
        { key: "updated", label: "Updated" },
        { key: "menu",    label: "", width: 40 },
      ]}
      rows={list.rows}
      renderRow={(r, i) => (
        <tr key={i} className="hover">
          <td><input type="checkbox"/></td>
          <td className="mono" style={{color: "var(--text)"}}>{r.name}</td>
          <td className="mute">{r.tpl}</td>
          <td className="mute mono">{r.forked}</td>
          <td><span className={`dot ${r.color}`}/>{r.status}</td>
          <td className="mono mute">{r.lastEval}</td>
          <td className="mono" style={{textAlign: "right"}}>{r.tokens}</td>
          <td className="mute">{r.updated}</td>
          <td style={{color: "var(--text-3)"}}>⋯</td>
        </tr>
      )}
    />
  );
};

// ── Eval runs — full ListCard ──────────────────────────────────────────────
const EvalRunsList = () => {
  const list = useListState({
    rows: RUN_ROWS,
    filters: [
      { id: "strategy", label: "Strategy",
        options: [
          { value: "all", label: "All strategies" },
          ...Array.from(new Set(RUN_ROWS.map(r => r.strat))).map(s => ({ value: s, label: s })),
        ]},
      { id: "mode", label: "Mode",
        options: [
          { value: "all", label: "All" }, { value: "Backtest", label: "Backtest" }, { value: "Paper", label: "Paper" },
        ]},
      { id: "status", label: "Status",
        options: [
          { value: "all", label: "All" },
          { value: "Completed", label: "Completed" },
          { value: "Running", label: "Running" },
          { value: "Queued", label: "Queued" },
          { value: "Failed", label: "Failed" },
        ]},
    ],
    sortOptions: [
      { value: "added",  label: "Recently added" },
      { value: "added-asc", label: "Oldest first" },
      { value: "sharpe", label: "Sharpe (high → low)" },
      { value: "ret",    label: "Return (high → low)" },
      { value: "dd",     label: "Max DD (low → high)" },
    ],
    filterFn: (r, q, f) => {
      const qq = q.trim().toLowerCase();
      if (qq && !r.id.toLowerCase().includes(qq) && !r.strat.toLowerCase().includes(qq) && !r.scen.toLowerCase().includes(qq)) return false;
      if (f.strategy !== "all" && r.strat !== f.strategy) return false;
      if (f.mode !== "all" && r.mode !== f.mode) return false;
      if (f.status !== "all") {
        if (f.status === "Running" && !r.running) return false;
        if (f.status !== "Running" && r.status !== f.status) return false;
      }
      return true;
    },
    sortFn: (rows, key) => {
      const num = (v) => v == null ? -Infinity : v;
      if (key === "added")     return rows.sort((a,b) => a.added - b.added);
      if (key === "added-asc") return rows.sort((a,b) => b.added - a.added);
      if (key === "sharpe")    return rows.sort((a,b) => num(b.sharpe) - num(a.sharpe));
      if (key === "ret")       return rows.sort((a,b) => num(b.ret) - num(a.ret));
      if (key === "dd")        return rows.sort((a,b) => num(a.dd) - num(b.dd));
      return rows;
    },
  });

  return (
    <ListCard
      title="Eval runs"
      count={RUN_ROWS.length}
      subtitle="32 completed · 15 in progress"
      toolbar={{
        ...list,
        search: { ...list.search, placeholder: "Search run ID, strategy, scenario…" },
        actions: (
          <>
            <button className="btn ghost">Compare (0)</button>
            <button className="btn primary"><Icon name="plus" size={13} color="#001A0A"/> New run</button>
          </>
        ),
      }}
      columns={[
        { key: "chk", label: "", width: 28 },
        { key: "id", label: "Run ID" },
        { key: "strat", label: "Strategy" },
        { key: "scen", label: "Scenario" },
        { key: "mode", label: "Mode" },
        { key: "status", label: "Status" },
        { key: "sharpe", label: "Sharpe", align: "right" },
        { key: "ret", label: "Return", align: "right" },
        { key: "dd", label: "Max DD", align: "right" },
        { key: "wr", label: "Win rate", align: "right" },
        { key: "n", label: "Trades", align: "right" },
        { key: "t", label: "Started" },
      ]}
      rows={list.rows}
      renderRow={(r) => {
        const retClass = r.ret == null ? "mute" : r.ret >= 0 ? "up" : "down";
        const fmt = (v, suffix = "", prefix = "") =>
          v == null ? "—" : `${prefix}${v > 0 && suffix === "%" ? "+" : ""}${v}${suffix}`;
        return (
          <tr key={r.id} className="hover">
            <td><input type="checkbox"/></td>
            <td className="mono" style={{color: "var(--text)"}}>{r.id}</td>
            <td className="mono">{r.strat}</td>
            <td className="mono mute">{r.scen}</td>
            <td>{r.mode}</td>
            <td><span className={`dot ${r.color}`}/>{r.status}</td>
            <td className="mono" style={{textAlign: "right"}}>{r.sharpe ?? "—"}</td>
            <td className={"mono " + retClass} style={{textAlign: "right"}}>{fmt(r.ret, "%")}</td>
            <td className="mono down" style={{textAlign: "right"}}>{r.dd == null ? "—" : r.dd + "%"}</td>
            <td className="mono" style={{textAlign: "right"}}>{r.wr == null ? "—" : r.wr + "%"}</td>
            <td className="mono" style={{textAlign: "right"}}>{r.n ?? "—"}</td>
            <td className="mute">{r.t}</td>
          </tr>
        );
      }}
    />
  );
};

// ── Recent runs — compact (mini-list, fits inside Home dashboard) ──────────
const RecentRunsList = () => {
  const list = useListState({
    rows: RUN_ROWS.slice(0, 6),
    filters: [
      { id: "mode", label: "Mode",
        options: [{value:"all", label:"All"}, {value:"Backtest", label:"Backtest"}, {value:"Paper", label:"Paper"}] },
    ],
    sortOptions: [
      { value: "added",  label: "Recent" },
      { value: "sharpe", label: "Sharpe" },
      { value: "ret",    label: "Return" },
    ],
    filterFn: (r, q, f) => {
      const qq = q.trim().toLowerCase();
      if (qq && !r.id.toLowerCase().includes(qq) && !r.strat.toLowerCase().includes(qq)) return false;
      if (f.mode !== "all" && r.mode !== f.mode) return false;
      return true;
    },
    sortFn: (rows, key) => {
      const num = (v) => v == null ? -Infinity : v;
      if (key === "added")  return rows.sort((a,b) => a.added - b.added);
      if (key === "sharpe") return rows.sort((a,b) => num(b.sharpe) - num(a.sharpe));
      if (key === "ret")    return rows.sort((a,b) => num(b.ret) - num(a.ret));
      return rows;
    },
  });
  return (
    <ListCard
      title="Recent runs"
      density="compact"
      toolbar={{
        ...list,
        search: { ...list.search, placeholder: "Search…" },
      }}
      columns={[
        { key: "id", label: "Run ID" },
        { key: "strat", label: "Strategy" },
        { key: "mode", label: "Mode" },
        { key: "status", label: "Status" },
        { key: "sharpe", label: "Sharpe", align: "right" },
        { key: "ret", label: "Return", align: "right" },
      ]}
      rows={list.rows}
      renderRow={(r) => {
        const retClass = r.ret == null ? "mute" : r.ret >= 0 ? "up" : "down";
        return (
          <tr key={r.id} className="hover">
            <td className="mono">{r.id}</td>
            <td className="mono">{r.strat}</td>
            <td>{r.mode}</td>
            <td><span className={`dot ${r.color}`}/>{r.status}</td>
            <td className="mono" style={{textAlign: "right"}}>{r.sharpe ?? "—"}</td>
            <td className={"mono " + retClass} style={{textAlign: "right"}}>
              {r.ret == null ? "—" : (r.ret >= 0 ? "+" : "") + r.ret + "%"}
            </td>
          </tr>
        );
      }}
      footer={
        <>
          <span>{list.rows.length} of {RUN_ROWS.length} runs</span>
          <a style={{color: "var(--gold)", textDecoration: "none"}}>View all runs →</a>
        </>
      }
    />
  );
};

// ── Decisions — compact, demonstrating filter chips replacing select ───────
const DecisionsList = () => {
  const decisions = [
    ["01-08 14:22:00", "BUY",   "ETH/USD", "2,851.50", "0.05", 0.78, "RSI 27 + bb_lower touch; ADX 24"],
    ["01-08 15:00:00", "HOLD",  "ETH/USD", "2,863.10", "—",    0.42, "RSI rising, no exit signal"],
    ["01-08 18:00:00", "SELL",  "ETH/USD", "2,902.30", "0.05", 0.71, "Target reached"],
    ["01-09 02:00:00", "HOLD",  "ETH/USD", "2,895.40", "—",    0.18, "Regime: chop"],
    ["01-12 09:15:00", "BUY",   "ETH/USD", "2,810.20", "0.05", 0.82, "RSI 24 + volume confirmation"],
    ["01-12 19:45:00", "CLOSE", "ETH/USD", "2,875.80", "0.05", 0.55, "Trailing stop hit at +2.33%"],
    ["01-15 03:48:00", "BUY",   "ETH/USD", "2,902.30", "0.05", 0.51, "Weak signal — RSI 31"],
    ["01-15 09:10:00", "CLOSE", "ETH/USD", "2,887.10", "0.05", 0.39, "Stop-loss triggered (−0.52%)"],
    ["01-19 11:02:00", "BUY",   "ETH/USD", "2,755.80", "0.05", 0.88, "Strong reversal setup"],
    ["01-20 02:15:00", "SELL",  "ETH/USD", "2,838.40", "0.05", 0.81, "Take-profit T2 hit"],
    ["01-22 16:30:00", "BUY",   "ETH/USD", "2,820.60", "0.05", 0.55, "Borderline; RSI 29.4"],
    ["01-26 08:04:00", "BUY",   "ETH/USD", "2,710.30", "0.05", 0.84, "Sharp dip + funding negative"],
  ].map((d, i) => ({ time: d[0], action: d[1], sym: d[2], price: d[3], qty: d[4], conv: d[5], reason: d[6], added: i }));

  const list = useListState({
    rows: decisions,
    filters: [
      { id: "action", label: "Action",
        options: [
          { value: "all", label: "All" }, { value: "BUY", label: "Buy" },
          { value: "SELL", label: "Sell" }, { value: "HOLD", label: "Hold" }, { value: "CLOSE", label: "Close" },
        ]},
      { id: "conv", label: "Conviction",
        options: [
          { value: "all", label: "Any" },
          { value: "high", label: "≥ 0.70" },
          { value: "mid", label: "0.40 – 0.69" },
          { value: "low", label: "< 0.40" },
        ]},
    ],
    sortOptions: [
      { value: "added",     label: "Recent" },
      { value: "added-asc", label: "Oldest" },
      { value: "conv",      label: "Conviction" },
    ],
    filterFn: (r, q, f) => {
      const qq = q.trim().toLowerCase();
      if (qq && !r.reason.toLowerCase().includes(qq) && !r.sym.toLowerCase().includes(qq)) return false;
      if (f.action !== "all" && r.action !== f.action) return false;
      if (f.conv === "high" && r.conv < 0.7) return false;
      if (f.conv === "mid"  && (r.conv < 0.4 || r.conv >= 0.7)) return false;
      if (f.conv === "low"  && r.conv >= 0.4) return false;
      return true;
    },
    sortFn: (rows, key) => {
      if (key === "added")     return rows.sort((a,b) => b.added - a.added);
      if (key === "added-asc") return rows.sort((a,b) => a.added - b.added);
      if (key === "conv")      return rows.sort((a,b) => b.conv - a.conv);
      return rows;
    },
  });

  const actionMeta = {
    BUY:   { color: "var(--gold)",   bg: "rgba(0, 230, 118, 0.12)", border: "rgba(0, 230, 118, 0.4)" },
    SELL:  { color: "var(--info)",   bg: "rgba(111,143,184,0.12)", border: "rgba(111,143,184,0.45)" },
    HOLD:  { color: "var(--text-2)", bg: "rgba(163,154,133,0.08)", border: "var(--border)" },
    CLOSE: { color: "var(--danger)", bg: "rgba(255, 77, 77, 0.10)",  border: "rgba(255, 77, 77, 0.4)" },
  };

  return (
    <ListCard
      title="Decisions"
      count={decisions.length}
      density="compact"
      toolbar={{
        ...list,
        search: { ...list.search, placeholder: "Search reasoning…" },
        actions: <button className="btn ghost" style={{padding: "4px 10px", fontSize: 12}}>Export</button>,
      }}
      columns={[
        { key: "time", label: "Time" },
        { key: "action", label: "Action" },
        { key: "sym", label: "Symbol" },
        { key: "price", label: "Price", align: "right" },
        { key: "qty", label: "Qty", align: "right" },
        { key: "conv", label: "Conv.", align: "right" },
        { key: "reason", label: "Reasoning" },
      ]}
      rows={list.rows}
      renderRow={(r, i) => {
        const m = actionMeta[r.action];
        return (
          <tr key={i}>
            <td className="mono mute" style={{whiteSpace: "nowrap"}}>{r.time}</td>
            <td>
              <span style={{
                display: "inline-block", padding: "2px 8px", borderRadius: 3,
                border: `1px solid ${m.border}`, background: m.bg, color: m.color,
                fontFamily: "JetBrains Mono, monospace", fontSize: 11, fontWeight: 500,
                letterSpacing: "0.04em", minWidth: 48, textAlign: "center",
              }}>{r.action}</span>
            </td>
            <td className="mono">{r.sym}</td>
            <td className="mono" style={{textAlign: "right"}}>{r.price}</td>
            <td className="mono mute" style={{textAlign: "right"}}>{r.qty}</td>
            <td className="mono" style={{textAlign: "right", color: r.conv >= 0.7 ? "var(--gold)" : r.conv >= 0.4 ? "var(--text)" : "var(--text-3)"}}>
              {r.conv.toFixed(2)}
            </td>
            <td className="mute" style={{fontSize: 12, maxWidth: 260, whiteSpace: "nowrap", overflow: "hidden", textOverflow: "ellipsis"}}>
              {r.reason}
            </td>
          </tr>
        );
      }}
    />
  );
};

// ── Open positions — minimal (search only, no filters) ─────────────────────
const PositionsList = () => {
  const rows = [
    { sym: "ETH/USD", side: "Long", sz: "0.05", mk: "2,851.50", pnl: 0.22, added: 1 },
    { sym: "BTC/USD", side: "Long", sz: "0.01", mk: "67,421.10", pnl: 0.11, added: 2 },
    { sym: "SOL/USD", side: "Long", sz: "1.20", mk: "152.31",    pnl: -0.08, added: 3 },
    { sym: "DOGE/USD",side: "Long", sz: "200",  mk: "0.0823",    pnl: 0.04, added: 4 },
  ];
  const list = useListState({
    rows,
    sortOptions: [
      { value: "added", label: "Recent" },
      { value: "pnl",   label: "PnL (high → low)" },
      { value: "sym",   label: "Symbol" },
    ],
    filterFn: (r, q) => {
      const qq = q.trim().toLowerCase();
      return !qq || r.sym.toLowerCase().includes(qq);
    },
    sortFn: (rows, k) =>
      k === "pnl" ? rows.sort((a,b)=>b.pnl-a.pnl)
      : k === "sym" ? rows.sort((a,b)=>a.sym.localeCompare(b.sym))
      : rows.sort((a,b)=>a.added-b.added),
  });
  return (
    <ListCard
      title="Open positions"
      density="compact"
      toolbar={{ ...list, search: { ...list.search, placeholder: "Symbol…" } }}
      columns={[
        { key: "sym", label: "Symbol" }, { key: "side", label: "Side" },
        { key: "sz", label: "Size", align: "right" },
        { key: "mk", label: "Mark", align: "right" },
        { key: "pnl", label: "PnL", align: "right" },
      ]}
      rows={list.rows}
      renderRow={(r) => (
        <tr key={r.sym}>
          <td className="mono">{r.sym}</td>
          <td className="up">{r.side}</td>
          <td className="mono" style={{textAlign: "right"}}>{r.sz}</td>
          <td className="mono" style={{textAlign: "right"}}>{r.mk}</td>
          <td className={"mono " + (r.pnl >= 0 ? "up" : "down")} style={{textAlign: "right"}}>
            {(r.pnl >= 0 ? "+" : "") + r.pnl.toFixed(2) + "%"}
          </td>
        </tr>
      )}
    />
  );
};

window.StrategiesList = StrategiesList;
window.EvalRunsList = EvalRunsList;
window.RecentRunsList = RecentRunsList;
window.DecisionsList = DecisionsList;
window.PositionsList = PositionsList;
