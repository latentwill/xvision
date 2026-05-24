import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { MemoryRouter, useLocation } from "react-router-dom";
import { parseCompareIds, useCompareSelection } from "./useCompareSelection";

function Harness() {
  const selection = useCompareSelection();
  const location = useLocation();
  return (
    <div>
      <div data-testid="ids">{selection.selectedIds.join("|")}</div>
      <div data-testid="search">{location.search}</div>
      <button onClick={() => selection.remove("a")}>remove a</button>
      <button onClick={() => selection.add("c")}>add c</button>
      <button onClick={() => selection.setLead("b")}>lead b</button>
    </div>
  );
}

describe("useCompareSelection", () => {
  afterEach(() => cleanup());

  it("parses ids once, preserves order, and caps at 10", () => {
    expect(parseCompareIds("a,b,a,,c")).toEqual(["a", "b", "c"]);
    expect(parseCompareIds("0,1,2,3,4,5,6,7,8,9,10")).toHaveLength(10);
  });

  it("enforces the min-2 remove invariant", () => {
    render(
      <MemoryRouter initialEntries={["/charts/compare?ids=a,b"]}>
        <Harness />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByText("remove a"));
    expect(screen.getByTestId("ids").textContent).toBe("a|b");
  });

  it("adds ids and can promote a lead id through the URL", () => {
    render(
      <MemoryRouter initialEntries={["/charts/compare?ids=a,b"]}>
        <Harness />
      </MemoryRouter>,
    );

    fireEvent.click(screen.getByText("add c"));
    expect(screen.getByTestId("ids").textContent).toBe("a|b|c");

    fireEvent.click(screen.getByText("lead b"));
    expect(screen.getByTestId("ids").textContent).toBe("b|a|c");
    expect(screen.getByTestId("search").textContent).toBe("?ids=b%2Ca%2Cc");
  });
});
