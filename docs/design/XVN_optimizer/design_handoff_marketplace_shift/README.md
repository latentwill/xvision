# Handoff: Marketplace Shift (Vibetrader Redesign)

## Overview

This handoff specifies the buyer/seller-facing redesign of the XVN marketplace + identity surfaces. The current `Blockchain Surfaces.pdf` design serves **Persona A (the operator running their own XVN)**; this redesign serves **Persona B (vibetraders, AI builders, and traders who want alpha)**.

It implements the design direction set in [`2026-05-26-marketplace-design-direction.md`](./2026-05-26-marketplace-design-direction.md) (also bundled here for reference). The shift is:

- **The strategy card is the product, not the marketplace.** Every strategy gets an OG-card-perfect identity URL that is the screenshot moment.
- **`Paid by N humans + M agents` is the moat** — the human/agent split is shown everywhere.
- **Self-hosted XVN listens for on-chain events** and surfaces purchase/usage notifications with a pre-loaded share composer.
- **Cloning is the fork mechanic** — every clone creates a parent edge in the on-chain lineage tree.
- **Filter slices generate shareable URLs** that mint OG cards per slice.

Six distinct surfaces are spec'd:

| # | Surface | Route | Purpose |
|---|---|---|---|
| 1 | Marketplace browse | `/marketplace` | Trader-grade list, sort, filter |
| 2 | Creator profile | `/marketplace/creator/<handle>` | OG creator virality, chain-derived |
| 3 | Lineage identity (closed) | `/marketplace/lineage/<name>` | The viral identity page above the fold |
| 4 | Lineage identity (drawer open) | same | Auditor view collapsed one click deep |
| 5 | Purchase receipt | `/marketplace/receipts/<tx-hash>` | Post-buy install + share composer |
| 6 | Shareable OG card | n/a (image) | 1200×630 social-preview composition |

## About the Design Files

The files in this bundle are **design references created in HTML + React (via in-browser Babel)** — prototypes showing intended look and behavior, **not production code to copy directly**.

Recreate these designs in the target codebase (`xvision/frontend`, likely Next.js + React) using its established patterns, components, design tokens, and routing. If a public viewer is being stood up at `xvn.market` (per direction §6.1 Option A), that's the right home for these surfaces; the same components are embedded in the self-hosted XVN's Marketplace tab.

Use the embedded Babel scripts and inline styles only as a spec, **not** as the implementation pattern.

## Fidelity

