import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TestnetBadge, TestnetBanner } from "./TestnetBadge";

describe("TestnetBadge", () => {
  it("renders the Testnet label", () => {
    render(<TestnetBadge />);
    expect(screen.getByText(/testnet/i)).toBeInTheDocument();
  });

  it("uses warn theme tokens (no hard white/gray borders)", () => {
    const { container } = render(<TestnetBadge />);
    const cls = container.firstElementChild?.className ?? "";
    expect(cls).toContain("border-warn/40");
    expect(cls).toContain("text-warn");
    expect(cls).not.toMatch(/border-white|border-gray-(100|200)/);
  });

  it("supports a larger sm size", () => {
    const { container } = render(<TestnetBadge size="sm" />);
    expect(container.firstElementChild?.className).toContain("text-[10px]");
  });
});

describe("TestnetBanner", () => {
  it("explains the marketplace is a simulated Mantle Sepolia testnet feature", () => {
    render(<TestnetBanner />);
    expect(screen.getByText(/Mantle Sepolia testnet/i)).toBeInTheDocument();
    expect(screen.getByText(/simulated/i)).toBeInTheDocument();
  });
});
