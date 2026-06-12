// src/features/marketplace/components/TxChip.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TxChip } from "./TxChip";

describe("TxChip", () => {
  it("renders a label when provided", () => {
    render(<TxChip hash="0xabc123" network="mantle-sepolia" label="View on explorer" />);
    expect(screen.getByText("View on explorer")).toBeInTheDocument();
  });

  it("does not render a label element when label is absent", () => {
    render(<TxChip hash="0xabc123" network="mantle-sepolia" />);
    // Should just have the hash link, no label span
    expect(screen.queryByText("View on explorer")).not.toBeInTheDocument();
  });

  it("uses explorer.sepolia.mantle.xyz for mantle-sepolia network", () => {
    render(<TxChip hash="0xabc123" network="mantle-sepolia" label="View" />);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "https://explorer.sepolia.mantle.xyz/tx/0xabc123");
  });

  it("uses explorer.sepolia.mantle.xyz for sepolia network", () => {
    render(<TxChip hash="0xdef456" network="sepolia" />);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "https://explorer.sepolia.mantle.xyz/tx/0xdef456");
  });

  it("uses explorer.sepolia.mantle.xyz for testnet network", () => {
    render(<TxChip hash="0xfeed" network="testnet" />);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "https://explorer.sepolia.mantle.xyz/tx/0xfeed");
  });

  it("uses explorer.mantle.xyz for mainnet mantle network", () => {
    render(<TxChip hash="0x789abc" network="mantle" />);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "https://explorer.mantle.xyz/tx/0x789abc");
  });

  it("defaults to explorer.sepolia.mantle.xyz when no network supplied", () => {
    render(<TxChip hash="0x123" />);
    const link = screen.getByRole("link");
    expect(link).toHaveAttribute("href", "https://explorer.sepolia.mantle.xyz/tx/0x123");
  });

  it("never uses the old mantlescan.xyz domain", () => {
    const { container } = render(<TxChip hash="0xabc" network="mantle-sepolia" />);
    expect(container.innerHTML).not.toContain("mantlescan.xyz");
  });
});
