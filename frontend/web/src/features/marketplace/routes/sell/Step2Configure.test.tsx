// src/features/marketplace/routes/sell/Step2Configure.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Step2Configure } from "./Step2Configure";
import type { PublishDraft } from "@/features/marketplace/data/types";

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
  ingredients: [{ name: "Claude Haiku 4.5", kind: "model", installed: true }],
  preview: {
    id: "btc-momentum", lineageId: "btc-momentum", version: "v3.0",
    creator: { address: "0xa83e", handle: "@ed" }, model: "Claude · Haiku 4.5", style: "Day",
    assets: ["BTC"], return30dPct: 47.2, sharpe: 1.31, buyers: { humans: 0, agents: 0 },
    priceUsdc: 49, tier: "sealed", verification: "unverified", acceptsX402: true,
    transferableLicense: false, genArtSeed: "btc-momentum-preview",
  },
};

const failingDraft: PublishDraft = {
  ...happyDraft,
  strategyId: "local-wip-draft",
  listable: [
    { ok: true, label: "Strategy exists in your XVN" },
    { ok: false, label: "Declares an asset universe", reason: "No assets configured" },
    { ok: true, label: "Has a backtest on record" },
  ],
};

describe("Step2Configure", () => {
  it("renders all listability check labels", () => {
    render(<Step2Configure draft={happyDraft} onUpdate={vi.fn()} onNext={vi.fn()} />);
    expect(screen.getByText("Strategy exists in your XVN")).toBeInTheDocument();
    expect(screen.getByText("Declares an asset universe")).toBeInTheDocument();
    expect(screen.getByText("Has a backtest on record")).toBeInTheDocument();
  });

  it("shows failure reason inline for failing checks", () => {
    render(<Step2Configure draft={failingDraft} onUpdate={vi.fn()} onNext={vi.fn()} />);
    expect(screen.getByText(/No assets configured/)).toBeInTheDocument();
  });

  it("Continue is disabled when any check fails", () => {
    render(<Step2Configure draft={failingDraft} onUpdate={vi.fn()} onNext={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Continue/ })).toBeDisabled();
  });

  it("Continue is enabled when all checks pass", () => {
    render(<Step2Configure draft={happyDraft} onUpdate={vi.fn()} onNext={vi.fn()} />);
    expect(screen.getByRole("button", { name: /Continue/ })).not.toBeDisabled();
  });

  it("clicking Continue calls onNext", async () => {
    const onNext = vi.fn();
    render(<Step2Configure draft={happyDraft} onUpdate={vi.fn()} onNext={onNext} />);
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    expect(onNext).toHaveBeenCalledOnce();
  });

  it("price input is visible for Tier B (sealed)", () => {
    render(<Step2Configure draft={happyDraft} onUpdate={vi.fn()} onNext={vi.fn()} />);
    expect(screen.getByTestId("price-input")).toBeInTheDocument();
  });

  it("price input is hidden for Tier A (open)", () => {
    render(
      <Step2Configure
        draft={{ ...happyDraft, tier: "open", priceUsdc: null }}
        onUpdate={vi.fn()}
        onNext={vi.fn()}
      />,
    );
    expect(screen.queryByTestId("price-input")).not.toBeInTheDocument();
  });

  it("switching to Tier A calls onUpdate with priceUsdc: null", async () => {
    const onUpdate = vi.fn();
    render(<Step2Configure draft={happyDraft} onUpdate={onUpdate} onNext={vi.fn()} />);
    await userEvent.click(screen.getByTestId("tier-open-btn"));
    expect(onUpdate).toHaveBeenCalledWith(expect.objectContaining({ tier: "open", priceUsdc: null }));
  });

  it("changing price input calls onUpdate with the new value", () => {
    const onUpdate = vi.fn();
    render(<Step2Configure draft={happyDraft} onUpdate={onUpdate} onNext={vi.fn()} />);
    const input = screen.getByTestId("price-input") as HTMLInputElement;
    fireEvent.change(input, { target: { value: "99" } });
    expect(onUpdate).toHaveBeenCalledWith(expect.objectContaining({ priceUsdc: 99 }));
  });

  it("payer checkboxes toggle acceptedPayers", async () => {
    const onUpdate = vi.fn();
    render(<Step2Configure draft={happyDraft} onUpdate={onUpdate} onNext={vi.fn()} />);
    await userEvent.click(screen.getByTestId("payer-agents"));
    expect(onUpdate).toHaveBeenCalledWith(
      expect.objectContaining({ acceptedPayers: { humans: true, agents: false } }),
    );
  });

  // QA fix: the listing name is editable at mint and pre-filled with the
  // strategy's display name so listings never render "Strategy #N".
  it("listing name input shows the draft name (defaulted to the strategy name)", () => {
    render(<Step2Configure draft={happyDraft} onUpdate={vi.fn()} onNext={vi.fn()} />);
    expect(screen.getByTestId("listing-name-input")).toHaveValue("BTC Momentum");
  });

  it("editing the listing name calls onUpdate with the new name", () => {
    const onUpdate = vi.fn();
    render(<Step2Configure draft={happyDraft} onUpdate={onUpdate} onNext={vi.fn()} />);
    fireEvent.change(screen.getByTestId("listing-name-input"), {
      target: { value: "My Renamed Listing" },
    });
    expect(onUpdate).toHaveBeenCalledWith(
      expect.objectContaining({ name: "My Renamed Listing" }),
    );
  });

  it("Continue is disabled when the listing name is blank even if all checks pass", () => {
    render(
      <Step2Configure draft={{ ...happyDraft, name: "   " }} onUpdate={vi.fn()} onNext={vi.fn()} />,
    );
    expect(screen.getByRole("button", { name: /Continue/ })).toBeDisabled();
    expect(screen.getByText(/Give your listing a name/)).toBeInTheDocument();
  });
});
