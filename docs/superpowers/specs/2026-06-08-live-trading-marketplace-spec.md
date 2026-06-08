# Live Trading Page + Marketplace — Design Spec

> **Date:** 2026-06-08
> **Status:** Approved — derived from operator grilling session, all decisions confirmed
> **Related specs:**
> - `2026-05-26-marketplace-phase-f-frontend-design.md`
> - `2026-05-26-marketplace-phase1-metadata-data-contract.md`
> - `2026-05-09-marketplace-plugin-design.md`
> **Related audit:** `2026-06-08-master-implementation-list.md`

---

## 1. Scope

This spec covers two surfaces and their overlap:

1. **Live Trading page** — the operator cockpit for managing real-money strategy deployments
2. **Marketplace** — browse, buy, sell, and attest AI trading strategies as on-chain assets
3. **Overlap** — how live trading performance feeds back to marketplace listings and how purchased strategies deploy live

---

## 2. Live Trading Page

### 2.1 Name and route

Page name: **Live Trading**
Route: `/live` (existing, replaces current minimal implementation)
Sidebar label: "Live Trading"

### 2.2 Design philosophy

Control-first, NASA cockpit. The operator comes here to act, not observe. Every piece of information has a fixed spatial home — operators build muscle memory. No surprises, no layout shifts, no mode-switching. The chat rail constraint (no right sidebar) applies; the cockpit layout uses the full center column.

### 2.3 Layout

```
┌─────────────────────────────────────────────────────────┐
│  STRATEGY STRIP (fixed, top)                            │
│  [BTC Momentum ▲ +2.1% ●] [ETH Mean Rev ▼ -0.4% ●] …  │
│  [column picker ⚙]                                      │
├─────────────────────────────────────────────────────────┤
│  WALLET BANNER (conditional — wallet not connected)     │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  VIEWPORT (selected strategy)                           │
│                                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Live chart (candles + equity + decision markers) │  │
│  └───────────────────────────────────────────────────┘  │
│  ┌─────────┬──────────┬──────────────┬───────────────┐  │
│  │ Equity  │ Daily PnL│ Drawdown peak│ Unrealized PnL│  │
│  └─────────┴──────────┴──────────────┴───────────────┘  │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Active positions table                           │  │
│  │  Symbol · Entry · Qty · Entry time · Unreal PnL  │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
└─────────────────────────────────────────────────────────┘
```

Single scroll, no tabs. Chart dominates vertical space. Account stat strip and positions table below, full-width inline.

### 2.4 Strategy strip

A fixed horizontal strip at the top of the page. Each pill shows a configurable set of per-strategy metrics. The strip does not scroll the rest of the page — it is always visible.

**Pill minimum content (always shown):**
- Strategy name
- Status pill: `ACTIVE` / `PAUSED` / `STOPPED`
- One configurable metric (default: daily PnL $, color-coded)
- Connection dot (SSE stream health: green / amber / red)

**Configurable metrics (column picker):**
Operator selects which metric appears in the strip pill. Options include: trades today, decisions today, run time, sharpe, max drawdown, daily PnL %, unrealized PnL, current equity. Preference persisted to `localStorage` via `safeStorageGet/Set` from `lib/storage.ts`. Key: `live_trading_strip_metric`.

**Selection:** clicking a pill selects that strategy and updates the viewport. Auto-selects the most recently started strategy on page load.

**Transport controls:** visible on each pill on hover — ⏸ Pause / ⏹ Stop. No play button shown for ACTIVE strategies; a ▶ Resume appears when PAUSED.

**"Deploy strategy →"** link at the right end of the strip navigates to `/strategies` (strategy launch lives on the strategy detail page, not here).

### 2.5 Wallet banner

Shown when `useWallet().address === null`. Full-width strip above the viewport, below the strategy strip. Shows strategy data regardless — data is never hidden. Only actions are disabled (pause, stop, resume buttons rendered but disabled with tooltip "Connect wallet to act").

Banner copy: "Wallet not connected — trading actions disabled. [Connect wallet]"

Does not use `SafetyPauseBanner` — separate component.

### 2.6 Viewport

**Chart:** `LiveChartV2Container` (existing). Real-time candles + equity curve + decision markers via SSE. SSE connection status badge stays inside the chart surface (existing behavior). Follow/Freeze/Resume live toggle stays.

**Account stat strip:** four stats in a horizontal row, full-width, compact height.

| Stat | Source | Notes |
|---|---|---|
| Current equity | Derived from equity curve stream | Latest `equity_usd` point |
| Daily PnL | Current equity minus equity at midnight UTC | $ and % both shown |
| Drawdown from peak | Max equity seen minus current equity | % only |
| Unrealized PnL | Sum of open position `pnl_realized` pending fills | From run stream |

