import { describe, expect, it, vi, afterEach } from "vitest";
import { formatRelativeTime, formatUntil } from "./time";

afterEach(() => vi.useRealTimers());

describe("formatRelativeTime", () => {
  it("returns empty string for undefined", () => {
    expect(formatRelativeTime(undefined)).toBe("");
  });

  it("returns 'just now' for < 1 minute ago", () => {
    const now = new Date().toISOString();
    expect(formatRelativeTime(now)).toBe("just now");
  });

  it("returns minutes label", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-11T10:05:00Z"));
    expect(formatRelativeTime("2026-06-11T10:02:00Z")).toBe("3m ago");
  });

  it("returns hours label", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-11T12:00:00Z"));
    expect(formatRelativeTime("2026-06-11T10:00:00Z")).toBe("2h ago");
  });

  it("returns days label", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-13T10:00:00Z"));
    expect(formatRelativeTime("2026-06-11T10:00:00Z")).toBe("2d ago");
  });

  it("returns the original string for an unparseable input", () => {
    expect(formatRelativeTime("not-a-date")).toBe("not-a-date");
  });
});

describe("formatUntil", () => {
  it("returns null for a past timestamp", () => {
    const past = new Date(Date.now() - 60_000).toISOString();
    expect(formatUntil(past)).toBeNull();
  });

  it("returns minutes label for < 1 hour ahead", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-11T10:00:00Z"));
    expect(formatUntil("2026-06-11T10:30:00Z")).toBe("in 30m");
  });

  it("returns hours label for < 24 hours ahead", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-11T10:00:00Z"));
    expect(formatUntil("2026-06-11T15:00:00Z")).toBe("in 5h");
  });

  it("returns days label for >= 24 hours ahead", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-06-11T10:00:00Z"));
    expect(formatUntil("2026-06-14T10:00:00Z")).toBe("in 3d");
  });

  it("returns null for an unparseable input", () => {
    expect(formatUntil("not-a-date")).toBeNull();
  });
});
