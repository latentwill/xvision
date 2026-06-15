# Marketplace purchase → land on your Strategy (design)

**Date:** 2026-06-14
**Covers QA:** #3, #4, #6, #7, #8, #9, #11, #12 (the post-purchase import flow).
**Status:** approved (operator, 2026-06-14). Implement on a branch stacked on the
Wave-1 QA quick-wins (`feat/mp-qa-quickwins` → `feat/mp-purchase-flow`).

## Goal

A bought NFT strategy lands the buyer **on the Strategy detail page in their own
Strategies**, fully decrypted, with the marketplace info and the satisfied/missing
requirements shown inline — ready to evaluate, then go live. No receipt page, no
post-order install stepper, no Agent page in the path.

## Decisions (locked with operator)

- Missing requirements → **open the strategy fully, highlight the gaps, block only
  "go live"** (not auto-substitute, not block import). (Q1)
- Destination is the **Strategy detail page** (`/strategies/:id`, the live
  `AuthoringRoute`). Referenced **Agents** are materialized **silently into the
  buyer's Agent library** so the strategy resolves — the buyer is NOT taken to an
  Agent page.
- Flow shape = **Approach A (import-first)**: on wallet-tx success, one server
  finalize call decrypts + materializes, returns the new strategy id, then the UI
  redirects to that Strategy page.

## Flow

```
Marketplace browse ─click─► Strategy detail (LineageRoute): requirements shown, "Acquire"
   ─Acquire─► wallet prompt (USDC amount = the confirmation)
   ─tx signed─► inline "Finalizing…" on the detail page
   ─finalize (server)─► verify license on-chain → decrypt sealed bundle →
        materialize Strategy + its Agents (fresh ids, AgentRefs remapped) → return strategyId
   ─redirect─► /strategies/:strategyId  (YOUR Strategies)
        • marketplace provenance strip (price paid, creator, NFT id, View on Explorer)
        • requirements panel: ✓ satisfied / ⚠ missing (highlighted, Configure/Install CTA)
        • evaluate freely; "Go live" gated until requirements satisfied
```

## Components

1. **Bundle format (publish).** A published bundle is self-contained: the `Strategy`
   **plus the full definitions of every Agent it references** (prompts/skills/model
   choices — the IP). Sealed (paid) listings encrypt this; only a license-holder
   decrypts. Pre-mainnet + no users → change the format directly (no migration);
   re-publish or wipe old testnet listings.

2. **Finalize endpoint (fixes #9, #6).** One license-gated call replaces the manual
   "Decrypt & import":
   1. Re-verify `balanceOf(buyer, listingId) > 0` (existing `license_gate`).
   2. Decrypt sealed bundle via Lit (existing) / fetch open manifest.
   3. **Materialize**: mint a fresh Strategy ULID; for each referenced Agent, create
      it in the buyer's Agent library with a fresh id and **remap the Strategy's
      `AgentRef`s** to the new ids (the step `import_strategy` is missing today).
   4. Return the new `strategyId`.
   Errors → clean 4xx + actionable message (no 500); idempotent/re-runnable since the
   purchase is already on-chain. The UI shows errors inline with Retry.

3. **Requirements diff (#4, #6, #10).** Computed from the materialized strategy's
   agents, not fixtures:
   - **Models** — per agent `(provider, model)`, reuse `resolve_provider` (provider
     configured + API key present + model enabled).
   - **MCP servers / skills** — each agent's declared tools/skills vs installed locally.
   - Render ✓ satisfied / ⚠ missing (highlighted) with a Configure/Install CTA.

4. **Strategy detail page additions (#12, #8).** On `AuthoringRoute`, as full-width
   inline strips (no right sidebar — chat-rail rule):
   - Marketplace provenance: creator, price paid, NFT/license token id, "View on
     Explorer" from the real purchase tx (fixes #8 — the tx now exists).
   - Requirements panel (from §3).
   - Go-live gate: eval/inspect always allowed; "Go live" disabled with a clear
     reason while any requirement is unmet (hooks the existing eval-launch
     `resolve_provider` refusal).

5. **Removals (#3, #7).** Delete the receipt route + `InstallSteps` +
   `IngredientBanner` + the dead Install buttons + the `0xdemo-tx` / Birdeye / Haiku /
   49-USDC fixtures. Replace `cloneIntent`'s fake-tx path (source of "free shows 49
   USDC", #7) with the real finalize flow for open-tier too.

6. **Pre-purchase confirm (#11).** Already mostly done in Wave 1 (list CTA routes to
   the detail page; requirements shown before Acquire). The wallet prompt is the
   amount confirmation; on success → finalize → land on the Strategy page.

## Testing

- Rust (TDD): finalize endpoint (license-gated, decrypt, **agent materialization +
  AgentRef remap**, idempotency, error→4xx); requirements-diff computation.
- Frontend (vitest, TDD): buy→finalize→redirect happy path; finalize error → inline
  retry (no 500); requirements panel ✓/⚠; go-live gate; removal of receipt/install
  surfaces.

## Out of scope (separate "mainnet-ready" track)

Mainnet contract deploy (V4-gated: audit + 2-of-3 multisig) and the frontend chain
env-switch. This work runs on testnet.
