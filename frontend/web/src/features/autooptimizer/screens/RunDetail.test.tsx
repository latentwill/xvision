import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { RunDetail } from "./RunDetail";
import * as apiModule from "../api";

beforeEach(() => {
  // @ts-expect-error jsdom EventSource stub
  global.EventSource = vi.fn().mockImplementation(() => ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  }));
});

afterEach(() => vi.restoreAllMocks());

describe("RunDetail", () => {
  it("renders header with back link to /optimizer", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEF" />, {
      route: "/optimizer/run/sess_01ABCDEF",
    });

    // Back link must point to /optimizer
    const backLink = await screen.findByRole("link", { name: /optimizer/i });
    expect(backLink).toHaveAttribute("href", "/optimizer");
  });

  it("renders session id in header", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEF" />, {
      route: "/optimizer/run/sess_01ABCDEF",
    });

    // Should show truncated session id (first 8 chars) — may appear in multiple places
    const matches = await screen.findAllByText(/sess_01A/);
    expect(matches.length).toBeGreaterThanOrEqual(1);
  });

  it("renders ActivityFeed placeholder area", async () => {
    vi.spyOn(apiModule, "useOptimizerStatus").mockReturnValue({
      active_session: null,
      last_event_seq: 0,
    });

    renderWithProviders(<RunDetail sessionId="sess_01ABCDEF" />, {
      route: "/optimizer/run/sess_01ABCDEF",
    });

    // ActivityFeed is rendered — look for its container via test-id
    expect(await screen.findByTestId("activity-feed")).toBeInTheDocument();
  });
});
