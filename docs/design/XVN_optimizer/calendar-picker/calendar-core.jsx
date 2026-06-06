// xvn — Calendar core utilities and primitives
// Folio dark theme, gold-accent range picker grammar

const TODAY = new Date(2026, 4, 18); // May 18, 2026 (fixed for design)

const DAY_MS = 86_400_000;
const startOfDay = (d) => new Date(d.getFullYear(), d.getMonth(), d.getDate());
const sameDay = (a, b) => a && b && a.getFullYear()===b.getFullYear() && a.getMonth()===b.getMonth() && a.getDate()===b.getDate();
const inRange = (d, a, b) => {
  if (!a || !b) return false;
  const t = startOfDay(d).getTime();
  const lo = Math.min(startOfDay(a).getTime(), startOfDay(b).getTime());
  const hi = Math.max(startOfDay(a).getTime(), startOfDay(b).getTime());
  return t >= lo && t <= hi;
};
const addMonths = (d, n) => new Date(d.getFullYear(), d.getMonth()+n, 1);
const monthName = (d) => d.toLocaleString("en", {month: "long"});
const monthShort = (d) => d.toLocaleString("en", {month: "short"});
const fmtDate = (d) => d ? `${d.toLocaleString("en",{month:"short"})} ${d.getDate()}, ${d.getFullYear()}` : "";
const fmtShort = (d) => d ? `${String(d.getMonth()+1).padStart(2,"0")}/${String(d.getDate()).padStart(2,"0")}/${String(d.getFullYear()).slice(2)}` : "";

// Build a 6x7 grid of dates for the month containing `anchor`, Monday-start.
function buildMonthGrid(anchor) {
  const first = new Date(anchor.getFullYear(), anchor.getMonth(), 1);
  // 0 = Mon, 6 = Sun
  const offset = (first.getDay() + 6) % 7;
  const gridStart = new Date(first);
  gridStart.setDate(first.getDate() - offset);
  const cells = [];
  for (let i = 0; i < 42; i++) {
    const d = new Date(gridStart);
    d.setDate(gridStart.getDate() + i);
    cells.push(d);
  }
  return cells;
}

const WEEKDAYS = ["M","T","W","T","F","S","S"];

// Compute scenario presets relative to TODAY
const PRESETS = [
  { id: "7d",    label: "Last 7 days",   range: () => [new Date(TODAY.getTime() - 6*DAY_MS), TODAY] },
  { id: "30d",   label: "Last 30 days",  range: () => [new Date(TODAY.getTime() - 29*DAY_MS), TODAY] },
  { id: "90d",   label: "Last 90 days",  range: () => [new Date(TODAY.getTime() - 89*DAY_MS), TODAY] },
  { id: "ytd",   label: "Year to date",  range: () => [new Date(TODAY.getFullYear(),0,1), TODAY] },
  { id: "q1-25", label: "Q1 2025 · bull",  range: () => [new Date(2025,0,1), new Date(2025,2,31)] },
  { id: "q2-25", label: "Q2 2025 · chop",  range: () => [new Date(2025,3,1), new Date(2025,5,30)] },
  { id: "q3-24", label: "Q3 2024 · bear",  range: () => [new Date(2024,6,1), new Date(2024,8,30)] },
  { id: "12m",   label: "Trailing 12 months", range: () => [new Date(TODAY.getFullYear()-1, TODAY.getMonth(), TODAY.getDate()), TODAY] },
];

