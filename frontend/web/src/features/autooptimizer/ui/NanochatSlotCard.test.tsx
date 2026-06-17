import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { createElement, type ReactNode } from "react";
import { NanochatSlotCard } from "./NanochatSlotCard";
import type { NanochatCheckpoint } from "@/api/nanochat";

afterEach(() => cleanup());

// input_spec is a RAW JSON STRING on the wire — use JSON.stringify here
// (the spec plan erroneously shows it as an object; fixed per WU-8.3 directive).
const CANDIDATE_CHECKPOINT: NanochatCheckpoint = {
  model_id: "mod-candidate",
  display_name: "Strat A — jun14a — acc 0.55",
  source_strategy_id: "strat-1",
  source_strategy_name: "Strat A",
  run_tag: "jun14a",
  checkpoint_path: "/models/mod-candidate",
  weights_format: "safetensors",
  weights_sha256: "abc",
  input_spec: JSON.stringify({ window_bars: 64, indicators: ["rsi_14", "atr_20"], normalization: "zscore" }),
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
};

const APPROVED_CHECKPOINT: NanochatCheckpoint = {
  ...CANDIDATE_CHECKPOINT,
  model_id: "mod-approved",
  live_approved: true,
};

vi.mock("@/api/nanochat", async () => {
  const actual = await vi.importActual<typeof import("@/api/nanochat")>(
    "@/api/nanochat",
  );
  return {
    ...actual,
    useNanochatCheckpoints: vi.fn(() => ({
      data: [CANDIDATE_CHECKPOINT, APPROVED_CHECKPOINT],
      isLoading: false,
    })),
    useApproveCheckpoint: vi.fn(() => ({
      mutateAsync: vi.fn().mockResolvedValue({ model_id: "mod-candidate", live_approved: true }),
      isPending: false,
    })),
  };
});

function makeWrapper(route = "/") {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return ({ children }: { children: ReactNode }) =>
    createElement(
      QueryClientProvider,
      { client: qc },
      createElement(MemoryRouter, { initialEntries: [route] }, children),
    );
}

const DEFAULT_PROPS = {
  strategyId: "strat-1",
  agentRefRole: "nanochat",
  availableIndicators: ["rsi_14", "atr_20"],
  checkpointModelId: null as string | null,
  veto: true,
  onCheckpointChange: vi.fn(),
  onVetoChange: vi.fn(),
};

describe("NanochatSlotCard — candidate badge", () => {
  it("shows 'Candidate — backtest before use' badge for non-live_approved checkpoint", async () => {
    render(
      <NanochatSlotCard
        {...DEFAULT_PROPS}
        checkpointModelId="mod-candidate"
      />,
      { wrapper: makeWrapper() },
    );
    expect(await screen.findByText(/candidate.*backtest before use/i)).toBeInTheDocument();
  });

  it("does NOT show candidate badge for live_approved checkpoint", async () => {
    render(
      <NanochatSlotCard
        {...DEFAULT_PROPS}
        checkpointModelId="mod-approved"
      />,
      { wrapper: makeWrapper() },
    );
    await waitFor(() => {
      expect(screen.queryByText(/candidate.*backtest before use/i)).toBeNull();
    });
  });
});

describe("NanochatSlotCard — compatibility badge + inline error", () => {
  it("shows green compatibility badge when all indicators are present", async () => {
    render(
      <NanochatSlotCard
        {...DEFAULT_PROPS}
        checkpointModelId="mod-approved"
        availableIndicators={["rsi_14", "atr_20"]}
      />,
      { wrapper: makeWrapper() },
    );
    expect(await screen.findByText(/compatible/i)).toBeInTheDocument();
  });

  it("shows red badge and lists missing indicators when strategy lacks required indicators", async () => {
    render(
      <NanochatSlotCard
        {...DEFAULT_PROPS}
        checkpointModelId="mod-approved"
        availableIndicators={["rsi_14"]} // atr_20 missing
      />,
      { wrapper: makeWrapper() },
    );
    expect(await screen.findByText(/incompatible/i)).toBeInTheDocument();
    // atr_20 appears in both the missing-indicators <ul> and the remediation
    // <ol>; scope to the first <ul> (the missing-indicators bullet list).
    const lists = await screen.findAllByRole("list");
    const missingList = lists.find((el) => el.tagName === "UL")!;
    expect(within(missingList).getByText(/atr_20/i)).toBeInTheDocument();
  });

  it("lists the three remediation options inline beside the error", async () => {
    render(
      <NanochatSlotCard
        {...DEFAULT_PROPS}
        checkpointModelId="mod-approved"
        availableIndicators={[]}
      />,
      { wrapper: makeWrapper() },
    );
    expect(await screen.findByText(/add.*to this strategy/i)).toBeInTheDocument();
    expect(await screen.findByText(/pick a different checkpoint/i)).toBeInTheDocument();
    expect(await screen.findByText(/remove the nanochat slot/i)).toBeInTheDocument();
  });

  it("calls onCompatibilityChange(false) while compatibility is red", async () => {
    const onCompatibilityChange = vi.fn();
    render(
      <NanochatSlotCard
        {...DEFAULT_PROPS}
        checkpointModelId="mod-approved"
        availableIndicators={[]}
        onCompatibilityChange={onCompatibilityChange}
      />,
      { wrapper: makeWrapper() },
    );
    await waitFor(() => {
      expect(onCompatibilityChange).toHaveBeenCalledWith(false);
    });
  });
});

describe("NanochatSlotCard — ?attach_checkpoint deep-link", () => {
  it("pre-selects the checkpoint from ?attach_checkpoint= search param", async () => {
    const onCheckpointChange = vi.fn();
    render(
      <NanochatSlotCard
        {...DEFAULT_PROPS}
        checkpointModelId={null}
        onCheckpointChange={onCheckpointChange}
      />,
      { wrapper: makeWrapper("/?attach_checkpoint=mod-approved") },
    );
    await waitFor(() => {
      expect(onCheckpointChange).toHaveBeenCalledWith("mod-approved");
    });
  });
});

describe("NanochatSlotCard — no right-side-box layout (CLAUDE.md rule)", () => {
  it("renders as a single-column card (no grid-cols-12 container)", () => {
    render(
      <NanochatSlotCard {...DEFAULT_PROPS} />,
      { wrapper: makeWrapper() },
    );
    // The card is an inline block (space-y-* / flex-col) — not a 12-column grid.
    const gridContainer = document.querySelector(".grid-cols-12");
    expect(gridContainer, "NanochatSlotCard must not render a grid-cols-12 container").toBeNull();
  });
});
