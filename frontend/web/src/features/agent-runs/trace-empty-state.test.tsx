// frontend/web/src/features/agent-runs/trace-empty-state.test.tsx
import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { emptyTreeReason, TraceEmptyState } from "./trace-empty-state";

afterEach(() => cleanup());

const base = {
  sourceCount: 5,
  filteredCount: 5,
  displayCount: 5,
  filterActive: false,
  isLive: false,
  advancedView: false,
};

describe("emptyTreeReason", () => {
  test("returns null when there are rows to render", () => {
    expect(emptyTreeReason({ ...base, displayCount: 3 })).toBeNull();
  });

  test("no source spans on a finished run → no-spans (NOT a filter message)", () => {
    expect(
      emptyTreeReason({ ...base, sourceCount: 0, filteredCount: 0, displayCount: 0 }),
    ).toBe("no-spans");
  });

  test("no source spans on a live run → live-waiting", () => {
    expect(
      emptyTreeReason({
        ...base,
        sourceCount: 0,
        filteredCount: 0,
        displayCount: 0,
        isLive: true,
      }),
    ).toBe("live-waiting");
  });

  test("spans exist but an active filter hides them all → filtered", () => {
    expect(
      emptyTreeReason({
        ...base,
        sourceCount: 5,
        filteredCount: 0,
        displayCount: 0,
        filterActive: true,
      }),
    ).toBe("filtered");
  });

  test("spans survive the filter but Simple view hides them all → simple-hidden", () => {
    expect(
      emptyTreeReason({
        ...base,
        sourceCount: 5,
        filteredCount: 5,
        displayCount: 0,
        advancedView: false,
      }),
    ).toBe("simple-hidden");
  });

  test("already in Advanced and still empty → all-hidden (internal markers)", () => {
    expect(
      emptyTreeReason({
        ...base,
        sourceCount: 5,
        filteredCount: 5,
        displayCount: 0,
        advancedView: true,
      }),
    ).toBe("all-hidden");
  });

  test("no active filter never yields a filtered reason (default filter passes all)", () => {
    // sourceCount>0 with filteredCount 0 only happens via an ACTIVE filter; the
    // default filter passes everything, so a fresh run can't read as 'filtered'.
    expect(
      emptyTreeReason({ ...base, sourceCount: 5, filteredCount: 0, displayCount: 0, filterActive: false }),
    ).toBe("simple-hidden"); // falls through to a view reason, never blames a phantom filter
  });
});

describe("TraceEmptyState", () => {
  test("no-spans renders an honest message and no filter blame / no action", () => {
    render(
      <TraceEmptyState reason="no-spans" hiddenCount={0} onClearFilters={vi.fn()} onShowAdvanced={vi.fn()} />,
    );
    expect(screen.getByText(/no spans were recorded for this run/i)).toBeInTheDocument();
    expect(screen.queryByText(/match the current filter/i)).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  test("live-waiting renders a waiting message", () => {
    render(
      <TraceEmptyState reason="live-waiting" hiddenCount={0} onClearFilters={vi.fn()} onShowAdvanced={vi.fn()} />,
    );
    expect(screen.getByText(/waiting for spans/i)).toBeInTheDocument();
  });

  test("filtered renders the filter message AND a Clear-filters button that fires the callback", () => {
    const onClearFilters = vi.fn();
    render(
      <TraceEmptyState reason="filtered" hiddenCount={0} onClearFilters={onClearFilters} onShowAdvanced={vi.fn()} />,
    );
    expect(screen.getByText(/no spans match the current filter/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /clear filters/i }));
    expect(onClearFilters).toHaveBeenCalledTimes(1);
  });

  test("simple-hidden renders a Show-Advanced button that fires the callback", () => {
    const onShowAdvanced = vi.fn();
    render(
      <TraceEmptyState reason="simple-hidden" hiddenCount={3} onClearFilters={vi.fn()} onShowAdvanced={onShowAdvanced} />,
    );
    expect(screen.getByText(/3 instrumentation spans are hidden in simple view/i)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /show advanced/i }));
    expect(onShowAdvanced).toHaveBeenCalledTimes(1);
  });

  test("all-hidden states the markers are internal with no action", () => {
    render(
      <TraceEmptyState reason="all-hidden" hiddenCount={2} onClearFilters={vi.fn()} onShowAdvanced={vi.fn()} />,
    );
    expect(screen.getByText(/2 internal marker spans/i)).toBeInTheDocument();
    expect(screen.queryByRole("button")).toBeNull();
  });

  test("singularizes copy for a single hidden span", () => {
    render(
      <TraceEmptyState reason="simple-hidden" hiddenCount={1} onClearFilters={vi.fn()} onShowAdvanced={vi.fn()} />,
    );
    expect(screen.getByText(/1 instrumentation span is hidden/i)).toBeInTheDocument();
  });
});
