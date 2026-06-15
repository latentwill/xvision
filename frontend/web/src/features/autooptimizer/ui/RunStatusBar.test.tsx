import { describe, expect, it } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { RunStatusBar } from "./RunStatusBar";
import type { SessionSummary } from "../api";

const session: SessionSummary = {
  session_id: "sess_01ABC",
  strategy_id: "strat-xyz",
  state: "running",
  mode: "explore",
  cycles_completed: 3,
  kept_count: 5,
  suspect_count: 1,
  dropped_count: 9,
};

describe("RunStatusBar", () => {
  it("announces a running run with a pulsing live pill and the cycle id", () => {
    renderWithProviders(
      <RunStatusBar
        activity="running"
        source="status"
        cycleId="01KV3X7EZZZZZZZZ"
        session={session}
        connected
        startedAtMs={null}
      />,
    );
    const region = screen.getByRole("status");
    expect(region).toHaveTextContent(/running/i);
    // Reuses the shared in-flight Pill (xvn-pill-animated) for the live pulse.
    expect(region.querySelector(".xvn-pill-animated")).not.toBeNull();
    // Cycle id is shown short (8 chars) and selectable.
    expect(screen.getByText("01KV3X7E")).toBeInTheDocument();
  });

  it("shows cycle number (completed + 1) and kept count from the session", () => {
    renderWithProviders(
      <RunStatusBar
        activity="running"
        source="status"
        cycleId="cyc1"
        session={session}
        connected
        startedAtMs={null}
      />,
    );
    expect(screen.getByText(/cycle #4/i)).toBeInTheDocument();
    // value + muted label render as sibling spans; match the combined text.
    expect(
      screen.getByText(
        (_, el) => el?.textContent?.replace(/\s+/g, " ").trim() === "5 kept",
      ),
    ).toBeInTheDocument();
  });

  it("paused state reads as PAUSED and does not pulse", () => {
    renderWithProviders(
      <RunStatusBar
        activity="paused"
        source="status"
        cycleId="cyc1"
        session={{ ...session, state: "paused" }}
        connected
        startedAtMs={null}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(/paused/i);
    expect(document.querySelector(".xvn-pill-animated")).toBeNull();
  });

  it("an inferred run (no session row) still shows RUNNING + cycle id, omitting counters", () => {
    renderWithProviders(
      <RunStatusBar
        activity="running"
        source="events"
        cycleId="01KV3X7EZZZZ"
        session={null}
        connected={false}
        startedAtMs={null}
      />,
    );
    expect(screen.getByRole("status")).toHaveTextContent(/running/i);
    expect(screen.getByText("01KV3X7E")).toBeInTheDocument();
    expect(screen.queryByText(/cycle #/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/kept/i)).not.toBeInTheDocument();
  });

  it("reflects the live connection: 'live' when SSE is connected, 'polling' otherwise", () => {
    const { rerender } = renderWithProviders(
      <RunStatusBar
        activity="running"
        source="status"
        cycleId="cyc1"
        session={session}
        connected
        startedAtMs={null}
      />,
    );
    expect(screen.getByText(/^live$/i)).toBeInTheDocument();

    rerender(
      <RunStatusBar
        activity="running"
        source="events"
        cycleId="cyc1"
        session={session}
        connected={false}
        startedAtMs={null}
      />,
    );
    expect(screen.getByText(/polling/i)).toBeInTheDocument();
  });

  it("shows a ticking elapsed label when a start time is known", () => {
    renderWithProviders(
      <RunStatusBar
        activity="running"
        source="status"
        cycleId="cyc1"
        session={session}
        connected
        startedAtMs={Date.now() - 75_000}
      />,
    );
    // 75s → "1m 1Xs"
    expect(screen.getByText(/1m \d\ds/)).toBeInTheDocument();
  });
});
