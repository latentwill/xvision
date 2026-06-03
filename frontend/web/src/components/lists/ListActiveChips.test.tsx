import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

import { ListActiveChips } from "./ListActiveChips";
import type { ActiveFilter } from "./useListState";

function activeStatusFilter(setValue = vi.fn()): ActiveFilter {
  return {
    def: {
      id: "status",
      label: "Status",
      options: [
        { value: "all", label: "All" },
        { value: "Validated", label: "Validated" },
      ],
    },
    value: "Validated",
    setValue,
  };
}

describe("ListActiveChips", () => {
  it("clears the search chip when clicked", () => {
    const setSearch = vi.fn();
    render(
      <ListActiveChips search={{ value: "eth", setValue: setSearch }} />,
    );

    fireEvent.click(screen.getByRole("button", { name: /search.*eth/i }));

    expect(setSearch).toHaveBeenCalledTimes(1);
    expect(setSearch).toHaveBeenCalledWith("");
  });

  it("invokes clearAll from the clear-all action", () => {
    const clearAll = vi.fn();
    render(
      <ListActiveChips
        search={{ value: "eth", setValue: vi.fn() }}
        filters={[activeStatusFilter()]}
        clearAll={clearAll}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /clear all/i }));

    expect(clearAll).toHaveBeenCalledTimes(1);
  });
});
