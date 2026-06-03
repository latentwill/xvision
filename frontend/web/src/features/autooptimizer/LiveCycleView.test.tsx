import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { apiFetch } from "@/api/client";
import { listProviders } from "@/api/settings";
import { listStrategies } from "@/api/strategies";
import { LiveCycleView } from "./LiveCycleView";

vi.mock("@/api/client", async () => {
  const actual = await vi.importActual<typeof import("@/api/client")>("@/api/client");
  return {
    ...actual,
    apiFetch: vi.fn(),
  };
});

vi.mock("@/api/settings", () => ({
  settingsKeys: { providers: () => ["settings", "providers"] },
  listProviders: vi.fn(),
}));

vi.mock("@/api/strategies", () => ({
  strategyKeys: { list: () => ["strategies", "list"] },
  listStrategies: vi.fn(),
}));

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

beforeEach(() => {
  vi.resetAllMocks();
  MockEventSource.instances = [];
  localStorage.clear();
  Element.prototype.scrollIntoView = vi.fn();
  vi.mocked(apiFetch).mockImplementation((path: string) => {
    if (path === "/api/autooptimizer/lineage") return Promise.resolve([]);
    if (path === "/api/autooptimizer/run-cycle") {
      return Promise.resolve({
        started: true,
        message: "Optimizer run started.",
      });
    }
    return Promise.resolve({});
  });
  vi.mocked(listStrategies).mockResolvedValue([
    {
      agent_id: "strategy-1",
      display_name: "Trend follower",
      template: "trend_follower",
      decision_cadence_minutes: 60,
    },
  ]);
  vi.mocked(listProviders).mockResolvedValue({
    providers: [
      {
        name: "anthropic",
        kind: "anthropic",
        base_url: "",
        api_key_env: "ANTHROPIC_API_KEY",
        api_key_set: true,
        synthetic: false,
        is_default: true,
        enabled_models: ["claude-sonnet-4-6"],
      },
    ],
    default_model: null,
  });
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
  it("renders named cycle events from the optimizer SSE stream", async () => {
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

  it("renders dashboard SSE envelope events from the optimizer stream", async () => {
    renderLiveCycleView();

    MockEventSource.instances[0].emit(
      "message",
      JSON.stringify({
        kind: "mutation_gated",
        display_label: "Gate evaluated",
        data: {
          type: "mutation_gated",
          cycle_id: "cycle-2",
          child_hash: "child-1",
          passed: false,
        },
      }),
    );

    expect(await screen.findByText("Gate evaluated")).toBeInTheDocument();
    expect(screen.getByText("cycle-2")).toBeInTheDocument();
  });

  it("launches an optimizer run through the dashboard API", async () => {
    const user = userEvent.setup();
    renderLiveCycleView();

    await screen.findByRole("option", { name: "Trend follower" });
    await user.selectOptions(screen.getByLabelText("Strategy"), "strategy-1");
    await user.click(screen.getByRole("button", { name: "Run optimizer" }));

    await waitFor(() =>
      expect(apiFetch).toHaveBeenCalledWith("/api/autooptimizer/run-cycle", {
        method: "POST",
        body: JSON.stringify({
          strategy_id: "strategy-1",
          mutator_model: "claude-haiku-4-5-20251001",
          judge_model: "claude-sonnet-4-6",
        }),
      }),
    );
  });
});
