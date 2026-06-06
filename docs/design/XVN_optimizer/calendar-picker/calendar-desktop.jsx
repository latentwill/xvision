// xvn — Desktop calendar variants
// Folio dark, gold-accent, Cormorant display + JetBrains numerics

// ─── V1 · Dual-month range popover w/ presets ──────────────────────────────
const DualMonthRangePopover = ({ initialStart, initialEnd, initialAnchor }) => {
  const [start, setStart] = React.useState(initialStart || new Date(2025, 0, 1));
  const [end, setEnd] = React.useState(initialEnd || new Date(2025, 2, 31));
  const [anchor, setAnchor] = React.useState(initialAnchor || new Date(2025, 0, 1));
  const [hover, setHover] = React.useState(null);
  const [picking, setPicking] = React.useState("end"); // "start" or "end"
  const [activePreset, setActivePreset] = React.useState("q1-25");
  // Shared view across both panes — when ≠ "days", we replace the dual-month
  // grid with a single full-width months/years picker so the user always sees
  // the jump-target candidates instead of two stale calendars.
  const [view, setView] = React.useState("days");

  const handlePick = (d) => {
    if (picking === "start" || (start && d.getTime() < start.getTime())) {
      setStart(d); setEnd(null); setPicking("end"); setActivePreset(null);
    } else {
      setEnd(d); setPicking("start"); setActivePreset(null);
    }
  };

  // Chevron behavior depends on which view is active.
  const step = (dir) => {
    if (view === "days") setAnchor(addMonths(anchor, dir));
    else if (view === "months") setAnchor(new Date(anchor.getFullYear() + dir, anchor.getMonth(), 1));
    else setAnchor(new Date(anchor.getFullYear() + dir * 12, anchor.getMonth(), 1));
  };

  const right = addMonths(anchor, 1);

  // Compute day count
  const dayCount = start && end ? Math.round((startOfDay(end) - startOfDay(start)) / DAY_MS) + 1 : null;

  return (
    <div style={{
      width: 680,
      background: "var(--surface-elev)",
      border: "1px solid var(--border-strong)",
      borderRadius: 6,
      boxShadow: "0 24px 48px rgba(0,0,0,0.55), 0 2px 8px rgba(0,0,0,0.4)",
      display: "grid",
      gridTemplateColumns: "176px 1fr",
      overflow: "hidden",
      fontFamily: "Inter, sans-serif",
    }}>
      {/* Preset rail */}
      <div style={{
        background: "var(--surface-sidebar)",
        borderRight: "1px solid var(--border-soft)",
        padding: "16px 0",
        display: "flex", flexDirection: "column",
      }}>
        <div style={{
          fontSize: 10.5, letterSpacing: "0.14em", textTransform: "uppercase",
          color: "var(--text-3)", padding: "0 16px 10px",
        }}>Quick ranges</div>
        {PRESETS.map(p => {
          const active = activePreset === p.id;
          return (
            <button key={p.id}
              onClick={() => {
                const [s, e] = p.range();
                setStart(s); setEnd(e); setAnchor(new Date(s.getFullYear(), s.getMonth(), 1));
                setActivePreset(p.id); setPicking("start");
              }}
              style={{
                textAlign: "left",
                padding: "7px 16px",
                background: active ? "var(--gold-bg)" : "transparent",
                border: "none",
                borderLeft: active ? "2px solid var(--gold)" : "2px solid transparent",
                color: active ? "var(--text)" : "var(--text-2)",
                fontFamily: "inherit",
                fontSize: 12.5,
                cursor: "pointer",
                fontWeight: active ? 500 : 400,
              }}>
              {p.label}
            </button>
          );
        })}
        <div style={{flex: 1}}/>
        <div style={{
          padding: "10px 16px",
          borderTop: "1px solid var(--border-soft)",
          fontSize: 11,
          color: "var(--text-3)",
        }}>
          Click a day to set the start.<br/>Click again to close the range.
        </div>
      </div>

      {/* Calendar body */}
      <div style={{padding: "16px 20px 12px"}}>
        {/* Top row: from/to display */}
        <div style={{display: "flex", gap: 10, alignItems: "stretch", marginBottom: 16}}>
          <DateField label="Start" value={start} active={picking === "start"} onClick={() => setPicking("start")} />
          <div style={{display: "flex", alignItems: "center", color: "var(--text-3)"}}>
            <svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M4 10h12M12 6l4 4-4 4"/>
            </svg>
          </div>
          <DateField label="End" value={end} active={picking === "end"} onClick={() => setPicking("end")} />
          <div style={{flex: 1}}/>
          {dayCount && (
            <div style={{
              alignSelf: "center",
              fontSize: 11,
              color: "var(--text-2)",
              fontFamily: "'Geist Mono', monospace",
              padding: "5px 10px",
              border: "1px solid var(--border)",
              borderRadius: 3,
              background: "var(--gold-bg)",
              letterSpacing: "0.02em",
            }}>
              {dayCount} <span style={{color: "var(--text-3)", marginLeft: 4}}>days</span>
            </div>
          )}
        </div>

        {/* Body: dual-month grid OR full-width month/year picker */}
        {view === "days" ? (
          <div style={{display: "grid", gridTemplateColumns: "1fr 1fr", gap: 24}}>
            <div>
              <MonthHeader
                anchor={anchor}
                view="days"
                onPrev={() => step(-1)}
                onNext={() => step(+1)}
                onMonthClick={() => setView("months")}
                onYearClick={() => setView("years")}
                canNext={false}
                size="md"
              />
              <MonthGrid anchor={anchor} start={start} end={end} hover={hover}
                onPick={handlePick} onHover={setHover} density="comfort" />
            </div>
            <div>
              <MonthHeader
                anchor={right}
                view="days"
                onPrev={() => step(-1)}
                onNext={() => step(+1)}
                onMonthClick={() => setView("months")}
                onYearClick={() => setView("years")}
                canPrev={false}
                size="md"
              />
              <MonthGrid anchor={right} start={start} end={end} hover={hover}
                onPick={handlePick} onHover={setHover} density="comfort" />
            </div>
          </div>
        ) : (
          <div>
            <MonthHeader
              anchor={anchor}
              view={view}
              onPrev={() => step(-1)}
              onNext={() => step(+1)}
              onMonthClick={() => setView(view === "months" ? "days" : "months")}
              onYearClick={() => setView(view === "years" ? "days" : "years")}
              size="md"
            />
            <div style={{maxWidth: 520, margin: "0 auto"}}>
              {view === "months" && (
                <MonthsView anchor={anchor}
                  selectedYear={start ? start.getFullYear() : null}
                  selectedMonth={start ? start.getMonth() : null}
                  onPick={(m) => {
                    setAnchor(new Date(anchor.getFullYear(), m, 1));
                    setView("days");
                  }}
                  density="comfort"
                />
              )}
              {view === "years" && (
                <YearsView anchor={anchor}
                  selectedYear={start ? start.getFullYear() : null}
                  onPick={(y) => {
                    setAnchor(new Date(y, anchor.getMonth(), 1));
                    setView("months");
                  }}
                  density="comfort"
                />
              )}
            </div>
          </div>
        )}

        {/* Footer */}
        <div style={{
          marginTop: 14, paddingTop: 14, borderTop: "1px solid var(--border-soft)",
          display: "flex", alignItems: "center", justifyContent: "space-between",
        }}>
          <div style={{fontSize: 11.5, color: "var(--text-3)"}}>
            <span style={{display: "inline-block", width: 8, height: 8, borderRadius: "50%", border: "1px solid rgba(0, 230, 118, 0.55)", marginRight: 6, verticalAlign: "middle"}}/>
            Today is <span className="mono" style={{color: "var(--text-2)"}}>May 18, 2026</span>
          </div>
          <div style={{display: "flex", gap: 8}}>
            <button className="btn ghost" style={{padding: "6px 14px", fontSize: 12.5}}>Cancel</button>
            <button className="btn primary" style={{padding: "6px 14px", fontSize: 12.5}}>Apply range</button>
          </div>
        </div>
      </div>
    </div>
  );
};

