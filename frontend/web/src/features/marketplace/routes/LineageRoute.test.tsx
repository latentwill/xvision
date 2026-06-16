// src/features/marketplace/routes/LineageRoute.test.tsx
import { render, screen, waitFor, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi, beforeAll, afterAll } from "vitest";
import { MemoryRouter, Routes, Route } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { LineageRoute } from "./LineageRoute";

// Mock uPlot so tests don't need a DOM canvas environment
vi.mock("uplot", () => ({
  default: class {
    constructor() {}
    setSize() {}
    destroy() {}
  },
}));

// Mock useWallet — tests run in jsdom without MetaMask; default to connected
// so the Buy button renders as "Buy" (not "Connect wallet to buy").
vi.mock("@/features/marketplace/lib/wallet", () => ({
  useWallet: () => ({
    address: "0xtest",
    connecting: false,
    connect: vi.fn(),
    disconnect: vi.fn(),
  }),
}));

// Mock ResizeObserver — jsdom doesn't provide it, but HeroGradientEquity uses it
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

function Wrapper({
  initialPath = "/marketplace/lineage/btc-momentum-v3",
  client = new FixtureMarketplaceData(),
}: {
  initialPath?: string;
  client?: FixtureMarketplaceData;
}) {
  return (
    <QueryClientProvider client={qc()}>
      <MarketplaceDataProvider client={client}>
        <MemoryRouter initialEntries={[initialPath]}>
          <Routes>
            <Route path="/marketplace/lineage/:name" element={<LineageRoute />} />
            {/* Buy/clone now finalize the acquisition and land on the runnable
                Strategy detail page (QA #11/#12) — no receipt page. */}
            <Route
              path="/strategies/:id"
              element={<div data-testid="strategy-detail">strategy detail</div>}
            />
          </Routes>
        </MemoryRouter>
      </MarketplaceDataProvider>
    </QueryClientProvider>
  );
}

