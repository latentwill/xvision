import { describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";

import {
  TimeWindowPills,
  sinceForWindow,
  TIME_WINDOWS,
  type TimeWindow,
} from "./TimeWindowPills";

// A fixed reference clock so the relative-window math is deterministic. Chosen
// mid-month so "start of today" is unambiguous in UTC.
const NOW = new Date("2026-06-13T14:30:00Z");

describe("sinceForWindow", () => {
  it("returns undefined for the All window (no filter)", () => {
    expect(sinceForWindow("all", NOW)).toBeUndefined();
  });

  it("returns start of local today for the Today window", () => {
    const since = sinceForWindow("today", NOW);
    expect(since).toBeDefined();
    const parsed = new Date(since!);
    // Start-of-today is local midnight; compare against a locally-constructed
    // midnight so the test is timezone-agnostic on CI.
    const localMidnight = new Date(
      NOW.getFullYear(),
      NOW.getMonth(),
      NOW.getDate(),
      0,
      0,
      0,
      0,
    );
    expect(parsed.getTime()).toBe(localMidnight.getTime());
    // It must be at or before "now".
    expect(parsed.getTime()).toBeLessThanOrEqual(NOW.getTime());
  });

  it("returns now - 7 days for the 7d window", () => {
    const since = sinceForWindow("7d", NOW);
    expect(since).toBeDefined();
    const parsed = new Date(since!);
    const expected = NOW.getTime() - 7 * 24 * 60 * 60 * 1000;
    expect(parsed.getTime()).toBe(expected);
  });

  it("returns now - 30 days for the 30d window", () => {
    const since = sinceForWindow("30d", NOW);
    expect(since).toBeDefined();
    const parsed = new Date(since!);
    const expected = NOW.getTime() - 30 * 24 * 60 * 60 * 1000;
    expect(parsed.getTime()).toBe(expected);
  });

  it("emits an RFC-3339 / ISO-8601 UTC string the backend can parse", () => {
    const since = sinceForWindow("7d", NOW)!;
    // toISOString() shape: 2026-06-06T14:30:00.000Z
    expect(since).toMatch(/^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(\.\d+)?Z$/);
  });

  it("defaults to the real clock when no `now` is supplied", () => {
    // Just assert it returns a parseable string and does not throw.
    const since = sinceForWindow("30d");
    expect(since).toBeDefined();
    expect(Number.isNaN(new Date(since!).getTime())).toBe(false);
  });
});

describe("TimeWindowPills", () => {
  it("renders all four windows as an accessible group", () => {
    render(<TimeWindowPills value="all" onChange={() => {}} />);
    const group = screen.getByTestId("time-window-pills");
    expect(group).toHaveAttribute("role", "group");
    expect(group).toHaveAttribute("aria-label");

    for (const w of TIME_WINDOWS) {
      // Each window renders as a button.
      expect(screen.getByRole("button", { name: w.label })).toBeInTheDocument();
    }
  });

  it("marks the selected window with aria-pressed=true and others false", () => {
    render(<TimeWindowPills value="7d" onChange={() => {}} />);
    expect(screen.getByRole("button", { name: "7d" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    expect(screen.getByRole("button", { name: "All" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );
  });

  it("calls onChange with the clicked window value", () => {
    const onChange = vi.fn();
    render(<TimeWindowPills value="all" onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: "30d" }));
    expect(onChange).toHaveBeenCalledWith("30d");
  });

  it("does not fire onChange when re-selecting the already-active window", () => {
    const onChange = vi.fn();
    render(<TimeWindowPills value="all" onChange={onChange} />);
    fireEvent.click(screen.getByRole("button", { name: "All" }));
    expect(onChange).not.toHaveBeenCalled();
  });

  it("renders buttons (keyboard-operable) — not divs", () => {
    render(<TimeWindowPills value="all" onChange={() => {}} />);
    const todayBtn = screen.getByRole("button", { name: "Today" });
    expect(todayBtn.tagName).toBe("BUTTON");
    expect(todayBtn).toHaveAttribute("type", "button");
  });

  it("exposes the canonical window order Today | 7d | 30d | All", () => {
    const order = TIME_WINDOWS.map((w) => w.value);
    expect(order).toEqual<TimeWindow[]>(["today", "7d", "30d", "all"]);
  });
});
