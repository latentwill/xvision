# xvision Execution Board — 2026-05-13

This is the deduped handoff board for the current recovery/rework pass.
Older plans remain useful as source material, but agents should execute
against the tracks below rather than blindly following overlapping plan
documents.

## Superseded wrappers

Use these as reference only:

- `docs/superpowers/plans/2026-05-12-pr91-94-unworked-features.md`
- `docs/superpowers/plans/2026-05-12-qa-pass-4-remediation.md`
- `docs/superpowers/plans/2026-05-12-qa-pass-4-surface-consistency.md`
- `docs/superpowers/plans/2026-05-12-agent-access-and-cli-discoverability.md`
- `docs/superpowers/plans/2026-05-12-remote-cli-over-tailscale.md`

## Global rules

- One track per worktree.
- Board claims and `team/queue/*__claim.md` files are plain git-tracked
  coordination artifacts; they do not use the MCP/subagent message queue.
- If MCP-backed subagents hang at a "booting MCP" or similar status, keep using
  board/status files for track state and diagnose the subagent runtime
  separately instead of changing repo coordination.
- Do not execute the wrapper plans directly.
- If two tracks touch the same source file, they are not parallel-safe unless
  explicitly marked below.
- Cherry-pick narrow useful slices from older branches or PRs where cheaper,
  but do not wholesale merge stale branches.
- Old strategy format compatibility is not required. We are pre-alpha; prefer
  the simpler target shape.

## Track board

| Track | Worktree | Main scope | Depends on | Parallel-safe? | Verification |
|---|---|---|---|---|---|
| `remote-cli-orphan-recovery` | `.worktrees/remote-cli-orphan-recovery` | CLI job restart/orphan sweep | none | yes | `cargo test -p xvision-dashboard cli_jobs -- --nocapture` |
| `agent-docs-cli-truth` | `.worktrees/agent-docs-cli-truth` | README, skills docs, CLI help truth | none | yes | `cargo test -p xvision-cli help_cli -- --nocapture` |
| `ghcr-build-optimization` | `.worktrees/ghcr-build-optimization` | GH Actions preflight + staging profile | none | yes | YAML parse + workflow diff check |
| `strategy-agent-backend` | `.worktrees/strategy-agent-backend-core` | Strategy-agent backend refactor | none | no overlap with tracks 9/10 | `cargo test -p xvision-engine && cargo test -p xvision-cli strategy -- --nocapture` |
| `pr94-chart-stabilization` | `.worktrees/pr94-chart-stabilization` | Narrow chart bug salvage from PR 94 | none | yes | `pnpm --dir frontend/web test -- RunChart LiveChart` |
| `runtime-render-optimization` | `.worktrees/runtime-render-optimization` | Rust + frontend rendering-speed optimization pass | after `pr94-chart-stabilization` preferred | no overlap with active chart/frontend tracks | frontend build/test + focused chart perf smoke; Rust checks in CI/non-deploy |
| `qa4-settings-zero-provider` | `.worktrees/qa4-settings-zero-provider` | Zero providers, no default LLM, provider edit, broker replace | none | yes | `cargo test -p xvision-core -p xvision-engine` |
| `qa4-scenarios-4h-bars-ui` | `.worktrees/qa4-scenarios-4h-bars-ui` | 4H scenarios + bars fetch UI | prefer remote CLI sweep first | yes if off wizard/strategy files | `cargo test -p xvision-engine scenario -- --nocapture` |
| `qa4-chat-eval-launcher` | `.worktrees/qa4-chat-eval-launcher` | Chat tools + eval launcher preflight/errors | after track 9 preferred | no overlap with track 9 | dashboard/eval tests |
| `qa4-surface-consistency` | `.worktrees/qa4-surface-consistency` | Wizard/API/list/home/eval consistency | after track 4 preferred | no overlap with tracks 4/8 | dashboard + frontend tests |
| `strategy-agent-inspector` | `.worktrees/strategy-agent-inspector` | Inspector rebuild for agent composition | track 4 | no | frontend typecheck + authoring smoke |
| `strategy-eval-ui-polish` | current workspace | Strategy/eval UI polish after modular agents: strategy list tags/model, Inspector chrome, overflow, eval timer, xvision skill trigger | none | no overlap with active frontend docs/runtime chart tracks | focused frontend tests + typecheck; Rust API compile in CI/non-deploy |

