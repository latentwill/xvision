// Ported from docs/design/calendar-picker/calendar-core.jsx. All Date
// arithmetic is UTC — never local-zone — because the scenario form
// serializes `${from}T00:00:00Z` and a local-zone drift here would
// shift the persisted window by ±1 day depending on the operator's
// browser locale.

import { useState } from 'react';

export const DAY_MS = 86_400_000;

export type DateRangeView = 'days' | 'months' | 'years';
export type Density = 'comfort' | 'compact' | 'mobile';

// `today()` is read on render; tests that need a fixed clock should mock
// at the component boundary rather than reaching in here.
export function today(): Date {
  const now = new Date();
  return utcDate(now.getUTCFullYear(), now.getUTCMonth(), now.getUTCDate());
}

export function utcDate(year: number, month: number, day: number): Date {
  return new Date(Date.UTC(year, month, day));
}

export function startOfDay(d: Date): Date {
  return utcDate(d.getUTCFullYear(), d.getUTCMonth(), d.getUTCDate());
}

export function sameDay(a: Date | null, b: Date | null): boolean {
  if (!a || !b) return false;
  return (
    a.getUTCFullYear() === b.getUTCFullYear() &&
    a.getUTCMonth() === b.getUTCMonth() &&
    a.getUTCDate() === b.getUTCDate()
  );
}

export function inRange(d: Date, a: Date | null, b: Date | null): boolean {
  if (!a || !b) return false;
  const t = startOfDay(d).getTime();
  const lo = Math.min(startOfDay(a).getTime(), startOfDay(b).getTime());
  const hi = Math.max(startOfDay(a).getTime(), startOfDay(b).getTime());
  return t >= lo && t <= hi;
}

export function addMonths(d: Date, n: number): Date {
  return utcDate(d.getUTCFullYear(), d.getUTCMonth() + n, 1);
}

const MONTH_LONG = [
  'January', 'February', 'March', 'April', 'May', 'June',
  'July', 'August', 'September', 'October', 'November', 'December',
];
const MONTH_SHORT = [
  'Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun',
  'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec',
];

export function monthName(d: Date): string {
  return MONTH_LONG[d.getUTCMonth()];
}

export function monthShort(d: Date): string {
  return MONTH_SHORT[d.getUTCMonth()];
}

// `Apr 18, 2026` style — UTC accessors so a UTC-midnight date doesn't
// flip to the previous day in negative timezones.
export function fmtDate(d: Date | null): string {
  if (!d) return '';
  return `${MONTH_SHORT[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`;
}

export function fmtShort(d: Date | null): string {
  if (!d) return '';
  const mm = String(d.getUTCMonth() + 1).padStart(2, '0');
  const dd = String(d.getUTCDate()).padStart(2, '0');
  const yy = String(d.getUTCFullYear()).slice(2);
  return `${mm}/${dd}/${yy}`;
}

// Bridge to the scenario form's existing `YYYY-MM-DD` state.
export function toIsoDate(d: Date | null): string {
  if (!d) return '';
  const m = String(d.getUTCMonth() + 1).padStart(2, '0');
  const day = String(d.getUTCDate()).padStart(2, '0');
  return `${d.getUTCFullYear()}-${m}-${day}`;
}

export function fromIsoDate(s: string): Date | null {
  if (!s) return null;
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(s);
  if (!m) return null;
  return utcDate(Number(m[1]), Number(m[2]) - 1, Number(m[3]));
}

// 6×7 grid of UTC dates for the month containing `anchor`, Monday-start.
export function buildMonthGrid(anchor: Date): Date[] {
  const firstDay = utcDate(anchor.getUTCFullYear(), anchor.getUTCMonth(), 1);
  // getUTCDay: 0 = Sunday … 6 = Saturday. Convert to Monday-start (0 = Mon).
  const offset = (firstDay.getUTCDay() + 6) % 7;
  const gridStart = utcDate(
    firstDay.getUTCFullYear(),
    firstDay.getUTCMonth(),
    1 - offset,
  );
  const cells: Date[] = [];
  for (let i = 0; i < 42; i++) {
    cells.push(
      utcDate(
        gridStart.getUTCFullYear(),
        gridStart.getUTCMonth(),
        gridStart.getUTCDate() + i,
      ),
    );
  }
  return cells;
}

export const WEEKDAYS = ['M', 'T', 'W', 'T', 'F', 'S', 'S'];

