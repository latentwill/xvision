# Marketplace F5 — Seller Onboarding (`/marketplace/sell`) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `/marketplace/sell` inline 3-step seller onboarding flow — pick a listable strategy, configure tier/price/payers with typed listability feedback, preview and mint — as a proper page route replacing the F0 stub, with no modals or popups.

**Architecture:** Three colocated step components inside `SellRoute.tsx` render as an inline stepper/accordion on the page. State is local to `SellRoute` (`useState`). Steps call `useMarketplaceData()` for `listListableStrategies`, `createPublishDraft`, and `submitListing`. On mint, the route navigates to `/marketplace/receipts/:tx`. A `usePublishDraft` hook encapsulates the async draft fetch triggered by strategy selection. The `routes.tsx` change is a single-line lazy-import swap from `MarketplaceSellStub` to `SellRoute`.

**Tech Stack:** React 18, TypeScript, React-Router v6 `useNavigate`, Vitest 2 + React Testing Library + jsdom, Tailwind (token classes only), pnpm. Renders under `MarketplaceDataProvider` with `FixtureMarketplaceData`.

**Source spec:** `docs/superpowers/specs/2026-05-26-marketplace-phase-f-frontend-design.md` §4 F5 + `docs/design/design_handoff_marketplace_shift/README.md` (seller onboarding direction).

**Conventions:**
- Run tests from `frontend/web`: `pnpm exec vitest run <path>`. Typecheck: `pnpm typecheck`.
- Path alias `@/` → `src/`. Colocate tests as `*.test.tsx`. Token classes only — no hex, no inline color.
- **No popups.** Steps are inline (`<section>` elements stacked vertically), not `Dialog`/`Modal`/`Sheet`/`Popover`.
- `[Testnet]` on the Mint action button (the only chain-bound CTA on this surface).
- Plan to the seam: all data comes from `useMarketplaceData()`. No direct fixture imports in production code.
- Fixture `local-wip-draft` exercises a failing listability check (no assets configured). `local-btc-momentum` is the happy path.
- `PublishDraft.priceUsdc` defaults to `49`; setting `tier = "open"` should set `priceUsdc` to `null` and hide the price field. The `preview` field on `PublishDraft` is a `ListingRow` — render it using `ListingPreviewCard`.
- Disable Mint until all `listable` checks with `ok: false` are resolved (currently impossible on fixtures, so the wip-draft fixture keeps Mint disabled permanently — document this as a known fixture constraint, not a bug).

---

## File map

```
src/features/marketplace/routes/
  SellRoute.tsx                        # page shell + inline stepper (Task 1)
  SellRoute.test.tsx                   # RTL tests for the full flow (Task 2)
  sell/
    Step1PickStrategy.tsx              # step 1: strategy picker (Task 3)
    Step1PickStrategy.test.tsx         # (Task 4)
    Step2Configure.tsx                 # step 2: tier/price/payers + listability (Task 5)
    Step2Configure.test.tsx            # (Task 6)
    Step3Preview.tsx                   # step 3: listing preview card + mint (Task 7)
    Step3Preview.test.tsx              # (Task 8)
    ListingPreviewCard.tsx             # shared ListingRow display component (Task 7)
src/routes.tsx                         # single-line stub → SellRoute swap (Task 9)
```

`src/features/marketplace/components/` — no new files; reuse `GenArtPlaceholder`, `AssetPill`, `VerifiedBadge`, `X402Badge`.

---

## Task 1: `SellRoute.tsx` — page shell + inline stepper

**Files:**
- Create: `src/features/marketplace/routes/SellRoute.tsx`
- Create (directory stubs only, content in later tasks): `src/features/marketplace/routes/sell/Step1PickStrategy.tsx`, `src/features/marketplace/routes/sell/Step2Configure.tsx`, `src/features/marketplace/routes/sell/Step3Preview.tsx`, `src/features/marketplace/routes/sell/ListingPreviewCard.tsx`

- [ ] **Step 1: Create the sell/ subdirectory stub files**

Create four minimal stub files that export their named components. These will be filled in by later tasks — they exist here so `SellRoute.tsx` can import them without compilation errors.

`src/features/marketplace/routes/sell/Step1PickStrategy.tsx`:

```tsx
// src/features/marketplace/routes/sell/Step1PickStrategy.tsx
import type { ListableStrategy } from "@/features/marketplace/data/types";

export function Step1PickStrategy({
  onSelect,
}: {
  onSelect: (strategy: ListableStrategy) => void;
}) {
  void onSelect;
  return <div data-step="1">Step 1 — stub</div>;
}
```

`src/features/marketplace/routes/sell/Step2Configure.tsx`:

```tsx
// src/features/marketplace/routes/sell/Step2Configure.tsx
import type { PublishDraft, Tier } from "@/features/marketplace/data/types";

export function Step2Configure({
  draft,
  onUpdate,
  onNext,
}: {
  draft: PublishDraft;
  onUpdate: (patch: Partial<Pick<PublishDraft, "tier" | "priceUsdc" | "acceptedPayers">>) => void;
  onNext: () => void;
}) {
  void draft; void onUpdate; void onNext;
  return <div data-step="2">Step 2 — stub</div>;
}
```

`src/features/marketplace/routes/sell/Step3Preview.tsx`:

