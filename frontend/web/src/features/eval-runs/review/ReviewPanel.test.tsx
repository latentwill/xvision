// Tests for ReviewPanel error surfacing.
//
// Regression coverage for `eval-review-400-diagnose`: when the
// `/api/eval/runs/:id/review` POST returns a 400 with a structured
// `{ code, message }` body (the `qa-review-agent-provider-config`
// /  #256 skip-with-remediation path), the panel must render BOTH the
// `code` discriminator and the operator-actionable `message` in a
// visible `role="alert"` block — not a tiny line of red text that
// reads as "click did nothing".

import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { GenerateErrorAlert, ReviewPanel } from "./ReviewPanel";
import { ApiError } from "@/api/client";

function renderPanel(runId = "run_test_1") {
  const qc = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={qc}>
      <ReviewPanel runId={runId} runIsCompleted />
    </QueryClientProvider>,
  );
}

/// Build a `fetch` mock that:
///   - returns an empty review-list for the GET `/reviews` calls so the
///     panel renders the "no review yet" baseline rather than blowing
///     up on the list query;
///   - returns the requested status + JSON body for the POST `/review`
///     call.
function installFetchMock(opts: {
  postStatus: number;
  postBody: unknown;
}) {
  const fetchMock = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    const method = init?.method ?? "GET";
    if (method === "POST" && url.endsWith("/review")) {
      return new Response(JSON.stringify(opts.postBody), {
        status: opts.postStatus,
        headers: { "content-type": "application/json" },
      });
    }
    if (method === "GET" && url.endsWith("/reviews")) {
      return new Response(JSON.stringify({ items: [] }), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    if (method === "GET" && url.endsWith("/eval/agent-profiles")) {
      return new Response(
        JSON.stringify({
          items: [
            {
              id: "reasoning-agent",
              name: "Reasoning",
              type: "review",
              provider: "openrouter",
              model: "anthropic/claude-sonnet-4.5",
              temperature: 0.2,
              max_tokens: 4096,
              system_prompt: "Review the run carefully.",
              enabled: true,
              created_at: "2026-05-23T00:00:00Z",
              updated_at: "2026-05-23T00:00:00Z",
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    if (method === "GET" && url.endsWith("/settings/providers")) {
      return new Response(
        JSON.stringify({
          providers: [
            {
              name: "openrouter",
              kind: "openai-compat",
              api_key_env: "OPENROUTER_KEY",
              api_key_set: true,
              base_url: "https://openrouter.ai/api/v1",
              enabled_models: ["anthropic/claude-sonnet-4.5"],
              synthetic: false,
              supports_browsing: false,
            },
          ],
          default_model: null,
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      );
    }
    // Unknown — return a 404 JSON so the failure surfaces as an
    // assertion error in the test rather than a generic fetch reject.
    return new Response(
      JSON.stringify({ code: "not_found", message: `no mock for ${method} ${url}` }),
      { status: 404, headers: { "content-type": "application/json" } },
    );
  });
  vi.stubGlobal("fetch", fetchMock);
  return fetchMock;
}

describe("GenerateErrorAlert", () => {
  afterEach(() => {
    cleanup();
  });

  test("renders ApiError code + message verbatim from the structured 400 body", () => {
    // Backend now emits `{ code, message, field }` with no `field: msg`
    // prefix embedded in `message` — see `crates/xvision-dashboard/src/error.rs`.
    // The alert renders `message` verbatim; no client-side massaging required.
    const err = new ApiError(
      400,
      "validation",
      "review skipped: agent profile `reasoning-agent` requires provider `anthropic` which is not configured in Settings → Providers (configured: openrouter). Add a compatible provider to run this review.",
      "request",
    );
    render(<GenerateErrorAlert error={err} />);
    const code = screen.getByTestId("review-generate-error-code");
    expect(code.textContent).toBe("validation");
    const message = screen.getByTestId("review-generate-error-message");
    expect(message.textContent).not.toMatch(/^request:/);
    expect(message.textContent).toMatch(/review skipped/);
    expect(message.textContent).toMatch(/anthropic/);
    expect(message.textContent).toMatch(/configured: openrouter/);
  });

  test("renders ApiError code + raw message for non-validation codes", () => {
    const err = new ApiError(500, "internal", "internal error");
    render(<GenerateErrorAlert error={err} />);
    expect(screen.getByTestId("review-generate-error-code").textContent).toBe(
      "internal",
    );
    expect(screen.getByTestId("review-generate-error-message").textContent).toBe(
      "internal error",
    );
  });

  test("falls back to 'error' code + message for plain Error", () => {
    render(<GenerateErrorAlert error={new Error("network is offline")} />);
    expect(screen.getByTestId("review-generate-error-code").textContent).toBe(
      "error",
    );
    expect(screen.getByTestId("review-generate-error-message").textContent).toBe(
      "network is offline",
    );
  });

  test("exposes role='alert' so it isn't visually overlooked", () => {
    render(<GenerateErrorAlert error={new ApiError(400, "validation", "x")} />);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });

  test("retry button calls onRetry when supplied", async () => {
    const onRetry = vi.fn();
    render(
      <GenerateErrorAlert
        error={new ApiError(400, "validation", "oops", "request")}
        onRetry={onRetry}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: /retry/i }));
    expect(onRetry).toHaveBeenCalledTimes(1);
  });
});

describe("ReviewPanel — copy completed review", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
  });

  test("copies completed review as Markdown and JSON", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal("navigator", {
      ...navigator,
      clipboard: { writeText },
    });
    vi.stubGlobal(
      "fetch",
      vi.fn(async (input: RequestInfo | URL) => {
        const url = typeof input === "string" ? input : input.toString();
        if (url.endsWith("/reviews")) {
          return new Response(
            JSON.stringify({
              items: [
                {
                  id: "review_1",
                  eval_run_id: "run_test_1",
                  agent_profile_id: "reasoning-agent",
                  status: "completed",
                  verdict: "promising",
                  confidence: 0.8,
                  score: 7,
                  summary: "Useful signal.",
                  raw_output_json: null,
                  error: null,
                  created_at: "2026-05-24T00:00:00Z",
                  updated_at: "2026-05-24T00:00:00Z",
                },
              ],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        if (url.endsWith("/api/eval/reviews/review_1")) {
          return new Response(
            JSON.stringify({
              review: {
                id: "review_1",
                eval_run_id: "run_test_1",
                agent_profile_id: "reasoning-agent",
                status: "completed",
                verdict: "promising",
                confidence: 0.8,
                score: 7,
                summary: "Useful signal.",
                raw_output_json: null,
                error: null,
                created_at: "2026-05-24T00:00:00Z",
                updated_at: "2026-05-24T00:00:00Z",
              },
              findings: [],
            }),
            { status: 200, headers: { "content-type": "application/json" } },
          );
        }
        return new Response(JSON.stringify({ message: `no mock for ${url}` }), {
          status: 404,
          headers: { "content-type": "application/json" },
        });
      }),
    );

    renderPanel();
    await screen.findByText("Useful signal.");

    await userEvent.click(screen.getByTestId("copy-review-md"));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining("# Eval Review review_1"));

    await userEvent.click(screen.getByTestId("copy-review-json"));
    expect(writeText).toHaveBeenLastCalledWith(expect.stringContaining('"id": "review_1"'));
  });
});

describe("ReviewPanel — operator-visible 400 surfacing", () => {
  beforeEach(() => {
    vi.useRealTimers();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    cleanup();
  });

  test("renders the structured 400 body from the review POST (#256 remediation copy)", async () => {
    installFetchMock({
      postStatus: 400,
      postBody: {
        code: "validation",
        message:
          "review skipped: agent profile `reasoning-agent` requires provider `anthropic` which is not configured in Settings → Providers (configured: openrouter). Add a compatible provider to run this review.",
        field: "request",
      },
    });

    renderPanel();

    // Wait for the empty-history baseline so we know the panel rendered
    // past the list query.
    await waitFor(() =>
      expect(screen.getByText(/No review yet for this run/i)).toBeInTheDocument(),
    );

    await screen.findByLabelText("Review prompt preset");
    const reasoningButton = screen.getByRole("button", {
      name: /generate review/i,
    });
    await act(async () => {
      await userEvent.click(reasoningButton);
    });

    // The structured error alert must render with both the code
    // discriminator AND the operator-facing message.
    const code = await screen.findByTestId("review-generate-error-code");
    expect(code.textContent).toBe("validation");
    const message = screen.getByTestId("review-generate-error-message");
    expect(message.textContent).toMatch(/review skipped/);
    expect(message.textContent).toMatch(/anthropic/);
    expect(message.textContent).toMatch(/Settings → Providers/);
    // Backend emits clean copy — the legacy `request: ` server-side
    // prefix is no longer attached to `message` (see
    // `crates/xvision-dashboard/src/error.rs`).
    expect(message.textContent).not.toMatch(/^request:/);
    expect(screen.getByRole("alert")).toBeInTheDocument();
  });
});
