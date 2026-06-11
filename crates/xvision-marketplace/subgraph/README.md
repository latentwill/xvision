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

Host-agnostic. Pick an indexer host (runbook §2.9 — Goldsky / The Graph hosted
or decentralized / self-hosted graph-node) and:

```bash
# Goldsky example
goldsky subgraph deploy xvision-marketplace/v0.1.0 --path .

# The Graph (hosted/studio) example
pnpm exec graph deploy <slug> --network mantle-testnet
```

### Before mainnet / first real deploy

- **Set `startBlock`** in `networks.json` to each contract's deploy block
  (currently `0`; scanning from genesis works but is slow). Get them from
  Mantlescan for the addresses in `config/mantle-sepolia.toml`.
- **Confirm the network alias.** `subgraph.yaml` uses `mantle-testnet`; Goldsky
  expects `mantle-sepolia`. Match your chosen host's chain name (`graph build
  --network <name>` injects `networks.json`).
- **Update `src/constants.ts`** `MARKETPLACE_ADDRESS` if redeploying the
  Marketplace proxy (used only for the `protocolFeeBps` snapshot).
- **Reconcile ABIs** against the Mantlescan-verified contracts (runbook §2.7
  AM7) if any event signature changed since this was authored.