// ─── Month grid component ────────────────────────────────────────────────
const MonthGrid = ({
  anchor,
  start,
  end,
  hover,
  onPick,
  onHover,
  density = "comfort", // "comfort" | "compact" | "mobile"
  showWeekHeader = true,
}) => {
  const cells = buildMonthGrid(anchor);
  const curMonth = anchor.getMonth();

  const dims = {
    comfort: { cell: 36, font: 13,  gap: 0,   weekFs: 10.5, weekHeight: 28 },
    compact: { cell: 30, font: 12,  gap: 0,   weekFs: 10,   weekHeight: 24 },
    mobile:  { cell: 44, font: 16,  gap: 2,   weekFs: 11,   weekHeight: 32 },
  }[density];

  // Resolve effective end for in-range computation (hover preview)
  const effEnd = end || (start && hover && hover.getTime() !== start.getTime() ? hover : null);

  return (
    <div style={{display: "grid", gridTemplateColumns: "repeat(7, 1fr)", rowGap: dims.gap}}>
      {showWeekHeader && WEEKDAYS.map((w, i) => (
        <div key={`wh-${i}`} style={{
          height: dims.weekHeight,
          display: "flex", alignItems: "center", justifyContent: "center",
          fontSize: dims.weekFs, color: "var(--text-3)",
          letterSpacing: "0.12em", textTransform: "uppercase",
          fontFamily: "Inter, sans-serif",
        }}>{w}</div>
      ))}
      {cells.map((d, i) => {
        const isCur = d.getMonth() === curMonth;
        const isToday = sameDay(d, TODAY);
        const isStart = sameDay(d, start);
        const isEnd = sameDay(d, end) || (!end && start && hover && sameDay(d, hover) && hover.getTime() !== start.getTime());
        const isWithin = inRange(d, start, effEnd) && !isStart && !isEnd;
        const isEdge = isStart || isEnd;

        // Determine if this cell connects to the left/right within the range
        const lo = start && effEnd ? Math.min(startOfDay(start).getTime(), startOfDay(effEnd).getTime()) : null;
        const hi = start && effEnd ? Math.max(startOfDay(start).getTime(), startOfDay(effEnd).getTime()) : null;
        const t = startOfDay(d).getTime();
        const hasLeft  = lo !== null && t > lo && t <= hi;
        const hasRight = lo !== null && t >= lo && t < hi;

        // Background bar for in-range and edge cells
        const bar = (hasLeft || hasRight) ? (
          <div style={{
            position: "absolute",
            top: 4, bottom: 4,
            left: hasLeft ? 0 : "50%",
            right: hasRight ? 0 : "50%",
            background: "var(--gold-bg)",
            pointerEvents: "none",
          }}/>
        ) : null;

        const dayCircle = (
          <div style={{
            position: "relative", zIndex: 1,
            width: dims.cell - 4, height: dims.cell - 4,
            display: "flex", alignItems: "center", justifyContent: "center",
            borderRadius: "50%",
            fontFamily: "'Geist Mono', monospace",
            fontVariantNumeric: "tabular-nums",
            fontSize: dims.font,
            fontWeight: isEdge ? 600 : 400,
            color: isEdge ? "#000000" :
                   !isCur ? "var(--text-4)" :
                   isToday ? "var(--gold)" :
                   "var(--text)",
            background: isEdge ? "var(--gold)" : "transparent",
            border: isToday && !isEdge ? "1px solid rgba(0, 230, 118, 0.55)" : "1px solid transparent",
            transition: "background .12s, color .12s",
          }}>
            {d.getDate()}
          </div>
        );

        return (
          <button
            key={i}
            onClick={() => onPick && onPick(d)}
            onMouseEnter={() => onHover && onHover(d)}
            style={{
              position: "relative",
              height: dims.cell,
              border: "none",
              background: "transparent",
              padding: 0,
              cursor: "pointer",
              display: "flex", alignItems: "center", justifyContent: "center",
              opacity: isCur ? 1 : 0.55,
            }}
          >
            {bar}
            {dayCircle}
          </button>
        );
      })}
    </div>
  );
};

