// Tests for DegenDeployStrip — inline deploy strip for the Degen Arena
// (Hyperliquid via Virtuals) venue. Covers: docs link, helper copy, key
// validation, address validation, network toggle, onDeploy payload shape,
// footer, and absence of fund-capable prompts.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DegenDeployStrip } from "./DegenDeployStrip";

// A valid 0x-prefixed 64-hex private key (66 characters total: "0x" + 64 hex digits).
const VALID_KEY =
  "0xabcdef1234567890abcdef1234567890abcdef1234567890abcdef1234567890";

// A valid EVM account address (0x + 40 hex digits = 42 chars total).
const VALID_ADDR = "0xAbCdEf1234567890AbCdEf1234567890AbCdEf12";

afterEach(() => {
  cleanup();
});

function renderStrip(
  onDeploy = vi.fn(),
  venue: "orderly" | "degen-arena" = "degen-arena",
) {
  return render(
    <DegenDeployStrip venue={venue} onDeploy={onDeploy} />,
  );
}

/** Fill in both valid fields so the button can be enabled. */
async function fillValid(user: ReturnType<typeof userEvent.setup>) {
  await user.type(screen.getByTestId("hl-api-key-input"), VALID_KEY);
  await user.type(screen.getByTestId("account-address-input"), VALID_ADDR);
}

describe("DegenDeployStrip", () => {
  it("renders the Virtuals docs link with the correct href", () => {
    renderStrip();
    const link = screen.getByTestId("virtuals-docs-link");
    expect(link).toBeInTheDocument();
    expect(link).toHaveAttribute("href", "https://degen.virtuals.io/docs");
    expect(link).toHaveAttribute("target", "_blank");
    expect(link).toHaveAttribute("rel", "noopener noreferrer");
  });

  it("renders the API key field with its helper copy", () => {
    renderStrip();
    expect(screen.getByTestId("hl-api-key-input")).toBeInTheDocument();
    expect(screen.getByTestId("api-key-helper")).toHaveTextContent(
      "Add your key if you haven't already.",
    );
  });

  it("keeps 'Go live' button disabled and shows hint when key is invalid", async () => {
    const user = userEvent.setup();
    renderStrip();

    const input = screen.getByTestId("hl-api-key-input");
    await user.type(input, "not-a-valid-key");

    expect(screen.getByTestId("go-live-btn")).toBeDisabled();
    expect(screen.getByTestId("api-key-hint")).toBeInTheDocument();
    expect(screen.getByTestId("api-key-hint")).toHaveTextContent(
      /0x-prefixed 64-hex/i,
    );
  });

  it("keeps 'Go live' button disabled when key is just 0x prefix", async () => {
    const user = userEvent.setup();
    renderStrip();
    await user.type(screen.getByTestId("hl-api-key-input"), "0x");
    expect(screen.getByTestId("go-live-btn")).toBeDisabled();
  });

  it("keeps 'Go live' disabled when only the key is valid but address is missing", async () => {
    const user = userEvent.setup();
    renderStrip();
    await user.type(screen.getByTestId("hl-api-key-input"), VALID_KEY);
    // Address field left empty — button must stay disabled.
    expect(screen.getByTestId("go-live-btn")).toBeDisabled();
  });

  it("keeps 'Go live' disabled when only the address is valid but key is missing", async () => {
    const user = userEvent.setup();
    renderStrip();
    await user.type(screen.getByTestId("account-address-input"), VALID_ADDR);
    expect(screen.getByTestId("go-live-btn")).toBeDisabled();
  });

  it("shows the address validation hint when address is typed but invalid", async () => {
    const user = userEvent.setup();
    renderStrip();
    await user.type(screen.getByTestId("account-address-input"), "not-an-address");
    expect(screen.getByTestId("account-address-hint")).toBeInTheDocument();
    expect(screen.getByTestId("account-address-hint")).toHaveTextContent(
      /0x-prefixed 40-hex/i,
    );
  });

  it("enables 'Go live' button when BOTH valid key AND valid address are entered", async () => {
    const user = userEvent.setup();
    renderStrip();
    await fillValid(user);
    expect(screen.getByTestId("go-live-btn")).not.toBeDisabled();
    // No validation hints shown for valid inputs.
    expect(screen.queryByTestId("api-key-hint")).not.toBeInTheDocument();
    expect(screen.queryByTestId("account-address-hint")).not.toBeInTheDocument();
  });

  it("calls onDeploy with apiKey, accountAddress, and network when 'Go live' is clicked", async () => {
    const onDeploy = vi.fn();
    const user = userEvent.setup();
    renderStrip(onDeploy);

    await fillValid(user);
    await user.click(screen.getByTestId("go-live-btn"));

    expect(onDeploy).toHaveBeenCalledOnce();
    expect(onDeploy).toHaveBeenCalledWith({
      apiKey: VALID_KEY,
      accountAddress: VALID_ADDR,
      network: "testnet",
    });
  });

  it("payload network is 'mainnet' when mainnet toggle is selected", async () => {
    const onDeploy = vi.fn();
    const user = userEvent.setup();
    renderStrip(onDeploy);

    await fillValid(user);
    await user.click(screen.getByTestId("network-mainnet"));
    await user.click(screen.getByTestId("go-live-btn"));

    expect(onDeploy).toHaveBeenCalledWith(
      expect.objectContaining({ network: "mainnet" }),
    );
  });

  it("network defaults to 'testnet' and its radio is checked", () => {
    renderStrip();
    const testnetRadio = screen.getByTestId("network-testnet");
    const mainnetRadio = screen.getByTestId("network-mainnet");
    expect(testnetRadio).toBeChecked();
    expect(mainnetRadio).not.toBeChecked();
  });

  it("does NOT call onDeploy when the button is clicked while key is invalid", () => {
    const onDeploy = vi.fn();
    renderStrip(onDeploy);
    // Button is disabled — fireEvent ignores disabled, so check the guard.
    fireEvent.click(screen.getByTestId("go-live-btn"));
    expect(onDeploy).not.toHaveBeenCalled();
  });

  it("renders the 'Powered by Virtuals Protocol' footer", () => {
    renderStrip();
    expect(screen.getByTestId("virtuals-footer")).toHaveTextContent(
      "Powered by Virtuals Protocol",
    );
  });

  it("does NOT render any fund / deposit / withdraw prompt", () => {
    const { container } = renderStrip();
    const text = container.textContent ?? "";
    // None of these finance-action words should appear.
    expect(text).not.toMatch(/\bfund\b/i);
    expect(text).not.toMatch(/\bdeposit\b/i);
    expect(text).not.toMatch(/\bwithdraw\b/i);
    expect(text).not.toMatch(/\btransfer\b/i);
  });

  it("does not show the Degen Arena detail when 'orderly' venue is selected", () => {
    renderStrip(vi.fn(), "orderly");
    expect(screen.queryByTestId("degen-arena-detail")).not.toBeInTheDocument();
    // Footer still present.
    expect(screen.getByTestId("virtuals-footer")).toBeInTheDocument();
  });

  it("shows the 'Go live' label text", async () => {
    renderStrip();
    const btn = screen.getByTestId("go-live-btn");
    expect(btn).toHaveTextContent(/map strategy.*go live/i);
  });
});
