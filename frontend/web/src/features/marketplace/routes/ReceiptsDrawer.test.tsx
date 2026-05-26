// src/features/marketplace/routes/ReceiptsDrawer.test.tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ReceiptsDrawer } from "./ReceiptsDrawer";
import type { OnChainReceipts } from "@/features/marketplace/data/types";
import { LISTING_DETAILS } from "@/features/marketplace/data/fixtures/listings";

const ON_CHAIN: OnChainReceipts = LISTING_DETAILS["btc-momentum-v3"].onChain;

describe("ReceiptsDrawer", () => {
  it("always renders the toggle row", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.getByTestId("receipts-toggle")).toBeInTheDocument();
  });

  it("shows 'View on-chain receipts' when closed", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.getByText(/view on-chain receipts/i)).toBeInTheDocument();
  });

  it("shows 'Hide on-chain receipts' when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getByText(/hide on-chain receipts/i)).toBeInTheDocument();
  });

  it("does NOT render the body when closed", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.queryByTestId("receipts-body")).not.toBeInTheDocument();
  });

  it("renders the body when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getByTestId("receipts-body")).toBeInTheDocument();
  });

  it("calls onToggle when the toggle row is clicked", async () => {
    const onToggle = vi.fn();
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={onToggle} />);
    screen.getByTestId("receipts-toggle").click();
    expect(onToggle).toHaveBeenCalledOnce();
  });

  it("shows NFT token id in the manifest card when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getByText("#0043")).toBeInTheDocument();
  });

  it("shows attestation verdicts when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    // fixture has endorse + question verdicts
    expect(screen.getAllByText(/endorse/i).length).toBeGreaterThanOrEqual(1);
    expect(screen.getByText(/question/i)).toBeInTheDocument();
  });

  it("shows anchor history entries when open", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={true} onToggle={vi.fn()} />);
    expect(screen.getAllByText(/merkle/i).length).toBeGreaterThanOrEqual(1);
    // "mint" appears as the anchor kind label; also appears in the label text "Identity NFT minted"
    expect(screen.getAllByText(/mint/i).length).toBeGreaterThanOrEqual(1);
  });

  it("renders the AUDITOR shield label on the toggle row", () => {
    render(<ReceiptsDrawer onChain={ON_CHAIN} open={false} onToggle={vi.fn()} />);
    expect(screen.getByText(/auditor/i)).toBeInTheDocument();
  });
});
