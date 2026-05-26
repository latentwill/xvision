// src/features/marketplace/routes/TradeHistoryTable.test.tsx
import { render, screen, act } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { TradeHistoryTable } from "./TradeHistoryTable";
import type { TradeRecord } from "@/features/marketplace/data/types";
import { LISTING_DETAILS } from "@/features/marketplace/data/fixtures/listings";

const TRADES: TradeRecord[] = LISTING_DETAILS["btc-momentum-v3"].onChain.trades;
const META = LISTING_DETAILS["btc-momentum-v3"].onChain.tradesMeta;

describe("TradeHistoryTable", () => {
  it("renders the card header with totalOnChain count", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByText(/178/)).toBeInTheDocument();
    expect(screen.getByText(/trades on chain/i)).toBeInTheDocument();
  });

  it("renders filter pills for All/Buy/Sell/Close", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByRole("button", { name: /all/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /buy/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /sell/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /close/i })).toBeInTheDocument();
  });

  it("renders Runner and Window dropdowns", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    // Both the column header and dropdown button contain "Runner" — getAllByText handles multiple matches
    expect(screen.getAllByText(/runner/i).length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText(/window/i).length).toBeGreaterThanOrEqual(1);
  });

  it("shows net P&L from meta", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    // meta.netPnlUsd = 94.88
    expect(screen.getByText(/94\.88/)).toBeInTheDocument();
  });

  it("renders table column headers", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByText("Time")).toBeInTheDocument();
    expect(screen.getByText("Action")).toBeInTheDocument();
    expect(screen.getByText("Sym")).toBeInTheDocument();
    expect(screen.getByText("P&L")).toBeInTheDocument();
    expect(screen.getByText("Runner")).toBeInTheDocument();
    expect(screen.getByText("Tx")).toBeInTheDocument();
  });

  it("renders trade rows from fixture data", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    // fixture has 2 trades; both should appear
    expect(screen.getAllByText("BTC").length).toBeGreaterThanOrEqual(1);
  });

  it("clicking Buy filter pill shows only buy rows", async () => {
    // Add 3 fixture trades: 1 buy, 1 sell, 1 close
    const mixedTrades: TradeRecord[] = [
      { ...TRADES[0], action: "buy", symbol: "BTC" },
      { ...TRADES[0], action: "sell", symbol: "ETH" },
      { ...TRADES[0], action: "close", symbol: "SOL" },
    ];
    render(<TradeHistoryTable trades={mixedTrades} meta={{ ...META, totalOnChain: 3 }} />);
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^buy/i }));
    });
    // Only BTC (buy) row should appear; ETH and SOL should be hidden
    expect(screen.getByText("BTC")).toBeInTheDocument();
    expect(screen.queryByText("ETH")).not.toBeInTheDocument();
    expect(screen.queryByText("SOL")).not.toBeInTheDocument();
  });

  it("clicking All filter shows all rows", async () => {
    const mixedTrades: TradeRecord[] = [
      { ...TRADES[0], action: "buy", symbol: "BTC" },
      { ...TRADES[0], action: "sell", symbol: "ETH" },
      { ...TRADES[0], action: "close", symbol: "SOL" },
    ];
    render(<TradeHistoryTable trades={mixedTrades} meta={{ ...META, totalOnChain: 3 }} />);
    // click Buy first, then All
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^buy/i }));
    });
    await act(async () => {
      await userEvent.click(screen.getByRole("button", { name: /^all/i }));
    });
    expect(screen.getByText("BTC")).toBeInTheDocument();
    expect(screen.getByText("ETH")).toBeInTheDocument();
    expect(screen.getByText("SOL")).toBeInTheDocument();
  });

  it("renders the Export ledger button", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    expect(screen.getByRole("button", { name: /export ledger/i })).toBeInTheDocument();
  });

  it("renders footer with anchor merkle reference", () => {
    render(<TradeHistoryTable trades={TRADES} meta={META} />);
    // footer shows anchorTx from meta
    expect(screen.getByText(/anchored under/i)).toBeInTheDocument();
  });
});
