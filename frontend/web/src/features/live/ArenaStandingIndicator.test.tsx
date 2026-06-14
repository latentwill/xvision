// Tests for ArenaStandingIndicator — Phase 8 arena standing inline strip.
// Covers: chip on/off states, rank rendering, live PnL (positive + negative),
// and absence of network calls (presentational-only assertion).
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { ArenaStandingIndicator } from "./ArenaStandingIndicator";

afterEach(() => {
  cleanup();
});

// --------------------------------------------------------------------------
// Baseline rendering
// --------------------------------------------------------------------------

describe("ArenaStandingIndicator — baseline", () => {
  it("renders the arena standing label", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena={false}
        aiPotInView={false}
      />,
    );
    expect(screen.getByTestId("arena-standing-indicator")).toBeInTheDocument();
    expect(screen.getByTestId("arena-standing-indicator").textContent).toMatch(
      /arena standing/i,
    );
  });

  it("renders both chip slots regardless of active state", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena={false}
        aiPotInView={false}
      />,
    );
    expect(screen.getByTestId("chip-trading-via-arena")).toBeInTheDocument();
    expect(screen.getByTestId("chip-ai-pot-in-view")).toBeInTheDocument();
  });
});

// --------------------------------------------------------------------------
// Chip on / off states
// --------------------------------------------------------------------------

describe("ArenaStandingIndicator — Trading via Arena chip", () => {
  it("shows ✓ when tradingViaArena is true", () => {
    render(
      <ArenaStandingIndicator tradingViaArena aiPotInView={false} />,
    );
    const chip = screen.getByTestId("chip-trading-via-arena");
    expect(chip.textContent).toContain("✓");
    expect(chip.textContent).not.toContain("—");
  });

  it("shows — when tradingViaArena is false", () => {
    render(
      <ArenaStandingIndicator tradingViaArena={false} aiPotInView={false} />,
    );
    const chip = screen.getByTestId("chip-trading-via-arena");
    expect(chip.textContent).toContain("—");
    expect(chip.textContent).not.toContain("✓");
  });
});

describe("ArenaStandingIndicator — AI Pot in view chip", () => {
  it("shows ✓ when aiPotInView is true", () => {
    render(
      <ArenaStandingIndicator tradingViaArena={false} aiPotInView />,
    );
    const chip = screen.getByTestId("chip-ai-pot-in-view");
    expect(chip.textContent).toContain("✓");
    expect(chip.textContent).not.toContain("—");
  });

  it("shows — when aiPotInView is false", () => {
    render(
      <ArenaStandingIndicator tradingViaArena={false} aiPotInView={false} />,
    );
    const chip = screen.getByTestId("chip-ai-pot-in-view");
    expect(chip.textContent).toContain("—");
    expect(chip.textContent).not.toContain("✓");
  });
});

// --------------------------------------------------------------------------
// Rank chip
// --------------------------------------------------------------------------

describe("ArenaStandingIndicator — rank chip", () => {
  it("renders the rank chip when rank is provided", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena
        aiPotInView
        rank={3}
      />,
    );
    expect(screen.getByTestId("chip-rank")).toBeInTheDocument();
    expect(screen.getByTestId("rank-value").textContent).toBe("#3");
  });

  it("does NOT render the rank chip when rank is null", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena
        aiPotInView
        rank={null}
      />,
    );
    expect(screen.queryByTestId("chip-rank")).not.toBeInTheDocument();
  });

  it("does NOT render the rank chip when rank is omitted", () => {
    render(
      <ArenaStandingIndicator tradingViaArena aiPotInView />,
    );
    expect(screen.queryByTestId("chip-rank")).not.toBeInTheDocument();
  });
});

// --------------------------------------------------------------------------
// PnL chip
// --------------------------------------------------------------------------

describe("ArenaStandingIndicator — PnL chip", () => {
  it("renders a positive PnL with a '+' prefix", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena
        aiPotInView
        pnlUsd={123.45}
      />,
    );
    expect(screen.getByTestId("chip-pnl")).toBeInTheDocument();
    expect(screen.getByTestId("pnl-value").textContent).toBe("+$123.45");
  });

  it("renders a negative PnL with a '-' prefix", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena
        aiPotInView
        pnlUsd={-42.0}
      />,
    );
    expect(screen.getByTestId("chip-pnl")).toBeInTheDocument();
    expect(screen.getByTestId("pnl-value").textContent).toBe("-$42.00");
  });

  it("renders zero PnL with a '+' prefix", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena
        aiPotInView
        pnlUsd={0}
      />,
    );
    expect(screen.getByTestId("pnl-value").textContent).toBe("+$0.00");
  });

  it("does NOT render the PnL chip when pnlUsd is null", () => {
    render(
      <ArenaStandingIndicator
        tradingViaArena
        aiPotInView
        pnlUsd={null}
      />,
    );
    expect(screen.queryByTestId("chip-pnl")).not.toBeInTheDocument();
  });

  it("does NOT render the PnL chip when pnlUsd is omitted", () => {
    render(
      <ArenaStandingIndicator tradingViaArena aiPotInView />,
    );
    expect(screen.queryByTestId("chip-pnl")).not.toBeInTheDocument();
  });
});

// --------------------------------------------------------------------------
// No network call assertion
// --------------------------------------------------------------------------

describe("ArenaStandingIndicator — presentational (no network)", () => {
  it("renders without triggering any fetch calls", () => {
    const fetchSpy = vi.spyOn(globalThis, "fetch");

    render(
      <ArenaStandingIndicator
        tradingViaArena
        aiPotInView
        rank={1}
        pnlUsd={500}
      />,
    );

    expect(fetchSpy).not.toHaveBeenCalled();
    fetchSpy.mockRestore();
  });
});
