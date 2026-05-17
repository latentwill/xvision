// frontend/web/src/features/agent-runs/SpanInspector.test.tsx
import { describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { SpanInspector } from "./SpanInspector";
import type { RunSpan } from "@/api/types-agent-runs";

const baseSpan: RunSpan = {
  span_id: "s_test",
  parent_span_id: null,
  name: "model.call gpt-5",
  kind: "model.call",
  started_at: "2026-05-17T10:00:00.000Z",
  finished_at: "2026-05-17T10:00:00.720Z",
  status: "ok",
  attributes: {},
  prompt: "Explain mean reversion in one sentence.",
  response: "Mean reversion is the tendency for prices to return to their average.",
  provider: "anthropic",
  model: "claude-opus-4-7",
  tokens_in: 8412,
  tokens_out: 1204,
  cost: 0.0416,
  hash: "sha256:a1b2c3",
};

describe("SpanInspector (with pull-quotes)", () => {
  test("renders PROMPT and RESPONSE pull-quotes when present", () => {
    render(
      <SpanInspector
        span={baseSpan}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByText("PROMPT")).toBeInTheDocument();
    expect(screen.getByText(/Explain mean reversion/)).toBeInTheDocument();
    expect(screen.getByText("RESPONSE")).toBeInTheDocument();
    expect(screen.getByText(/tendency for prices/)).toBeInTheDocument();
  });

  test("renders TOOL ARGS as preformatted JSON", () => {
    render(
      <SpanInspector
        span={{ ...baseSpan, args: { symbol: "SPY", qty: 100 } }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByText("TOOL ARGS")).toBeInTheDocument();
    expect(screen.getByText(/"symbol": "SPY"/)).toBeInTheDocument();
  });

  test("RESPONSE (PARTIAL) shows STREAMING badge when live", () => {
    render(
      <SpanInspector
        span={{ ...baseSpan, response: undefined, response_partial: "Take a half-Kelly long…", streaming: true }}
        isLive
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByText("RESPONSE (PARTIAL)")).toBeInTheDocument();
    // There are two STREAMING markers when isLive+streaming: header pill and pull-quote header. At least one must appear.
    expect(screen.getAllByText(/STREAMING/).length).toBeGreaterThan(0);
  });

  test("rerun button shows `LOCKED · LIVE` and is disabled when isLive", () => {
    render(
      <SpanInspector
        span={baseSpan}
        isLive
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    const btn = screen.getByRole("button", { name: /rerun from here/i });
    expect(btn).toBeDisabled();
    expect(screen.getByText(/LOCKED · LIVE/)).toBeInTheDocument();
  });

  test("rerun button enabled when not live; clicking calls onRerun(span_id)", async () => {
    const { default: userEvent } = await import("@testing-library/user-event");
    const onRerun = vi.fn();
    render(
      <SpanInspector
        span={baseSpan}
        isLive={false}
        onRerun={onRerun}
        onJumpToDecision={() => {}}
      />,
    );
    const btn = screen.getByRole("button", { name: /rerun from here/i });
    expect(btn).not.toBeDisabled();
    await userEvent.click(btn);
    expect(onRerun).toHaveBeenCalledWith("s_test");
  });
});
