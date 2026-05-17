// frontend/web/src/features/agent-runs/SpanInspector.test.tsx
import { afterEach, describe, expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { SpanInspector } from "./SpanInspector";
import { useTraceDock } from "@/stores/trace-dock";
import type { RunSpan } from "@/api/types-agent-runs";

afterEach(() => {
  // Reset streaming slice + dock shell between tests so one test's
  // active-span set doesn't leak into the next.
  useTraceDock.getState().setActiveRun(null, "post-hoc");
});

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

  test("model.call without raw text shows hash-only preview + retention note", () => {
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          hash: "sha256:promptaaa",
          response_hash: "sha256:respbbb",
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByText("PROMPT")).toBeInTheDocument();
    expect(screen.getByText("RESPONSE")).toBeInTheDocument();
    expect(screen.getAllByText(/hash-only retention/i).length).toBeGreaterThan(0);
    // Hash also appears in the FIELDS table as response.hash.
    expect(screen.getByText("response.hash")).toBeInTheDocument();
  });

  test("model.call with payload refs surfaces them in preview + fields", () => {
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          prompt_payload_ref: "blob://prompts/p1",
          response_payload_ref: "blob://responses/r1",
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getAllByText("blob://prompts/p1").length).toBeGreaterThan(0);
    expect(screen.getAllByText("blob://responses/r1").length).toBeGreaterThan(0);
    expect(screen.getByText("prompt.ref")).toBeInTheDocument();
    expect(screen.getByText("response.ref")).toBeInTheDocument();
  });

  test("displays projected per-call provider + model from model_calls join", () => {
    render(
      <SpanInspector
        span={{ ...baseSpan, provider: "openai", model: "gpt-5-mini" }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByText("openai")).toBeInTheDocument();
    expect(screen.getByText("gpt-5-mini")).toBeInTheDocument();
  });

  test("live streaming model.call preempts the post-hoc RESPONSE hash fallback", () => {
    // Seed streaming state via the public reducers, exactly as the
    // SSE bridge would.
    useTraceDock.getState().markSpanActive(baseSpan.span_id, {
      name: baseSpan.name,
      started_at: baseSpan.started_at,
      kind: baseSpan.kind,
    });
    useTraceDock.getState().appendDelta(baseSpan.span_id, 421);

    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          // Both response artefacts would normally render the post-hoc
          // fallback; the streaming branch must preempt them.
          hash: "sha256:promptaaa",
          response_hash: "sha256:respbbb",
        }}
        isLive
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );

    const indicator = screen.getByTestId("span-inspector-streaming");
    expect(indicator.textContent).toMatch(/Streaming response/);
    expect(indicator.textContent).toMatch(/421 chars/);
    // The hash-only fallback must NOT be on screen while streaming.
    expect(
      screen.queryByText(/hash-only retention — completion body not stored on disk/i),
    ).not.toBeInTheDocument();
  });

  test("stream finish (span removed from activeSpanIds) restores hash/ref fallback", () => {
    useTraceDock.getState().markSpanActive(baseSpan.span_id, {
      name: baseSpan.name,
      started_at: baseSpan.started_at,
      kind: baseSpan.kind,
    });
    useTraceDock.getState().appendDelta(baseSpan.span_id, 100);
    // Simulate model_call_finished — the SSE bridge would invoke this.
    useTraceDock.getState().markSpanInactive(baseSpan.span_id);

    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          response_hash: "sha256:respbbb",
        }}
        isLive
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );

    expect(screen.queryByTestId("span-inspector-streaming")).not.toBeInTheDocument();
    expect(
      screen.getAllByText(/hash-only retention — completion body not stored on disk/i)
        .length,
    ).toBeGreaterThan(0);
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
