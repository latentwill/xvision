// xvn — Mobile list applications
// Wires the standard component to every mobile list surface.

const MOBILE_LIST_CHROME = ({ title, children }) => (
  <div style={{
    position: "absolute", inset: 0, paddingTop: 46,
    background: "var(--bg)", display: "flex", flexDirection: "column",
  }}>
    {/* App bar */}
    <div style={{
      height: 44, display: "flex", alignItems: "center", gap: 4, padding: "0 8px",
      borderBottom: "1px solid var(--border-soft)", background: "var(--bg)", flexShrink: 0,
    }}>
      <button style={{
        width: 36, height: 36, borderRadius: 999, display: "flex", alignItems: "center", justifyContent: "center",
        background: "transparent", border: "none", color: "var(--text-2)",
      }}>
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
          <path d="M3 7h18M3 12h18M3 17h12" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
        </svg>
      </button>
      <div style={{flex: 1, textAlign: "center", fontFamily: "'Geist', sans-serif", fontStyle: "normal", fontSize: 21, fontWeight: 500, color: "var(--text)"}}>
        {title}
      </div>
      <button style={{
        width: 36, height: 36, borderRadius: 999, display: "flex", alignItems: "center", justifyContent: "center",
        background: "transparent", border: "none", color: "var(--text-2)",
      }}>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
          <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round"/>
        </svg>
      </button>
    </div>
    <div style={{flex: 1, minHeight: 0, display: "flex", flexDirection: "column"}}>
      {children}
    </div>
  </div>
);

