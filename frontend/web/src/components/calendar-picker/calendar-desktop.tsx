// Ported from docs/design/calendar-picker/calendar-desktop.jsx. Only
// `InlineRangeBar` (the recommended pattern per the package README) and
// the internal `DateField` are ported. `DualMonthRangePopover` is NOT
// ported — popover ≠ inline; explicitly excluded by the contract.

import { useState } from 'react';
import {
  DAY_MS,
  MonthGrid,
  MonthHeader,
  MonthsView,
  YearsView,
  addMonths,
  fmtDate,
  fromIsoDate,
  presets,
  startOfDay,
  today,
  toIsoDate,
  utcDate,
  type DateRangeView,
  type PresetDef,
} from './calendar-core';

interface DateFieldProps {
  label: string;
  value: Date | null;
  active: boolean;
  onClick: () => void;
}

function DateField({ label, value, active, onClick }: DateFieldProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        flex: 1,
        background: 'var(--surface-card)',
        border: active
          ? '1px solid var(--gold-soft)'
          : '1px solid var(--border)',
        borderRadius: 4,
        padding: '7px 12px 8px',
        textAlign: 'left',
        fontFamily: 'inherit',
        cursor: 'pointer',
        minWidth: 130,
      }}
    >
      <div
        style={{
          fontSize: 10,
          letterSpacing: '0.12em',
          textTransform: 'uppercase',
          color: active ? 'var(--gold)' : 'var(--text-3)',
          marginBottom: 2,
        }}
      >
        {label}
      </div>
      <div
        style={{
          fontFamily: "'JetBrains Mono', monospace",
          fontSize: 13,
          color: value ? 'var(--text)' : 'var(--text-3)',
        }}
      >
        {value ? fmtDate(value) : '—'}
      </div>
    </button>
  );
}

export interface InlineRangeBarProps {
  /** ISO date string (YYYY-MM-DD), UTC date semantics. */
  startIso: string;
  endIso: string;
  onChange: (next: { startIso: string; endIso: string }) => void;
  /** Optional label rendered in the closed header (default: "Backtest window"). */
  label?: string;
  /** Width of the rendered bar. Defaults to 100% of its parent so the
   *  form can constrain it via column layout. The design package's
   *  reference size is 720; pass that explicitly for the wide variant. */
  width?: number | string;
  /** Whether the body is open on first render. */
  defaultOpen?: boolean;
}

/**
 * Inline date-range picker. In-flow disclosure pattern (no overlay).
 * Click the header to expand the dual-month grid; the page below
 * shifts down rather than being covered. Apply / Cancel are explicit;
 * no escape-to-close, no backdrop.
 */
