// src/features/marketplace/components/FilterDrawer.test.tsx
// The FilterDrawer is now an inline accordion (spec 3.1C, QA4): in document
// flow, NOT an absolute overlay / Dialog / Sheet / Popover / complementary aside.
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { FilterDrawer } from "./FilterDrawer";

describe("FilterDrawer (inline accordion)", () => {
  it("does not render content when closed", () => {
    render(
      <FilterDrawer open={false}>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.queryByText("filters")).not.toBeInTheDocument();
  });

  it("renders content in document flow when open", () => {
    render(
      <FilterDrawer open>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.getByText("filters")).toBeInTheDocument();
  });

  it("is not a dialog or an absolute complementary aside (no-popups rule)", () => {
    const { container } = render(
      <FilterDrawer open>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("complementary")).not.toBeInTheDocument();
    // In-flow region wrapper carries the accordion marker, not an absolute aside.
    const region = container.querySelector("[data-filter-accordion]");
    expect(region).not.toBeNull();
    expect(region!.className).not.toMatch(/absolute/);
  });
});
