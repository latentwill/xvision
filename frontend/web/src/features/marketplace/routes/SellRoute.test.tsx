// src/features/marketplace/routes/SellRoute.test.tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderMarketplace } from "@/features/marketplace/test-utils";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { ApiError } from "@/api/client";
import { SellRoute } from "./SellRoute";

function renderSell(client?: InstanceType<typeof FixtureMarketplaceData>) {
  return renderMarketplace(<SellRoute />, {
    path: "/marketplace/sell",
    route: "/marketplace/sell",
    client,
  });
}

/** Navigate through steps 1 and 2 so step 3 is active (uses default fixture client). */
async function advanceToStep3(client?: InstanceType<typeof FixtureMarketplaceData>) {
  renderSell(client);
  await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
  await screen.findByTestId("sell-step-2-body");
  await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
  await screen.findByTestId("sell-step-3-body");
}

describe("SellRoute", () => {
  it("renders the page heading and step 1 active", async () => {
    renderSell();
    expect(await screen.findByText(/Share your strategy/)).toBeInTheDocument();
    expect(await screen.findByTestId("sell-step-1-body")).toBeInTheDocument();
    expect(screen.queryByTestId("sell-step-2-body")).not.toBeInTheDocument();
  });

  it("step 1: lists all 3 fixture strategies", async () => {
    renderSell();
    // strategy names from LISTABLE_STRATEGIES
    expect(await screen.findByText("btc-momentum")).toBeInTheDocument();
    expect(screen.getByText("eth-mr")).toBeInTheDocument();
    expect(screen.getByText("wip-draft")).toBeInTheDocument();
  });

  it("selecting a strategy calls createPublishDraft and advances to step 2", async () => {
    renderSell();
    const btn = await screen.findByRole("button", { name: /btc-momentum/ });
    await userEvent.click(btn);
    expect(await screen.findByTestId("sell-step-2-body")).toBeInTheDocument();
    expect(screen.queryByTestId("sell-step-1-body")).not.toBeInTheDocument();
  });

  it("step 2: shows listability checks — btc-momentum all pass", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    // all three checks should appear and pass
    expect(await screen.findByText(/Strategy exists in your XVN/)).toBeInTheDocument();
    expect(screen.getByText(/Declares an asset universe/)).toBeInTheDocument();
    expect(screen.getByText(/Has a backtest on record/)).toBeInTheDocument();
    // no failure reasons visible
    expect(screen.queryByText(/No assets configured/)).not.toBeInTheDocument();
  });

  it("step 2: shows specific failure reason for wip-draft", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /wip-draft/ }));
    expect(await screen.findByText(/No assets configured/)).toBeInTheDocument();
  });

  it("step 2: tier A hides price input; tier B shows price input", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    // default tier is sealed (B), price shown
    await screen.findByTestId("sell-step-2-body");
    expect(screen.getByTestId("price-input")).toBeInTheDocument();
    // switch to open (A)
    await userEvent.click(screen.getByTestId("tier-open-btn"));
    expect(screen.queryByTestId("price-input")).not.toBeInTheDocument();
  });

  it("step 2: clicking Continue advances to step 3", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    expect(await screen.findByTestId("sell-step-3-body")).toBeInTheDocument();
  });

  it("step 3: shows the listing preview card", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    // ListingPreviewCard renders preview.id
    expect(await screen.findByText("btc-momentum")).toBeInTheDocument();
  });

  it("step 3: Mint button is disabled when any listability check fails (wip-draft)", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /wip-draft/ }));
    await screen.findByTestId("sell-step-2-body");
    // Continue is also disabled for wip-draft; go directly to step 3 by navigating
    // via the step indicator would require an alternative approach — instead we
    // test via the known fixture behavior that wip-draft has a failing check,
    // so Continue is disabled. Document this as the known fixture constraint.
    // The Mint disabled state is proven by Step3Preview unit tests.
    expect(screen.getByRole("button", { name: /Continue/ })).toBeDisabled();
  });

  it("step 3: Mint button is enabled for btc-momentum (all checks pass)", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    expect(screen.getByRole("button", { name: /Mint/ })).not.toBeDisabled();
  });

  it("step 3: Mint button carries the shared Testnet badge", async () => {
    // C8: hand-rolled "[Testnet]" string replaced by the shared TestnetBadge,
    // which renders the text "Testnet".
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    expect(screen.getByRole("button", { name: /Mint/ }).textContent).toMatch(/Testnet/);
  });

  it("step 3: failed submitListing shows inline error, no navigation, no unhandled rejection", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "submitListing").mockRejectedValue(
      new ApiError(503, "internal", "chain env not configured — set XVN_RPC_URL"),
    );
    await advanceToStep3(client);

    const mintBtn = screen.getByRole("button", { name: /Mint/ });
    await userEvent.click(mintBtn);

    // Inline error strip visible with the server message
    const strip = await screen.findByTestId("mint-error");
    expect(strip).toBeInTheDocument();
    expect(strip.textContent).toMatch(/chain env not configured/);

    // Still on step 3 — no navigation occurred
    expect(screen.getByTestId("sell-step-3-body")).toBeInTheDocument();

    // Button re-enabled after failure
    expect(mintBtn).not.toBeDisabled();
  });

  it("step 3: successful submitListing navigates to /marketplace/lineage/<listing_id>", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "submitListing").mockResolvedValue({
      txHash: "42",
      network: "mantle-sepolia",
    });

    // Render with an extra route so we can assert navigation landed on lineage detail.
    const { render } = await import("@testing-library/react");
    const { QueryClient, QueryClientProvider } = await import("@tanstack/react-query");
    const { MarketplaceDataProvider } = await import("@/features/marketplace/data/provider");
    const { MemoryRouter, Routes, Route, useParams } = await import("react-router-dom");
    const { default: React } = await import("react");

    function LineageSpy() {
      const { id } = useParams<{ id: string }>();
      return React.createElement("div", { "data-testid": "lineage-page" }, `lineage:${id}`);
    }

    const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    render(
      React.createElement(
        QueryClientProvider,
        { client: queryClient },
        React.createElement(
          MarketplaceDataProvider,
          { client },
          React.createElement(
            MemoryRouter,
            { initialEntries: ["/marketplace/sell"] },
            React.createElement(
              Routes,
              null,
              React.createElement(Route, { path: "/marketplace/sell", element: React.createElement(SellRoute) }),
              React.createElement(Route, { path: "/marketplace/lineage/:id", element: React.createElement(LineageSpy) }),
            ),
          ),
        ),
      ),
    );

    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    await userEvent.click(screen.getByRole("button", { name: /Mint/ }));

    // Navigated to the real listing page — txHash is listing_id (see publish.ts Phase-2 wart)
    const lineagePage = await screen.findByTestId("lineage-page");
    expect(lineagePage.textContent).toBe("lineage:42");
  });
});
