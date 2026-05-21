// Tests for the Filter v1 per-bar event timeline.
//
// The timeline takes the `FilterEventV1` stream emitted by FilterGated runs
// and renders one tick per cadence-gated bar. The visual marker is distinct
// per kind (triggered / not-triggered / suppressed-in-position /
// suppressed-daily-cap / suppressed-cooldown). Hovering surfaces the bar
// timestamp + the indicator snapshot via the native `title` attribute.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

import type { FilterEventV1 } from "@/api/types.gen/FilterEventV1";

import { FilterEventTimeline } from "./FilterEventTimeline";

afterEach(() => {
  cleanup();
});

function makeEvent(overrides: Partial<FilterEventV1> = {}): FilterEventV1 {
  return {
    bar_timestamp: "2026-05-21T00:00:00Z",
    filter_id: "f_01JX9TEST",
    triggered: false,
    suppressed_reason: null,
    conditions_passed: [],
    conditions_failed: [],
    indicator_snapshot: {},
    ...overrides,
  };
}

describe("FilterEventTimeline", () => {
  it("renders nothing when given an empty events array", () => {
    const { container } = render(<FilterEventTimeline events={[]} />);
    expect(container).toBeEmptyDOMElement();
  });

  it("renders one tick per event", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({ bar_timestamp: "2026-05-21T00:00:00Z" }),
          makeEvent({ bar_timestamp: "2026-05-21T01:00:00Z" }),
          makeEvent({ bar_timestamp: "2026-05-21T02:00:00Z" }),
        ]}
      />,
    );
    const ticks = screen.getAllByTestId("filter-event-tick");
    expect(ticks).toHaveLength(3);
  });

  it("classifies each suppression reason with a distinct data-kind/data-reason marker", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({ triggered: true, bar_timestamp: "t1" }),
          makeEvent({ suppressed_reason: "in_position", bar_timestamp: "t2" }),
          makeEvent({ suppressed_reason: "daily_cap", bar_timestamp: "t3" }),
          makeEvent({ suppressed_reason: "cooldown", bar_timestamp: "t4" }),
          makeEvent({ bar_timestamp: "t5" }),
        ]}
      />,
    );
    const ticks = screen.getAllByTestId("filter-event-tick");
    expect(ticks.map((t) => t.getAttribute("data-kind"))).toEqual([
      "triggered",
      "suppressed",
      "suppressed",
      "suppressed",
      "idle",
    ]);
    expect(ticks.map((t) => t.getAttribute("data-reason"))).toEqual([
      "",
      "in_position",
      "daily_cap",
      "cooldown",
      "",
    ]);

    // Visual markers — each class string is distinct so the operator can
    // tell the kinds apart at a glance.
    const classes = ticks.map((t) => t.className);
    expect(new Set(classes).size).toBe(5);
  });

  it("renders a legend with one entry per kind/reason", () => {
    render(
      <FilterEventTimeline events={[makeEvent({ triggered: true })]} />,
    );
    const legend = screen.getByTestId("filter-event-timeline-legend");
    const items = within(legend).getAllByRole("listitem");
    expect(items.map((li) => li.getAttribute("data-legend-kind"))).toEqual([
      "triggered",
      "in_position",
      "daily_cap",
      "cooldown",
      "idle",
    ]);
  });

  it("includes the indicator snapshot values in the tick `title` for hover", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({
            triggered: true,
            bar_timestamp: "2026-05-21T00:00:00Z",
            indicator_snapshot: { ema_20: 1230.5, rsi_14: 64.2 },
          }),
        ]}
      />,
    );
    const tick = screen.getByTestId("filter-event-tick");
    const title = tick.getAttribute("title") ?? "";
    expect(title).toContain("2026-05-21T00:00:00Z");
    expect(title).toContain("ema_20 = 1230.50");
    expect(title).toContain("rsi_14 = 64.2000");
  });

  it("renders the optional title when provided", () => {
    render(
      <FilterEventTimeline
        events={[makeEvent({ triggered: true })]}
        title="Filter timeline"
      />,
    );
    expect(screen.getByText("Filter timeline")).toBeInTheDocument();
  });

  it("omits the title heading when not provided", () => {
    render(<FilterEventTimeline events={[makeEvent({ triggered: true })]} />);
    expect(screen.queryByText("Filter timeline")).not.toBeInTheDocument();
  });
});
