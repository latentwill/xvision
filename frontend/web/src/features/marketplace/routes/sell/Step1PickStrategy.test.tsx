// src/features/marketplace/routes/sell/Step1PickStrategy.test.tsx
import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { renderMarketplace } from "@/features/marketplace/test-utils";
import { Step1PickStrategy } from "./Step1PickStrategy";

function renderStep(onSelect = vi.fn()) {
  renderMarketplace(<Step1PickStrategy onSelect={onSelect} />, {
    path: "/",
    route: "/",
  });
  return { onSelect };
}

describe("Step1PickStrategy", () => {
  it("shows all fixture strategies after load", async () => {
    renderStep();
    expect(await screen.findByText("btc-momentum")).toBeInTheDocument();
    expect(screen.getByText("eth-mr")).toBeInTheDocument();
    expect(screen.getByText("wip-draft")).toBeInTheDocument();
  });

  it("shows asset pills for strategies with assets", async () => {
    renderStep();
    await screen.findByText("btc-momentum");
    expect(screen.getByText("BTC")).toBeInTheDocument();
    expect(screen.getByText("ETH")).toBeInTheDocument();
  });

  it("shows 'no assets' hint for wip-draft (no assets configured)", async () => {
    renderStep();
    await screen.findByText("btc-momentum");
    expect(screen.getByText(/no assets/i)).toBeInTheDocument();
  });

  it("calls onSelect with the full ListableStrategy when clicked", async () => {
    const { onSelect } = renderStep();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    expect(onSelect).toHaveBeenCalledOnce();
    expect(onSelect.mock.calls[0][0]).toMatchObject({ id: "local-btc-momentum", name: "btc-momentum" });
  });

  it("shows version string in mono", async () => {
    renderStep();
    expect(await screen.findByText("v3.0")).toBeInTheDocument();
  });
});
