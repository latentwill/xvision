// xvn — ListCard variants and states
const ListVariants = () => {
  const STRAT = [
    { name: "eth-mr-v3", tpl: "mean_reversion", status: "Validated", color: "gold", tok: "53.5k", updated: "14m ago" },
    { name: "btc-momentum-v1", tpl: "trend_follower", status: "Validated", color: "gold", tok: "42.0k", updated: "5h ago" },
    { name: "sol-trend-follow-v1", tpl: "trend_follower", status: "Draft", color: "muted", tok: "—", updated: "1d ago" },
    { name: "arb-revert-v1", tpl: "stat_arb", status: "Warnings", color: "warn", tok: "67.4k", updated: "1d ago" },
  ];

  const cols = [
    { key: "name", label: "Name" }, { key: "tpl", label: "Template" },
    { key: "status", label: "Status" }, { key: "updated", label: "Updated" },
  ];
  const renderRow = (r, i) => (
    <tr key={i} className="hover">
      <td className="mono">{r.name}</td>
      <td className="mute">{r.tpl}</td>
      <td><span className={`dot ${r.color}`}/>{r.status}</td>
      <td className="mute">{r.updated}</td>
    </tr>
  );

  const baseFilters = [
    { id: "status", label: "Status", options: [
      { value: "all", label: "All" }, { value: "Validated", label: "Validated" }, { value: "Draft", label: "Draft" }
    ]},
  ];

  return (
    <div style={{
      background: "var(--bg)", padding: "44px 60px 60px",
      width: 1440, minHeight: 900, color: "var(--text)",
    }}>
      <div style={{marginBottom: 32}}>
        <div style={{fontSize: 11, color: "var(--text-3)", letterSpacing: "0.12em", textTransform: "uppercase", marginBottom: 6}}>
          Component · variants &amp; states
        </div>
        <h1 className="serif" style={{fontSize: 36, margin: 0, letterSpacing: "-0.02em"}}>
          One component, three densities.
        </h1>
      </div>

      <div style={{display: "grid", gridTemplateColumns: "1fr 1fr", gap: 32}}>
        {/* — Full — */}
        <Variant
          tag="density='full'"
          title="Full — primary list pages"
          note="Default. Used on Strategies, Eval runs, Journal. Toolbar spans the card width with labelled selects."
        >
          <ListCard
            title="Strategies"
            count={4}
            toolbar={{
              search: { value: "", onChange: () => {}, placeholder: "Search…" },
              filters: baseFilters.map(f => ({...f, value: f.options[0].value, onChange: () => {}})),
              sort: { value: "added", onChange: () => {}, options: LIST_STD_DEFAULT_SORT },
            }}
            columns={cols}
            rows={STRAT}
            renderRow={renderRow}
          />
        </Variant>

        {/* — Compact — */}
        <Variant
          tag="density='compact'"
          title="Compact — dashboard cards"
          note="Used inside a two-up grid on Home, Run detail. Search collapses to an icon button until clicked; labels hide on selects to save width."
        >
          <ListCard
            title="Recent runs"
            density="compact"
            toolbar={{
              search: { value: "", onChange: () => {}, placeholder: "Search…" },
              filters: [{...baseFilters[0], value: "all", onChange: () => {}}],
              sort: { value: "added", onChange: () => {}, options: [
                {value: "added", label: "Recent"}, {value: "sharpe", label: "Sharpe"},
              ]},
            }}
            columns={cols}
            rows={STRAT.slice(0,3)}
            renderRow={renderRow}
            footer={<><span>3 of 47 runs</span><a style={{color: "var(--gold)", textDecoration: "none"}}>View all →</a></>}
          />
        </Variant>

        {/* — Active filter chips — */}
        <Variant
          tag="state · active"
          title="With active filters"
          note="Non-default values surface as removable chips under the toolbar. The select itself also turns gold. 'Clear all' resets everything."
        >
          <ListCard
            title="Strategies"
            count={1}
            toolbar={{
              search: { value: "momentum", onChange: () => {}, placeholder: "Search…" },
              filters: [
                { id: "status", label: "Status", value: "Validated", onChange: () => {}, options: [
                  { value: "all", label: "All" }, { value: "Validated", label: "Validated" },
                ]},
                { id: "tpl", label: "Template", value: "trend_follower", onChange: () => {}, options: [
                  { value: "all", label: "All" }, { value: "trend_follower", label: "trend_follower" },
                ]},
              ],
              sort: { value: "name", onChange: () => {}, options: LIST_STD_DEFAULT_SORT },
            }}
            columns={cols}
            rows={[STRAT[1]]}
            renderRow={renderRow}
          />
        </Variant>

        {/* — Empty state — */}
        <Variant
          tag="state · empty"
          title="No matches"
          note="Empty body message stays inside the table so column headers, filter chips, and toolbar all remain operable."
        >
          <ListCard
            title="Strategies"
            count={0}
            toolbar={{
              search: { value: "frobnicate", onChange: () => {}, placeholder: "Search…" },
              filters: [{...baseFilters[0], value: "Draft", onChange: () => {}}],
              sort: { value: "added", onChange: () => {}, options: LIST_STD_DEFAULT_SORT },
            }}
            columns={cols}
            rows={[]}
            renderRow={renderRow}
            empty="No strategies match. Try clearing 'search' or 'status'."
          />
        </Variant>
      </div>
    </div>
  );
};

const Variant = ({ tag, title, note, children }) => (
  <div>
    <div style={{marginBottom: 14}}>
      <div style={{display: "flex", alignItems: "baseline", gap: 10, marginBottom: 4}}>
        <span style={{
          fontFamily: "'Geist Mono', monospace", fontSize: 10.5,
          color: "var(--gold)", letterSpacing: "0.04em",
          padding: "2px 6px", border: "1px solid rgba(0, 230, 118, 0.3)",
          borderRadius: 2, background: "rgba(0, 230, 118, 0.06)",
        }}>{tag}</span>
        <span className="serif" style={{fontSize: 20, color: "var(--text)"}}>{title}</span>
      </div>
      <div style={{fontSize: 12.5, color: "var(--text-2)", lineHeight: 1.5, maxWidth: 540}}>{note}</div>
    </div>
    {children}
  </div>
);

window.ListVariants = ListVariants;
