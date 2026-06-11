# xvision marketplace subgraph

Indexes the xvision marketplace + ERC-8004 identity contracts on Mantle and
serves the public read model the dashboard `MarketplaceData` layer consumes
(runbook §3.2 / C7). The browse experience reads entirely from this indexer —
no centralized DB (surface spec §6.3).

## Entities

`Agent`, `Listing`, `Sale`, `EvalAttestation`, `Feedback`, `Validation` — see
`schema.graphql`. `FeedbackCounter` is an internal bookkeeping entity (ignore it
on the read side); it assigns each `Feedback` the on-chain append index so
`FeedbackRevoked(agentId, index)` can tombstone the right row.

## Indexed contracts (Mantle Sepolia, chain 5003)

Addresses + start blocks live in `networks.json` (mirrors
`config/mantle-sepolia.toml`). Events handled:

| Contract | Event → handler |
|---|---|
| IdentityRegistry | `AgentRegistered` → Agent |
| ListingRegistry | `ListingCreated` / `ListingUpdated` / `ListingRevoked` → Listing |
| Marketplace | `Sold` → Sale (also `protocolFeeBps()` eth_call to snapshot the fee onto a Listing) |
| EvalAttestationRegistry | `AttestationPosted` → EvalAttestation |
| ReputationRegistry | `FeedbackPosted` / `FeedbackRevoked` → Feedback |
| ValidationRegistry | `ValidationPosted` → Validation |

## Build & test

```bash
pnpm install        # or npm install
pnpm codegen        # generate AssemblyScript types from schema + ABIs
pnpm build          # compile mappings to wasm (validates the whole subgraph)
pnpm test           # matchstick unit tests (tests/)
```

`codegen` writes `generated/` (git-ignored); `build` writes `build/`
(git-ignored). The minimal ABIs in `abis/` contain only the events/calls the
mappings use, so the subgraph builds without the Foundry artifacts.

## Deploy

Host: **Goldsky** (the only host indexing Mantle Sepolia today — The Graph's
decentralized net doesn't list Mantle, and its hosted Mantle service is
deprecated). The artifact stays host-agnostic; redeploy elsewhere unchanged if
that changes.

```bash
goldsky subgraph deploy xvision-marketplace/<version> --path .
goldsky subgraph tag create xvision-marketplace/<version> --tag live   # stable URL
```

### Live deployment (2026-06-11)

- **Version:** `xvision-marketplace/1.0.1` (tag `live`).
- **GraphQL endpoint (public read):**
  `https://api.goldsky.com/api/public/project_cmq8zp6etzmv101xwaeih73lr/subgraphs/xvision-marketplace/live/gn`
- **Network alias:** `mantle-sepolia` (Goldsky's slug for chain 5003).
- **startBlocks** set to each contract's creation block (Identity 39765646,
  Reputation 39765649, Validation 39765652, Listing 39765663, EvalAttestation
  39765668, Marketplace 39765674) — synced in seconds vs. a full-chain scan.
- Verified live: indexes agent #0/#1, listing #1 (1.0 USDC), and the two
  smoke-test buys from the runbook addenda.

The frontend reads this endpoint via `VITE_MARKETPLACE_SUBGRAPH_URL` (baked by
`Dockerfile.deploy`; see `frontend/web/.env.example`).

### When redeploying / changing addresses

- Re-run the deploy + re-point the `live` tag (frontend env stays stable via the
  tag). Recompute `startBlock`s if the contracts move.
- **Update `src/constants.ts`** `MARKETPLACE_ADDRESS` if the Marketplace proxy
  changes (used only for the `protocolFeeBps` snapshot).
- **Reconcile ABIs** against the Mantlescan-verified contracts (runbook §2.7
  AM7) if any event signature changed.