// Presets are computed relative to the live `today()` so they stay
// truthful when the operator opens the form weeks after a deploy.
export interface PresetDef {
  id: string;
  label: string;
  range: () => [Date, Date];
}

export function presets(): PresetDef[] {
  const t = today();
  return [
    {
      id: '7d',
      label: 'Last 7 days',
      range: () => [new Date(t.getTime() - 6 * DAY_MS), t],
    },
    {
      id: '30d',
      label: 'Last 30 days',
      range: () => [new Date(t.getTime() - 29 * DAY_MS), t],
    },
    {
      id: '90d',
      label: 'Last 90 days',
      range: () => [new Date(t.getTime() - 89 * DAY_MS), t],
    },
    {
      id: 'ytd',
      label: 'Year to date',
      range: () => [utcDate(t.getUTCFullYear(), 0, 1), t],
    },
    {
      id: '12m',
      label: 'Trailing 12 months',
      range: () => [
        utcDate(t.getUTCFullYear() - 1, t.getUTCMonth(), t.getUTCDate()),
        t,
      ],
    },
  ];
}

// ─── MonthGrid ────────────────────────────────────────────────────────

interface MonthGridProps {
  anchor: Date;
  start: Date | null;
  end: Date | null;
  hover: Date | null;
  onPick: (d: Date) => void;
  onHover?: (d: Date) => void;
  density?: Density;
  showWeekHeader?: boolean;
}

const DIMS = {
  comfort: { cell: 36, font: 13, gap: 0, weekFs: 10.5, weekHeight: 28 },
  compact: { cell: 30, font: 12, gap: 0, weekFs: 10, weekHeight: 24 },
  mobile: { cell: 44, font: 16, gap: 2, weekFs: 11, weekHeight: 32 },
} as const;

export function MonthGrid({
  anchor,
  start,
  end,
  hover,
  onPick,
  onHover,
  density = 'comfort',
  showWeekHeader = true,
}: MonthGridProps) {
  const cells = buildMonthGrid(anchor);
  const curMonth = anchor.getUTCMonth();
  const dims = DIMS[density];
  const todayUtc = today();

  // Resolve effective end for in-range computation (hover preview).
  const effEnd =
    end ||
    (start && hover && hover.getTime() !== start.getTime() ? hover : null);

  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(7, 1fr)',
        rowGap: dims.gap,
      }}
    >
      {showWeekHeader &&
        WEEKDAYS.map((w, i) => (
          <div
            key={`wh-${i}`}
            style={{
              height: dims.weekHeight,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              fontSize: dims.weekFs,
              color: 'var(--text-3)',
              letterSpacing: '0.12em',
              textTransform: 'uppercase',
              fontFamily: 'Geist, sans-serif',
            }}
          >
            {w}
          </div>
        ))}
      {cells.map((d, i) => {
        const isCur = d.getUTCMonth() === curMonth;
        const isToday = sameDay(d, todayUtc);
        const isStart = sameDay(d, start);
        const isEnd =
          sameDay(d, end) ||
          (!end &&
            start &&
            hover &&
            sameDay(d, hover) &&
            hover.getTime() !== start.getTime());
        const isEdge = isStart || isEnd;

        // Range-bar connector geometry.
        const lo =
          start && effEnd
            ? Math.min(
                startOfDay(start).getTime(),
                startOfDay(effEnd).getTime(),
              )
            : null;
        const hi =
          start && effEnd
            ? Math.max(
                startOfDay(start).getTime(),
                startOfDay(effEnd).getTime(),
              )
            : null;
        const t = startOfDay(d).getTime();
        const hasLeft = lo !== null && hi !== null && t > lo && t <= hi;
        const hasRight = lo !== null && hi !== null && t >= lo && t < hi;

        const bar =
          hasLeft || hasRight ? (
            <div
              style={{
                position: 'absolute',
                top: 4,
                bottom: 4,
                left: hasLeft ? 0 : '50%',
                right: hasRight ? 0 : '50%',
                background: 'var(--gold-bg)',
                pointerEvents: 'none',
              }}
            />
          ) : null;

        return (
          <button
            key={i}
            type="button"
            onClick={() => onPick(d)}
            onMouseEnter={() => onHover?.(d)}
            aria-label={`${MONTH_SHORT[d.getUTCMonth()]} ${d.getUTCDate()}, ${d.getUTCFullYear()}`}
            aria-pressed={isEdge ? true : undefined}
            data-iso={toIsoDate(d)}
            data-in-range={hasLeft || hasRight ? 'true' : undefined}
            data-edge={isEdge ? 'true' : undefined}
            style={{
              position: 'relative',
              height: dims.cell,
              border: 'none',
              background: 'transparent',
              padding: 0,
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              opacity: isCur ? 1 : 0.55,
            }}
          >
            {bar}
            <div
              style={{
                position: 'relative',
                zIndex: 1,
                width: dims.cell - 4,
                height: dims.cell - 4,
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                borderRadius: '50%',
                fontFamily: 'Geist Mono, ui-monospace, monospace',
                fontVariantNumeric: 'tabular-nums',
                fontSize: dims.font,
                fontWeight: isEdge ? 600 : 400,
                color: isEdge
                  ? '#000000'
                  : !isCur
                    ? 'var(--text-4)'
                    : isToday
                      ? 'var(--gold)'
                      : 'var(--text)',
                background: isEdge ? 'var(--gold)' : 'transparent',
                border:
                  isToday && !isEdge
                    ? '1px solid rgba(0,230,118,0.55)'
                    : '1px solid transparent',
                transition: 'background .12s, color .12s',
              }}
            >
              {d.getUTCDate()}
            </div>
          </button>
        );
      })}
    </div>
  );
}

