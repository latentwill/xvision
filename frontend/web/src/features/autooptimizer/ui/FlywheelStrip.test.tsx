import { describe, expect, it, vi, afterEach } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { FlywheelStrip } from "./FlywheelStrip";
import * as apiModule from "../api";

afterEach(() => vi.restoreAllMocks());

const enabledBase = {
  enabled: true,
  cohort_count: 12,
  threshold: 20,
  compiled_pattern_count: 3,
  latest_optimization_run_id: "run_01ABC",
  last_prompt_compile: null,
};

describe("FlywheelStrip", () => {
  it("renders line 1 from a fixture enabled response", async () => {
    vi.spyOn(apiModule, "useFlywheel").mockReturnValue({
      data: enabledBase,
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useFlywheel>);

    renderWithProviders(<FlywheelStrip />);

    // cohort_count / threshold
    expect(await screen.findByText(/12\/20/)).toBeInTheDocument();
    // compiled pattern count
    expect(screen.getByText(/3 patterns compiled/)).toBeInTheDocument();
    // Observations label
    expect(screen.getByText(/Observations toward next prompt compile/i)).toBeInTheDocument();
  });

  it("renders line 2 with link when last_prompt_compile is set", async () => {
    vi.spyOn(apiModule, "useFlywheel").mockReturnValue({
      data: {
        ...enabledBase,
        last_prompt_compile: {
          dev_metric: "sharpe",
          parent_dev_score: 1.2,
          child_dev_score: 1.45,
          delta_dev: 0.25,
          parent_holdout_score: 1.1,
          child_holdout_score: 1.32,
          delta_holdout: 0.22,
          gate_verdict: "Accepted",
          gated_at: "2026-06-07T12:00:00Z",
        },
      },
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useFlywheel>);

    renderWithProviders(<FlywheelStrip />);

    // line 2 content
    expect(await screen.findByText(/Last prompt compile/i)).toBeInTheDocument();
    expect(screen.getByText(/\+0\.25/)).toBeInTheDocument();
    expect(screen.getByText(/\+0\.22/)).toBeInTheDocument();
    expect(screen.getByText(/Accepted/)).toBeInTheDocument();

    // link to /optimizations/:id
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "/optimizations/run_01ABC");
  });

  it("renders null when enabled=false", () => {
    vi.spyOn(apiModule, "useFlywheel").mockReturnValue({
      data: { enabled: false },
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useFlywheel>);

    const { container } = renderWithProviders(<FlywheelStrip />);

    // component should render nothing
    expect(container.firstChild).toBeNull();
  });

  it("renders gracefully when last_prompt_compile is null — no line 2", async () => {
    vi.spyOn(apiModule, "useFlywheel").mockReturnValue({
      data: { ...enabledBase, last_prompt_compile: null },
      isLoading: false,
      isError: false,
    } as unknown as ReturnType<typeof apiModule.useFlywheel>);

    renderWithProviders(<FlywheelStrip />);

    // line 1 present
    expect(await screen.findByText(/Observations toward next prompt compile/i)).toBeInTheDocument();
    // line 2 absent
    expect(screen.queryByText(/Last prompt compile/i)).toBeNull();
  });
});