## Recommended order

1. `remote-cli-orphan-recovery`
2. `agent-docs-cli-truth`
3. `ghcr-build-optimization`
4. `strategy-agent-backend`
5. `pr94-chart-stabilization`
6. `runtime-render-optimization`
7. `qa4-settings-zero-provider`
8. `qa4-scenarios-4h-bars-ui`
9. `qa4-surface-consistency`
10. `qa4-chat-eval-launcher`
11. `strategy-agent-inspector`

## Immediate start set

Safe to start now:

- `remote-cli-orphan-recovery`
- `agent-docs-cli-truth`
- `ghcr-build-optimization`
- `pr94-chart-stabilization`
- `runtime-render-optimization` once `pr94-chart-stabilization` is merged or no
  longer active
- `qa4-settings-zero-provider`
- `qa4-scenarios-4h-bars-ui`

Wait for `strategy-agent-backend`:

- `qa4-surface-consistency`
- `strategy-agent-inspector`

Do not overlap:

- `strategy-agent-backend` with `qa4-surface-consistency`
- `strategy-agent-backend` with `strategy-agent-inspector`
- `qa4-surface-consistency` with `qa4-chat-eval-launcher`

## Cherry-pick policy

Allowed:

- Chart-only fixes from PR 94 into `pr94-chart-stabilization`
- Narrow bars-fetch UI slices from `.worktrees/bars-fetch-ui`
- Missing remote CLI job pieces only if confirmed absent

Avoid:

- Merging `origin/codex/qa-pass-4` wholesale
- Reviving `2026-05-12-pr91-94-unworked-features.md` as an execution branch
- Carrying forward old strategy-shape compatibility unless a live caller forces it

## Track notes

### `strategy-agent-backend`

- This is the central backend unblocker.
- Old slot-shape compatibility can be cut aggressively.
- Default authoring model: inline agents first, promote later.
- Use `.worktrees/strategy-agent-backend-core`. The earlier
  `.worktrees/strategy-agent-backend` branch contains out-of-scope frontend
  work and is source-only unless deliberately cherry-picked.
- Current checkpoint: `strategy-agent-backend-core` commit `2ae9828` exposes
  existing agent-ref/pipeline API through `xvn strategy add-agent`,
  `remove-agent`, and `set-pipeline`.
- Additional checkpoints: `f2786a3` adds `xvn strategy migrate-agents`, and
  `fd1fc0e` executes resolved single/sequential AgentRef pipelines through
  eval and `xvn strategy run`.
- Latest checkpoint: `b9c39f1` makes `xvn strategy new` seed template drafts
  directly as AgentRefs + pipeline, and updates `xvn strategy run` token
  estimates to derive from resolved agent slots for AgentRef strategies.
- Inspector can build against AgentRefs; graph pipeline execution is still
  intentionally not implemented in runtime.

### `pr94-chart-stabilization`

- Target chart stabilization first.
- Safe scenario/new-screen fixes may be absorbed only if clearly scoped and
  do not drag in unrelated PR 94 state.
- Completed checkpoint: `pr94-chart-stabilization` commit `da8b3a6`
  stabilizes RunChart time-scale synchronization and passes
  `corepack pnpm --dir frontend/web test -- RunChart LiveChart`.

### `runtime-render-optimization`

Execution goal: optimize Rust chart/API payload generation and frontend
rendering speed. This track is a runtime rendering-speed pass, not the GHCR
build-optimization track.

Static pass findings from 2026-05-13:

