// Tests for the V2D `MemoryPanel`.
//
// The dispatcher seam in `crates/xvision-engine/src/agent/execute.rs`
// emits three event kinds (`memory_recall`, `memory_write`,
// `memory_disabled_no_embedder`) when a slot's `MemoryMode` is non-Off.
// `MemoryPanel` filters a cycle's `events` array down to those kinds and
// surfaces them as an inline section inside the eval-review surface.
// Anything outside the V2D vocabulary is ignored.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";

import { MemoryPanel } from "./MemoryPanel";

function renderPanel(events: { kind: string; payload: unknown }[]) {
  return render(
    <MemoryRouter>
      <MemoryPanel events={events} />
    </MemoryRouter>,
  );
}

afterEach(() => {
  cleanup();
});

const recall = {
  kind: "memory_recall",
  payload: {
    namespace: "agent:01HZTEST",
    k: 2,
    items: [
      { id: "m1", score: 0.92, text_preview: "noted last RSI cross was a fade" },
      { id: "m2", score: 0.71, text_preview: "stop tightened pre-event" },
    ],
  },
};

const write = {
  kind: "memory_write",
  payload: {
    namespace: "agent:01HZTEST",
    id: "m3",
    text_preview: "decided to hold; volatility expanding",
  },
};

describe("MemoryPanel (V2D)", () => {
  it("renders nothing when no memory events are present", () => {
    const { container } = renderPanel([]);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when only non-memory events are present", () => {
    const { container } = renderPanel([
      { kind: "tool_call", payload: { name: "broker.submit" } },
      { kind: "model_call_finished", payload: { provider: "anthropic" } },
    ]);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders recall + write rows with namespace + previews visible", () => {
    renderPanel([recall, write]);
    // Namespace surfaces once per row — both recall and write share the
    // same agent namespace in this fixture, so we expect at least one
    // visible occurrence of it.
    expect(screen.getAllByText(/agent:01HZTEST/).length).toBeGreaterThanOrEqual(2);
    // Recall hits: both preview strings render alongside their scores.
    expect(screen.getByText(/noted last RSI cross/)).toBeInTheDocument();
    expect(screen.getByText(/stop tightened pre-event/)).toBeInTheDocument();
    expect(screen.getByText("0.92")).toBeInTheDocument();
    expect(screen.getByText("0.71")).toBeInTheDocument();
    // Write preview renders too.
    expect(screen.getByText(/decided to hold/)).toBeInTheDocument();
  });

  it("renders an amber warning row when memory_disabled_no_embedder is present", () => {
    const evt = {
      kind: "memory_disabled_no_embedder",
      payload: { namespace: "agent:01HZTEST" },
    };
    renderPanel([evt]);
    expect(screen.getByText(/no embedder configured/i)).toBeInTheDocument();
    expect(screen.getByText(/agent:01HZTEST/)).toBeInTheDocument();
  });
});

describe("MemoryPanel — recall row overflow menu (Phase 4 deep-link)", () => {
  it("renders an overflow trigger on each recall row", () => {
    renderPanel([recall]);
    // One trigger per recall item (2 items in the fixture).
    expect(
      screen.getAllByRole("button", { name: /open recall actions/i }),
    ).toHaveLength(2);
  });

  it("Open Pattern links to /agents/<id>?tab=memory&pattern=<id> for agent namespaces", async () => {
    const user = userEvent.setup();
    renderPanel([recall]);

    const triggers = screen.getAllByRole("button", {
      name: /open recall actions/i,
    });
    await user.click(triggers[0]!);

    const link = await screen.findByRole("link", { name: /open pattern/i });
    expect(link).toHaveAttribute(
      "href",
      "/agents/01HZTEST?tab=memory&pattern=m1",
    );
  });

  it("Open Pattern links to /memory?pattern=<id> for the global namespace", async () => {
    const user = userEvent.setup();
    const globalRecall = {
      kind: "memory_recall",
      payload: {
        namespace: "global",
        k: 1,
        items: [
          {
            id: "g1",
            score: 0.88,
            text_preview: "operator pattern",
          },
        ],
      },
    };
    renderPanel([globalRecall]);

    await user.click(
      screen.getByRole("button", { name: /open recall actions/i }),
    );

    const link = await screen.findByRole("link", { name: /open pattern/i });
    expect(link).toHaveAttribute("href", "/memory?pattern=g1");
  });

  it("uses the recall item text_preview as a hover tooltip on the row", () => {
    renderPanel([recall]);
    // The first recall item's row carries its preview as a `title`
    // attribute for native hover tooltips. The component renders the
    // preview span itself with the same tooltip so we can assert on it.
    const previewSpan = screen.getByText(/noted last RSI cross was a fade/);
    const row = previewSpan.closest("li");
    expect(row).not.toBeNull();
    expect(within(row as HTMLElement).getByTitle(/noted last RSI cross/i))
      .toBeInTheDocument();
  });
});

describe("MemoryPanel — per-decision provenance (memory-provenance-in-decisions-trace)", () => {
  it("renders a 'Decision N' prefix when recall payload carries decision_id", () => {
    const recallWithDecision = {
      kind: "memory_recall",
      payload: {
        namespace: "agent:01HZTEST",
        k: 1,
        decision_id: 7,
        items: [
          { id: "m1", score: 0.5, text_preview: "preview body" },
        ],
      },
    };
    renderPanel([recallWithDecision]);
    // Header row must mention the decision id so operators can
    // attribute the recall set to the specific decision it fed into.
    expect(screen.getByText(/Decision 7/)).toBeInTheDocument();
    expect(screen.getByText(/recall/)).toBeInTheDocument();
  });

  it("omits the 'Decision N' prefix when recall payload has no decision_id", () => {
    // Back-compat: traces emitted before this contract landed don't
    // carry decision_id. The recall row must still render cleanly.
    renderPanel([recall]);
    // recall fixture has no decision_id — assert no "Decision N" text.
    expect(screen.queryByText(/Decision \d+/)).not.toBeInTheDocument();
  });

  it("renders separate Decision headers for multiple recall events on the same run", () => {
    // Eval-review surface: one run can emit multiple memory_recall
    // events across different decisions. Each event must surface its
    // own decision id so the operator can scan the per-decision
    // attribution at a glance.
    const events = [
      {
        kind: "memory_recall",
        payload: {
          namespace: "agent:01HZTEST",
          k: 1,
          decision_id: 1,
          items: [{ id: "m1", score: 0.5, text_preview: "first decision recall" }],
        },
      },
      {
        kind: "memory_recall",
        payload: {
          namespace: "agent:01HZTEST",
          k: 1,
          decision_id: 2,
          items: [{ id: "m2", score: 0.5, text_preview: "second decision recall" }],
        },
      },
    ];
    renderPanel(events);
    expect(screen.getByText(/Decision 1/)).toBeInTheDocument();
    expect(screen.getByText(/Decision 2/)).toBeInTheDocument();
    expect(screen.getByText(/first decision recall/)).toBeInTheDocument();
    expect(screen.getByText(/second decision recall/)).toBeInTheDocument();
  });
});
