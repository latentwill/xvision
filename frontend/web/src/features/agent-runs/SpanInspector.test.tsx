// frontend/web/src/features/agent-runs/SpanInspector.test.tsx
import { afterEach, describe, expect, test, vi } from "vitest";
import { render as rtlRender, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import type { ReactElement } from "react";
import { SpanInspector, promptPlaceholderReason } from "./SpanInspector";
import { useTraceDock } from "@/stores/trace-dock";
import { agentRunKeys } from "@/api/agent-runs";
import type {
  AgentRunDetail,
  AgentRunSummary,
  RetentionMode,
  RunSpan,
} from "@/api/types-agent-runs";

/**
 * Render helper that wires a fresh QueryClient + Provider around the
 * inspector. SpanInspector reads `retention_mode` off the cached run
 * detail via `useQueryClient`, so a Provider is mandatory; seeding
 * the cache here lets us assert the retention-aware placeholder copy.
 */
function render(
  ui: ReactElement,
  options: { activeRunId?: string; retentionMode?: RetentionMode } = {},
) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (options.activeRunId) {
    // SpanInspector now derives its scope from the route; the default
    // MemoryRouter path "/" maps to the eval scope, so seed eval.
    useTraceDock.getState().setActiveRun("eval", options.activeRunId, "post-hoc");
    if (options.retentionMode) {
      const detail = mockRunDetail(options.activeRunId, options.retentionMode);
      qc.setQueryData(agentRunKeys.run(options.activeRunId), detail);
    }
  }
  // SpanInspector reads `useCurrentTraceScope()` (→ `useLocation`), so a
  // Router is mandatory. Default path "/" → eval scope.
  return rtlRender(
    <QueryClientProvider client={qc}>
      <MemoryRouter>{ui}</MemoryRouter>
    </QueryClientProvider>,
  );
}

function mockRunDetail(runId: string, retention: RetentionMode): AgentRunDetail {
  const summary: AgentRunSummary = {
    run_id: runId,
    objective: "test",
    strategy_id: null,
    agent_id: null,
    started_at: "2026-05-17T10:00:00.000Z",
    finished_at: "2026-05-17T10:00:01.000Z",
    status: "completed",
    span_count: 1,
    model_call_count: 1,
    tool_call_count: 0,
    error_count: 0,
    total_cost_usd: 0,
    total_input_tokens: 0,
    total_output_tokens: 0,
    duration_ms: 1000,
    financial_eval_id: null,
    retention_mode: retention,
  };
  return { summary, spans: [], model_calls: [], tool_calls: [] };
}

