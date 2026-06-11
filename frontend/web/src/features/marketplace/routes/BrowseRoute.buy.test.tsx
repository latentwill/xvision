// BrowseRoute.buy.test.tsx — F6 closure: card Buy navigates to the receipt on
// success; failures surface in the inline strip above the grid (no popups).
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { BrowseRoute } from "./BrowseRoute";

function Wrapper({ client }: { client: FixtureMarketplaceData }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={client}>
        <MemoryRouter initialEntries={["/marketplace"]}>
          <Routes>
            <Route path="/marketplace" element={<BrowseRoute />} />
            <Route path="/marketplace/receipts/:tx" element={<div>receipt page</div>} />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>
  );
}

describe("BrowseRoute buy", () => {
  it("navigates to the receipt after a successful card buy", async () => {
    const client = new FixtureMarketplaceData();
    const spy = vi.spyOn(client, "purchaseIntent").mockResolvedValue({
      txHash: "0xbrowse-buy",
      network: "mantle-sepolia",
    });
    render(<Wrapper client={client} />);

    const user = userEvent.setup();
    const buyButtons = await screen.findAllByRole("button", { name: /^buy$/i });
    await user.click(buyButtons[0]);

    expect(await screen.findByText("receipt page")).toBeInTheDocument();
    expect(spy).toHaveBeenCalledTimes(1);
  });

  it("shows a dismissible inline error strip when the buy fails", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "purchaseIntent").mockRejectedValue(
      new Error("Connect a wallet to buy — no wallet connected."),
    );
    render(<Wrapper client={client} />);

    const user = userEvent.setup();
    const buyButtons = await screen.findAllByRole("button", { name: /^buy$/i });
    await user.click(buyButtons[0]);

    const strip = await screen.findByTestId("browse-buy-error");
    expect(strip).toHaveTextContent(/connect a wallet to buy/i);
    // still on the browse page
    expect(screen.queryByText("receipt page")).not.toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: /dismiss/i }));
    expect(screen.queryByTestId("browse-buy-error")).not.toBeInTheDocument();
  });
});
