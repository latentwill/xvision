// src/features/marketplace/routes/sell/Step3Preview.test.tsx
import { render as rtlRender, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Step3Preview } from "./Step3Preview";
import type { PublishDraft } from "@/features/marketplace/data/types";

// Step3Preview prefills the public description from the stored strategy.
vi.mock("@/api/strategies", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/api/strategies")>();
  return { ...actual, getStrategy: vi.fn() };
});
import { getStrategy } from "@/api/strategies";
const mockedGetStrategy = vi.mocked(getStrategy);

function render(ui: React.ReactElement) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return rtlRender(<QueryClientProvider client={qc}>{ui}</QueryClientProvider>);
}

beforeEach(() => {
  vi.clearAllMocks();
  // default: local engine unreachable — textarea starts empty
  mockedGetStrategy.mockRejectedValue(new Error("engine unreachable"));
});

const happyDraft: PublishDraft = {
  strategyId: "local-btc-momentum",
  name: "BTC Momentum",
  listable: [
    { ok: true, label: "Strategy exists in your XVN" },
    { ok: true, label: "Declares an asset universe" },
    { ok: true, label: "Has a backtest on record" },
  ],
  tier: "sealed",
  priceUsdc: 49,
  acceptedPayers: { humans: true, agents: true },
  ingredients: [
    { name: "Claude Haiku 4.5", kind: "model", installed: true },
    { name: "Birdeye MCP", kind: "mcp", installed: false },
  ],
  preview: {
    id: "btc-momentum", lineageId: "btc-momentum", version: "v3.0",
    creator: { address: "0xa83e", handle: "@ed" }, model: "Claude · Haiku 4.5", style: "Day",
    assets: ["BTC"], return30dPct: 47.2, sharpe: 1.31, buyers: { humans: 0, agents: 0 },
    priceUsdc: 49, tier: "sealed", verification: "unverified", acceptsX402: true, clones: 0,
    transferableLicense: false, genArtSeed: "btc-momentum-preview",
  },
};

const failingDraft: PublishDraft = {
  ...happyDraft,
  listable: [
    ...happyDraft.listable.slice(0, 1),
    { ok: false, label: "Declares an asset universe", reason: "No assets configured" },
    happyDraft.listable[2],
  ],
  preview: {
    ...happyDraft.preview,
    id: "wip-draft",
    assets: [],
  },
};

