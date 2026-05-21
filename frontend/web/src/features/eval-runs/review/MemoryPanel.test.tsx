// Tests for the V2D `MemoryPanel`.
//
// The dispatcher seam in `crates/xvision-engine/src/agent/execute.rs`
// emits three event kinds (`memory_recall`, `memory_write`,
// `memory_disabled_no_embedder`) when a slot's `MemoryMode` is non-Off.
// `MemoryPanel` filters a cycle's `events` array down to those kinds and
// surfaces them as an inline section inside the eval-review surface.
// Anything outside the V2D vocabulary is ignored.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { MemoryPanel } from "./MemoryPanel";

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
    const { container } = render(<MemoryPanel events={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders nothing when only non-memory events are present", () => {
    const { container } = render(
      <MemoryPanel
        events={[
          { kind: "tool_call", payload: { name: "broker.submit" } },
          { kind: "model_call_finished", payload: { provider: "anthropic" } },
        ]}
      />,
    );
    expect(container).toBeEmptyDOMElement();
  });

  it("renders recall + write rows with namespace + previews visible", () => {
    render(<MemoryPanel events={[recall, write]} />);
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
    render(<MemoryPanel events={[evt]} />);
    expect(screen.getByText(/no embedder configured/i)).toBeInTheDocument();
    expect(screen.getByText(/agent:01HZTEST/)).toBeInTheDocument();
  });
});
