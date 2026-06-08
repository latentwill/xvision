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
    // Stub the historic-cycles list so `RecentCyclesSectionFull` gets an array
    // (the catch-all `{}` made `cycleRuns.map` throw — a pre-existing test gap).
    if (path === "/api/autooptimizer/cycles") return Promise.resolve([]);
    if (path === "/api/autooptimizer/run-defaults") {
      return Promise.resolve({
        mutator_provider: "anthropic",
        mutator_model: "claude-haiku-4-5",
        judge_provider: "anthropic",
        judge_model: "claude-haiku-4-5",
        config_path: "/tmp/xvn/autooptimizer.toml",
        config_exists: false,
      });
    }
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

function renderLiveCycleView(props: { activeTab?: string; launchOnly?: boolean } = {}) {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={client}>
      <LiveCycleView activeTab={props.activeTab} launchOnly={props.launchOnly} />
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

  it("streams live cost/tokens for the running cycle (F35.3)", async () => {
    vi.mocked(apiFetch).mockImplementation((path: string) => {
      if (path === "/api/autooptimizer/lineage") return Promise.resolve([]);
      if (path === "/api/autooptimizer/cycles") return Promise.resolve([]);
      if (path === "/api/autooptimizer/run-defaults") {
        return Promise.resolve({
          mutator_provider: "anthropic",
          mutator_model: "claude-haiku-4-5",
          judge_provider: "anthropic",
          judge_model: "claude-haiku-4-5",
          config_path: "/tmp/xvn/autooptimizer.toml",
          config_exists: false,
        });
      }
      if (path === "/api/autooptimizer/cycles/cycle-live/cost") {
        return Promise.resolve({
          cycle_id: "cycle-live",
          cost_usd: 0.1234,
          input_tokens: 1_935_625,
          output_tokens: 18_859,
          unpriced_calls: 0,
          recorded: true,
        });
      }
      return Promise.resolve({});
    });

    renderLiveCycleView();

    // Before a cycle starts, no live-spend strip.
    expect(screen.queryByText(/Live spend/i)).not.toBeInTheDocument();

    MockEventSource.instances[0].emit(
      "cycle_started",
      JSON.stringify({
        kind: "cycle_started",
        display_label: "Cycle started",
        data: { cycle_id: "cycle-live" },
      }),
    );

    // The ticker polls /cost for the running cycle and renders the spend.
    expect(await screen.findByText(/Live spend/i)).toBeInTheDocument();
    expect(await screen.findByText("$0.1234")).toBeInTheDocument();
    await waitFor(() =>
      expect(apiFetch).toHaveBeenCalledWith("/api/autooptimizer/cycles/cycle-live/cost"),
    );
  });

  it("launches an optimizer run through the dashboard API", async () => {
    const user = userEvent.setup();
    renderLiveCycleView({ launchOnly: true });

    await screen.findByRole("option", { name: "Trend follower" });
    await user.selectOptions(screen.getByLabelText("Strategy"), "strategy-1");
    await user.click(screen.getByRole("button", { name: "Run optimizer" }));

    // Assert the POST happened against the right path with the selected
    // strategy. Avoid pinning every optional field (F28 added budget/window,
    // future flags will add more) — strategy_id + method are the stable
    // contract; an exact-body match silently rots on each new launch option.
    await waitFor(() => {
      const call = vi
        .mocked(apiFetch)
        .mock.calls.find(([path]) => path === "/api/autooptimizer/run-cycle");
      expect(call, "run-cycle POST was issued").toBeTruthy();
      const init = call![1] as { method?: string; body?: string };
      expect(init.method).toBe("POST");
      expect(JSON.parse(init.body ?? "{}")).toMatchObject({ strategy_id: "strategy-1" });
    });
  });

  it("lists no-auth provider models in optimizer writer and reviewer pickers", async () => {
    vi.mocked(listProviders).mockResolvedValue({
      providers: [
        {
          name: "ollama",
          kind: "ollama",
          base_url: "http://localhost:11434",
          api_key_env: "",
          api_key_set: false,
          synthetic: false,
          is_default: false,
          enabled_models: ["llama3.2:latest", "qwen2.5-coder:7b"],
        },
      ],
      default_model: null,
    });

    renderLiveCycleView({ launchOnly: true });

    expect(await screen.findAllByRole("option", { name: "qwen2.5-coder:7b" })).toHaveLength(2);
    expect(screen.getByText(/No override uses built-in fallback/)).toBeInTheDocument();
    expect(screen.getByText(/No override reviews with built-in fallback/)).toBeInTheDocument();
  });

  it("launches with the visible optimizer model overrides", async () => {
    const user = userEvent.setup();
    vi.mocked(listProviders).mockResolvedValue({
      providers: [
        {
          name: "ollama",
          kind: "ollama",
          base_url: "http://localhost:11434",
          api_key_env: "",
          api_key_set: false,
          synthetic: false,
          is_default: false,
          enabled_models: ["qwen2.5-coder:7b"],
        },
      ],
      default_model: null,
    });
    renderLiveCycleView({ launchOnly: true });

    await screen.findByRole("option", { name: "Trend follower" });
    await user.selectOptions(
      screen.getByLabelText("Experiment writer model override"),
      "ollama::qwen2.5-coder:7b",
    );
    await user.selectOptions(
      screen.getByLabelText("Reviewer model override"),
      "ollama::qwen2.5-coder:7b",
    );
    await user.selectOptions(screen.getByLabelText("Strategy"), "strategy-1");
    await user.click(screen.getByRole("button", { name: "Run optimizer" }));

    await waitFor(() => {
      const call = vi
        .mocked(apiFetch)
        .mock.calls.find(([path]) => path === "/api/autooptimizer/run-cycle");
      const init = call?.[1] as { body?: string } | undefined;
      expect(JSON.parse(init?.body ?? "{}")).toMatchObject({
        strategy_id: "strategy-1",
        mutator_provider: "ollama",
        mutator_model: "qwen2.5-coder:7b",
        judge_provider: "ollama",
        judge_model: "qwen2.5-coder:7b",
      });
    });
  });

  it("does not render ActiveLineagesSectionFull on the default (home) tab", async () => {
    renderLiveCycleView();
    await waitFor(() =>
      expect(screen.queryByText("Active lineages")).not.toBeInTheDocument(),
    );
  });

  it("renders ActiveLineagesSectionFull when the genealogy tab is active", async () => {
    renderLiveCycleView({ activeTab: "genealogy" });
    await waitFor(() =>
      expect(screen.getByText("Active lineages")).toBeInTheDocument(),
    );
  });

  it("does not launch with stale stored optimizer model overrides absent from the picker", async () => {
    localStorage.setItem("autooptimizer_mutator_provider", "openrouter");
    localStorage.setItem("autooptimizer_mutator_model", "old/model");
    localStorage.setItem("autooptimizer_judge_provider", "openrouter");
    localStorage.setItem("autooptimizer_judge_model", "old/judge");
    const user = userEvent.setup();
    vi.mocked(listProviders).mockResolvedValue({
      providers: [
        {
          name: "ollama",
          kind: "ollama",
          base_url: "http://localhost:11434",
          api_key_env: "",
          api_key_set: false,
          synthetic: false,
          is_default: false,
          enabled_models: ["qwen2.5-coder:7b"],
        },
      ],
      default_model: null,
    });
    renderLiveCycleView({ launchOnly: true });

    await screen.findByRole("option", { name: "Trend follower" });
    await waitFor(() => {
      expect(localStorage.getItem("autooptimizer_mutator_provider")).toBeNull();
      expect(localStorage.getItem("autooptimizer_mutator_model")).toBeNull();
      expect(localStorage.getItem("autooptimizer_judge_provider")).toBeNull();
      expect(localStorage.getItem("autooptimizer_judge_model")).toBeNull();
    });
    await user.selectOptions(screen.getByLabelText("Strategy"), "strategy-1");
    await user.click(screen.getByRole("button", { name: "Run optimizer" }));

    await waitFor(() => {
      const call = vi
        .mocked(apiFetch)
        .mock.calls.find(([path]) => path === "/api/autooptimizer/run-cycle");
      const init = call?.[1] as { body?: string } | undefined;
      expect(JSON.parse(init?.body ?? "{}")).toMatchObject({
        strategy_id: "strategy-1",
        mutator_provider: null,
        mutator_model: null,
        judge_provider: null,
        judge_model: null,
      });
    });
  });
});
