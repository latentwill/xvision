// xvn — Mobile calendar variants
// Bottom-sheet & inline patterns for sub-768px viewports

// ─── M1 · Bottom-sheet range picker ─────────────────────────────────────────
// Lives at the bottom of a 390×844 viewport; renders inside an iPhone-ish frame.
const MobileBottomSheet = ({ initialView = "days" }) => {
  const [start, setStart] = React.useState(new Date(2026, 3, 18));
  const [end, setEnd] = React.useState(new Date(2026, 4, 18));
  const [anchor, setAnchor] = React.useState(new Date(2026, 4, 1));
  const [hover, setHover] = React.useState(null);
  const [activePreset, setActivePreset] = React.useState("30d");

  const handlePick = (d) => {
    if (!start || (start && end)) { setStart(d); setEnd(null); setActivePreset(null); }
    else if (d.getTime() < start.getTime()) { setStart(d); setActivePreset(null); }
    else { setEnd(d); setActivePreset(null); }
  };

  const dayCount = start && end ? Math.round((startOfDay(end) - startOfDay(start)) / DAY_MS) + 1 : null;

  return (
    <PhoneFrame>
      {/* Dimmed page beneath */}
      <div style={{
        position: "absolute", inset: 0,
        background: "linear-gradient(180deg, rgba(15,14,12,0.4), rgba(15,14,12,0.85))",
      }}>
        {/* faint glimpse of underlying page */}
        <div style={{padding: "60px 20px 0", opacity: 0.5}}>
          <div style={{
            fontFamily: "'Geist', sans-serif", fontSize: 28, color: "var(--text)",
            letterSpacing: "-0.01em",
          }}>Strategies</div>
          <div style={{color: "var(--text-3)", fontSize: 12, marginTop: 4}}>8 drafts · 5 validated</div>
        </div>
      </div>

      {/* Sheet */}
      <div style={{
        position: "absolute", left: 0, right: 0, bottom: 0,
        background: "var(--surface-card)",
        borderTopLeftRadius: 18, borderTopRightRadius: 18,
        borderTop: "1px solid var(--border-strong)",
        boxShadow: "0 -16px 40px rgba(0,0,0,0.45)",
        paddingBottom: 18,
        fontFamily: "Inter, sans-serif",
      }}>
        {/* Drag handle */}
        <div style={{display: "flex", justifyContent: "center", paddingTop: 8, paddingBottom: 4}}>
          <div style={{width: 36, height: 4, borderRadius: 2, background: "var(--border-strong)"}}/>
        </div>

        {/* Title + close */}
        <div style={{display: "flex", alignItems: "center", justifyContent: "space-between", padding: "8px 20px 6px"}}>
          <div>
            <div style={{fontFamily: "'Geist', sans-serif", fontSize: 22, fontWeight: 500, color: "var(--text)", letterSpacing: "-0.01em"}}>
              Backtest window
            </div>
            <div style={{fontSize: 11.5, color: "var(--text-3)", marginTop: 1}}>
              Pick a date range to backtest your strategies against
            </div>
          </div>
          <button style={{
            width: 32, height: 32, borderRadius: 16, border: "1px solid var(--border)",
            background: "transparent", color: "var(--text-2)",
            display: "flex", alignItems: "center", justifyContent: "center", cursor: "pointer",
          }}>
            <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round">
              <path d="M5 5l10 10M15 5L5 15"/>
            </svg>
          </button>
        </div>

        {/* Start / End pills */}
        <div style={{display: "grid", gridTemplateColumns: "1fr 1fr", gap: 8, padding: "12px 20px 0"}}>
          <MobileDateField label="Start" value={start} active={!end} />
          <MobileDateField label="End" value={end} active={!!end} />
        </div>

        {/* Preset chips */}
        <div style={{
          display: "flex", gap: 6, overflowX: "auto",
          padding: "14px 20px 12px",
        }}>
          {PRESETS.slice(0, 6).map(p => {
            const active = activePreset === p.id;
            return (
              <button key={p.id} onClick={() => {
                const [s, e] = p.range();
                setStart(s); setEnd(e);
                setAnchor(new Date(e.getFullYear(), e.getMonth(), 1));
                setActivePreset(p.id);
              }} style={{
                whiteSpace: "nowrap",
                padding: "6px 11px",
                borderRadius: 999,
                border: "1px solid " + (active ? "rgba(0, 230, 118, 0.5)" : "var(--border)"),
                background: active ? "var(--gold-bg)" : "var(--surface-elev)",
                color: active ? "var(--gold)" : "var(--text-2)",
                fontFamily: "inherit",
                fontSize: 12,
                cursor: "pointer",
                flexShrink: 0,
              }}>{p.label.replace(/ ·.*/,"")}</button>
            );
          })}
        </div>

        {/* Calendar */}
        <div style={{padding: "4px 18px 0"}}>
          <CalendarView
            anchor={anchor} setAnchor={setAnchor}
            start={start} end={end} hover={hover}
            onPick={handlePick} onHover={setHover}
            density="mobile"
            monthHeaderSize="lg"
            initialView={initialView}
          />
        </div>

        {/* Sticky footer: summary + apply */}
        <div style={{
          marginTop: 14, padding: "12px 20px 0",
          borderTop: "1px solid var(--border-soft)",
        }}>
          <div style={{display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 10}}>
            <div style={{
              fontFamily: "'Geist Mono', monospace",
              fontSize: 13,
              color: "var(--text)",
              letterSpacing: "0.01em",
            }}>
              {fmtShort(start) || "—"} <span style={{color: "var(--text-3)"}}>→</span> {fmtShort(end) || "—"}
            </div>
            {dayCount && (
              <div style={{
                fontSize: 11, color: "var(--gold)",
                fontFamily: "'Geist Mono', monospace",
                padding: "3px 8px",
                background: "var(--gold-bg)",
                border: "1px solid rgba(0, 230, 118, 0.3)",
                borderRadius: 3,
              }}>{dayCount} days</div>
            )}
          </div>
          <button className="btn primary" style={{
            width: "100%", justifyContent: "center", padding: "12px",
            fontSize: 14, fontWeight: 500,
          }}>Apply range</button>
        </div>
      </div>
    </PhoneFrame>
  );
};

