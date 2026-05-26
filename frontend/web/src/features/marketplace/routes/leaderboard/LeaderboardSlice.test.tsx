// src/features/marketplace/routes/leaderboard/LeaderboardSlice.test.tsx
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderMarketplace } from "@/features/marketplace/test-utils";
import { LeaderboardSlice } from "./LeaderboardSlice";

// Fixture slice: sol-7d = { label: "Top on SOL · 7d", hint: "asset=SOL · 7d", count: 142 }
// FixtureMarketplaceData.getLeaderboard filters ALL_LISTINGS by slice.filter
// which for sol-7d is { assets: ["SOL"], sort: "return30d" }

function render(sliceId = "sol-7d") {
  return renderMarketplace(<LeaderboardSlice />, {
    path: "/marketplace/leaderboard/:sliceId",
    route: `/marketplace/leaderboard/${sliceId}`,
  });
}

describe("LeaderboardSlice", () => {
  it("renders the slice label as the page heading", async () => {
    render();
    const heading = await screen.findByTestId("slice-label");
    expect(heading).toHaveTextContent("Top on SOL · 7d");
  });

  it("renders the slice hint text", async () => {
    render();
    await screen.findByTestId("slice-label");
    expect(screen.getByText(/asset=SOL · 7d/)).toBeInTheDocument();
  });

  it("renders the slice count", async () => {
    render();
    await screen.findByTestId("slice-label");
    expect(screen.getByText(/142/)).toBeInTheDocument();
  });

  it("renders a back link to /marketplace/leaderboard", async () => {
    render();
    await screen.findByTestId("slice-label");
    const backLink = screen.getByRole("link", { name: /← Leaderboard/i });
    expect(backLink).toHaveAttribute("href", "/marketplace/leaderboard");
  });

  it("renders ListingCard rows for the slice", async () => {
    render();
    // sol-7d slice filters by assets: ["SOL"]; fixture has SOL listings
    // Wait for the slice header to appear first
    await screen.findByTestId("slice-label");
    // At least one ListingCard should render (SOL-filtered rows)
    const cards = screen.getAllByRole("button", { name: /buy|run free/i });
    expect(cards.length).toBeGreaterThan(0);
  });

  it("renders the column header row", async () => {
    render();
    await screen.findByTestId("slice-label");
    expect(screen.getByText("Strategy")).toBeInTheDocument();
    expect(screen.getByText("30d return")).toBeInTheDocument();
    expect(screen.getByText("Buyers")).toBeInTheDocument();
    expect(screen.getByText("Sharpe")).toBeInTheDocument();
    expect(screen.getByText("Price")).toBeInTheDocument();
  });

  it("renders non-empty rows for the trending slice", async () => {
    renderMarketplace(<LeaderboardSlice />, {
      path: "/marketplace/leaderboard/:sliceId",
      route: "/marketplace/leaderboard/trending",
    });
    // trending slice: segment="trending", sort="return30d" — all listings pass
    await screen.findByTestId("slice-label");
    expect(screen.getByTestId("slice-label")).toHaveTextContent("Trending");
    const cards = screen.getAllByRole("button", { name: /buy|run free/i });
    expect(cards.length).toBeGreaterThan(0);
  });

  it("renders the Testnet badge from ListingCard on paid listings", async () => {
    render();
    await screen.findByTestId("slice-label");
    // ListingCard renders a [Testnet] badge on Buy CTAs
    const testnetBadges = screen.getAllByText(/testnet/i);
    expect(testnetBadges.length).toBeGreaterThan(0);
  });

  it("does not render any dialog or modal (no-popups rule)", async () => {
    render();
    await screen.findByTestId("slice-label");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
