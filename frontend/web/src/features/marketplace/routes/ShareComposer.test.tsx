// src/features/marketplace/routes/ShareComposer.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { RECEIPTS } from "@/features/marketplace/data/fixtures/receipts";
import { ShareComposer } from "./ShareComposer";

const share = RECEIPTS["0xdemo-tx"].share;

function wrap(ui: React.ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>);
}

describe("ShareComposer", () => {
  it("collapsed by default — OG preview not shown initially", () => {
    wrap(<ShareComposer share={share} />);
    // data-og-preview only appears in expanded state
    expect(document.querySelector("[data-og-preview]")).toBeNull();
  });

  it("shows 'Customize post' button in collapsed state", () => {
    wrap(<ShareComposer share={share} />);
    expect(screen.getByRole("button", { name: /Customize post/i })).toBeInTheDocument();
  });

  it("expands to show OG preview when 'Customize post' is clicked", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    const btn = screen.getByRole("button", { name: /Customize post/i });
    await user.click(btn);
    expect(document.querySelector("[data-og-preview]")).not.toBeNull();
  });

  it("shows OG CARD size hint in expanded state", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    await user.click(screen.getByRole("button", { name: /Customize post/i }));
    expect(screen.getByText(/OG CARD · 1200 × 630/i)).toBeInTheDocument();
  });

  it("shows initial caption in expanded state", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    await user.click(screen.getByRole("button", { name: /Customize post/i }));
    expect(screen.getByDisplayValue(share.caption)).toBeInTheDocument();
  });

  it("renders all suggested variant texts in expanded state", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    await user.click(screen.getByRole("button", { name: /Customize post/i }));
    for (const v of share.variants) {
      const escaped = v.slice(0, 20).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      expect(screen.getByText(new RegExp(escaped, "i"))).toBeInTheDocument();
    }
  });

  it("clicking a variant updates the caption textarea in expanded state", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    await user.click(screen.getByRole("button", { name: /Customize post/i }));
    const firstVariant = share.variants[0];
    const escaped = firstVariant.slice(0, 20).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    await user.click(screen.getByText(new RegExp(escaped, "i")));
    expect(screen.getByDisplayValue(firstVariant)).toBeInTheDocument();
  });

  it("renders Post to X primary CTA button (always visible)", () => {
    wrap(<ShareComposer share={share} />);
    expect(screen.getByRole("link", { name: /Post to X/i })).toBeInTheDocument();
  });

  it("renders Farcaster and Copy link buttons (always visible)", () => {
    wrap(<ShareComposer share={share} />);
    expect(screen.getByRole("link", { name: /Farcaster/i })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Copy link/i })).toBeInTheDocument();
  });

  it("does NOT render a Discord link", () => {
    wrap(<ShareComposer share={share} />);
    expect(screen.queryByText(/Discord/i)).toBeNull();
  });

  it("X / Twitter primary CTA opens a new tab with twitter.com/intent/tweet href", () => {
    wrap(<ShareComposer share={share} />);
    const xBtn = screen.getByRole("link", { name: /Post to X/i });
    expect(xBtn.getAttribute("href")).toMatch(/twitter\.com\/intent\/tweet/);
    expect(xBtn.getAttribute("target")).toBe("_blank");
  });

  it("Farcaster button opens warpcast.com/~/compose", () => {
    wrap(<ShareComposer share={share} />);
    const fc = screen.getByRole("link", { name: /Farcaster/i });
    expect(fc.getAttribute("href")).toMatch(/warpcast\.com/);
  });

  it("renders the chain-native notification hint chip from fixture", () => {
    wrap(<ShareComposer share={share} />);
    // notificationHint = "@ed's XVN just got a +$46.55 notification"
    expect(screen.getByText(new RegExp("46\\.55"))).toBeInTheDocument();
  });

  it("renders the buyer stamp overlay in the mini preview (expanded)", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    await user.click(screen.getByRole("button", { name: /Customize post/i }));
    // buyerStamp = "just bought by 0x7c…aa07"
    expect(screen.getByText(/just bought by/i)).toBeInTheDocument();
  });

  it("collapses back when 'Collapse' is clicked", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    // Expand
    await user.click(screen.getByRole("button", { name: /Customize post/i }));
    expect(document.querySelector("[data-og-preview]")).not.toBeNull();
    // Collapse
    await user.click(screen.getByRole("button", { name: /Collapse/i }));
    expect(document.querySelector("[data-og-preview]")).toBeNull();
  });
});