// ─── MonthHeader ──────────────────────────────────────────────────────

interface MonthHeaderProps {
  anchor: Date;
  view?: DateRangeView;
  onPrev: () => void;
  onNext: () => void;
  onMonthClick?: () => void;
  onYearClick?: () => void;
  size?: 'sm' | 'md' | 'lg';
  canPrev?: boolean;
  canNext?: boolean;
}

const HEADER_SIZES = {
  sm: { fs: 16, btn: 24, iconSize: 13, gap: 6 },
  md: { fs: 20, btn: 28, iconSize: 14, gap: 6 },
  lg: { fs: 26, btn: 32, iconSize: 16, gap: 8 },
} as const;

export function MonthHeader({
  anchor,
  view = 'days',
  onPrev,
  onNext,
  onMonthClick,
  onYearClick,
  size = 'md',
  canPrev = true,
  canNext = true,
}: MonthHeaderProps) {
  const sizes = HEADER_SIZES[size];

  const labelBtn = (
    text: string | number,
    italic: boolean,
    onClick: (() => void) | undefined,
    active: boolean,
  ) => (
    <button
      type="button"
      onClick={onClick}
      data-active={active ? 'true' : 'false'}
      style={{
        background: active ? 'var(--gold-bg)' : 'transparent',
        border: 'none',
        borderRadius: 3,
        padding: '2px 5px 2px 6px',
        cursor: 'pointer',
        fontFamily: 'Geist, sans-serif',
        fontWeight: italic ? 600 : 500,
        fontSize: sizes.fs,
        letterSpacing: '-0.01em',
        color: active ? 'var(--gold)' : 'var(--text)',
        transition: 'background .12s, color .12s',
        display: 'inline-flex',
        alignItems: 'baseline',
        gap: 3,
      }}
    >
      <span
        style={{
          borderBottom: active
            ? '1px solid transparent'
            : '1px dotted var(--text-4)',
          paddingBottom: 1,
        }}
      >
        {text}
      </span>
      <svg
        width={Math.round(sizes.fs * 0.42)}
        height={Math.round(sizes.fs * 0.42)}
        viewBox="0 0 20 20"
        fill="none"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
        strokeLinejoin="round"
        style={{
          opacity: active ? 1 : 0.55,
          transform: 'translateY(2px)',
          flexShrink: 0,
        }}
      >
        <path d="M5 8l5 5 5-5" />
      </svg>
    </button>
  );

  let titleEl: React.ReactNode;
  if (view === 'years') {
    const startYear = Math.floor(anchor.getUTCFullYear() / 12) * 12;
    titleEl = (
      <div
        style={{
          fontFamily: 'Geist, sans-serif',
          fontWeight: 500,
          fontSize: sizes.fs,
          letterSpacing: '-0.01em',
          color: 'var(--text)',
          padding: '2px 6px',
        }}
      >
        {startYear} <span style={{ color: 'var(--text-3)' }}>—</span>{' '}
        <span style={{ color: 'var(--text-3)', fontWeight: 600 }}>
          {startYear + 11}
        </span>
      </div>
    );
  } else if (view === 'months') {
    titleEl = (
      <div style={{ display: 'flex', alignItems: 'center', gap: 2 }}>
        <span
          style={{
            fontFamily: 'Geist, sans-serif',
            fontWeight: 500,
            fontSize: sizes.fs,
            color: 'var(--text-3)',
            padding: '2px 6px',
          }}
        >
          Pick a month in{' '}
        </span>
        {labelBtn(anchor.getUTCFullYear(), true, onYearClick, false)}
      </div>
    );
  } else {
    titleEl = (
      <div style={{ display: 'flex', alignItems: 'center', gap: 2 }}>
        {labelBtn(monthName(anchor), false, onMonthClick, false)}
        {labelBtn(anchor.getUTCFullYear(), true, onYearClick, false)}
      </div>
    );
  }

  return (
    <div
      style={{
        display: 'flex',
        alignItems: 'center',
        justifyContent: 'space-between',
        padding: '2px 4px 14px',
      }}
    >
      <button
        type="button"
        onClick={onPrev}
        disabled={!canPrev}
        aria-label="Previous"
        style={{
          width: sizes.btn,
          height: sizes.btn,
          border: '1px solid var(--border)',
          background: 'transparent',
          borderRadius: 4,
          cursor: canPrev ? 'pointer' : 'default',
          color: canPrev ? 'var(--text-2)' : 'var(--text-4)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          opacity: canPrev ? 1 : 0.4,
        }}
      >
        <svg
          width={sizes.iconSize}
          height={sizes.iconSize}
          viewBox="0 0 20 20"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M12 5l-5 5 5 5" />
        </svg>
      </button>
      {titleEl}
      <button
        type="button"
        onClick={onNext}
        disabled={!canNext}
        aria-label="Next"
        style={{
          width: sizes.btn,
          height: sizes.btn,
          border: '1px solid var(--border)',
          background: 'transparent',
          borderRadius: 4,
          cursor: canNext ? 'pointer' : 'default',
          color: canNext ? 'var(--text-2)' : 'var(--text-4)',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          opacity: canNext ? 1 : 0.4,
        }}
      >
        <svg
          width={sizes.iconSize}
          height={sizes.iconSize}
          viewBox="0 0 20 20"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M8 5l5 5-5 5" />
        </svg>
      </button>
    </div>
  );
}

