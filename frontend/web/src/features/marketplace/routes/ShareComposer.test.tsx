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
  it("renders the mini OG card preview (ShareableCard aspect box present)", () => {
    wrap(<ShareComposer share={share} />);
    // The preview wrapper carries data-og-preview for test targeting
    expect(document.querySelector("[data-og-preview]")).not.toBeNull();
  });

  it("renders the OG CARD size hint", () => {
    wrap(<ShareComposer share={share} />);
    expect(screen.getByText(/OG CARD · 1200 × 630/i)).toBeInTheDocument();
  });

  it("renders the initial caption from fixture", () => {
    wrap(<ShareComposer share={share} />);
    expect(screen.getByDisplayValue(share.caption)).toBeInTheDocument();
  });

  it("renders all suggested variant texts", () => {
    wrap(<ShareComposer share={share} />);
    for (const v of share.variants) {
      // escape regex metacharacters so literal strings like "+47%" match correctly
      const escaped = v.slice(0, 20).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
      expect(screen.getByText(new RegExp(escaped, "i"))).toBeInTheDocument();
    }
  });

  it("clicking a variant updates the caption textarea", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    const firstVariant = share.variants[0];
    const escaped = firstVariant.slice(0, 20).replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    await user.click(screen.getByText(new RegExp(escaped, "i")));
    expect(screen.getByDisplayValue(firstVariant)).toBeInTheDocument();
  });

  it("renders the four post-to targets as buttons/links", () => {
    wrap(<ShareComposer share={share} />);
    expect(screen.getByText(/X \/ Twitter/i)).toBeInTheDocument();
    expect(screen.getByText(/Farcaster/i)).toBeInTheDocument();
    expect(screen.getByText(/Discord/i)).toBeInTheDocument();
    expect(screen.getByText(/Copy link/i)).toBeInTheDocument();
  });

  it("X / Twitter post button opens a new tab with twitter.com/intent/tweet href", () => {
    wrap(<ShareComposer share={share} />);
    const xBtn = screen.getByRole("link", { name: /X \/ Twitter/i });
    expect(xBtn.getAttribute("href")).toMatch(/twitter\.com\/intent\/tweet/);
    expect(xBtn.getAttribute("target")).toBe("_blank");
  });

  it("Farcaster button opens warpcast.com/~/compose", () => {
    wrap(<ShareComposer share={share} />);
    const fc = screen.getByRole("link", { name: /Farcaster/i });
    expect(fc.getAttribute("href")).toMatch(/warpcast\.com/);
  });

  it("renders the chain-native notification hint from fixture", () => {
    wrap(<ShareComposer share={share} />);
    // notificationHint = "@ed's XVN just got a +$46.55 notification"
    expect(screen.getByText(new RegExp("46\\.55"))).toBeInTheDocument();
  });

  it("renders the buyer stamp overlay on the mini preview", () => {
    wrap(<ShareComposer share={share} />);
    // buyerStamp = "just bought by 0x7c…aa07"
    expect(screen.getByText(/just bought by/i)).toBeInTheDocument();
  });
});
