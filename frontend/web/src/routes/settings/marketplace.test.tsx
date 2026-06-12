import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { SettingsMarketplaceRoute } from "./marketplace";

afterEach(() => {
  localStorage.clear();
});

describe("SettingsMarketplaceRoute", () => {
  it("does not expose Marketplace Testnet wording in Settings", () => {
    render(<SettingsMarketplaceRoute />);

    expect(screen.getByRole("heading", { name: "Marketplace" })).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Enable Marketplace" })).toBeInTheDocument();
    expect(screen.queryByText(/Testnet/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/Marketplace Testnet/i)).not.toBeInTheDocument();
  });

  it("frames seller-facing settings as Marketplace Profile", () => {
    render(<SettingsMarketplaceRoute />);

    expect(
      screen.getByRole("heading", { name: "Marketplace Profile" }),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^Identity$/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/On-Chain Identity/i)).not.toBeInTheDocument();
  });
});