afterEach(() => {
  // Reset streaming slice + dock shell between tests so one test's
  // active-span set doesn't leak into the next.
  useTraceDock.getState().setActiveRun("eval", null, "post-hoc");
  vi.restoreAllMocks();
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

describe("promptPlaceholderReason", () => {
  test("full_debug → re-run to capture", () => {
    expect(promptPlaceholderReason("full_debug")).toBe(
      "prompt body not captured for this run — re-run to capture",
    );
  });

  test("redacted → redacted-mode notice", () => {
    expect(promptPlaceholderReason("redacted")).toBe(
      "redacted retention — prompt body suppressed",
    );
  });

  test("hash_only → historical hash-only copy", () => {
    expect(promptPlaceholderReason("hash_only")).toBe(
      "hash-only retention — prompt body not stored on disk",
    );
  });

  test("undefined → neutral fallback (doesn't lie about mode)", () => {
    expect(promptPlaceholderReason(undefined)).toBe("prompt body not stored on disk");
  });
});

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

  // WS-11b — the nested eval-run node renders a navigable drill-link to the
  // candidate's persisted eval-run trace.
  test("opti.eval-run span renders a navigable link to /agent-runs/:runId", () => {
    const evalRunSpan: RunSpan = {
      span_id: "opti-evalrun:cyc1:child1",
      parent_span_id: "opti-exp:cyc1:child1",
      name: "Eval run",
      kind: "opti.eval-run",
      started_at: "2026-06-14T10:00:02.000Z",
      finished_at: "2026-06-14T10:00:02.000Z",
      status: "ok",
      attributes: { eval_run_id: "01EVALRUNULID", child_hash: "child1" },
    };
    render(
      <SpanInspector
        span={evalRunSpan}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    const link = screen.getByTestId("span-inspector-eval-run-link");
    expect(link).toHaveAttribute("href", "/agent-runs/01EVALRUNULID");
    expect(link).toHaveTextContent("View eval-run trace");
  });

  test("opti.eval-run span without an eval_run_id renders a muted fallback (no broken link)", () => {
    const evalRunSpan: RunSpan = {
      span_id: "opti-evalrun:cyc1:child2",
      parent_span_id: "opti-exp:cyc1:child2",
      name: "Eval run",
      kind: "opti.eval-run",
      started_at: "2026-06-14T10:00:02.000Z",
      finished_at: "2026-06-14T10:00:02.000Z",
      status: "ok",
      attributes: {},
    };
    render(
      <SpanInspector
        span={evalRunSpan}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.queryByTestId("span-inspector-eval-run-link")).toBeNull();
    expect(screen.getByTestId("span-inspector-eval-run-missing")).toBeInTheDocument();
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
    // qa-ui-polish-round2 #9: the streaming indicator is canonical at the
    // PullQuote header — exactly one "STREAMING" label per active span,
    // not the prior header-pill + pull-quote-pill duplicate.
    expect(screen.getAllByText(/STREAMING/)).toHaveLength(1);
  });

  test("legacy span.streaming=true (no SSE entry, no partial body) still shows one STREAMING indicator", () => {
    // Regression for PR #264 review: the dedupe removed the header
    // pill that covered the legacy `span.streaming` path. A live
    // model.call span with `streaming: true` but no
    // `streamingState.activeSpanIds` entry and no `response_partial`
    // must still render a single streaming indicator via the body-
    // level PullQuote (the "RESPONSE" placeholder with the streaming
    // header pill + animated caret).
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          response: undefined,
          response_partial: undefined,
          streaming: true,
        }}
        isLive
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getAllByText(/STREAMING/)).toHaveLength(1);
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

  test("model.call without raw text shows hash-only preview + retention note (hash_only mode)", () => {
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
      { activeRunId: "run-hashonly", retentionMode: "hash_only" },
    );
    expect(screen.getByText("PROMPT")).toBeInTheDocument();
    expect(screen.getByText("RESPONSE")).toBeInTheDocument();
    const reason = screen.getByTestId("span-inspector-prompt-placeholder-reason");
    expect(reason.textContent).toMatch(/hash-only retention/i);
    expect(screen.getByText("response.hash")).toBeInTheDocument();
  });

  test("full_debug run with no prompt_payload_ref shows 're-run to capture' notice — not 'hash-only'", () => {
    // Operator 2026-05-18: "prompts still redacted despite full_debug
    // while responses appear". Until the producer-side payload-write
    // fix lands (queue note), prompts have no payload_ref. The
    // placeholder must NOT lie that retention is hash-only when
    // the run was configured for full_debug.
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
      { activeRunId: "run-fulldebug", retentionMode: "full_debug" },
    );
    const reason = screen.getByTestId("span-inspector-prompt-placeholder-reason");
    expect(reason.textContent).toMatch(/re-run to capture/i);
    expect(reason.textContent).not.toMatch(/hash-only/i);
  });

  test("redacted retention shows the redacted-mode notice", () => {
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          hash: "sha256:promptaaa",
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
      { activeRunId: "run-redacted", retentionMode: "redacted" },
    );
    const reason = screen.getByTestId("span-inspector-prompt-placeholder-reason");
    expect(reason.textContent).toMatch(/redacted retention/i);
  });

  test("unknown / missing retention mode falls back to a neutral message", () => {
    // Cache miss path: render the inspector without seeding the
    // detail cache. The placeholder must not pick a specific mode
    // it can't verify.
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          hash: "sha256:promptaaa",
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    const reason = screen.getByTestId("span-inspector-prompt-placeholder-reason");
    expect(reason.textContent).toBe("prompt body not stored on disk");
    expect(reason.textContent).not.toMatch(/hash-only/i);
    expect(reason.textContent).not.toMatch(/re-run/i);
    expect(reason.textContent).not.toMatch(/redacted retention/i);
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
      screen.queryByTestId("span-inspector-response-placeholder"),
    ).not.toBeInTheDocument();
  });

  test("live streaming pull-quote renders accumulated delta_text body", () => {
    // When the SSE feed carries delta_text the inspector must render the
    // assistant body in the streaming pull-quote — not the chars-only
    // placeholder.
    useTraceDock.getState().markSpanActive(baseSpan.span_id, {
      name: baseSpan.name,
      started_at: baseSpan.started_at,
      kind: baseSpan.kind,
    });
    useTraceDock.getState().appendDelta(baseSpan.span_id, 5, "hello");
    useTraceDock.getState().appendDelta(baseSpan.span_id, 7, ", world");

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

    const body = screen.getByTestId("span-inspector-streaming-body");
    expect(body.textContent).toBe("hello, world");
    // The chars-only placeholder must not be on screen when we have
    // body text to show.
    expect(
      screen.queryByTestId("span-inspector-streaming"),
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
    // Response fallback renders the retention-aware placeholder. With
    // no retention mode seeded into the query cache, the copy uses the
    // neutral default — not the old hardcoded "hash-only retention" line.
    expect(screen.getByTestId("span-inspector-response-placeholder")).toBeInTheDocument();
    expect(
      screen.getByTestId("span-inspector-response-placeholder-reason").textContent,
    ).toMatch(/completion body not stored on disk/i);
  });

  test("post-stream-finish: accumulated body persists as RESPONSE pull-quote", () => {
    // The engine's post-hoc bridge in agent/execute.rs emits a final
    // delta carrying the full body, then immediately fires
    // model_call_finished. Without the post-hoc fallback the body
    // would flash briefly while the span was active and then vanish
    // when the hash/ref placeholder took over. The fix: when a span
    // has bodiesBySpan content, render it as the RESPONSE pull-quote
    // even after the live state has cleared.
    useTraceDock.getState().markSpanActive(baseSpan.span_id, {
      name: baseSpan.name,
      started_at: baseSpan.started_at,
      kind: baseSpan.kind,
    });
    useTraceDock.getState().appendDelta(baseSpan.span_id, 11, "hello world");
    useTraceDock.getState().markSpanInactive(baseSpan.span_id);

    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          // hash-only placeholder would normally render here; the
          // post-hoc body takes precedence.
          response_hash: "sha256:respccc",
        }}
        isLive
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );

    // Live pull-quote is gone (span no longer active).
    expect(
      screen.queryByTestId("span-inspector-streaming-body"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("span-inspector-streaming"),
    ).not.toBeInTheDocument();
    // Body persists as the post-hoc RESPONSE.
    const body = screen.getByTestId("span-inspector-posthoc-body");
    expect(body.textContent).toBe("hello world");
    // hash-only placeholder must NOT render alongside.
    expect(
      screen.queryByTestId("span-inspector-response-placeholder"),
    ).not.toBeInTheDocument();
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

  // qa-trace-error-surfacing (2026-05-17): operator complained that a
  // failed eval LLM call never surfaced in the trace. Once observability
  // is wired (engine half is a follow-up; see status doc), errored spans
  // must show their error prominently — badge in the header, pull-quote
  // at the top of the body.
  test("renders ERROR badge + pull-quote when span.status === 'error'", () => {
    const errored: RunSpan = {
      ...baseSpan,
      status: "error",
      error_message:
        "[unclassified] error decoding response body: EOF while parsing a value at line 1145 column 0",
    };
    render(
      <SpanInspector
        span={errored}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByTestId("span-error-badge")).toHaveTextContent("ERROR");
    expect(
      screen.getByText(/EOF while parsing a value at line 1145 column 0/),
    ).toBeInTheDocument();
  });

  test("ERROR badge replaces STREAMING when both are eligible", () => {
    // A span can be both errored and 'streaming' if the observability
    // path closes it as error mid-stream. The badge slot must pick
    // the error (more salient) over the streaming pulse.
    const erroredStreaming: RunSpan = {
      ...baseSpan,
      status: "error",
      error_message: "boom",
      streaming: true,
    };
    render(
      <SpanInspector
        span={erroredStreaming}
        isLive
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByTestId("span-error-badge")).toBeInTheDocument();
    // STREAMING badge must NOT also render (single badge slot).
    expect(screen.queryByText(/^STREAMING$/)).not.toBeInTheDocument();
  });

  test("renders no ERROR pull-quote when span has no error_message", () => {
    // Defensive: an errored span with no message attached (older
    // observability rows) should still render the badge but no body
    // pull-quote.
    const errored: RunSpan = {
      ...baseSpan,
      status: "error",
      error_message: undefined,
    };
    render(
      <SpanInspector
        span={errored}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    expect(screen.getByTestId("span-error-badge")).toBeInTheDocument();
    // The pull-quote message body must NOT render without a message.
    expect(screen.queryByText(/EOF while parsing/)).not.toBeInTheDocument();
  });

  test("expanding a payload-ref details block fetches the blob and shows the body", async () => {
    // Seed the dock so the inspector knows which run to query.
    useTraceDock.getState().setActiveRun("eval", "run_blob_ui", "post-hoc");

    const fetchSpy = vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response("PROMPT BODY TEXT", {
        status: 200,
        headers: { "content-type": "application/octet-stream" },
      }),
    );

    const ref = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          prompt_payload_ref: ref,
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );

    // Idle state pre-expand: ref visible, body not.
    const details = screen.getByTestId(
      "span-inspector-prompt-ref-details",
    ) as HTMLDetailsElement;
    expect(details.open).toBe(false);
    expect(screen.getAllByText(ref).length).toBeGreaterThan(0);
    expect(screen.queryByTestId("span-inspector-prompt-ref-body")).toBeNull();
    expect(fetchSpy).not.toHaveBeenCalled();

    // Expand → fetch fires.
    details.open = true;
    details.dispatchEvent(new Event("toggle"));

    await waitFor(() => {
      expect(
        screen.getByTestId("span-inspector-prompt-ref-body"),
      ).toBeInTheDocument();
    });
    expect(
      screen.getByTestId("span-inspector-prompt-ref-body"),
    ).toHaveTextContent("PROMPT BODY TEXT");
    expect(fetchSpy).toHaveBeenCalledTimes(1);
    const url = String(fetchSpy.mock.calls[0]?.[0] ?? "");
    expect(url).toBe(`/api/agent-runs/run_blob_ui/blobs/${ref}`);

    // Collapse + re-expand should NOT re-fetch (one-shot cache).
    details.open = false;
    details.dispatchEvent(new Event("toggle"));
    details.open = true;
    details.dispatchEvent(new Event("toggle"));
    expect(fetchSpy).toHaveBeenCalledTimes(1);
  });

  test("expanding a payload-ref details block surfaces 403 inline", async () => {
    useTraceDock.getState().setActiveRun("eval", "run_blob_403", "post-hoc");

    vi.spyOn(globalThis, "fetch").mockResolvedValue(
      new Response(
        JSON.stringify({
          code: "forbidden",
          message: "retention is hash_only — blob bodies are not stored on disk",
        }),
        { status: 403, headers: { "content-type": "application/json" } },
      ),
    );

    const ref = "ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          prompt: undefined,
          response: undefined,
          response_payload_ref: ref,
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    const details = screen.getByTestId(
      "span-inspector-response-ref-details",
    ) as HTMLDetailsElement;
    details.open = true;
    details.dispatchEvent(new Event("toggle"));

    const err = await screen.findByTestId("span-inspector-response-ref-error");
    expect(err.textContent).toMatch(/hash_only/);
  });

  test("broker.call span renders side / qty / fill / venue rows (filled)", () => {
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          span_id: "span_broker_filled",
          kind: "broker.call",
          name: "paper BTC/USD short",
          prompt: undefined,
          response: undefined,
          broker_call: {
            side: "short",
            symbol: "BTC/USD",
            qty: 0.1,
            intended_price: 60_000,
            order_type: "market",
            venue: "paper",
            idempotency_key: "run_42-0001",
            outcome: "filled",
            fill_price: 60_010,
            fill_qty: 0.1,
            fee: 0.01,
            broker_order_id: "ord_42",
            error_class: null,
            error_message: null,
            severity: null,
          },
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    const detail = screen.getByTestId("span-inspector-broker-call");
    expect(detail).toHaveTextContent(/short/i);
    expect(detail).toHaveTextContent("BTC/USD");
    expect(detail).toHaveTextContent("paper");
    expect(detail).toHaveTextContent("filled");
    expect(detail).toHaveTextContent("ord_42");
    expect(detail.textContent).toMatch(/60010\.0000/);
  });

  test("broker.call span renders error class + message on failed outcome", () => {
    render(
      <SpanInspector
        span={{
          ...baseSpan,
          span_id: "span_broker_failed",
          kind: "broker.call",
          name: "paper BTC/USD buy",
          prompt: undefined,
          response: undefined,
          broker_call: {
            side: "buy",
            symbol: "BTC/USD",
            qty: 0.5,
            intended_price: 60_000,
            order_type: "market",
            venue: "alpaca-paper",
            idempotency_key: "run_99-0007",
            outcome: "failed",
            fill_price: null,
            fill_qty: null,
            fee: null,
            broker_order_id: null,
            error_class: "broker_insufficient_funds",
            error_message: "alpaca create_order: insufficient buying power",
            severity: "warn",
          },
        }}
        isLive={false}
        onRerun={() => {}}
        onJumpToDecision={() => {}}
      />,
    );
    const detail = screen.getByTestId("span-inspector-broker-call");
    expect(detail).toHaveTextContent("broker_insufficient_funds");
    expect(detail).toHaveTextContent(/insufficient buying power/);
    expect(detail).toHaveTextContent("failed");
    // agent-error-feedback-self-healing: severity=warn surfaces the
    // self-healing posture so the operator sees the run wasn't
    // killed by a recoverable error.
    expect(
      screen.getByTestId("span-inspector-broker-severity"),
    ).toHaveTextContent(/warn — agent received feedback/);
  });
});