describe("LineageRoute", () => {
  it("renders the hero info stack with title, description, and 30d return", async () => {
    render(<Wrapper />);
    expect(await screen.findByTestId("lineage-info-stack")).toBeInTheDocument();
    // The hero title uses the listing's display name (app-native title style,
    // no raw tech slug): btc-momentum-v3 → "BTC Momentum v3".
    expect(screen.getByText("BTC Momentum v3")).toBeInTheDocument();
    expect(screen.getByText(/BTC momentum/)).toBeInTheDocument();
    // 30D RETURN label
    expect(screen.getByText(/30D Return/i)).toBeInTheDocument();
    // value shown as percentage
    expect(screen.getByText(/47\.2/)).toBeInTheDocument();
  });

  it("renders the strategy description above the fold from detail.promise", async () => {
    render(<Wrapper />);
    const desc = await screen.findByTestId("strategy-description");
    // Fixture promise text appears in the dedicated description block.
    expect(desc).toHaveTextContent(/BTC momentum with Claude regime detection/i);
    // The sealed-fallback line is NOT shown when a promise exists.
    expect(
      screen.queryByTestId("strategy-description-sealed"),
    ).not.toBeInTheDocument();
  });

  it("shows the honest sealed-strategy line when the promise is empty", async () => {
    const client = new FixtureMarketplaceData();
    const base = await new FixtureMarketplaceData().getListing("btc-momentum-v3");
    vi.spyOn(client, "getListing").mockResolvedValue({ ...base, promise: "" });
    render(<Wrapper client={client} />);
    const sealed = await screen.findByTestId("strategy-description-sealed");
    expect(sealed).toHaveTextContent(
      /sealed strategy — contents verified on-chain, revealed after purchase/i,
    );
    expect(screen.queryByTestId("strategy-description")).not.toBeInTheDocument();
  });

  it("renders all five metric cells with values that fit (tabular-nums, nowrap)", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-info-stack");
    // Labels present
    for (const label of ["30D Return", "Sharpe", "Win rate", "Max DD", "Avg dur"]) {
      expect(screen.getByText(label)).toBeInTheDocument();
    }
    // The value cell uses whitespace-nowrap + tabular-nums so values never clip.
    const ret = screen.getByText(/\+?47\.2%/);
    expect(ret.className).toMatch(/whitespace-nowrap/);
    expect(ret.className).toMatch(/tabular-nums/);
  });

  it("renders asset pills and badges in the hero", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-hero");
    expect(screen.getByText("BTC")).toBeInTheDocument(); // AssetPill
    // VerifiedBadge renders; the x402 badge was removed from listing surfaces
    // (operator QA) — it must no longer appear in the hero.
    expect(screen.getByTestId("verified-badge")).toBeInTheDocument();
    expect(screen.queryByTestId("x402-badge")).not.toBeInTheDocument();
  });

  it("renders no verification badge for unverified listings", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "getListing").mockResolvedValue({
      ...(await new FixtureMarketplaceData().getListing("btc-momentum-v3")),
      verification: "unverified",
    });
    render(<Wrapper client={client} />);
    await screen.findByTestId("lineage-hero");
    expect(screen.queryByTestId("verified-badge")).not.toBeInTheDocument();
    expect(screen.queryByText(/unverified/i)).not.toBeInTheDocument();
  });

  it("shows buyer count: N humans + M agents", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-info-stack");
    expect(screen.getByText(/247/)).toBeInTheDocument();
    // "14" appears in both buyer card ("14 agents") and recent buyers ("agent #14")
    expect(screen.getAllByText(/14/).length).toBeGreaterThanOrEqual(1);
  });

  // QA fix: the "$X paid to 0x…" line must NOT append the platform fee. The
  // standalone fee disclosure below the price (data-testid="fee-line") is a
  // separate element and stays.
  it("does not append the platform fee to the buyer 'paid to' line", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-info-stack");
    const paidLine = screen.getByText(/paid to/i);
    expect(paidLine).toBeInTheDocument();
    expect(paidLine).not.toHaveTextContent(/platform fee/i);
  });

  it("clicking the gen-art thumbnail inline-expands the artifact & provenance inspector", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-hero");
    // Closed by default
    expect(screen.queryByTestId("inspect-art")).not.toBeInTheDocument();
    await act(async () => {
      await userEvent.click(screen.getByTestId("plate-inspect-toggle"));
    });
    const inspector = await screen.findByTestId("inspect-art");
    expect(inspector).toHaveTextContent(/artifact & provenance/i);
    // On-chain metadata + a real transaction explorer link.
    expect(inspector).toHaveTextContent(/manifest_hash/i);
    expect(inspector).toHaveTextContent(/view on explorer/i);
    expect(
      screen.getByRole("link", { name: /0xc0a4f3b2/i }),
    ).toHaveAttribute(
      "href",
      "https://explorer.sepolia.mantle.xyz/tx/0xc0a4f3b2",
    );
  });

  it("renders the purchase block with price (no fee in the price) and an Acquire CTA", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-purchase-col");
    expect(screen.getByText(/49 USDC/)).toBeInTheDocument(); // 49 USDC, no parenthesized fee
    // Fee lives on a separate muted line
    expect(screen.getByTestId("fee-line")).toHaveTextContent(/platform fee 5%/i);
    expect(screen.getByRole("button", { name: /^acquire$/i })).toBeInTheDocument();
  });

  it("does not render a Share button in the hero (removed)", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-purchase-col");
    expect(screen.queryByRole("button", { name: /^share$/i })).not.toBeInTheDocument();
  });

  it("Acquire finalizes the buy and navigates to the Strategy detail page", async () => {
    const client = new FixtureMarketplaceData();
    const spy = vi.spyOn(client, "purchaseIntent").mockResolvedValue({
      txHash: "0xdeadbeef",
      network: "mantle-sepolia",
    });
    // Finalize materializes the agents and resolves the new local strategy id.
    vi.spyOn(client, "importSealed").mockResolvedValue({ agent_id: "NEW" });
    render(<Wrapper client={client} />);
    await screen.findByRole("button", { name: /^acquire$/i });
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^acquire$/i }));
    });
    await waitFor(() => {
      expect(spy).toHaveBeenCalledWith("btc-momentum-v3");
    });
    // After success, landed on the runnable Strategy detail page (not a receipt).
    expect(await screen.findByTestId("strategy-detail")).toBeInTheDocument();
  });

  it("open-tier listings show Run free and import via importListing", async () => {
    const client = new FixtureMarketplaceData();
    const base = await new FixtureMarketplaceData().getListing("btc-momentum-v3");
    vi.spyOn(client, "getListing").mockResolvedValue({
      ...base,
      tier: "open",
      priceUsdc: null,
    });
    const importSpy = vi
      .spyOn(client, "importListing")
      .mockResolvedValue({ agent_id: "FREE-NEW" });
    render(<Wrapper client={client} />);
    const runFree = await screen.findByTestId("run-free-btn");
    expect(runFree).toHaveTextContent(/run free/i);
    // No Acquire/buy button on an open listing
    expect(screen.queryByTestId("buy-btn")).not.toBeInTheDocument();
    await act(async () => {
      await userEvent.click(runFree);
    });
    await waitFor(() => expect(importSpy).toHaveBeenCalledWith("btc-momentum-v3"));
    // Landed on the Strategy detail page (not a receipt).
    expect(await screen.findByTestId("strategy-detail")).toBeInTheDocument();
  });

  it("does not render a 'Clone to edit' button — clone lives on the Strategies page, and sealed listings are encrypted", async () => {
    // Even for a listing the viewer owns, the marketplace detail no longer
    // offers clone: sealed bundles are encrypted and cannot be duplicated,
    // and cloning a local draft is a Strategies-page action.
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "getViewer").mockResolvedValue({
      isConnected: true,
      address: "0xabc",
      handle: "@test",
      createdListingIds: [],
      ownedListingIds: ["btc-momentum-v3"],
    });
    render(<Wrapper client={client} />);
    await screen.findByTestId("lineage-purchase-col");
    expect(
      screen.queryByRole("button", { name: /clone to edit/i }),
    ).not.toBeInTheDocument();
  });

  it("receipts drawer is collapsed by default", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-page");
    expect(screen.getByTestId("receipts-toggle")).toBeInTheDocument();
    expect(screen.queryByTestId("receipts-body")).not.toBeInTheDocument();
  });

  it("receipts drawer expands when ?receipts=open is in URL", async () => {
    render(
      <Wrapper initialPath="/marketplace/lineage/btc-momentum-v3?receipts=open" />,
    );
    await screen.findByTestId("lineage-page");
    expect(await screen.findByTestId("receipts-body")).toBeInTheDocument();
  });

  it("clicking the receipts toggle adds ?receipts=open to the URL", async () => {
    render(<Wrapper />);
    await screen.findByTestId("lineage-page");
    await act(async () => {
      await userEvent.click(screen.getByTestId("receipts-toggle"));
    });
    expect(await screen.findByTestId("receipts-body")).toBeInTheDocument();
  });

  it("shows the app-native not-found state for an unknown strategy name", async () => {
    render(<Wrapper initialPath="/marketplace/lineage/does-not-exist" />);
    expect(
      await screen.findByText(/strategy not found/i),
    ).toBeInTheDocument();
    expect(screen.getByTestId("lineage-not-found")).toBeInTheDocument();
    expect(
      screen.getByRole("link", { name: /back to marketplace/i }),
    ).toBeInTheDocument();
  });
});

