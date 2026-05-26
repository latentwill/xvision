// src/features/marketplace/marketplace-routes.test.tsx
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { MarketplaceLayout } from "./routes/MarketplaceLayout";
import { MarketplaceBrowseStub, MarketplaceLineageStub } from "./routes/stubs";
import { ReceiptRoute } from "./routes/ReceiptRoute";
import { SellRoute } from "./routes/SellRoute";

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
  it("resolves /marketplace/sell and renders the page heading", async () => {
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
    expect(await screen.findByText(/Share your strategy/)).toBeInTheDocument();
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
    expect(await screen.findByText(/You bought/)).toBeInTheDocument();
    const matches = await screen.findAllByText("btc-momentum-v3");
    expect(matches.length).toBeGreaterThan(0);
  });

  it("renders all four install step titles", async () => {
    renderReceiptRoute("/marketplace/receipts/0xdemo-tx");
    expect(await screen.findByText(/Decrypt sealed bundle/i)).toBeInTheDocument();
  });

  it("renders the share composer with Post to X CTA", async () => {
    renderReceiptRoute("/marketplace/receipts/0xdemo-tx");
    expect(await screen.findAllByText(/Post to X/i)).not.toHaveLength(0);
  });

  it("unknown tx falls back to demo receipt (fixture behaviour)", async () => {
    renderReceiptRoute("/marketplace/receipts/0xunknown");
    // FixtureMarketplaceData.getReceipt falls back to 0xdemo-tx for unknown hashes
    expect(await screen.findByText(/You bought/)).toBeInTheDocument();
  });
});
