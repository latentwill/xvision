// src/features/marketplace/routes/leaderboard/LeaderboardIndex.test.tsx
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderMarketplace } from "@/features/marketplace/test-utils";
import { LeaderboardIndex } from "./LeaderboardIndex";

// fixture slice ids from SLICES fixture:
// trending, sol-7d, claude, agents, newest, cloned, free

function render() {
  return renderMarketplace(<LeaderboardIndex />, {
    path: "/marketplace/leaderboard",
    route: "/marketplace/leaderboard",
  });
}

describe("LeaderboardIndex", () => {
  it("renders the page heading", async () => {
    render();
    expect(await screen.findByRole("heading", { level: 1 })).toHaveTextContent("Leaderboard");
  });

  it("renders all slice labels from the fixture", async () => {
    render();
    expect(await screen.findByText("Trending")).toBeInTheDocument();
    expect(await screen.findByText("Top on SOL · 7d")).toBeInTheDocument();
    expect(await screen.findByText("Top with Claude")).toBeInTheDocument();
    expect(await screen.findByText("Most agent-bought")).toBeInTheDocument();
    expect(await screen.findByText("Newest 24h")).toBeInTheDocument();
    expect(await screen.findByText("Free-tier breakouts")).toBeInTheDocument();
  });

  it("renders the hint text for each slice", async () => {
    render();
    expect(await screen.findByText(/weighted by 24h velocity/)).toBeInTheDocument();
    expect(await screen.findByText(/asset=SOL/)).toBeInTheDocument();
  });

  it("renders the live count for each slice (computed from the curated pool)", async () => {
    render();
    // Counts are computed live by getSlices() — no stale hardcoded figures.
    // trending matches the whole curated collection (6).
    expect(await screen.findByTestId("slice-count-trending")).toHaveTextContent("6");
    // sol-7d matches the two SOL listings.
    expect(await screen.findByTestId("slice-count-sol-7d")).toHaveTextContent("2");
  });

  it("each slice item is a link to /marketplace/leaderboard/<id>", async () => {
    render();
    const link = await screen.findByTestId("slice-link-trending");
    expect(link).toHaveAttribute("href", "/marketplace/leaderboard/trending");

    const solLink = await screen.findByTestId("slice-link-sol-7d");
    expect(solLink).toHaveAttribute("href", "/marketplace/leaderboard/sol-7d");
  });

  it("renders the slices container", async () => {
    render();
    expect(await screen.findByTestId("slices-index")).toBeInTheDocument();
  });

  it("does not render any dialog or modal (no-popups rule)", async () => {
    render();
    await screen.findByRole("heading", { level: 1 });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
