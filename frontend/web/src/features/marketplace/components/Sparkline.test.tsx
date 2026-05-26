// src/features/marketplace/components/Sparkline.test.tsx
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { Sparkline } from "./Sparkline";

describe("Sparkline", () => {
  it("renders a path with 30 points and is seed-deterministic", () => {
    const { container: a } = render(<Sparkline seed="x" positive />);
    const { container: b } = render(<Sparkline seed="x" positive />);
    const path = a.querySelector("path");
    expect(path).not.toBeNull();
    expect((path!.getAttribute("d") ?? "").match(/L/g)?.length).toBe(29);
    expect(a.innerHTML).toBe(b.innerHTML);
  });
  it("uses danger stroke when negative", () => {
    const { container } = render(<Sparkline seed="x" positive={false} />);
    expect(container.querySelector("path")?.getAttribute("stroke")).toContain("danger");
  });
});
