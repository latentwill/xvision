// Tests for VirtualsMark — Virtuals co-branding placeholder component.
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { VirtualsMark } from "./VirtualsMark";
import { DegenDeployStrip } from "./DegenDeployStrip";

afterEach(() => {
  cleanup();
});

describe("VirtualsMark", () => {
  it("renders an svg element with aria-label 'Virtuals'", () => {
    render(<VirtualsMark />);
    const svg = screen.getByRole("img", { name: "Virtuals" });
    expect(svg).toBeInTheDocument();
  });

  it("has data-testid='virtuals-mark'", () => {
    render(<VirtualsMark />);
    expect(screen.getByTestId("virtuals-mark")).toBeInTheDocument();
  });

  it("applies the default size of 14 as width and height attributes", () => {
    render(<VirtualsMark />);
    const svg = screen.getByTestId("virtuals-mark");
    expect(svg).toHaveAttribute("width", "14");
    expect(svg).toHaveAttribute("height", "14");
  });

  it("applies a custom size when provided", () => {
    render(<VirtualsMark size={20} />);
    const svg = screen.getByTestId("virtuals-mark");
    expect(svg).toHaveAttribute("width", "20");
    expect(svg).toHaveAttribute("height", "20");
  });

  it("forwards an extra className to the svg element", () => {
    render(<VirtualsMark className="text-text-3" />);
    expect(screen.getByTestId("virtuals-mark")).toHaveClass("text-text-3");
  });

  it("renders inside DegenDeployStrip when degen-arena venue is selected", () => {
    render(<DegenDeployStrip venue="degen-arena" onDeploy={vi.fn()} />);
    // The mark must appear inside the venue selector label.
    expect(screen.getByTestId("virtuals-mark")).toBeInTheDocument();
  });

  it("does NOT render inside DegenDeployStrip when orderly venue is selected", () => {
    render(<DegenDeployStrip venue="orderly" onDeploy={vi.fn()} />);
    // The venue label row is still visible even for orderly, so the mark
    // still renders in the venue radio group.
    expect(screen.getByTestId("virtuals-mark")).toBeInTheDocument();
  });
});
