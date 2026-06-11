import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Sparkline } from "./Sparkline";

describe("Sparkline", () => {
  it("renders a polyline for a numeric series", () => {
    const { container } = render(
      <Sparkline values={[1, 2, 1.5, 3]} data-testid="spark" />,
    );
    const line = container.querySelector("polyline");
    expect(line).not.toBeNull();
    expect(line!.getAttribute("points")).toContain(",");
    // Area fill present by default.
    expect(container.querySelector("polygon")).not.toBeNull();
  });

  it("renders nothing with fewer than 2 finite samples", () => {
    const { container } = render(<Sparkline values={[1, NaN, Infinity]} />);
    expect(container.querySelector("svg")).toBeNull();
  });

  it("filters non-finite samples and never emits NaN coordinates", () => {
    const { container } = render(<Sparkline values={[1, NaN, 2, Infinity, 3]} />);
    const points = container.querySelector("polyline")!.getAttribute("points")!;
    expect(points).not.toMatch(/NaN/);
    expect(points.split(" ")).toHaveLength(3);
  });

  it("renders a midline for a flat series (no edge hugging)", () => {
    const { container } = render(
      <Sparkline values={[2, 2, 2]} height={20} fill={false} />,
    );
    const points = container.querySelector("polyline")!.getAttribute("points")!;
    for (const pair of points.split(" ")) {
      const y = Number(pair.split(",")[1]);
      expect(y).toBeCloseTo(10, 1);
    }
  });

  it("uses the tone token for the stroke", () => {
    const { container } = render(<Sparkline values={[1, 2]} tone="danger" />);
    expect(container.querySelector("polyline")!.getAttribute("stroke")).toBe(
      "var(--danger)",
    );
  });
});