// Month header with prev/next chevrons.
// Month name and year are independently clickable — `view` is the currently-
// active panel ("days" | "months" | "years"); clicking the matching label
// toggles back to days. When `view !== "days"` the chevrons step by year
// (or decade in years view) since the day-step chevrons are meaningless there.
const MonthHeader = ({
  anchor, view = "days",
  onPrev, onNext,
  onMonthClick, onYearClick,
  size = "md", canPrev = true, canNext = true,
}) => {
  const sizes = {
    sm: { fs: 16, btn: 24, iconSize: 13, gap: 6 },
    md: { fs: 20, btn: 28, iconSize: 14, gap: 6 },
    lg: { fs: 26, btn: 32, iconSize: 16, gap: 8 },
  }[size];

  const labelBtn = (text, italic, onClick, active) => (
    <button onClick={onClick} className="month-jump-btn" data-active={active ? "true" : "false"} style={{
      background: active ? "var(--gold-bg)" : "transparent",
      border: "none",
      borderRadius: 3,
      padding: "2px 5px 2px 6px",
      cursor: "pointer",
      fontFamily: "'Geist', sans-serif",
      fontStyle: italic ? "italic" : "normal",
      fontWeight: 500,
      fontSize: sizes.fs,
      letterSpacing: "-0.01em",
      color: active ? "var(--gold)" : "var(--text)",
      transition: "background .12s, color .12s",
      display: "inline-flex",
      alignItems: "baseline",
      gap: 3,
    }}>
      <span style={{
        borderBottom: active ? "1px solid transparent" : "1px dotted var(--text-4)",
        paddingBottom: 1,
      }}>{text}</span>
      <svg width={Math.round(sizes.fs * 0.42)} height={Math.round(sizes.fs * 0.42)} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={{
        opacity: active ? 1 : 0.55,
        transform: "translateY(2px)",
        flexShrink: 0,
      }}>
        <path d="M5 8l5 5 5-5"/>
      </svg>
    </button>
  );

  // In years view, render the decade range (e.g. "2020 — 2031")
  let titleEl;
  if (view === "years") {
    const startYear = Math.floor(anchor.getFullYear() / 12) * 12;
    titleEl = (
      <div style={{
        fontFamily: "'Geist', sans-serif",
        fontWeight: 500, fontSize: sizes.fs,
        letterSpacing: "-0.01em", color: "var(--text)",
        padding: "2px 6px",
      }}>
        {startYear} <span style={{color: "var(--text-3)"}}>—</span> <span style={{color: "var(--text-3)", fontStyle: "normal"}}>{startYear + 11}</span>
      </div>
    );
  } else if (view === "months") {
    titleEl = (
      <div style={{display: "flex", alignItems: "center", gap: 2}}>
        <span style={{
          fontFamily: "'Geist', sans-serif",
          fontWeight: 500, fontSize: sizes.fs,
          color: "var(--text-3)", padding: "2px 6px",
        }}>Pick a month in </span>
        {labelBtn(anchor.getFullYear(), true, onYearClick, false)}
      </div>
    );
  } else {
    titleEl = (
      <div style={{display: "flex", alignItems: "center", gap: 2}}>
        {labelBtn(monthName(anchor), false, onMonthClick, false)}
        {labelBtn(anchor.getFullYear(), true, onYearClick, false)}
      </div>
    );
  }

  return (
    <div style={{
      display: "flex", alignItems: "center", justifyContent: "space-between",
      padding: "2px 4px 14px",
    }}>
      <button onClick={onPrev} disabled={!canPrev} style={{
        width: sizes.btn, height: sizes.btn, border: "1px solid var(--border)",
        background: "transparent", borderRadius: 4, cursor: canPrev ? "pointer" : "default",
        color: canPrev ? "var(--text-2)" : "var(--text-4)",
        display: "flex", alignItems: "center", justifyContent: "center",
        opacity: canPrev ? 1 : 0.4,
      }}>
        <svg width={sizes.iconSize} height={sizes.iconSize} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M12 5l-5 5 5 5"/>
        </svg>
      </button>
      {titleEl}
      <button onClick={onNext} disabled={!canNext} style={{
        width: sizes.btn, height: sizes.btn, border: "1px solid var(--border)",
        background: "transparent", borderRadius: 4, cursor: canNext ? "pointer" : "default",
        color: canNext ? "var(--text-2)" : "var(--text-4)",
        display: "flex", alignItems: "center", justifyContent: "center",
        opacity: canNext ? 1 : 0.4,
      }}>
        <svg width={sizes.iconSize} height={sizes.iconSize} viewBox="0 0 20 20" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round">
          <path d="M8 5l5 5-5 5"/>
        </svg>
      </button>
    </div>
  );
};

// 4x3 grid of months; tap one to jump anchor to that month, year unchanged.
const MonthsView = ({ anchor, selectedYear, selectedMonth, onPick, density = "comfort" }) => {
  const months = ["Jan","Feb","Mar","Apr","May","Jun","Jul","Aug","Sep","Oct","Nov","Dec"];
  const rowH = density === "mobile" ? 56 : 48;
  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: "repeat(3, 1fr)",
      gap: 6,
      paddingTop: 4,
    }}>
      {months.map((m, i) => {
        const isCurrent = i === anchor.getMonth();
        const isSelected = selectedYear === anchor.getFullYear() && selectedMonth === i;
        return (
          <button key={m} onClick={() => onPick(i)} style={{
            height: rowH,
            border: "1px solid " + (isSelected ? "rgba(0, 230, 118, 0.5)" : "var(--border)"),
            background: isSelected ? "var(--gold)" : (isCurrent ? "var(--gold-bg)" : "transparent"),
            color: isSelected ? "#000000" : (isCurrent ? "var(--gold)" : "var(--text-2)"),
            borderRadius: 4,
            fontFamily: "Inter, sans-serif",
            fontSize: density === "mobile" ? 14 : 13,
            fontWeight: isSelected ? 600 : 500,
            cursor: "pointer",
            transition: "background .12s, color .12s, border-color .12s",
          }}
          onMouseEnter={e => { if (!isSelected) e.currentTarget.style.borderColor = "var(--text-3)"; }}
          onMouseLeave={e => { if (!isSelected) e.currentTarget.style.borderColor = "var(--border)"; }}
          >{m}</button>
        );
      })}
    </div>
  );
};