// Small read-only field used inside the popover header
const DateField = ({ label, value, active, onClick }) => (
  <button onClick={onClick} style={{
    flex: 1,
    background: "var(--surface-card)",
    border: active ? "1px solid var(--gold-soft)" : "1px solid var(--border)",
    borderRadius: 4,
    padding: "7px 12px 8px",
    textAlign: "left",
    fontFamily: "inherit",
    cursor: "pointer",
    minWidth: 130,
  }}>
    <div style={{
      fontSize: 10, letterSpacing: "0.12em", textTransform: "uppercase",
      color: active ? "var(--gold)" : "var(--text-3)",
      marginBottom: 2,
    }}>{label}</div>
    <div style={{
      fontFamily: "'Geist Mono', monospace",
      fontSize: 13,
      color: value ? "var(--text)" : "var(--text-3)",
    }}>{value ? fmtDate(value) : "—"}</div>
  </button>
);

// ─── V2 · Single-month compact w/ preset chips ─────────────────────────────
const CompactPresetCalendar = ({ initialStart, initialEnd, initialAnchor, initialView = "days" }) => {
  const [start, setStart] = React.useState(initialStart || new Date(2026, 3, 18));
  const [end, setEnd] = React.useState(initialEnd || new Date(2026, 4, 18));
  const [anchor, setAnchor] = React.useState(initialAnchor || new Date(2026, 4, 1));
  const [hover, setHover] = React.useState(null);
  const [activePreset, setActivePreset] = React.useState("30d");
  const [view, setView] = React.useState(initialView);

  const handlePick = (d) => {
    if (!start || (start && end)) { setStart(d); setEnd(null); setActivePreset(null); }
    else if (d.getTime() < start.getTime()) { setStart(d); setActivePreset(null); }
    else { setEnd(d); setActivePreset(null); }
  };

  const step = (dir) => {
    if (view === "days") setAnchor(addMonths(anchor, dir));
    else if (view === "months") setAnchor(new Date(anchor.getFullYear() + dir, anchor.getMonth(), 1));
    else setAnchor(new Date(anchor.getFullYear() + dir * 12, anchor.getMonth(), 1));
  };

  const quickRanges = PRESETS.slice(0, 5);

  return (
    <div style={{
      width: 340,
      background: "var(--surface-elev)",
      border: "1px solid var(--border-strong)",
      borderRadius: 6,
      boxShadow: "0 24px 48px rgba(0,0,0,0.55)",
      padding: "14px 16px 12px",
      fontFamily: "Inter, sans-serif",
    }}>
      <div style={{display: "flex", flexWrap: "wrap", gap: 4, marginBottom: 14}}>
        {quickRanges.map(p => {
          const active = activePreset === p.id;
          return (
            <button key={p.id} onClick={() => {
              const [s, e] = p.range();
              setStart(s); setEnd(e); setAnchor(new Date(e.getFullYear(), e.getMonth(), 1));
              setActivePreset(p.id);
            }} style={{
              padding: "4px 9px",
              borderRadius: 3,
              border: "1px solid " + (active ? "rgba(0, 230, 118, 0.5)" : "var(--border)"),
              background: active ? "var(--gold-bg)" : "transparent",
              color: active ? "var(--gold)" : "var(--text-2)",
              fontFamily: "inherit",
              fontSize: 11.5,
              cursor: "pointer",
              letterSpacing: "0.01em",
            }}>{p.label.replace(/ ·.*/,"")}</button>
          );
        })}
      </div>

      <CalendarView
        anchor={anchor} setAnchor={setAnchor}
        start={start} end={end} hover={hover}
        onPick={handlePick} onHover={setHover}
        density="comfort"
        monthHeaderSize="md"
        initialView={initialView}
      />

      <div style={{
        marginTop: 12, paddingTop: 12,
        borderTop: "1px solid var(--border-soft)",
        display: "flex", alignItems: "center", justifyContent: "space-between",
        gap: 10,
      }}>
        <div style={{
          fontFamily: "'Geist Mono', monospace",
          fontSize: 11.5,
          color: "var(--text-2)",
          letterSpacing: "0.01em",
        }}>
          {fmtShort(start) || "—"} <span style={{color: "var(--text-3)"}}>→</span> {fmtShort(end) || "—"}
        </div>
        <button className="btn primary" style={{padding: "5px 12px", fontSize: 12}}>Apply</button>
      </div>
    </div>
  );
};

