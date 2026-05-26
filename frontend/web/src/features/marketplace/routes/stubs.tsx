// src/features/marketplace/routes/stubs.tsx
// F0 stubs — replaced by real surfaces in F1–F7. Each names its route so the
// routing smoke test and manual nav prove the subtree resolves under the provider.
function Stub({ name }: { name: string }) {
  return (
    <div className="px-7 py-8 text-[13px] text-text-3" data-marketplace-stub={name}>
      Marketplace · {name} — coming in Phase F{name === "browse" ? "1" : ""}.
    </div>
  );
}

export const MarketplaceBrowseStub = () => <Stub name="browse" />;
export const MarketplaceLeaderboardStub = () => <Stub name="leaderboard" />;
export const MarketplaceLineageStub = () => <Stub name="lineage" />;
export const MarketplaceCreatorStub = () => <Stub name="creator" />;
export const MarketplaceSellStub = () => <Stub name="sell" />;
export const MarketplaceReceiptStub = () => <Stub name="receipt" />;