// ── Eval attestations (on-chain) ─────────────────────────────────────────────
describe("LineageRoute eval attestations", () => {
  afterEach(() => vi.unstubAllGlobals());

  const ATTESTER = "0xa83e000000000000000000000000000000000001";

  async function verifiedNumericClient() {
    const client = new FixtureMarketplaceData();
    const base = await new FixtureMarketplaceData().getListing(
      "btc-momentum-v3",
    );
    vi.spyOn(client, "getListing").mockResolvedValue({
      ...base,
      id: "3",
      verification: "verified",
    });
    return client;
  }

  it("fetches attestations for a verified on-chain listing and renders the section", async () => {
    const client = await verifiedNumericClient();
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(
        JSON.stringify({
          items: [
            {
              attester: ATTESTER,
              posted_at_unix: 1760000000, // 2025-10-09
              eval_result_uri: "xvn://eval/listing/3",
              eval_result_hash: "0x" + "ab".repeat(32),
              schema: "0x" + "00".repeat(32),
            },
          ],
        }),
        { status: 200, headers: { "content-type": "application/json" } },
      ),
    );
    vi.stubGlobal("fetch", fetchMock);

    render(<Wrapper client={client} />);

    expect(await screen.findByTestId("verified-evals")).toBeInTheDocument();
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/marketplace/listings/3/attestations",
      expect.anything(),
    );
    // attester truncated in the middle
    expect(screen.getByText(/0xa83e…0001/)).toBeInTheDocument();
    // date derived from posted_at_unix
    expect(screen.getByText(/2025/)).toBeInTheDocument();
    // eval result uri rendered as text
    expect(screen.getByText("xvn://eval/listing/3")).toBeInTheDocument();
    // honest wording: self-attestations render as "attested", never "verified"
    expect(screen.getByText("Eval attestations")).toBeInTheDocument();
    expect(screen.getByText(/attested/i)).toBeInTheDocument();
    expect(screen.queryByText(/endorsed/i)).not.toBeInTheDocument();
  });

  it("does not fetch or render the section for fixture (non-numeric) listings", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);

    render(<Wrapper />); // fixture detail: verified but slug id
    await screen.findByTestId("lineage-page");

    expect(screen.queryByTestId("verified-evals")).not.toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
  });

  it("does not fetch attestations for unverified on-chain listings", async () => {
    const client = new FixtureMarketplaceData();
    const base = await new FixtureMarketplaceData().getListing(
      "btc-momentum-v3",
    );
    vi.spyOn(client, "getListing").mockResolvedValue({
      ...base,
      id: "3",
      verification: "unverified",
    });
    const fetchMock = vi.fn().mockRejectedValue(new Error("offline"));
    vi.stubGlobal("fetch", fetchMock);

    render(<Wrapper client={client} />);
    await screen.findByTestId("lineage-page");

    expect(screen.queryByTestId("verified-evals")).not.toBeInTheDocument();
    // The bundle enrichment fetch may fire for numeric ids; the attestations
    // route specifically must not.
    expect(fetchMock).not.toHaveBeenCalledWith(
      "/api/marketplace/listings/3/attestations",
      expect.anything(),
    );
  });
});

