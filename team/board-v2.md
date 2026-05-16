# xvision V2 board

> Roadmap and active contracts for V2A → V2C. Source plan:
> `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.
>
> Same rules as the main board (`team/board.md`): one line per active track,
> each linking to a `team/contracts/<slug>.md`. Conductor-owned.
>
> Last updated: 2026-05-16.

## Active — V2A (onboarding & docs)

- [v2a-driver-tour](contracts/v2a-driver-tour.md) — leaf · ready · independent
- [v2a-in-app-docs](contracts/v2a-in-app-docs.md) — leaf · ready · independent
- [v2a-example-artifacts](contracts/v2a-example-artifacts.md) — leaf · ready · independent

All three are independent leaves — safe to claim in parallel.

## Not yet decomposed

The conductor decomposes one phase at a time. Items below are roadmap-only;
no contracts exist yet. Do **not** freelance contracts from this list — go
through `team/intake/<date>-<phase>.md` first.

### V2B — security & operability (next intake)

| # | Item | Source |
|---|---|---|
| 4 | Dashboard mutating-route auth boundary | F35 |
| 5 | Remote CLI orphan recovery + audit trail | F37, remote CLI specs |
| 6 | Broker/wallet/testnet kill switch + limits | security + blockchain plans |

### V2C — on-chain identity (after V2B)

| # | Item | Source |
|---|---|---|
| 7 | Mantle Sepolia identity/reputation address deploy | SLF2, ADR 0008 |
| 8 | Strategy NFT mint + readback flow | SLF3 |
| 9 | Testnet marketplace list/buy/sell/delegate flow | marketplace spec |
| 10 | Reputation + validation receipt write/readback | SLF4, SLF5 |

### V3 — autoresearcher

| # | Item | Source |
|---|---|---|
| 11 | Autoresearcher mutation / eval / judge loop | autoresearcher plans |
| 12 | Autoresearcher dashboard + lineage review | autoresearcher dashboard plan |
| 13 | Final UI/UX pass across dashboard surfaces | design docs, chart plans |

### V4 — mainnet readiness

| # | Item | Source |
|---|---|---|
| 14 | Contract audit, launch flags, mainnet runbook | ADR 0008, contract specs |

## Wave intake

- V2A intake: `team/intake/2026-05-16-eval-review-and-v2a.md` (V2A items 1–3 decomposed).
- V2B/V2C/V3/V4: no intake yet.

## Closeout

When all V2A contracts merge, the conductor:

1. Archives V2A contracts to `team/archive/<date>-v2a/contracts/`.
2. Updates this file to reflect V2B as the active phase.
3. Opens a V2B intake doc and decomposes items 4–6 into contracts.

## See also

- Main board (`team/board.md`) for non-V2 active work and eval-review wave.