```tsx
// src/features/marketplace/routes/sell/Step3Preview.tsx
import type { PublishDraft } from "@/features/marketplace/data/types";

export function Step3Preview({
  draft,
  onMint,
  minting,
}: {
  draft: PublishDraft;
  onMint: () => void;
  minting: boolean;
}) {
  void draft; void onMint; void minting;
  return <div data-step="3">Step 3 — stub</div>;
}
```

`src/features/marketplace/routes/sell/ListingPreviewCard.tsx`:

```tsx
// src/features/marketplace/routes/sell/ListingPreviewCard.tsx
import type { ListingRow } from "@/features/marketplace/data/types";

export function ListingPreviewCard({ listing }: { listing: ListingRow }) {
  void listing;
  return <div data-preview="listing">ListingPreviewCard — stub</div>;
}
```

- [ ] **Step 2: Write `SellRoute.tsx`**

```tsx
// src/features/marketplace/routes/SellRoute.tsx
import { useState, useCallback } from "react";
import { useNavigate } from "react-router-dom";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { ListableStrategy, PublishDraft } from "@/features/marketplace/data/types";
import { Step1PickStrategy } from "./sell/Step1PickStrategy";
import { Step2Configure } from "./sell/Step2Configure";
import { Step3Preview } from "./sell/Step3Preview";

type Step = 1 | 2 | 3;

export function SellRoute() {
  const mp = useMarketplaceData();
  const navigate = useNavigate();

  const [step, setStep] = useState<Step>(1);
  const [draft, setDraft] = useState<PublishDraft | null>(null);
  const [loadingDraft, setLoadingDraft] = useState(false);
  const [minting, setMinting] = useState(false);

  const handleStrategySelect = useCallback(
    async (strategy: ListableStrategy) => {
      setLoadingDraft(true);
      try {
        const d = await mp.createPublishDraft(strategy.id);
        setDraft(d);
        setStep(2);
      } finally {
        setLoadingDraft(false);
      }
    },
    [mp],
  );

  const handleDraftUpdate = useCallback(
    (patch: Partial<Pick<PublishDraft, "tier" | "priceUsdc" | "acceptedPayers">>) => {
      setDraft((prev) => (prev ? { ...prev, ...patch } : prev));
    },
    [],
  );

  const handleMint = useCallback(async () => {
    if (!draft) return;
    setMinting(true);
    try {
      const tx = await mp.submitListing(draft);
      navigate(`/marketplace/receipts/${tx.txHash}`);
    } finally {
      setMinting(false);
    }
  }, [draft, mp, navigate]);

  return (
    <div className="px-7 py-8 max-w-2xl" data-page="sell">
      <h1 className="text-[22px] font-semibold tracking-tight mb-1">Share your strategy</h1>
      <p className="text-[13px] text-text-2 mb-8">
        List a strategy from your XVN to the marketplace. Three steps.
      </p>

      <section
        data-sell-step="1"
        className={`mb-4 rounded-md border border-border ${step === 1 ? "border-gold/40" : ""}`}
      >
        <div className="px-5 py-3 flex items-center gap-3">
          <StepIndicator n={1} active={step === 1} done={step > 1} />
          <span className="text-[13px] font-medium">Pick a strategy</span>
          {step > 1 && draft && (
            <button
              className="ml-auto text-[11px] text-text-3 hover:text-text-2"
              onClick={() => {
                setStep(1);
                setDraft(null);
              }}
            >
              Change
            </button>
          )}
        </div>
        {step === 1 && (
          <div className="px-5 pb-5">
            {loadingDraft ? (
              <p className="text-[13px] text-text-3">Loading draft…</p>
            ) : (
              <Step1PickStrategy onSelect={handleStrategySelect} />
            )}
          </div>
        )}
      </section>

      <section
        data-sell-step="2"
        className={`mb-4 rounded-md border border-border ${step === 2 ? "border-gold/40" : ""}`}
      >
        <div className="px-5 py-3 flex items-center gap-3">
          <StepIndicator n={2} active={step === 2} done={step > 2} />
          <span className={`text-[13px] font-medium ${step < 2 ? "text-text-3" : ""}`}>
            Configure listing
          </span>
          {step > 2 && draft && (
            <button
              className="ml-auto text-[11px] text-text-3 hover:text-text-2"
              onClick={() => setStep(2)}
            >
              Change
            </button>
          )}
        </div>
        {step === 2 && draft && (
          <div className="px-5 pb-5">
            <Step2Configure
              draft={draft}
              onUpdate={handleDraftUpdate}
              onNext={() => setStep(3)}
            />
          </div>
        )}
      </section>

      <section
        data-sell-step="3"
        className={`rounded-md border border-border ${step === 3 ? "border-gold/40" : ""}`}
      >
        <div className="px-5 py-3 flex items-center gap-3">
          <StepIndicator n={3} active={step === 3} done={false} />
          <span className={`text-[13px] font-medium ${step < 3 ? "text-text-3" : ""}`}>
            Preview &amp; mint
          </span>
        </div>
        {step === 3 && draft && (
          <div className="px-5 pb-5">
            <Step3Preview draft={draft} onMint={handleMint} minting={minting} />
          </div>
        )}
      </section>
    </div>
  );
}

function StepIndicator({
  n,
  active,
  done,
}: {
  n: number;
  active: boolean;
  done: boolean;
}) {
  if (done) {
    return (
      <span className="w-5 h-5 rounded-full bg-gold/20 border border-gold/40 flex items-center justify-center text-gold text-[10px]">
        ✓
      </span>
    );
  }
  return (
    <span
      className={`w-5 h-5 rounded-full border flex items-center justify-center text-[10px] font-mono ${
        active ? "border-gold/60 text-gold" : "border-border text-text-3"
      }`}
    >
      {n}
    </span>
  );
}
```

