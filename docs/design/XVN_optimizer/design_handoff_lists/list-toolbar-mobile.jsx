// xvn — Mobile list component
// ----------------------------------------------------------------------------
// Mobile counterpart to <ListCard>. Same contract (search + filters + sort,
// 'Recently added' is always available & default) — different shape:
//
//   - No table; rows are card-style list items via your renderRow
//   - Search bar is always visible, full width
//   - One control row: [Filter (n)] [Sort ▾]
//     Filter opens a bottom sheet with every filter and sort option
//   - Active filter chips below the controls
//
// API:
//   <MListCard
//     title="Strategies" count={8}
//     toolbar={{ search, filters, sort, sheetState: [open, setOpen] }}
//     rows={data}
//     renderRow={(r, i) => <MListRow …>}
//     empty="No matches" />
//
// useListState (from list-toolbar.jsx) works unchanged.
// ----------------------------------------------------------------------------

// ── Search bar (mobile, always-visible) ─────────────────────────────────────
const MListSearch = ({ value, onChange, placeholder = "Search…" }) => (
  <div className="ml-search">
    <Icon name="search" size={14} color="var(--text-3)" />
    <input
      value={value || ""}
      onChange={(e) => onChange?.(e.target.value)}
      placeholder={placeholder}
      spellCheck={false}
    />
    {value && (
      <button
        className="ml-clear"
        onClick={() => onChange?.("")}
        aria-label="Clear"
      >×</button>
    )}
  </div>
);

// ── Filter + Sort control row ───────────────────────────────────────────────
const MListControls = ({ filterCount, sortLabel, onOpenSheet, onOpenSort }) => (
  <div className="ml-controls">
    <button className={"ml-ctrl" + (filterCount > 0 ? " is-active" : "")} onClick={onOpenSheet}>
      <Icon name="sliders" size={13} color={filterCount > 0 ? "var(--gold)" : "var(--text-2)"} />
      <span>Filter</span>
      {filterCount > 0 && <span className="ml-ctrl-badge">{filterCount}</span>}
    </button>
    <button className="ml-ctrl ml-sort" onClick={onOpenSort}>
      <span className="ml-ctrl-l">Sort</span>
      <span className="ml-ctrl-v">{sortLabel}</span>
      <svg width="9" height="9" viewBox="0 0 16 16" fill="none" style={{flexShrink: 0}}>
        <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round"/>
      </svg>
    </button>
  </div>
);

// ── Active filter chips (mobile) ────────────────────────────────────────────
const MListChips = ({ search, onClearSearch, filters, onClearFilter, onClearAll }) => {
  const active = (filters || []).filter(f => f.value && f.value !== f.options[0].value);
  const hasSearch = !!(search && String(search).trim());
  if (!hasSearch && active.length === 0) return null;
  return (
    <div className="ml-chips">
      {hasSearch && (
        <button className="ml-chip" onClick={onClearSearch}>
          <span className="ml-chip-key">"{search}"</span>
          <span className="ml-chip-x">×</span>
        </button>
      )}
      {active.map(f => {
        const opt = f.options.find(o => o.value === f.value);
        return (
          <button key={f.id} className="ml-chip" onClick={() => onClearFilter(f.id)}>
            <span className="ml-chip-key">{f.label.toLowerCase()}</span>
            <span className="ml-chip-val">{opt?.label}</span>
            <span className="ml-chip-x">×</span>
          </button>
        );
      })}
      {(hasSearch || active.length > 1) && (
        <button className="ml-chip-clear" onClick={onClearAll}>Clear</button>
      )}
    </div>
  );
};

