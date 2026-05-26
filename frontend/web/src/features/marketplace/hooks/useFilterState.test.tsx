// src/features/marketplace/hooks/useFilterState.test.tsx
import { render, screen, act } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { useFilterState } from "./useFilterState";

function Probe() {
  const { filter, setFilter } = useFilterState();
  return (
    <div>
      <span data-testid="assets">{filter.assets.join(",")}</span>
      <span data-testid="sort">{filter.sort}</span>
      <button onClick={() => setFilter({ assets: ["SOL"], sort: "sharpe" })}>set</button>
    </div>
  );
}

describe("useFilterState", () => {
  it("reads initial state from the URL query", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace?assets=BTC,SOL&sort=buyers"]}>
        <Probe />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("assets").textContent).toBe("BTC,SOL");
    expect(screen.getByTestId("sort").textContent).toBe("buyers");
  });
  it("writes updates back to the URL", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace"]}>
        <Probe />
      </MemoryRouter>,
    );
    act(() => screen.getByText("set").click());
    expect(screen.getByTestId("assets").textContent).toBe("SOL");
    expect(screen.getByTestId("sort").textContent).toBe("sharpe");
  });
});