**High-fidelity (hifi).** Final colors, typography, spacing, copy, and interactions. The Signal-theme palette is locked. The BRKT brand mark is locked per [`XVN Logo Handoff.html`](https://github.com/...).

## Design system context

The visual system is already established (see `Blockchain Surfaces.html`). Reuse:

- Sidebar nav, top status bar, breadcrumb chrome
- `Card`, `Btn`, `TxChip`, `StatusPill`, `Icon` primitives
- Signal-green accent palette
- Geist + Geist Mono type stack
- BRKT brand lockup (green brackets around `XVN` mono wordmark) — see `bc-shared.jsx → BrandMark`

New primitives introduced in this redesign:

- `GenArt` — deterministic SVG generator keyed on `lineage_id + manifest_hash`. Renders consistently from 32px favicon to 1200px hero. **Critical:** the algorithm must be reproducible byte-for-byte in production (same hashes → same art) because it's how the NFT artwork is stamped on-chain via `data:` URI in `tokenURI`.
- `Sparkline` — 30-point inline trend, positive-tinted gold or negative-tinted red.
- `AgentIcon` — small bot SVG used wherever the `🤖` agent count appears (we avoid emoji for brand control).
- `VerifiedBadge` — green-check pill condensing the attestation machinery into one trust signal.
- `X402Badge` — agent-paid-purchase indicator.
- `AssetPill` — colored ticker pill (BTC = amber, ETH = sky, SOL = violet, etc.).
- `RemovableChip` — applied filter chip with × to remove.
- `FilterDrawer` — right-edge slide-in panel holding all filter categories at once (Sort, Assets, Models, Style, Trust toggles, Price range, Min buyers). Replaces popovers per the **no-popups** rule.
- `ShareableCard` — the OG card composition (1200×630).

---

## Screens / Views

### 1. Marketplace browse — `/marketplace`

**Layout:** 200px sidebar · top status bar · page header strip (1-line promise + counter flex) · toolbar (segmented + search + sort + Filters button) · applied-filter chip row · body grid (232px leaderboard rail | strategy list).

**Page header strip:**
- H1 (24px / 600 / -0.025em): *"Buy a strategy. Run it. Or share yours and get paid."*
- Counter flex (Geist Mono 11.5px / `--text-3`):
  - `1,247 strategies · $34,820 paid this week · 🤖ic 218 agent purchases · 64 minted in 24h`
- Right CTAs: `Share` (ghost) · `+ Share your strategy` (primary)

**Toolbar:**
- Segmented `Trending | New | Mine` (active state = solid `--gold`)
- Search field (max 380px, `/` shortcut hint)
- `Sort · 30d return ▾` (FilterButton)
- Divider
- `Filters [4] ›` (FilterButton — opens drawer)
- `Save view` (ghost, right-aligned)

**Applied-filter row:** `APPLIED  [Asset: BTC ×] [Asset: SOL ×] [Model: Claude ×] [Verified only ×]   Clear all                          342 matches`

**Leaderboard rail (232px):** 7 saved filter slices, each with stable URL:
1. **Trending** (active by default, weighted by 24h velocity × return) — `1247`
2. Top on SOL · 7d — `142`
3. Top with Claude — `431`
4. Most agent-bought — `88`
5. Newest 24h — `23`
6. Most cloned — `64`
7. Free-tier breakouts — `17`

Active slice has `--gold` left-border + `--gold-bg` background.

Below the slices, a `CHAIN OPS` callout pointing to Settings → Chain ops (where anchor / mint / attester actions live now, per the redesign's deletion of operator surfaces from the public browse).

**List row** (8-column grid: `56px 1.8fr 0.75fr 1.1fr 1.05fr 0.6fr 0.85fr 110px`):
- 48px gen-art thumb (radius 4)
- Name + version + verified + x402 badges; below: `@creator · Model · Style`
- Asset pills (BTC/ETH/SOL with their tones)
- Big return % (16px / 600 / gold or danger) + sparkline (88×24)
- Buyer count: `247 · [🤖 14]`
- Sharpe muted right-aligned (`--text-3`)
- Price `49 USDC` mono OR `● OPEN` gold pill for Tier-A free listings
- CTA: `Buy` (primary) or `Run free` (primary, free tier)

**Filter drawer (right edge, 400px wide):**
- Header: "Filter strategies" + meta "4 filters active · 342 of 1,247 match" + close button
- Sections (scrollable body):
  - **Sort by** — radio list (5 options)
  - **Assets** — search + 4 grouped checklists (Majors, L2 & memes, Equities, FX) with strategy counts per asset. Selected = `--gold-bg` background + green check.
  - **Models** — checklist of model providers
  - **Style** — chip multi-select
  - **Trust** — toggle switches (Verified only, Accepts agents, Audited only)
  - **Price (USDC)** — dual-handle range slider (0–500)
  - **Minimum buyers** — single-min range slider
- Footer: `Clear all` · `342 matches` · `Apply` (primary)

The drawer covers the list area only (sidebar + leaderboard rail stay visible). Backdrop dim: `rgba(0,0,0,0.55)` with `backdrop-filter: blur(1.5px)`.

---

### 2. Creator profile — `/marketplace/creator/<handle-or-address>`

**Fully derived from chain.** Wallet IS the account. No off-chain signup.

**Hero (3-col grid: `96px · 1fr · 280px`, left-padded to 44px to align with card titles):**
- 96px deterministic identicon (GenArt seeded from address)
- `@ed` (Geist Mono 28px / 600) + ENS pill (`ed.xvn`) + agent-#0 badge
- Address row: `0xa83e…f12d4` + copy/Mantlescan + joined date + rep
- Right: `Follow @ed` primary CTA, `Share profile` + `Tip` ghost row

**Counter flex (6 columns, also padded to 44px):**
1. **Strategies** — `3`
2. **Lifetime earned** — `$4,820` (gold)
3. **Total buyers** — `469` with `🤖+27` sub
4. **Clones spawned** — `11` · "upstream of $2.1k"
5. **Attestations** — `14 issued`
6. **Member since** — `9mo ago`

**Strategies row (2-col: 1fr | 380px):**
- **Strategies card** (left) — 3-up grid of compact strategy cards. Each: gen-art (46px) + name + asset pills + verified/x402 badges; bottom row: 30d % · buyers (h+a) · clones.
- **Earnings · weekly** (right) — 32-week area chart (gold fill gradient + stroke). Sub: `+$420 last 7d · +$1,180 last 30d`.

**Lineage forest card** — SVG visualization. Each row = one of the creator's lineages (BTC-MOMENTUM, BTC-GRID, ETH-MR labeled left). Variant nodes are mini gen-art tiles. Edges: solid `--border-strong` for variant-of, dashed `--info` for clone edges. HEAD nodes have `--gold` 2px border. Clones-by-others appear as ghosted nodes branching off, with a "+6 more" stub. Legend in the card header.

**Bottom row (2-col):**
- **Reputation** — Activity feed of issued/received attestations with verdict pills (ENDORSE/QUESTION/REJECT) and `RECEIVED`/`ISSUED` direction labels.
- **Cloned by · downstream** — Who cloned this creator's work, what they made, and how much they earned (sum of upstream attribution).

---

### 3. Lineage identity (closed) — `/marketplace/lineage/<name>`

**This is the destination of every viral share.** Single-screen above-the-fold; the on-chain receipts drawer collapses everything auditor-grade out of sight.

**Above the fold (3-col: `320px · 1fr · 250px`):**

Left — hero gen-art (320×320, full-bleed inside the column). NFT stamp overlay `NFT #0043` bottom-left.

Middle — info stack:
- Title `btc-momentum-v3` mono 30px + version pill + verified + x402 badges
- Creator line `@ed · 0xa83e…f12d4 · Claude Haiku 4.5`
- One-line promise: *"BTC momentum with Claude regime detection. Holds 1–3 days, 2% risk cap."* (14.5px / 1.45 line-height)
- Big metric row (`auto 1fr 1fr 1fr 1fr` grid):
  - **30D RETURN** `+47.2%` (Geist Mono 42px / 600 / gold)
  - Sharpe `+1.31`
  - Win rate `62%`
  - Max DD `-8.4%` (warn)
  - Avg dur `1.8d`
- Buyer card: 5 mini avatars + agent stamp, "Run by **247 humans + 14 agents**", sub "$1,240 paid to @ed · 5% platform fee"

Right — purchase column:
- Price card with gold-tinted gradient bg, `49 USDC`, "perpetual license · one-time", `Buy` primary button (full-width, 13.5px / 700)
- Row of `Clone to edit` + `Share` ghost buttons

**Ingredient check banner** (full-width below hero, warn-tinted bg):
- Warn-amber circle icon
- "**Ingredient check · 2 of 4 installed in your XVN.** Install the missing two before purchase."
- 4 ingredient pills (green-check if installed, plus-amber if missing) with kind label (MODEL / MCP / SKILL)
- `Install missing` chip CTA on right

**Below the fold (2-col: 1fr | 380px):**

Left column:
- **Equity curve** card — 90d chart, base $1,000. Backtest segment dashed faded, live segment solid gold. `LIVE` marker at the boundary. Toggle buttons: `If I bought at mint` · `30d` · `90d`.
- **What you get / What you don't get** — side-by-side cards with bulleted lists. "What you get" lists the sealed bundle (Tier 2); "What you don't get" lists Tier 3 exclusions.
- **Lineage tree** — horizontal mini-tree of variants (each node is a mini gen-art) connected by gray edges; HEAD node has gold border. Right side: "CLONES OF YOURS · 8 · upstream of $2.1k".

Right column:
- **Recent buyers + outcomes** — list of recent purchases with anonymized address or `agent #N`, outcome (`+12.4% · 6d`, `running · 2 trades`), and relative time.
- **More from @ed** — 2-3 of the creator's other strategies as compact cards.

**On-chain receipts drawer (collapsed, bottom):**
- Single row: `▸ View on-chain receipts · NFT, manifest hash, attestations, anchor history, validator activity                            AUDITOR 🛡`
- Hover/click expands to frame 4.

---

### 4. Lineage identity (drawer open) — same route, expanded state

When `▾ Hide on-chain receipts` is clicked, the auditor surface unfurls inline. Three cards in a 2-col grid + a full-width 4th:

**Identity NFT & manifest** (left, 2-col field/value):
- `nft_token_id #0043`, `lineage_id btc-momentum`, `agentURI ipfs://…`, `manifest_hash blake3:…`, `parent_lineage — (seed)`, `born_at`, `operator_sig`

**Attestation verdicts** (right):
- 4 verdict rows: regime-verifier ENDORSE v3.0 · diversity-check ENDORSE v3.0 · regime-verifier ENDORSE v2.1 · diversity-check QUESTION v3.1
- Verdict pills in tone-coded outline (gold / warn / danger)

**Anchor history** (full-width, span 2):
- 3 rows: MERKLE Snapshot · MINT Identity NFT minted · COMMIT SessionCommitment
- Each row: kind label · target · tx chip · gas + time

**Trade history** (full-width, span 2) — *the new card*:
- Header: "178 trades on chain · last anchor 2h ago · receipt_kind=TradeBatch"  · `Export ledger` ghost
- Filter row (mirroring eval-detail Decisions pattern): pills for `All / Buy / Sell / Close` with counts, plus `Runner: any ▾` and `Window: 7d ▾` selectors. Right: net P&L `+$94.88 · 7d window`.
- Table (9 cols): Time · Action (BUY/SELL/CLOSE tone-coded pill) · Sym · Qty · Entry · Exit · P&L (with % below) · Runner (agent badge or human address) · Tx chip
- Footer: `Showing 10 of 178 · all anchored under Merkle 0x2e1d…44a9` · prev/next · Mantlescan link

---

### 5. Purchase receipt — `/marketplace/receipts/<tx-hash>`

The post-buy moment. Sets up the install AND the share loop.

**Success header strip** (full-width, gold-tinted gradient bg):
- 44px green check circle
- H1: "You bought `btc-momentum-v3`"
- Sub line: `49 USDC paid · license #0184 minted · 46.55 USDC → @ed · 0xa83e…b91d4e (tx chip)`
- Right: `View on Mantlescan` ghost

**Body (3-col grid: `320px · 1fr · 380px`):**

**License NFT card** (left):
- 290px gen-art with two overlays: `LICENSE #0184` top-left, `OWNED · YOU` gold-tinted bottom-right
- Metadata stack: strategy, version, creator, manifest hash, IPFS bundle, paid (with fee breakdown), minted timestamp

**Install in your XVN** (middle):
- Card header sub: `detected at localhost:3000 · 4 steps · sealed bundle auto-decrypts`
- `Install all` primary right-aligned
- 4 step rows (`38px · 1fr · auto` grid):
  1. **XVN install detected** (done, struck-through, green check)
  2. **Decrypt sealed bundle** (active, gold ring, `Decrypt now` primary chip action)
  3. **Install missing ingredients** (4 ingredient chips inline showing installed/missing, `Install missing (2)` chip action)
  4. **Add to your Strategies and run paper-trade first** (`Add to strategies` + `Open in XVN` actions)

**Share composer** (right):
- Embedded mini OG card preview (1200×630 aspect, full width of column) with `just bought by 0x7c…aa07` stamp
- Size hint: `OG CARD · 1200 × 630 · twitter / farcaster / opengraph`
- **CAPTION** editor with pre-loaded copy:
  > I just bought `btc-momentum-v3` by @ed — running it now.
  > +47.2% in 30d · 247 humans + 14 agents already running it.
- **SUGGESTED VARIANTS** dashed-bordered list of 3 alt captions
- **POST TO** 4-button grid: X / Twitter · Farcaster · Discord · Copy link
- Full-width primary: `Post to X`
- Footer hint (gold-tinted): "@ed's XVN just got a +$46.55 notification" — this is the chain-native notification loop closing.

---

### 6. Shareable OG card — 1200 × 630

The image embedded in every share. Server-rendered (Astro / Next.js / Satori) per `<meta property="og:image">`. Walletless to view.

**Two-column split (1fr | 1fr, no XVN sidebar/chrome):**

**Left (600×630)** — full-bleed gen-art:
- Gen-art SVG scaled to 110% with -5% offset (slight crop bleed for visual interest)
- Subtle radial tint overlay: `radial-gradient(circle at 30% 30%, rgba(0,230,118,0.06), transparent 60%)`
- Top-left overlay: BRKT brandmark @ 20px + `XVN · MARKET` mono label
- Bottom-left stamp (blurred-glass pill): `NFT #0043 · MANTLE`

**Right (600×630)** — info composition (padding 38×44):
- Top row: VERIFIED + x402 badges (gold-tinted)
- Title `btc-momentum-v3` (Geist Mono 44px / 600)
- Creator line: `by @ed · v3.0`
- Promise paragraph (15px / 1.4)
- Big metric block (top + bottom border separators):
  - Left: `30D RETURN` label + `+47.2%` (Geist Mono 64px / 600 / gold / -0.035em)
  - Right: `RUN BY` label + `247 humans + [🤖ic 14 agents]` pill
- Bottom row: `BUY · USDC` label + `49 USDC` (30px) + `perpetual · $1,240 paid to creator` sub + URL `xvn.market/lineage/btc-momentum-v3` (gold)
- Bottom-right: green QR code on `--gold` background panel (78px square content, 5px padding)

The card is designed to be the screenshot artifact, but is also generated server-side as a `.png` so social-media scrapers render previews automatically.

---

## Interactions & Behavior

### Marketplace browse
- Search field debounced; query persisted to URL
- Sort change updates URL slug
- Filter chips × removes that filter and re-fetches
- Leaderboard slice click loads that saved view
- "Save view" snapshots current filter+sort to a new slice (with stable URL)
- Drawer open/close: slide in 220ms ease-out, backdrop fades

### Lineage identity
- `Buy` button → wallet prompt if not connected → on-chain `LicenseToken.mint` tx → on receipt redirect to `/marketplace/receipts/<tx>`
- `Clone to edit` → requires purchase first if Tier B; opens local XVN edit flow with parent edge written on-chain
- `Share` → opens share composer with caption + OG URL pre-loaded
- `Install missing` → links to plugin / MCP marketplace inside XVN
- Receipts drawer toggles `?receipts=open` URL param
- Trade history filter pills filter the table in place; pagination preserves filter state in URL

### Creator profile
- `Follow @ed` → optimistic toggle; backed by a small on-chain follow registry (or Lens / Farcaster if integrated)
- `Tip` → opens a `TipJar` panel with quick-amount buttons
- Lineage forest node click → routes to that variant's lineage page
- Reputation filter tabs (All / Received / Issued) filter the activity feed in place

### Purchase receipt
- Step 2 `Decrypt now` calls the decryption relay (signs auth if `LicenseToken.balanceOf(buyer) >= 1`)
- Step 3 inline ingredient install kicks off MCP / skill installs in the local XVN
- Step 4 surfaces a deep link into the XVN app's Strategies panel
- Share composer `Post to X` opens a new tab with `twitter.com/intent/tweet?text=…&url=…`; same for Farcaster (`warpcast.com/~/compose`) and Discord (webhook or copy-formatted link)

### Shareable card
- Static image — no interactions. Generated server-side per `og:image` request.

### Chain-native notifications (§3.3)
- Self-hosted XVN subscribes to `LicenseToken.Transfer` events filtered by `to == operator wallet`. On match:
  - Toast: `+$X from @buyer just bought btc-momentum-v3`
  - Tap → opens share composer pre-loaded with the updated buyer count
- Later: `ValidationRegistry` receipts for usage notifications ("btc-momentum-v3 just executed a trade for @buyer · +2.3%").

---

## State Management

- `useFilterState({ sort, assets, models, style, trustToggles, priceRange, minBuyers })` — synced bi-directionally to URL query params
- `useStrategy(lineage_id)` — fetches manifest from IPFS + subgraph data for ret/sharpe/buyer counts
- `useReceiptsDrawer(open)` — URL-backed boolean
- `useTradeHistoryFilter({ action, runner, window })` — local + URL
- `useCreatorProfile(handle_or_address)` — subgraph query for strategies, lineage edges, earnings, attestations
- `usePurchase(tx_hash)` — receipt page reads tx + LicenseToken state + ingredient detection on local XVN

Data sources:
- **Goldsky / The Graph subgraph** (per direction §6.5) — marketplace browse, creator profile aggregations
- **Pinata IPFS** (per direction §6.5) via gateway HTTPS — manifest JSONs, sealed bundles
- **Local XVN HTTP API** — `GET /installed-ingredients`, `POST /import-strategy`, `WS /subscribe` for live notifications

---

## Design Tokens

All tokens already exist in the project's CSS (`bc-shared.jsx` inline definitions + `Blockchain Surfaces.html` `:root`):

### Color

| Var | Value | Use |
|---|---|---|
| `--bg` | `#000` | Page bg |
| `--surface-sidebar` | `#000` | Sidebar |
| `--surface-card` | `#0A0A0A` | Card raised |
| `--surface-elev` | `#0E0E0E` | Chip / input bg |
| `--surface-panel` | `#121212` | Modal-equiv panels |
| `--border` | `#1A1A1A` | Default border |
| `--border-strong` | `#2A2A2A` | Toolbar / button border |
| `--border-soft` | `#141414` | Inner separators |
| `--text` | `#FFFFFF` | Primary text |
| `--text-2` | `#9CA3AF` | Secondary |
| `--text-3` | `#5F6670` | Muted / labels |
| `--text-4` | `#3A3F47` | Faded separators |
| `--gold` | `#00E676` | Brand · primary action · positive |
| `--gold-soft` | `#00B85F` | Darker green for light surfaces |
| `--gold-bg` | `rgba(0,230,118,0.10)` | Accent panel bg |
| `--gold-bg-strong` | `rgba(0,230,118,0.18)` | Stronger accent bg |
| `--warn` | `#FFB020` | Warning · pending |
| `--danger` | `#FF4D4D` | Negative · destructive |
| `--info` | `#5FA8FF` | Info · merkle · clone edge |

Asset pill tones (in `bc2-marketplace.jsx → AssetPill`):
- BTC: `#FBBF24` on `rgba(251,191,36,0.10)`
- ETH: `#5FA8FF` on `rgba(95,168,255,0.10)`
- SOL: `#A78BFA` on `rgba(167,139,250,0.10)`
- DOGE: `#F472B6` on `rgba(244,114,182,0.10)`

### Typography
- **Sans**: Geist 400 / 500 / 600 / 700 (Google Fonts)
- **Mono**: Geist Mono 400 / 500 / 600 / 700 (used for all numerics, addresses, identifiers, labels)
- Common sizes: 28 (page H1), 24 (success H1), 16 (return %), 13–13.5 (body), 11.5–12 (mono), 9–10 (ulabel)
- `--ulabel` style: Geist Mono 10px / 500 / 0.18em letter-spacing / uppercase / `--text-3`

### Spacing
- Card border-radius: 6px
- Small radius: 4px / 3px (pills, chips)
- Page horizontal padding: 28px
- Creator profile content padding: 44px (aligned to card-title position)
- Grid gaps: 12 / 14 / 18 / 24

### Brand
- **BRKT lockup** (locked). See `bc-shared.jsx → BrandMark`. SVG with green brackets + mono `XVN` wordmark, 24:7 aspect ratio. Brackets default `--gold` (`#00E676`), wordmark `currentColor`. On light surfaces, brackets become `--gold-soft` (`#00B85F`) and wordmark goes black.

---

## Generative art — implementation note

The `GenArt` component in `bc2-genart.jsx` is the algorithm to reproduce in production. Key contract:

- Input: a string seed (`agent_id + manifestHash` per direction §6.2 Tier 0)
- Output: an SVG (100×100 viewBox) that scales correctly from 32px to 1200px
- Deterministic: same seed → byte-identical SVG output forever
- Algorithm:
  1. Hash seed with FNV-1a (32-bit)
  2. Build LCG-style PRNG from the hash
  3. Pick palette from an 8-curated set (indexed by hash)
  4. Pick composition family from `["mesh", "rings", "blob", "stripes"]` (indexed by hash >> 4)
  5. Layer: radial-gradient bg + family-specific shapes (4–10 primitives)

**Critical:** This is the on-chain artwork stored as `data:image/svg+xml` in `tokenURI`. Lock the palette set and family selection rules in a written spec before deployment. Lineage-coherent variants (children of the same lineage NFT) share a base palette by construction since they share `lineage_id` prefix in their seed.

Sample-wall the algorithm at 200 strategies before going to production to make sure it doesn't look like garbage at scale (per direction §10 step 4).

---

## Assets

All visuals are SVG or CSS — no raster assets bundled.

The OG card needs server-side rendering (Astro / Next.js + Satori or Resvg) so social-media scrapers see a proper PNG, not the React HTML. Mirror the `ShareableCard` JSX into a Satori-compatible component.

---

## Files

| File | Purpose |
|---|---|
| `Marketplace Shift.html` | The host doc — boots React + Babel, includes all 6 frames in a design canvas |
| `bc-shared.jsx` | Sidebar, top status bar, `BrandMark` (BRKT lockup), `Card`, `Btn`, `TxChip`, `StatusPill`, `Icon`, `Frame`, `LineageDot` |
| `bc2-genart.jsx` | `GenArt` deterministic SVG, `Sparkline`, `AgentIcon` — the most important new primitives |
| `bc2-marketplace.jsx` | Frame 1 — browse list, filter drawer, leaderboard rail |
| `bc2-creator.jsx` | Frame 2 — creator profile incl. lineage forest |
| `bc2-lineage.jsx` | Frames 3 + 4 — lineage identity (closed and drawer-open variants), trade history card |
| `bc2-receipt.jsx` | Frames 5 + 6 — purchase receipt and shareable OG card |
| `bc2-canvas.jsx` | Frame definitions list and DesignCanvas root |
| `design-canvas.jsx` | The design canvas component itself — not part of production, just hosts the frames for review |
| `2026-05-26-marketplace-design-direction.md` | The source design direction this redesign implements |

---

## Open questions for the implementer

Carried from direction §8 — resolve before shipping:

1. **Domain** — `xvn.market` vs `xvision.dev`. Affects every URL the shareable card encodes.
2. **Handle system** — ENS, on-chain handle registry, or `agentURI` display-name. Affects `/marketplace/creator/<handle>` resolution.
3. **Wallet-connect UX** — Privy embedded vs WalletConnect vs MetaMask-direct. Affects the `Buy` flow on lineage identity.
4. **OG card SSR layer** — Astro vs Next.js vs custom. Must support Satori or Resvg for SVG → PNG with the BRKT font and GenArt component.
5. **Cloning sealed listings** — confirmed direction is "requires purchase first." Lock contract semantics so the lineage edge is only writable after `LicenseToken` mint.
6. **Verification badge thresholds** — what qualifies for the green check. Suggested: green = backtested + ≥30 days of live-paper data + at least one closed cycle with positive PnL hash on chain.
7. **Notification UX inside self-hosted XVN** — toast vs sidebar vs both. Where does the share composer live (modal-equivalent forbidden — inline panel only).

---

## Maintenance

- This handoff is **canonical** for the buyer-facing surfaces. The original PDF auditor design remains canonical for the operator-facing Settings surfaces (where anchor / mint / attester actions now live).
- When a contract surface change lands in `smart-contract-surface-design.md`, re-check that the manifest hash / NFT token id / LicenseToken / clone-edge writes line up with what these screens read.
- The GenArt algorithm spec **must** stay in sync between this design and the on-chain `tokenURI` builder. If you change the palette set or family list, every existing NFT's on-chain art changes — don't.
