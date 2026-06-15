// LineageRoute.buy.test.tsx — inline purchase states for the Buy CTA:
// inline error strip, InsufficientUsdcError → faucet affordance → retry.
// chain.ts is module-mocked (jsdom has no wallet); purchase flows through the
// MarketplaceData seam, so a spied fixture client drives the outcomes.
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { InsufficientUsdcError } from "@/features/marketplace/lib/purchaseErrors";
import { LineageRoute } from "./LineageRoute";

const faucetUsdcMock = vi.fn(async () => "0xfaucet-tx");
vi.mock("@/features/marketplace/lib/chain", () => ({
  activeNetworkSlug: "mantle-sepolia",
  faucetUsdc: (...args: unknown[]) =>
    (faucetUsdcMock as (...a: unknown[]) => Promise<string>)(...args),
}));

vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
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

function Wrapper({ client }: { client: FixtureMarketplaceData }) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={qc}>
      <MarketplaceDataProvider client={client}>
        <MemoryRouter initialEntries={["/marketplace/lineage/btc-momentum-v3"]}>
          <Routes>
            <Route path="/marketplace/lineage/:name" element={<LineageRoute />} />
            {/* Buy now finalizes and lands on the Strategy detail page. */}
            <Route path="/strategies/:id" element={<div>strategy detail</div>} />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>
  );
}

describe("LineageRoute buy states", () => {
  it("shows the error message inline when purchaseIntent rejects (no popup)", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "purchaseIntent").mockRejectedValue(
      new Error("relay exploded"),
    );
    render(<Wrapper client={client} />);

    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^acquire$/i }));

    const strip = await screen.findByTestId("buy-error");
    expect(strip).toHaveTextContent("relay exploded");
    // No faucet affordance for generic errors
    expect(screen.queryByTestId("faucet-btn")).not.toBeInTheDocument();
  });

  it("InsufficientUsdcError: offers Get test USDC, mints the needed amount, retries the buy", async () => {
    const client = new FixtureMarketplaceData();
    const intentSpy = vi
      .spyOn(client, "purchaseIntent")
      .mockRejectedValueOnce(new InsufficientUsdcError(49_000_000n, 1_000_000n))
      .mockResolvedValueOnce({ txHash: "0xreal", network: "mantle-sepolia" });
    // After the funded retry succeeds, finalize materializes the strategy.
    vi.spyOn(client, "importSealed").mockResolvedValue({ agent_id: "NEW" });
    render(<Wrapper client={client} />);

    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: /^acquire$/i }));

    // Inline error names the shortfall and offers the faucet
    const strip = await screen.findByTestId("buy-error");
    expect(strip).toHaveTextContent(/insufficient usdc/i);
    const faucetBtn = screen.getByTestId("faucet-btn");
    expect(faucetBtn).toHaveTextContent(/get test usdc/i);

    await user.click(faucetBtn);

    // Faucet mints the needed amount, then the purchase retries
    await waitFor(() => expect(faucetUsdcMock).toHaveBeenCalledWith(49_000_000n));
    await waitFor(() => expect(intentSpy).toHaveBeenCalledTimes(2));
    // Second attempt succeeds → finalizes and lands on the Strategy detail page
    expect(await screen.findByText("strategy detail")).toBeInTheDocument();
  });
});