- [ ] **Step 3: Verify it typechecks**

Run (from `frontend/web`): `pnpm typecheck`
Expected: PASS — no errors on the new files. (Stub components satisfy their prop interfaces because the imports resolve.)

- [ ] **Step 4: Commit**

```bash
git add src/features/marketplace/routes/SellRoute.tsx src/features/marketplace/routes/sell/Step1PickStrategy.tsx src/features/marketplace/routes/sell/Step2Configure.tsx src/features/marketplace/routes/sell/Step3Preview.tsx src/features/marketplace/routes/sell/ListingPreviewCard.tsx
git commit -m "feat(marketplace): F5 SellRoute shell + step stubs"
```

---

## Task 2: `SellRoute.test.tsx` — integration smoke tests for the full flow

These tests render `SellRoute` under a real `MarketplaceDataProvider` (fixture client) + `MemoryRouter`, walking through all three steps.

**Files:**
- Create: `src/features/marketplace/routes/SellRoute.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/SellRoute.test.tsx
import { render, screen, act, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { SellRoute } from "./SellRoute";

function renderSell() {
  const client = new FixtureMarketplaceData();
  render(
    <MemoryRouter initialEntries={["/marketplace/sell"]}>
      <MarketplaceDataProvider client={client}>
        <SellRoute />
      </MarketplaceDataProvider>
    </MemoryRouter>,
  );
  return { client };
}

describe("SellRoute", () => {
  it("renders the page heading and step 1 active", async () => {
    renderSell();
    expect(await screen.findByText(/Share your strategy/)).toBeInTheDocument();
    expect(screen.getByTestId("sell-step-1-body")).toBeInTheDocument();
    expect(screen.queryByTestId("sell-step-2-body")).not.toBeInTheDocument();
  });

  it("step 1: lists all 3 fixture strategies", async () => {
    renderSell();
    // strategy names from LISTABLE_STRATEGIES
    expect(await screen.findByText("btc-momentum")).toBeInTheDocument();
    expect(screen.getByText("eth-mr")).toBeInTheDocument();
    expect(screen.getByText("wip-draft")).toBeInTheDocument();
  });

  it("selecting a strategy calls createPublishDraft and advances to step 2", async () => {
    renderSell();
    const btn = await screen.findByRole("button", { name: /btc-momentum/ });
    await userEvent.click(btn);
    expect(await screen.findByTestId("sell-step-2-body")).toBeInTheDocument();
    expect(screen.queryByTestId("sell-step-1-body")).not.toBeInTheDocument();
  });

  it("step 2: shows listability checks — btc-momentum all pass", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    // all three checks should appear and pass
    expect(await screen.findByText(/Strategy exists in your XVN/)).toBeInTheDocument();
    expect(screen.getByText(/Declares an asset universe/)).toBeInTheDocument();
    expect(screen.getByText(/Has a backtest on record/)).toBeInTheDocument();
    // no failure reasons visible
    expect(screen.queryByText(/No assets configured/)).not.toBeInTheDocument();
  });

  it("step 2: shows specific failure reason for wip-draft", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /wip-draft/ }));
    expect(await screen.findByText(/No assets configured/)).toBeInTheDocument();
  });

  it("step 2: tier A hides price input; tier B shows price input", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    // default tier is sealed (B), price shown
    await screen.findByTestId("sell-step-2-body");
    expect(screen.getByTestId("price-input")).toBeInTheDocument();
    // switch to open (A)
    await userEvent.click(screen.getByTestId("tier-open-btn"));
    expect(screen.queryByTestId("price-input")).not.toBeInTheDocument();
  });

  it("step 2: clicking Continue advances to step 3", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    expect(await screen.findByTestId("sell-step-3-body")).toBeInTheDocument();
  });

  it("step 3: shows the listing preview card", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    // ListingPreviewCard renders preview.id
    expect(await screen.findByText("btc-momentum")).toBeInTheDocument();
  });

  it("step 3: Mint button is disabled when any listability check fails (wip-draft)", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /wip-draft/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    expect(screen.getByRole("button", { name: /Mint/ })).toBeDisabled();
  });

  it("step 3: Mint button is enabled for btc-momentum (all checks pass)", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    expect(screen.getByRole("button", { name: /Mint/ })).not.toBeDisabled();
  });

  it("step 3: Mint button shows [Testnet] label", async () => {
    renderSell();
    await userEvent.click(await screen.findByRole("button", { name: /btc-momentum/ }));
    await screen.findByTestId("sell-step-2-body");
    await userEvent.click(screen.getByRole("button", { name: /Continue/ }));
    await screen.findByTestId("sell-step-3-body");
    expect(screen.getByRole("button", { name: /Mint/ }).textContent).toMatch(/\[Testnet\]/);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/routes/SellRoute.test.tsx`
Expected: FAIL — tests for step-1 list content, step bodies, and testids will fail because the step components are stubs.

