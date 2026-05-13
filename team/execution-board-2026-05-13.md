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
| `qa4-settings-zero-provider` | `.worktrees/qa4-settings-zero-provider` | Zero providers, no default LLM, provider edit, broker replace | none | yes | `cargo test -p xvision-core -p xvision-engine` |
| `qa4-scenarios-4h-bars-ui` | `.worktrees/qa4-scenarios-4h-bars-ui` | 4H scenarios + bars fetch UI | prefer remote CLI sweep first | yes if off wizard/strategy files | `cargo test -p xvision-engine scenario -- --nocapture` |
| `qa4-chat-eval-launcher` | `.worktrees/qa4-chat-eval-launcher` | Chat tools + eval launcher preflight/errors | after track 9 preferred | no overlap with track 9 | dashboard/eval tests |
| `qa4-surface-consistency` | `.worktrees/qa4-surface-consistency` | Wizard/API/list/home/eval consistency | after track 4 preferred | no overlap with tracks 4/8 | dashboard + frontend tests |
| `strategy-agent-inspector` | `.worktrees/strategy-agent-inspector` | Inspector rebuild for agent composition | track 4 | no | frontend typecheck + authoring smoke |

## Recommended order

1. `remote-cli-orphan-recovery`
2. `agent-docs-cli-truth`
3. `ghcr-build-optimization`
4. `strategy-agent-backend`
5. `pr94-chart-stabilization`
6. `qa4-settings-zero-provider`
7. `qa4-scenarios-4h-bars-ui`
8. `qa4-surface-consistency`
9. `qa4-chat-eval-launcher`
10. `strategy-agent-inspector`

## Immediate start set

Safe to start now:

- `remote-cli-orphan-recovery`
- `agent-docs-cli-truth`
- `ghcr-build-optimization`
- `pr94-chart-stabilization`
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
