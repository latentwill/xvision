import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';

import {
  InlineRangeBar,
  MobileInlineCard,
  fromIsoDate,
  toIsoDate,
  utcDate,
} from '.';

afterEach(() => {
  cleanup();
});

// UTC discipline. The form serializes `${from}T00:00:00Z`, so anything
// the picker round-trips must hold the calendar date stable regardless
// of the runner's local timezone.
describe('calendar-core date utilities', () => {
  it('toIsoDate / fromIsoDate round-trip preserves the UTC calendar date', () => {
    const d = utcDate(2024, 0, 15);
    expect(toIsoDate(d)).toBe('2024-01-15');
    const parsed = fromIsoDate('2024-01-15');
    expect(parsed?.getUTCFullYear()).toBe(2024);
    expect(parsed?.getUTCMonth()).toBe(0);
    expect(parsed?.getUTCDate()).toBe(15);
  });

  it('fromIsoDate returns null for malformed input', () => {
    expect(fromIsoDate('')).toBeNull();
    expect(fromIsoDate('not a date')).toBeNull();
    expect(fromIsoDate('2024-13-40')).not.toBeNull(); // Date normalizes
  });
});

describe('InlineRangeBar', () => {
  it('renders closed by default and shows the start/end summary', () => {
    render(
      <InlineRangeBar
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={vi.fn()}
      />,
    );
    const bar = screen.getByTestId('inline-range-bar');
    expect(bar.dataset.open).toBe('false');
    // Closed view shows the formatted summary (Jan 15, 2024 → Jan 20, 2024)
    expect(bar.textContent).toContain('Jan 15, 2024');
    expect(bar.textContent).toContain('Jan 20, 2024');
  });

  it('opens when the header is clicked and reveals month grids', () => {
    render(
      <InlineRangeBar
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={vi.fn()}
      />,
    );
    const bar = screen.getByTestId('inline-range-bar');
    const toggle = bar.querySelector('button[aria-expanded]') as HTMLElement;
    fireEvent.click(toggle);
    expect(bar.dataset.open).toBe('true');
    // After opening, day buttons are in the DOM with data-iso markers.
    expect(bar.querySelector('button[data-iso="2024-01-15"]')).toBeTruthy();
  });

  it('renders without overlay primitives — no Dialog / Popover / Sheet roles', () => {
    render(
      <InlineRangeBar
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={vi.fn()}
        defaultOpen
      />,
    );
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByRole('alertdialog')).toBeNull();
    expect(screen.queryByRole('menu')).toBeNull();
  });

  it('cancel reverts working state to the parent value', () => {
    const onChange = vi.fn();
    render(
      <InlineRangeBar
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={onChange}
        defaultOpen
      />,
    );
    // Pick a different start, then cancel — no onChange must fire.
    const earlier = screen
      .getByTestId('inline-range-bar')
      .querySelector('button[data-iso="2024-01-10"]') as HTMLElement;
    fireEvent.click(earlier);
    fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it('apply commits the typed range to onChange in YYYY-MM-DD shape', () => {
    const onChange = vi.fn();
    render(
      <InlineRangeBar
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={onChange}
        defaultOpen
      />,
    );
    const bar = screen.getByTestId('inline-range-bar');
    // Click a new start, then a new end.
    fireEvent.click(bar.querySelector('button[data-iso="2024-01-05"]') as HTMLElement);
    fireEvent.click(bar.querySelector('button[data-iso="2024-01-25"]') as HTMLElement);
    fireEvent.click(screen.getByRole('button', { name: 'Apply range' }));
    expect(onChange).toHaveBeenLastCalledWith({
      startIso: '2024-01-05',
      endIso: '2024-01-25',
    });
  });

  it('rejects clicking earlier than start as a new start (not an end)', () => {
    const onChange = vi.fn();
    render(
      <InlineRangeBar
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={onChange}
        defaultOpen
      />,
    );
    const bar = screen.getByTestId('inline-range-bar');
    // Click a new start at 1/10 — clears end.
    fireEvent.click(bar.querySelector('button[data-iso="2024-01-10"]') as HTMLElement);
    // Click a day earlier than 1/10 — this becomes the new start, not the end.
    fireEvent.click(bar.querySelector('button[data-iso="2024-01-05"]') as HTMLElement);
    // Now pick the end.
    fireEvent.click(bar.querySelector('button[data-iso="2024-01-25"]') as HTMLElement);
    fireEvent.click(screen.getByRole('button', { name: 'Apply range' }));
    expect(onChange).toHaveBeenLastCalledWith({
      startIso: '2024-01-05',
      endIso: '2024-01-25',
    });
  });

  it('collapsing the header discards an unapplied draft range', () => {
    const onChange = vi.fn();
    render(
      <InlineRangeBar
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={onChange}
        defaultOpen
      />,
    );
    const bar = screen.getByTestId('inline-range-bar');
    fireEvent.click(bar.querySelector('button[data-iso="2024-01-05"]') as HTMLElement);
    fireEvent.click(bar.querySelector('button[data-iso="2024-01-25"]') as HTMLElement);
    fireEvent.click(bar.querySelector('button[aria-expanded]') as HTMLElement);

    expect(onChange).not.toHaveBeenCalled();
    expect(bar.dataset.open).toBe('false');
    expect(bar.textContent).toContain('Jan 15, 2024');
    expect(bar.textContent).toContain('Jan 20, 2024');
  });
});

describe('MobileInlineCard', () => {
  it('does not render any overlay primitives', () => {
    render(
      <MobileInlineCard
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={vi.fn()}
      />,
    );
    expect(screen.queryByRole('dialog')).toBeNull();
    expect(screen.queryByRole('alertdialog')).toBeNull();
  });

  it('commits a range on the second click (no Apply button)', () => {
    const onChange = vi.fn();
    const { container } = render(
      <MobileInlineCard
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={onChange}
      />,
    );
    // First click picks new start, clears end. Second click sets end +
    // emits onChange.
    fireEvent.click(
      container.querySelector('button[data-iso="2024-01-05"]') as HTMLElement,
    );
    fireEvent.click(
      container.querySelector('button[data-iso="2024-01-25"]') as HTMLElement,
    );
    expect(onChange).toHaveBeenLastCalledWith({
      startIso: '2024-01-05',
      endIso: '2024-01-25',
    });
  });

  it('reflects updated parent start and end props', () => {
    const onChange = vi.fn();
    const { rerender } = render(
      <MobileInlineCard
        startIso="2024-01-15"
        endIso="2024-01-20"
        onChange={onChange}
      />,
    );

    rerender(
      <MobileInlineCard
        startIso="2024-02-01"
        endIso="2024-02-10"
        onChange={onChange}
      />,
    );

    const card = screen.getByTestId('mobile-inline-card');
    expect(card.textContent).toContain('Feb 1, 2024');
    expect(card.textContent).toContain('Feb 10, 2024');
  });
});
