// src/features/marketplace/routes/ReceiptRoute.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MarketplaceLayout } from "./MarketplaceLayout";
import { ReceiptRoute } from "./ReceiptRoute";
import { MARKETPLACE_OPTIN_KEY } from "@/features/marketplace/lib/optin";
import { RECEIPTS } from "@/features/marketplace/data/fixtures/receipts";

// C8: MarketplaceLayout gates on the opt-in (default OFF). Enable it so the
// receipt surface behind the gate renders instead of redirecting.
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
  it("renders 'Acquired' in the success header for a paid receipt", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    // fixture pricePaidUsdc=49 > 0, so header says "Acquired" with the display
    // name (not the raw URL slug): "Acquired BTC Momentum v3".
    expect(await screen.findByText(/Acquired BTC Momentum v3/)).toBeInTheDocument();
    // The display name appears in the header and the licence STRATEGY row.
    const matches = await screen.findAllByText("BTC Momentum v3");
    expect(matches.length).toBeGreaterThan(0);
    // The raw URL slug is never shown as the title.
    expect(screen.queryByText("btc-momentum-v3")).not.toBeInTheDocument();
  });

  it("does not say 'You bought' in the success header", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    await screen.findByText(/Acquired/);
    expect(screen.queryByText(/You bought/)).toBeNull();
  });

  it("renders the fee breakdown line with price, token id, and net-to-creator", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    // these values appear in multiple panels (header, license card)
    const priceMatches = await screen.findAllByText(/49 USDC/);
    expect(priceMatches.length).toBeGreaterThan(0);
    const tokenMatches = await screen.findAllByText(/#0184/);
    expect(tokenMatches.length).toBeGreaterThan(0);
    const netMatches = await screen.findAllByText(/46\.55/);
    expect(netMatches.length).toBeGreaterThan(0);
  });

  it("paid row in license card shows price only (no fee parenthetical)", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    await screen.findByText(/Acquired/);
    // The paid row value is "49 USDC" without fee breakdown
    expect(screen.queryByText(/5% fee/)).toBeNull();
    expect(screen.queryByText(/2\.45/)).toBeNull();
  });

  it("renders a TxChip explorer link with the receipt txHash", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    // TxChip with label="View on explorer" renders the hash as a link
    expect(await screen.findByRole("link", { name: /0xdemo-tx/ })).toBeInTheDocument();
  });

  it("explorer link points at explorer.sepolia.mantle.xyz (via TxChip, QA16)", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    const link = await screen.findByRole("link", { name: /0xdemo-tx/ });
    expect(link.getAttribute("href")).toContain("explorer.sepolia.mantle.xyz");
    expect(link.getAttribute("href")).not.toContain("mantlescan.xyz");
  });

  it("renders License NFT and Install in your XVN panel headings", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    expect(await screen.findByText(/License NFT/i)).toBeInTheDocument();
    expect(await screen.findByText(/Install in your XVN/i)).toBeInTheDocument();
  });

  it("does not render XVN-detection language in the install panel (QA #10)", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    await screen.findByText(/Install in your XVN/i);
    expect(screen.queryByText(/XVN not detected/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/detected at/i)).not.toBeInTheDocument();
  });

  it("renders 2-column layout (no 380px third column)", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    await screen.findByText(/Acquired/);
    // The body grid should be 320px 1fr only — no '380px' in gridTemplateColumns
    const grids = document.querySelectorAll("[style*='grid']");
    for (const el of grids) {
      expect((el as HTMLElement).style.gridTemplateColumns).not.toContain("380px");
    }
  });

  it("Share collapsed by default — collapse panel is not shown initially", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    await screen.findByText(/Acquired/);
    // OG preview should not be visible in collapsed state
    expect(document.querySelector("[data-og-preview]")).toBeNull();
  });

  it("Share accordion expands on 'Customize post' click", async () => {
    const user = userEvent.setup();
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    await screen.findByText(/Acquired/);
    const customizeBtn = screen.getByRole("button", { name: /Customize post/i });
    await user.click(customizeBtn);
    // OG preview should now be visible
    expect(document.querySelector("[data-og-preview]")).not.toBeNull();
  });

  it("renders inline share strip with 'Share this acquisition' label", async () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    expect(await screen.findByText(/Share this acquisition/i)).toBeInTheDocument();
  });

  it("shows loading state before receipt resolves", () => {
    renderWithQuery("/marketplace/receipts/0xdemo-tx");
    // The loading placeholder must be in the document synchronously
    expect(document.body.textContent).toMatch(/Loading|receipt/i);
  });

  describe("'Copy link' button in share strip — copies URL only, never caption", () => {
    it("writes only the URL (no caption) to the clipboard when the share-strip 'Copy link' is clicked", async () => {
      const user = userEvent.setup();
      // userEvent.setup() installs its own clipboard stub on navigator.clipboard.
      // Spy on that stub after it is in place so our spy captures the real call.
      const writeText = vi.spyOn(navigator.clipboard, "writeText").mockResolvedValue(undefined);
      renderWithQuery("/marketplace/receipts/0xdemo-tx");
      // Wait for receipt to load
      await screen.findByText(/Share this acquisition/i);
      const copyBtn = screen.getByRole("button", { name: /Copy link/i });
      await user.click(copyBtn);
      expect(writeText).toHaveBeenCalledTimes(1);
      const receipt = RECEIPTS["0xdemo-tx"];
      const expectedUrl = `https://${receipt.share.ogCard.url}`;
      expect(writeText).toHaveBeenCalledWith(expectedUrl);
      // Confirm caption text is NOT in the argument
      expect(writeText.mock.calls[0][0]).not.toContain(receipt.share.caption);
      writeText.mockRestore();
    });
  });
});