- [ ] **Step 3: Commit the test (red state)**

```bash
git add src/features/marketplace/routes/SellRoute.test.tsx
git commit -m "test(marketplace): F5 SellRoute integration tests (red)"
```

---

## Task 3: `Step1PickStrategy.tsx` — strategy picker

**Files:**
- Modify: `src/features/marketplace/routes/sell/Step1PickStrategy.tsx`

- [ ] **Step 1: Implement `Step1PickStrategy`**

Replace the stub with the real implementation:

```tsx
// src/features/marketplace/routes/sell/Step1PickStrategy.tsx
import { useEffect, useState } from "react";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import type { ListableStrategy } from "@/features/marketplace/data/types";

export function Step1PickStrategy({
  onSelect,
}: {
  onSelect: (strategy: ListableStrategy) => void;
}) {
  const mp = useMarketplaceData();
  const [strategies, setStrategies] = useState<ListableStrategy[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    mp.listListableStrategies().then((s) => {
      setStrategies(s);
      setLoading(false);
    });
  }, [mp]);

  if (loading) {
    return <p className="text-[13px] text-text-3">Loading strategies…</p>;
  }

  if (strategies.length === 0) {
    return (
      <p className="text-[13px] text-text-2">
        No listable strategies found. Run at least one backtest first.
      </p>
    );
  }

  return (
    <ul data-testid="sell-step-1-body" className="flex flex-col gap-2">
      {strategies.map((s) => (
        <li key={s.id}>
          <button
            onClick={() => onSelect(s)}
            aria-label={s.name}
            className="w-full flex items-center gap-3 px-4 py-3 rounded-md bg-surface-elev border border-border hover:border-gold/40 text-left transition-colors"
          >
            <div className="flex-1 min-w-0">
              <p className="text-[13px] font-medium truncate">{s.name}</p>
              <p className="text-[11px] text-text-3 font-mono">{s.version}</p>
            </div>
            <div className="flex gap-1 flex-wrap justify-end">
              {s.assets.length > 0 ? (
                s.assets.map((a) => <AssetPill key={a} asset={a} />)
              ) : (
                <span className="text-[11px] text-text-3 italic">no assets</span>
              )}
            </div>
            <svg
              width="14"
              height="14"
              viewBox="0 0 14 14"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              className="text-text-3 shrink-0"
              aria-hidden="true"
            >
              <path d="M5 3l4 4-4 4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        </li>
      ))}
    </ul>
  );
}
```

- [ ] **Step 2: Run the integration tests — strategy listing tests should now pass**

Run: `pnpm exec vitest run src/features/marketplace/routes/SellRoute.test.tsx`
Expected: "renders the page heading", "lists all 3 fixture strategies" PASS; "selecting a strategy" may still fail (step 2 body not rendered).

---

## Task 4: `Step1PickStrategy.test.tsx` — unit tests for the strategy picker

**Files:**
- Create: `src/features/marketplace/routes/sell/Step1PickStrategy.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/sell/Step1PickStrategy.test.tsx
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { Step1PickStrategy } from "./Step1PickStrategy";

function renderStep(onSelect = vi.fn()) {
  render(
    <MemoryRouter>
      <MarketplaceDataProvider client={new FixtureMarketplaceData()}>
        <Step1PickStrategy onSelect={onSelect} />
      </MarketplaceDataProvider>
    </MemoryRouter>,
  );
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/routes/sell/Step1PickStrategy.test.tsx`
Expected: FAIL — "shows all fixture strategies after load" fails because the stub renders "Step 1 — stub".

- [ ] **Step 3: Verify it passes (implementation already done in Task 3)**

Run: `pnpm exec vitest run src/features/marketplace/routes/sell/Step1PickStrategy.test.tsx`
Expected: PASS (5 tests).

- [ ] **Step 4: Commit**

```bash
git add src/features/marketplace/routes/sell/Step1PickStrategy.tsx src/features/marketplace/routes/sell/Step1PickStrategy.test.tsx
git commit -m "feat(marketplace): F5 Step1PickStrategy — strategy list with asset pills"
```

---

## Task 5: `Step2Configure.tsx` — tier/price/payers + listability checks

**Files:**
- Modify: `src/features/marketplace/routes/sell/Step2Configure.tsx`

- [ ] **Step 1: Implement `Step2Configure`**

Replace the stub:

