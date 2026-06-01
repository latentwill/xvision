// Tests for the Filter v1 per-bar event timeline.
//
// The timeline takes the `FilterEventV1` stream emitted by FilterGated runs
// and renders one tick per cadence-gated bar. The visual marker is distinct
// per kind (triggered / not-triggered / suppressed-in-position /
// suppressed-daily-cap / suppressed-cooldown). Hovering surfaces the bar
// timestamp + the indicator snapshot via the native `title` attribute.

import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";

import type { FilterEventV1 } from "@/api/types.gen/FilterEventV1";

import { FilterEventTimeline } from "./FilterEventTimeline";

afterEach(() => {
  cleanup();
});

function makeEvent(overrides: Partial<FilterEventV1> = {}): FilterEventV1 {
  return {
    schema_version: 1,
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

  it("does not render a static range stamp at the strip corners", () => {
    // Intake 2026-05-28 §4. The strip used to render the first and last bar
    // timestamps via `formatTimelineStamp` in two corners (`Feb 28, 07:00 PM`
    // on the right). The right-hand stamp read as a static page-level date
    // and the locale-formatted output (toLocaleString) drifted into the host
    // timezone while everything else on the page is UTC. Per-bar info is now
    // accessed via the per-tick title/aria-label tooltip.
    render(
      <FilterEventTimeline
        events={[
          makeEvent({ bar_timestamp: "2026-05-21T00:00:00Z" }),
          makeEvent({ bar_timestamp: "2026-05-22T01:00:00Z" }),
        ]}
      />,
    );
    expect(
      screen.queryByTestId("filter-event-timeline-range"),
    ).not.toBeInTheDocument();
  });

  it("keeps per-tick timestamps reachable via the title/aria-label tooltip", () => {
    // After removing the static range stamps, the per-bar timestamps must
    // still be one hover away — the title attr is the operator's only path
    // back to the bar's ISO.
    render(
      <FilterEventTimeline
        events={[makeEvent({ bar_timestamp: "2026-05-21T00:00:00Z" })]}
      />,
    );
    const tick = screen.getByTestId("filter-event-tick");
    expect(tick.getAttribute("title") ?? "").toContain("2026-05-21T00:00:00Z");
    expect(tick.getAttribute("aria-label") ?? "").toContain("2026-05-21T00:00:00Z");
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

  it("does not render the inline detail panel before any tick is clicked", () => {
    render(
      <FilterEventTimeline events={[makeEvent({ triggered: true })]} />,
    );
    expect(screen.queryByTestId("filter-event-detail")).not.toBeInTheDocument();
    expect(screen.queryByTestId("filter-event-preview")).not.toBeInTheDocument();
  });

  it("reveals an inline detail panel with bar timestamp and indicators on click", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({
            triggered: true,
            bar_timestamp: "2026-05-21T00:00:00Z",
            filter_id: "f_01JX9TEST",
            indicator_snapshot: { ema_20: 1230.5, rsi_14: 64.2 },
            conditions_passed: [0, 2],
            conditions_failed: [1],
          }),
        ]}
      />,
    );
    fireEvent.click(screen.getByTestId("filter-event-tick"));
    const detail = screen.getByTestId("filter-event-detail");
    expect(detail).toHaveTextContent("2026-05-21T00:00:00Z");
    expect(detail).toHaveTextContent("f_01JX9TEST");
    expect(detail).toHaveTextContent("triggered");
    expect(detail).toHaveTextContent("2 passed");
    expect(detail).toHaveTextContent("1 failed");
    const indicators = screen.getByTestId("filter-event-detail-indicators");
    expect(indicators).toHaveTextContent("ema_20");
    expect(indicators).toHaveTextContent("1230.50");
    expect(indicators).toHaveTextContent("rsi_14");
    expect(indicators).toHaveTextContent("64.2000");
  });

  it("hides the detail panel when clicking the same tick a second time", () => {
    render(
      <FilterEventTimeline events={[makeEvent({ triggered: true })]} />,
    );
    const tick = screen.getByTestId("filter-event-tick");
    fireEvent.click(tick);
    expect(screen.getByTestId("filter-event-detail")).toBeInTheDocument();
    fireEvent.click(tick);
    expect(screen.queryByTestId("filter-event-detail")).not.toBeInTheDocument();
  });

  it("replaces the detail panel when clicking a different tick", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({ triggered: true, bar_timestamp: "2026-05-21T00:00:00Z" }),
          makeEvent({
            suppressed_reason: "cooldown",
            bar_timestamp: "2026-05-21T01:00:00Z",
          }),
        ]}
      />,
    );
    const ticks = screen.getAllByTestId("filter-event-tick");
    fireEvent.click(ticks[0]);
    expect(screen.getByTestId("filter-event-detail")).toHaveAttribute(
      "data-bar-timestamp",
      "2026-05-21T00:00:00Z",
    );
    fireEvent.click(ticks[1]);
    expect(screen.getByTestId("filter-event-detail")).toHaveAttribute(
      "data-bar-timestamp",
      "2026-05-21T01:00:00Z",
    );
    expect(screen.getByTestId("filter-event-detail")).toHaveTextContent(
      "suppressed (cooldown)",
    );
  });

  it("surfaces a preview strip on hover and clears it on leave", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({
            triggered: true,
            bar_timestamp: "2026-05-21T00:00:00Z",
            conditions_passed: [0],
            conditions_failed: [],
          }),
        ]}
      />,
    );
    const tick = screen.getByTestId("filter-event-tick");
    fireEvent.mouseEnter(tick);
    const preview = screen.getByTestId("filter-event-preview");
    expect(preview).toHaveTextContent("2026-05-21T00:00:00Z");
    expect(preview).toHaveTextContent("triggered");
    expect(preview).toHaveTextContent("1 passed · 0 failed");
    fireEvent.mouseLeave(tick);
    expect(screen.queryByTestId("filter-event-preview")).not.toBeInTheDocument();
  });

  it("surfaces the preview on keyboard focus for keyboard parity", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({
            triggered: true,
            bar_timestamp: "2026-05-21T00:00:00Z",
          }),
        ]}
      />,
    );
    const tick = screen.getByTestId("filter-event-tick");
    fireEvent.focus(tick);
    expect(screen.getByTestId("filter-event-preview")).toHaveTextContent(
      "2026-05-21T00:00:00Z",
    );
    fireEvent.blur(tick);
    expect(screen.queryByTestId("filter-event-preview")).not.toBeInTheDocument();
  });

  it("keeps the preview visible while the selected tick is no longer hovered", () => {
    render(
      <FilterEventTimeline
        events={[
          makeEvent({ triggered: true, bar_timestamp: "2026-05-21T00:00:00Z" }),
        ]}
      />,
    );
    const tick = screen.getByTestId("filter-event-tick");
    fireEvent.click(tick);
    fireEvent.mouseEnter(tick);
    fireEvent.mouseLeave(tick);
    // After selection, the preview falls back to the selected tick so the
    // operator can read the detail header without keeping the cursor pinned.
    expect(screen.getByTestId("filter-event-preview")).toHaveTextContent(
      "2026-05-21T00:00:00Z",
    );
  });
});
