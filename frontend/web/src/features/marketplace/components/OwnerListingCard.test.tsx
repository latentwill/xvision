// src/features/marketplace/components/OwnerListingCard.test.tsx
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { OwnerListingCard } from "./OwnerListingCard";
import type { IndexedListing } from "@/features/marketplace/data/ApiMarketplaceData";

const listing: IndexedListing = {
  listing_id: 7,
  agent_nft_id: "12",
  agent_id: "01HXAGENT",
  seller: "0xseller",
  content_hash: "0xhash",
  content_uri: "ipfs://x",
  tier: 1,
  price_usdc: 49,
  transferable_license: true,
  revoked: false,
  gen_art_seed: "seed-1",
  name: "Test Listing",
  symmetry: "radial",
  palette: "gold",
  attestation_count: 0,
  units_sold: 0,
  units_sold_agents: 0,
  earned_usdc: 0,
};

function Wrapper({ children }: { children: React.ReactNode }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const mp = new FixtureMarketplaceData();
  return (
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={mp}>{children}</MarketplaceDataProvider>
    </QueryClientProvider>
  );
}

describe("OwnerListingCard", () => {
  it("renders listing name, price, tier, and action buttons", () => {
    render(
      <Wrapper>
        <OwnerListingCard listing={listing} onChanged={() => {}} />
      </Wrapper>,
    );
    expect(screen.getByText("Test Listing")).toBeInTheDocument();
    expect(screen.getByText("49 USDC")).toBeInTheDocument();
    expect(screen.getByText("sealed")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /edit price/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /post attestation/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /republish content/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^revoke$/i })).toBeInTheDocument();
  });

  it("renders edit price inline control on click and calls setListingPrice", async () => {
    const mp = new FixtureMarketplaceData();
    const spy = vi.spyOn(mp, "setListingPrice").mockResolvedValue({ txHash: "0xtx", network: "mantle-sepolia" });
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const user = userEvent.setup();

    render(
      <QueryClientProvider client={qc}>
        <MarketplaceDataProvider client={mp}>
          <OwnerListingCard listing={listing} onChanged={() => {}} />
        </MarketplaceDataProvider>
      </QueryClientProvider>,
    );

    await user.click(screen.getByRole("button", { name: /edit price/i }));
    // Inline control appears
    expect(screen.getByRole("spinbutton", { name: /price usdc/i })).toBeInTheDocument();
    // Change price to 25
    await user.clear(screen.getByRole("spinbutton", { name: /price usdc/i }));
    await user.type(screen.getByRole("spinbutton", { name: /price usdc/i }), "25");
    await user.click(screen.getByRole("button", { name: /^save$/i }));

    await waitFor(() => expect(spy).toHaveBeenCalledWith("7", 25));
  });

  it("shows revoked state for revoked listing", () => {
    render(
      <Wrapper>
        <OwnerListingCard listing={{ ...listing, revoked: true }} onChanged={() => {}} />
      </Wrapper>,
    );
    expect(screen.getByText(/^revoked$/i)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /^revoke$/i })).not.toBeInTheDocument();
  });
});
