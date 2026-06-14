import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
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

// On a mainnet build the "Testnet · purchases are simulated" copy is false and
// unsafe — gate on the active network (VITE_MARKETPLACE_NETWORK).
describe("on a mainnet build", () => {
  afterEach(() => {
    vi.unstubAllEnvs();
  });

  it("TestnetBadge renders nothing (never label a real-funds surface 'Testnet')", () => {
    vi.stubEnv("VITE_MARKETPLACE_NETWORK", "mainnet");
    const { container } = render(<TestnetBadge />);
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByText(/testnet/i)).not.toBeInTheDocument();
  });

  it("TestnetBanner shows an accurate real-funds notice, not the simulated-testnet copy", () => {
    vi.stubEnv("VITE_MARKETPLACE_NETWORK", "mainnet");
    render(<TestnetBanner />);
    expect(screen.getByText(/Mantle mainnet/i)).toBeInTheDocument();
    expect(screen.getByText(/real USDC/i)).toBeInTheDocument();
    expect(screen.queryByText(/simulated/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Mantle Sepolia testnet/i)).not.toBeInTheDocument();
  });
});