const MobileDateField = ({ label, value, active }) => (
  <div style={{
    background: "var(--surface-elev)",
    border: active ? "1px solid var(--gold-soft)" : "1px solid var(--border)",
    borderRadius: 6,
    padding: "8px 12px 9px",
  }}>
    <div style={{
      fontSize: 9.5, letterSpacing: "0.14em", textTransform: "uppercase",
      color: active ? "var(--gold)" : "var(--text-3)",
      marginBottom: 3,
    }}>{label}</div>
    <div style={{
      fontFamily: "'Geist Mono', monospace",
      fontSize: 13.5,
      color: value ? "var(--text)" : "var(--text-3)",
    }}>{value ? fmtDate(value) : "Pick a date"}</div>
  </div>
);

// ─── M2 · Mobile inline picker (single-month, lives in a settings sub-screen) ───
const MobileInlineCard = () => {
  const [start, setStart] = React.useState(new Date(2025, 0, 1));
  const [end, setEnd] = React.useState(new Date(2025, 2, 31));
  const [anchor, setAnchor] = React.useState(new Date(2025, 1, 1));
  const [hover, setHover] = React.useState(null);
  const [activePreset, setActivePreset] = React.useState("q1-25");

  const handlePick = (d) => {
    if (!start || (start && end)) { setStart(d); setEnd(null); setActivePreset(null); }
    else if (d.getTime() < start.getTime()) { setStart(d); setActivePreset(null); }
    else { setEnd(d); setActivePreset(null); }
  };

  const dayCount = start && end ? Math.round((startOfDay(end) - startOfDay(start)) / DAY_MS) + 1 : null;

  return (
    <PhoneFrame>
      {/* Page chrome */}
      <div style={{padding: "56px 0 0"}}>
        <div style={{padding: "0 20px 14px", display: "flex", alignItems: "center", gap: 12}}>
          <button style={{
            width: 32, height: 32, borderRadius: 16, border: "1px solid var(--border)",
            background: "var(--surface-elev)", color: "var(--text-2)",
            display: "flex", alignItems: "center", justifyContent: "center", cursor: "pointer",
          }}>
            <svg width="14" height="14" viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 5l-5 5 5 5"/>
            </svg>
          </button>
          <div>
            <div style={{fontFamily: "'Geist', sans-serif", fontSize: 24, color: "var(--text)", letterSpacing: "-0.01em"}}>
              New backtest
            </div>
            <div style={{fontSize: 11, color: "var(--text-3)", marginTop: 1, fontFamily: "'Geist Mono', monospace"}}>eth-mr-v3</div>
          </div>
        </div>

        {/* Section label */}
        <div style={{
          padding: "8px 20px 8px",
          fontSize: 10.5, letterSpacing: "0.14em", textTransform: "uppercase", color: "var(--text-3)",
        }}>Window</div>

        {/* Date display row */}
        <div style={{margin: "0 16px", padding: "12px 14px", border: "1px solid var(--border)", borderRadius: 6, background: "var(--surface-card)"}}>
          <div style={{display: "flex", alignItems: "center", justifyContent: "space-between"}}>
            <div>
              <div style={{fontSize: 9.5, letterSpacing: "0.14em", textTransform: "uppercase", color: "var(--text-3)"}}>From</div>
              <div style={{fontFamily: "'Geist Mono', monospace", fontSize: 14, color: "var(--text)", marginTop: 2}}>{fmtDate(start)}</div>
            </div>
            <svg width="16" height="16" viewBox="0 0 20 20" fill="none" stroke="var(--text-3)" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M4 10h12M12 6l4 4-4 4"/>
            </svg>
            <div style={{textAlign: "right"}}>
              <div style={{fontSize: 9.5, letterSpacing: "0.14em", textTransform: "uppercase", color: "var(--text-3)"}}>To</div>
              <div style={{fontFamily: "'Geist Mono', monospace", fontSize: 14, color: "var(--text)", marginTop: 2}}>{fmtDate(end)}</div>
            </div>
          </div>
          {dayCount && (
            <div style={{
              marginTop: 10, paddingTop: 10, borderTop: "1px solid var(--border-soft)",
              display: "flex", justifyContent: "space-between", alignItems: "center",
            }}>
              <span style={{fontSize: 11, color: "var(--text-3)"}}>Trading days</span>
              <span style={{fontFamily: "'Geist Mono', monospace", fontSize: 12, color: "var(--gold)"}}>{dayCount}</span>
            </div>
          )}
        </div>

        {/* Preset chips row */}
        <div style={{display: "flex", gap: 6, overflowX: "auto", padding: "14px 16px 4px"}}>
          {PRESETS.slice(2, 7).map(p => {
            const active = activePreset === p.id;
            return (
              <button key={p.id} onClick={() => {
                const [s, e] = p.range();
                setStart(s); setEnd(e);
                setAnchor(new Date(s.getFullYear(), s.getMonth() + 1, 1));
                setActivePreset(p.id);
              }} style={{
                whiteSpace: "nowrap",
                padding: "6px 11px",
                borderRadius: 999,
                border: "1px solid " + (active ? "rgba(0, 230, 118, 0.5)" : "var(--border)"),
                background: active ? "var(--gold-bg)" : "transparent",
                color: active ? "var(--gold)" : "var(--text-2)",
                fontFamily: "inherit",
                fontSize: 11.5,
                cursor: "pointer",
                flexShrink: 0,
              }}>{p.label.replace(/ ·.*/,"")}</button>
            );
          })}
        </div>

        {/* Embedded calendar */}
        <div style={{margin: "10px 16px 0", padding: "14px 14px 10px", border: "1px solid var(--border)", borderRadius: 6, background: "var(--surface-card)"}}>
          <CalendarView
            anchor={anchor} setAnchor={setAnchor}
            start={start} end={end} hover={hover}
            onPick={handlePick} onHover={setHover}
            density="mobile"
            monthHeaderSize="md"
          />
        </div>
      </div>

      {/* Bottom Run button */}
      <div style={{
        position: "absolute", left: 0, right: 0, bottom: 0,
        padding: "12px 16px 30px",
        background: "linear-gradient(180deg, rgba(15,14,12,0) 0%, var(--bg) 40%)",
      }}>
        <button className="btn primary" style={{
          width: "100%", justifyContent: "center",
          padding: "13px", fontSize: 14, fontWeight: 500,
        }}>Run backtest</button>
      </div>
    </PhoneFrame>
  );
};