| Priority | Effort | Item |
|---|---:|---|
| P0 | M | Stop rebuilding the entire live chart on every SSE point. `useRunStream` copies arrays per event and `RunChart` recreates all chart instances whenever `payload` changes. Keep series refs and call Lightweight Charts `series.update()` for bars/equity/markers. |
| P0 | S | Add HTTP compression for API and static responses. Chart JSON and the JS bundle are large, but `build_router` has no compression layer and static serving writes raw bodies. |
| P0 | L | Make chart payloads range/layer-aware. Rust computes and returns all bars, all indicators, position, equity, and drawdown for every run chart request. Add query params for visible window and enabled layers, plus server downsampling for long windows. |
| P1 | M | Route-level code splitting. Every route is statically imported into the initial bundle; use React Router lazy route modules so settings/authoring/eval/chart pages load only when visited. |
| P1 | S | Lazy-load heavy chart and markdown modules. `Layout` always loads `ChatRail`, which imports `react-markdown`/`remark-gfm`; `lightweight-charts` is also in the main chunk. |
| P1 | S | Reduce font payload. `main.tsx` imports full `@fontsource/*` CSS for multiple weights, emitting many subset files and both `woff2`/`woff`. Prefer latin-only imports or local `@font-face` with only used `woff2` files. |
| P1 | S | Add immutable cache headers for hashed assets and no-cache for `index.html`. Static responses currently only set content type. |
| P1 | S | Disable production source maps or make them hidden. `vite.config.ts` emits source maps into the embedded static folder; current map is about 3 MB. |
| P1 | M | Cache server-built chart payloads or indicator series by `(run_id, bars cache key, layer set)`. `build_run_payload` recomputes indicators and remaps vectors on every request. |
| P2 | M | Replace per-bar `position` series with compact intervals/transitions. Rust emits one `PositionPoint` per bar and frontend filters it twice. |
| P2 | S | Do not send compare price backdrop unless requested. Backend includes `price_backdrop` whenever runs share a scenario, even though the UI default is off. |
| P2 | M | Bulk-load equity for strategy/compare charts. Strategy chart does one equity query per run serially; use one SQL query grouped by `run_id`. |
| P2 | S | Make range buttons actually constrain rendered data. `ChartContainer` stores `1d/1w/1m/3m/All`, but charts still `setData` for full arrays. |
| P2 | S | Avoid full latest-run charts on overview pages unless visible or expanded. Home and eval list both fetch/render a full `RunChart` for latest run. |
| P3 | S | Use scenario-filtered run API instead of fetching all runs and filtering client-side in scenario detail. |
| P3 | M | Reduce chart synchronization churn. `RunChart` creates up to five separate chart instances and each visible-range change propagates to all peers. |

Initial verification guidance:

- Frontend: `corepack pnpm --dir frontend/web test -- RunChart LiveChart`,
  `corepack pnpm --dir frontend/web build`, and a focused manual smoke on run
  detail, live run, compare, scenario detail, and home.
- Rust: dashboard/chart API tests should run in CI or another non-deploy
  environment because local Cargo is forbidden on this deploy host.

### `ghcr-build-optimization`

- Completed checkpoint: `ghcr-build-optimization` commit `ef76621`
  installs pnpm before `actions/setup-node` enables pnpm cache in the GHCR
  workflow preflight.
- Verified with `git diff --check`, workflow diff check, and Python YAML parse.

### `agent-docs-cli-truth`

- Completed checkpoint: `agent-docs-cli-truth` commit `b617c09` aligns
  README/MANUAL/Claude-skill/frontend CLI guidance with the shipped command
  surface.
- Verified with `bash scripts/check_agent_docs.sh` and `git diff --check`.
- Rust help test remains CI/non-deploy follow-up because cargo is forbidden on
  this deploy host.

### `remote-cli-orphan-recovery`

- Completed checkpoint: `remote-cli-orphan-recovery` commit `d1d41b3`
  wires CLI job orphan recovery/restart handling.
- Verified with `git diff --check main...HEAD`, scoped `rg` audit, and clean
  branch status. Cargo verification remains CI/non-deploy follow-up.

### `qa4-settings-zero-provider`

- Workspace startup with zero providers and no default LLM is required.
- Completed checkpoint: `qa4-settings-zero-provider` commit `1ed7c45`
  allows zero-provider startup/no default LLM and updates provider/broker UI
  behavior.
- Verified with `git diff --check`, static conflict scan, focused provider
  frontend test, full frontend test suite, and frontend typecheck. Rust
  core/engine tests remain CI/non-deploy follow-up.

### `qa4-scenarios-4h-bars-ui`

- Completed checkpoint: `qa4-scenarios-4h-bars-ui` commit `7f00196`
  implements 4H scenarios + bars-fetch UI across API/CLI/frontend surfaces.