describe("Step3Preview", () => {
  it("renders the listing preview card with the strategy id", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText("btc-momentum")).toBeInTheDocument();
  });

  it("renders the gen-art placeholder inside the preview card", () => {
    const { container } = render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(container.querySelector('[data-genart="bitfields-v3"]')).not.toBeNull();
  });

  it("keeps identity art inside the listing preview instead of a separate duplicate preview", () => {
    const { container } = render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    const canvases = container.querySelectorAll('[data-genart="bitfields-v3"]');
    expect(canvases).toHaveLength(1);
    expect(screen.queryByText(/^Identity art preview$/)).not.toBeInTheDocument();
  });

  it("lists all ingredients with their kind label", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText("Claude Haiku 4.5")).toBeInTheDocument();
    expect(screen.getByText("Birdeye MCP")).toBeInTheDocument();
    expect(screen.getAllByText(/model|mcp/i).length).toBeGreaterThanOrEqual(2);
  });

  it("renders an honest empty state when no bundle ingredients are known", () => {
    render(
      <Step3Preview
        draft={{ ...happyDraft, ingredients: [] }}
        onMint={vi.fn()}
        minting={false}
      />,
    );
    expect(screen.getByText(/No bundle ingredients detected/i)).toBeInTheDocument();
    expect(screen.queryByText("Claude Haiku 4.5")).not.toBeInTheDocument();
    expect(screen.queryByText("Birdeye MCP")).not.toBeInTheDocument();
  });

  it("Mint button is enabled for happy draft", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByRole("button", { name: /Mint/ })).not.toBeDisabled();
  });

  it("Mint button carries the shared Testnet badge", () => {
    // C8: the hand-rolled "[Testnet]" string is replaced by the shared
    // TestnetBadge component, which renders the text "Testnet".
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByRole("button", { name: /Mint/ }).textContent).toContain("Testnet");
  });

  it("Mint button is disabled when any listability check fails", () => {
    render(<Step3Preview draft={failingDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByRole("button", { name: /Mint/ })).toBeDisabled();
  });

  it("shows a warning message when Mint is disabled due to check failures", () => {
    render(<Step3Preview draft={failingDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText(/Mint is disabled/)).toBeInTheDocument();
  });

  it("Mint button is disabled while minting=true", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={true} />);
    expect(screen.getByRole("button", { name: /Minting/ })).toBeDisabled();
  });

  it("clicking Mint calls onMint", async () => {
    const onMint = vi.fn();
    render(<Step3Preview draft={happyDraft} onMint={onMint} minting={false} />);
    await userEvent.click(screen.getByRole("button", { name: /Mint/ }));
    expect(onMint).toHaveBeenCalledOnce();
  });

  // ── public description editor ──────────────────────────────────────────────
  describe("public description", () => {
    function strategyWithSummary(summary: string) {
      return {
        manifest: { plain_summary: summary },
      } as Awaited<ReturnType<typeof getStrategy>>;
    }

    it("prefills from the stored strategy's manifest.plain_summary", async () => {
      mockedGetStrategy.mockResolvedValue(strategyWithSummary("Buys dips."));
      render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
      await waitFor(() => {
        expect(screen.getByTestId("public-description")).toHaveValue("Buys dips.");
      });
      expect(mockedGetStrategy).toHaveBeenCalledWith("local-btc-momentum");
    });

    it("shows the IPFS-publication helper text", () => {
      render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
      expect(
        screen.getByText(/Published publicly to IPFS with your strategy/i),
      ).toBeInTheDocument();
    });

    it("renders an empty editable textarea when the strategy fetch fails", async () => {
      render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
      const ta = screen.getByTestId("public-description");
      expect(ta).toHaveValue("");
      expect(ta).toBeEnabled();
    });

    it("Mint passes dirty: false when the description is untouched", async () => {
      mockedGetStrategy.mockResolvedValue(strategyWithSummary("Buys dips."));
      const onMint = vi.fn();
      render(<Step3Preview draft={happyDraft} onMint={onMint} minting={false} />);
      await waitFor(() =>
        expect(screen.getByTestId("public-description")).toHaveValue("Buys dips."),
      );
      await userEvent.click(screen.getByRole("button", { name: /Mint/ }));
      expect(onMint).toHaveBeenCalledWith({ value: "Buys dips.", dirty: false });
    });

    it("Mint passes the edited text with dirty: true", async () => {
      mockedGetStrategy.mockResolvedValue(strategyWithSummary("Buys dips."));
      const onMint = vi.fn();
      render(<Step3Preview draft={happyDraft} onMint={onMint} minting={false} />);
      await waitFor(() =>
        expect(screen.getByTestId("public-description")).toHaveValue("Buys dips."),
      );
      const ta = screen.getByTestId("public-description");
      await userEvent.clear(ta);
      await userEvent.type(ta, "Buys dips with confirmation.");
      await userEvent.click(screen.getByRole("button", { name: /Mint/ }));
      expect(onMint).toHaveBeenCalledWith({
        value: "Buys dips with confirmation.",
        dirty: true,
      });
    });

    it("reverting an edit back to the stored text passes dirty: false", async () => {
      mockedGetStrategy.mockResolvedValue(strategyWithSummary("Buys dips."));
      const onMint = vi.fn();
      render(<Step3Preview draft={happyDraft} onMint={onMint} minting={false} />);
      await waitFor(() =>
        expect(screen.getByTestId("public-description")).toHaveValue("Buys dips."),
      );
      const ta = screen.getByTestId("public-description");
      await userEvent.clear(ta);
      await userEvent.type(ta, "Buys dips.");
      await userEvent.click(screen.getByRole("button", { name: /Mint/ }));
      expect(onMint).toHaveBeenCalledWith({ value: "Buys dips.", dirty: false });
    });
  });

  it("preview card shows asset pill for BTC", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText("BTC")).toBeInTheDocument();
  });

  it("preview card shows 'No assets configured' for empty assets", () => {
    render(<Step3Preview draft={failingDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText(/No assets configured/)).toBeInTheDocument();
  });
});
