# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-16.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B–V4 roadmap) lives on its own board:
**`team/board-v2.md`**.

## Active

Nothing. `alpaca-paper-crypto-submit` landed via #191 on 2026-05-16. Remaining
v1 work is the V2A onboarding leaves on `team/board-v2.md`.

## Immediate start set

Safe to claim right now (no unresolved Foundation dependency):

- V2A leaves — see `team/board-v2.md` (`v2a-driver-tour`, `v2a-in-app-docs`,
  `v2a-example-artifacts`; independent, parallel-safe).

## Deferred

- [q15-tailscale-serve-api-reachability](contracts/q15-tailscale-serve-api-reachability.md) — integration · deferred 2026-05-16. Mobile/QA over tailnet parked, not archived. Revive by flipping `status:` back to `ready` and re-adding to an Active wave block above.

## Recently closed waves

Archived 2026-05-16:

- **Q4** — all four tracks merged to `main`.
- **Q8** — board tracks landed via #124 / #162 / #164 combined PRs; individual `qa8-*` PRs closed unmerged on purpose.
- **Q9** — all `qa9-*` PRs merged (#131–#161).
- **Q10** — all `qa10-*` PRs merged (#166–#180); chat/runtime recovery via #169 and #170.
- **eval-review data-model** — merged via #176.
- **color-themes-light-dark** — merged via #135.
- **mobile-safari-load** — merged via #147; iPhone Safari follow-up via #181.
- **q15-scenario-granularity-dropdown** — merged via #182; archived under `team/archive/2026-05-16-q15/`.
- **q15-scenario-warmup-bars** — merged via #183; archived under `team/archive/2026-05-16-q15/`.
- **q15-agent-max-tokens-from-model** — merged via #185; archived under `team/archive/2026-05-16-q15/`.
- **q15-eval-json-export** — merged via #187; archived under `team/archive/2026-05-16-q15/`.
- **q15-object-json-output** — merged via #189; archived under `team/archive/2026-05-16-q15/`.
- **q15-eval-retry-button** — merged via #184; archived under `team/archive/2026-05-16-q15/`.
- **eval-review-agent-engine** — merged via #186; archived under `team/archive/2026-05-16-eval-review/`.
- **eval-review-api-cli** — merged via #188; archived under `team/archive/2026-05-16-eval-review/`.
- **eval-review-run-detail-ui** — merged via #190; archived under `team/archive/2026-05-16-eval-review/`.
- **alpaca-paper-crypto-submit** — merged via #191; contract retained at `team/contracts/alpaca-paper-crypto-submit.md` for regression context.

Q15 wave closed except the deferred integration track. Eval-review wave fully closed.

See `team/archive/2026-05-16-migration/` for the historical board snapshot.

## V2B+ intake (not yet decomposed)

`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` lists items 4–14
(auth boundary, kill switch, on-chain wallets, autoresearcher, audit). The
conductor decomposes one wave at a time. Do not freelance contracts from
that list without going through intake.
