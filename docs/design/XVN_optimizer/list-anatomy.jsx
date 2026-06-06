// xvn — ListCard component anatomy / spec
const ListAnatomy = () => {
  const callout = (label, x, y, dir = "right") => (
    <div className="anno" style={{ left: x, top: y, ["--anno-dir"]: dir }}>
      <div className="anno-line"/>
      <div className="anno-dot"/>
      <div className="anno-label">{label}</div>
    </div>
  );

  return (
    <div style={{
      background: "var(--bg)", padding: "44px 60px 60px",
      width: 1440, minHeight: 900, color: "var(--text)", position: "relative",
      fontFamily: "Inter, sans-serif",
    }}>
      <div style={{marginBottom: 36}}>
        <div style={{fontSize: 11, color: "var(--text-3)", letterSpacing: "0.12em", textTransform: "uppercase", marginBottom: 6}}>
          Component · standard
        </div>
        <h1 className="serif" style={{fontSize: 48, margin: 0, letterSpacing: "-0.02em"}}>
          <span className="serif-i">List</span><span style={{color: "var(--text-2)"}}>Card</span>
        </h1>
        <div style={{color: "var(--text-2)", fontSize: 14, marginTop: 6, maxWidth: 640}}>
          One component for every list in xvn — strategies, runs, decisions, positions, journal.
          Search, filter, and sort-by-added are mandatory. Filters and sort options are
          domain-specific; the chrome is identical everywhere.
        </div>
      </div>

      <div style={{display: "grid", gridTemplateColumns: "1fr 360px", gap: 48, alignItems: "start"}}>
        {/* — Annotated specimen — */}
        <div style={{position: "relative"}}>
          <ListCard
            title="Strategies"
            count={8}
            toolbar={{
              search: { value: "", onChange: () => {}, placeholder: "Search strategies, templates…" },
              filters: [
                { id: "status", label: "Status", value: "all", onChange: () => {}, options: [
                  { value: "all", label: "All status" }, { value: "Validated", label: "Validated" },
                ]},
                { id: "tpl", label: "Template", value: "all", onChange: () => {}, options: [
                  { value: "all", label: "All templates" },
                ]},
              ],
              sort: { value: "added", onChange: () => {}, options: LIST_STD_DEFAULT_SORT },
              actions: (
                <>
                  <button className="btn ghost"><Icon name="plus" size={13}/> New from template</button>
                  <button className="btn primary"><Icon name="plus" size={13} color="#001A0A"/> New strategy</button>
                </>
              ),
            }}
            columns={[
              { key: "name", label: "Name" }, { key: "tpl", label: "Template" },
              { key: "status", label: "Status" }, { key: "tokens", label: "Tokens", align: "right" },
              { key: "updated", label: "Updated" },
            ]}
            rows={[
              { name: "eth-mr-v3", tpl: "mean_reversion", status: "Validated", tok: "53.5k", updated: "14m ago" },
              { name: "btc-momentum-v1", tpl: "trend_follower", status: "Validated", tok: "42.0k", updated: "5h ago" },
              { name: "stablecoin-flow-v1", tpl: "carry", status: "Validated", tok: "31.0k", updated: "2d ago" },
            ]}
            renderRow={(r) => (
              <tr key={r.name} className="hover">
                <td className="mono">{r.name}</td>
                <td className="mute">{r.tpl}</td>
                <td><span className="dot gold"/>{r.status}</td>
                <td className="mono" style={{textAlign: "right"}}>{r.tok}</td>
                <td className="mute">{r.updated}</td>
              </tr>
            )}
          />

          {/* Anatomy callouts */}
          <div className="anatomy-overlay">
            {callout("1 · Header — title, count pill", -20, 28, "left")}
            {callout("2 · Search — / shortcut, clearable", -20, 84, "left")}
            {callout("3 · Filters — auto-highlight when active", 530, 84, "right")}
            {callout("4 · Sort — always present, defaults to 'Recently added'", 880, 84, "right")}
            {callout("5 · Right actions — page-level CTAs", 880, 28, "right")}
            {callout("6 · Active filter chips — appear under toolbar when set", -20, 150, "left")}
            {callout("7 · Table body — opaque to component, your renderRow", -20, 234, "left")}
          </div>
        </div>

        {/* — Right column: spec panel — */}
        <div style={{display: "flex", flexDirection: "column", gap: 18}}>
          <SpecPanel title="Behaviour" items={[
            ["Sort default", "Recently added"],
            ["Search", "Free-text, debounced 1 frame, '/' to focus"],
            ["Filters", "0–4, single-select, dropdown UI"],
            ["Chips", "Reflect non-default state, click to clear"],
            ["Density", "full · compact (mini-lists, rails)"],
          ]}/>
          <SpecPanel title="Sort options · default set" items={[
            ["1", "Recently added"],
            ["2", "Oldest first"],
            ["3", "Recently updated"],
            ["4", "Name A → Z"],
            ["5", "Name Z → A"],
          ]} mono/>
          <SpecPanel title="Used by" items={[
            ["Strategies", "/strategies"],
            ["Eval runs", "/eval/runs"],
            ["Decisions", "Run detail"],
            ["Trade ledger", "Run detail"],
            ["Positions", "Home rail"],
            ["Recent runs", "Home"],
            ["Journal", "/journal"],
          ]}/>
        </div>
      </div>

      <style>{`
        .anatomy-overlay { position: absolute; inset: 0; pointer-events: none; }
        .anno { position: absolute; display: flex; align-items: center; pointer-events: none; }
        .anno-dot {
          width: 6px; height: 6px; border-radius: 50%;
          background: var(--gold); box-shadow: 0 0 0 3px rgba(0, 230, 118, 0.18);
        }
        .anno-line {
          position: absolute; top: 50%; height: 1px; background: rgba(0, 230, 118, 0.5);
        }
        .anno-label {
          font-size: 11px; color: var(--gold);
          font-family: 'Geist Mono', monospace;
          letter-spacing: 0.02em;
          background: rgba(0, 230, 118, 0.08);
          padding: 3px 8px;
          border: 1px solid rgba(0, 230, 118, 0.25);
          border-radius: 2px;
          white-space: nowrap;
        }
        .anno[style*="--anno-dir: left"] { transform: translateX(-100%); }
        .anno[style*="--anno-dir: left"] .anno-dot { order: 2; margin-left: 8px; }
        .anno[style*="--anno-dir: left"] .anno-line { right: 0; width: 28px; transform: translateX(100%); }
        .anno[style*="--anno-dir: left"] .anno-label { order: 1; margin-right: 36px; }
        .anno[style*="--anno-dir: right"] .anno-dot { margin-right: 8px; }
        .anno[style*="--anno-dir: right"] .anno-line { left: 14px; width: 28px; }
        .anno[style*="--anno-dir: right"] .anno-label { margin-left: 36px; }
      `}</style>
    </div>
  );
};

const SpecPanel = ({ title, items, mono = false }) => (
  <div style={{
    background: "var(--surface-card)", border: "1px solid var(--border)",
    borderRadius: 6, padding: "14px 16px",
  }}>
    <div className="serif" style={{fontSize: 18, marginBottom: 10, color: "var(--text)"}}>{title}</div>
    <div style={{display: "flex", flexDirection: "column", gap: 6}}>
      {items.map(([k, v]) => (
        <div key={k} style={{display: "flex", justifyContent: "space-between", gap: 12, fontSize: 12}}>
          <span style={{color: "var(--text-3)", fontFamily: mono ? "'Geist Mono', monospace" : "inherit"}}>{k}</span>
          <span style={{color: "var(--text)", fontFamily: "'Geist Mono', monospace", textAlign: "right"}}>{v}</span>
        </div>
      ))}
    </div>
  </div>
);

window.ListAnatomy = ListAnatomy;
