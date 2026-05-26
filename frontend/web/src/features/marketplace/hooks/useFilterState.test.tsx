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
      <span data-testid="price-from">{filter.priceUsdc.from}</span>
      <span data-testid="price-to">{filter.priceUsdc.to}</span>
      <span data-testid="segment">{filter.segment}</span>
      <span data-testid="tier">{filter.tier.join(",")}</span>
      <button onClick={() => setFilter({ assets: ["SOL"], sort: "sharpe" })}>set</button>
      <button onClick={() => setFilter({ tier: ["open"] })}>set-tier</button>
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
  it("parses a price range from the URL into priceUsdc", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace?price=10-80"]}>
        <Probe />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("price-from").textContent).toBe("10");
    expect(screen.getByTestId("price-to").textContent).toBe("80");
  });
  it("falls back to default segment for an invalid segment param", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace?segment=evil"]}>
        <Probe />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("segment").textContent).toBe("trending");
  });
  it("parses ?tier=open from URL into filter.tier", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace?tier=open"]}>
        <Probe />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("tier").textContent).toBe("open");
  });
  it("serializes tier back to the URL when set", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace"]}>
        <Probe />
      </MemoryRouter>,
    );
    act(() => screen.getByText("set-tier").click());
    expect(screen.getByTestId("tier").textContent).toBe("open");
  });
  it("strips invalid tier values from the URL", () => {
    render(
      <MemoryRouter initialEntries={["/marketplace?tier=open,nope,sealed"]}>
        <Probe />
      </MemoryRouter>,
    );
    expect(screen.getByTestId("tier").textContent).toBe("open,sealed");
  });
});