// ─── V3 · Inline filter-bar trigger (collapsed state) ──────────────────────
const FilterBarTrigger = ({ start, end, label = "Backtest window", open = false }) => (
  <button style={{
    display: "inline-flex",
    alignItems: "center",
    gap: 10,
    padding: "7px 10px 7px 12px",
    background: open ? "var(--surface-elev)" : "var(--surface-elev)",
    border: "1px solid " + (open ? "var(--gold-soft)" : "var(--border)"),
    borderRadius: 4,
    color: "var(--text)",
    fontFamily: "inherit",
    fontSize: 13,
    cursor: "pointer",
    minWidth: 280,
  }}>
    <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="var(--text-3)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
      <rect x="3" y="5" width="14" height="12" rx="1.5"/>
      <path d="M7 3v4M13 3v4M3 9h14"/>
    </svg>
    <div style={{flex: 1, textAlign: "left"}}>
      <div style={{fontSize: 10, letterSpacing: "0.12em", textTransform: "uppercase", color: "var(--text-3)", lineHeight: 1}}>
        {label}
      </div>
      <div style={{marginTop: 3, fontFamily: "'Geist Mono', monospace", fontSize: 12.5, color: "var(--text)"}}>
        {fmtShort(start)} <span style={{color: "var(--text-3)"}}>→</span> {fmtShort(end)}
      </div>
    </div>
    <svg width="12" height="12" viewBox="0 0 20 20" fill="none" stroke="var(--text-3)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{
      transform: open ? "rotate(180deg)" : "none",
      transition: "transform .18s",
    }}>
      <path d="M5 8l5 5 5-5"/>
    </svg>
  </button>
);

