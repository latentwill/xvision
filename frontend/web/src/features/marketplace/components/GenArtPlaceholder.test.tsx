// src/features/marketplace/components/GenArtPlaceholder.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GenArtPlaceholder } from "./GenArtPlaceholder";

describe("GenArtPlaceholder", () => {
  it("renders a canvas tagged bitfields-v3", () => {
    render(<GenArtPlaceholder seed="listing-abc" />);
    const canvas = screen.getByLabelText("strategy generative art");
    expect(canvas).toHaveAttribute("data-genart", "bitfields-v3");
    expect(canvas).toHaveAttribute("width", "140");
  });

  it("renders at the requested display size", () => {
    const { container } = render(<GenArtPlaceholder seed="sol-strategist-12fa" size={48} />);
    const canvas = container.querySelector("canvas") as HTMLCanvasElement;
    expect(canvas.style.width).toBe("48px");
    expect(canvas.style.height).toBe("48px");
  });

  it("has an accessible label", () => {
    const { container } = render(<GenArtPlaceholder seed="x" />);
    expect(container.querySelector('[aria-label="strategy generative art"]')).not.toBeNull();
  });

  it("does not throw for any seed string", () => {
    expect(() => render(<GenArtPlaceholder seed="aaa" />)).not.toThrow();
    expect(() => render(<GenArtPlaceholder seed="zzz" />)).not.toThrow();
    expect(() => render(<GenArtPlaceholder seed="" />)).not.toThrow();
  });
});
