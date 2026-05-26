# Marketplace F6 — Purchase Receipt Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to execute this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement F6 — the `/marketplace/receipts/:tx` purchase receipt surface. Renders a `Receipt` from `getReceipt(tx)` into three inline panels: License NFT card, Install stepper, Share composer. Replaces `MarketplaceReceiptStub` with a real `ReceiptRoute` plus two colocated sub-components (`InstallSteps`, `ShareComposer`) and their tests.

**Architecture:** One new file per component + one test file per component, all colocated under `src/features/marketplace/routes/`. The route reads `:tx` from URL params, calls `useMarketplaceData().getReceipt(tx)` inside a `useEffect`/`useState`, and renders three inline panels. No new seam methods. No modals. External share targets open in a new tab via intent URLs.

**Source of truth (read before coding):**
- Types: `src/features/marketplace/data/types.ts` — `Receipt`, `ShareComposerData`, `ShareableCardData`, `Ingredient`
- Data seam: `src/features/marketplace/data/MarketplaceData.ts` — `getReceipt(txHash: string): Promise<Receipt>`
- Fixture: `src/features/marketplace/data/fixtures/receipts.ts` — `RECEIPTS["0xdemo-tx"]`
- Reused components: `GenArtPlaceholder`, `TxChip`, `AgentIcon`, `ShareableCard` (all in `src/features/marketplace/components/`)
- Design reference: `docs/design/design_handoff_marketplace_shift/bc2-receipt.jsx` — §5 / `PurchaseReceipt` + `Step` + `ShareableCardMini`
- Format template: `docs/superpowers/plans/2026-05-26-marketplace-phase-f0-foundation.md`
- Route wiring: `src/routes.tsx` line 64 (`MarketplaceReceiptStub`) + `src/features/marketplace/routes/stubs.tsx` line 17

**Conventions:**
- Run tests from `frontend/web`: `pnpm exec vitest run <path>`. Typecheck: `pnpm typecheck`.
- Path alias `@/` → `src/`. Colocate tests as `*.test.tsx`. Token classes only (no inline hex).
- No popups. Install steps and share composer are inline panels. Post-to targets open `window.open(..., "_blank")`, not modals.
- Tests wrap in `MarketplaceDataProvider` + `MemoryRouter` routed at `/marketplace/receipts/0xdemo-tx`.
- Execute on `feat/marketplace-f0` (or the designated F6 branch). Commit per task.

---

## File map

```
src/features/marketplace/routes/
  ReceiptRoute.tsx              # Task 1 — page shell + data fetch
  ReceiptRoute.test.tsx         # Task 1 — smoke + header assertions
  InstallSteps.tsx              # Task 2 — install stepper panel
  InstallSteps.test.tsx         # Task 2 — step state + ingredient chips
  ShareComposer.tsx             # Task 3 — share composer panel
  ShareComposer.test.tsx        # Task 3 — caption, variants, post targets
  stubs.tsx                     # unchanged (MarketplaceReceiptStub still exists)
```

`src/routes.tsx` modified once (Task 4) — single import line + single element swap.

---

## Task 1: `ReceiptRoute` — page shell, success header, 3-col grid

**Files:**
- Create: `src/features/marketplace/routes/ReceiptRoute.tsx`
- Create: `src/features/marketplace/routes/ReceiptRoute.test.tsx`

### Step 1: Write the failing test

```tsx
// src/features/marketplace/routes/ReceiptRoute.test.tsx
import { render, screen } from "@testing-library/react";
import { RouterProvider, createMemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { MarketplaceDataProvider } from "@/features/marketplace/data/provider";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import { ReceiptRoute } from "./ReceiptRoute";
import { MarketplaceLayout } from "./MarketplaceLayout";

function routerAt(path: string) {
  return createMemoryRouter(
    [
      {
        path: "/marketplace",
        element: <MarketplaceLayout />,
        children: [
          { path: "receipts/:tx", element: <ReceiptRoute /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );
}

describe("ReceiptRoute", () => {
  it("renders the success header with strategy id from fixture", async () => {
    render(<RouterProvider router={routerAt("/marketplace/receipts/0xdemo-tx")} />);
    expect(await screen.findByText(/You bought/)).toBeInTheDocument();
    expect(await screen.findByText("btc-momentum-v3")).toBeInTheDocument();
  });

  it("renders the fee breakdown line with price, token id, and net-to-creator", async () => {
    render(<RouterProvider router={routerAt("/marketplace/receipts/0xdemo-tx")} />);
    expect(await screen.findByText(/49 USDC/)).toBeInTheDocument();
    expect(await screen.findByText(/#0184/)).toBeInTheDocument();
    expect(await screen.findByText(/46\.55/)).toBeInTheDocument();
  });

  it("renders a TxChip with the receipt txHash", async () => {
    render(<RouterProvider router={routerAt("/marketplace/receipts/0xdemo-tx")} />);
    // TxChip renders the hash as a link; fixture txHash is "0xdemo-tx"
    expect(await screen.findByRole("link", { name: /0xdemo-tx/ })).toBeInTheDocument();
  });

  it("renders all three panel headings", async () => {
    render(<RouterProvider router={routerAt("/marketplace/receipts/0xdemo-tx")} />);
    expect(await screen.findByText(/License NFT/i)).toBeInTheDocument();
    expect(await screen.findByText(/Install in your XVN/i)).toBeInTheDocument();
    expect(await screen.findByText(/Share/i)).toBeInTheDocument();
  });

  it("shows loading state before receipt resolves", () => {
    render(<RouterProvider router={routerAt("/marketplace/receipts/0xdemo-tx")} />);
    // The loading placeholder must be in the document synchronously
    expect(document.body.textContent).toMatch(/Loading|receipt/i);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run (from `frontend/web`): `pnpm exec vitest run src/features/marketplace/routes/ReceiptRoute.test.tsx`
Expected: FAIL — `ReceiptRoute` module not found.

### Step 3: Implement `ReceiptRoute`

```tsx
// src/features/marketplace/routes/ReceiptRoute.tsx
// F6 — /marketplace/receipts/:tx — post-buy install + share surface.
// No modals. All panels are inline. Post targets open new tabs.
import { useEffect, useState } from "react";
import { useParams } from "react-router-dom";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { Receipt } from "@/features/marketplace/data/types";
import { TxChip } from "@/features/marketplace/components/TxChip";
import { GenArtPlaceholder } from "@/features/marketplace/components/GenArtPlaceholder";
import { InstallSteps } from "./InstallSteps";
import { ShareComposer } from "./ShareComposer";

