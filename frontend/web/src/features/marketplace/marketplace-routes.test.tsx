// src/features/marketplace/marketplace-routes.test.tsx
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { MarketplaceLayout } from "./routes/MarketplaceLayout";
import { MarketplaceBrowseStub, MarketplaceLineageStub } from "./routes/stubs";
import { SellRoute } from "./routes/SellRoute";
import { MARKETPLACE_OPTIN_KEY } from "./lib/optin";

// C8: MarketplaceLayout now gates on the opt-in (default OFF), redirecting to
// Settings → Marketplace when off. These integration tests exercise the
// surfaces *behind* the gate, so enable the opt-in first.
beforeEach(() => {
  localStorage.setItem(MARKETPLACE_OPTIN_KEY, "1");
});
afterEach(() => {
  localStorage.clear();
});

function routerAt(path: string) {
  return createMemoryRouter(
    [
      {
        path: "/marketplace",
        element: <MarketplaceLayout />,
        children: [
          { index: true, element: <MarketplaceBrowseStub /> },
          { path: "lineage/:name", element: <MarketplaceLineageStub /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );
}

describe("marketplace routes", () => {
  it("mounts the browse stub under the data provider", async () => {
    render(<RouterProvider router={routerAt("/marketplace")} />);
    expect(await screen.findByText(/Marketplace · browse/)).toBeInTheDocument();
  });
  it("resolves the lineage route", async () => {
    render(<RouterProvider router={routerAt("/marketplace/lineage/btc-momentum-v3")} />);
    expect(await screen.findByText(/Marketplace · lineage/)).toBeInTheDocument();
  });
  it("resolves /marketplace/sell and renders the listing heading", async () => {
    const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const router = createMemoryRouter(
      [
        {
          path: "/marketplace",
          element: (
            <QueryClientProvider client={qc}>
              <MarketplaceLayout />
            </QueryClientProvider>
          ),
          children: [
            { index: true, element: <MarketplaceBrowseStub /> },
            { path: "lineage/:name", element: <MarketplaceLineageStub /> },
            { path: "sell", element: <SellRoute /> },
          ],
        },
      ],
      { initialEntries: ["/marketplace/sell"] },
    );
    render(<RouterProvider router={router} />);
    // W2-SELL renamed the funnel from "Share your strategy" to "List your strategy"
    // (QA2). The word "Share" no longer labels this flow.
    expect(await screen.findByText(/List your strategy/)).toBeInTheDocument();
  });
});
