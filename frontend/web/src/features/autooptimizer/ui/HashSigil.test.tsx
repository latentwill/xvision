import { describe, expect, it } from "vitest";
import { render } from "@testing-library/react";
import { HashSigil } from "./HashSigil";

describe("HashSigil", () => {
  it("renders a deterministic svg for a hash", () => {
    const { container, rerender } = render(<HashSigil hash="abc123" size={48} />);
    const svg = container.querySelector("svg");
    expect(svg).toBeTruthy();
    expect(svg).toHaveAttribute("width", "48");
    const first = container.innerHTML;
    rerender(<HashSigil hash="abc123" size={48} />);
    expect(container.innerHTML).toBe(first); // same hash → identical render
  });

  it("renders differently for a different hash", () => {
    const a = render(<HashSigil hash="aaaa" size={32} />).container.innerHTML;
    const b = render(<HashSigil hash="zzzz" size={32} />).container.innerHTML;
    expect(a).not.toBe(b);
  });
});
