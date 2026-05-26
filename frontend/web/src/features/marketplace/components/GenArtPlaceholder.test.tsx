// src/features/marketplace/components/GenArtPlaceholder.test.tsx
import { render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { GenArtPlaceholder } from "./GenArtPlaceholder";

describe("GenArtPlaceholder", () => {
  it("is deterministic for a seed (same gradient stops)", () => {
    const { container: a } = render(<GenArtPlaceholder seed="btc-momentum-7a91-v3" size={80} />);
    const { container: b } = render(<GenArtPlaceholder seed="btc-momentum-7a91-v3" size={80} />);
    expect(a.querySelector("svg")?.innerHTML).toBe(b.querySelector("svg")?.innerHTML);
  });
  it("differs across seeds", () => {
    const { container: a } = render(<GenArtPlaceholder seed="aaa" />);
    const { container: b } = render(<GenArtPlaceholder seed="zzz" />);
    expect(a.querySelector("svg")?.innerHTML).not.toBe(b.querySelector("svg")?.innerHTML);
  });
  it("marks itself a placeholder for later swap", () => {
    const { container } = render(<GenArtPlaceholder seed="x" />);
    expect(container.querySelector('[data-genart="placeholder"]')).not.toBeNull();
  });
});
