// LineageRoute.finalize.test.tsx — QA #11/#12 buy→finalize→redirect flow.
//
// A purchase no longer lands on a receipt page. After the wallet confirm /
// relay resolves, the route finalizes the acquisition (decrypt + import +
// materialize for sealed; plain import for open/free) and redirects the user to
// the runnable Strategy detail page at /strategies/:agent_id.
//
// The post-purchase sealed import uses a bounded retry on the
// license-not-yet-visible (403) condition; we mock the retry helper's sleep to
// a no-op so the 403-then-resolve case runs instantly under real timers.
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { ApiError } from "@/api/client";
import { LineageRoute } from "./LineageRoute";

// Drive the bounded retry with a no-op sleep so 403-retry cases don't wait the
// real 1.5s between attempts. The retry logic itself is exercised for real.
vi.mock("@/features/marketplace/lib/finalizeImport", async () => {
  const actual = await vi.importActual<
    typeof import("@/features/marketplace/lib/finalizeImport")
  >("@/features/marketplace/lib/finalizeImport");
  return {
    ...actual,
    finalizeImportWithRetry: (
      fn: () => Promise<{ agent_id: string }>,
      opts: { attempts?: number; delayMs?: number } = {},
    ) => actual.finalizeImportWithRetry(fn, { ...opts, sleep: async () => {} }),
  };
});

vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

vi.mock("@/features/marketplace/lib/chain", () => ({
  faucetUsdc: vi.fn(async () => "0xfaucet-tx"),
}));

vi.mock("@/features/marketplace/lib/wallet", () => ({
  useWallet: () => ({
    address: "0xtest",
    connecting: false,
    connect: vi.fn(),
    disconnect: vi.fn(),
  }),
}));

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

function qc() {
  return new QueryClient({ defaultOptions: { queries: { retry: false } } });
}

function Wrapper({ client }: { client: FixtureMarketplaceData }) {
  return (
    <QueryClientProvider client={qc()}>
      <MarketplaceDataProvider client={client}>
        <MemoryRouter initialEntries={["/marketplace/lineage/btc-momentum-v3"]}>
          <Routes>
            <Route path="/marketplace/lineage/:name" element={<LineageRoute />} />
            <Route
              path="/strategies/:id"
              element={<div data-testid="strategy-detail">strategy detail</div>}
            />
            <Route
              path="/marketplace/receipts/:tx"
              element={<div data-testid="receipt-page">receipt</div>}
            />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>
  );
}

async function openTierClient() {
  const client = new FixtureMarketplaceData();
  const base = await new FixtureMarketplaceData().getListing("btc-momentum-v3");
  vi.spyOn(client, "getListing").mockResolvedValue({
    ...base,
    tier: "open",
    priceUsdc: null,
  });
  return client;
}

describe("LineageRoute finalize-on-buy (paid / sealed)", () => {
  it("buy success finalizes via importSealed and redirects to /strategies/:agent_id (not a receipt)", async () => {
    const client = new FixtureMarketplaceData();
    const buySpy = vi
      .spyOn(client, "purchaseIntent")
      .mockResolvedValue({ txHash: "0xreal", network: "mantle-sepolia" });
    const importSpy = vi
      .spyOn(client, "importSealed")
      .mockResolvedValue({ agent_id: "NEW" });
    render(<Wrapper client={client} />);

    await screen.findByTestId("lineage-purchase-col");
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^acquire$/i }));
    });

    await waitFor(() => expect(buySpy).toHaveBeenCalledWith("btc-momentum-v3"));
    await waitFor(() => expect(importSpy).toHaveBeenCalledWith("btc-momentum-v3"));
    // Lands on the runnable Strategy detail page — never a receipt page.
    expect(await screen.findByTestId("strategy-detail")).toBeInTheDocument();
    expect(screen.queryByTestId("receipt-page")).not.toBeInTheDocument();
  });

  it("retries on a transient 403 (twice) then redirects once the license is visible", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "purchaseIntent").mockResolvedValue({
      txHash: "0xreal",
      network: "mantle-sepolia",
    });
    const importSpy = vi
      .spyOn(client, "importSealed")
      .mockRejectedValueOnce(new ApiError(403, "forbidden", "no license yet"))
      .mockRejectedValueOnce(new ApiError(403, "forbidden", "no license yet"))
      .mockResolvedValueOnce({ agent_id: "NEW" });
    render(<Wrapper client={client} />);

    await screen.findByTestId("lineage-purchase-col");
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^acquire$/i }));
    });

    await screen.findByTestId("strategy-detail");
    expect(importSpy).toHaveBeenCalledTimes(3);
  });

  it("permanent 403 shows the inline buy-error and does NOT navigate", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "purchaseIntent").mockResolvedValue({
      txHash: "0xreal",
      network: "mantle-sepolia",
    });
    vi.spyOn(client, "importSealed").mockRejectedValue(
      new ApiError(403, "forbidden", "No license held for this wallet."),
    );
    render(<Wrapper client={client} />);

    await screen.findByTestId("lineage-purchase-col");
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^acquire$/i }));
    });

    const strip = await screen.findByTestId("buy-error");
    expect(strip).toHaveTextContent(/no license held/i);
    // No dead end: stays on the lineage page, never navigates.
    expect(screen.queryByTestId("strategy-detail")).not.toBeInTheDocument();
    expect(screen.getByTestId("lineage-page")).toBeInTheDocument();
  });
});

describe("LineageRoute finalize-on-buy (open / free)", () => {
  it("Run free imports via importListing and redirects to /strategies/:agent_id", async () => {
    const client = await openTierClient();
    const importSpy = vi
      .spyOn(client, "importListing")
      .mockResolvedValue({ agent_id: "FREE-NEW" });
    render(<Wrapper client={client} />);

    await screen.findByTestId("lineage-purchase-col");
    await act(async () => {
      await userEvent.click(screen.getByTestId("run-free-btn"));
    });

    await waitFor(() =>
      expect(importSpy).toHaveBeenCalledWith("btc-momentum-v3"),
    );
    expect(await screen.findByTestId("strategy-detail")).toBeInTheDocument();
    expect(screen.queryByTestId("receipt-page")).not.toBeInTheDocument();
  });
});