// ─── V4 · Inline range bar — header + swing-out disclosure ─────────────────
// Sits in the page flow. Click to expand: header becomes a 700px-wide bar,
// the calendar body grows out below it. Doesn't overlay anything; the table
// underneath gets pushed down (an animated max-height transition handles
// the swing-out feel). No backdrop, no escape-to-close — Cancel/Apply do.
const InlineRangeBar = ({
  initialOpen = true,
  initialStart, initialEnd, initialAnchor,
  width = 720,
  label = "Backtest window",
}) => {
  const [open, setOpen] = React.useState(initialOpen);
  const [start, setStart] = React.useState(initialStart || new Date(2025, 0, 1));
  const [end, setEnd] = React.useState(initialEnd || new Date(2025, 2, 31));
  const [anchor, setAnchor] = React.useState(initialAnchor || new Date(2025, 0, 1));
  const [hover, setHover] = React.useState(null);
  const [picking, setPicking] = React.useState("end");
  const [activePreset, setActivePreset] = React.useState("q1-25");
  const [view, setView] = React.useState("days");

  const handlePick = (d) => {
    if (picking === "start" || (start && d.getTime() < start.getTime())) {
      setStart(d); setEnd(null); setPicking("end"); setActivePreset(null);
    } else {
      setEnd(d); setPicking("start"); setActivePreset(null);
    }
  };

  const step = (dir) => {
    if (view === "days") setAnchor(addMonths(anchor, dir));
    else if (view === "months") setAnchor(new Date(anchor.getFullYear() + dir, anchor.getMonth(), 1));
    else setAnchor(new Date(anchor.getFullYear() + dir * 12, anchor.getMonth(), 1));
  };

  const right = addMonths(anchor, 1);
  const dayCount = start && end ? Math.round((startOfDay(end) - startOfDay(start)) / DAY_MS) + 1 : null;

  return (
    <div style={{
      width,
      background: "var(--surface-elev)",
      border: "1px solid " + (open ? "var(--gold-soft)" : "var(--border)"),
      borderRadius: 6,
      fontFamily: "Inter, sans-serif",
      overflow: "hidden",
      transition: "border-color .15s",
    }}>
      {/* Always-visible header bar — clicking anywhere except the date fields toggles open */}
      <button
        onClick={() => setOpen(!open)}
        style={{
          width: "100%",
          display: "flex",
          alignItems: "center",
          gap: 14,
          padding: "10px 14px",
          background: "transparent",
          border: "none",
          borderBottom: open ? "1px solid var(--border-soft)" : "1px solid transparent",
          color: "var(--text)",
          fontFamily: "inherit",
          fontSize: 13,
          cursor: "pointer",
          textAlign: "left",
        }}
      >
        <svg width="15" height="15" viewBox="0 0 20 20" fill="none" stroke={open ? "var(--gold)" : "var(--text-3)"} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
          <rect x="3" y="5" width="14" height="12" rx="1.5"/>
          <path d="M7 3v4M13 3v4M3 9h14"/>
        </svg>
        <div style={{
          fontSize: 10.5, letterSpacing: "0.14em", textTransform: "uppercase",
          color: open ? "var(--gold)" : "var(--text-3)",
        }}>{label}</div>
        <div style={{
          fontFamily: "'Geist Mono', monospace",
          fontSize: 13,
          color: "var(--text)",
          letterSpacing: "0.01em",
        }}>
          {fmtDate(start)} <span style={{color: "var(--text-3)", margin: "0 6px"}}>→</span> {fmtDate(end)}
        </div>
        {dayCount && (
          <div style={{
            fontSize: 11, color: "var(--gold)",
            fontFamily: "'Geist Mono', monospace",
            padding: "2px 7px",
            background: "var(--gold-bg)",
            border: "1px solid rgba(0, 230, 118, 0.3)",
            borderRadius: 3,
            letterSpacing: "0.02em",
          }}>{dayCount} days</div>
        )}
        <div style={{flex: 1}}/>
        {activePreset && (
          <div style={{fontSize: 11, color: "var(--text-3)", fontStyle: "normal"}}>
            from preset · {PRESETS.find(p => p.id === activePreset)?.label}
          </div>
        )}
        <svg width="13" height="13" viewBox="0 0 20 20" fill="none" stroke="var(--text-3)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" style={{
          transform: open ? "rotate(180deg)" : "none",
          transition: "transform .18s",
        }}>
          <path d="M5 8l5 5 5-5"/>
        </svg>
      </button>

      {/* Swing-out body */}
      <div style={{
        maxHeight: open ? 540 : 0,
        overflow: "hidden",
        transition: "max-height .28s cubic-bezier(.2,.7,.3,1)",
      }}>
        <div style={{
          display: "grid",
          gridTemplateColumns: "176px 1fr",
        }}>
          {/* Preset rail */}
          <div style={{
            background: "var(--surface-card)",
            borderRight: "1px solid var(--border-soft)",
            padding: "12px 0 16px",
            display: "flex", flexDirection: "column",
          }}>
            <div style={{
              fontSize: 10, letterSpacing: "0.14em", textTransform: "uppercase",
              color: "var(--text-3)", padding: "0 16px 8px",
            }}>Quick ranges</div>
            {PRESETS.map(p => {
              const active = activePreset === p.id;
              return (
                <button key={p.id}
                  onClick={() => {
                    const [s, e] = p.range();
                    setStart(s); setEnd(e); setAnchor(new Date(s.getFullYear(), s.getMonth(), 1));
                    setActivePreset(p.id); setPicking("start"); setView("days");
                  }}
                  style={{
                    textAlign: "left",
                    padding: "6px 16px",
                    background: active ? "var(--gold-bg)" : "transparent",
                    border: "none",
                    borderLeft: active ? "2px solid var(--gold)" : "2px solid transparent",
                    color: active ? "var(--text)" : "var(--text-2)",
                    fontFamily: "inherit",
                    fontSize: 12.5,
                    cursor: "pointer",
                    fontWeight: active ? 500 : 400,
                  }}>
                  {p.label}
                </button>
              );
            })}
          </div>

          {/* Calendar body */}
          <div style={{padding: "14px 18px 14px"}}>
            {/* Start/End fields */}
            <div style={{display: "flex", gap: 8, alignItems: "stretch", marginBottom: 14}}>
              <DateField label="Start" value={start} active={picking === "start"} onClick={() => setPicking("start")} />
              <div style={{display: "flex", alignItems: "center", color: "var(--text-3)"}}>
                <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M4 10h12M12 6l4 4-4 4"/>
                </svg>
              </div>
              <DateField label="End" value={end} active={picking === "end"} onClick={() => setPicking("end")} />
            </div>

            {/* Body: dual-month OR month/year picker */}
            {view === "days" ? (
              <div style={{display: "grid", gridTemplateColumns: "1fr 1fr", gap: 20}}>
                <div>
                  <MonthHeader anchor={anchor} view="days"
                    onPrev={() => step(-1)} onNext={() => step(+1)}
                    onMonthClick={() => setView("months")}
                    onYearClick={() => setView("years")}
                    canNext={false} size="md"/>
                  <MonthGrid anchor={anchor} start={start} end={end} hover={hover}
                    onPick={handlePick} onHover={setHover} density="compact"/>
                </div>
                <div>
                  <MonthHeader anchor={right} view="days"
                    onPrev={() => step(-1)} onNext={() => step(+1)}
                    onMonthClick={() => setView("months")}
                    onYearClick={() => setView("years")}
                    canPrev={false} size="md"/>
                  <MonthGrid anchor={right} start={start} end={end} hover={hover}
                    onPick={handlePick} onHover={setHover} density="compact"/>
                </div>
              </div>
            ) : (
              <div>
                <MonthHeader anchor={anchor} view={view}
                  onPrev={() => step(-1)} onNext={() => step(+1)}
                  onMonthClick={() => setView(view === "months" ? "days" : "months")}
                  onYearClick={() => setView(view === "years" ? "days" : "years")}
                  size="md"/>
                <div style={{maxWidth: 460, margin: "0 auto"}}>
                  {view === "months" && (
                    <MonthsView anchor={anchor}
                      selectedYear={start ? start.getFullYear() : null}
                      selectedMonth={start ? start.getMonth() : null}
                      onPick={(m) => { setAnchor(new Date(anchor.getFullYear(), m, 1)); setView("days"); }}
                      density="comfort"/>
                  )}
                  {view === "years" && (
                    <YearsView anchor={anchor}
                      selectedYear={start ? start.getFullYear() : null}
                      onPick={(y) => { setAnchor(new Date(y, anchor.getMonth(), 1)); setView("months"); }}
                      density="comfort"/>
                  )}
                </div>
              </div>
            )}

            {/* Footer */}
            <div style={{
              marginTop: 12, paddingTop: 12, borderTop: "1px solid var(--border-soft)",
              display: "flex", alignItems: "center", justifyContent: "space-between",
            }}>
              <div style={{fontSize: 11, color: "var(--text-3)"}}>
                Tip: click the <span style={{color: "var(--text-2)"}}>month</span> or <span style={{color: "var(--text-2)", fontStyle: "normal"}}>year</span> to jump.
              </div>
              <div style={{display: "flex", gap: 8}}>
                <button onClick={() => setOpen(false)} className="btn ghost" style={{padding: "5px 12px", fontSize: 12}}>Cancel</button>
                <button onClick={() => setOpen(false)} className="btn primary" style={{padding: "5px 12px", fontSize: 12}}>Apply range</button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};

Object.assign(window, {
  DualMonthRangePopover,
  CompactPresetCalendar,
  FilterBarTrigger,
  DateField,
  InlineRangeBar,
});
