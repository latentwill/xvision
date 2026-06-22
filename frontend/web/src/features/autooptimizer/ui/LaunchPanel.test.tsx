import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { apiFetch } from "@/api/client";
import { listProviders } from "@/api/settings";
import { listStrategies } from "@/api/strategies";
import { LaunchPanel } from "./LaunchPanel";
import type * as ClientApiModule from "@/api/client";

vi.mock("@/api/client", async () => {
  const actual = await vi.importActual<typeof ClientApiModule>("@/api/client");
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

function renderLaunchPanel() {
  const client = new QueryClient({
    defaultOptions: {
      queries: { retry: false },
      mutations: { retry: false },
    },
  });
  return render(
    <QueryClientProvider client={client}>
      <LaunchPanel />
    </QueryClientProvider>,
  );
}

async function chooseStrategy(user: ReturnType<typeof userEvent.setup>, name: RegExp | string) {
  const optionName = typeof name === "string" ? new RegExp(name) : name;
  await user.click(screen.getByRole("button", { name: "Strategy" }));
  await user.click(await screen.findByRole("option", { name: optionName }));
}

describe("LaunchPanel", () => {
  it("renders the launch form with strategy picker and run button", async () => {
    renderLaunchPanel();

    expect(screen.getByText("Optimizer Run")).toBeInTheDocument();
    expect(await screen.findByLabelText("Strategy")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Run optimizer" })).toBeInTheDocument();
  });

  it("searches parent strategies by id before launching", async () => {
    vi.mocked(listStrategies).mockResolvedValue([
      {
        agent_id: "strategy-1",
        display_name: "Trend follower",
        template: "trend_follower",
        decision_cadence_minutes: 60,
      },
      {
        agent_id: "strategy-2",
        display_name: "Mean reversion",
        template: "mean_reversion",
        decision_cadence_minutes: 60,
      },
    ]);
    const user = userEvent.setup();

    renderLaunchPanel();

    const picker = await screen.findByRole("button", { name: /strategy/i });
    await user.click(picker);
    await user.type(screen.getByRole("textbox", { name: /search strategy/i }), "strategy-2");
    await user.click(screen.getByRole("option", { name: /Mean reversion/i }));

    expect(screen.getByRole("button", { name: /strategy/i })).toHaveTextContent(
      "Mean reversion",
    );
  });

  it("launches an optimizer run through the dashboard API", async () => {
    const user = userEvent.setup();
    renderLaunchPanel();

    await chooseStrategy(user, "Trend follower");
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

    const user = userEvent.setup();
    renderLaunchPanel();

    // Each override is a Signal dropdown; open them one at a time and confirm
    // the no-auth model is listed. (option names carry a trailing kind label.)
    const writer = await screen.findByLabelText("Experiment writer model override");
    await user.click(writer);
    expect(
      await screen.findByRole("option", { name: /qwen2\.5-coder:7b/ }),
    ).toBeInTheDocument();
    await user.click(writer); // close before opening the next

    const reviewer = screen.getByLabelText("Reviewer model override");
    await user.click(reviewer);
    expect(
      await screen.findByRole("option", { name: /qwen2\.5-coder:7b/ }),
    ).toBeInTheDocument();
    await user.click(reviewer);

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
    renderLaunchPanel();

    await chooseStrategy(user, "Trend follower");
    // The model overrides are Signal dropdowns: open, then click the option.
    const writer = screen.getByLabelText("Experiment writer model override");
    await user.click(writer);
    await user.click(await screen.findByRole("option", { name: /qwen2\.5-coder:7b/ }));
    const reviewer = screen.getByLabelText("Reviewer model override");
    await user.click(reviewer);
    await user.click(await screen.findByRole("option", { name: /qwen2\.5-coder:7b/ }));
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
    renderLaunchPanel();

    await chooseStrategy(user, "Trend follower");
    await waitFor(() => {
      expect(localStorage.getItem("autooptimizer_mutator_provider")).toBeNull();
      expect(localStorage.getItem("autooptimizer_mutator_model")).toBeNull();
      expect(localStorage.getItem("autooptimizer_judge_provider")).toBeNull();
      expect(localStorage.getItem("autooptimizer_judge_model")).toBeNull();
    });
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
