import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import { ExpandableArtifact } from "./ExpandableArtifact";

// Mock the api module at module scope. All tests use this mock by default.
// Individual tests that need different return values call mockReturnValueOnce.
vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    useExperimentDetail: vi.fn(),
    // useBlob is used by ParentDiffPanel — return empty so tests don't need a real network.
    useBlob: vi.fn(() => ({ data: undefined, isLoading: false, isError: false })),
  };
});

// Import the mocked module AFTER vi.mock so we can reset return values per test.
import { useExperimentDetail } from "../api";

const mockDetail = {
  lineage_node: {
    bundle_hash: "abcd1234ef",
    parent_hash: "parenthash99",
    status: "active" as const,
    created_at: "2026-06-11T00:00:00Z",
  },
  rationale: "Tighten the stop to cut tail losses",
  gate_record: {
    bundle_hash: "abcd1234ef",
    parent_day_score: 1.2,
    child_day_score: 1.41,
    parent_holdout_score: 1.1,
    child_holdout_score: 1.31,
    gate_epsilon: 0.05,
    delta_day: 0.21,
    delta_holdout: 0.21,
    drawdown_ratio: null,
    verdict: "passed",
    reason: null,
    edge_over_random: null,
    parent_edge: null,
    edge_delta: null,
  },
  findings: [],
  regime_results: [],
};

function useDetailOk() {
  vi.mocked(useExperimentDetail).mockReturnValue({
    data: mockDetail,
    isLoading: false,
    isError: false,
    isPending: false,
    status: "success",
    error: null,
    isSuccess: true,
  } as ReturnType<typeof useExperimentDetail>);
}

function useDetailLoading() {
  vi.mocked(useExperimentDetail).mockReturnValue({
    data: undefined,
    isLoading: true,
    isError: false,
    isPending: true,
    status: "pending",
    error: null,
    isSuccess: false,
  } as ReturnType<typeof useExperimentDetail>);
}

function useDetailError() {
  vi.mocked(useExperimentDetail).mockReturnValue({
    data: undefined,
    isLoading: false,
    isError: true,
    isPending: false,
    status: "error",
    error: new Error("not found"),
    isSuccess: false,
  } as ReturnType<typeof useExperimentDetail>);
}

function renderArtifact(
  props: {
    hash?: string;
    summary?: React.ReactNode;
    defaultOpen?: boolean;
    writerModel?: string | null;
  } = {},
) {
  return renderWithProviders(
    <ExpandableArtifact
      hash={props.hash ?? "abcd1234ef"}
      summary={props.summary ?? <span>v3.1.g · kept · +0.21</span>}
      defaultOpen={props.defaultOpen}
      writerModel={props.writerModel}
    />,
  );
}

describe("ExpandableArtifact — expand / collapse", () => {
  it("renders a collapsed summary with aria-expanded=false by default", () => {
    useDetailOk();
    renderArtifact();
    const btn = screen.getByRole("button", { name: /v3\.1\.g/ });
    expect(btn).toHaveAttribute("aria-expanded", "false");
    // Body should not be visible yet
    expect(screen.queryByText(/Tighten the stop/)).not.toBeInTheDocument();
  });

  it("expands inline to show the artifact body on click", async () => {
    useDetailOk();
    renderArtifact();
    const btn = screen.getByRole("button", { name: /v3\.1\.g/ });
    await userEvent.click(btn);
    expect(btn).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(/Tighten the stop/)).toBeInTheDocument();
  });

  it("collapses again on second click", async () => {
    useDetailOk();
    renderArtifact();
    const btn = screen.getByRole("button", { name: /v3\.1\.g/ });
    await userEvent.click(btn);
    expect(btn).toHaveAttribute("aria-expanded", "true");
    await userEvent.click(btn);
    expect(btn).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByText(/Tighten the stop/)).not.toBeInTheDocument();
  });

  it("starts expanded when defaultOpen=true", () => {
    useDetailOk();
    renderArtifact({ defaultOpen: true });
    const btn = screen.getByRole("button");
    expect(btn).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByText(/Tighten the stop/)).toBeInTheDocument();
  });
});

describe("ExpandableArtifact — writer model row", () => {
  it("renders the writer model row when writerModel is provided", () => {
    useDetailOk();
    renderArtifact({ defaultOpen: true, writerModel: "gemini-2.5-pro" });
    expect(screen.getByText(/gemini-2\.5-pro/)).toBeInTheDocument();
  });

  it("does not render the writer model row when writerModel is absent", () => {
    useDetailOk();
    renderArtifact({ defaultOpen: true });
    expect(screen.queryByText(/Writer:/)).not.toBeInTheDocument();
  });
});

describe("ExpandableArtifact — body sections from mocked detail", () => {
  it("renders the diff section (ParentDiffPanel) when expanded", () => {
    useDetailOk();
    renderArtifact({ defaultOpen: true });
    expect(screen.getByText(/What this experiment changed/)).toBeInTheDocument();
  });

  it("renders the regime section (RegimeCards) when expanded", () => {
    useDetailOk();
    renderArtifact({ defaultOpen: true });
    expect(screen.getByText(/Per-regime evaluation/)).toBeInTheDocument();
  });

  it("always renders the transcript footnote when expanded", () => {
    useDetailOk();
    renderArtifact({ defaultOpen: true });
    expect(
      screen.getByText(/Full prompt\/response transcripts aren't persisted yet\./),
    ).toBeInTheDocument();
  });

  it("renders 'Open strategy →' link pointing to the correct route", () => {
    useDetailOk();
    renderArtifact({ defaultOpen: true });
    const link = screen.getByRole("link", { name: /Open strategy/ });
    expect(link).toBeInTheDocument();
    expect(link).toHaveAttribute("href", "/optimizer/strategy/abcd1234ef");
  });
});

describe("ExpandableArtifact — loading state", () => {
  it("shows loading text while the detail is fetching", () => {
    useDetailLoading();
    renderArtifact({ defaultOpen: true });
    expect(screen.getByText(/Loading experiment…/)).toBeInTheDocument();
  });
});

describe("ExpandableArtifact — error state", () => {
  it("shows 'Artifact not available on this backend.' on error", () => {
    useDetailError();
    renderArtifact({ defaultOpen: true });
    expect(
      screen.getByText(/Artifact not available on this backend\./),
    ).toBeInTheDocument();
  });
});