// ─── MonthsView ───────────────────────────────────────────────────────

interface MonthsViewProps {
  anchor: Date;
  selectedYear: number | null;
  selectedMonth: number | null;
  onPick: (month: number) => void;
  density?: Density;
}

export function MonthsView({
  anchor,
  selectedYear,
  selectedMonth,
  onPick,
  density = 'comfort',
}: MonthsViewProps) {
  const months = MONTH_SHORT;
  const rowH = density === 'mobile' ? 56 : 48;
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(3, 1fr)',
        gap: 6,
        paddingTop: 4,
      }}
    >
      {months.map((m, i) => {
        const isCurrent = i === anchor.getUTCMonth();
        const isSelected =
          selectedYear === anchor.getUTCFullYear() && selectedMonth === i;
        return (
          <button
            key={m}
            type="button"
            onClick={() => onPick(i)}
            data-selected={isSelected ? 'true' : undefined}
            style={{
              height: rowH,
              border:
                '1px solid ' +
                (isSelected ? 'rgba(0,230,118,0.5)' : 'var(--border)'),
              background: isSelected
                ? 'var(--gold)'
                : isCurrent
                  ? 'var(--gold-bg)'
                  : 'transparent',
              color: isSelected
                ? '#000000'
                : isCurrent
                  ? 'var(--gold)'
                  : 'var(--text-2)',
              borderRadius: 4,
              fontFamily: 'Geist, sans-serif',
              fontSize: density === 'mobile' ? 14 : 13,
              fontWeight: isSelected ? 600 : 500,
              cursor: 'pointer',
              transition:
                'background .12s, color .12s, border-color .12s',
            }}
          >
            {m}
          </button>
        );
      })}
    </div>
  );
}

// ─── YearsView ────────────────────────────────────────────────────────

interface YearsViewProps {
  anchor: Date;
  selectedYear: number | null;
  onPick: (year: number) => void;
  density?: Density;
}

