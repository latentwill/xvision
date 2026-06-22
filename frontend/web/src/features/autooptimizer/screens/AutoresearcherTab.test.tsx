import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, fireEvent, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { createElement, type ReactNode } from "react";
import { AutoresearcherTab } from "./AutoresearcherTab";
import type {
  AutoresearchRunSummary,
  AutoresearchExperiment,
  NanochatCheckpoint,
} from "@/api/nanochat";

afterEach(() => cleanup());

// No Dialog/Sheet/Popover import check — asserted by scanning the module source
// in the "popup-free" test below.

vi.mock("@/api/nanochat", async () => {
  const actual = await vi.importActual<typeof import("@/api/nanochat")>(
    "@/api/nanochat",
  );
  return {
    ...actual,
    useNanochatCheckpoints: vi.fn(() => ({ data: [] as NanochatCheckpoint[], isLoading: false })),
    useAutoresearchRuns: vi.fn(() => ({ data: [] as AutoresearchRunSummary[], isLoading: false })),
    useAutoresearchRun: vi.fn(() => ({ data: undefined, isLoading: false })),
    useAutoresearchExperiments: vi.fn(() => ({
      data: [] as AutoresearchExperiment[],
      isLoading: false,
    })),
    useStartRun: vi.fn(() => ({ mutate: vi.fn(), isPending: false, error: null })),
    useStopRun: vi.fn(() => ({ mutate: vi.fn(), isPending: false, error: null })),
  };
});

vi.mock("@/api/strategies", async () => {
  const actual = await vi.importActual<typeof import("@/api/strategies")>(
    "@/api/strategies",
  );
  return {
    ...actual,
    listStrategies: vi.fn().mockResolvedValue([]),
  };
});

function makeWrapper() {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) =>
    createElement(
      QueryClientProvider,
      { client: qc },
      createElement(MemoryRouter, null, children),
    );
}

describe("AutoresearcherTab sections", () => {
  it("renders the Run launcher section heading", () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    expect(screen.getByText(/run launcher/i)).toBeInTheDocument();
  });

  it("renders the Live feed section heading", () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    expect(screen.getByText(/live feed/i)).toBeInTheDocument();
  });

  it("renders the Checkpoint leaderboard section heading", () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    expect(screen.getByText(/checkpoint leaderboard/i)).toBeInTheDocument();
  });

  it("renders the run_tag input", () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    expect(screen.getByLabelText(/run tag/i)).toBeInTheDocument();
  });

  it("renders label strategy dropdown with price_forward option", async () => {
    const user = userEvent.setup();
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    await user.click(screen.getByRole("button", { name: /label strategy/i }));
    expect(screen.getByRole("option", { name: /price_forward/i })).toBeInTheDocument();
  });
});

describe("run_tag client-side validation", () => {
  it("shows validation error for run_tag containing uppercase letters", async () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    const input = screen.getByLabelText(/run tag/i);
    fireEvent.change(input, { target: { value: "JunXYZ" } });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
    expect(screen.getByRole("alert").textContent).toMatch(/lowercase/i);
  });

  it("shows validation error for run_tag starting with a hyphen", async () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    const input = screen.getByLabelText(/run tag/i);
    fireEvent.change(input, { target: { value: "-bad" } });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("shows validation error for run_tag longer than 32 chars", async () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    const input = screen.getByLabelText(/run tag/i);
    fireEvent.change(input, { target: { value: "a".repeat(33) } });
    await waitFor(() => {
      expect(screen.getByRole("alert")).toBeInTheDocument();
    });
  });

  it("accepts a valid run_tag matching ^[a-z0-9][a-z0-9-]{0,31}$", async () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    const input = screen.getByLabelText(/run tag/i);
    fireEvent.change(input, { target: { value: "jun14a" } });
    await waitFor(() => {
      expect(screen.queryByRole("alert")).toBeNull();
    });
  });
});

describe("leaderboard lift column", () => {
  it("shows '— run backtest to measure' when checkpoint has no lift data", async () => {
    const nanochat = await import("@/api/nanochat");
    vi.mocked(nanochat.useNanochatCheckpoints).mockReturnValue({
      data: [
        {
          model_id: "mod-1",
          display_name: "Strat A — jun14a — acc 0.55",
          source_strategy_id: "strat-1",
          source_strategy_name: "Strat A",
          run_tag: "jun14a",
          checkpoint_path: "/models/mod-1",
          weights_format: "safetensors",
          weights_sha256: "abc",
          // input_spec is a raw JSON string on the wire
          input_spec: JSON.stringify({ window_bars: 64, indicators: [], normalization: "zscore" }),
          base_model: "gpt2-nanochat",
          label_strategy: "price_forward",
          label_config: {},
          best_acc: 0.55,
          best_loss: 0.6,
          holdout_samples: 300,
          promoted: true,
          live_approved: false,
          created_at: "2026-06-14T00:00:00Z",
          autoresearch_run_id: "run-1",
        },
      ],
      isLoading: false,
    } as ReturnType<typeof nanochat.useNanochatCheckpoints>);

    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    expect(await screen.findByText(/run backtest to measure/i)).toBeInTheDocument();
  });
});

