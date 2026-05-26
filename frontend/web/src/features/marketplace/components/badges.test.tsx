// src/features/marketplace/components/badges.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { AssetPill } from "./AssetPill";
import { VerifiedBadge } from "./VerifiedBadge";
import { X402Badge } from "./X402Badge";

describe("badges", () => {
  it("AssetPill shows the ticker and applies a per-asset tone class", () => {
    const { container } = render(<AssetPill asset="BTC" />);
    expect(screen.getByText("BTC")).toBeInTheDocument();
    expect(container.firstElementChild?.className).toContain("text-");
  });
  it("AssetPill falls back gracefully for unknown tickers", () => {
    render(<AssetPill asset="WIF" />);
    expect(screen.getByText("WIF")).toBeInTheDocument();
  });
  it("VerifiedBadge has an accessible title", () => {
    render(<VerifiedBadge />);
    expect(screen.getByTitle(/backtested/i)).toBeInTheDocument();
  });
  it("X402Badge labels x402", () => {
    render(<X402Badge />);
    expect(screen.getByText("x402")).toBeInTheDocument();
  });
});
