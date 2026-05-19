import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { RegimeRangePresets } from "./RegimeRangePresets";

describe("RegimeRangePresets", () => {
  const originalTz = process.env.TZ;

  beforeEach(() => {
    process.env.TZ = "Asia/Taipei";
    vi.useFakeTimers();
    vi.setSystemTime(new Date(2026, 4, 19, 12, 0, 0));
  });

  afterEach(() => {
    vi.useRealTimers();
    if (originalTz === undefined) {
      delete process.env.TZ;
    } else {
      process.env.TZ = originalTz;
    }
  });

  it("uses local calendar dates for year presets", () => {
    const onPick = vi.fn();
    render(<RegimeRangePresets onPick={onPick} />);

    fireEvent.click(screen.getByRole("button", { name: "Last year" }));
    fireEvent.click(screen.getByRole("button", { name: "YTD" }));

    expect(onPick).toHaveBeenNthCalledWith(1, "2025-01-01", "2025-12-31");
    expect(onPick).toHaveBeenNthCalledWith(2, "2026-01-01", "2026-05-19");
  });

  it("uses local calendar dates for trailing ranges", () => {
    const onPick = vi.fn();
    render(<RegimeRangePresets onPick={onPick} />);

    fireEvent.click(screen.getByRole("button", { name: "Last 30 days" }));
    fireEvent.click(screen.getByRole("button", { name: "Last 90 days" }));

    expect(onPick).toHaveBeenNthCalledWith(1, "2026-04-19", "2026-05-19");
    expect(onPick).toHaveBeenNthCalledWith(2, "2026-02-18", "2026-05-19");
  });
});