**Active positions table:** columns — Symbol, Entry price, Qty, Entry time, Current value, Unrealized PnL, % gain/loss. Derived from `DecisionRowDto` open positions (buys without corresponding close). No pagination for v1 — strategies are expected to hold few concurrent positions.

### 2.7 Transport controls

#### Stop

- Trigger: ⏹ button in strategy pill (hover) or in viewport topbar
- Behavior: close all open positions at market, then terminate run via `POST /api/eval/runs/:id/cancel`
- Confirmation: type-to-confirm using `HaltStrategyButton` pattern (type strategy name). No modal — inline expand on the pill.
- Result: strategy pill status → `STOPPED`, viewport shows final state

#### Pause

- Trigger: ⏸ button in strategy pill (hover) or in viewport topbar
- Behavior: halt LLM decision dispatch. Open positions remain open (hold positions default).
- Confirmation: single click, no type-to-confirm (reversible action)
- On pause, an inline expand appears on the pill: "Positions held. [Flatten positions] [Keep open]" — flatten is explicit secondary action
- **Backend blocker:** requires a `paused` flag on the run record, checked in the eval loop before each decision dispatch. This is a hard blocker for shipping the Live Trading page. Per-strategy pause does NOT exist today — `SafetyManager` pause is global only.

#### Resume

- Trigger: ▶ button visible when strategy is PAUSED
- Behavior: re-enable LLM decision dispatch
- No confirmation required

#### Global pause (existing)

The existing `SafetyPauseBanner` + `SafetyPauseBadge` system remains unchanged. Global pause halts all broker submits. It coexists with per-strategy pause — both can be active simultaneously.

### 2.8 Multi-strategy behavior

**Portfolio header (future v2):** aggregate PnL, total capital deployed, net exposure by asset. Not in v1.

**Net exposure:** not surfaced in v1. Documented as a known gap — two strategies holding opposite positions in the same asset will not show the net.

**Capital allocation:** not in v1.

**Per-strategy isolation:** each strategy pill is fully independent. No cross-strategy data in v1 viewport.

### 2.9 Connection health

**SSE stream:** existing `ConnectionStatus` badge inside `LiveChartV2Container`. No changes.

**Broker/wallet connection:** not surfaced as a health indicator. Wallet is connected or not (binary). Network outages surface as SSE stream failures. No additional broker health UI.

### 2.10 Relationship to existing surfaces

**`LiveStrategiesSection` on home:** replaced with a compact summary strip. Shows total active strategies count, aggregate daily PnL (if available), and a "Go to Live Trading →" CTA. The full cockpit lives at `/live` only. Item 8.14 from the audit applies — copy rewrite required.

**`/live/:id` (current):** the existing minimal page (`<LiveChartV2Container>`) is superseded by this spec. The detail view is now the viewport within the cockpit, not a standalone route. Route `/live/:id` can remain as a direct-link deep-link that opens the cockpit with that strategy pre-selected.

### 2.11 Strategy launch

Strategy launch (configure capital, broker, stop policy, deploy) lives on `/strategies/:id`. The Live Trading page has no launch UI. The "Deploy strategy →" link in the strip navigates to `/strategies`.

### 2.12 Backend blockers (must ship before page)

| Blocker | Description |
|---|---|
| **Per-strategy pause** | `paused: bool` field on run record, checked in eval loop before dispatch. New Rust field + DB migration + API route `POST /api/eval/runs/:id/pause` and `/resume`. |
| **Stop + close positions** | Current `cancel` endpoint terminates the run. Needs to close open broker positions before terminating. Requires broker-side order submission at cancel time. |

---

## 3. Marketplace

### 3.1 Token stack

| Layer | Standard | Purpose |
|---|---|---|
| Agent identity | ERC-8004 Identity Registry | Each strategy = an on-chain AI agent with `agentId` |
| Reputation | ERC-8004 Reputation Registry | Automated performance attestations via `giveFeedback` |
| Validation | ERC-8004 Validation Registry | Listing validation before publish |
| License token | ERC-1155 | Purchase license — one token type per listing, unlimited supply |
| Payment | x402 (existing `acceptsX402` flag) | Native payment flow |
| Storage | IPFS | Encrypted strategy bundles (Tier B) |

ERC-8004 went live on Ethereum mainnet 2026-01-29. The existing `agentURI`, `manifestHash`, `operatorSig`, and `acceptsX402` fields in `OnChainReceipts` are already aligned with ERC-8004.

### 3.2 Tiers

