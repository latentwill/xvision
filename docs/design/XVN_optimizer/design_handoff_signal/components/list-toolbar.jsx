// xvn — Standard List component
// ----------------------------------------------------------------------------
// One component used by every list in the app.
//
//   <ListCard
//     title="Strategies"            // optional, omit when used inside a card
//     count={8}                     // shown next to title as a pill
//     search={{ value, onChange, placeholder }}
//     filters={[                    // 0..n filters
//       { id: 'status', label: 'Status', value, onChange,
//         options: [{value:'all', label:'All'}, …] }
//     ]}
//     sort={{ value, onChange,      // 'added' is always available + default
//       options: [{value:'added', label:'Recently added'}, …] }}
//     actions={<>…</>}              // right-side buttons
//     density="full" | "compact"    // compact = mini lists, icon-only search
//     columns={…}                   // {key,label,align,width,sortable?}[]
//     rows={data}                   // any [], filtered/sorted internally
//     filterFn={(row, query, filterValues) => boolean}
//     sortFn={(rows, sortKey) => sortedRows}
//     renderRow={(row, i) => <tr>…</tr>}
//   />
//
// Two sub-pieces are exported too:
//   <ListToolbar … />     — toolbar only, drop above any custom table
//   <ListActiveChips … /> — removable filter chips
// ----------------------------------------------------------------------------

const LIST_STD_DEFAULT_SORT = [
  { value: "added",       label: "Recently added" },
  { value: "added-asc",   label: "Oldest first" },
  { value: "updated",     label: "Recently updated" },
  { value: "name",        label: "Name A → Z" },
  { value: "name-desc",   label: "Name Z → A" },
];

// ── Search input ────────────────────────────────────────────────────────────
const ListSearch = ({ value, onChange, placeholder = "Search…", width = 280, compact = false }) => {
  const [open, setOpen] = React.useState(!compact);
  React.useEffect(() => { if (!compact) setOpen(true); }, [compact]);
  if (compact && !open) {
    return (
      <button
        className="lt-iconbtn"
        onClick={() => setOpen(true)}
        title="Search"
        aria-label="Search"
      >
        <Icon name="search" size={13} color="var(--text-2)" />
      </button>
    );
  }
  return (
    <div className="lt-search" style={{ width: compact ? 200 : width }}>
      <Icon name="search" size={13} color="var(--text-3)" />
      <input
        autoFocus={compact}
        value={value || ""}
        onChange={(e) => onChange?.(e.target.value)}
        placeholder={placeholder}
        spellCheck={false}
      />
      {value && (
        <button
          className="lt-clear"
          onClick={() => { onChange?.(""); if (compact) setOpen(false); }}
          aria-label="Clear"
        >×</button>
      )}
      <span className="lt-kbd">/</span>
    </div>
  );
};

// ── Filter / Sort select ────────────────────────────────────────────────────
const ListSelect = ({ label, value, onChange, options, icon, width }) => {
  const selected = options.find(o => o.value === value) || options[0];
  const isDefault = selected?.value === options[0]?.value;
  return (
    <label className={"lt-select" + (isDefault ? "" : " is-active")} style={width ? { minWidth: width } : null}>
      {icon && <Icon name={icon} size={12} color={isDefault ? "var(--text-3)" : "var(--gold)"} />}
      <span className="lt-select-label">{label}</span>
      <span className="lt-select-value">{selected?.label}</span>
      <Icon name="chevR" size={11} color="var(--text-3)" />
      <select value={value} onChange={(e) => onChange?.(e.target.value)}>
        {options.map(o => <option key={o.value} value={o.value}>{o.label}</option>)}
      </select>
    </label>
  );
};

