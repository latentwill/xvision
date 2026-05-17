// frontend/web/src/features/agent-runs/RunStatusStrip.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RunStatusStrip } from "./RunStatusStrip";
import { MOCK_RUN_COMPLETED, MOCK_RUN_LIVE, MOCK_RUN_ERROR } from "./mock-fixtures";

describe("RunStatusStrip", () => {
  test("renders COMPLETED label and aggregates for a completed run", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_COMPLETED.summary}
        currentSpan={null}
        isLive={false}
        liveDurationSec={0}
        tone="completed"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByText(/COMPLETED/)).toBeInTheDocument();
    expect(screen.getByText(/spans/)).toBeInTheDocument();
  });

  test("LIVE tone shows ticking duration as m:ss", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_LIVE.summary}
        currentSpan={null}
        isLive
        liveDurationSec={43}
        tone="live"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByTestId("run-status-strip")).toHaveAttribute("data-tone", "live");
    expect(screen.getByText("0:43")).toBeInTheDocument();
  });

  test("error tone appends `1 error` pill", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_ERROR.summary}
        currentSpan={null}
        isLive={false}
        liveDurationSec={0}
        tone="error"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByText(/1 error/i)).toBeInTheDocument();
  });

  test("currentSpan chip renders kind label + truncated name + elapsed", () => {
    render(
      <RunStatusStrip
        summary={MOCK_RUN_COMPLETED.summary}
        currentSpan={{ name: "model.call gpt-5", color: "#7dd3fc", label: "MODEL", elapsedMs: 720 }}
        isLive={false}
        liveDurationSec={0}
        tone="completed"
        onExpand={() => {}}
        onPopOut={() => {}}
      />,
    );
    expect(screen.getByText("MODEL")).toBeInTheDocument();
    expect(screen.getByText(/model\.call gpt-5/)).toBeInTheDocument();
    expect(screen.getByText("720ms")).toBeInTheDocument();
  });

  test("Enter key on the strip body activates onExpand", async () => {
    const onExpand = vi.fn();
    render(
      <RunStatusStrip
        summary={MOCK_RUN_COMPLETED.summary}
        currentSpan={null}
        isLive={false}
        liveDurationSec={0}
        tone="completed"
        onExpand={onExpand}
        onPopOut={() => {}}
      />,
    );
    const strip = screen.getByTestId("run-status-strip");
    strip.focus();
    await userEvent.keyboard("{Enter}");
    expect(onExpand).toHaveBeenCalledOnce();
  });

  test("clicking the body calls onExpand; clicking pop-out calls onPopOut (no double-fire)", async () => {
    const onExpand = vi.fn();
    const onPopOut = vi.fn();
    render(
      <RunStatusStrip
        summary={MOCK_RUN_COMPLETED.summary}
        currentSpan={null}
        isLive={false}
        liveDurationSec={0}
        tone="completed"
        onExpand={onExpand}
        onPopOut={onPopOut}
      />,
    );
    await userEvent.click(screen.getByTestId("run-status-strip"));
    expect(onExpand).toHaveBeenCalledOnce();
    await userEvent.click(screen.getByLabelText(/open dedicated trace view/i));
    expect(onPopOut).toHaveBeenCalledOnce();
    expect(onExpand).toHaveBeenCalledOnce(); // unchanged — pop-out stopped propagation
  });
});