// ── Filter / Sort bottom sheet ──────────────────────────────────────────────
const MListSheet = ({ open, mode = "all", onClose, search, filters, sort, onClear, resultCount }) => {
  if (!open) return null;
  const sortOptions = sort?.options || LIST_STD_DEFAULT_SORT;
  const focusSort = mode === "sort";
  return (
    <div className="ml-sheet-wrap" onClick={onClose}>
      <div className="ml-sheet" onClick={(e) => e.stopPropagation()}>
        <div className="ml-sheet-grip"/>
        <div className="ml-sheet-head">
          <h3 className="serif">{focusSort ? "Sort by" : "Filter & sort"}</h3>
          <button className="ml-sheet-clear" onClick={onClear}>Clear all</button>
        </div>
        <div className="ml-sheet-body">
          {!focusSort && (
            <>
              {filters.map(f => (
                <div key={f.id} className="ml-group">
                  <div className="ml-group-label">{f.label}</div>
                  <div className="ml-group-options">
                    {f.options.map(o => {
                      const on = o.value === f.value;
                      return (
                        <button
                          key={o.value}
                          className={"ml-pill" + (on ? " is-on" : "")}
                          onClick={() => f.onChange(o.value)}
                        >
                          {on && <span className="ml-pill-check">✓</span>}
                          {o.label}
                        </button>
                      );
                    })}
                  </div>
                </div>
              ))}
            </>
          )}
          <div className="ml-group">
            <div className="ml-group-label">
              <Icon name="sliders" size={11} color="var(--gold)"/> Sort by
            </div>
            <div className="ml-sort-list">
              {sortOptions.map(o => {
                const on = o.value === (sort?.value || sortOptions[0].value);
                return (
                  <button
                    key={o.value}
                    className={"ml-sort-row" + (on ? " is-on" : "")}
                    onClick={() => sort.onChange(o.value)}
                  >
                    <span className="ml-sort-bullet">{on ? "●" : "○"}</span>
                    <span>{o.label}</span>
                  </button>
                );
              })}
            </div>
          </div>
        </div>
        <div className="ml-sheet-foot">
          <button className="btn primary ml-apply" onClick={onClose}>
            Show {resultCount} {resultCount === 1 ? "result" : "results"}
          </button>
        </div>
      </div>
    </div>
  );
};

// ── Toolbar (sticky header + search + controls + chips) ────────────────────
const MListToolbar = ({ search, filters = [], sort, sheetState, resultCount }) => {
  const [sheet, setSheet] = sheetState;
  const filterCount = filters.filter(f => f.value !== f.options[0].value).length;
  const sortOpts = sort?.options || LIST_STD_DEFAULT_SORT;
  const currentSort = sortOpts.find(o => o.value === sort?.value) || sortOpts[0];
  const clearAll = () => {
    search?.onChange?.("");
    filters.forEach(f => f.onChange?.(f.options[0].value));
    sort?.onChange?.(sortOpts[0].value);
  };
  return (
    <>
      <MListSearch
        value={search?.value}
        onChange={search?.onChange}
        placeholder={search?.placeholder}
      />
      <MListControls
        filterCount={filterCount}
        sortLabel={currentSort.label}
        onOpenSheet={() => setSheet("all")}
        onOpenSort={() => setSheet("sort")}
      />
      <MListChips
        search={search?.value}
        onClearSearch={() => search?.onChange?.("")}
        filters={filters}
        onClearFilter={(id) => {
          const f = filters.find(x => x.id === id);
          f?.onChange?.(f.options[0].value);
        }}
        onClearAll={clearAll}
      />
      <MListSheet
        open={!!sheet}
        mode={sheet}
        onClose={() => setSheet(null)}
        search={search}
        filters={filters}
        sort={sort}
        onClear={clearAll}
        resultCount={resultCount}
      />
    </>
  );
};

// ── Wrapper: sticky-header list with toolbar above scrollable rows ─────────
const MListCard = ({
  title, count, subtitle, rightAction,
  toolbar,
  rows,
  renderRow,
  empty = "No matches.",
  className = "",
  pad = true,
}) => {
  const sheetState = React.useState(null);
  return (
    <div className={"ml-card " + className}>
      <div className="ml-head">
        <div className="ml-head-l">
          <h2 className="serif">{title}</h2>
          {count != null && <span className="ml-count">{count}</span>}
          {subtitle && <span className="ml-subtitle">{subtitle}</span>}
        </div>
        {rightAction}
      </div>
      <div className="ml-toolbar">
        <MListToolbar {...toolbar} sheetState={sheetState} resultCount={rows.length}/>
      </div>
      <div className={"ml-rows" + (pad ? " ml-rows--pad" : "")}>
        {rows.length === 0 ? (
          <div className="ml-empty">{empty}</div>
        ) : rows.map((r, i) => renderRow(r, i))}
      </div>
    </div>
  );
};

