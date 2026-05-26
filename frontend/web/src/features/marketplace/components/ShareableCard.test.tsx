// src/features/marketplace/components/ShareableCard.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { ShareableCard } from "./ShareableCard";
import type { ShareableCardData } from "../data/types";

const data: ShareableCardData = {
  id: "btc-momentum-v3", version: "v3.0", creator: { address: "0xa83e", handle: "@ed" },
  genArtSeed: "btc-momentum-7a91-v3", return30dPct: 47.2, return30dLabel: "30D",
  buyers: { humans: 247, agents: 14 }, paidToCreatorUsd: 1240, priceUsdc: 49,
  verification: "verified", acceptsX402: true, promise: "BTC momentum with Claude regime detection.",
  url: "xvn.market/lineage/btc-momentum-v3",
};

describe("ShareableCard", () => {
  it("composes at 1200x630 with title, return and url", () => {
    const { container } = render(<ShareableCard data={data} />);
    const root = container.firstElementChild as HTMLElement;
    expect(root.style.width).toBe("1200px");
    expect(root.style.height).toBe("630px");
    expect(screen.getByText("btc-momentum-v3")).toBeInTheDocument();
    expect(screen.getByText(/47.2%/)).toBeInTheDocument();
    expect(screen.getByText(/xvn\.market\/lineage\/btc-momentum-v3/)).toBeInTheDocument();
  });
});
