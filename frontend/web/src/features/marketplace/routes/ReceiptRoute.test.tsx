// src/features/marketplace/routes/ReceiptRoute.test.tsx
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it } from "vitest";
import { MarketplaceLayout } from "./MarketplaceLayout";
import { ReceiptRoute } from "./ReceiptRoute";

function routerAt(path: string) {
  return createMemoryRouter(
    [
      {
        path: "/marketplace",
        element: <MarketplaceLayout />,
        children: [
          { path: "receipts/:tx", element: <ReceiptRoute /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );
}

// MarketplaceLayout provides the DataProvider; we wrap with QueryClientProvider
// (MarketplaceLayout doesn't do that since the app shell normally provides it).
function renderWithQuery(path: string) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={qc}>
      <RouterProvider router={routerAt(path)} />
    </QueryClientProvider>
  );
}

describe("ReceiptRoute", () => {
  it("renders the success header with strategy id from fixture", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    expect(await screen.findByText(/You bought/)).toBeInTheDocument();
    // btc-momentum-v3 appears in header, license card, and OG card — use findAllByText
    const matches = await screen.findAllByText("btc-momentum-v3");
    expect(matches.length).toBeGreaterThan(0);
  });

  it("renders the fee breakdown line with price, token id, and net-to-creator", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    // these values appear in multiple panels (header, license card, OG card preview)
    const priceMatches = await screen.findAllByText(/49 USDC/);
    expect(priceMatches.length).toBeGreaterThan(0);
    const tokenMatches = await screen.findAllByText(/#0184/);
    expect(tokenMatches.length).toBeGreaterThan(0);
    const netMatches = await screen.findAllByText(/46\.55/);
    expect(netMatches.length).toBeGreaterThan(0);
  });

  it("renders a TxChip with the receipt txHash", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    // TxChip renders the hash as a link; fixture txHash is "0xdemo-tx"
    expect(await screen.findByRole("link", { name: /0xdemo-tx/ })).toBeInTheDocument();
  });

  it("renders all three panel headings", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    expect(await screen.findByText(/License NFT/i)).toBeInTheDocument();
    expect(await screen.findByText(/Install in your XVN/i)).toBeInTheDocument();
    expect(await screen.findByText(/Share/i)).toBeInTheDocument();
  });

  it("shows loading state before receipt resolves", () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    // The loading placeholder must be in the document synchronously
    expect(document.body.textContent).toMatch(/Loading|receipt/i);
  });
});
