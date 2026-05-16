# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-16.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

## Active

### Wave: eval-review

- [eval-review-agent-engine](contracts/eval-review-agent-engine.md) — foundation · ready · depends on `eval-review-data-model` (merged #176)
- [eval-review-api-cli](contracts/eval-review-api-cli.md) — leaf · ready · depends on `eval-review-agent-engine`
- [eval-review-run-detail-ui](contracts/eval-review-run-detail-ui.md) — leaf · ready · depends on `eval-review-api-cli`

### Wave: v2a (onboarding & docs)

- [v2a-driver-tour](contracts/v2a-driver-tour.md) — leaf · ready · independent
- [v2a-in-app-docs](contracts/v2a-in-app-docs.md) — leaf · ready · independent
- [v2a-example-artifacts](contracts/v2a-example-artifacts.md) — leaf · ready · independent

## Immediate start set

Safe to claim right now (no unresolved Foundation dependency):

- `eval-review-agent-engine` (Foundation for the rest of the eval-review wave)
- `v2a-driver-tour`
- `v2a-in-app-docs`
- `v2a-example-artifacts`

## Waiting

- `eval-review-api-cli` — waits on `eval-review-agent-engine`.
- `eval-review-run-detail-ui` — waits on `eval-review-api-cli`.

## Recommended order

1. `eval-review-agent-engine` (unblocks api-cli + ui)
2. Any v2a track in parallel
3. `eval-review-api-cli`
4. `eval-review-run-detail-ui`

## Recently closed waves

Archived 2026-05-16:

- **Q4** — all four tracks merged to `main`.
- **Q8** — board tracks landed via #124 / #162 / #164 combined PRs; individual `qa8-*` PRs closed unmerged on purpose.
- **Q9** — all `qa9-*` PRs merged (#131–#161).
- **Q10** — all `qa10-*` PRs merged (#166–#180); chat/runtime recovery via #169 and #170.
- **eval-review data-model** — merged via #176; remainder of the wave above.
- **color-themes-light-dark** — merged via #135.
- **mobile-safari-load** — merged via #147.

See `team/archive/2026-05-16-migration/` for the historical board snapshot.

## V2B+ intake (not yet decomposed)

`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` lists items 4–14
(auth boundary, kill switch, on-chain wallets, autoresearcher, audit). The
conductor decomposes one wave at a time. Do not freelance contracts from
that list without going through intake.