| Tier | Label | Price | Bundle | What buyer sees before purchase |
|---|---|---|---|---|
| A | `"open"` | Free (`priceUsdc: null`) or paid | Unencrypted | Full strategy internals — agent config, prompt previews, full metric set |
| B | `"sealed"` | Paid (USDC) | IPFS-encrypted, decrypted on purchase | Performance metrics + ingredients list only |

### 3.3 Listing flow (sell)

Existing 3-step `SellRoute`: Step1PickStrategy → Step2Configure → Step3Preview.

**Mint model:** lazy mint. Creator signs the listing at publish time (committing to token ID and terms). Gas is paid by the buyer on first purchase. `manifestHash` + `operatorSig` implement the creator's pre-commitment.

**Validation gate:** before a listing can go live, a Validation Registry request is submitted. A trusted xvision validator signs off on the claimed backtest metrics against the anchored run data. On approval, `VerifiedBadge` is trustlessly assigned. zkML / TEE validation deferred to v2.

**Listing cannot publish without:**
1. At least one completed backtest run anchored on-chain
2. Validation Registry response with `response >= 70` (pass threshold, platform-fixed)
3. Creator wallet connected and ERC-8004 agent registered for the strategy

### 3.4 Purchase and install flow

Existing `InstallSteps` (4 steps: XVN detected → Decrypt sealed bundle → Install missing ingredients → Add to Strategies). No changes to step structure.

**Express deploy:** after step 4 completes, a "Deploy live →" button appears on the receipt page. Navigates directly to the live config form for the purchased strategy (bypasses strategy detail page). A soft gate warning inline: "No backtest on your instance yet — recommended before going live. [Deploy anyway] [Run backtest first]". Not blocking.

**Soft gate trigger:** shown if zero completed backtest runs exist for the purchased strategy in the operator's local instance.

### 3.5 Purchased strategies in the strategies list

- Appear in the main `/strategies` list alongside operator-built strategies — unified treatment
- Tagged with a gold "Marketplace" pill on the row
- Row shows creator handle/ENS/truncated address as a sub-label
- Filterable: strategies list gains a "Source" filter — "All / Mine / Marketplace"
- Clicking the creator label navigates to `/marketplace/creator/:handleOrAddr`
- Clicking the Marketplace pill navigates back to the listing

### 3.6 Attestation system

**Who attests:** automated only. No manual review UI. Attestation is computed from on-chain trade data.

**Gate:** only ERC-1155 license holders can submit feedback to the Reputation Registry for a listing. Enforced on-chain: `balanceOf(clientAddress, tokenId) > 0` checked before `giveFeedback` is accepted.

**Trigger:** attestation fires after every 20 completed trades in the operator's live deployment of the strategy. Re-fires every 20 trades thereafter.

**Metric:** Sharpe ratio delta between the operator's live run and the listing's claimed sharpe.

**Verdict mapping (platform-fixed thresholds):**

| Condition | ERC-8004 tag1 | Feedback value | Verdict label |
|---|---|---|---|
| Buyer sharpe within 20% of listed sharpe | `tradingYield` | `100` | Endorses |
| Buyer sharpe 20–50% below listed | `tradingYield` | `50` | Questions |
| Buyer sharpe >50% below, or net negative when listed positive | `tradingYield` | `0` | Rejects |

`tag2` = `month` (rolling 20-trade window approximation).

**Effect on listing:**
- Endorsements boost ranking on "trending" sort
- A listing with 3+ verified rejections gets a visible warning badge in browse
- Rejections never auto-hide a listing — informational only
- Listing shows most recent attestation date and total attestation count

### 3.7 Live equity curve

**Source:** on-chain anchored trades only. Requires blockchain provenance. The `"live"` phase of `EquityCurve` is not self-reported — it is derived from `OnChainReceipts.trades` (anchored via Merkle / mint / commit anchors).

**Backtest phase:** creator's local run data. Not on-chain, not verified. Clearly labeled "Backtest (unverified)" in the UI.

**Buyer contributions:** by default, a buyer's anchored live trades contribute to the listing's aggregate live equity curve. Opt-out available at purchase time ("Don't share my results"). Wallet address visible — standard web3 transparency, on-chain reality.

**Aggregation:** median equity curve across all contributing buyers + creator. Buyers who opt out are excluded. Attestation revocations (`revokeFeedback`) remove that buyer's data from the aggregate on next recompute.

### 3.8 Revenue split

| Event | Creator | Platform |
|---|---|---|
| Primary sale | 90% | 10% |
| Secondary sale (resale between buyers) | 5% royalty (ERC-2981) | — |
| Clone/fork sale | 10% upstream to original creator | — |