// ─── PhoneFrame · lightweight iPhone-ish bezel ─────────────────────────────
const PhoneFrame = ({ children }) => (
  <div style={{
    width: 390, height: 720,
    background: "var(--bg)",
    border: "1px solid var(--border-strong)",
    borderRadius: 28,
    position: "relative",
    overflow: "hidden",
    boxShadow: "inset 0 0 0 6px #000, 0 24px 48px rgba(0,0,0,0.55)",
  }}>
    {/* Status bar */}
    <div style={{
      position: "absolute", top: 0, left: 0, right: 0, height: 44,
      display: "flex", alignItems: "center", justifyContent: "space-between",
      padding: "0 28px",
      fontFamily: "'Geist Mono', monospace", fontSize: 13, color: "var(--text)",
      zIndex: 10,
    }}>
      <span>9:41</span>
      <div style={{display: "flex", gap: 5, alignItems: "center"}}>
        {/* signal */}
        <svg width="16" height="10" viewBox="0 0 16 10" fill="var(--text)">
          <rect x="0" y="6" width="2" height="3" rx="0.5"/>
          <rect x="3" y="4" width="2" height="5" rx="0.5"/>
          <rect x="6" y="2" width="2" height="7" rx="0.5"/>
          <rect x="9" y="0" width="2" height="9" rx="0.5"/>
        </svg>
        {/* battery */}
        <svg width="22" height="10" viewBox="0 0 22 10" fill="none">
          <rect x="0.5" y="0.5" width="18" height="9" rx="2" stroke="var(--text)" strokeOpacity="0.7"/>
          <rect x="2" y="2" width="14" height="6" rx="1" fill="var(--text)"/>
          <rect x="19.5" y="3.5" width="2" height="3" rx="0.5" fill="var(--text)"/>
        </svg>
      </div>
    </div>
    {/* Notch */}
    <div style={{
      position: "absolute", top: 8, left: "50%", transform: "translateX(-50%)",
      width: 110, height: 28, background: "#000", borderRadius: 14, zIndex: 11,
    }}/>
    {children}
  </div>
);

Object.assign(window, {
  MobileBottomSheet,
  MobileInlineCard,
  MobileDateField,
  PhoneFrame,
});