export function InlineRangeBar({
  startIso,
  endIso,
  onChange,
  label = 'Backtest window',
  width = '100%',
  defaultOpen = false,
}: InlineRangeBarProps) {
  const [open, setOpen] = useState(defaultOpen);
  // Working state — only committed back to the parent on "Apply range".
  const [start, setStart] = useState<Date | null>(() => fromIsoDate(startIso));
  const [end, setEnd] = useState<Date | null>(() => fromIsoDate(endIso));
  const [anchor, setAnchor] = useState<Date>(
    () => fromIsoDate(startIso) ?? today(),
  );
  const [hover, setHover] = useState<Date | null>(null);
  const [picking, setPicking] = useState<'start' | 'end'>('end');
  const [activePreset, setActivePreset] = useState<string | null>(null);
  const [view, setView] = useState<DateRangeView>('days');

  // If the parent commits a new value (e.g. via a regime-preset chip
  // outside the bar), reflect it on next open.
  function syncFromParent() {
    setStart(fromIsoDate(startIso));
    setEnd(fromIsoDate(endIso));
    setAnchor(fromIsoDate(startIso) ?? today());
  }

  const handleToggle = () => {
    syncFromParent();
    setOpen(!open);
  };

  const handlePick = (d: Date) => {
    if (picking === 'start' || (start && d.getTime() < start.getTime())) {
      setStart(d);
      setEnd(null);
      setPicking('end');
      setActivePreset(null);
    } else {
      setEnd(d);
      setPicking('start');
      setActivePreset(null);
    }
  };

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

  const right = addMonths(anchor, 1);
  const dayCount =
    start && end
      ? Math.round(
          (startOfDay(end).getTime() - startOfDay(start).getTime()) / DAY_MS,
        ) + 1
      : null;

  const presetList: PresetDef[] = presets();

  const apply = () => {
    if (start && end) {
      onChange({ startIso: toIsoDate(start), endIso: toIsoDate(end) });
    }
    setOpen(false);
  };

  const cancel = () => {
    syncFromParent();
    setOpen(false);
  };

  return (
    <div
      data-testid="inline-range-bar"
      data-open={open ? 'true' : 'false'}
      style={{
        width,
        background: 'var(--surface-elev)',
        border:
          '1px solid ' + (open ? 'var(--gold-soft)' : 'var(--border)'),
        borderRadius: 6,
        fontFamily: 'Inter, sans-serif',
        overflow: 'hidden',
        transition: 'border-color .15s',
      }}
    >
      <button
        type="button"
        onClick={handleToggle}
        aria-expanded={open}
        style={{
          width: '100%',
          display: 'flex',
          alignItems: 'center',
          gap: 14,
          padding: '10px 14px',
          background: 'transparent',
          border: 'none',
          borderBottom: open
            ? '1px solid var(--border-soft)'
            : '1px solid transparent',
          color: 'var(--text)',
          fontFamily: 'inherit',
          fontSize: 13,
          cursor: 'pointer',
          textAlign: 'left',
        }}
      >
        <svg
          width="15"
          height="15"
          viewBox="0 0 20 20"
          fill="none"
          stroke={open ? 'var(--gold)' : 'var(--text-3)'}
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <rect x="3" y="5" width="14" height="12" rx="1.5" />
          <path d="M7 3v4M13 3v4M3 9h14" />
        </svg>
        <div
          style={{
            fontSize: 10.5,
            letterSpacing: '0.14em',
            textTransform: 'uppercase',
            color: open ? 'var(--gold)' : 'var(--text-3)',
          }}
        >
          {label}
        </div>
        <div
          style={{
            fontFamily: "'JetBrains Mono', monospace",
            fontSize: 13,
            color: 'var(--text)',
            letterSpacing: '0.01em',
          }}
        >
          {start ? fmtDate(start) : '—'}
          <span style={{ color: 'var(--text-3)', margin: '0 6px' }}>→</span>
          {end ? fmtDate(end) : '—'}
        </div>
        {dayCount !== null && (
          <div
            style={{
              fontSize: 11,
              color: 'var(--gold)',
              fontFamily: "'JetBrains Mono', monospace",
              padding: '2px 7px',
              background: 'var(--gold-bg)',
              border: '1px solid rgba(212,165,71,0.3)',
              borderRadius: 3,
              letterSpacing: '0.02em',
            }}
          >
            {dayCount} days
          </div>
        )}
        <div style={{ flex: 1 }} />
        {activePreset && (
          <div
            style={{
              fontSize: 11,
              color: 'var(--text-3)',
              fontWeight: 500,
            }}
          >
            from preset ·{' '}
            {presetList.find((p) => p.id === activePreset)?.label}
          </div>
        )}
        <svg
          width="13"
          height="13"
          viewBox="0 0 20 20"
          fill="none"
          stroke="var(--text-3)"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          style={{
            transform: open ? 'rotate(180deg)' : 'none',
            transition: 'transform .18s',
          }}
        >
          <path d="M5 8l5 5 5-5" />
        </svg>
      </button>

      {/* Swing-out body */}
      <div
        style={{
          maxHeight: open ? 600 : 0,
          overflow: 'hidden',
          transition: 'max-height .28s cubic-bezier(.2,.7,.3,1)',
        }}
      >
        <div
          style={{
            display: 'grid',
            gridTemplateColumns: 'minmax(140px, 176px) 1fr',
          }}
        >
          {/* Preset rail */}
          <div
            style={{
              background: 'var(--surface-card)',
              borderRight: '1px solid var(--border-soft)',
              padding: '12px 0 16px',
              display: 'flex',
              flexDirection: 'column',
            }}
          >
            <div
              style={{
                fontSize: 10,
                letterSpacing: '0.14em',
                textTransform: 'uppercase',
                color: 'var(--text-3)',
                padding: '0 16px 8px',
              }}
            >
              Quick ranges
            </div>
            {presetList.map((p) => {
              const active = activePreset === p.id;
              return (
                <button
                  key={p.id}
                  type="button"
                  onClick={() => {
                    const [s, e] = p.range();
                    setStart(s);
                    setEnd(e);
                    setAnchor(utcDate(s.getUTCFullYear(), s.getUTCMonth(), 1));
                    setActivePreset(p.id);
                    setPicking('start');
                    setView('days');
                  }}
                  style={{
                    textAlign: 'left',
                    padding: '6px 16px',
                    background: active ? 'var(--gold-bg)' : 'transparent',
                    border: 'none',
                    borderLeft: active
                      ? '2px solid var(--gold)'
                      : '2px solid transparent',
                    color: active ? 'var(--text)' : 'var(--text-2)',
                    fontFamily: 'inherit',
                    fontSize: 12.5,
                    cursor: 'pointer',
                    fontWeight: active ? 500 : 400,
                  }}
                >
                  {p.label}
                </button>
              );
            })}
          </div>

          {/* Calendar body */}
          <div style={{ padding: '14px 18px 14px' }}>
            {/* Start/End fields */}
            <div
              style={{
                display: 'flex',
                gap: 8,
                alignItems: 'stretch',
                marginBottom: 14,
              }}
            >
              <DateField
                label="Start"
                value={start}
                active={picking === 'start'}
                onClick={() => setPicking('start')}
              />
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  color: 'var(--text-3)',
                }}
              >
                <svg
                  width="14"
                  height="14"
                  viewBox="0 0 20 20"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.5"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="M4 10h12M12 6l4 4-4 4" />
                </svg>
              </div>
              <DateField
                label="End"
                value={end}
                active={picking === 'end'}
                onClick={() => setPicking('end')}
              />
            </div>

            {view === 'days' ? (
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: '1fr 1fr',
                  gap: 20,
                }}
              >
                <div>
                  <MonthHeader
                    anchor={anchor}
                    view="days"
                    onPrev={() => step(-1)}
                    onNext={() => step(+1)}
                    onMonthClick={() => setView('months')}
                    onYearClick={() => setView('years')}
                    canNext={false}
                    size="md"
                  />
                  <MonthGrid
                    anchor={anchor}
                    start={start}
                    end={end}
                    hover={hover}
                    onPick={handlePick}
                    onHover={setHover}
                    density="compact"
                  />
                </div>
                <div>
                  <MonthHeader
                    anchor={right}
                    view="days"
                    onPrev={() => step(-1)}
                    onNext={() => step(+1)}
                    onMonthClick={() => setView('months')}
                    onYearClick={() => setView('years')}
                    canPrev={false}
                    size="md"
                  />
                  <MonthGrid
                    anchor={right}
                    start={start}
                    end={end}
                    hover={hover}
                    onPick={handlePick}
                    onHover={setHover}
                    density="compact"
                  />
                </div>
              </div>
            ) : (
              <div>
                <MonthHeader
                  anchor={anchor}
                  view={view}
                  onPrev={() => step(-1)}
                  onNext={() => step(+1)}
                  onMonthClick={() =>
                    setView(view === 'months' ? 'days' : 'months')
                  }
                  onYearClick={() =>
                    setView(view === 'years' ? 'days' : 'years')
                  }
                  size="md"
                />
                <div style={{ maxWidth: 460, margin: '0 auto' }}>
                  {view === 'months' && (
                    <MonthsView
                      anchor={anchor}
                      selectedYear={start ? start.getUTCFullYear() : null}
                      selectedMonth={start ? start.getUTCMonth() : null}
                      onPick={(m) => {
                        setAnchor(
                          utcDate(anchor.getUTCFullYear(), m, 1),
                        );
                        setView('days');
                      }}
                      density="comfort"
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
                      density="comfort"
                    />
                  )}
                </div>
              </div>
            )}

            {/* Footer */}
            <div
              style={{
                marginTop: 12,
                paddingTop: 12,
                borderTop: '1px solid var(--border-soft)',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
              }}
            >
              <div style={{ fontSize: 11, color: 'var(--text-3)' }}>
                Tip: click the{' '}
                <span style={{ color: 'var(--text-2)' }}>month</span> or{' '}
                <span
                  style={{
                    color: 'var(--text-2)',
                    fontWeight: 600,
                  }}
                >
                  year
                </span>{' '}
                to jump.
              </div>
              <div style={{ display: 'flex', gap: 8 }}>
                <button
                  type="button"
                  onClick={cancel}
                  style={{
                    padding: '5px 12px',
                    fontSize: 12,
                    background: 'transparent',
                    border: '1px solid var(--border)',
                    borderRadius: 4,
                    color: 'var(--text-2)',
                    cursor: 'pointer',
                  }}
                >
                  Cancel
                </button>
                <button
                  type="button"
                  onClick={apply}
                  disabled={!start || !end}
                  style={{
                    padding: '5px 12px',
                    fontSize: 12,
                    background: 'var(--gold)',
                    border: '1px solid var(--gold-soft)',
                    borderRadius: 4,
                    color: '#0F0E0C',
                    cursor: start && end ? 'pointer' : 'default',
                    opacity: start && end ? 1 : 0.5,
                  }}
                >
                  Apply range
                </button>
              </div>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
