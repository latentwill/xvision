# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-16.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B–V4 roadmap) lives on its own board:
**`team/board-v2.md`**.

## Active — Q15 wave (new QA intake)

Foundation:

- [q15-scenario-warmup-bars](contracts/q15-scenario-warmup-bars.md) — foundation · ready · highest priority (bar-1 decisions in a vacuum)
- [q15-agent-max-tokens-from-model](contracts/q15-agent-max-tokens-from-model.md) — foundation · ready · stops empty-output truncation
- [q15-eval-json-export](contracts/q15-eval-json-export.md) — foundation · ready · anchors per-object JSON shape

Leaves:

- [q15-scenario-granularity-dropdown](contracts/q15-scenario-granularity-dropdown.md) — leaf · ready · independent
- [q15-eval-retry-button](contracts/q15-eval-retry-button.md) — leaf · ready · serialize with eval-runs-detail.tsx editors
- [q15-object-json-output](contracts/q15-object-json-output.md) — leaf · ready · depends on `q15-eval-json-export`

Intake: `team/intake/2026-05-16-q15.md`. Spec for the meaty items:
`docs/superpowers/specs/2026-05-16-q15-eval-resilience-and-contracts.md`.

## Active — eval-review wave

- [eval-review-agent-engine](contracts/eval-review-agent-engine.md) — foundation · ready · depends on `eval-review-data-model` (merged #176)
- [eval-review-api-cli](contracts/eval-review-api-cli.md) — leaf · ready · depends on `eval-review-agent-engine`
- [eval-review-run-detail-ui](contracts/eval-review-run-detail-ui.md) — leaf · ready · depends on `eval-review-api-cli`

## Immediate start set

Safe to claim right now (no unresolved Foundation dependency):

- `q15-scenario-warmup-bars` (highest QA priority)
- `q15-agent-max-tokens-from-model`
- `q15-eval-json-export`
- `q15-scenario-granularity-dropdown` (independent leaf)
- `eval-review-agent-engine`
- V2A leaves — see `team/board-v2.md`

## Waiting

- `q15-object-json-output` — waits on `q15-eval-json-export`.
- `q15-eval-retry-button` — coordinate with `q15-eval-json-export` and
  eval-review UI on `eval-runs-detail.tsx`.
- `eval-review-api-cli` — waits on `eval-review-agent-engine`.
- `eval-review-run-detail-ui` — waits on `eval-review-api-cli`.

## Recommended order

1. `q15-scenario-warmup-bars` (operator-facing fix — biggest leverage).
2. `q15-agent-max-tokens-from-model` (stops the empty-output failure mode).
3. `q15-eval-json-export` (anchors the JSON contract).
4. `q15-scenario-granularity-dropdown` in parallel (UI fix).
5. `q15-object-json-output` once #3 lands.
6. `q15-eval-retry-button` in series with the eval-runs-detail editors.
7. `eval-review-agent-engine` → `eval-review-api-cli` → `eval-review-run-detail-ui`.
8. V2A from `team/board-v2.md` in parallel.

## Recently closed waves

Archived 2026-05-16:

- **Q4** — all four tracks merged to `main`.
- **Q8** — board tracks landed via #124 / #162 / #164 combined PRs; individual `qa8-*` PRs closed unmerged on purpose.
- **Q9** — all `qa9-*` PRs merged (#131–#161).
- **Q10** — all `qa10-*` PRs merged (#166–#180); chat/runtime recovery via #169 and #170.
- **eval-review data-model** — merged via #176; remainder of the wave above.
- **color-themes-light-dark** — merged via #135.
- **mobile-safari-load** — merged via #147; iPhone Safari follow-up via #181.

See `team/archive/2026-05-16-migration/` for the historical board snapshot.

## V2B+ intake (not yet decomposed)

`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` lists items 4–14
(auth boundary, kill switch, on-chain wallets, autoresearcher, audit). The
conductor decomposes one wave at a time. Do not freelance contracts from
that list without going through intake.