// ── Bundle manifest enrichment (real on-chain listings) ─────────────────────
describe("LineageRoute bundle enrichment", () => {
  afterEach(() => vi.unstubAllGlobals());

  const SELLER = "0x7c2e000000000000000000000000000000000007";

  async function numericClient() {
    const client = new FixtureMarketplaceData();
    const base = await new FixtureMarketplaceData().getListing(
      "btc-momentum-v3",
    );
    vi.spyOn(client, "getListing").mockResolvedValue({
      ...base,
      id: "3",
      creator: { address: SELLER },
      verification: "unverified",
    });
    return client;
  }

  function stubBundleFetch() {
    const bundle = {
      listing_id: 3,
      content_uri: "ipfs://bafytestcid",
      verified: true,
      manifest: {
        manifest: {
          id: "01HSTRAT",
          display_name: "BTC Dip Buyer",
          plain_summary: "Buys BTC dips with a momentum confirmation gate.",
          creator: "@ed",
          attested_with: ["claude-haiku-4.5"],
          required_tools: ["birdeye-mcp"],
        },
      },
    };
    const fetchMock = vi.fn(async (url: RequestInfo | URL) => {
      if (String(url).endsWith("/bundle")) {
        return new Response(JSON.stringify(bundle), {
          status: 200,
          headers: { "content-type": "application/json" },
        });
      }
      return new Response("{}", { status: 404 });
    });
    vi.stubGlobal("fetch", fetchMock);
    return fetchMock;
  }

  it("replaces the generic title with the manifest display_name", async () => {
    stubBundleFetch();
    render(<Wrapper client={await numericClient()} />);
    expect(await screen.findByText("BTC Dip Buyer")).toBeInTheDocument();
  });

  it("renders the About section with the plain summary", async () => {
    stubBundleFetch();
    render(<Wrapper client={await numericClient()} />);
    const about = await screen.findByTestId("about-strategy");
    expect(about).toHaveTextContent("About this strategy");
    expect(about).toHaveTextContent(/momentum confirmation gate/);
  });

  it("renders requirement chips for attested models + required tools, with the note", async () => {
    stubBundleFetch();
    render(<Wrapper client={await numericClient()} />);
    // The row appears immediately (it leads with the listing's model); wait for
    // the async manifest fetch to add the attested-model + tool chips.
    await screen.findByText("birdeye-mcp");
    const row = screen.getByTestId("requirements-row");
    expect(row).toHaveTextContent("claude-haiku-4.5");
    expect(row).toHaveTextContent("birdeye-mcp");
    expect(row).toHaveTextContent(/you'll need these to run the strategy after purchase/i);
  });

  it("links the seller address to the creator page for real listings", async () => {
    stubBundleFetch();
    render(<Wrapper client={await numericClient()} />);
    const link = await screen.findByTestId("creator-link");
    expect(link).toHaveAttribute("href", `/marketplace/creator/${SELLER}`);
  });

  it("renders no enrichment when the bundle route errors (fixture parity)", async () => {
    const fetchMock = vi.fn().mockResolvedValue(
      new Response(JSON.stringify({ code: "not_found", message: "nope" }), {
        status: 404,
        headers: { "content-type": "application/json" },
      }),
    );
    vi.stubGlobal("fetch", fetchMock);
    render(<Wrapper client={await numericClient()} />);
    await screen.findByTestId("lineage-page");
    // Manifest-derived enrichment (about + tool requirements) stays gone on a
    // bundle error, but the run-requirements row still leads with the model the
    // listing runs on, so it renders without the manifest's tool chips.
    expect(screen.queryByTestId("about-strategy")).not.toBeInTheDocument();
    const reqRow = await screen.findByTestId("requirements-row");
    expect(reqRow).not.toHaveTextContent("birdeye-mcp");
    expect(reqRow).toHaveTextContent(/you'll need these to run the strategy/i);
  });

  it("never fetches the bundle for fixture (slug) listings and keeps the fixture title", async () => {
    const fetchMock = vi.fn();
    vi.stubGlobal("fetch", fetchMock);
    render(<Wrapper />); // fixture slug listing
    await screen.findByTestId("lineage-page");
    // Fixture title is the listing's display name, not the raw id.
    expect(screen.getByText("BTC Momentum v3")).toBeInTheDocument();
    expect(fetchMock).not.toHaveBeenCalled();
    expect(screen.queryByTestId("creator-link")).not.toBeInTheDocument();
  });
});

// ── Owner strip ──────────────────────────────────────────────────────────────
describe("LineageRoute owner strip", () => {
  it("shows owner strip when viewer.createdListingIds includes the listing id", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "getViewer").mockResolvedValue({
      isConnected: true,
      address: "0xowner",
      createdListingIds: ["btc-momentum-v3"],
      ownedListingIds: [],
    });
    render(<Wrapper client={client} />);
    expect(await screen.findByTestId("owner-strip")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /edit price/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /^revoke$/i })).toBeInTheDocument();
  });

  it("does NOT show owner strip when viewer does not own the listing", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "getViewer").mockResolvedValue({
      isConnected: true,
      address: "0xstranger",
      createdListingIds: [],
      ownedListingIds: [],
    });
    render(<Wrapper client={client} />);
    await screen.findByTestId("lineage-page");
    expect(screen.queryByTestId("owner-strip")).not.toBeInTheDocument();
  });

  it("does NOT show owner strip when viewer is not connected", async () => {
    const client = new FixtureMarketplaceData();
    vi.spyOn(client, "getViewer").mockResolvedValue({
      isConnected: false,
      createdListingIds: [],
      ownedListingIds: [],
    });
    render(<Wrapper client={client} />);
    await screen.findByTestId("lineage-page");
    expect(screen.queryByTestId("owner-strip")).not.toBeInTheDocument();
  });
});
