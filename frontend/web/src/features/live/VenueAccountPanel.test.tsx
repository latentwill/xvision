// frontend/web/src/features/live/VenueAccountPanel.test.tsx
import { afterEach, describe, expect, test, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MemoryRouter } from "react-router-dom";
import { VenueAccountPanel } from "./VenueAccountPanel";
import * as liveApi from "@/api/live";
import type { VenueAccount } from "@/api/live";

vi.mock("@/api/live", async () => {
  const actual = await vi.importActual<typeof import("@/api/live")>("@/api/live");
  return { ...actual, getVenueAccount: vi.fn() };
});

function renderPanel() {
  const qc = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return render(
    <QueryClientProvider client={qc}>
      <MemoryRouter>
        <VenueAccountPanel />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  localStorage.clear();
});

describe("VenueAccountPanel", () => {
  test("renders connected venue stats and positions", async () => {
    const acct: VenueAccount = {
      connected: true,
      venue: "orderly",
      network: "testnet",
      account_id: "0xabc",
      equity_usd: 1010.5,
      usdc_holding: 1000,
      unrealized_pnl: 10.5,
      positions: [
        {
          symbol: "PERP_BTC_USDC",
          qty: 0.001,
          entry_price: 100000,
          mark_price: 101000,
          unrealized_pnl: 1.0,
        },
      ],
    };
    vi.mocked(liveApi.getVenueAccount).mockResolvedValue(acct);

    renderPanel();

    await waitFor(() => {
      expect(screen.getByText(/orderly · testnet/i)).toBeInTheDocument();
    });
    expect(screen.getByText("Venue equity")).toBeInTheDocument();
    expect(screen.getByText("USDC holding")).toBeInTheDocument();
    expect(screen.getByText("PERP_BTC_USDC")).toBeInTheDocument();
  });

  test("renders quiet disconnected state with reason, never an error", async () => {
    const acct: VenueAccount = {
      connected: false,
      venue: "orderly",
      positions: [],
      reason: "ORDERLY_KEY not set",
    };
    vi.mocked(liveApi.getVenueAccount).mockResolvedValue(acct);

    renderPanel();

    await waitFor(() => {
      expect(screen.getByText(/ORDERLY_KEY not set/)).toBeInTheDocument();
    });
    expect(screen.getByText(/not connected/i)).toBeInTheDocument();
    expect(screen.queryByText("Venue equity")).not.toBeInTheDocument();
  });

  test("shows the connected wallet address from localStorage", async () => {
    localStorage.setItem(
      "xvn_wallet_address",
      "0xb5d2a3734aF76eFb7bC258b35c970F1Cc9c4E553",
    );
    vi.mocked(liveApi.getVenueAccount).mockResolvedValue({
      connected: false,
      venue: "orderly",
      positions: [],
      reason: "ORDERLY_KEY not set",
    });

    renderPanel();

    await waitFor(() => {
      expect(screen.getByTestId("venue-wallet-addr")).toHaveTextContent(
        /0xb5d2…E553/,
      );
    });
  });
});
