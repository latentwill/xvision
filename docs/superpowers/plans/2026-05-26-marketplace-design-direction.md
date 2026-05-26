# Marketplace + Identity — Design Direction

> **Purpose:** Pare down the marketplace and ERC-8004 identity surfaces from the
> auditor-grade design currently in `XVN · Blockchain surfaces.pdf` into
> something a vibetrader, AI-twitter user, or trader can immediately understand
> and share. Captures the chat between Edward + Claude on 2026-05-26 and Edward's
> direction. Feeds Phase 1 of [`2026-05-26-blockchain-plan-navigation.md`](./2026-05-26-blockchain-plan-navigation.md)
> (design sprint).
>
> **Status:** Direction set; mockups + metadata spec not yet locked.
> **Related:**
> [`smart-contract-surface-design.md`](../specs/2026-05-08-smart-contract-surface-design.md),
> [`marketplace-plugin-design.md`](../specs/2026-05-09-marketplace-plugin-design.md),
> [blockchain nav doc](./2026-05-26-blockchain-plan-navigation.md).

---

## 1. Context

The current PDF design (`XVN · Blockchain surfaces.pdf`) renders the
marketplace and `btc-momentum` lineage page in what amounts to an auditor view:
attester verdicts (`ENDORSE regime-verifier`, `QUESTION diversity-check`),
anchor history (`merkle 0x4f8a…dc11 · 0.0024 ETH`), operator chores ("Anchor
all final", "Mint missing NFTs · est. ~0.001 ETH"), and content hashes
(`blake3:7f2b1ad…91c4`) above the fold. That surface is correct for **Persona A
(the operator running their own XVN)** when they need to confirm chain state.
It is wrong for **Persona B (buyers and sellers)** because it leads with
machinery, not money or art.

The audience for the buyer/seller surface is a Venn of three people:

- A trader who wants alpha.
- An AI builder who wants to see Claude/GPT/Gemini agents trade live.
- A "vibetrader" / get-rich-quick-x.com user who wants the screenshot moment.

All three respond to the same primitives — gen-art, a big return %, a buyer
count, a buy button — just labeled differently. The auditor view should
collapse into a single "On-chain receipts" drawer one click deep, available but
out of the way.

Reference for the column layout direction: [botspot.trade/marketplace](https://botspot.trade/marketplace).
It is dense, scannable, sorts cleanly. Start there.

---

## 2. Core thesis

**The strategy card is the product, not the marketplace.** The marketplace is a
directory. What goes viral is one strategy's identity page, screenshotted and
tweeted. Every decision below serves making that single shareable artifact
irresistible.

Three premises that follow from Edward's direction:

1. **XVN is free and self-hosted.** The dashboard, the engine, the CLI, the
   marketplace tab — all free. No SaaS account, no central server custody.
2. **Creators get paid; the platform takes a small fee.** Buyers pay sellers
   directly in USDC via the on-chain marketplace contract. The platform fee is
   the only revenue extracted, taken at settlement, fully on-chain.
3. **The only unified infrastructure is the blockchain.** No central database
   of users, no central API for strategy metadata, no central identity provider.
   Everything that needs to be cross-installation goes on chain or on IPFS.

The website question (which the PDF design implicitly assumed) is therefore
non-trivial — addressed in §6 (Infrastructure).

---

## 3. The viral loop

This is the loop the design needs to support, in order:

```
Creator mints strategy
  → screenshots the gen-art card
  → tweets it
  → cold visitor clicks the link
  → public lineage page loads with a Buy button
  → buys
  → creator's XVN catches the on-chain event
  → fires "+$X from @buyer just bought btc-momentum-v3" notification
  → creator tweets that
  → loop
```

The five primitives that make the loop spin:

### 3.1 The card is the unit of virality

Every minted strategy gets a public URL that renders an OG-card-perfect hero:
the gen-art, the name, the 30d return %, the buyer count, a buy button.
Walletless to view. Server-rendered HTML + Open Graph tags so X / Discord /
Reddit / Slack previews render correctly. This is the screenshot moment.

### 3.2 "Paid by 247 humans + 14 agents"

The second number is the moat. Botspot doesn't have it. Pump.fun doesn't have
it. "An autonomous agent just paid for my strategy" is a uniquely-2026 hook
for AI-twitter. Display the split everywhere — list rows, identity hero,
leaderboards, notifications. The `🤖` count is the differentiator.

### 3.3 Chain-native usage notifications

Genuinely novel because XVN is self-hosted: when someone buys a strategy, a
`LicenseToken` is minted, an event fires on Mantle. The creator's self-hosted
XVN install listens for events on its own recipient address and surfaces them
in-app, no central notification server, no polling SaaS.

Two flavors:

- **Purchase notification:** "+$X · @buyer just bought `btc-momentum-v3`."
  Tappable → opens a share composer pre-loaded with the gen-art card and the
  updated buyer count, ready to post.
- **Usage notification (later, depends on `ValidationRegistry` receipts):**
  "`btc-momentum-v3` just executed a trade for @buyer · +2.3%." Ongoing
  social proof — the strategy is *being used*, not just *being sold*.

This is the dopamine engine for sellers. Build it as a first-class feature; do
not relegate it to a tab.

### 3.4 Clone-to-edit (existing button, lineage-aware)

XVN already has a "Clone to edit" button on strategies. Make that the
marketplace's fork mechanic too. Cloning a marketplace strategy creates a
**parent edge in the on-chain lineage tree**. The creator of the original sees
"N strategies cloned from yours, upstream of $X in earnings." Free virality
engine + creator-recognition system, ArtBlocks-style.

Mechanic open question: does cloning from a Tier B (sealed) listing require
buying first? Default answer: yes, because the clone needs the sealed bundle's
contents. Cloning a Tier A (open) listing is free. Decision to lock in design
sprint.

### 3.5 Mutable leaderboards as shareable URLs

Every filter combination on the marketplace generates a stable URL: "Top SOL ·
Claude · this week" → `/marketplace?asset=sol&model=claude&window=7d&sort=return`.
Every **leaderboard slice also renders a shareable OG card**: "btc-momentum is
#1 on `Top SOL · 7d`," with the gen-art and the rank, ready to screenshot.
Creators rank-flex on slices, screenshot, share. The slices themselves become
the meme — invite curators to define new ones over time ("Top by agent
purchases," "Free-tier breakouts," etc.).

---

## 4. Columns, buttons, graphs

### 4.1 Marketplace (browse) — `/marketplace`

Top strip:

- **One-line promise:** "Buy a strategy. Run it. Or share yours and get paid."
- **Counter flex:** "1,247 strategies · $34k paid to creators this week ·
  `🤖` 218 agent purchases" (numbers pulled from chain).
- **Trending / New / Mine** toggle.
- **Search** (free text over name + creator handle).
- **Tag chips** (scrollable, multi-select): SOL · BTC · ETH · Equities ·
  Memes · Claude · GPT · Gemini · Long-only · Long/Short · Day · Swing.
  These are the "mutable leaderboard" knobs.
- **Connect wallet** (top-right, only required at purchase or mint time;
  browse is walletless).

The list, one row per strategy:

| Column | Why it earns its space |
|---|---|
| Gen-art thumbnail (~80px) | Visual identity, lineage-encoded, the hook |
| Name · `@creator` | Social attribution beats "NFT #0043" |
| Asset tag pills | SOL/BTC/ETH — filter at a glance |
| **30d return %** | Lead metric — the number people actually want |
| 30d sparkline | Free visual proof, ~30 SVG points, costs nothing |
| Sharpe (smaller, muted) | For the traders; subordinate to % |
| Buyers `247 · 🤖14` | Humans + agents split — the moat |
| Price (USDC) or `Open` | Tier B price or Tier A free-tier flag |
| `Buy` / `Run free` button | One click, inline, no modal (no-popups rule) |

Sorts: `30d return`, `Sharpe`, `Buyers`, `Newest`, `Most cloned`. Default
sort = `Trending` (a blended weight: recent buyer velocity × return %).

**Left rail (not a popup):** mutable leaderboard presets, each a saved
filter+sort with a stable URL. Initial set:

- Top on SOL · 7d
- Top with Claude
- Most agent-bought
- Newest 24h
- Most cloned
- Free-tier breakouts

Hidden from this surface (lives in the auditor drawer one click deep, or in
Settings → Chain ops): NFT ID, content hash, anchor status, attestation
verdicts, gas estimates, session IDs, "Anchor all final" CTAs, "Mint missing
NFTs" actions.

### 4.2 Strategy identity page — `/marketplace/lineage/<name>`

Route matches the existing app convention (sub-route under `/marketplace`, as
in the current PDF's `MARKETPLACE / LINEAGE / btc-momentum` breadcrumb). This
URL is the destination of every viral share.

Above the fold (single desktop screen, no scroll required):

| Region | Content |
|---|---|
| Hero gen-art, ~40% width | The piece. Subtle motion. This *is* the page. |
| Name · `@creator` · `vN.N` | One line, links to creator profile |
| One-line promise (creator-authored) | "BTC momentum with Claude regime detection." Optional. |
| **Big metric: 30d return %** | Hero number, color-coded |
| Secondary metrics row | Sharpe · Win rate · Max DD · Avg duration |
| Buyer counts | "Run by 247 humans + 14 agents" with small avatar bar |
| Lifetime paid to creator | "$1,240 paid to `@creator` · 5% platform fee" |
| Price + primary **Buy** button | One click, USDC, no popup |
| **Clone to edit** + **Share** buttons | Lower-friction CTA + copy-public-URL |
| Verification badge | One green check ("backtested + live-paper data") or gray |
| `🤖 x402` badge | When listing accepts agent-paid auto-purchase |

Below the fold, ordered by decreasing audience size:

1. **Equity curve.** Backtest + live. Toggle "If I bought at mint" overlay.
   One big chart.
2. **What you get.** Plain English. "Prompt, model (Claude Haiku 4.5), skills
   list, MCP list, risk caps."
3. **What you don't get.** Honesty. "Creator's data sources, future updates
   without re-purchase."
4. **Ingredient check.** If the visitor's XVN is connected (or installed):
   "Claude Haiku 4.5? ✅ Birdeye MCP? ❌ Install via plugin marketplace."
   Defuses no-install-no-value before paying. Doubles as upsell to MCP/skill
   marketplaces.
5. **Lineage tree.** Visual mini-tree, each node a tiny version of that
   variant's gen-art. Hover → metrics. Click → ancestor's page.
6. **Recent buyers + outcomes.** "Buyer `0x…7a` — +12% in 6d." Anonymous but
   chain-verifiable. Social proof.
7. **Creator's other strategies.** Funnel to the creator profile.

**On-chain receipts drawer** — single collapsible row at the bottom labeled
`▸ View on-chain receipts`. Expanded, it gives the PDF's view near-verbatim:
NFT ID, contract address, Mantlescan link, manifest hash, attestations (with
ENDORSE / QUESTION / REJECT pills), anchor history, validator activity. This
is where the auditor-grade surface lives — present, available, but not in the
way of buyers.

### 4.3 Deletions vs. the PDF design

For the buyer-facing surface specifically:

- "Anchor all final" CTA at the top — into Settings → Chain ops.
- "Add attester (advanced)" — into Settings.
- "Recent verdicts" feed above the fold — into the on-chain receipts drawer.
- "Mint missing NFTs · est. ~0.001 ETH" — into Settings → Chain ops.
- "Anchor history" table above the fold — into the drawer (full table in
  Settings).
- "Switch to Sepolia" — into Settings; global `[Testnet]` chrome elsewhere
  per the hard rule in the nav doc.

### 4.4 Additions vs. the PDF design

- **Clone count** + **Clone to edit** button on the identity hero (reuses
  existing app button).
- **`🤖 x402` badge** on listings that accept agent-paid purchase.
- **Single verification badge** (green / gray) hiding the attestation
  machinery behind one trust signal.
- **Ingredient check** running against the visitor's local XVN before
  purchase.
- **Buyer counter with human/agent split** as a first-class metric.
- **`@creator` handle linking to creator profile** (see §5).

### 4.5 Graphs

- Marketplace browse: sparklines only. No charts; lists need to scan fast.
- Identity above the fold: one equity curve. Optional small per-asset returns
  bar below.
- Lineage tree counts as a graph but isn't a chart.

---

## 5. Other pages

The PDF treats the marketplace as monolithic. A few pages are missing that
materially affect virality and trust.

### 5.1 Creator profile — `/marketplace/creator/<handle-or-address>`

Confirmed direction: **fully derived from chain.** No off-chain account, no
signup, no auth. Wallet IS the account. The profile is computed:

- All strategies minted by the address (`IdentityRegistry` events).
- Lineage tree across all their work (parent/child edges from those NFTs).
- Lifetime earnings (sum of `LicenseToken` purchases minus the platform fee).
- All attestations the creator has issued or received
  (`ValidationRegistry` + `ReputationRegistry`).
- Cloned-from edges (who they cloned from, who cloned from them).

Optional human-readable handle: tiny on-chain handle registry, or ENS, or the
xvn `IdentityRegistry`'s `agentURI` metadata field. Defer the choice to design
sprint. The address is always the fallback.

This is where the "OG creator" virality lives — people follow creators, not
strategies.

### 5.2 Leaderboard page — `/marketplace/leaderboard`

Same data as marketplace, different framing. Marketplace = browsing.
Leaderboard = competition. Curated slices with permanent URLs and **shareable
OG cards per slice** (per §3.5):

- All-time top earners.
- This week's agent-buyer favorites.
- Top with Claude Haiku 4.5.
- Top SOL · 7d.
- Most cloned this month.

The marketplace's filter chips generate ad-hoc leaderboards; this page
features the canonical slices. Curated, not algorithmic — a small set the
operator (or community) defines.

### 5.3 Public landing — `/`

The cold visitor from a tweet does not have an XVN install. Three sections:
buy and run, share and get paid, see proof on chain. Gen-art wall as
backdrop. Without this, viral traffic hits a wall.

Open question (covered in §6): where does `/` actually live, given XVN is
self-hosted and there is no central app website?

### 5.4 Purchase receipt — `/marketplace/receipts/<tx-hash>`

After purchase: gen-art card, license token NFT, install steps for the
buyer's XVN, share composer pre-loaded with "I just bought `btc-momentum-v3`
— running it now." Often skipped in marketplaces; this is where the viral
handoff usually dies.

### 5.5 Seller onboarding — `/marketplace/sell`

Three-step inline flow (not a modal, per no-popups rule):

1. Pick a strategy from your XVN install — gated by §5.5.1 publish checklist.
2. Choose **Tier A (Open)** or **Tier B (Sealed, paid)**, set price, choose
   accepted payers (humans / agents / both).
3. Mint.

Inline-expanded on the marketplace home from a `Share your strategy` CTA.

### 5.5.1 Publish checklist — what makes a strategy listable

Step 1 of §5.5 is not just an inventory picker; it's a gate. The strategy
must satisfy the publish checklist below before it can be listed. The
operative principle: **the bundle must be reproducible by the buyer with
only their own infrastructure** — no creator-hosted middleware, no private
endpoints, no bundled secrets, no closed-source dependencies. Without this
gate, "recipe not kitchen" (§6.3) becomes "recipe you can't actually cook."

Most checks run automatically inside the seller's XVN before the listing
reaches step 2. A few are creator-supplied metadata. Failures surface as
specific actionable errors, not generic "publish blocked."

#### A. Identity & metadata (creator-supplied)

- Strategy name, kebab-case, unique per creator. Scoped as `@creator/name` so
  two creators can both have `btc-momentum` without collision.
- Variant version (e.g. `v1.0`, `v2.3`). Auto-incremented when cloning your
  own; new variants under the same lineage NFT.
- One-line promise (≤120 chars).
- Description — what it does, when it works, when it doesn't.
- At least one each of: asset tag, model tag, style tag.

#### B. Bundle ingredients (the recipe)

Every ingredient must be machine-resolvable so the buyer's XVN can install
or connect to it without contacting the seller.

**Model**

| Field | Requirement |
|---|---|
| Provider | One of an allowlist: `anthropic`, `openai`, `google`, `azure-openai`, `aws-bedrock`, `xai`, `groq` (extends over time). |
| Model ID | Canonical name from that provider (e.g. `claude-haiku-4.5`). |
| Endpoint | Canonical provider endpoint. **No proxies, no private rewrappers, no creator-hosted middleware.** |
| Non-default params | Temperature, max_tokens, top_p, etc., declared if not provider default. |

**MCPs** — each MCP referenced must declare:

| Field | Requirement |
|---|---|
| Source URL | Public GitHub / GitLab / npm / pypi / equivalent. Resolvable on HEAD. |
| Version pin | Git ref, tag, or package version. No floating refs. |
| Env-var schema | Names of env vars the buyer must supply (e.g. `BIRDEYE_API_KEY`). Names only, never values. |

Private / internal MCPs block publish.

**Skills** — same fields as MCPs: public source URL + pinned version. Private skills block publish.

**Risk + universe**

- Max position size (% of portfolio or fixed USDC).
- Stop-loss / drawdown caps.
- Asset universe (explicit list or asset-class filter).
- Trade-frequency cap, if any (cycles/day).

**Broker compatibility**

- At least one supported broker: `alpaca-paper`, `alpaca-live`, `bybit-testnet`, `bybit-live`, etc.
- Minimum account balance recommendation (optional).

**Compatibility**

- Minimum XVN version (e.g. `xvn ≥ 2.0`).

#### C. Performance evidence (anti-fraud)

Summary stats are hashed and committed on-chain at mint, so they can't be
edited post-publish.

- Backtest manifest: date range, assets covered, return, Sharpe, max DD, win rate.
- Equity-curve data file (CSV or JSON, pinned to IPFS, hash on chain).
- Live-paper trading window: **≥30 days + ≥1 closed cycle with positive PnL hash on chain** for the green verification badge (§4.2). Less than that is still publishable but receives a gray badge.
- Trade-log summary (anonymized OK).

#### D. Public-source gates (hard pass/fail)

These are the rules that block publish entirely if violated.

- Every MCP resolves to a publicly-installable source (HEAD 200, valid package metadata).
- Every skill resolves to a publicly-installable source.
- Model endpoint is on the canonical-provider allowlist.
- No bundled secrets — secret scanner runs against the bundle before mint and rejects on any match for API-key, token, or `.env`-shaped strings.
- No private endpoints anywhere in the bundle.

Each failure produces a specific error pointing at the offending item with
the fix ("Skill `foo` resolves to a private repo — change visibility or use
a public fork before publishing").

#### E. Pricing & terms (creator-chosen)

- Tier A (Open, bundle public) **or** Tier B (Sealed, bundle encrypted).
- USDC price (Tier B only; ≥ $0.01).
- Accepted payers: humans / agents via x402 / both.
- License perpetual (V2 default; only option in V2).
- License transferable: default off for V2; opt-in per listing.

#### F. Pre-publish validations (XVN runs automatically)

| Check | Pass condition |
|---|---|
| Schema | Bundle JSON validates against the strategy schema. |
| Ingredient resolution | All MCP / skill URLs return HEAD 200; all package registries return metadata. |
| Secrets | Scanner finds no API keys, tokens, `.env`-shaped strings, or PII. |
| Determinism | Manifest hash is deterministic — same bundle → same hash, every time. |
| Ingredient list parseable | The required-ingredients list (the artifact the buyer's ingredient check consumes per §4.2) is well-formed. |
| Sandbox cycle (recommended) | One end-to-end cycle executes without error against the declared stack. Gates the green verification badge alongside the live-paper requirement. |

#### G. Auto-generated (no creator input)

- Generative-art SVG (from `agent_id + manifestHash` per §6.2 Tier 0).
- NFT `tokenURI` payload.
- Lineage parent edge if cloned via clone-to-edit.
- Creator wallet attribution.
- Mint timestamp.
- Manifest content hash.
- IPFS pin (Tier 1 metadata for all listings; Tier 2 sealed bundle for Tier B).

#### H. NOT required (anti-paranoia clarifications)

- Creator's broker account, API keys, real-world identity.
- Creator's journal, scratch, deleted-prompt history (Tier 3 per §6.2).
- Future-update commitment — publish a v2 variant if you iterate, or don't.
- Human reviewer or approval step.
- Buyer wallet info — buyers bring their own at purchase.

#### I. Update / revoke

- Updates are new variants (`v2.0`, `v2.1`, …) minted under the same lineage NFT. The old listing remains live until the seller revokes it.
- Revoke via the `revokeListing` verb in the surface spec — blocks new purchases; existing license-holders keep access.
- **License compatibility note.** The strategy bundle inherits the license terms of its declared MCPs and skills. Creator is responsible for ensuring they're allowed to sell a derivative. The classic trap: a GPL skill bundled into a paid Tier B listing. XVN surfaces a warning at mint time but doesn't block — license interpretation is the creator's call.

#### J. Second-order notes

- **Naming collisions** — scope per creator (`@alex/btc-momentum`). Two creators can both list `btc-momentum` without conflict.
- **Anti-spam** — mint costs gas (Mantle is cheap but non-zero). Natural anti-spam; no minimum stake needed unless spam materializes.
- **Plagiarism / re-list of a cloned bundle without changes** — the lineage tree exposes parent edges on the identity page, so a clone-without-changes is visually obvious. Don't add a "non-trivial fork required" rule until it becomes a problem.
- **Things explicitly rejected** — no creator-written "trade rationale" or "why this works" prose field. Those degrade to AI-spam within months. The performance evidence + lineage history is the rationale.

### 5.6 (Later) Creator earnings dashboard

Inside the self-hosted XVN, not on the public viewer. The chain-native
notification stream (§3.3) is more important than the page itself.

### 5.7 Pages explicitly NOT added

- Standalone "Attestations" page — no. Drawer on identity.
- Standalone "Anchor history" page — no. Drawer on identity; full table in
  Settings → Chain ops inside XVN.
- Standalone "Operator" surface in the public marketplace — no. Operator
  surfaces live in the self-hosted XVN's Settings.

---

## 6. Infrastructure — public/private split, IPFS, and the website question

### 6.1 The website question (Edward's pushback)

> *"I don't have a website for the app, it's self hosted. So need to rethink
> the IPFS aspect."*

The original proposal mirrored IPFS metadata to `xvision.dev/m/<cid>` HTTPS
routes for OG-card rendering. That assumed a central web property exists.
It doesn't, and the operating principle ("only unified infrastructure is the
blockchain") cuts against creating one.

The architectural reframe:

**The chain is the canonical backend. A thin, read-only viewer is required
*somewhere* for the marketplace to be viable as a viral surface, but the
viewer is stateless and forkable, not a central platform.**

Three placement options, with trade-offs:

| Option | What it is | Trade-off |
|---|---|---|
| **A. Public-only reference viewer** | Operator hosts one read-only deployment at a public domain (e.g. `xvn.market`); the same codebase is also embedded inside the self-hosted XVN's Marketplace tab. Anyone can fork and host their own. | Requires provisioning ONE domain (small ops cost). Best UX for cold visitors, OG cards, GEO/AI crawlers. |
| **B. No public viewer; IPFS gateways only** | Shareable URLs resolve to e.g. `https://w3s.link/ipfs/<cid>`. No central domain. | OG cards still work on some scrapers, fail on others. URLs look like cryptojunk. Worse virality but maximum decentralization. |
| **C. No public viewer; chain explorer as canonical** | Shareable URLs resolve to Mantlescan. | Bad UX. Looks like Etherscan, not a marketplace. Effectively kills virality. Mentioned for completeness only. |

**Recommendation: Option A,** framed as part of the blockchain infrastructure
rather than as a SaaS app. The viewer:

- Reads Mantle + IPFS only. Holds no user data. Touches no funds.
- Provides marketplace browse, identity pages, leaderboards, creator profiles,
  receipts.
- Renders OG cards server-side (Astro or Next.js for the public surface).
- Is fully forkable. The operator runs the canonical instance; community can
  run mirrors. Architecturally identical to Mantlescan vs. other block
  explorers — there's a canonical one, others are valid.
- Is embedded as the Marketplace tab inside self-hosted XVN, so authenticated
  users browse without leaving their app.

The cost is one domain (`xvn.market`, `xvision.dev`, or another — decision
deferred). The benefit is the entire viral loop in §3 actually works.

If Edward declines to provision a domain at all, fall back to Option B and
accept that virality will be capped by ugly IPFS-gateway URLs and inconsistent
OG-card rendering. This is a real choice with real consequences.

### 6.2 Storage tiers

Four tiers, not the spec's implied two:

**Tier 0 — On-chain, immutable, free to read forever.**

- NFT itself (`agentNftId`, owner).
- Manifest hash (blake3 of the strategy bundle).
- Lineage edges (parent → child, including clone edges).
- ERC-8004 reputation and validation receipts.
- Buyer-count events (`LicenseToken` mints).
- **Generative art:** deterministic SVG from `agent_id + manifestHash`,
  embedded as `data:` URI in `tokenURI`. The brand-critical surface has zero
  external dependencies. This matches Phase 4 of the nav doc; lock it in.

**Tier 1 — Public, free to read, free to copy. IPFS-pinned.**

The marketing-copy tier. Everything a buyer needs to decide whether to buy:

- Strategy name, description, creator handle.
- Performance summary (Sharpe, return, win rate, max DD) — **with hash
  committed on-chain** so creators can't fake numbers retroactively.
- Asset tags, model tags, style tags.
- Equity-curve data (CSV / JSON).
- Lineage human-readable metadata.
- **Required-ingredients list:** what model, what MCPs, what skills the buyer
  needs installed. This drives the ingredient check in §4.2.
- License terms.
- Rating receipts.

Pinned via Pinata (V2) or web3.storage. The public viewer (§6.1) fetches via
HTTPS gateway. No need to mirror to operator-owned HTTPS because the viewer
itself is the HTTPS surface that scrapers see.

**Tier 2 — Sealed bundle, paywalled (Tier B listings only).**

The actual strategy content:

- Full prompt(s).
- Exact agent topology and ordering.
- Threshold values, rule bodies.
- Exact MCP/skill configurations.
- Any creator-shipped notes.

Storage: encrypted client-side, IPFS-pinned (anyone can pin; only buyers can
decrypt). Decryption gated by a small relay that verifies
`LicenseToken.balanceOf(buyer) >= 1` and signs a decryption authorization.

Centralization assessment: the relay can frustrate purchases (refuse to sign)
but cannot fake purchases (can't mint LicenseTokens). Acceptable for V2.
Migrate to Lit / TACo / threshold encryption when justified. Don't optimize
this on day one.

Tier A "open" listings skip encryption and the relay. Get an `Open` badge.

**Tier 3 — Never shared.**

Creator's local journal, research scratch, broker account, identity,
proprietary data feeds, deleted-prompt history. Stays in the creator's
self-hosted XVN. Creators may choose to publish blog posts linked from the
strategy page; Tier 3 is never bundled.

### 6.2.1 Pinning architecture — install-mesh primary, viewer gateway, paid backstop

Locked 2026-05-26. **Three tiers of IPFS pinning, not one.** Each tier
covers a failure mode the others don't.

**1. Install-mesh (primary).** Every XVN install embeds an `iroh` node and
pins the content it cares about by default: its own published listings
(Tier 1 metadata + Tier 2 sealed bundles for what it sells), its active
licenses (Tier 2 bundles for what it bought). Pinning is a byproduct of
using the app — sellers want their listings live, buyers want their
purchases retrievable. Zero altruism required for the default behavior.

Optional opt-in (Settings toggle): "help host the network" — pin popular
public content (high-buyer-count listings, agent-bought favorites).
Configurable disk + bandwidth caps.

Privacy: pin sets are observable on the DHT, so "node X serves CID Y"
weakly reveals "X owns license to Y." For Tier 2 this is a marginal leak
above what the on-chain `LicenseToken` already discloses, but it's real.
Default behavior: pin own listings (always), pin own licenses (always),
"help host the network" off by default with onboarding disclosure when
toggled on.

**2. Viewer gateway.** The public viewer (§6.1) runs its own IPFS gateway
with a pin set covering everything the marketplace browse / identity /
leaderboard surfaces need to render. Sub-second resolution for cold visitors,
OG-card scrapers, and AI crawlers comes from here. The install-mesh is a
swarm of laptops that close at night — it cannot deliver sub-second OG
cards to X's scraper, and the viewer must.

**3. Paid backstop ("node of last resort").** Pinata or web3.storage
pinning service holding all of Tier 1 (cheap, KB-scale) and a capped budget
of popular Tier 2 listings. Operator-paid out of pocket as a regular
subscription — at low scale this is single-digit dollars per month.
**No auto-funded treasury, no fee splits, no on-chain routing.** Marketplace
fees go straight to the operator's wallet (per §6.4 / §7); paying for the
backstop is just an operating expense the operator chooses to incur. Once
install-mesh density makes the backstop redundant, the subscription can
shrink or drop.

The three tiers are complementary, not redundant. Install-mesh provides
real decentralization for popular content + creator skin-in-the-game.
Viewer gateway provides cold-traffic speed. Paid backstop covers the
cold-start period and the long tail of unpopular content.

#### Library choice

- **Rust engine side:** [`iroh`](https://www.iroh.computer/) — embedded-
  friendly, CID-compatible, written for "ship inside an app" use cases by
  Number Zero (the ex-IPFS core team). The historical "embedded IPFS is
  painful" reputation comes from kubo / go-ipfs and does not apply to iroh.
- **Browser side** (dashboard SPA + public viewer): [`Helia`](https://helia.io/)
  for Tier 1 metadata reads. Mature enough for production now.

Wrap behind an `IpfsStore` trait in `xvision-marketplace` (same pattern the
plugin spec's `AnchorDriver` uses). V2 may ship a single `PinataDriver`
implementation to land the basic pin/unpin operations and unblock the
backstop tier; V3 swaps in `IrohDriver` without touching call sites and
turns the install-mesh tier on.

#### Why this fits XVN specifically

Embedded-IPFS-in-the-app has failed in consumer products (Brave's IPFS
support, IPFS Companion) because those users had no reason to host other
people's content. XVN's user base self-selects for operator-tolerance:
someone willing to self-host a Rust trading engine + Vite SPA + Mantle
wallet is exactly the kind of user who'll accept "and your install helps
host other people's strategies." The social fit is genuinely better than
consumer apps. This doesn't make it free — NAT traversal, residential
bandwidth, and ISP / ToS exposure remain real — but it makes the value
proposition coherent rather than aspirational.

#### Cold-start expectation

Until the install base hits roughly **100–200 active nodes**, the
install-mesh is unreliable on its own and the paid backstop is
load-bearing. Past that threshold, popular content drifts toward the
install-mesh and the backstop becomes a tail-content insurance policy.
Set the launch expectation accordingly: V2 ships with the paid backstop
doing most of the work; install-mesh contribution scales as the user base
grows.

#### Legal / ToS surface

Users hosting other people's content on residential connections face minor
ToS risk with ISPs and minor legal exposure if hosted content turns out to
be problematic. Strategies are abstract algorithm bundles, not media, so
the content-risk profile is materially lower than file-sharing platforms.
Mitigation: default to "pin own stuff only"; "help host the network" is
opt-in with onboarding disclosure of what turns on.

### 6.3 The public/private balance — the operative framing

Public on the listing page (Tier 1):

- Performance (with on-chain hash commitments).
- Asset class, model, style tags.
- Required ingredients: "Uses Claude Haiku 4.5 + Birdeye MCP + the SOL
  Strategist skill. Holds 3 days avg. Risk caps: 2% per trade."
- Hash of the prompt + config (committed on-chain so the creator can't
  swap content later under the same listing).

Sealed until purchase (Tier 2):

- Actual prompt text.
- Exact agent topology values.
- Threshold values, rule bodies.

User-facing framing on the listing page header:

> **You're paying for the recipe. You're not paying for the kitchen.**
> This strategy uses Claude Haiku 4.5 + Birdeye MCP + the SOL Strategist
> skill. Make sure those are installed in your XVN. Once you buy, you get
> the prompts and rules to plug in.

The ingredient check on the identity page operationalizes this: the visitor
sees whether they have the ingredients *before* paying. If not, the system
tells them what's missing and links to the plugin/MCP marketplace to install.
This is the single biggest refund-risk mitigation available, and it doubles
as upsell to the plugin ecosystem.

### 6.4 Purchase model — fixed-fee, perpetual

Confirmed direction: **fixed USDC price, perpetual license, no subscription.**
One purchase = use the strategy forever in your XVN install. License is held
as a non-transferable (or transferable, TBD) ERC-721 token.

Streamable / subscription pricing deferred to a later phase. Mentioned in
Edward's notes as a possible V3+ direction.

This makes the listing UX simple: one price, one button, no plan picker.

### 6.5 Phase 3 infrastructure picks (carried from nav doc §4.C)

- **IPFS pinning (C1):** Three-tier per §6.2.1 — `iroh`-embedded install-mesh as the primary pinning tier; viewer gateway for cold-traffic speed; Pinata (or equivalent paid pin) as the backstop "node of last resort," funded from marketplace fees. Library: `iroh` Rust-side, `Helia` browser-side, wrapped in an `IpfsStore` trait so a V2 single-driver build (Pinata-only) can ship before the install-mesh tier lights up at V3.
- **Subgraph (C2):** Goldsky or The Graph hosted. Don't self-host; correctness of marketplace browse depends on the indexer being right.
- **Domain (C3):** decision required per §6.1. `xvn.market` and `xvision.dev` are both reasonable candidates. Provision before Phase 4.
- **EAS vs bespoke (C4):** unchanged from nav doc — bespoke unless EAS canonical deployment lands on Mantle by Phase 5.
- **Audit firm (C5):** unchanged from nav doc — engage during Phase 0.
- **Faucet ops (C6):** unchanged from nav doc.

---

## 7. Decisions from the 2026-05-26 chat

Captured as decisions for §4 (open questions) in the nav doc.

| # | Question (from nav doc) | Decision |
|---|---|---|
| A1 | Fee surfacing | Listing page says "$X paid to `@creator` · 5% platform fee." Buyer sees creator-net + fee inline, not as a separate line. |
| A1 | Tier A vs Tier B presentation | Tier A badged `Open` with a `Run free` button. Tier B shows USDC price with a `Buy` button. Same row layout otherwise. |
| A1 | x402 surfacing | `🤖 x402` badge on listings that accept agent-paid purchase. Buyer count shows humans + agents split everywhere. |
| A1 | Free-listing UX (`priceUSDC=0`) | Same as Tier A `Open` — `Run free` button. Not differentiated from Tier A in UI. |
| A1 | Transferable licenses | Deferred. Default non-transferable for V2; revisit when streamable pricing lands. |
| A2 | Identity page scope | Public; `/marketplace/lineage/<name>`. Contents per §4.2. Auditor view in drawer. |
| A2 | Operator identity page | Separate; lives in self-hosted XVN's Settings, not on the public viewer. |
| A2 | agent #0 (xvn) page | Same template as any creator profile (§5.1), at `/marketplace/creator/xvn`. |
| A3 | Generative art approach | Deterministic SVG from `agent_id + manifestHash`, embedded `data:` URI in `tokenURI`. Per-variant art, lineage-coherent palette. Evolution-with-reputation deferred. |
| A4 | Lineage NFT vs strategy NFT | **One NFT per lineage** (marketplace-plugin spec position wins). Variants are content-hash records under the lineage NFT. Reconcile [`smart-contract-surface-design.md`](../specs/2026-05-08-smart-contract-surface-design.md) before Phase 3. |
| A5 | Wallet-connect | Deferred; design sprint decision. Privy-embedded vs WalletConnect vs both. Browse remains walletless. |

New decisions not in the nav doc:

| Topic | Decision |
|---|---|
| Custody | Non-custodial. Buyers pay sellers directly via on-chain marketplace contract. Platform extracts a small fee at settlement. |
| Pricing model | Fixed USDC, perpetual license. Streamable pricing deferred. |
| App business model | XVN dashboard + engine + CLI are free. Marketplace fee is the only revenue stream. |
| Notifications | Chain-native — self-hosted XVN listens for `LicenseToken` mint events on its own address and surfaces "+$X from @buyer" in-app. Share composer pre-loaded with gen-art card. |
| Fork mechanic | Reuse existing "Clone to edit" button. Creates a parent edge in the on-chain lineage tree. Cloning a Tier B listing requires purchase first. |
| Creator profile data source | Fully derived from chain. No off-chain account. Wallet = account. |
| Leaderboard slices | Each slice has a stable URL and a server-rendered OG card. Curators (initially the operator) define canonical slices over time. |
| Public viewer | Required (Option A in §6.1). Reference deployment on a single domain (TBD). Forkable, stateless. Embedded inside self-hosted XVN as the Marketplace tab. |
| Pinning architecture | Three-tier per §6.2.1: install-mesh (every XVN install embeds an `iroh` node, default-pins its own listings + licenses) + viewer gateway (cold-traffic speed) + paid backstop (Pinata or equivalent, operator-paid out of pocket — no treasury contract, no fee splits). Library: `iroh` Rust + `Helia` browser, behind an `IpfsStore` trait. |
| Fee model | **Flat commission to a single operator wallet.** Existing `Marketplace.setFeeRecipient(address)` is sufficient. No fee splits, no auto-routing, no treasury contract. Keep it simple. |
| Sub-routes | Match existing app convention: `/marketplace`, `/marketplace/lineage/<name>`, `/marketplace/creator/<handle>`, `/marketplace/leaderboard`, `/marketplace/sell`, `/marketplace/receipts/<tx>`. |

---

## 8. Open questions remaining

To resolve in the Phase 1 design sprint:

1. **Domain pick.** `xvn.market` vs `xvision.dev` vs other. Provisioning lead time matters for Phase 4 launch.
2. **Handle system.** ENS, on-chain handle registry, or address-only with optional `agentURI` display-name field?
3. **Wallet-connect UX.** Privy embedded vs WalletConnect vs MetaMask-direct vs all three. Affects onboarding friction for the vibetrader audience (likely no wallet today).
4. **Cloning a sealed (Tier B) listing.** Confirmed direction: requires purchase. Lock the contract semantics so the lineage edge is only writable after `LicenseToken` mint.
5. **Verification badge thresholds.** What qualifies a strategy for the green check vs the gray one. Suggest: green = backtested + ≥30 days of live-paper data + at least one closed cycle with positive PnL hash on chain. Lock in design sprint.
6. **Notification feed UX inside self-hosted XVN.** Toast? Sidebar? Both? Where does the share composer live?
7. **OG card rendering.** Astro vs Next.js vs custom SSR layer. Decide alongside viewer codebase choice.
8. **Leaderboard slice curation.** Operator-defined for V2; community-proposed later? Out-of-scope for design sprint, but flag.
9. **Install-mesh defaults.** Default pin sets per §6.2.1 (own listings + own licenses always; "help host the network" opt-in). Disk + bandwidth caps. NAT traversal strategy (libp2p hole-punching, optional relays). Onboarding disclosure copy for the opt-in toggle. Decide alongside Phase 5 contracts when `xvision-marketplace` gains the `IpfsStore` trait.
10. **`iroh` integration timing.** Two options: (a) V2 ships with only a `PinataDriver` implementation of `IpfsStore` and swaps in `IrohDriver` at V3 once the install base justifies it; (b) V2 ships with `iroh` from day one. Recommendation: (a) — Pinata-driver V2 unblocks the backstop tier immediately and the `IpfsStore` trait makes the V3 swap mechanical. Confirm in Phase 5 planning.

---

## 9. Hard rules carried forward

From [`CLAUDE.md`](../../../CLAUDE.md):

- **No popups.** Marketplace and identity surfaces use routes, rails,
  accordions, drawers. The on-chain receipts drawer inline-expands; the
  ingredient check inline-renders; the seller flow is a three-step inline
  expansion of the marketplace home. No `Dialog`, `Modal`, `Sheet`, or
  `Popover`.
- **Testnet labeling** wherever a chain action appears. Global `[Testnet]`
  chrome until V4 cutover.

From the nav doc:

- **No timelock on testnet.** UUPS proxy admin = operator EOA through V2.
- **Same CREATE2 salts on testnet and mainnet.** Predictable addresses from
  Phase 3 onward.
- **Foundry deploys never on small VPS / Coolify nodes.**

---

## 10. Next steps (feeds Phase 1 design sprint)

1. **Mockup pass** on the four core surfaces, using the column/region
   specs in §4 and §5:
   - Marketplace browse (`/marketplace`).
   - Identity page (`/marketplace/lineage/<name>`).
   - Creator profile (`/marketplace/creator/<handle>`).
   - Leaderboard (`/marketplace/leaderboard`).
2. **Metadata spec + bundle schema.** Driven by §5.5.1 publish checklist — the schema must enforce the public-source gates (§5.5.1 D) and ingredient-resolution requirements (§5.5.1 B). One-pager listing the fields in:
   - The NFT `tokenURI` JSON (Tier 0).
   - The IPFS-pinned Tier 1 metadata JSON (including the required-ingredients list consumed by the buyer-side ingredient check in §4.2).
   - The Tier 2 sealed bundle.
   - The events the chain emits and the subgraph indexes for marketplace
     browse + creator profile.
3. **Domain decision** (§6.1).
4. **Gen-art algorithm.** Locked deterministic-SVG approach (per §6.2 Tier 0).
   Lock the input space (`agent_id + manifestHash + lineage_root`?), the
   palette mapping, and how variants visually relate. Sample wall of 200 to
   confirm it doesn't look like garbage at scale.
5. **Reconcile lineage NFT vs strategy NFT in `smart-contract-surface-design.md`**
   (decision A4) before Phase 3 starts. Every downstream contract is keyed on
   `agentNftId`.
6. **Notification system spike.** Smallest possible prototype: self-hosted XVN
   listens for `LicenseToken` Transfer events on its own address and pops a
   toast. Validates the chain-native notification primitive before building
   the share composer on top.

---

## 11. Maintenance

- Update §7 (Decisions) as new design decisions land.
- Update §8 (Open questions) → §7 (Decisions) as questions resolve.
- When Phase 1 design sprint produces locked mockups, link them here and
  retire §4 / §5 as the source of truth (mockups become canonical, this doc
  becomes archival).
- Don't re-litigate decisions logged here in the linked specs; this doc and
  those specs hold non-overlapping authority (this = direction + UX; specs =
  contract surface + data model).