`clonesOfYours: { count, upstreamEarningsUsd }` already modelled in `ListingDetail`. `transferableLicense: boolean` gates secondary market eligibility.

**Note on supply:** ERC-1155 licenses are infinite supply — the creator can always sell another copy at the same price. Secondary market value depends on creator stopping sales, version locking, or scarcity imposed by the creator at listing time.

### 3.9 Identity — ERC-8004 registration

Each strategy listed on the marketplace registers as an ERC-8004 agent:

```json
{
  "type": "https://eips.ethereum.org/EIPS/eip-8004#registration-v1",
  "name": "<strategy display name>",
  "description": "<listing promise field>",
  "image": "<genArtSeed rendered>",
  "services": [
    { "name": "xvn", "endpoint": "<operator xvn endpoint if shared>" }
  ],
  "x402Support": true,
  "supportedTrust": ["reputation", "validation"]
}
```

`agentId` (ERC-8004 tokenId) maps to `agent_id` (ULID) in xvision's terminology. Post-mint, the NFT token ID becomes the marketplace's canonical identifier.

---

## 4. Overlap — Live Trading ↔ Marketplace

### 4.1 Purchased strategy → live deployment

```
Marketplace listing
  → Buy (ERC-1155 mint to buyer, IPFS bundle decrypted)
  → InstallSteps (XVN detected → decrypt → install ingredients → add to strategies)
  → Express deploy CTA on receipt (soft gate if no backtest)
  → Live config form (capital, stop policy)
  → Strategy appears in Live Trading cockpit strip
```

### 4.2 Live performance → marketplace listing

```
Strategy running live (Live Trading cockpit)
  → Trades execute and anchor on-chain every N bars
  → After 20 completed trades: attestation computed
  → ERC-1155 ownership checked (buyer must hold token)
  → giveFeedback(agentId, sharpeValue, 0, "tradingYield", "month", ...) submitted
  → Listing Reputation Registry updated
  → Listing equity curve aggregate recomputed
  → Every 20 trades: re-attest (rolling)
```

### 4.3 What the listing shows over time

| Signal | Source | Update cadence |
|---|---|---|
| Equity curve (live phase) | On-chain anchored trades (all contributors) | On each anchor event |
| Sharpe (live) | Computed from anchored trades | On each attestation (every 20 trades) |
| Attestation count | ERC-8004 Reputation Registry | Real-time |
| Buyer count | ERC-1155 `balanceOf` queries | Real-time |
| Warning badge | 3+ verified rejections | On attestation |

### 4.4 Constraints that apply to both surfaces

- No popups (CLAUDE.md hard rule) — all confirmations inline-expand
- No right sidebar when chat rail is present (CLAUDE.md QA30) — both pages are single-column
- `safeStorageGet/Set` for all operator preferences
- Wallet address visible (web3 transparency, operator expectation)
- `VenueLabel::Live` (currently rejected in v1) must be enabled before real-money trades can execute — coordinate with engine team

---

## 5. Open questions and v2 deferrals

| Item | Status |
|---|---|
| zkML / TEE validation for Validation Registry | Deferred v2 |
| Portfolio-level aggregate PnL across strategies | Deferred v2 |
| Net exposure across strategies trading same asset | Deferred v2 |
| Per-strategy capital allocation display | Deferred v2 |
| `VenueLabel::Live` engine enablement | Dependency — coordinate with engine team |
| Secondary market mechanics (if creator limits supply) | Deferred — define at listing time |
| Agent-to-agent purchases (`payerKind: "agent"`) | Deferred v2 |
| Reputation scoring service (off-chain aggregation) | Deferred v2 — ERC-8004 enables but doesn't define |

---

## 6. Implementation order

### Must ship first (blockers)
1. Per-strategy pause backend (`paused` flag on run, eval loop check, pause/resume API routes)
2. Stop + close positions (broker order submission at cancel time)
3. `VenueLabel::Live` engine enablement

### Live Trading page
4. Strategy strip with column picker and localStorage persistence
5. Wallet banner component
6. Account stat strip (4 stats from run stream)
7. Active positions table
8. Per-strategy pause/resume/stop transport controls
9. `LiveStrategiesSection` home replacement (summary strip + CTA)

### Marketplace
10. ERC-8004 Identity Registry integration (register strategy as agent on listing)
11. ERC-1155 license contract (lazy mint, infinite supply)
12. Validation Registry gate for listing publish
13. Attestation engine (20-trade trigger, sharpe delta computation, `giveFeedback` submission)
14. Buyer contribution to equity curve aggregate
15. Express deploy CTA on receipt page
16. Purchased strategy badge + creator sub-label in strategies list
17. Secondary royalty (ERC-2981) wiring
