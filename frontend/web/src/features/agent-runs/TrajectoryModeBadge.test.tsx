// frontend/web/src/features/agent-runs/TrajectoryModeBadge.test.tsx
import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";
import { TrajectoryModeBadge, TrajectoryModePill } from "./TrajectoryModeBadge";
import { MOCK_RUN_COMPLETED, MOCK_RUN_REPLAY, MOCK_RUN_RECORD } from "./mock-fixtures";

describe("TrajectoryModePill", () => {
  test("renders LIVE label", () => {
    render(<TrajectoryModePill mode="live" />);
    expect(screen.getByTestId("trajectory-mode-pill")).toHaveAttribute("data-mode", "live");
    expect(screen.getByText("LIVE")).toBeInTheDocument();
  });

  test("renders RECORD label", () => {
    render(<TrajectoryModePill mode="record" />);
    expect(screen.getByTestId("trajectory-mode-pill")).toHaveAttribute("data-mode", "record");
    expect(screen.getByText("RECORD")).toBeInTheDocument();
  });

  test("renders REPLAY label", () => {
    render(<TrajectoryModePill mode="replay" />);
    expect(screen.getByTestId("trajectory-mode-pill")).toHaveAttribute("data-mode", "replay");
    expect(screen.getByText("REPLAY")).toBeInTheDocument();
  });
});

describe("TrajectoryModeBadge", () => {
  test("renders nothing for a pre-migration run (no trajectory_mode)", () => {
    const { container } = render(
      <TrajectoryModeBadge summary={MOCK_RUN_COMPLETED.summary} />,
    );
    expect(container.firstChild).toBeNull();
  });

  test("renders RECORD pill and no replay metrics for a record run", () => {
    render(<TrajectoryModeBadge summary={MOCK_RUN_RECORD.summary} />);
    expect(screen.getByTestId("trajectory-mode-badge")).toBeInTheDocument();
    expect(screen.getByTestId("trajectory-mode-pill")).toHaveAttribute("data-mode", "record");
    expect(screen.queryByTestId("replay-hit-ratio")).toBeNull();
    expect(screen.queryByTestId("dropped-events")).toBeNull();
    expect(screen.queryByTestId("recovery-reason-chip")).toBeNull();
  });

  test("renders REPLAY pill with hit ratio, dropped events, and recovery reason", () => {
    render(<TrajectoryModeBadge summary={MOCK_RUN_REPLAY.summary} />);
    expect(screen.getByTestId("trajectory-mode-pill")).toHaveAttribute("data-mode", "replay");
    // hit ratio: 0.875 → 87%
    expect(screen.getByTestId("replay-hit-ratio")).toHaveTextContent("hit 88%");
    // dropped events: 3
    expect(screen.getByTestId("dropped-events")).toHaveTextContent("3 dropped");
    // recovery reason: replay_divergence → human label
    expect(screen.getByTestId("recovery-reason-chip")).toHaveTextContent("replay diverged");
  });

  test("recovery reason chip has data-reason attribute set to the raw value", () => {
    render(<TrajectoryModeBadge summary={MOCK_RUN_REPLAY.summary} />);
    expect(screen.getByTestId("recovery-reason-chip")).toHaveAttribute(
      "data-reason",
      "replay_divergence",
    );
  });

  test("omits hit-ratio when replay_hit_ratio is null", () => {
    render(
      <TrajectoryModeBadge
        summary={{
          ...MOCK_RUN_REPLAY.summary,
          replay_hit_ratio: null,
          dropped_events: 0,
          recovery_reason: null,
        }}
      />,
    );
    expect(screen.queryByTestId("replay-hit-ratio")).toBeNull();
    expect(screen.queryByTestId("dropped-events")).toBeNull();
    expect(screen.queryByTestId("recovery-reason-chip")).toBeNull();
  });

  test("omits dropped-events chip when count is 0", () => {
    render(
      <TrajectoryModeBadge
        summary={{
          ...MOCK_RUN_REPLAY.summary,
          dropped_events: 0,
          recovery_reason: null,
        }}
      />,
    );
    expect(screen.queryByTestId("dropped-events")).toBeNull();
  });
});