// ── card wrapper matching F0 design tokens ──────────────────────────────────
function Panel({
  title,
  sub,
  right,
  children,
}: {
  title: string;
  sub?: string;
  right?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div className="rounded-md border border-border bg-surface-card flex flex-col overflow-hidden">
      <div className="flex items-start justify-between gap-3 px-4 py-3 border-b border-border">
        <div className="min-w-0">
          <div className="text-[13.5px] font-semibold text-text leading-tight">{title}</div>
          {sub ? (
            <div className="mt-0.5 font-mono text-[11px] text-text-3 leading-snug">{sub}</div>
          ) : null}
        </div>
        {right ? <div className="shrink-0">{right}</div> : null}
      </div>
      {children}
    </div>
  );
}

// ── license metadata stack ───────────────────────────────────────────────────
function LicenseCard({ receipt }: { receipt: Receipt }) {
  const { license, listing } = receipt;
  const rows: [string, React.ReactNode, "gold" | "mono" | "muted" | "link"][] = [
    ["strategy", <span className="text-gold">{listing.id}</span>, "gold"],
    ["version", listing.version, "mono"],
    ["creator", listing.creator.handle ?? listing.creator.address, "mono"],
    ["manifest", license.manifestHash, "mono"],
    ["bundle", `ipfs://${license.bundleCid}`, "link"],
    [
      "paid",
      `${license.pricePaidUsdc} USDC (5% fee · ${license.feeUsdc} USDC)`,
      "muted",
    ],
    ["minted", license.mintedAt, "mono"],
  ];
  return (
    <div className="p-4 flex flex-col gap-4">
      <div className="relative">
        <GenArtPlaceholder seed={listing.genArtSeed} size={290} className="w-full !rounded-[5px]" />
        {/* LICENSE #N overlay — top-left */}
        <div className="absolute top-2 left-2 px-2 py-0.5 rounded-sm bg-black/75 backdrop-blur">
          <span className="font-mono text-[10px] font-semibold tracking-[0.14em] text-text">
            LICENSE {license.tokenId}
          </span>
        </div>
        {/* OWNED · YOU overlay — bottom-right */}
        <div className="absolute bottom-2 right-2 px-2 py-0.5 rounded-sm bg-gold/10 border border-gold-soft">
          <span className="font-mono text-[9.5px] font-semibold tracking-[0.14em] text-gold">
            OWNED · YOU
          </span>
        </div>
      </div>

      <div className="flex flex-col gap-1.5">
        {rows.map(([label, value, tone]) => (
          <div key={label} className="grid gap-2" style={{ gridTemplateColumns: "72px 1fr" }}>
            <span className="font-mono text-[9px] uppercase tracking-[0.16em] text-text-3 leading-relaxed">
              {label}
            </span>
            <span
              className={[
                "font-mono text-[11px] break-all leading-relaxed",
                tone === "gold" ? "text-gold" : "",
                tone === "link" ? "text-info underline decoration-dotted underline-offset-2" : "",
                tone === "muted" ? "text-text-3" : "",
                tone === "mono" ? "text-text-2" : "",
              ]
                .filter(Boolean)
                .join(" ")}
            >
              {value}
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── page ─────────────────────────────────────────────────────────────────────
export function ReceiptRoute() {
  const { tx = "" } = useParams<{ tx: string }>();
  const mp = useMarketplaceData();
  const [receipt, setReceipt] = useState<Receipt | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    mp.getReceipt(tx).then(
      (r) => { if (!cancelled) setReceipt(r); },
      (e) => { if (!cancelled) setError(String(e)); },
    );
    return () => { cancelled = true; };
  }, [mp, tx]);

  if (error) {
    return (
      <div className="px-7 py-8 text-[13px] text-danger font-mono">
        Receipt not found: {error}
      </div>
    );
  }

  if (!receipt) {
    return (
      <div className="px-7 py-8 text-[13px] text-text-3">
        Loading receipt…
      </div>
    );
  }

  const { listing, license, network } = receipt;
  const creatorLabel = listing.creator.handle ?? listing.creator.address;

  return (
    <div className="flex flex-col min-h-0">
      {/* ── Success header strip ────────────────────────────────────────────── */}
      <div
        className="flex items-center gap-4 px-7 py-4 border-b border-border"
        style={{
          background: "linear-gradient(90deg, rgba(0,230,118,0.10), rgba(0,230,118,0.02))",
        }}
      >
        {/* 44px green check circle */}
        <div className="w-11 h-11 rounded-full flex items-center justify-center shrink-0 bg-gold/[0.18] border border-gold">
          <svg
            width="22"
            height="22"
            viewBox="0 0 22 22"
            fill="none"
            stroke="var(--gold)"
            strokeWidth="2.4"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M4 11l5 5 9-11" />
          </svg>
        </div>

        <div className="flex-1 min-w-0">
          <h1 className="text-2xl font-semibold tracking-tight leading-tight">
            You bought{" "}
            <span className="font-mono text-gold">{listing.id}</span>
          </h1>
          <div className="mt-1.5 font-mono text-[11.5px] text-text-3 flex flex-wrap items-center gap-x-2 gap-y-0.5">
            <span>
              <span className="text-gold">{license.pricePaidUsdc} USDC</span>{" "}
              paid
            </span>
            <span className="text-text-4">·</span>
            <span>
              license{" "}
              <span className="text-text-2">{license.tokenId}</span> minted
            </span>
            <span className="text-text-4">·</span>
            <span>
              {license.netToCreatorUsdc} USDC → {creatorLabel}
            </span>
            <span className="text-text-4">·</span>
            <TxChip hash={receipt.txHash} network={network} />
          </div>
        </div>

        <a
          href={`https://${network.includes("sepolia") ? "sepolia." : ""}mantlescan.xyz/tx/${receipt.txHash}`}
          target="_blank"
          rel="noreferrer"
          className="shrink-0 font-mono text-[12px] text-text-3 border border-border-strong rounded px-2.5 py-1.5 hover:text-text hover:border-border-strong/80 flex items-center gap-1.5"
        >
          View on Mantlescan
          <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
            <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        </a>
      </div>

      {/* ── Body: 3-col ─────────────────────────────────────────────────────── */}
      <div
        className="flex-1 min-h-0 overflow-auto p-4"
        style={{ display: "grid", gridTemplateColumns: "320px 1fr 380px", gap: 14 }}
      >
        {/* Col 1 — License NFT */}
        <Panel
          title="License NFT"
          sub="non-transferable · ERC-1155 on Mantle"
        >
          <LicenseCard receipt={receipt} />
        </Panel>

        {/* Col 2 — Install steps */}
        <Panel
          title="Install in your XVN"
          sub={
            receipt.install.xvnDetected
              ? `detected at ${receipt.install.xvnEndpoint} · 4 steps · sealed bundle auto-decrypts`
              : "XVN not detected · install XVN first"
          }
          right={
            <button className="font-mono text-[12px] bg-gold text-black px-3 py-1.5 rounded hover:opacity-90 font-semibold">
              Install all
            </button>
          }
        >
          <InstallSteps receipt={receipt} />
        </Panel>

        {/* Col 3 — Share composer */}
        <Panel
          title="Share"
          sub="OG card pre-loaded · post to X / Farcaster / Discord"
        >
          <ShareComposer share={receipt.share} />
        </Panel>
      </div>
    </div>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/routes/ReceiptRoute.test.tsx`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/routes/ReceiptRoute.tsx src/features/marketplace/routes/ReceiptRoute.test.tsx
git commit -m "feat(marketplace): F6 ReceiptRoute shell + success header"
```

---

## Task 2: `InstallSteps` — inline install stepper

**Files:**
- Create: `src/features/marketplace/routes/InstallSteps.tsx`
- Create: `src/features/marketplace/routes/InstallSteps.test.tsx`

### Step 1: Write the failing test

```tsx
// src/features/marketplace/routes/InstallSteps.test.tsx
import { render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it } from "vitest";
import { RECEIPTS } from "@/features/marketplace/data/fixtures/receipts";
import { InstallSteps } from "./InstallSteps";

const receipt = RECEIPTS["0xdemo-tx"];

function wrap(ui: React.ReactElement) {
  return render(<MemoryRouter>{ui}</MemoryRouter>);
}

describe("InstallSteps", () => {
  it("renders all four step titles", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.getByText(/XVN install detected/i)).toBeInTheDocument();
    expect(screen.getByText(/Decrypt sealed bundle/i)).toBeInTheDocument();
    expect(screen.getByText(/Install missing ingredients/i)).toBeInTheDocument();
    expect(screen.getByText(/Add to your Strategies/i)).toBeInTheDocument();
  });

  it("step 1 renders as done (struck-through) when xvnDetected is true", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const step1title = screen.getByText(/XVN install detected/i);
    // done steps get line-through decoration
    expect(step1title.className).toMatch(/line-through/);
  });

  it("step 3 renders ingredient chips for all ingredients", () => {
    wrap(<InstallSteps receipt={receipt} />);
    for (const ing of receipt.install.ingredients) {
      expect(screen.getByText(ing.name)).toBeInTheDocument();
    }
  });

  it("installed ingredients show a different tone to missing ones", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const installed = receipt.install.ingredients.filter((i) => i.installed);
    const missing   = receipt.install.ingredients.filter((i) => !i.installed);
    // Installed chips carry data-installed="true" for test accessibility
    expect(
      screen.getAllByTestId("ingredient-chip").filter(
        (el) => el.getAttribute("data-installed") === "true"
      )
    ).toHaveLength(installed.length);
    expect(
      screen.getAllByTestId("ingredient-chip").filter(
        (el) => el.getAttribute("data-installed") === "false"
      )
    ).toHaveLength(missing.length);
  });

  it("shows xvnEndpoint in step 1 description when detected", () => {
    wrap(<InstallSteps receipt={receipt} />);
    expect(screen.getByText(/localhost:3000/)).toBeInTheDocument();
  });

  it("shows 'not detected' message in step 1 when xvnDetected is false", () => {
    const noXvn = {
      ...receipt,
      install: { ...receipt.install, xvnDetected: false },
    };
    wrap(<InstallSteps receipt={noXvn} />);
    expect(screen.getByText(/not detected/i)).toBeInTheDocument();
  });

  it("step 3 action chip shows count of missing ingredients", () => {
    wrap(<InstallSteps receipt={receipt} />);
    const missingCount = receipt.install.ingredients.filter((i) => !i.installed).length;
    expect(screen.getByText(new RegExp(`Install missing \\(${missingCount}\\)`))).toBeInTheDocument();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/routes/InstallSteps.test.tsx`
Expected: FAIL — `InstallSteps` module not found.

### Step 3: Implement `InstallSteps`

```tsx
// src/features/marketplace/routes/InstallSteps.tsx
// Inline install stepper — no modals, no external navigation.
// All step actions are anchors/buttons that remain within the page.
import type { Receipt, Ingredient } from "@/features/marketplace/data/types";

// ── visual step states ───────────────────────────────────────────────────────
type StepState = "done" | "active" | "pending";

function StepCircle({ n, state }: { n: number; state: StepState }) {
  const isDone   = state === "done";
  const isActive = state === "active";
  return (
    <div
      className={[
        "w-[26px] h-[26px] rounded-full border-[1.5px] flex items-center justify-center shrink-0",
        isDone   ? "border-gold bg-gold" : "",
        isActive ? "border-gold bg-gold/10" : "",
        state === "pending" ? "border-border-strong bg-transparent" : "",
      ].filter(Boolean).join(" ")}
    >
      {isDone ? (
        <svg
          width="13" height="13" viewBox="0 0 13 13" fill="none"
          stroke="#001A0A" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M2 7l3 3 6-7" />
        </svg>
      ) : (
        <span
          className={[
            "font-mono text-[12px] font-semibold",
            isActive ? "text-gold" : "text-text-3",
          ].join(" ")}
        >
          {n}
        </span>
      )}
    </div>
  );
}

function Step({
  n,
  state,
  title,
  description,
  action,
  last = false,
}: {
  n: number;
  state: StepState;
  title: string;
  description: React.ReactNode;
  action?: React.ReactNode;
  last?: boolean;
}) {
  return (
    <div
      className={[
        "grid gap-3 px-4 py-3.5",
        !last ? "border-b border-border-soft" : "",
      ].join(" ")}
      style={{ gridTemplateColumns: "38px 1fr auto" }}
    >
      <StepCircle n={n} state={state} />
      <div className="min-w-0">
        <div
          className={[
            "text-[13.5px] font-semibold leading-tight",
            state === "done" ? "text-text-3 line-through" : "text-text",
          ].join(" ")}
        >
          {title}
        </div>
        <div className="mt-1.5 text-[12px] text-text-2 leading-snug">{description}</div>
      </div>
      {action ? (
        <div className="flex items-start pt-0.5">{action}</div>
      ) : (
        <div />
      )}
    </div>
  );
}

function ChipBtn({ children, variant = "chip" }: { children: React.ReactNode; variant?: "primary" | "chip" | "ghost" }) {
  const base = "font-mono text-[11.5px] px-2.5 py-1 rounded cursor-pointer flex items-center gap-1";
  const styles = {
    primary: `${base} bg-gold text-black font-semibold`,
    chip:    `${base} border border-border-strong text-text-2 hover:text-text`,
    ghost:   `${base} text-text-3 hover:text-text`,
  };
  return <button className={styles[variant]}>{children}</button>;
}

function IngredientChip({ ingredient }: { ingredient: Ingredient }) {
  const { name, kind, installed } = ingredient;
  return (
    <span
      data-testid="ingredient-chip"
      data-installed={String(installed)}
      className={[
        "inline-flex items-center gap-1 px-2 py-0.5 rounded-sm border font-mono text-[10.5px]",
        installed
          ? "border-gold-soft bg-gold/10 text-gold"
          : "border-warn/60 bg-warn/[0.08] text-warn",
      ].join(" ")}
    >
      {/* check / plus icon */}
      {installed ? (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M2 7l3 3 5-6" />
        </svg>
      ) : (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M6 2v8M2 6h8" />
        </svg>
      )}
      {name}
      <span className="text-[9px] tracking-[0.14em] text-text-4 uppercase">{kind}</span>
    </span>
  );
}

// ── component ────────────────────────────────────────────────────────────────
export function InstallSteps({ receipt }: { receipt: Receipt }) {
  const { install } = receipt;
  const missingCount = install.ingredients.filter((i) => !i.installed).length;

  return (
    <div className="py-1">
      {/* Step 1 — XVN detected */}
      <Step
        n={1}
        state={install.xvnDetected ? "done" : "active"}
        title="XVN install detected"
        description={
          install.xvnDetected ? (
            <>
              Connected to your XVN at{" "}
              <span className="font-mono text-gold">{install.xvnEndpoint}</span>
            </>
          ) : (
            <span className="text-warn">
              XVN not detected — install XVN locally and reopen this receipt.
            </span>
          )
        }
      />

      {/* Step 2 — Decrypt sealed bundle */}
      <Step
        n={2}
        state="active"
        title="Decrypt sealed bundle"
        description={
          <>
            Sealed bundle from IPFS — your license token authorises decryption.{" "}
            About to fetch{" "}
            <span className="font-mono text-text-2">{receipt.license.bundleCid}</span>.
          </>
        }
        action={<ChipBtn variant="primary">Decrypt now</ChipBtn>}
      />

      {/* Step 3 — Install missing ingredients */}
      <Step
        n={3}
        state="pending"
        title="Install missing ingredients"
        description={
          <div>
            <span className="text-text-2">
              {install.ingredients.filter((i) => i.installed).length} of{" "}
              {install.ingredients.length} already installed in your XVN.
            </span>
            <div className="flex flex-wrap gap-1.5 mt-2">
              {install.ingredients.map((ing) => (
                <IngredientChip key={ing.name} ingredient={ing} />
              ))}
            </div>
          </div>
        }
        action={
          missingCount > 0 ? (
            <ChipBtn variant="chip">
              {/* plus icon */}
              <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <path d="M6 2v8M2 6h8" />
              </svg>
              Install missing ({missingCount})
            </ChipBtn>
          ) : undefined
        }
      />

      {/* Step 4 — Add to Strategies */}
      <Step
        n={4}
        state="pending"
        title="Add to your Strategies and run backtest first"
        description={
          <>
            Lands in{" "}
            <span className="font-mono text-text-2">
              Strategies / Marketplace · {receipt.listing.id}
            </span>
            . Recommended: 7-day backtest with 2% risk cap before going live.
          </>
        }
        action={
          <div className="flex gap-1.5">
            <ChipBtn variant="chip">Add to strategies</ChipBtn>
            <ChipBtn variant="ghost">
              Open in XVN
              <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
                <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </ChipBtn>
          </div>
        }
        last
      />
    </div>
  );
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/routes/InstallSteps.test.tsx`
Expected: PASS (7 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/routes/InstallSteps.tsx src/features/marketplace/routes/InstallSteps.test.tsx
git commit -m "feat(marketplace): F6 InstallSteps panel"
```

---

## Task 3: `ShareComposer` — share panel with mini OG preview + intent links

**Files:**
- Create: `src/features/marketplace/routes/ShareComposer.tsx`
- Create: `src/features/marketplace/routes/ShareComposer.test.tsx`

### Step 1: Write the failing test

```tsx
// src/features/marketplace/routes/ShareComposer.test.tsx
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { MemoryRouter } from "react-router-dom";
import { describe, expect, it, vi, beforeEach } from "vitest";
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
      expect(screen.getByText(new RegExp(v.slice(0, 20), "i"))).toBeInTheDocument();
    }
  });

  it("clicking a variant updates the caption textarea", async () => {
    const user = userEvent.setup();
    wrap(<ShareComposer share={share} />);
    const firstVariant = share.variants[0];
    await user.click(screen.getByText(new RegExp(firstVariant.slice(0, 20), "i")));
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm exec vitest run src/features/marketplace/routes/ShareComposer.test.tsx`
Expected: FAIL — `ShareComposer` module not found.

### Step 3: Implement `ShareComposer`

```tsx
// src/features/marketplace/routes/ShareComposer.tsx
// Share composer — mini OG preview + editable caption + variant picker + intent links.
// No modals. Post targets are <a target="_blank"> (Twitter intent, Warpcast compose,
// Discord webhook, clipboard). ShareableCard renders at full-container width inside a
// 1200/630 aspect-ratio box for the mini preview.
import { useState } from "react";
import { ShareableCard } from "@/features/marketplace/components/ShareableCard";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import type { ShareComposerData } from "@/features/marketplace/data/types";

function buildTwitterUrl(caption: string, url: string): string {
  const params = new URLSearchParams({ text: caption, url });
  return `https://twitter.com/intent/tweet?${params.toString()}`;
}

function buildWarpcastUrl(caption: string, url: string): string {
  const params = new URLSearchParams({ text: `${caption} ${url}` });
  return `https://warpcast.com/~/compose?${params.toString()}`;
}

// Discord doesn't have a universal web-based composer intent;
// best available is a direct share-url that opens a DM or channel paste.
// We produce a pre-formatted clipboard-friendly URL for the Discord link.
function buildDiscordUrl(caption: string, url: string): string {
  // Discord has no web intent — we point to discord.com/channels/@me with
  // the text pre-encoded as a hash for copy-paste parity. Callers fall back
  // gracefully.
  const params = new URLSearchParams({ text: `${caption} ${url}` });
  return `https://discord.com/channels/@me?${params.toString()}`;
}

export function ShareComposer({ share }: { share: ShareComposerData }) {
  const { ogCard, buyerStamp, variants, notificationHint } = share;
  const [caption, setCaption] = useState(share.caption);

  const shareUrl = `https://${ogCard.url}`;
  const twitterHref  = buildTwitterUrl(caption, shareUrl);
  const warpcastHref = buildWarpcastUrl(caption, shareUrl);
  const discordHref  = buildDiscordUrl(caption, shareUrl);

  return (
    <div className="p-3 flex flex-col gap-3">
      {/* ── Mini OG preview ───────────────────────────────────────────────── */}
      <div>
        <div
          data-og-preview
          className="relative rounded-md border border-border overflow-hidden bg-black"
          style={{ aspectRatio: "1200 / 630" }}
        >
          {/* ShareableCard renders the 1200×630 composition; scale-to-fit via transform */}
          <div
            style={{
              width: "1200px",
              height: "630px",
              transform: "scale(var(--og-scale, 1))",
              transformOrigin: "top left",
            }}
            // Let CSS handle the scale via a container-size trick; in RTL tests
            // the DOM is present and the aspect-ratio wrapper provides the right
            // visual bounding box. Width is exact for snapshot purposes.
          >
            <ShareableCard data={ogCard} />
          </div>
        </div>

        {/* Buyer stamp — absolute is inside the aspect-ratio div above in production;
            in the plan render we place it as a sibling with negative margin for
            simplicity. Implementation must place it as absolute inside the preview. */}
        {buyerStamp && (
          <div
            // This must be positioned absolute top-1.5 right-1.5 over the preview div
            // above. The test finds it via text regardless of exact position.
            className="mt-1 font-mono text-[9.5px] text-text-3"
            aria-label="buyer stamp"
          >
            {buyerStamp}
          </div>
        )}

        <div className="mt-1.5 flex items-center justify-between">
          <span className="font-mono text-[9.5px] uppercase tracking-[0.16em] text-text-3">
            OG CARD · 1200 × 630
          </span>
          <span className="font-mono text-[9.5px] text-text-3">
            twitter / farcaster / opengraph
          </span>
        </div>
      </div>

      {/* ── Caption editor ────────────────────────────────────────────────── */}
      <div>
        <div className="font-mono text-[9px] uppercase tracking-[0.18em] text-text-3 mb-1.5">
          CAPTION
        </div>
        <textarea
          value={caption}
          onChange={(e) => setCaption(e.target.value)}
          rows={4}
          className="w-full px-2.5 py-2 rounded border border-border-strong bg-surface-elev text-[12.5px] text-text leading-snug resize-none font-sans focus:outline-none focus:border-gold/50"
        />
      </div>

      {/* ── Suggested variants ────────────────────────────────────────────── */}
      <div className="rounded-sm border border-dashed border-border-strong p-2.5">
        <div className="font-mono text-[8.5px] uppercase tracking-[0.18em] text-text-3 mb-2">
          SUGGESTED VARIANTS
        </div>
        <div className="flex flex-col gap-1">
          {variants.map((v) => (
            <button
              key={v}
              onClick={() => setCaption(v)}
              className="font-mono text-[10.5px] text-text-3 text-left hover:text-text py-0.5"
            >
              ↳ {v}
            </button>
          ))}
        </div>
      </div>

      {/* ── Post-to targets ───────────────────────────────────────────────── */}
      <div>
        <div className="font-mono text-[9px] uppercase tracking-[0.18em] text-text-3 mb-1.5">
          POST TO
        </div>
        <div className="grid grid-cols-2 gap-1.5">
          <a
            href={twitterHref}
            target="_blank"
            rel="noreferrer"
            className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-2 py-1.5 flex items-center justify-center gap-1.5 hover:text-text"
          >
            X / Twitter
            <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
              <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </a>

          <a
            href={warpcastHref}
            target="_blank"
            rel="noreferrer"
            className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-2 py-1.5 flex items-center justify-center gap-1.5 hover:text-text"
          >
            Farcaster
            <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
              <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </a>

          <a
            href={discordHref}
            target="_blank"
            rel="noreferrer"
            className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-2 py-1.5 flex items-center justify-center gap-1.5 hover:text-text"
          >
            Discord
            <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
              <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </a>

          <button
            onClick={() => navigator.clipboard?.writeText(`${caption} ${shareUrl}`)}
            className="font-mono text-[11.5px] text-text-2 border border-border-strong rounded px-2 py-1.5 flex items-center justify-center gap-1.5 hover:text-text"
          >
            Copy link
            <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
              <rect x="4" y="4" width="7" height="7" rx="1" />
              <path d="M2 8V2h6" strokeLinecap="round" />
            </svg>
          </button>
        </div>

        {/* Primary CTA */}
        <a
          href={twitterHref}
          target="_blank"
          rel="noreferrer"
          className="mt-2.5 w-full bg-gold text-black font-mono text-[12.5px] font-semibold rounded py-2 flex items-center justify-center hover:opacity-90"
        >
          Post to X
        </a>
      </div>

      {/* ── Chain-native notification hint ────────────────────────────────── */}
      <div className="flex items-center gap-2 px-3 py-2 rounded border border-gold-soft bg-gold/10">
        <AgentIcon size={11} />
        <span className="font-mono text-[11px] text-gold">{notificationHint}</span>
      </div>
    </div>
  );
}
```

> **Implementation note on the mini preview scale:** The `ShareableCard` component renders at a fixed `1200px × 630px` internal size (as built in F0 — `style={{ width: "1200px", height: "630px" }}`). Inside the `aspect-ratio: 1200/630` wrapper the implementer must apply a CSS scale transform so it fills the ~352px (380px column minus padding) container. The correct scale factor is `containerWidth / 1200`. Use a `ref` + `ResizeObserver` or a hardcoded `scale(0.293)` for the ~352px case; set `transform-origin: top left` and give the outer div `overflow: hidden`. The test only checks that `[data-og-preview]` is present and that the buyer stamp text is in the DOM — it does not assert pixel dimensions, so any scale approach that keeps the card in the DOM passes.

> **Buyer stamp overlay:** In the final implementation, move the buyer stamp `<div>` inside the `data-og-preview` wrapper as `position: absolute; top: 6px; right: 6px; …`. The test finds it by text content so placement is not test-visible; this note is for visual fidelity.

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm exec vitest run src/features/marketplace/routes/ShareComposer.test.tsx`
Expected: PASS (10 tests).

- [ ] **Step 5: Commit**

```bash
git add src/features/marketplace/routes/ShareComposer.tsx src/features/marketplace/routes/ShareComposer.test.tsx
git commit -m "feat(marketplace): F6 ShareComposer panel"
```

---

## Task 4: Wire `ReceiptRoute` into `routes.tsx`

**Files:**
- Edit: `src/routes.tsx` — one import line swap + one element swap

### Step 1: Add the lazy import for `ReceiptRoute`

In `src/routes.tsx`, add one new lazy import directly after the existing `MarketplaceSellStub` import line (line 63):

```ts
// BEFORE (line 64):
const MarketplaceReceiptStub = lazy(() => import("./features/marketplace/routes/stubs").then((m) => ({ default: m.MarketplaceReceiptStub })));

// AFTER (replace that line with):
const MarketplaceReceiptRoute = lazy(() => import("./features/marketplace/routes/ReceiptRoute").then((m) => ({ default: m.ReceiptRoute })));
```

### Step 2: Swap the element in the route tree

Find the `receipts/:tx` route entry (around line 192):

```tsx
// BEFORE:
{ path: "receipts/:tx", element: page(<MarketplaceReceiptStub />) },

// AFTER:
{ path: "receipts/:tx", element: page(<MarketplaceReceiptRoute />) },
```

### Step 3: Verify typecheck

Run: `pnpm typecheck`
Expected: PASS.

### Step 4: Verify routing smoke test still passes

Run: `pnpm exec vitest run src/features/marketplace/marketplace-routes.test.tsx`
Expected: PASS (the existing tests cover browse and lineage stubs; the receipt route is not in that test but the overall routing tree must remain valid).

### Step 5: Commit

```bash
git add src/routes.tsx
git commit -m "feat(marketplace): F6 wire ReceiptRoute into routes.tsx"
```

> **Note:** `MarketplaceReceiptStub` remains exported from `stubs.tsx` — do not remove it. It may be needed for fallback/test isolation. Only the import in `routes.tsx` changes.

---

## Task 5: Full integration smoke test

**Files:**
- Append to: `src/features/marketplace/marketplace-routes.test.tsx` (or create a separate `receipt-route.integration.test.tsx` if the existing file grows unwieldy)

### Step 1: Write the integration smoke test

Append the following describe block to `src/features/marketplace/marketplace-routes.test.tsx`:

```tsx
// ── Receipt route integration (appended to marketplace-routes.test.tsx) ──────
import { ReceiptRoute } from "./routes/ReceiptRoute";

// Extend routerAt to also cover the receipt route:
function receiptRouterAt(path: string) {
  return createMemoryRouter(
    [
      {
        path: "/marketplace",
        element: <MarketplaceLayout />,
        children: [
          { index: true, element: <MarketplaceBrowseStub /> },
          { path: "lineage/:name", element: <MarketplaceLineageStub /> },
          { path: "receipts/:tx", element: <ReceiptRoute /> },
        ],
      },
    ],
    { initialEntries: [path] },
  );
}

describe("marketplace receipt route integration", () => {
  it("renders the receipt page for the demo fixture tx", async () => {
    render(<RouterProvider router={receiptRouterAt("/marketplace/receipts/0xdemo-tx")} />);
    expect(await screen.findByText(/You bought/)).toBeInTheDocument();
    expect(await screen.findByText("btc-momentum-v3")).toBeInTheDocument();
  });

  it("renders all four install step titles", async () => {
    render(<RouterProvider router={receiptRouterAt("/marketplace/receipts/0xdemo-tx")} />);
    expect(await screen.findByText(/Decrypt sealed bundle/i)).toBeInTheDocument();
  });

  it("renders the share composer with Post to X CTA", async () => {
    render(<RouterProvider router={receiptRouterAt("/marketplace/receipts/0xdemo-tx")} />);
    expect(await screen.findAllByText(/Post to X/i)).not.toHaveLength(0);
  });

  it("unknown tx falls back to demo receipt (fixture behaviour)", async () => {
    render(<RouterProvider router={receiptRouterAt("/marketplace/receipts/0xunknown")} />);
    // FixtureMarketplaceData.getReceipt falls back to 0xdemo-tx for unknown hashes
    expect(await screen.findByText(/You bought/)).toBeInTheDocument();
  });
});
```

### Step 2: Run to verify it passes

Run: `pnpm exec vitest run src/features/marketplace/marketplace-routes.test.tsx`
Expected: PASS (all tests including the 2 pre-existing + 4 new).

### Step 3: Run the full marketplace test suite

Run: `pnpm exec vitest run src/features/marketplace/`
Expected: ALL PASS.

### Step 4: Commit

```bash
git add src/features/marketplace/marketplace-routes.test.tsx
git commit -m "test(marketplace): F6 receipt route integration smoke"
```

---

## Self-review checklist

Before considering F6 complete, verify each item:

- [ ] `pnpm exec vitest run src/features/marketplace/` — all tests pass
- [ ] `pnpm typecheck` — zero errors
- [ ] No `Dialog`, `Modal`, `Sheet`, `Popover` imports anywhere in the three new files
- [ ] All post targets are `<a target="_blank" rel="noreferrer">` or `window.open`; none open inline overlays
- [ ] `ShareableCard` is imported from `@/features/marketplace/components/ShareableCard` (no copy-paste)
- [ ] `TxChip` receives both `hash={receipt.txHash}` and `network={receipt.network}`; the `[Testnet]` label appears for `mantle-sepolia`
- [ ] `GenArtPlaceholder` is used for the license NFT art — no inline SVG duplicate
- [ ] `data-og-preview` attribute present on the mini preview wrapper
- [ ] `data-testid="ingredient-chip"` and `data-installed` on each ingredient chip
- [ ] No inline color hex values in JSX — token classes only (`text-gold`, `text-warn`, `bg-gold/10`, etc.)
- [ ] `border-border` / `border-border-strong` used on all card/panel borders — no `border-white`, `border-gray-100/200`
- [ ] `routes.tsx` change is exactly two lines: one import swap and one element swap

---

## Open questions

These are not blockers for F6 but should be resolved before a production-ready receipt surface ships:

1. **Mini OG preview scale:** `ShareableCard` renders at a hard 1200×630. The column is ~352px wide (380px minus padding). The implementer must apply `scale(containerWidth / 1200)` dynamically. A `ResizeObserver` approach is cleanest; a hardcoded `scale(0.293)` works for the fixed column width but breaks on narrow viewports. Recommend `ResizeObserver` + a `scaleStyle` state variable. This is a visual concern only — no test assertion blocks on it.

2. **"Decrypt now" action wiring:** Step 2 shows a "Decrypt now" button. The real action calls the decryption relay (`POST /decrypt-bundle` authenticated by `LicenseToken.balanceOf(buyer) >= 1`). No such endpoint exists in the data seam today — the button is a no-op stub in F6. Wire it when the decrypt relay lands (Phase H6 per program strategy).

3. **"Install missing" action wiring:** Step 3's "Install missing (N)" button should trigger MCP/skill installs in the local XVN. No install endpoint exists in the seam today. Stub in F6; wire with the ingredient-install surface.

4. **"Add to strategies / Open in XVN" deep links:** Step 4 should route to `/strategies?importFrom=<bundleCid>` or similar. The exact deep-link format is not yet specified. Leave as no-op buttons in F6.

5. **"Copy link" clipboard permission:** `navigator.clipboard.writeText` requires user gesture + secure context. In tests it is `undefined`. Consider a `try/catch` fallback that shows a toast. Since no popup is allowed, a transient toast (allowed per the no-popup rule) is the right pattern.

6. **Discord intent URL:** Discord has no standard web-based composer intent. The current plan encodes a `?text=` param on `discord.com/channels/@me` as the best available approximation. Confirm with product whether a Discord webhook or a "copy formatted" fallback is preferred.

7. **Receipt for unknown txHash:** `FixtureMarketplaceData.getReceipt` returns the demo receipt for any unknown hash. In production this should return a 404. The error path in `ReceiptRoute` is wired but untested against a real 404 since the fixture never throws. Add a test for the error state once the real client can throw.

8. **`transferableLicense` badge:** `receipt.license` does not carry a `transferable` flag directly (it would come from the listing). Consider showing a "non-transferable" note under the NFT card. The design reference says "non-transferable · ERC-1155 on Mantle" — source that from `ListingRow.transferableLicense` if the listing is available in the receipt context, or hardcode per the design until the receipt type is enriched.

---

## Task count summary

| Task | Description | New files | Tests |
|---|---|---|---|
| 1 | `ReceiptRoute` page shell + header | 2 | 5 |
| 2 | `InstallSteps` panel | 2 | 7 |
| 3 | `ShareComposer` panel | 2 | 10 |
| 4 | `routes.tsx` wiring | 0 (edit 1) | — |
| 5 | Integration smoke | 0 (edit 1) | 4 |

**Total:** 4 new source files + 3 new test files (appended or created) + 2 single-line edits. 26 new test assertions.