```tsx
// src/features/marketplace/routes/sell/Step2Configure.tsx
import type { PublishDraft, Tier } from "@/features/marketplace/data/types";

export function Step2Configure({
  draft,
  onUpdate,
  onNext,
}: {
  draft: PublishDraft;
  onUpdate: (patch: Partial<Pick<PublishDraft, "tier" | "priceUsdc" | "acceptedPayers">>) => void;
  onNext: () => void;
}) {
  const allPass = draft.listable.every((c) => c.ok);

  function setTier(t: Tier) {
    onUpdate({ tier: t, priceUsdc: t === "open" ? null : (draft.priceUsdc ?? 49) });
  }

  return (
    <div data-testid="sell-step-2-body" className="flex flex-col gap-6">

      {/* Listability checks */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Listability checks
        </p>
        <ul className="flex flex-col gap-1">
          {draft.listable.map((check, i) => (
            <li key={i} className="flex items-start gap-2 text-[13px]">
              {check.ok ? (
                <span className="text-gold mt-0.5" aria-label="pass">✓</span>
              ) : (
                <span className="text-danger mt-0.5" aria-label="fail">✗</span>
              )}
              <span className={check.ok ? "text-text-2" : "text-text"}>
                {check.label}
                {!check.ok && check.reason && (
                  <span className="ml-1 text-danger text-[11px]">— {check.reason}</span>
                )}
              </span>
            </li>
          ))}
        </ul>
      </div>

      {/* Tier selector */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">Tier</p>
        <div className="flex gap-2">
          <button
            data-testid="tier-open-btn"
            onClick={() => setTier("open")}
            className={`px-3 py-2 rounded-md border text-[13px] ${
              draft.tier === "open"
                ? "border-gold/60 bg-gold/10 text-gold"
                : "border-border text-text-2 hover:border-border-strong"
            }`}
          >
            <span className="font-medium">Tier A</span>
            <span className="ml-1 text-text-3">· open / free</span>
          </button>
          <button
            data-testid="tier-sealed-btn"
            onClick={() => setTier("sealed")}
            className={`px-3 py-2 rounded-md border text-[13px] ${
              draft.tier === "sealed"
                ? "border-gold/60 bg-gold/10 text-gold"
                : "border-border text-text-2 hover:border-border-strong"
            }`}
          >
            <span className="font-medium">Tier B</span>
            <span className="ml-1 text-text-3">· sealed / paid</span>
          </button>
        </div>
      </div>

      {/* Price (Tier B only) */}
      {draft.tier === "sealed" && (
        <div>
          <label
            htmlFor="price-input"
            className="block text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2"
          >
            Price (USDC)
          </label>
          <input
            id="price-input"
            data-testid="price-input"
            type="number"
            min={1}
            step={1}
            value={draft.priceUsdc ?? 49}
            onChange={(e) => onUpdate({ priceUsdc: Math.max(1, Number(e.target.value)) })}
            className="w-28 px-3 py-2 bg-surface-elev border border-border rounded-md text-[13px] font-mono text-text focus:border-gold/60 focus:outline-none"
          />
        </div>
      )}

      {/* Accepted payers */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Accepted payers
        </p>
        <div className="flex gap-4">
          <label className="flex items-center gap-2 text-[13px] cursor-pointer">
            <input
              type="checkbox"
              data-testid="payer-humans"
              checked={draft.acceptedPayers.humans}
              onChange={(e) =>
                onUpdate({ acceptedPayers: { ...draft.acceptedPayers, humans: e.target.checked } })
              }
              className="accent-gold"
            />
            Humans
          </label>
          <label className="flex items-center gap-2 text-[13px] cursor-pointer">
            <input
              type="checkbox"
              data-testid="payer-agents"
              checked={draft.acceptedPayers.agents}
              onChange={(e) =>
                onUpdate({ acceptedPayers: { ...draft.acceptedPayers, agents: e.target.checked } })
              }
              className="accent-gold"
            />
            Agents (x402)
          </label>
        </div>
      </div>

      {/* Continue */}
      <div className="flex items-center gap-4">
        <button
          onClick={onNext}
          disabled={!allPass}
          className={`px-4 py-2 rounded-md text-[13px] font-medium ${
            allPass
              ? "bg-gold text-black hover:bg-gold/90"
              : "bg-surface-elev border border-border text-text-3 cursor-not-allowed"
          }`}
        >
          Continue
        </button>
        {!allPass && (
          <p className="text-[12px] text-danger">
            Resolve listability failures before continuing.
          </p>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Run the integration tests — step 2 tests should now mostly pass**

Run: `pnpm exec vitest run src/features/marketplace/routes/SellRoute.test.tsx`
Expected: All tests up through "clicking Continue advances to step 3" PASS. Step 3 tests may still fail (stub).

---

## Task 6: `Step2Configure.test.tsx` — unit tests for configuration step

**Files:**
- Create: `src/features/marketplace/routes/sell/Step2Configure.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/sell/Step2Configure.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Step2Configure } from "./Step2Configure";
import type { PublishDraft } from "@/features/marketplace/data/types";