- Verified with frontend install, full frontend test suite, frontend typecheck,
  `git diff --check`, and clean branch status. Rust scenario tests remain
  CI/non-deploy follow-up.

### `strategy-agent-inspector`

- Completed checkpoint: `strategy-agent-inspector` commit `cd5687d` adapts the
  authoring/Inspector UI for strategy AgentRefs and pipeline state.
- Verified with `corepack pnpm --dir frontend/web test -- authoring-risk`,
  frontend typecheck, and `git diff --check`.

### `qa4-surface-consistency`

- Completed checkpoint: `qa4-surface-consistency` commit `350d462` repairs
  command-palette/frontend route consistency and updates frontend docs.
- Verified with full frontend test suite, frontend typecheck, frontend build,
  and `git diff --check`. Rust dashboard verification remains CI/non-deploy
  follow-up.

### `qa4-chat-eval-launcher`

- Completed checkpoint: `qa4-chat-eval-launcher` commit `18ab4c0` adds eval
  launcher preflight, uses scenario-registry scenarios, defaults the web
  launcher to backtest mode, and keeps launch/preflight errors inline.
- Final branch head is `4fbabaa`, which removes a branch-local board edit so
  the track status stays scoped to `team/status/qa4-chat-eval-launcher.md`.
- Verified with focused eval-runs test, frontend typecheck, full frontend test
  suite, and `git diff --check`. Rust checks remain CI/non-deploy follow-up.

### Board Closeout

- All ten execution-board tracks now have branch/status checkpoints and clean
  worktrees.
- No additional unclaimed task remains on this board. New work should be added
  as a fresh board item or selected from a newer execution board.

### `strategy-eval-ui-polish`

Claimed 2026-05-13T08:46:17Z in the current workspace.

Scope:

- Fix the Strategies model column for the modular AgentRef structure.
- Surface strategy tags on the Strategies page.
- Remove the Inspector validation box.
- De-emphasize the Inspector strategy id.
- Prevent long IDs/errors from overflowing UI boxes.
- Add elapsed/duration timing to eval runs.
- Confirm conversation persistence status; SQLite-backed chat sessions/messages
  already exist, so no new work is needed unless product wants a richer export.
- Narrow the xvision Claude skill trigger so it is for agents using the `xvn`
  CLI/dashboard, not generic coding in this repo.

Initial claim order:

1. Strategy list model/tags.
2. Inspector cleanup and overflow.
3. Eval timer.
4. Skill trigger wording.

## Next board intake

The active roadmap for followups, specs, and todo notes is now
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.

Seed the next execution board from these first tickets:

| Order | Ticket | Phase | Effort | Source |
|---|---|---|---|---|
| 1 | Add Driver.js first-run and restart-tour infrastructure | V2A | M | F36 |
| 2 | Add in-app docs/help route and docs index | V2A | M | onboarding/settings, dashboard docs |
| 3 | Create resettable example strategies/scenarios/tutorial artifacts | V2A | M | frontend docs, eval docs |
| 4 | Add dashboard mutating-route auth boundary | V2B | L | F35 |
| 5 | Add remote CLI orphan recovery and audit trail | V2B | M | F37, remote CLI specs |
| 6 | Add broker/wallet/testnet kill switch and limits | V2B | M | security + blockchain plans |
| 7 | Deploy/refactor Mantle Sepolia identity/reputation addresses | V2C | M | SLF2, ADR 0008 |
| 8 | Implement strategy NFT mint/readback flow | V2C | L | SLF3 |
| 9 | Implement testnet marketplace list/buy/sell/delegate flow | V2C | L | marketplace spec |
| 10 | Implement reputation and validation receipt write/readback | V2C | L | SLF4, SLF5 |
| 11 | Build autoresearcher mutation/eval/judge loop | V3 | L | autoresearcher plans |
| 12 | Build autoresearcher dashboard and lineage review | V3 | L | autoresearcher dashboard plan |
| 13 | Run final UI/UX pass across dashboard surfaces | V3 | L | design docs, chart plans |
| 14 | Prepare contract audit, launch flags, and mainnet runbook | V4 | L | ADR 0008, contract specs |