// ── Strategies (mobile) ─────────────────────────────────────────────────────
const MobileStrategiesList = () => {
  const list = useListState({
    rows: STRAT_ROWS,
    filters: [
      { id: "status", label: "Status",
        options: [
          { value: "all", label: "All" },
          { value: "Validated", label: "Validated" },
          { value: "Draft", label: "Draft" },
          { value: "Warnings", label: "Warnings" },
          { value: "Archived", label: "Archived" },
        ]},
      { id: "tpl", label: "Template",
        options: [
          { value: "all", label: "All" },
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
      { value: "tokens",  label: "Tokens" },
    ],
    filterFn: (r, q, f) => {
      const qq = q.trim().toLowerCase();
      if (qq && !r.name.toLowerCase().includes(qq) && !r.tpl.toLowerCase().includes(qq)) return false;
      if (f.status !== "all" && r.status !== f.status) return false;
      if (f.tpl !== "all" && r.tpl !== f.tpl) return false;
      return true;
    },
    sortFn: (rows, k) =>
      k === "added"     ? rows.sort((a,b) => a.addedRank - b.addedRank) :
      k === "added-asc" ? rows.sort((a,b) => b.addedRank - a.addedRank) :
      k === "updated"   ? rows.sort((a,b) => a.updatedRank - b.updatedRank) :
      k === "name"      ? rows.sort((a,b) => a.name.localeCompare(b.name)) :
      k === "tokens"    ? rows.sort((a,b) => b.tokensN - a.tokensN) :
      rows,
  });

  return (
    <MOBILE_LIST_CHROME title="Strategies">
      <MListCard
        title="Strategies"
        count={STRAT_ROWS.length}
        toolbar={{
          ...list,
          search: { ...list.search, placeholder: "Search strategies, templates…" },
        }}
        rows={list.rows}
        renderRow={(r) => (
          <MListRow
            key={r.name}
            title={r.name}
            badge={r.status}
            badgeColor={r.color}
            subtitle={r.tpl + (r.forked !== "—" ? " · forked " + r.forked : "")}
            meta={`${r.tokens} tok · ${r.updated}`}
            rightTop={r.lastEval !== "—" ? r.lastEval.split(" · ")[0] : "—"}
            rightSub={r.lastEval !== "—" ? "Sharpe" : "no evals"}
            rightColor={r.lastEval.startsWith("−") ? "down" : r.lastEval !== "—" ? "up" : "mute"}
          />
        )}
        empty="No strategies match. Adjust filters or clear search."
      />
    </MOBILE_LIST_CHROME>
  );
};

// ── Eval runs (mobile) ─────────────────────────────────────────────────────
const MobileEvalRunsList = () => {
  const list = useListState({
    rows: RUN_ROWS,
    filters: [
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
      { id: "strategy", label: "Strategy",
        options: [
          { value: "all", label: "All" },
          ...Array.from(new Set(RUN_ROWS.map(r => r.strat))).map(s => ({ value: s, label: s })),
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
      if (f.mode !== "all" && r.mode !== f.mode) return false;
      if (f.status === "Running" && !r.running) return false;
      if (f.status !== "all" && f.status !== "Running" && r.status !== f.status) return false;
      if (f.strategy !== "all" && r.strat !== f.strategy) return false;
      return true;
    },
    sortFn: (rows, k) => {
      const n = (v) => v == null ? -Infinity : v;
      return k === "added"     ? rows.sort((a,b) => a.added - b.added) :
             k === "added-asc" ? rows.sort((a,b) => b.added - a.added) :
             k === "sharpe"    ? rows.sort((a,b) => n(b.sharpe) - n(a.sharpe)) :
             k === "ret"       ? rows.sort((a,b) => n(b.ret) - n(a.ret)) :
             k === "dd"        ? rows.sort((a,b) => n(a.dd) - n(b.dd)) :
             rows;
    },
  });

  return (
    <MOBILE_LIST_CHROME title="Eval runs">
      <MListCard
        title="Eval runs"
        count={RUN_ROWS.length}
        toolbar={{
          ...list,
          search: { ...list.search, placeholder: "Run ID, strategy, scenario…" },
        }}
        rows={list.rows}
        renderRow={(r) => {
          const retC = r.ret == null ? "mute" : r.ret >= 0 ? "up" : "down";
          const fmt = (v, suf = "%") => v == null ? "—" : (v > 0 ? "+" : "") + v + suf;
          return (
            <MListRow
              key={r.id}
              title={r.id}
              badge={r.status === "Completed" ? r.mode : r.status}
              badgeColor={r.status === "Completed" ? "muted" : r.color}
              subtitle={r.strat + " · " + r.scen}
              meta={r.sharpe != null ? `Sharpe ${r.sharpe} · ${r.n ?? "—"} trades · ${r.t}` : `${r.t}`}
              rightTop={fmt(r.ret)}
              rightSub={r.dd != null ? "DD " + r.dd + "%" : ""}
              rightColor={retC}
            />
          );
        }}
        empty="No runs match. Try clearing a filter."
      />
    </MOBILE_LIST_CHROME>
  );
};

// ── Decisions (mobile, inside Run detail) ──────────────────────────────────
const MobileDecisionsList = () => {
  const decisions = [
    { time: "14:22", action: "BUY",   sym: "ETH/USD", price: "2,851.50", qty: "0.05", conv: 0.78, reason: "RSI 27 + bb_lower; ADX 24", added: 0 },
    { time: "15:00", action: "HOLD",  sym: "ETH/USD", price: "2,863.10", qty: "—",    conv: 0.42, reason: "RSI rising, no exit signal", added: 1 },
    { time: "18:00", action: "SELL",  sym: "ETH/USD", price: "2,902.30", qty: "0.05", conv: 0.71, reason: "Target reached", added: 2 },
    { time: "02:00", action: "HOLD",  sym: "ETH/USD", price: "2,895.40", qty: "—",    conv: 0.18, reason: "Regime: chop", added: 3 },
    { time: "09:15", action: "BUY",   sym: "ETH/USD", price: "2,810.20", qty: "0.05", conv: 0.82, reason: "RSI 24 + volume confirmation", added: 4 },
    { time: "19:45", action: "CLOSE", sym: "ETH/USD", price: "2,875.80", qty: "0.05", conv: 0.55, reason: "Trailing stop hit at +2.33%", added: 5 },
    { time: "03:48", action: "BUY",   sym: "ETH/USD", price: "2,902.30", qty: "0.05", conv: 0.51, reason: "Weak signal — RSI 31", added: 6 },
    { time: "09:10", action: "CLOSE", sym: "ETH/USD", price: "2,887.10", qty: "0.05", conv: 0.39, reason: "Stop-loss triggered", added: 7 },
    { time: "11:02", action: "BUY",   sym: "ETH/USD", price: "2,755.80", qty: "0.05", conv: 0.88, reason: "Strong reversal setup", added: 8 },
    { time: "02:15", action: "SELL",  sym: "ETH/USD", price: "2,838.40", qty: "0.05", conv: 0.81, reason: "Take-profit T2 hit", added: 9 },
  ];

  const list = useListState({
    rows: decisions,
    filters: [
      { id: "action", label: "Action",
        options: [
          { value: "all", label: "All" }, { value: "BUY", label: "Buy" }, { value: "SELL", label: "Sell" },
          { value: "HOLD", label: "Hold" }, { value: "CLOSE", label: "Close" },
        ]},
      { id: "conv", label: "Conviction",
        options: [
          { value: "all", label: "Any" }, { value: "high", label: "≥ 0.70" },
          { value: "mid", label: "0.40 – 0.69" }, { value: "low", label: "< 0.40" },
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
    sortFn: (rows, k) =>
      k === "added"     ? rows.sort((a,b) => b.added - a.added) :
      k === "added-asc" ? rows.sort((a,b) => a.added - b.added) :
      k === "conv"      ? rows.sort((a,b) => b.conv - a.conv) :
      rows,
  });

  const actionMap = {
    BUY: "gold", SELL: "info", HOLD: "muted", CLOSE: "danger",
  };

  return (
    <MOBILE_LIST_CHROME title="Decisions">
      <MListCard
        title="Decisions"
        count={decisions.length}
        subtitle="Run 01H8N7Z"
        toolbar={{
          ...list,
          search: { ...list.search, placeholder: "Search reasoning…" },
        }}
        rows={list.rows}
        renderRow={(r, i) => (
          <MListRow
            key={i}
            title={r.action}
            badge={r.sym}
            badgeColor="muted"
            subtitle={r.reason}
            meta={`${r.time} · ${r.price} · qty ${r.qty}`}
            rightTop={r.conv.toFixed(2)}
            rightSub="conviction"
            rightColor={r.conv >= 0.7 ? "up" : r.conv >= 0.4 ? "" : "mute"}
          />
        )}
        empty="No decisions match."
      />
    </MOBILE_LIST_CHROME>
  );
};

// ── State demos for the canvas (specific UI states) ────────────────────────
// These render the same screens but inject specific filter/sheet/search state.

// Eval runs with filters applied + chips visible
const MobileEvalRunsFiltered = () => {
  const ctlRef = React.useRef(null);
  return (
    <MOBILE_LIST_CHROME title="Eval runs">
      <FilteredEvalRunsInner />
    </MOBILE_LIST_CHROME>
  );
};

const FilteredEvalRunsInner = () => {
  const list = useListState({
    rows: RUN_ROWS,
    filters: [
      { id: "mode", label: "Mode", defaultValue: "Backtest",
        options: [{value:"all", label:"All"},{value:"Backtest", label:"Backtest"},{value:"Paper", label:"Paper"}]},
      { id: "status", label: "Status",
        options: [{value:"all", label:"All"},{value:"Completed", label:"Completed"},{value:"Running", label:"Running"},{value:"Failed", label:"Failed"}]},
      { id: "strategy", label: "Strategy", defaultValue: "eth-mr-v3",
        options: [
          { value: "all", label: "All" },
          ...Array.from(new Set(RUN_ROWS.map(r => r.strat))).map(s => ({ value: s, label: s })),
        ]},
    ],
    initialSort: "ret",
    sortOptions: [
      { value: "added",  label: "Recently added" },
      { value: "added-asc", label: "Oldest first" },
      { value: "sharpe", label: "Sharpe (high → low)" },
      { value: "ret",    label: "Return (high → low)" },
    ],
    filterFn: (r, q, f) => {
      const qq = q.trim().toLowerCase();
      if (qq && !r.id.toLowerCase().includes(qq) && !r.strat.toLowerCase().includes(qq) && !r.scen.toLowerCase().includes(qq)) return false;
      if (f.mode !== "all" && r.mode !== f.mode) return false;
      if (f.strategy !== "all" && r.strat !== f.strategy) return false;
      if (f.status === "Running" && !r.running) return false;
      if (f.status !== "all" && f.status !== "Running" && r.status !== f.status) return false;
      return true;
    },
    sortFn: (rows, k) => {
      const n = v => v == null ? -Infinity : v;
      return k === "ret" ? rows.sort((a,b) => n(b.ret) - n(a.ret))
        : k === "sharpe" ? rows.sort((a,b) => n(b.sharpe) - n(a.sharpe))
        : k === "added-asc" ? rows.sort((a,b) => b.added - a.added)
        : rows.sort((a,b) => a.added - b.added);
    },
  });

  return (
    <MListCard
      title="Eval runs"
      count={list.rows.length}
      toolbar={{
        ...list,
        search: { value: "bull", onChange: list.search.onChange, placeholder: "Run ID, strategy…" },
      }}
      rows={list.rows.filter(r => r.scen.includes("bull"))}
      renderRow={(r) => {
        const retC = r.ret == null ? "mute" : r.ret >= 0 ? "up" : "down";
        const fmt = (v, suf = "%") => v == null ? "—" : (v > 0 ? "+" : "") + v + suf;
        return (
          <MListRow
            key={r.id}
            title={r.id}
            badge={r.status === "Completed" ? r.mode : r.status}
            badgeColor={r.status === "Completed" ? "muted" : r.color}
            subtitle={r.strat + " · " + r.scen}
            meta={r.sharpe != null ? `Sharpe ${r.sharpe} · ${r.n ?? "—"} trades · ${r.t}` : r.t}
            rightTop={fmt(r.ret)}
            rightSub={r.dd != null ? "DD " + r.dd + "%" : ""}
            rightColor={retC}
          />
        );
      }}
    />
  );
};

// Sheet open demo
const MobileSheetOpen = () => {
  const list = useListState({
    rows: STRAT_ROWS,
    filters: [
      { id: "status", label: "Status",
        options: [
          { value: "all", label: "All" }, { value: "Validated", label: "Validated" },
          { value: "Draft", label: "Draft" }, { value: "Warnings", label: "Warnings" }, { value: "Archived", label: "Archived" },
        ], defaultValue: "Validated" },
      { id: "tpl", label: "Template",
        options: [
          { value: "all", label: "All" }, { value: "mean_reversion", label: "mean_reversion" },
          { value: "trend_follower", label: "trend_follower" }, { value: "stat_arb", label: "stat_arb" }, { value: "carry", label: "carry" },
        ]},
    ],
    sortOptions: [
      { value: "added",     label: "Recently added" },
      { value: "added-asc", label: "Oldest first" },
      { value: "updated",   label: "Recently updated" },
      { value: "name",      label: "Name A → Z" },
      { value: "tokens",    label: "Tokens" },
    ],
    initialSort: "added",
    filterFn: (r, _, f) => f.status === "all" || r.status === f.status,
    sortFn: (rows) => rows,
  });

  // Force sheet open
  const [sheet, setSheet] = React.useState("all");

  return (
    <MOBILE_LIST_CHROME title="Strategies">
      <MListCard
        title="Strategies"
        count={list.rows.length}
        toolbar={{
          ...list,
          search: { ...list.search, placeholder: "Search…" },
        }}
        rows={list.rows}
        renderRow={(r) => (
          <MListRow
            key={r.name}
            title={r.name}
            badge={r.status}
            badgeColor={r.color}
            subtitle={r.tpl}
            meta={`${r.tokens} tok · ${r.updated}`}
            rightTop={r.lastEval !== "—" ? r.lastEval.split(" · ")[0] : "—"}
            rightSub="Sharpe"
            rightColor={r.lastEval.startsWith("−") ? "down" : "up"}
          />
        )}
      />
      {/* Manual sheet overlay because the toolbar uses internal state */}
      <MListSheet
        open={true}
        mode="all"
        onClose={() => setSheet(null)}
        search={list.search}
        filters={list.filters}
        sort={list.sort}
        onClear={() => {}}
        resultCount={list.rows.length}
      />
    </MOBILE_LIST_CHROME>
  );
};

window.MobileStrategiesList = MobileStrategiesList;
window.MobileEvalRunsList = MobileEvalRunsList;
window.MobileDecisionsList = MobileDecisionsList;
window.MobileEvalRunsFiltered = MobileEvalRunsFiltered;
window.MobileSheetOpen = MobileSheetOpen;