// ── Convenience row component (most lists can use this) ────────────────────
const MListRow = ({
  title, subtitle, badge, badgeColor = "muted",
  rightTop, rightSub, rightColor,
  meta, onClick,
}) => (
  <button className="ml-row" onClick={onClick}>
    <div className="ml-row-body">
      <div className="ml-row-top">
        <span className="ml-row-title mono">{title}</span>
        {badge && <span className={"ml-row-badge ml-row-badge--" + badgeColor}>{badge}</span>}
      </div>
      {subtitle && <div className="ml-row-sub">{subtitle}</div>}
      {meta && <div className="ml-row-meta">{meta}</div>}
    </div>
    {(rightTop || rightSub) && (
      <div className="ml-row-right">
        {rightTop && <div className={"ml-row-right-top mono " + (rightColor || "")}>{rightTop}</div>}
        {rightSub && <div className="ml-row-right-sub mono">{rightSub}</div>}
      </div>
    )}
  </button>
);

// ── Styles ──────────────────────────────────────────────────────────────────
if (typeof document !== "undefined" && !document.getElementById("ml-list-styles")) {
  const s = document.createElement("style");
  s.id = "ml-list-styles";
  s.textContent = `
.ml-card {
  display: flex; flex-direction: column;
  height: 100%; min-height: 0;
  background: var(--bg);
}
.ml-head {
  display: flex; align-items: baseline; justify-content: space-between;
  padding: 16px 16px 6px;
  gap: 10px;
}
.ml-head-l { display: flex; align-items: baseline; gap: 8px; min-width: 0; }
.ml-head h2 {
  margin: 0;
  font-family: 'Geist', sans-serif;
  font-weight: 500; font-size: 26px; letter-spacing: -0.01em;
  color: var(--text);
}
.ml-count {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 22px; height: 20px; padding: 0 6px;
  border-radius: 100px;
  background: var(--gold-bg-strong);
  border: 1px solid rgba(0, 230, 118, 0.35);
  color: var(--gold);
  font-family: 'Geist Mono', monospace;
  font-size: 11px;
  font-variant-numeric: tabular-nums;
}
.ml-subtitle {
  font-size: 11.5px; color: var(--text-3);
  font-family: 'Geist Mono', monospace;
  margin-left: 4px;
}
.ml-toolbar {
  padding: 6px 16px 8px;
  display: flex; flex-direction: column; gap: 8px;
  border-bottom: 1px solid var(--border-soft);
}

/* Search */
.ml-search {
  display: flex; align-items: center; gap: 10px;
  height: 38px; padding: 0 12px;
  background: var(--surface-elev);
  border: 1px solid var(--border);
  border-radius: 100px;
}
.ml-search:focus-within { border-color: var(--gold-soft); }
.ml-search input {
  flex: 1; min-width: 0;
  background: transparent; border: none; outline: none;
  color: var(--text);
  font-family: inherit; font-size: 14px;
  padding: 0;
}
.ml-search input::placeholder { color: var(--text-3); }
.ml-clear {
  border: none; background: none; cursor: pointer;
  color: var(--text-3); font-size: 20px;
  line-height: 1; padding: 0 4px;
}

/* Controls row */
.ml-controls { display: flex; gap: 8px; }
.ml-ctrl {
  flex: 0 0 auto;
  display: inline-flex; align-items: center; gap: 6px;
  height: 32px; padding: 0 12px;
  background: var(--surface-card);
  border: 1px solid var(--border);
  border-radius: 100px;
  color: var(--text-2);
  font-family: inherit; font-size: 13px;
  cursor: pointer;
}
.ml-ctrl.is-active {
  border-color: rgba(0, 230, 118, 0.45);
  background: rgba(0, 230, 118, 0.06);
  color: var(--gold);
}
.ml-ctrl-badge {
  display: inline-flex; align-items: center; justify-content: center;
  min-width: 18px; height: 18px; padding: 0 5px;
  border-radius: 100px;
  background: var(--gold);
  color: #000000;
  font-family: 'Geist Mono', monospace;
  font-size: 10.5px;
  font-weight: 600;
}
.ml-sort {
  flex: 1;
  justify-content: space-between;
}
.ml-ctrl-l { color: var(--text-3); font-size: 11.5px; letter-spacing: 0.02em; }
.ml-ctrl-v { flex: 1; text-align: left; color: var(--text); font-weight: 500; }

/* Active filter chips */
.ml-chips {
  display: flex; gap: 6px; flex-wrap: wrap;
  margin-top: 2px;
}
.ml-chip {
  display: inline-flex; align-items: center; gap: 6px;
  height: 24px; padding: 0 4px 0 10px;
  border-radius: 100px;
  border: 1px solid rgba(0, 230, 118, 0.35);
  background: rgba(0, 230, 118, 0.08);
  color: var(--gold);
  font-family: inherit; font-size: 11.5px;
  cursor: pointer;
}
.ml-chip-key { color: var(--text-3); }
.ml-chip-val { font-weight: 500; }
.ml-chip-x { font-size: 14px; padding: 0 4px; line-height: 1; }
.ml-chip-clear {
  border: none; background: none; cursor: pointer;
  color: var(--text-3); font-family: inherit; font-size: 11.5px;
  padding: 0 6px;
  text-decoration: underline; text-decoration-color: var(--text-4);
  text-underline-offset: 3px;
}

/* Rows */
.ml-rows {
  flex: 1; overflow-y: auto;
  display: flex; flex-direction: column;
}
.ml-rows--pad { padding: 4px 12px 24px; gap: 6px; }
.ml-empty {
  padding: 36px 20px;
  text-align: center;
  color: var(--text-3);
  font-size: 13px;
}

/* Standard row */
.ml-row {
  display: flex; align-items: flex-start; justify-content: space-between;
  gap: 12px;
  width: 100%;
  padding: 12px 14px;
  background: var(--surface-card);
  border: 1px solid var(--border);
  border-radius: 8px;
  text-align: left;
  cursor: pointer;
  font-family: inherit;
}
.ml-row:active { background: var(--surface-hover); }
.ml-row-body { flex: 1; min-width: 0; display: flex; flex-direction: column; gap: 4px; }
.ml-row-top { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.ml-row-title {
  font-size: 13.5px;
  color: var(--text);
  font-family: 'Geist Mono', monospace;
  font-weight: 500;
}
.ml-row-badge {
  display: inline-flex; align-items: center; gap: 5px;
  height: 18px; padding: 0 7px;
  border-radius: 3px;
  font-family: 'Geist Mono', monospace;
  font-size: 9.5px;
  letter-spacing: 0.08em;
  text-transform: uppercase;
  border: 1px solid var(--border);
  background: transparent;
  color: var(--text-2);
}
.ml-row-badge--gold   { color: var(--gold);   border-color: rgba(0, 230, 118, 0.4); background: rgba(0, 230, 118, 0.08); }
.ml-row-badge--warn   { color: var(--warn);   border-color: rgba(255, 176, 32, 0.45); background: rgba(255, 176, 32, 0.08); }
.ml-row-badge--danger { color: var(--danger); border-color: rgba(255, 77, 77, 0.4);  background: rgba(255, 77, 77, 0.08); }
.ml-row-badge--info   { color: var(--info);   border-color: rgba(111,143,184,0.45); background: rgba(111,143,184,0.08); }
.ml-row-badge--muted  { color: var(--text-3); border-color: var(--border-soft); }
.ml-row-sub {
  font-size: 12px;
  color: var(--text-2);
  font-family: 'Geist Mono', monospace;
}
.ml-row-meta {
  font-size: 11px;
  color: var(--text-3);
  font-family: 'Geist Mono', monospace;
  margin-top: 2px;
}
.ml-row-right { display: flex; flex-direction: column; align-items: flex-end; gap: 2px; flex-shrink: 0; }
.ml-row-right-top {
  font-size: 15px;
  font-family: 'Geist', sans-serif;
  font-weight: 500;
  letter-spacing: -0.01em;
}
.ml-row-right-sub {
  font-size: 11px;
  color: var(--text-3);
}

/* Sheet */
.ml-sheet-wrap {
  position: absolute; inset: 0;
  background: rgba(0,0,0,0.55);
  backdrop-filter: blur(2px);
  z-index: 100;
  display: flex; align-items: flex-end;
}
.ml-sheet {
  width: 100%;
  max-height: 88%;
  background: var(--surface-card);
  border-top: 1px solid var(--border-strong);
  border-radius: 18px 18px 0 0;
  display: flex; flex-direction: column;
  box-shadow: 0 -20px 60px rgba(0,0,0,0.6);
  animation: ml-sheet-in .22s cubic-bezier(.2,.7,.3,1);
}
@keyframes ml-sheet-in { from { transform: translateY(100%); } to { transform: translateY(0); } }
.ml-sheet-grip {
  align-self: center;
  width: 36px; height: 4px;
  border-radius: 2px;
  background: var(--border-strong);
  margin: 10px 0 6px;
}
.ml-sheet-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 4px 18px 10px;
}
.ml-sheet-head h3 {
  margin: 0;
  font-family: 'Geist', sans-serif;
  font-weight: 500; font-size: 22px;
  font-style: normal;
  letter-spacing: -0.01em;
  color: var(--text);
}
.ml-sheet-clear {
  border: none; background: none; cursor: pointer;
  color: var(--text-3); font-family: 'Geist Mono', monospace;
  font-size: 11px; letter-spacing: 0.14em; text-transform: uppercase;
}
.ml-sheet-body {
  flex: 1; min-height: 0; overflow-y: auto;
  padding: 4px 18px 14px;
  display: flex; flex-direction: column; gap: 18px;
}
.ml-group { display: flex; flex-direction: column; gap: 8px; }
.ml-group-label {
  display: inline-flex; align-items: center; gap: 6px;
  font-family: 'Geist Mono', monospace;
  font-size: 10.5px; letter-spacing: 0.14em;
  text-transform: uppercase;
  color: var(--text-3);
}
.ml-group-options {
  display: flex; gap: 6px; flex-wrap: wrap;
}
.ml-pill {
  display: inline-flex; align-items: center; gap: 6px;
  height: 32px; padding: 0 12px;
  background: var(--surface-elev);
  border: 1px solid var(--border);
  border-radius: 100px;
  color: var(--text-2);
  font-family: inherit; font-size: 12.5px;
  cursor: pointer;
}
.ml-pill.is-on {
  background: rgba(0, 230, 118, 0.10);
  border-color: rgba(0, 230, 118, 0.5);
  color: var(--gold);
}
.ml-pill-check {
  font-size: 10px;
  color: var(--gold);
  margin-right: -2px;
}
.ml-sort-list {
  display: flex; flex-direction: column;
  background: var(--surface-elev);
  border: 1px solid var(--border);
  border-radius: 8px;
  overflow: hidden;
}
.ml-sort-row {
  display: flex; align-items: center; gap: 12px;
  padding: 12px 14px;
  background: transparent;
  border: none;
  border-bottom: 1px solid var(--border-soft);
  color: var(--text-2);
  font-family: inherit; font-size: 14px;
  text-align: left;
  cursor: pointer;
}
.ml-sort-row:last-child { border-bottom: none; }
.ml-sort-row.is-on { color: var(--gold); background: rgba(0, 230, 118, 0.06); }
.ml-sort-bullet {
  font-size: 16px;
  width: 14px;
  color: var(--gold);
}
.ml-sort-row:not(.is-on) .ml-sort-bullet { color: var(--text-4); }
.ml-sheet-foot {
  padding: 12px 18px 18px;
  border-top: 1px solid var(--border-soft);
}
.ml-apply {
  width: 100%; justify-content: center;
  height: 46px;
  border-radius: 100px;
  font-size: 14px;
}
`;
  document.head.appendChild(s);
}

window.MListSearch = MListSearch;
window.MListControls = MListControls;
window.MListChips = MListChips;
window.MListSheet = MListSheet;
window.MListToolbar = MListToolbar;
window.MListCard = MListCard;
window.MListRow = MListRow;
