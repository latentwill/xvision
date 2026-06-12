// BrowseRoute.buy.test.tsx — The Catalogue removes the list-row buy flow.
// Rows are whole <Link>s to the inspector (inspect-before-buy); no tx fires from
// the list and no inline buy-error strip exists. Clicking a catalogue entry
// navigates to /marketplace/lineage/:name (spec 3.1E, QA10/QA12).
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { BrowseRoute } from "./BrowseRoute";

// The demo catalogue now renders a real MiniSparkline (uPlot pane) for the
// curated named listings. Mock uPlot so tests don't need a canvas-backed DOM
// (same pattern as LineageRoute.test.tsx).
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    setData() {}
    destroy() {}
  },
}));

// usePlot wires a ResizeObserver; jsdom doesn't provide one.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
beforeAll(() => {
  Object.defineProperty(globalThis, "ResizeObserver", {
    writable: true,
    configurable: true,
    value: ResizeObserverStub,
  });
});
afterAll(() => {
  delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
});

function Wrapper({ client }: { client: FixtureMarketplaceData }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={client}>
        <MemoryRouter initialEntries={["/marketplace"]}>
          <Routes>
            <Route path="/marketplace" element={<BrowseRoute />} />
            <Route path="/marketplace/lineage/:name" element={<div>inspector page</div>} />
            <Route path="/marketplace/receipts/:tx" element={<div>receipt page</div>} />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>
  );
}

describe("BrowseRoute entries route to the inspector", () => {
  it("navigates to the inspector (not a receipt) when a catalogue entry is clicked", async () => {
    const client = new FixtureMarketplaceData();
    const buySpy = vi.spyOn(client, "purchaseIntent");
    render(<Wrapper client={client} />);

    const user = userEvent.setup();
    const entry = await screen.findByText("Btc Momentum V3");
    await user.click(entry);

    // Routed to the inspector, never a receipt; no purchase tx fired from the list.
    expect(await screen.findByText("inspector page")).toBeInTheDocument();
    expect(screen.queryByText("receipt page")).not.toBeInTheDocument();
    expect(buySpy).not.toHaveBeenCalled();
  });

  it("does not render a list-row buy button or an inline buy-error strip", async () => {
    const client = new FixtureMarketplaceData();
    render(<Wrapper client={client} />);
    await screen.findByText("Btc Momentum V3");
    expect(screen.queryByRole("button", { name: /^buy$/i })).not.toBeInTheDocument();
    expect(screen.queryByTestId("browse-buy-error")).not.toBeInTheDocument();
  });
});