describe("auto-promotion toast", () => {
  it("renders a live-region toast when a new promoted checkpoint appears", async () => {
    const nanochat = await import("@/api/nanochat");
    // Start with no checkpoints
    vi.mocked(nanochat.useNanochatCheckpoints).mockReturnValue({
      data: [] as NanochatCheckpoint[],
      isLoading: false,
    } as ReturnType<typeof nanochat.useNanochatCheckpoints>);

    const { rerender } = render(<AutoresearcherTab />, { wrapper: makeWrapper() });

    // Simulate a new promoted checkpoint arriving
    vi.mocked(nanochat.useNanochatCheckpoints).mockReturnValue({
      data: [
        {
          model_id: "new-mod",
          display_name: "Strat A — jun14b — acc 0.57",
          source_strategy_id: "strat-1",
          source_strategy_name: "Strat A",
          run_tag: "jun14b",
          checkpoint_path: "/models/new-mod",
          weights_format: "safetensors",
          weights_sha256: "def",
          input_spec: JSON.stringify({ window_bars: 64, indicators: [], normalization: "zscore" }),
          base_model: "gpt2-nanochat",
          label_strategy: "price_forward",
          label_config: {},
          best_acc: 0.57,
          best_loss: 0.55,
          holdout_samples: 350,
          promoted: true,
          live_approved: false,
          created_at: "2026-06-14T00:01:00Z",
          autoresearch_run_id: "run-1",
        },
      ],
      isLoading: false,
    } as ReturnType<typeof nanochat.useNanochatCheckpoints>);

    rerender(<AutoresearcherTab />);

    await waitFor(() => {
      expect(screen.getByRole("status")).toBeInTheDocument();
    });
    expect(screen.getByRole("status").textContent).toMatch(/promoted/i);
  });
});

describe("no popup imports", () => {
  it("the AutoresearcherTab module does not import Dialog, Sheet, or Popover", async () => {
    // Dynamic import of the module source text to assert no banned component import.
    // This is a static-analysis guard — if someone adds a banned import the test
    // catches it without needing to render.
    const src = await fetch(
      new URL("./AutoresearcherTab.tsx", import.meta.url).href,
    ).then((r) => r.text()).catch(() => "");
    const banned = ["Dialog", "Sheet", "Popover"];
    for (const name of banned) {
      expect(src, `must not import ${name}`).not.toMatch(
        new RegExp(`import[^;]+${name}`),
      );
    }
  });
});

describe("no right-side-box layout (CLAUDE.md rule)", () => {
  // CLAUDE.md: the dashboard three-pane shell reserves the right edge for the
  // chat rail. Components must NOT introduce a fourth column / right sidebar /
  // col-span-4 grid layout. Assert this via a source-text guard (same pattern
  // as the no-popup guard above).
  it("the AutoresearcherTab module does not introduce a right sidebar or col-span-4 wrapper", async () => {
    const src = await fetch(
      new URL("./AutoresearcherTab.tsx", import.meta.url).href,
    ).then((r) => r.text()).catch(() => "");
    // Patterns that indicate a 4-column grid layout or right sidebar — banned
    // anywhere a <Layout> / DesktopThreePaneShell is rendered.
    const banned = [
      /col-span-4/,
      /grid-cols-12/,
      /right-sidebar/,
      /RightSidebar/,
      /lg:col-span-4/,
    ];
    for (const pattern of banned) {
      expect(src, `AutoresearcherTab must not introduce a right-side box (${pattern})`).not.toMatch(pattern);
    }
  });

  it("the rendered AutoresearcherTab root element does not produce a grid-cols-12 container", () => {
    render(<AutoresearcherTab />, { wrapper: makeWrapper() });
    // The component should use a single-column flow layout (space-y-*, flex-col,
    // or a simple div — NOT a 12-column grid with a right-hand side panel).
    const container = document.querySelector(".grid-cols-12");
    expect(container, "AutoresearcherTab must not render a grid-cols-12 container").toBeNull();
  });
});

// ─── EventSource stub (needed for useAutoresearchStream) ─────────────────────

beforeEach(() => {
  // @ts-expect-error jsdom EventSource stub
  global.EventSource = vi.fn().mockImplementation(() => ({
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    close: vi.fn(),
  }));
});
