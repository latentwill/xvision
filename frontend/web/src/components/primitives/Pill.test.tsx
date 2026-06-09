import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import { Pill } from "./Pill";

describe("Pill", () => {
  it("renders children", () => {
    render(<Pill>Active</Pill>);
    expect(screen.getByText("Active")).toBeInTheDocument();
  });

  it("is not aria-disabled by default", () => {
    render(<Pill>Ready</Pill>);
    const el = screen.getByText("Ready");
    expect(el).not.toHaveAttribute("aria-disabled");
    expect(el.className).not.toContain("pointer-events-none");
  });

  it("applies disabled state when disabled", () => {
    render(<Pill disabled>Locked</Pill>);
    const el = screen.getByText("Locked");
    expect(el).toHaveAttribute("aria-disabled", "true");
    expect(el.className).toContain("opacity-40");
    expect(el.className).toContain("pointer-events-none");
  });
});
