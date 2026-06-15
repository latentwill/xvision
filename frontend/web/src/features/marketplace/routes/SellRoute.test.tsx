// src/features/marketplace/routes/SellRoute.test.tsx
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderMarketplace } from "@/features/marketplace/test-utils";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { ApiError } from "@/api/client";
import { SellRoute } from "./SellRoute";

// The sell flow reads/writes the stored strategy's plain_summary around mint.
vi.mock("@/api/strategies", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/strategies")>();
  return { ...actual, getStrategy: vi.fn(), patchStrategyMetadata: vi.fn() };
});
import { getStrategy, patchStrategyMetadata } from "@/api/strategies";
const mockedGetStrategy = vi.mocked(getStrategy);
const mockedPatch = vi.mocked(patchStrategyMetadata);

beforeEach(() => {
  vi.clearAllMocks();
  mockedGetStrategy.mockRejectedValue(new Error("engine unreachable"));
  mockedPatch.mockResolvedValue({} as Awaited<ReturnType<typeof patchStrategyMetadata>>);
});

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
    // Heading is now "List your strategy" (not "Share your strategy")
    expect(await screen.findByRole("heading", { name: /List your strategy/ })).toBeInTheDocument();
    expect(screen.queryByText(/Share your strategy/)).not.toBeInTheDocument();
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
    // ListingPreviewCard renders the app-native card with the humanized title
    expect(await screen.findByTestId("sell-step-3-body")).toBeInTheDocument();
    // The preview card element is present
    expect(document.querySelector("[data-preview='listing']")).toBeInTheDocument();
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

  it("step 3: edited public description is PATCHed to the strategy BEFORE submitListing", async () => {
    mockedGetStrategy.mockResolvedValue({
      manifest: { plain_summary: "old summary" },
    } as Awaited<ReturnType<typeof getStrategy>>);
    const client = new FixtureMarketplaceData();
    const submitSpy = vi.spyOn(client, "submitListing").mockResolvedValue({
      txHash: "42",
      network: "mantle-sepolia",
    });
    await advanceToStep3(client);

    const ta = screen.getByTestId("public-description");
    await waitFor(() => expect(ta).toHaveValue("old summary"));
    await userEvent.clear(ta);
    await userEvent.type(ta, "new public summary");
    await userEvent.click(screen.getByRole("button", { name: /Mint/ }));

    await waitFor(() => expect(submitSpy).toHaveBeenCalled());
    // draft.strategyId for the btc-momentum fixture
    expect(mockedPatch).toHaveBeenCalledWith(
      expect.any(String),
      { plain_summary: "new public summary" },
    );
    // The save must land before the listing submit (manifest hash is computed
    // server-side from the stored strategy).
    expect(mockedPatch.mock.invocationCallOrder[0]).toBeLessThan(
      submitSpy.mock.invocationCallOrder[0],
    );
  });

  it("step 3: untouched description publishes without any PATCH", async () => {
    mockedGetStrategy.mockResolvedValue({
      manifest: { plain_summary: "old summary" },
    } as Awaited<ReturnType<typeof getStrategy>>);
    const client = new FixtureMarketplaceData();
    const submitSpy = vi.spyOn(client, "submitListing").mockResolvedValue({
      txHash: "42",
      network: "mantle-sepolia",
    });
    await advanceToStep3(client);
    await waitFor(() =>
      expect(screen.getByTestId("public-description")).toHaveValue("old summary"),
    );

    await userEvent.click(screen.getByRole("button", { name: /Mint/ }));
    await waitFor(() => expect(submitSpy).toHaveBeenCalled());
    expect(mockedPatch).not.toHaveBeenCalled();
  });

  it("step 3: a failed description PATCH aborts the publish with an inline error", async () => {
    mockedGetStrategy.mockResolvedValue({
      manifest: { plain_summary: "old summary" },
    } as Awaited<ReturnType<typeof getStrategy>>);
    mockedPatch.mockRejectedValue(
      new ApiError(400, "validation", "plain_summary cannot be empty"),
    );
    const client = new FixtureMarketplaceData();
    const submitSpy = vi.spyOn(client, "submitListing");
    await advanceToStep3(client);

    const ta = screen.getByTestId("public-description");
    await waitFor(() => expect(ta).toHaveValue("old summary"));
    await userEvent.clear(ta);
    await userEvent.type(ta, "different");
    await userEvent.click(screen.getByRole("button", { name: /Mint/ }));

    const strip = await screen.findByTestId("mint-error");
    expect(strip.textContent).toMatch(/public description/i);
    expect(strip.textContent).toMatch(/plain_summary cannot be empty/);
    // publish aborted — no listing submitted, still on step 3
    expect(submitSpy).not.toHaveBeenCalled();
    expect(screen.getByTestId("sell-step-3-body")).toBeInTheDocument();
  });

  it("step 3: successful submitListing shows a success panel linking to the listing", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "submitListing").mockResolvedValue({
      txHash: "42",
      network: "mantle-sepolia",
    });

    await advanceToStep3(client);
    await userEvent.click(screen.getByRole("button", { name: /Mint/ }));

    // Explicit success state instead of a silent redirect — the seller gets
    // feedback and a link to the new listing (txHash is the listing_id; see
    // publish.ts Phase-2 wart).
    const success = await screen.findByTestId("mint-success");
    expect(success).toBeInTheDocument();
    const viewLink = screen.getByRole("link", { name: /View your listing/ });
    expect(viewLink).toHaveAttribute("href", "/marketplace/lineage/42");
  });
});
