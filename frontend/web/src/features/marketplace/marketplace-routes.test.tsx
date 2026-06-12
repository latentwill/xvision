// src/features/marketplace/marketplace-routes.test.tsx
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { MarketplaceLayout } from "./routes/MarketplaceLayout";
import { MarketplaceBrowseStub, MarketplaceLineageStub } from "./routes/stubs";
import { ReceiptRoute } from "./routes/ReceiptRoute";
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
  it("resolves /marketplace/sell and renders the catalogue listing heading", async () => {
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

// ── Receipt route integration ─────────────────────────────────────────────────
function receiptRouterAt(path: string) {
  return createMemoryRouter(
    [
      {
        path: "/marketplace",
        element: <MarketplaceLayout />,
        children: [
          { index: true, element: <MarketplaceBrowseStub /> },
          { path: "lineage/:name", element: <MarketplaceLineageStub /> },
          { path: "receipts/:tx", element: <ReceiptRoute /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );
}

function renderReceiptRoute(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={receiptRouterAt(path)} />
    </QueryClientProvider>
  );
}

describe("marketplace receipt route integration", () => {
  it("renders the receipt page for the demo fixture tx", async () => {
    renderReceiptRoute("/marketplace/receipts/0xdemo-tx");
    // W2-RECEIPT branches the success header on price (QA12): the paid demo tx
    // (pricePaidUsdc=49) reads "Acquired {id}", never "You bought".
    expect(await screen.findByText(/Acquired btc-momentum-v3/)).toBeInTheDocument();
    const matches = await screen.findAllByText("btc-momentum-v3");
    expect(matches.length).toBeGreaterThan(0);
  });

  it("renders the bundle install step title", async () => {
    renderReceiptRoute("/marketplace/receipts/0xdemo-tx");
    expect(await screen.findByText(/Fetch strategy bundle/i)).toBeInTheDocument();
  });

  it("renders the collapsed share strip (QA13)", async () => {
    renderReceiptRoute("/marketplace/receipts/0xdemo-tx");
    // W2-RECEIPT collapses share to an inline accordion strip by default (QA13):
    // the strip shows "Share this acquisition" + "Customize post"; the full
    // composer (Post to X / Farcaster) is revealed only on expand.
    expect(await screen.findByText(/Share this acquisition/i)).toBeInTheDocument();
    expect(await screen.findByText(/Customize post/i)).toBeInTheDocument();
  });

  it("unknown tx falls back to demo receipt (fixture behaviour)", async () => {
    renderReceiptRoute("/marketplace/receipts/0xunknown");
    // FixtureMarketplaceData.getReceipt falls back to 0xdemo-tx for unknown hashes;
    // the paid demo header reads "Acquired {id}" after the QA12 header branch.
    expect(await screen.findByText(/Acquired btc-momentum-v3/)).toBeInTheDocument();
  });
});