const happyDraft: PublishDraft = {
  strategyId: "local-btc-momentum",
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
    priceUsdc: 49, tier: "sealed", verification: "unverified", acceptsX402: true, clones: 0,
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

  it("changing price input calls onUpdate with the new value", async () => {
    const onUpdate = vi.fn();
    render(<Step2Configure draft={happyDraft} onUpdate={onUpdate} onNext={vi.fn()} />);
    const input = screen.getByTestId("price-input") as HTMLInputElement;
    await userEvent.clear(input);
    await userEvent.type(input, "99");
    // onUpdate called with priceUsdc: 99 at some point
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
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/routes/sell/Step2Configure.test.tsx`
Expected: FAIL — stub renders "Step 2 — stub", not the real form.

- [ ] **Step 3: Verify it passes (implementation done in Task 5)**

Run: `pnpm exec vitest run src/features/marketplace/routes/sell/Step2Configure.test.tsx`
Expected: PASS (9 tests).

- [ ] **Step 4: Commit**

```bash
git add src/features/marketplace/routes/sell/Step2Configure.tsx src/features/marketplace/routes/sell/Step2Configure.test.tsx
git commit -m "feat(marketplace): F5 Step2Configure — tier/price/payers with listability checks"
```

---

## Task 7: `ListingPreviewCard.tsx` + `Step3Preview.tsx`

**Files:**
- Modify: `src/features/marketplace/routes/sell/ListingPreviewCard.tsx`
- Modify: `src/features/marketplace/routes/sell/Step3Preview.tsx`

- [ ] **Step 1: Implement `ListingPreviewCard`**

Replace the stub:

```tsx
// src/features/marketplace/routes/sell/ListingPreviewCard.tsx
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import { VerifiedBadge } from "@/features/marketplace/components/VerifiedBadge";
import { X402Badge } from "@/features/marketplace/components/X402Badge";
import type { ListingRow } from "@/features/marketplace/data/types";

export function ListingPreviewCard({ listing }: { listing: ListingRow }) {
  const priceLabel =
    listing.priceUsdc === null
      ? "OPEN"
      : `${listing.priceUsdc} USDC`;

  return (
    <div
      data-preview="listing"
      className="flex gap-4 p-4 rounded-md bg-surface-elev border border-border"
    >
      <GenArtPlaceholder seed={listing.genArtSeed} size={56} className="shrink-0 rounded-md" />

      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 flex-wrap mb-0.5">
          <span className="text-[14px] font-medium font-mono truncate">{listing.id}</span>
          <span className="text-[11px] text-text-3 font-mono">{listing.version}</span>
          {listing.verification === "verified" && <VerifiedBadge />}
          {listing.acceptsX402 && <X402Badge />}
        </div>

        <p className="text-[12px] text-text-3 mb-1.5">
          {listing.creator.handle ?? listing.creator.address} · {listing.model} · {listing.style}
        </p>

        <div className="flex items-center gap-2 flex-wrap">
          {listing.assets.map((a) => (
            <AssetPill key={a} asset={a} />
          ))}
          {listing.assets.length === 0 && (
            <span className="text-[11px] text-danger italic">No assets configured</span>
          )}
        </div>
      </div>

      <div className="shrink-0 text-right">
        {listing.priceUsdc === null ? (
          <span className="text-[12px] font-medium text-gold">● OPEN</span>
        ) : (
          <span className="text-[13px] font-mono font-medium text-text">
            {listing.priceUsdc} <span className="text-text-3">USDC</span>
          </span>
        )}
        <p className="text-[11px] text-text-3 mt-0.5">
          {listing.tier === "open" ? "Tier A" : "Tier B"}
        </p>
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Implement `Step3Preview`**

Replace the stub:

```tsx
// src/features/marketplace/routes/sell/Step3Preview.tsx
import type { PublishDraft } from "@/features/marketplace/data/types";
import { ListingPreviewCard } from "./ListingPreviewCard";

export function Step3Preview({
  draft,
  onMint,
  minting,
}: {
  draft: PublishDraft;
  onMint: () => void;
  minting: boolean;
}) {
  const allPass = draft.listable.every((c) => c.ok);
  const mintDisabled = !allPass || minting;

  return (
    <div data-testid="sell-step-3-body" className="flex flex-col gap-5">

      {/* Ingredients summary */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Ingredients in bundle
        </p>
        <ul className="flex flex-col gap-1">
          {draft.ingredients.map((ing, i) => (
            <li key={i} className="flex items-center gap-2 text-[12px]">
              <span
                className={`w-1.5 h-1.5 rounded-full ${ing.installed ? "bg-gold" : "bg-warn"}`}
                aria-hidden="true"
              />
              <span className={ing.installed ? "text-text-2" : "text-warn"}>{ing.name}</span>
              <span className="text-text-3 text-[10px] font-mono uppercase ml-1">{ing.kind}</span>
            </li>
          ))}
        </ul>
      </div>

      {/* Preview card */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Listing preview
        </p>
        <ListingPreviewCard listing={draft.preview} />
      </div>

      {/* Failed checks warning (when minting is blocked) */}
      {!allPass && (
        <div className="flex items-start gap-2 px-4 py-3 rounded-md bg-danger/10 border border-danger/30">
          <svg
            width="14"
            height="14"
            viewBox="0 0 14 14"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.6"
            className="text-danger mt-0.5 shrink-0"
            aria-hidden="true"
          >
            <circle cx="7" cy="7" r="5.5" />
            <path d="M7 4.5v3M7 9.5v.2" strokeLinecap="round" />
          </svg>
          <p className="text-[12px] text-danger">
            Mint is disabled — resolve listability failures in step 2 before minting.
          </p>
        </div>
      )}

      {/* Mint action */}
      <div className="flex items-center gap-4">
        <button
          onClick={onMint}
          disabled={mintDisabled}
          className={`px-4 py-2 rounded-md text-[13px] font-medium flex items-center gap-2 ${
            mintDisabled
              ? "bg-surface-elev border border-border text-text-3 cursor-not-allowed"
              : "bg-gold text-black hover:bg-gold/90"
          }`}
          aria-label="Mint [Testnet]"
        >
          {minting ? "Minting…" : "Mint [Testnet]"}
        </button>
        <p className="text-[11px] text-text-3">
          Submits listing to the Mantle Sepolia testnet · one-time fee
        </p>
      </div>
    </div>
  );
}
```

- [ ] **Step 3: Run the integration tests — all step 3 tests should now pass**

Run: `pnpm exec vitest run src/features/marketplace/routes/SellRoute.test.tsx`
Expected: PASS (all 9 tests).

---

## Task 8: `Step3Preview.test.tsx` — unit tests for preview + mint step

**Files:**
- Create: `src/features/marketplace/routes/sell/Step3Preview.test.tsx`

- [ ] **Step 1: Write the failing test**

```tsx
// src/features/marketplace/routes/sell/Step3Preview.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Step3Preview } from "./Step3Preview";
import type { PublishDraft } from "@/features/marketplace/data/types";

const happyDraft: PublishDraft = {
  strategyId: "local-btc-momentum",
  listable: [
    { ok: true, label: "Strategy exists in your XVN" },
    { ok: true, label: "Declares an asset universe" },
    { ok: true, label: "Has a backtest on record" },
  ],
  tier: "sealed",
  priceUsdc: 49,
  acceptedPayers: { humans: true, agents: true },
  ingredients: [
    { name: "Claude Haiku 4.5", kind: "model", installed: true },
    { name: "Birdeye MCP", kind: "mcp", installed: false },
  ],
  preview: {
    id: "btc-momentum", lineageId: "btc-momentum", version: "v3.0",
    creator: { address: "0xa83e", handle: "@ed" }, model: "Claude · Haiku 4.5", style: "Day",
    assets: ["BTC"], return30dPct: 47.2, sharpe: 1.31, buyers: { humans: 0, agents: 0 },
    priceUsdc: 49, tier: "sealed", verification: "unverified", acceptsX402: true, clones: 0,
    transferableLicense: false, genArtSeed: "btc-momentum-preview",
  },
};

const failingDraft: PublishDraft = {
  ...happyDraft,
  listable: [
    ...happyDraft.listable.slice(0, 1),
    { ok: false, label: "Declares an asset universe", reason: "No assets configured" },
    happyDraft.listable[2],
  ],
  preview: {
    ...happyDraft.preview,
    id: "wip-draft",
    assets: [],
  },
};

describe("Step3Preview", () => {
  it("renders the listing preview card with the strategy id", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText("btc-momentum")).toBeInTheDocument();
  });

  it("renders the gen-art placeholder inside the preview card", () => {
    const { container } = render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(container.querySelector('[data-genart="placeholder"]')).not.toBeNull();
  });

  it("lists all ingredients with their kind label", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText("Claude Haiku 4.5")).toBeInTheDocument();
    expect(screen.getByText("Birdeye MCP")).toBeInTheDocument();
    expect(screen.getAllByText(/model|mcp/i).length).toBeGreaterThanOrEqual(2);
  });

  it("Mint button is enabled for happy draft", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByRole("button", { name: /Mint/ })).not.toBeDisabled();
  });

  it("Mint button contains [Testnet] label", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByRole("button", { name: /Mint/ }).textContent).toContain("[Testnet]");
  });

  it("Mint button is disabled when any listability check fails", () => {
    render(<Step3Preview draft={failingDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByRole("button", { name: /Mint/ })).toBeDisabled();
  });

  it("shows a warning message when Mint is disabled due to check failures", () => {
    render(<Step3Preview draft={failingDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText(/Mint is disabled/)).toBeInTheDocument();
  });

  it("Mint button is disabled while minting=true", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={true} />);
    expect(screen.getByRole("button", { name: /Minting/ })).toBeDisabled();
  });

  it("clicking Mint calls onMint", async () => {
    const onMint = vi.fn();
    render(<Step3Preview draft={happyDraft} onMint={onMint} minting={false} />);
    await userEvent.click(screen.getByRole("button", { name: /Mint/ }));
    expect(onMint).toHaveBeenCalledOnce();
  });

  it("preview card shows asset pill for BTC", () => {
    render(<Step3Preview draft={happyDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText("BTC")).toBeInTheDocument();
  });

  it("preview card shows 'No assets configured' for empty assets", () => {
    render(<Step3Preview draft={failingDraft} onMint={vi.fn()} minting={false} />);
    expect(screen.getByText(/No assets configured/)).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/routes/sell/Step3Preview.test.tsx`
Expected: FAIL — stub renders "Step 3 — stub".

- [ ] **Step 3: Verify it passes (implementation done in Task 7)**

Run: `pnpm exec vitest run src/features/marketplace/routes/sell/Step3Preview.test.tsx`
Expected: PASS (11 tests).

- [ ] **Step 4: Commit**

```bash
git add src/features/marketplace/routes/sell/Step3Preview.tsx src/features/marketplace/routes/sell/Step3Preview.test.tsx src/features/marketplace/routes/sell/ListingPreviewCard.tsx
git commit -m "feat(marketplace): F5 Step3Preview — listing preview card + [Testnet] mint"
```

---

## Task 9: Wire `routes.tsx` + update routing smoke test

**Files:**
- Modify: `src/routes.tsx` (one line)
- Modify: `src/features/marketplace/marketplace-routes.test.tsx`

- [ ] **Step 1: Swap the stub import in `routes.tsx`**

In `src/routes.tsx`, replace the single lazy import line for `MarketplaceSellStub`:

Old line (line 63):
```ts
const MarketplaceSellStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceSellStub })));
```

New line:
```ts
const MarketplaceSellRoute = lazy(() => import("./features/marketplace/routes/SellRoute").then((m) => ({ default: m.SellRoute })));
```

Also update the route element reference (line 191 area):

Old:
```tsx
{ path: "sell", element: page(<MarketplaceSellStub />) },
```

New:
```tsx
{ path: "sell", element: page(<MarketplaceSellRoute />) },
```

- [ ] **Step 2: Add a routing smoke test for the sell route**

In `src/features/marketplace/marketplace-routes.test.tsx`, add one test alongside the existing ones:

```tsx
import { SellRoute } from "./routes/SellRoute";

// Add inside the existing describe block:
it("resolves /marketplace/sell and renders the page heading", async () => {
  const router = createMemoryRouter(
    [
      {
        path: "/marketplace",
        element: <MarketplaceLayout />,
        children: [
          { index: true, element: <MarketplaceBrowseStub /> },
          { path: "lineage/:name", element: <MarketplaceLineageStub /> },
          { path: "sell", element: <SellRoute /> },
        ],
      },
    ],
    { initialEntries: ["/marketplace/sell"] },
  );
  render(<RouterProvider router={router} />);
  expect(await screen.findByText(/Share your strategy/)).toBeInTheDocument();
});
```

- [ ] **Step 3: Run the full test suite for the marketplace feature**

Run: `pnpm exec vitest run src/features/marketplace/`
Expected: PASS — all tests including the new routing smoke test.

- [ ] **Step 4: Typecheck**

Run (from `frontend/web`): `pnpm typecheck`
Expected: PASS — no errors.

- [ ] **Step 5: Commit**

```bash
git add src/routes.tsx src/features/marketplace/marketplace-routes.test.tsx
git commit -m "feat(marketplace): F5 wire SellRoute into router, replace stub"
```

---

## Task 10: Final verification

- [ ] **Step 1: Run the complete F5 test suite**

Run: `pnpm exec vitest run src/features/marketplace/`
Expected: PASS — all tests across all F5 files and the updated marketplace-routes smoke test.

Run output should include the following test files all green:
- `src/features/marketplace/routes/SellRoute.test.tsx`
- `src/features/marketplace/routes/sell/Step1PickStrategy.test.tsx`
- `src/features/marketplace/routes/sell/Step2Configure.test.tsx`
- `src/features/marketplace/routes/sell/Step3Preview.test.tsx`
- `src/features/marketplace/marketplace-routes.test.tsx`

- [ ] **Step 2: Typecheck the full frontend**

Run (from `frontend/web`): `pnpm typecheck`
Expected: PASS.

- [ ] **Step 3: Final commit summary**

```bash
git log --oneline -8
```

Expected to see (newest first):
```
feat(marketplace): F5 wire SellRoute into router, replace stub
feat(marketplace): F5 Step3Preview — listing preview card + [Testnet] mint
feat(marketplace): F5 Step2Configure — tier/price/payers with listability checks
feat(marketplace): F5 Step1PickStrategy — strategy list with asset pills
test(marketplace): F5 SellRoute integration tests (red)
feat(marketplace): F5 SellRoute shell + step stubs
```

---

## Open Questions

1. **`SellRoute` navigates to `/marketplace/receipts/:tx` on mint — `ReceiptRoute` (F6) is currently a stub.** The navigation will succeed (the stub renders) but the receipt surface is not implemented yet. This is expected; F6 fills it in. No action needed in F5.

2. **`useNavigate` in tests is backed by `MemoryRouter`.** After clicking Mint in `SellRoute.test.tsx`, the test could assert that navigation to `/marketplace/receipts/<txHash>` occurred (via `window.location` or a mock navigate). The current tests stop short of this assertion because the `MemoryRouter` + `RouterProvider` pattern would require a more elaborate route tree. This is a known gap — add it when F6 ships and the receipt route has real content to assert on.

3. **`createPublishDraft` is fixture-backed.** The real implementation (Phase 6) reads local `/api/strategies`. The seam call (`mp.createPublishDraft(strategyId)`) is already in place; no additional plumbing is needed in F5.

4. **Mint button label** — the spec says `[Testnet]` on the Mint action. The implementation uses `"Mint [Testnet]"` as the button text. If the design direction changes this to a badge or chip next to the button text, it can be adjusted without changing the test selector since the test checks `textContent` for the substring `[Testnet]` rather than exact match.

5. **"Change" back-navigation in `SellRoute`** — clicking "Change" in step 2 resets to step 1 and clears the draft. Clicking "Change" in step 3 returns to step 2 without clearing the draft. This is the simplest correct behavior; a more sophisticated flow might preserve form edits across back-nav. Fine for F5.

6. **`Step1PickStrategy` fetches on mount (`useEffect`).** Future: wrap in `usePublishDraft` hook per the spec's hooks inventory (§4 F8). For F5 this is intentionally inline — the hook refactor is deferred to keep F5 scoped.

---

## Constraint: Mint disabled on `local-wip-draft`

The `wip-draft` fixture always has `ok: false` on the "Declares an asset universe" check because `assets: []`. This means Mint will always be disabled for that strategy — the Continue button in Step 2 is also disabled. This is **correct behavior** (the flow enforces fix-before-mint), not a bug. The fixture was designed this way to exercise the failure path. An implementer who wants to manually test the full mint flow should use `local-btc-momentum`.
