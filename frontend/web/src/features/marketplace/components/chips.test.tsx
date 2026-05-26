// src/features/marketplace/components/chips.test.tsx
import { render, screen, fireEvent } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { RemovableChip } from "./RemovableChip";
import { TxChip } from "./TxChip";

describe("RemovableChip", () => {
  it("fires onRemove when the × is clicked", () => {
    const onRemove = vi.fn();
    render(<RemovableChip onRemove={onRemove}>Asset: BTC</RemovableChip>);
    fireEvent.click(screen.getByRole("button", { name: /remove/i }));
    expect(onRemove).toHaveBeenCalledOnce();
  });
});

describe("TxChip", () => {
  it("shows a truncated hash and an external link", () => {
    render(<TxChip hash="0x2e1d…44a9" />);
    expect(screen.getByText("0x2e1d…44a9")).toBeInTheDocument();
    expect(screen.getByRole("link")).toHaveAttribute("href");
  });
  it("renders a Testnet marker when network is a testnet", () => {
    render(<TxChip hash="0x1" network="mantle-sepolia" />);
    expect(screen.getByText(/testnet/i)).toBeInTheDocument();
  });
});
