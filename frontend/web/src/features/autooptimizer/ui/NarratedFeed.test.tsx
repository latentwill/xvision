import { describe, expect, it, vi } from "vitest";
import { screen } from "@testing-library/react";
import { renderWithProviders } from "../test-utils";
import { NarratedFeed } from "./NarratedFeed";

// Mock ExpandableArtifact so we can test NarratedFeed without triggering
// the full experiment-detail network stack. We still render the summary
// children so we can assert on the narrated sentences.
vi.mock("./ExpandableArtifact", () => ({
  ExpandableArtifact: ({
    hash,
    summary,
  }: {
    hash: string;
    summary: React.ReactNode;
  }) => (
    <div data-testid={`artifact-${hash}`} role="button" aria-expanded="false">
      {summary}
    </div>
  ),
}));

// Fixtures mirror the REAL wire shapes (progress.rs): flattened fields, "type" tag.
const EVENT_PROPOSED = {
  type: "mutation_proposed",
  cycle_id: "c1",
  parent_hash: "ffff0000aa",
  child_hash: "abcd1234ef",
  mutator_model: "gemini-2.5-pro",
  ts: "2026-06-11T10:01:00Z",
};

const EVENT_GATED_KEPT = {
  type: "mutation_gated",
  child_hash: "abcd1234ef",
  passed: true,
  outcome: "kept",
  delta_day: 0.21,
  ts: "2026-06-11T10:02:00Z",
};

const EVENT_GATED_REJECTED = {
  type: "mutation_gated",
  child_hash: "beef5678cd",
  passed: false,
  outcome: "dropped",
  delta_day: -0.08,
  ts: "2026-06-11T10:03:00Z",
};

const EVENT_GATED_SUSPECT = {
  type: "mutation_gated",
  child_hash: "cafe9012de",
  passed: false,
  outcome: "suspect",
  ts: "2026-06-11T10:04:00Z",
};

const EVENT_PHASE_STARTED = {
  type: "phase_started",
  phase: "eval",
  detail: "backtesting",
  ts: "2026-06-11T10:00:30Z",
};

const EVENT_NO_CANDIDATE = {
  type: "no_candidate",
  parent_hash: "abcd1234ef",
  reason: "no diversity",
  ts: "2026-06-11T10:05:00Z",
};

describe("NarratedFeed — basic rendering", () => {
  it("renders one row per event in order", () => {
    renderWithProviders(
      <NarratedFeed
        events={[EVENT_PROPOSED, EVENT_GATED_KEPT, EVENT_GATED_REJECTED]}
      />,
    );
    const list = screen.getByRole("list", { name: /cycle events/i });
    const items = list.querySelectorAll("li");
    expect(items).toHaveLength(3);
  });

  it("renders the narrateEvent sentence for each event", () => {
    renderWithProviders(
      <NarratedFeed events={[EVENT_PROPOSED, EVENT_GATED_KEPT]} />,
    );
    expect(
      screen.getByText(/Writer gemini-2\.5-pro proposed an experiment → abcd1234/),
    ).toBeInTheDocument();
    expect(screen.getByText(/Gate passed abcd1234 · ΔSharpe \+0\.21 — kept/)).toBeInTheDocument();
  });

  it("renders a timestamp for each event that has ts", () => {
    renderWithProviders(<NarratedFeed events={[EVENT_PROPOSED]} />);
    // Should contain a time-like string (HH:MM format). We just check that it's in the DOM.
    const list = screen.getByRole("list");
    expect(list.textContent).toMatch(/\d{1,2}:\d{2}/);
  });

  it("renders an empty list when no events are provided", () => {
    renderWithProviders(<NarratedFeed events={[]} />);
    const list = screen.getByRole("list");
    expect(list.querySelectorAll("li")).toHaveLength(0);
  });
});

