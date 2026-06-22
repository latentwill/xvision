import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { AssetPicker } from "./AssetPicker";
import type { AssetInfo } from "@/api/assets";

function asset(symbol: string, category = "crypto", data = "alpaca"): AssetInfo {
  return {
    symbol,
    category,
    data,
    venues: {},
    enabled: true,
  };
}

afterEach(() => cleanup());

describe("AssetPicker", () => {
  it("filters assets and selects the highlighted asset with keyboard", async () => {
    const onChange = vi.fn();
    const user = userEvent.setup();

    render(
      <AssetPicker
        assets={[asset("BTC/USD"), asset("ETH/USD")]}
        value=""
        onChange={onChange}
      />,
    );

    await user.click(screen.getByRole("button", { name: "Asset picker" }));
    await user.type(screen.getByRole("textbox", { name: "Search Asset picker" }), "eth");
    await user.keyboard("{ArrowDown}{Enter}");

    expect(onChange).toHaveBeenCalledWith("ETH/USD");
  });

  it("shows orderly-only assets with the no-backtest badge", async () => {
    const user = userEvent.setup();

    render(
      <AssetPicker
        assets={[asset("ORDERLY-PERP", "perp", "orderly-only")]}
        value=""
        onChange={() => {}}
        showOrderlyOnlyBadge
      />,
    );

    await user.click(screen.getByRole("button", { name: "Asset picker" }));

    expect(await screen.findByText("no backtest data")).toBeInTheDocument();
  });
});
