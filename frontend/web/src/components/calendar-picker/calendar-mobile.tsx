// Ported from docs/design/calendar-picker/calendar-mobile.jsx. Only
// the calendar-card piece of `MobileInlineCard` — the page-chrome wrap
// (`PhoneFrame`, the route header, the Run-backtest button) is NOT
// ported per the contract's component-only scope. The bottom-sheet
// variant (`MobileBottomSheet`) is also NOT ported — overlay/popup,
// excluded by the no-popup rule.

import { useEffect, useState } from 'react';
import {
  DAY_MS,
  CalendarView,
  fmtDate,
  fromIsoDate,
  presets,
  startOfDay,
  today,
  toIsoDate,
  utcDate,
  type PresetDef,
} from './calendar-core';

export interface MobileInlineCardProps {
  startIso: string;
  endIso: string;
  onChange: (next: { startIso: string; endIso: string }) => void;
  label?: string;
}

/**
 * Compact mobile range-picker card. Uses the same `CalendarView` core
 * as the desktop bar at `density="mobile"`. Sits inline in the form —
 * no bottom sheet, no overlay. Apply is implicit: every commit on the
 * second click of a range emits via `onChange`.
 */
export function MobileInlineCard({
  startIso,
  endIso,
  onChange,
  label = 'Backtest window',
}: MobileInlineCardProps) {
  const [start, setStart] = useState<Date | null>(() => fromIsoDate(startIso));
  const [end, setEnd] = useState<Date | null>(() => fromIsoDate(endIso));
  const [anchor, setAnchor] = useState<Date>(
    () => fromIsoDate(startIso) ?? today(),
  );
  const [hover, setHover] = useState<Date | null>(null);
  const [activePreset, setActivePreset] = useState<string | null>(null);

  useEffect(() => {
    const nextStart = fromIsoDate(startIso);
    const nextEnd = fromIsoDate(endIso);
    setStart(nextStart);
    setEnd(nextEnd);
    setAnchor(nextStart ?? today());
    setHover(null);
    setActivePreset(null);
  }, [startIso, endIso]);

  const handlePick = (d: Date) => {
    if (!start || (start && end)) {
      setStart(d);
      setEnd(null);
      setActivePreset(null);
      return;
    }
    if (d.getTime() < start.getTime()) {
      setStart(d);
      setActivePreset(null);
      return;
    }
    setEnd(d);
    setActivePreset(null);
    onChange({ startIso: toIsoDate(start), endIso: toIsoDate(d) });
  };

  const dayCount =
    start && end
      ? Math.round(
          (startOfDay(end).getTime() - startOfDay(start).getTime()) / DAY_MS,
        ) + 1
      : null;

  const presetList: PresetDef[] = presets();

  return (
    <div
      data-testid="mobile-inline-card"
      style={{
        background: 'var(--surface-card)',
        border: '1px solid var(--border)',
        borderRadius: 6,
        fontFamily: 'Inter, sans-serif',
        padding: '12px 14px',
      }}
    >
      <div
        style={{
          fontSize: 10,
          letterSpacing: '0.14em',
          textTransform: 'uppercase',
          color: 'var(--text-3)',
          marginBottom: 8,
        }}
      >
        {label}
      </div>

      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          marginBottom: 10,
        }}
      >
        <div>
          <div
            style={{
              fontSize: 9.5,
              letterSpacing: '0.14em',
              textTransform: 'uppercase',
              color: 'var(--text-3)',
            }}
          >
            From
          </div>
          <div
            style={{
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 13,
              color: 'var(--text)',
              marginTop: 2,
            }}
          >
            {start ? fmtDate(start) : '—'}
          </div>
        </div>
        <svg
          width="16"
          height="16"
          viewBox="0 0 20 20"
          fill="none"
          stroke="var(--text-3)"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        >
          <path d="M4 10h12M12 6l4 4-4 4" />
        </svg>
        <div style={{ textAlign: 'right' }}>
          <div
            style={{
              fontSize: 9.5,
              letterSpacing: '0.14em',
              textTransform: 'uppercase',
              color: 'var(--text-3)',
            }}
          >
            To
          </div>
          <div
            style={{
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 13,
              color: 'var(--text)',
              marginTop: 2,
            }}
          >
            {end ? fmtDate(end) : '—'}
          </div>
        </div>
      </div>

      {dayCount !== null && (
        <div
          style={{
            marginBottom: 10,
            paddingTop: 8,
            borderTop: '1px solid var(--border-soft)',
            display: 'flex',
            justifyContent: 'space-between',
            alignItems: 'center',
          }}
        >
          <span style={{ fontSize: 11, color: 'var(--text-3)' }}>
            Trading days
          </span>
          <span
            style={{
              fontFamily: "'JetBrains Mono', monospace",
              fontSize: 12,
              color: 'var(--gold)',
            }}
          >
            {dayCount}
          </span>
        </div>
      )}

      <div
        style={{
          display: 'flex',
          gap: 6,
          overflowX: 'auto',
          padding: '4px 0 8px',
        }}
      >
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
                onChange({ startIso: toIsoDate(s), endIso: toIsoDate(e) });
              }}
              style={{
                whiteSpace: 'nowrap',
                padding: '5px 10px',
                borderRadius: 999,
                border:
                  '1px solid ' +
                  (active ? 'rgba(212,165,71,0.5)' : 'var(--border)'),
                background: active ? 'var(--gold-bg)' : 'transparent',
                color: active ? 'var(--gold)' : 'var(--text-2)',
                fontFamily: 'inherit',
                fontSize: 11.5,
                cursor: 'pointer',
                flexShrink: 0,
              }}
            >
              {p.label}
            </button>
          );
        })}
      </div>

      <div
        style={{
          marginTop: 8,
          padding: '10px 10px 4px',
          border: '1px solid var(--border)',
          borderRadius: 6,
          background: 'var(--surface-elev)',
        }}
      >
        <CalendarView
          anchor={anchor}
          setAnchor={setAnchor}
          start={start}
          end={end}
          hover={hover}
          onPick={handlePick}
          onHover={setHover}
          density="mobile"
          monthHeaderSize="md"
        />
      </div>
    </div>
  );
}