describe("NarratedFeed — tone classes", () => {
  it("applies text-gold tone class for kept events", () => {
    renderWithProviders(<NarratedFeed events={[EVENT_GATED_KEPT]} />);
    const sentence = screen.getByText(/Gate passed abcd1234/);
    expect(sentence.className).toContain("text-gold");
  });

  it("applies text-danger tone class for rejected events", () => {
    renderWithProviders(<NarratedFeed events={[EVENT_GATED_REJECTED]} />);
    const sentence = screen.getByText(/Gate failed beef5678/);
    expect(sentence.className).toContain("text-danger");
  });

  it("applies text-warn tone class for suspect events", () => {
    renderWithProviders(<NarratedFeed events={[EVENT_GATED_SUSPECT]} />);
    const sentence = screen.getByText(/Gate flagged cafe9012 — suspect/);
    expect(sentence.className).toContain("text-warn");
  });

  it("applies text-warn tone class for warn events (no_candidate)", () => {
    renderWithProviders(<NarratedFeed events={[EVENT_NO_CANDIDATE]} />);
    const sentence = screen.getByText(/No experiment produced/);
    expect(sentence.className).toContain("text-warn");
  });

  it("applies neutral class for phase_started events", () => {
    renderWithProviders(<NarratedFeed events={[EVENT_PHASE_STARTED]} />);
    const sentence = screen.getByText(/Phase eval started/);
    expect(sentence.className).toContain("text-text-2");
  });
});

describe("NarratedFeed — ExpandableArtifact for events with a hash", () => {
  it("renders events with a hash as ExpandableArtifact (clickable)", () => {
    renderWithProviders(<NarratedFeed events={[EVENT_PROPOSED]} />);
    // ExpandableArtifact mock renders with data-testid="artifact-<hash>"
    expect(screen.getByTestId("artifact-abcd1234ef")).toBeInTheDocument();
  });

  it("renders events without a hash as plain rows (not ExpandableArtifact)", () => {
    // cycle_started has no child_hash or bundle_hash
    const cycleStarted = {
      type: "cycle_started",
      cycle_id: "c1",
      parent_count: 3,
      ts: "2026-06-11T10:00:00Z",
    };
    renderWithProviders(<NarratedFeed events={[cycleStarted]} />);
    // The mock ExpandableArtifact uses data-testid="artifact-<hash>" — should not be there
    expect(screen.queryByTestId(/artifact-/)).not.toBeInTheDocument();
    expect(screen.getByText(/Cycle c1 started · 3 parents/)).toBeInTheDocument();
  });

  it("renders both hash and no-hash events in the same feed", () => {
    const cycleStarted = { type: "cycle_started", cycle_id: "c1", ts: "2026-06-11T10:00:00Z" };
    renderWithProviders(
      <NarratedFeed events={[cycleStarted, EVENT_PROPOSED]} />,
    );
    expect(screen.getByText(/Cycle c1 started/)).toBeInTheDocument();
    expect(screen.getByTestId("artifact-abcd1234ef")).toBeInTheDocument();
  });
});

describe("NarratedFeed — maxItems prop", () => {
  it("respects maxItems, keeping the newest events", () => {
    // Build 5 events; maxItems=3 should keep the last 3
    const events = [
      { type: "cycle_started", cycle_id: "c1", ts: "2026-06-11T10:00:00Z" },
      { type: "phase_started", phase: "propose", ts: "2026-06-11T10:00:10Z" },
      { type: "phase_started", phase: "eval", ts: "2026-06-11T10:00:20Z" },
      { type: "phase_started", phase: "gate", ts: "2026-06-11T10:00:30Z" },
      { type: "cycle_finished", active_count: 1, suspect_count: 0, rejected_count: 2, ts: "2026-06-11T10:01:00Z" },
    ];
    renderWithProviders(<NarratedFeed events={events} maxItems={3} />);
    const list = screen.getByRole("list");
    expect(list.querySelectorAll("li")).toHaveLength(3);
    // Last 3 events: phase gate, phase started gate, cycle_finished
    // The oldest two (cycle_started, phase propose) should not be in the DOM
    expect(screen.queryByText(/Cycle c1 started/)).not.toBeInTheDocument();
    expect(screen.queryByText(/Phase propose started/)).not.toBeInTheDocument();
    expect(screen.getByText(/Phase gate started/)).toBeInTheDocument();
    expect(screen.getByText(/Cycle finished/)).toBeInTheDocument();
  });

  it("defaults maxItems to 20 — renders all when ≤20 events", () => {
    const events = Array.from({ length: 10 }, (_, i) => ({
      type: "phase_started",
      phase: `p${i}`,
      ts: `2026-06-11T10:0${i}:00Z`,
    }));
    renderWithProviders(<NarratedFeed events={events} />);
    const list = screen.getByRole("list");
    expect(list.querySelectorAll("li")).toHaveLength(10);
  });
});