export function YearsView({
  anchor,
  selectedYear,
  onPick,
  density = 'comfort',
}: YearsViewProps) {
  const startYear = Math.floor(anchor.getUTCFullYear() / 12) * 12;
  const rowH = density === 'mobile' ? 56 : 48;
  const cells = Array.from({ length: 12 }, (_, i) => startYear + i);
  const todayYear = today().getUTCFullYear();
  return (
    <div
      style={{
        display: 'grid',
        gridTemplateColumns: 'repeat(3, 1fr)',
        gap: 6,
        paddingTop: 4,
      }}
    >
      {cells.map((y) => {
        const isCurrent = y === anchor.getUTCFullYear();
        const isSelected = y === selectedYear;
        const isToday = y === todayYear;
        return (
          <button
            key={y}
            type="button"
            onClick={() => onPick(y)}
            data-selected={isSelected ? 'true' : undefined}
            style={{
              height: rowH,
              border:
                '1px solid ' +
                (isSelected
                  ? 'rgba(0,230,118,0.5)'
                  : isToday && !isSelected
                    ? 'rgba(0,230,118,0.35)'
                    : 'var(--border)'),
              background: isSelected
                ? 'var(--gold)'
                : isCurrent
                  ? 'var(--gold-bg)'
                  : 'transparent',
              color: isSelected
                ? '#000000'
                : isCurrent || isToday
                  ? 'var(--gold)'
                  : 'var(--text-2)',
              borderRadius: 4,
              fontFamily: 'Geist Mono, ui-monospace, monospace',
              fontVariantNumeric: 'tabular-nums',
              fontSize: density === 'mobile' ? 14 : 13,
              fontWeight: isSelected ? 600 : 500,
              cursor: 'pointer',
              transition:
                'background .12s, color .12s, border-color .12s',
            }}
          >
            {y}
          </button>
        );
      })}
    </div>
  );
}

// ─── CalendarView ─────────────────────────────────────────────────────

interface CalendarViewProps {
  anchor: Date;
  setAnchor: (d: Date) => void;
  start: Date | null;
  end: Date | null;
  hover: Date | null;
  onPick: (d: Date) => void;
  onHover?: (d: Date) => void;
  density?: Density;
  monthHeaderSize?: 'sm' | 'md' | 'lg';
  initialView?: DateRangeView;
}

export function CalendarView({
  anchor,
  setAnchor,
  start,
  end,
  hover,
  onPick,
  onHover,
  density = 'comfort',
  monthHeaderSize = 'md',
  initialView = 'days',
}: CalendarViewProps) {
  const [view, setView] = useState<DateRangeView>(initialView);

  const step = (dir: number) => {
    if (view === 'days') {
      setAnchor(addMonths(anchor, dir));
    } else if (view === 'months') {
      setAnchor(
        utcDate(anchor.getUTCFullYear() + dir, anchor.getUTCMonth(), 1),
      );
    } else {
      setAnchor(
        utcDate(anchor.getUTCFullYear() + dir * 12, anchor.getUTCMonth(), 1),
      );
    }
  };

  return (
    <div>
      <MonthHeader
        anchor={anchor}
        view={view}
        onPrev={() => step(-1)}
        onNext={() => step(+1)}
        onMonthClick={() =>
          setView(view === 'months' ? 'days' : 'months')
        }
        onYearClick={() => setView(view === 'years' ? 'days' : 'years')}
        size={monthHeaderSize}
      />
      {view === 'days' && (
        <MonthGrid
          anchor={anchor}
          start={start}
          end={end}
          hover={hover}
          onPick={onPick}
          onHover={onHover}
          density={density}
        />
      )}
      {view === 'months' && (
        <MonthsView
          anchor={anchor}
          selectedYear={start ? start.getUTCFullYear() : null}
          selectedMonth={start ? start.getUTCMonth() : null}
          onPick={(m) => {
            setAnchor(utcDate(anchor.getUTCFullYear(), m, 1));
            setView('days');
          }}
          density={density}
        />
      )}
      {view === 'years' && (
        <YearsView
          anchor={anchor}
          selectedYear={start ? start.getUTCFullYear() : null}
          onPick={(y) => {
            setAnchor(utcDate(y, anchor.getUTCMonth(), 1));
            setView('months');
          }}
          density={density}
        />
      )}
    </div>
  );
}