// ── Active filter chips row ─────────────────────────────────────────────────
const ListActiveChips = ({ search, onClearSearch, filters, onClearFilter, onClearAll }) => {
  const activeFilters = (filters || []).filter(f => f.value && f.value !== (f.options?.[0]?.value));
  const hasSearch = !!(search && String(search).trim());
  if (!hasSearch && activeFilters.length === 0) return null;
  return (
    <div className="lt-chips">
      <span className="lt-chips-label">Active</span>
      {hasSearch && (
        <button className="lt-chip" onClick={onClearSearch}>
          <span className="lt-chip-key">search</span>
          <span className="lt-chip-val">"{search}"</span>
          <span className="lt-chip-x">×</span>
        </button>
      )}
      {activeFilters.map(f => {
        const opt = f.options.find(o => o.value === f.value);
        return (
          <button key={f.id} className="lt-chip" onClick={() => onClearFilter(f.id)}>
            <span className="lt-chip-key">{f.label.toLowerCase()}</span>
            <span className="lt-chip-val">{opt?.label}</span>
            <span className="lt-chip-x">×</span>
          </button>
        );
      })}
      <button className="lt-chip-clear" onClick={onClearAll}>Clear all</button>
    </div>
  );
};

// ── Toolbar (search + filters + sort + actions) ─────────────────────────────
const ListToolbar = ({
  search, filters = [], sort, actions,
  density = "full",
  showSearch = true,
  showSort = true,
  showActiveChips = true,
}) => {
  const compact = density === "compact";
  const sortObj = sort || {};
  const sortOptions = sortObj.options || LIST_STD_DEFAULT_SORT;
  return (
    <div className={"lt" + (compact ? " lt--compact" : "")}>
      <div className="lt-row">
        {showSearch && search && (
          <ListSearch
            value={search.value}
            onChange={search.onChange}
            placeholder={search.placeholder}
            compact={compact}
          />
        )}
        <div className="lt-filters">
          {filters.map(f => (
            <ListSelect
              key={f.id}
              label={f.label}
              value={f.value}
              onChange={f.onChange}
              options={f.options}
              icon={f.icon}
            />
          ))}
        </div>
        {showSort && (
          <ListSelect
            label="Sort"
            icon="sliders"
            value={sortObj.value || sortOptions[0].value}
            onChange={sortObj.onChange}
            options={sortOptions}
            width={compact ? 120 : 180}
          />
        )}
        {actions && <div className="lt-actions">{actions}</div>}
      </div>

      {showActiveChips && !compact && (
        <ListActiveChips
          search={search?.value}
          onClearSearch={() => search?.onChange?.("")}
          filters={filters}
          onClearFilter={(id) => {
            const f = filters.find(x => x.id === id);
            f?.onChange?.(f.options[0].value);
          }}
          onClearAll={() => {
            search?.onChange?.("");
            filters.forEach(f => f.onChange?.(f.options[0].value));
          }}
        />
      )}
    </div>
  );
};

// ── Hook: useListState — manages search/filter/sort with derived rows ──────
const useListState = ({
  rows,
  filters: filterDefs = [],
  sortOptions,
  filterFn,
  sortFn,
  initialSort,
}) => {
  const [search, setSearch] = React.useState("");
  const [filterValues, setFilterValues] = React.useState(() => {
    const o = {};
    filterDefs.forEach(f => { o[f.id] = f.defaultValue ?? f.options[0].value; });
    return o;
  });
  const [sortKey, setSortKey] = React.useState(initialSort || (sortOptions || LIST_STD_DEFAULT_SORT)[0].value);

  const filters = filterDefs.map(f => ({
    ...f,
    value: filterValues[f.id],
    onChange: (v) => setFilterValues(s => ({ ...s, [f.id]: v })),
  }));

  const derived = React.useMemo(() => {
    let out = rows;
    if (filterFn) {
      out = out.filter(r => filterFn(r, search, filterValues));
    }
    if (sortFn) {
      out = sortFn([...out], sortKey);
    }
    return out;
  }, [rows, search, filterValues, sortKey, filterFn, sortFn]);

  return {
    search: { value: search, onChange: setSearch },
    filters,
    sort: { value: sortKey, onChange: setSortKey, options: sortOptions || LIST_STD_DEFAULT_SORT },
    rows: derived,
    setSearch, setFilterValues, setSortKey, filterValues,
  };
};

