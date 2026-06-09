import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { MemoryRouter, Route, Routes } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceLayout } from "./MarketplaceLayout";
import { MARKETPLACE_OPTIN_KEY } from "@/features/marketplace/lib/optin";

afterEach(() => {
  cleanup();
  localStorage.clear();
});

function renderAt(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter initialEntries={[path]}>
        <Routes>
          <Route path="/marketplace" element={<MarketplaceLayout />}>
            <Route index element={<div>browse-content</div>} />
          </Route>
          <Route
            path="/settings/marketplace"
            element={<div>settings-marketplace-tab</div>}
          />
        </Routes>
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

describe("MarketplaceLayout opt-in gate (C8)", () => {
  it("redirects /marketplace to Settings → Marketplace when opt-in is off", async () => {
    renderAt("/marketplace");
    expect(
      await screen.findByText("settings-marketplace-tab"),
    ).toBeInTheDocument();
    expect(screen.queryByText("browse-content")).toBeNull();
  });

  it("renders the marketplace content (and testnet banner) when opt-in is on", async () => {
    localStorage.setItem(MARKETPLACE_OPTIN_KEY, "1");
    renderAt("/marketplace");
    expect(await screen.findByText("browse-content")).toBeInTheDocument();
    expect(screen.getByText(/Mantle Sepolia testnet/i)).toBeInTheDocument();
  });
});
