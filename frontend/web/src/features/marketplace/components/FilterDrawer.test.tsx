// src/features/marketplace/components/FilterDrawer.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { FilterDrawer } from "./FilterDrawer";

describe("FilterDrawer", () => {
  it("does not render content when closed", () => {
    render(
      <FilterDrawer open={false} onClose={() => {}}>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.queryByText("filters")).not.toBeInTheDocument();
  });
  it("renders content and a close affordance when open", () => {
    const onClose = vi.fn();
    render(
      <FilterDrawer open onClose={onClose}>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.getByText("filters")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /close/i }));
    expect(onClose).toHaveBeenCalledOnce();
  });
  it("is a docked complementary panel, not a dialog (no-popups rule)", () => {
    render(
      <FilterDrawer open onClose={() => {}}>
        <p>filters</p>
      </FilterDrawer>,
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.getByRole("complementary")).toBeInTheDocument();
  });
});