// ── ListCard — opinionated full list (toolbar + table inside a .card) ──────
const ListCard = ({
  title, count, subtitle,
  density = "full",
  toolbar,            // pre-built toolbar props OR a useListState result
  columns = [],
  renderRow,
  rows,
  empty = "Nothing matches your filters.",
  actions,
  footer,
  className = "",
  style,
}) => {
  const t = toolbar || {};
  return (
    <div className={"lc card " + className} style={style}>
      {(title || actions) && (
        <div className="lc-head">
          <div className="lc-head-l">
            {title && <h2 className="lc-title serif">{title}</h2>}
            {count != null && <span className="pill lc-count">{count}</span>}
            {subtitle && <span className="lc-subtitle mute">{subtitle}</span>}
          </div>
          {actions && <div className="lc-head-r">{actions}</div>}
        </div>
      )}
      <div className="lc-toolbar">
        <ListToolbar
          search={t.search}
          filters={t.filters}
          sort={t.sort}
          actions={t.actions}
          density={density}
          showSearch={t.showSearch !== false}
          showSort={t.showSort !== false}
          showActiveChips={t.showActiveChips !== false}
        />
      </div>
      <div className="lc-body">
        <table className="tbl lc-tbl">
          <thead>
            <tr>
              {columns.map((c, i) => (
                <th
                  key={c.key || i}
                  style={{
                    textAlign: c.align || "left",
                    width: c.width,
                    paddingLeft: i === 0 ? 20 : undefined,
                    paddingRight: i === columns.length - 1 ? 20 : undefined,
                  }}
                >{c.label}</th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr><td colSpan={columns.length} className="lc-empty mute">{empty}</td></tr>
            ) : rows.map((r, i) => renderRow(r, i))}
          </tbody>
        </table>
      </div>
      {footer && <div className="lc-footer">{footer}</div>}
    </div>
  );
};

// Inject component styles once
if (typeof document !== "undefined" && !document.getElementById("lt-styles")) {
  const s = document.createElement("style");
  s.id = "lt-styles";
  s.textContent = `
.lt { display: flex; flex-direction: column; gap: 10px; }
.lt-row { display: flex; align-items: center; gap: 8px; flex-wrap: wrap; }
.lt--compact .lt-row { gap: 6px; }

/* Search */
.lt-search {
  display: flex; align-items: center; gap: 8px;
  padding: 0 10px;
  height: 32px;
  background: var(--surface-elev);
  border: 1px solid var(--border);
  border-radius: 4px;
  transition: border-color .12s;
}
.lt-search:focus-within { border-color: var(--gold-soft); }
.lt-search input {
  flex: 1; min-width: 0;
  background: transparent;
  border: none; outline: none;
  color: var(--text);
  font-family: inherit;
  font-size: 13px;
  padding: 0;
}
.lt-search input::placeholder { color: var(--text-3); }
.lt-clear {
  border: none; background: none; cursor: pointer;
  color: var(--text-3); font-size: 16px; padding: 0 2px;
  line-height: 1;
}
.lt-clear:hover { color: var(--text); }
.lt-kbd {
  display: inline-flex; align-items: center; justify-content: center;
  width: 18px; height: 18px;
  border: 1px solid var(--border-strong);
  border-radius: 3px;
  font-family: 'Geist Mono', monospace;
  font-size: 10px;
  color: var(--text-3);
}
.lt--compact .lt-kbd { display: none; }

/* Icon-only button (compact search collapsed) */
.lt-iconbtn {
  width: 32px; height: 32px;
  display: inline-flex; align-items: center; justify-content: center;
  background: transparent;
  border: 1px solid var(--border);
  border-radius: 4px;
  cursor: pointer;
  transition: all .12s;
}
.lt-iconbtn:hover { border-color: var(--text-3); }

/* Select (filter/sort) */
.lt-filters { display: flex; gap: 6px; flex-wrap: wrap; align-items: center; }
.lt-select {
  position: relative;
  display: inline-flex; align-items: center; gap: 6px;
  height: 32px;
  padding: 0 10px 0 10px;
  background: var(--surface-elev);
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text-2);
  font-size: 12.5px;
  cursor: pointer;
  transition: all .12s;
  white-space: nowrap;
}
.lt-select:hover { border-color: var(--text-3); }
.lt-select.is-active {
  border-color: rgba(0, 230, 118, 0.45);
  background: rgba(0, 230, 118, 0.06);
}
.lt-select-label {
  color: var(--text-3);
  font-size: 11.5px;
  letter-spacing: 0.02em;
}
.lt--compact .lt-select-label { display: none; }
.lt-select-value {
  color: var(--text);
  font-weight: 500;
  font-size: 12.5px;
}
.lt-select.is-active .lt-select-value { color: var(--gold); }
.lt-select select {
  position: absolute; inset: 0;
  opacity: 0; cursor: pointer;
  font-family: inherit;
}

/* Right-aligned actions */
.lt-actions { margin-left: auto; display: flex; gap: 8px; align-items: center; }

/* Active chips row */
.lt-chips {
  display: flex; align-items: center; gap: 6px;
  flex-wrap: wrap;
}
.lt-chips-label {
  font-size: 10.5px; letter-spacing: 0.1em;
  text-transform: uppercase;
  color: var(--text-3);
  margin-right: 2px;
}
.lt-chip {
  display: inline-flex; align-items: center; gap: 6px;
  height: 22px;
  padding: 0 4px 0 8px;
  border-radius: 3px;
  border: 1px solid rgba(0, 230, 118, 0.35);
  background: rgba(0, 230, 118, 0.08);
  color: var(--gold);
  font-size: 11px;
  font-family: inherit;
  cursor: pointer;
  transition: all .12s;
}
.lt-chip:hover { background: rgba(0, 230, 118, 0.14); }
.lt-chip-key { color: var(--text-3); }
.lt-chip-val { font-weight: 500; }
.lt-chip-x {
  font-size: 14px; line-height: 1;
  margin-left: 2px;
  padding: 0 4px;
  color: var(--text-2);
}
.lt-chip:hover .lt-chip-x { color: var(--text); }
.lt-chip-clear {
  border: none; background: none; cursor: pointer;
  color: var(--text-3);
  font-family: inherit; font-size: 11.5px;
  padding: 0 4px;
  text-decoration: underline;
  text-decoration-color: var(--text-4);
  text-underline-offset: 3px;
}
.lt-chip-clear:hover { color: var(--text); }

/* ListCard wrapper */
.lc { display: flex; flex-direction: column; }
.lc-head {
  display: flex; align-items: center; justify-content: space-between;
  padding: 16px 20px 8px;
  gap: 16px;
}
.lc-head-l { display: flex; align-items: baseline; gap: 10px; }
.lc-title {
  margin: 0;
  font-family: 'Geist', sans-serif;
  font-weight: 500;
  font-size: 22px;
  letter-spacing: -0.01em;
  color: var(--text);
}
.lc-count { transform: translateY(-2px); }
.lc-subtitle { font-size: 12.5px; margin-left: 4px; }
.lc-head-r { display: flex; gap: 8px; align-items: center; }
.lc-toolbar { padding: 4px 20px 14px; }
.lc-body { border-top: 1px solid var(--border-soft); }
.lc-tbl { margin: 0; }
.lc-empty { padding: 28px 20px; text-align: center; }
.lc-footer {
  padding: 10px 20px;
  border-top: 1px solid var(--border-soft);
  color: var(--text-3);
  font-size: 12px;
  display: flex; align-items: center; justify-content: space-between;
}
`;
  document.head.appendChild(s);
}

window.ListSearch = ListSearch;
window.ListSelect = ListSelect;
window.ListActiveChips = ListActiveChips;
window.ListToolbar = ListToolbar;
window.ListCard = ListCard;
window.useListState = useListState;
window.LIST_STD_DEFAULT_SORT = LIST_STD_DEFAULT_SORT;
