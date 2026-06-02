import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { LiveCycleView } from "./LiveCycleView";

type Listener = (event: MessageEvent) => void;

class MockEventSource {
  static instances: MockEventSource[] = [];

  readonly url: string;
  private listeners = new Map<string, Set<Listener>>();

  constructor(url: string) {
    this.url = url;
    MockEventSource.instances.push(this);
  }

  addEventListener(type: string, listener: Listener) {
    const existing = this.listeners.get(type) ?? new Set<Listener>();
    existing.add(listener);
    this.listeners.set(type, existing);
  }

  removeEventListener(type: string, listener: Listener) {
    this.listeners.get(type)?.delete(listener);
  }

  close() {}

  emit(type: string, data: string) {
    for (const listener of this.listeners.get(type) ?? []) {
      listener(new MessageEvent(type, { data }));
    }
  }
}

const fetchMock = vi.fn();

beforeEach(() => {
  MockEventSource.instances = [];
  fetchMock.mockImplementation(async (input: RequestInfo | URL, init?: RequestInit) => {
    const url = typeof input === "string" ? input : input.toString();
    if (url === "/api/autooptimizer/lineage") {
      return jsonResponse([]);
    }
    if (url === "/api/strategies") {
      return jsonResponse({
        items: [
          {
            agent_id: "example-trend-follower",
            display_name: "Example Trend Follower",
            template: "mechanistic",
            decision_cadence_minutes: 60,
          },
        ],
        total: 1,
      });
    }
    if (url === "/api/settings/providers") {
      return jsonResponse({
        providers: [
          {
            name: "anthropic",
            kind: "anthropic",
            base_url: "",
            api_key_env: "ANTHROPIC_API_KEY",
            api_key_set: true,
            synthetic: false,
            enabled_models: ["claude-haiku-4-5-20251001", "claude-sonnet-4-6"],
          },
        ],
      });
    }
    if (url === "/api/autooptimizer/evening-cycle" && init?.method === "POST") {
      return jsonResponse(
        {
          started: true,
          message: "Evening run started. Watch the Live tab for progress.",
        },
        202,
      );
    }
    return jsonResponse({ code: "not_found", message: url }, 404);
  });
  vi.stubGlobal("fetch", fetchMock);
  Object.defineProperty(globalThis, "EventSource", {
    configurable: true,
    writable: true,
    value: MockEventSource,
  });
  Object.defineProperty(window, "EventSource", {
    configurable: true,
    writable: true,
    value: MockEventSource,
  });
  localStorage.clear();
});

afterEach(() => {
  vi.unstubAllGlobals();
});

function renderLiveCycleView() {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={client}>
      <LiveCycleView />
    </QueryClientProvider>,
  );
}

describe("LiveCycleView", () => {
  it("renders named and nested cycle events from the optimizer SSE stream", async () => {
    renderLiveCycleView();

    expect(screen.getByText(/Waiting for cycle/)).toBeInTheDocument();
    expect(MockEventSource.instances[0]?.url).toBe("/api/autooptimizer/events");

    MockEventSource.instances[0].emit(
      "cycle_started",
      JSON.stringify({
        kind: "cycle_started",
        display_label: "Cycle started",
        data: { cycle_id: "cycle-1" },
      }),
    );

    expect(await screen.findByRole("log")).toBeInTheDocument();
    expect(screen.getByText("Cycle started")).toBeInTheDocument();
    expect(screen.getByText("cycle-1")).toBeInTheDocument();
  });

  it("launches an evening run through the autooptimizer REST endpoint", async () => {
    const user = userEvent.setup();
    renderLiveCycleView();

    await screen.findByRole("option", { name: "Example Trend Follower" });
    await user.selectOptions(
      screen.getByLabelText("Parent strategy"),
      "example-trend-follower",
    );
    await screen.findAllByRole("option", { name: "claude-haiku-4-5-20251001" });
    await user.selectOptions(
      screen.getByLabelText("Experiment writer model"),
      "anthropic::claude-haiku-4-5-20251001",
    );
    await user.selectOptions(
      screen.getByLabelText("Reviewer model"),
      "anthropic::claude-sonnet-4-6",
    );
    await user.click(screen.getByRole("button", { name: "Start evening run" }));

    await screen.findByText("Evening run started. Watch the Live tab for progress.");
    const post = fetchMock.mock.calls.find(
      ([url, init]) => url === "/api/autooptimizer/evening-cycle" && init?.method === "POST",
    );
    expect(post).toBeDefined();
    expect(JSON.parse(String(post![1]?.body))).toEqual({
      strategy_id: "example-trend-follower",
      mutator_provider: "anthropic",
      mutator_model: "claude-haiku-4-5-20251001",
      judge_provider: "anthropic",
      judge_model: "claude-sonnet-4-6",
    });
  });

  it("keeps the start button disabled until a strategy is selected", async () => {
    renderLiveCycleView();

    const button = await screen.findByRole("button", { name: "Start evening run" });
    expect(button).toBeDisabled();

    await screen.findByRole("option", { name: "Example Trend Follower" });
    await userEvent.selectOptions(
      screen.getByLabelText("Parent strategy"),
      "example-trend-follower",
    );
    await waitFor(() => expect(button).toBeEnabled());
  });
});

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json" },
  });
}
