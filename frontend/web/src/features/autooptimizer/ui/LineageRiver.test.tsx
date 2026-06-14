import { beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { renderWithProviders } from "../test-utils";
import type { RiverNode } from "../api";
import type { EventRow } from "../hooks/useCycleEventStream";
import { LineageRiver } from "./LineageRiver";

// ─── Mocks ────────────────────────────────────────────────────────────────────

const mockNavigate = vi.fn();
vi.mock("react-router-dom", async (orig) => {
  const real = await orig<typeof import("react-router-dom")>();
  return { ...real, useNavigate: () => mockNavigate };
});

vi.mock("../api", async (orig) => {
  const real = await orig<typeof import("../api")>();
  return {
    ...real,
    useRiver: vi.fn(),
    // ExpandableArtifact's body fetches these when expanded — stub them out.
    useExperimentDetail: vi.fn(() => ({
      data: undefined,
      isLoading: false,
      isError: true,
    })),
    useBlob: vi.fn(() => ({ data: undefined, isLoading: false, isError: false })),
  };
});

vi.mock("../hooks/useCycleEventStream", async (orig) => {
  const real = await orig<typeof import("../hooks/useCycleEventStream")>();
  return { ...real, useCycleEventStream: vi.fn() };
});

import { useRiver } from "../api";
import { useCycleEventStream } from "../hooks/useCycleEventStream";

type StreamReturn = ReturnType<typeof useCycleEventStream>;

function mockStream(over: Partial<StreamReturn> = {}) {
  vi.mocked(useCycleEventStream).mockReturnValue({
    events: [],
    connected: false,
    isRunning: false,
    activeCycleId: null,
    ...over,
  });
}

function mockRiver(data: RiverNode[] | undefined) {
  vi.mocked(useRiver).mockReturnValue({
    data,
    isLoading: false,
    isError: false,
  } as ReturnType<typeof useRiver>);
}

// ─── Fixtures ─────────────────────────────────────────────────────────────────

const OLD = "2026-06-01T12:00:00Z";
const MID = "2026-06-03T12:00:00Z";
const NOW = "2026-06-11T12:00:00Z";

function node(over: Partial<RiverNode> & { bundle_hash: string }): RiverNode {
  return {
    parent_hash: null,
    cycle_id: "cyc-1",
    status: "active",
    created_at: NOW,
    child_day_score: 1.0,
    delta_day: null,
    ...over,
  };
}

/** Kept chain (root→kept, alive+champion), one dead single-point line,
 * an old rejected stub off the root and a new suspect stub off the kept tip. */
const FIXTURE: RiverNode[] = [
  node({ bundle_hash: "rootroot11", created_at: OLD, child_day_score: 1.0 }),
  node({
    bundle_hash: "keptkept22",
    parent_hash: "rootroot11",
    created_at: NOW,
    child_day_score: 1.3,
    cycle_id: "cyc-2",
  }),
  node({
    bundle_hash: "rejoldd333",
    parent_hash: "rootroot11",
    status: "rejected",
    created_at: MID,
    child_day_score: 0.8,
    delta_day: -0.2,
    cycle_id: "cyc-1",
  }),
  node({
    bundle_hash: "susneww444",
    parent_hash: "keptkept22",
    status: "quarantined",
    created_at: NOW,
    child_day_score: 1.1,
    delta_day: 0.05,
    cycle_id: "cyc-2",
  }),
  // Dead line: single point with an old tip (outside newest 25% of time span)
  node({ bundle_hash: "deadroot55", created_at: OLD, child_day_score: 0.9 }),
];

const SINGLE_NODE: RiverNode[] = [
  node({ bundle_hash: "lonelyy666", created_at: NOW, child_day_score: 1.0 }),
];

beforeEach(() => {
  vi.clearAllMocks();
  mockStream();
});

// ─── Tests ────────────────────────────────────────────────────────────────────

describe("LineageRiver", () => {
  it("renders an svg with role img, aria-label, and a path per line + stub per stub", () => {
    mockRiver(FIXTURE);
    renderWithProviders(<LineageRiver />);
    const svg = screen.getByRole("img", { name: "Lineage river" });
    expect(svg.tagName.toLowerCase()).toBe("svg");
    // Two lines: root→kept chain + dead single-point line
    expect(screen.getAllByTestId("river-line")).toHaveLength(2);
    // Two stubs: rejected + suspect
    expect(screen.getAllByTestId("river-stub")).toHaveLength(2);
  });

  it("passes refetchIntervalWhileRunning to useRiver based on stream state", () => {
    mockStream({ isRunning: true });
    mockRiver(FIXTURE);
    renderWithProviders(<LineageRiver />);
    expect(vi.mocked(useRiver)).toHaveBeenCalledWith({
      refetchIntervalWhileRunning: true,
    });
  });

  it("fades stubs with age: an older stub has lower opacity than a newer one", () => {
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const stubs = [...container.querySelectorAll('[data-testid="river-stub"]')];
    expect(stubs).toHaveLength(2);
    const opacityByHash = new Map(
      stubs.map((s) => [s.getAttribute("data-hash"), Number(s.getAttribute("opacity"))]),
    );
    const oldOpacity = opacityByHash.get("rejoldd333")!;
    const newOpacity = opacityByHash.get("susneww444")!;
    expect(oldOpacity).toBeLessThan(newOpacity);
  });

  it("dims retired lines with stroke-text-4/40; live lines are not dimmed", () => {
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const lines = [...container.querySelectorAll('[data-testid="river-line"]')];
    const dimmed = lines.filter((l) => l.classList.contains("stroke-text-4/40"));
    expect(dimmed).toHaveLength(1);
    const live = lines.filter((l) => !l.classList.contains("stroke-text-4/40"));
    expect(live).toHaveLength(1);
  });

  it("hovering a stub populates the readout strip with hash, verdict, and delta", () => {
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const rejected = container.querySelector('[data-hash="rejoldd333"]')!;
    fireEvent.mouseOver(rejected);
    expect(screen.getByText(/rejoldd3/)).toBeInTheDocument();
    expect(screen.getByText("Rejected")).toBeInTheDocument();
    expect(screen.getByText(/ΔSharpe −0\.20/)).toBeInTheDocument();

    const suspect = container.querySelector('[data-hash="susneww444"]')!;
    fireEvent.mouseOver(suspect);
    expect(screen.getByText(/susneww4/)).toBeInTheDocument();
    expect(screen.getByText("Suspect")).toBeInTheDocument();
  });

  it("clicking a stub pins an expanded artifact readout with an Open cycle link", async () => {
    const user = userEvent.setup();
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const rejected = container.querySelector('[data-hash="rejoldd333"]')!;
    fireEvent.click(rejected);
    // Pinned readout renders as an expanded ExpandableArtifact
    expect(screen.getByRole("button", { expanded: true })).toBeInTheDocument();
    expect(screen.getByText(/rejoldd3/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Open cycle/ }));
    expect(mockNavigate).toHaveBeenCalledWith("/optimizer/cycle/cyc-1?exp=rejoldd333");
  });

  it("clicking a line point pins the readout and routes Open cycle to that point's cycle", async () => {
    const user = userEvent.setup();
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const point = container.querySelector('circle[data-hash="keptkept22"]')!;
    fireEvent.click(point);
    expect(screen.getByText(/keptkept/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /Open cycle/ }));
    expect(mockNavigate).toHaveBeenCalledWith("/optimizer/cycle/cyc-2?exp=keptkept22");
  });

  it("renders a live-end affordance per live line tip that opens the strategy", async () => {
    const user = userEvent.setup();
    mockRiver(FIXTURE);
    renderWithProviders(<LineageRiver />);
    const ends = screen.getAllByTestId("river-live-end");
    expect(ends).toHaveLength(1); // only one line is alive
    expect(ends[0]).toHaveAttribute("aria-label", "Open strategy keptkept22");
    await user.click(ends[0]);
    expect(mockNavigate).toHaveBeenCalledWith("/optimizer/strategy/keptkept22");
  });

  it("renders pulsing frontier + one ghost per unresolved in-flight experiment while running", () => {
    const events = [
      { kind: "mutation_proposed", child_hash: "ghosthash1", _row_id: 1 },
      { kind: "mutation_proposed", child_hash: "ghosthash2", _row_id: 2 },
      { kind: "mutation_gated", child_hash: "ghosthash1", passed: false, _row_id: 3 },
    ] as unknown as EventRow[];
    mockStream({ isRunning: true, events });
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const frontier = screen.getByTestId("river-frontier");
    expect(frontier.querySelector("animate")).not.toBeNull();
    expect(screen.getAllByTestId("river-ghost")).toHaveLength(1);
    expect(container.querySelectorAll('[data-testid="river-ghost"]')).toHaveLength(1);
  });

  it("renders neither frontier nor ghosts when not running", () => {
    mockStream({ isRunning: false });
    mockRiver(FIXTURE);
    renderWithProviders(<LineageRiver />);
    expect(screen.queryByTestId("river-frontier")).toBeNull();
    expect(screen.queryByTestId("river-ghost")).toBeNull();
  });

  it("does NOT show 'nothing kept yet' for a single-node (one active node = kept)", () => {
    // W18 fix: keptCount counts nodes, not edges. A single active node has
    // keptCount=1, which is > 0, so the banner must not appear.
    mockRiver(SINGLE_NODE);
    renderWithProviders(<LineageRiver />);
    expect(screen.queryByText(/nothing kept yet/)).not.toBeInTheDocument();
  });

  it("renders an honest empty panel when the river is empty but history exists", () => {
    mockRiver([]);
    renderWithProviders(<LineageRiver hasHistory={true} />);
    expect(screen.getByText("No lineage recorded yet.")).toBeInTheDocument();
  });

  it("renders nothing when the river is empty and there is no history", () => {
    mockRiver([]);
    const { container } = renderWithProviders(<LineageRiver hasHistory={false} />);
    expect(container.firstChild).toBeNull();
  });

  it("readout stays populated when the mouse leaves the svg (sticky last-hovered)", () => {
    // The readout must not vanish when the user moves the mouse off the branch
    // toward the "Open cycle →" button — only the in-chart stroke highlight clears.
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const rejected = container.querySelector('[data-hash="rejoldd333"]')!;
    fireEvent.mouseOver(rejected);
    expect(screen.getByText(/rejoldd3/)).toBeInTheDocument();
    const svg = screen.getByRole("img", { name: "Lineage river" });
    fireEvent.mouseLeave(svg);
    // Readout content still present after mouseleave
    expect(screen.getByText(/rejoldd3/)).toBeInTheDocument();
    expect(screen.getByText("Rejected")).toBeInTheDocument();
  });

  it("'Open cycle →' button remains clickable after moving mouse off the branch", async () => {
    const user = userEvent.setup();
    mockRiver(FIXTURE);
    const { container } = renderWithProviders(<LineageRiver />);
    const rejected = container.querySelector('[data-hash="rejoldd333"]')!;
    fireEvent.mouseOver(rejected);
    const svg = screen.getByRole("img", { name: "Lineage river" });
    fireEvent.mouseLeave(svg);
    // Button is still present and clickable after mouse left the SVG
    const btn = screen.getByRole("button", { name: /Open cycle/ });
    expect(btn).toBeInTheDocument();
    await user.click(btn);
    expect(mockNavigate).toHaveBeenCalledWith("/optimizer/cycle/cyc-1?exp=rejoldd333");
  });

  it("navigates to strategy on Enter keydown on river-live-end", async () => {
    const user = userEvent.setup();
    mockRiver(FIXTURE);
    renderWithProviders(<LineageRiver />);
    const end = screen.getByTestId("river-live-end");
    end.focus();
    await user.keyboard("{Enter}");
    expect(mockNavigate).toHaveBeenCalledWith("/optimizer/strategy/keptkept22");
  });

  it("never emits NaN in any path d attribute for degenerate single-node data", () => {
    mockRiver(SINGLE_NODE);
    const { container } = renderWithProviders(<LineageRiver />);
    expect(container.querySelector('path[d*="NaN"]')).toBeNull();
  });
});
