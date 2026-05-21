# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-21 — `qa-chat-rail-2026-05-21` wave
> decomposed from
> `team/intake/2026-05-21-qa-chat-rail-strategy-create-broken.md`.
> Five tracks: one P0 foundation (`templates-elimination`), one
> parallel P1 engine leaf (`chat-messages-insert-failing`), one
> blocked P2 leaf (`wizard-folder-recall-honesty`), two parallel
> P2 frontend leaves (`strategies-folder-into-view-toggle`,
> `memory-into-agents-section`).
> Earlier 2026-05-21 sweep: 28 merged contracts archived under
> `team/archive/2026-05-21-conductor-sweep/`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) has its own board:
`team/board-v2.md`.

## Active

### qa-chat-rail-2026-05-21 — chat-rail "make me a strategy" path broken; templates eliminated

Decomposed 2026-05-21 from
`team/intake/2026-05-21-qa-chat-rail-strategy-create-broken.md`.
The wave's spine is `templates-elimination`: the operator's
"whatever is in my strategies folder is the context" stance
retires the parallel `template_registry`, dissolves the
placeholder-deadlock on `create_strategy`, and migrates the
existing template starter library into folder seed entries.

- [templates-elimination](contracts/templates-elimination.md) — **P0 foundation** · ready · blocks `wizard-folder-recall-honesty`
- [chat-messages-insert-failing](contracts/chat-messages-insert-failing.md) — P1 engine leaf · ready · parallel-safe
- [wizard-folder-recall-honesty](contracts/wizard-folder-recall-honesty.md) — P2 leaf · deferred · becomes `ready` when `templates-elimination` merges
- [strategies-folder-into-view-toggle](contracts/strategies-folder-into-view-toggle.md) — P2 frontend leaf · ready · coordinates with `memory-into-agents-section` on `routes.tsx`
- [memory-into-agents-section](contracts/memory-into-agents-section.md) — P2 frontend leaf · ready · coordinates with `strategies-folder-into-view-toggle` on `routes.tsx`

## Reserved

Intakes that exist in `team/intake/` but **need spec authoring first**
before contracts can open. Conductor will not freelance these into
contracts without an operator-approved spec:

- **`team/intake/2026-05-19-compare-ab-evaluations.md`** — 10 open-
  ended product asks for the AB-compare surface (live compare for
  in-flight runs, promote/demote arms, per-agent metrics, side-by-side
  traces, statistical confidence, templates, capsule→compare bridge,
  mobile view, shareable charts, strategy-name labels). Needs a
  product-design spec under `docs/superpowers/specs/` before
  decomposition.
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** —
  P2, explicitly gated on the V2 capability-first agent-model spec
  per the intake itself; resolves as part of that refactor.
- **`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`** —
  P0 bundle shipped via #425, #428, #429 (see Recently Closed below).
  P1 (#4–#11) and P2 (#13–#15) tracks Reserved pending operator
  confirmation. P1 #12 (`remote-cli-safe-eval-allowlist`) folded into
  the now-merged `v2b-remote-cli-job-safety` (#447).

## Recently Closed

### Merged 2026-05-21 sweep — archived under `team/archive/2026-05-21-conductor-sweep/`

Implementation sweep (28 contracts archived this pass):

- **Eval honesty wave** — `eval-honesty-smell-tests` (#448),
  `eval-guardrail-log-collapse` (#449), `eval-provider-attestation`
  (#450), `eval-provider-preflight` (#452 + follow-up #468).
- **V2B (security hardening) — foundation complete** —
  `v2b-dashboard-auth-boundary` (#465), `v2b-remote-cli-job-safety`
  (#447), `v2b-broker-wallet-kill-switch` (#466). All three V2B
  tracks landed in the same wave.
- **V2D (agent memory)** — single-contract wave shipped as commit
  `81007d1` (agent memory + cortex tier split F+L+T). Follow-ups
  still open as #453 / #455 / #460 / #458 (D5 kills, D2 cross-symbol).
- **V2E foundation tail** — `eval-trace-surface-foundation` (#422)
  rolled out the determinism receipts + cycle_features parquet (V2E
  item 17). Remaining V2E rows already archived 2026-05-21 on
  `team/board-v2.md`.
- **V2F (strategy authoring) — fully complete (six tracks)** —
  `strategies-folder-surface` (#414), `agent-pipeline-template-library-expansion`
  (#409), `wizard-prompt-strategy-folder-and-templates` (#408),
  `strategies-folder-prepopulation` (#419), `strategies-folder-import`
  (#420), `strategy-ideas-tool-surface` (#421).
- **CLI operator safety P0** — `cli-operator-safety-p0` (#425, #428,
  #429): `xvn eval cancel`, engine hard limits, `xvn experiment run`
  scope guardrails.
- **Docs / lists / metric polish — 2026-05-21 intake closed** —
  `max-drawdown-danger-tone` (#424), `list-search-filter-completion-audit`
  (#430), `docs-ui-prototype-alignment` (#441 + #454),
  `list-search-filter-missing-surfaces` (#433, #439, #442, #457 —
  four slices: skills, providers, eval-compare sort, scenarios-detail
  Runs tab). `docs-search-list-component-adoption` stays deferred
  (deferred lane).
- **Docs / agent wiki tail** — `docs-freshness-staleness-guard` (#434),
  `docs-agentd-surface-page` (#432).
- **Clawpatch blockers** — `clawpatch-engine-test-helpers` (#431 —
  B-1 through B-4), `clawpatch-frontend-components` (#438 — B-6
  through B-10, B-11 closed by the MobileDrawer no-popups refactor
  in #451), `clawpatch-cli-test-assert` (B-5 was already satisfied
  by #423; contract closed without code change).
- **QA Round 4 outstanding tail** — `paper-eval-inspector-parity`
  (#440 — recon doc; no source-visible gap),
  `scenario-clone-form-structural-fields` (#437),
  `strategy-require-at-least-one-agent-fixture-migration` (#443
  worked example + #467 fallback removal + 3 fixture migrations).

### Archived earlier (still on disk)

- **2026-05-20 sweep** — Lists v1 phase 1 (`list-component-port-desktop`
  #390, `list-component-port-mobile` #395,
  `list-component-tokens-reconcile` #396), backend pagination follow-up
  (#397), QA operator round 7 (intake-direct PRs #385/#386/#387/#391),
  QA Round 6 (#360), skills refresh (#379). See
  `team/archive/2026-05-20-lists-v1-phase-1/`.

- **2026-05-19 sweep** — Harness observability audit F-2/F-6 (#294,
  #302), QA Round 5 F-1/F-2/F-3/F-5 (#316), QA Round 5 F-4 closed,
  `q15-tailscale-serve-api-reachability` parked, Agent CI/CD Phase-1
  parked to a handoff doc. See `team/archive/2026-05-19-sweep/`.

- **2026-05-18 sweep #2** — QA Round 2/3 tail (#275, #282, #283, #284,
  #286, #280), V2A onboarding closed (`v2a-in-app-docs` #281), Harness
  observability F-1 (`harness-prompt-hash-real-digest` #277). See
  `team/archive/2026-05-18-sweep-2/`.

## Stale-info hygiene

The cumulative "Recently Closed" section above is a window, not a log.
Lookups for closed work go to `team/archive/<sweep>/` first. After two
conductor sweeps, an entry rolls off this section.
