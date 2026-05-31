import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { MListSheet } from "./MListSheet";
import type { ActiveFilter, ListSortState } from "./useListState";

function makeFilter(over: Partial<ActiveFilter> = {}): ActiveFilter {
  return {
    def: {
      id: "status",
      label: "Status",
      options: [
        { value: "all", label: "All" },
        { value: "Validated", label: "Validated" },
        { value: "Draft", label: "Draft" },
      ],
    },
    value: "all",
    setValue: vi.fn(),
    ...over,
  };
}

function makeSort(over: Partial<ListSortState> = {}): ListSortState {
  return {
    value: "added",
    setValue: vi.fn(),
    options: [
      { value: "added", label: "Recently added" },
      { value: "name", label: "Name A → Z" },
    ],
    ...over,
  };
}

afterEach(() => {
  document.body.style.overflow = "";
  document.body.style.paddingRight = "";
});

describe("MListSheet", () => {
  it("renders nothing when closed", () => {
    const { container } = render(
      <MListSheet
        open={false}
        onClose={() => {}}
        filters={[makeFilter()]}
        sort={makeSort()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("locks body scroll while open and restores on unmount", () => {
    const { unmount } = render(
      <MListSheet
        open
        onClose={() => {}}
        filters={[makeFilter()]}
        sort={makeSort()}
      />,
    );
    expect(document.body.style.overflow).toBe("hidden");
    unmount();
    expect(document.body.style.overflow).toBe("");
  });

  it("renders filter groups in filters-focus mode and Apply button labels result count", () => {
    render(
      <MListSheet
        open
        focus="filters"
        onClose={() => {}}
        filters={[makeFilter()]}
        sort={makeSort()}
        resultCount={4}
      />,
    );
    expect(screen.getByText("Status")).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /Filter & sort/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Show 4 results/i }),
    ).toBeInTheDocument();
  });

  it("hides filter groups in sort-focus mode", () => {
    render(
      <MListSheet
        open
        focus="sort"
        onClose={() => {}}
        filters={[makeFilter()]}
        sort={makeSort()}
        resultCount={1}
      />,
    );
    expect(screen.queryByText("Status")).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: /Sort by/i }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /Show 1 result\b/i }),
    ).toBeInTheDocument();
  });

  it("backdrop click dismisses", () => {
    const onClose = vi.fn();
    render(
      <MListSheet
        open
        onClose={onClose}
        filters={[makeFilter()]}
        sort={makeSort()}
      />,
    );
    fireEvent.click(screen.getByRole("dialog"));
    expect(onClose).toHaveBeenCalled();
  });

  it("Escape dismisses", () => {
    const onClose = vi.fn();
    render(
      <MListSheet
        open
        onClose={onClose}
        filters={[makeFilter()]}
        sort={makeSort()}
      />,
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalled();
  });

  it("filter pill click updates the filter value", () => {
    const setValue = vi.fn();
    render(
      <MListSheet
        open
        onClose={() => {}}
        filters={[makeFilter({ setValue })]}
        sort={makeSort()}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Validated" }));
    expect(setValue).toHaveBeenCalledWith("Validated");
  });

  it("sort row click updates sort value", () => {
    const setSort = vi.fn();
    render(
      <MListSheet
        open
        onClose={() => {}}
        filters={[makeFilter()]}
        sort={makeSort({ setValue: setSort })}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: /Name A → Z/i }));
    expect(setSort).toHaveBeenCalledWith("name");
  });

  it("traps Tab inside the sheet", () => {
    render(
      <MListSheet
        open
        onClose={() => {}}
        filters={[makeFilter()]}
        sort={makeSort()}
      />,
    );
    const buttons = screen.getAllByRole("button");
    const last = buttons[buttons.length - 1];
    last.focus();
    expect(document.activeElement).toBe(last);
    // Shift-tab wraps to last when we're at first; forward tab from last wraps to first.
    fireEvent.keyDown(window, { key: "Tab" });
    expect(document.activeElement).toBe(buttons[0]);
  });

  it("pointerCancel after threshold drag does not dismiss and sheet remains", () => {
    const onClose = vi.fn();
    const { container } = render(
      <MListSheet
        open
        onClose={onClose}
        filters={[makeFilter()]}
        sort={makeSort()}
      />,
    );
    const sheet = container.querySelector('[data-sheet-drag]')!
      .parentElement as HTMLElement;
    (sheet as unknown as { setPointerCapture: (id: number) => void }).setPointerCapture =
      () => {};
    fireEvent.pointerDown(
      container.querySelector('[data-sheet-drag]')!,
      { clientY: 100, pointerId: 1 },
    );
    fireEvent.pointerMove(sheet, { clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(sheet, { clientY: 250, pointerId: 1 });
    fireEvent.pointerCancel(sheet, { clientY: 260, pointerId: 1 });
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByRole("dialog")).toBeInTheDocument();
  });

  it("swipe-down past the threshold dismisses", () => {
    const onClose = vi.fn();
    const { container } = render(
      <MListSheet
        open
        onClose={onClose}
        filters={[makeFilter()]}
        sort={makeSort()}
      />,
    );
    const sheet = container.querySelector('[data-sheet-drag]')!
      .parentElement as HTMLElement;
    // jsdom doesn't implement setPointerCapture; stub it.
    (sheet as unknown as { setPointerCapture: (id: number) => void }).setPointerCapture =
      () => {};
    fireEvent.pointerDown(
      container.querySelector('[data-sheet-drag]')!,
      { clientY: 100, pointerId: 1 },
    );
    fireEvent.pointerMove(sheet, { clientY: 100, pointerId: 1 });
    fireEvent.pointerMove(sheet, { clientY: 250, pointerId: 1 });
    fireEvent.pointerUp(sheet, { clientY: 260, pointerId: 1 });
    expect(onClose).toHaveBeenCalled();
  });
});
