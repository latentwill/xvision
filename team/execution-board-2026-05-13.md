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
| `mobile-safari-load` | `.worktrees/mobile-safari-load` | Mobile Safari still does not load the dashboard | none | no overlap with active frontend tracks | Safari/mobile load repro + frontend test/build smoke |

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
12. `mobile-safari-load`
13. `strategy-eval-ui-polish`

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
- `mobile-safari-load`

Wait for `strategy-agent-backend`:

- `qa4-surface-consistency`
- `strategy-agent-inspector`

Do not overlap:

- `strategy-agent-backend` with `qa4-surface-consistency`
- `strategy-agent-backend` with `strategy-agent-inspector`
- `qa4-surface-consistency` with `qa4-chat-eval-launcher`
- `mobile-safari-load` with broad frontend route refactors

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

- Claimed in `.worktrees/runtime-render-optimization`.
- Implemented checkpoint:
  - `RunChart` now keeps chart instances stable across data-only payload
    changes and updates existing series instead of rebuilding every pane.
  - Dashboard router applies HTTP compression.
  - Embedded static assets get cache headers (`index.html` no-cache, hashed
    assets immutable).
  - Frontend production source maps are no longer embedded.
  - Font imports are latin-only.
  - Route modules and the chat rail are lazily split from the initial bundle.
- Verification:
  - `corepack pnpm --dir frontend/web test -- RunChart LiveChart`
  - `corepack pnpm --dir frontend/web typecheck`
  - `corepack pnpm --dir frontend/web test`
  - `corepack pnpm --dir frontend/web exec vite build --outDir /tmp/xvision-runtime-render-build --emptyOutDir true`
  - `git diff --check`
- Build artifact check: no `.map` files emitted, font asset count is 18
  instead of the previous 100, and the old single ~805 KB JS bundle is split
  into route/chart/chat chunks.
- Rust checks remain CI/non-deploy follow-up because local Cargo is forbidden
  on this deploy host.

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

### `mobile-safari-load`

- New regression report: mobile still does not load on Safari.
- Claimed in `.worktrees/mobile-safari-load`; focus on root-cause isolation
  before changing production code.
- Initial target surface: Vite/React dashboard startup path and any browser
  compatibility assumptions that can break iOS Safari before the app renders.

### Board Closeout

- All previous execution-board tracks now have branch/status checkpoints and clean
  worktrees.
- `mobile-safari-load` and `strategy-eval-ui-polish` were the final open work
  on this board during the 2026-05-13 merge/deploy pass.

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
