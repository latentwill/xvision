// src/features/marketplace/data/provider.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { MarketplaceDataProvider, useMarketplaceData } from "./provider";
import { FixtureMarketplaceData } from "./MarketplaceData";

function Probe() {
  const mp = useMarketplaceData();
  return <span>{mp instanceof FixtureMarketplaceData ? "fixture" : "other"}</span>;
}

describe("MarketplaceDataProvider", () => {
  it("provides the instance to children", () => {
    render(
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <Probe />
      </MarketplaceDataProvider>,
    );
    expect(screen.getByText("fixture")).toBeInTheDocument();
  });
  it("throws when used outside provider", () => {
    expect(() => render(<Probe />)).toThrow(/MarketplaceDataProvider/);
  });
});