// 4x3 grid of years; tap one to jump anchor's year. Chevrons step the decade.
const YearsView = ({ anchor, selectedYear, onPick, density = "comfort" }) => {
  const startYear = Math.floor(anchor.getFullYear() / 12) * 12;
  const rowH = density === "mobile" ? 56 : 48;
  const cells = Array.from({length: 12}, (_, i) => startYear + i);
  return (
    <div style={{
      display: "grid",
      gridTemplateColumns: "repeat(3, 1fr)",
      gap: 6,
      paddingTop: 4,
    }}>
      {cells.map(y => {
        const isCurrent = y === anchor.getFullYear();
        const isSelected = y === selectedYear;
        const isToday = y === TODAY.getFullYear();
        return (
          <button key={y} onClick={() => onPick(y)} style={{
            height: rowH,
            border: "1px solid " + (isSelected ? "rgba(0, 230, 118, 0.5)" :
                                    isToday && !isSelected ? "rgba(0, 230, 118, 0.35)" :
                                    "var(--border)"),
            background: isSelected ? "var(--gold)" : (isCurrent ? "var(--gold-bg)" : "transparent"),
            color: isSelected ? "#000000" : (isCurrent || isToday ? "var(--gold)" : "var(--text-2)"),
            borderRadius: 4,
            fontFamily: "'Geist Mono', monospace",
            fontVariantNumeric: "tabular-nums",
            fontSize: density === "mobile" ? 14 : 13,
            fontWeight: isSelected ? 600 : 500,
            cursor: "pointer",
            transition: "background .12s, color .12s, border-color .12s",
          }}
          onMouseEnter={e => { if (!isSelected) e.currentTarget.style.borderColor = "var(--text-3)"; }}
          onMouseLeave={e => { if (!isSelected) e.currentTarget.style.borderColor = isToday ? "rgba(0, 230, 118, 0.35)" : "var(--border)"; }}
          >{y}</button>
        );
      })}
    </div>
  );
};

// CalendarView · stitches MonthHeader + (days|months|years) bodies together.
// Manages its own `view` state and translates chevron clicks correctly:
// days → ±1 month, months → ±1 year, years → ±12 years.
const CalendarView = ({
  anchor, setAnchor,
  start, end, hover,
  onPick, onHover,
  density = "comfort",
  monthHeaderSize = "md",
  initialView = "days",
}) => {
  const [view, setView] = React.useState(initialView);

  const step = (dir) => {
    if (view === "days") setAnchor(addMonths(anchor, dir));
    else if (view === "months") setAnchor(new Date(anchor.getFullYear() + dir, anchor.getMonth(), 1));
    else setAnchor(new Date(anchor.getFullYear() + dir * 12, anchor.getMonth(), 1));
  };

  return (
    <div>
      <MonthHeader
        anchor={anchor}
        view={view}
        onPrev={() => step(-1)}
        onNext={() => step(+1)}
        onMonthClick={() => setView(view === "months" ? "days" : "months")}
        onYearClick={() => setView(view === "years" ? "days" : "years")}
        size={monthHeaderSize}
      />
      {view === "days" && (
        <MonthGrid anchor={anchor} start={start} end={end} hover={hover}
          onPick={onPick} onHover={onHover} density={density}/>
      )}
      {view === "months" && (
        <MonthsView anchor={anchor}
          selectedYear={start ? start.getFullYear() : null}
          selectedMonth={start ? start.getMonth() : null}
          onPick={(m) => {
            setAnchor(new Date(anchor.getFullYear(), m, 1));
            setView("days");
          }}
          density={density}
        />
      )}
      {view === "years" && (
        <YearsView anchor={anchor}
          selectedYear={start ? start.getFullYear() : null}
          onPick={(y) => {
            setAnchor(new Date(y, anchor.getMonth(), 1));
            setView("months");
          }}
          density={density}
        />
      )}
    </div>
  );
};

Object.assign(window, {
  TODAY, DAY_MS, startOfDay, sameDay, inRange, addMonths,
  monthName, monthShort, fmtDate, fmtShort,
  buildMonthGrid, WEEKDAYS, PRESETS,
  MonthGrid, MonthHeader, MonthsView, YearsView, CalendarView,
});
