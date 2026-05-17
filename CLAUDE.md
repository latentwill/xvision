# xvision — project guidance

Project-specific guidance. The workspace-level `/Users/edkennedy/Code/CLAUDE.md`
covers shared conventions across projects; the rules here are xvision-specific
and override anything in conflict with the workspace file.

## Deployment guardrails (hard rules)

Two deployment paths exist. **Local image build is preferred.** GHCR is reserved
for remote deployment when no local build host is available.

### Preferred: local image build → ship over SSH

Builds the Rust workspace + Vite SPA on the local build host and transfers the
~150 MB runtime image to the target. Avoids GitHub Actions minutes and the
OOM trap on small (4 GB) deploy hosts.

```bash
scripts/deploy-image.sh                          # build only, tag xvision:deploy-<sha>
scripts/deploy-image.sh --push root@host         # build + transfer + docker load
scripts/deploy-image.sh --with-identity          # include xvision-identity (Mantle)
scripts/deploy-image.sh --platform linux/arm64   # for ARM hosts (Graviton, Oracle ARM)
```

The target host only needs `docker`. After the image lands, the consuming
service (Compose, Coolify) must be recreated/redeployed so the running
container switches to the new image.

### Fallback: GHCR via GitHub Actions

Use only when there is no local build host capable of running the full
Rust+Vite build (e.g. iterating on a remote dev box with no Docker locally).

```bash
scripts/deploy-ghcr.sh
```

When triggering GHCR builds, pass `workflow_dispatch` inputs explicitly:

- `dockerfile=Dockerfile.deploy`
- `build_identity=false` (unless identity image is intentionally requested)
- `build_profile=release` for production; `staging` only for manual test images

### Rules that apply to both paths

- On remote/deploy hosts (small VPS, Coolify nodes), NEVER run `cargo`,
  `cargo build`, `cargo check`, or `cargo test`. The Rust toolchain is not
  installed there and a stray invocation can OOM the box.
- On remote/deploy hosts, NEVER build Docker images. Builds happen on the
  local build host or in GHCR.
- ALWAYS source `.op_env` before using `gh` or `op` so GitHub and 1Password
  access come from the expected environment.
- ALWAYS verify rollout by checking the running container's image digest
  matches the digest you just built (local) or published (GHCR).

These rules do not apply to local development workstations doing
`cargo test` or `docker compose build` for normal dev work.

## Terminology

Naming conventions across the xvision codebase. Locked in 2026-05-10 (terminology
rename Option B — see `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`).
Diverging from these names should require a written rationale.

| Concept | Use this name | Don't use |
|---|---|---|
| Per-decision-cycle id (briefing → decision → outcome) | `cycle_id` | ~~setup_id~~ |
| Pre-mint local id of a marketplace pipeline | `agent_id` (string ULID, becomes the NFT token id post-mint) | ~~strategy_id~~ |
| Immutable pipeline configuration (the strategy artifact) | `Strategy` (in `crates/xvision-engine/src/strategies/`) | ~~StrategyBundle~~, ~~bundle~~ |
| Reusable agent template (per-prompt+model+skills record) | `Agent` (with `Vec<AgentSlot>`) | ~~agent template~~, ~~saved profile~~ |
| Strategy's reference to a library agent | `AgentRef { agent_id, role }` | (no rename) |
| Trading-decision producer trait (xvision-eval baselines) | `Algorithm` | ~~Strategy~~ |
| One experimental arm in A/B compare | `arm` / `Box<dyn Algorithm>` | (no change) |
| The trader's call (input to risk) | `TraderDecision` | (no change) |
| The risk gate's verdict (Approved / Modified / Vetoed) | `RiskDecision` | (no change) |
| Wallet plan's per-rule verdict (planned new enum) | `PerStrategyVerdict` | ~~Verdict~~ (collides with RiskDecision) |
| The DB table for cycles (formerly `setups`) | `cycles` | ~~setups~~ |
| Eval-result count of cycles processed | `cycles_evaluated` | ~~setups_evaluated~~ |

**Pipeline-stage names** (intern, trader, risk, executor) are **valid
conventions, not enforced.** They live as starter-template labels in
`crates/xvision-engine/src/agents/templates.rs`; the underlying data model
treats slot names as user-defined free text. Users can rename or invent
slot names per strategy. The conventions exist so multi-stage strategies
have a shared vocabulary; nothing in the engine requires them.

Amended 2026-05-12 to reflect the agents page v1 outcome (see
`docs/superpowers/plans/2026-05-11-agents-page-v1.md` and the followup
strategies refactor at `2026-05-12-strategies-refactor-agent-composition.md`).
Before that, slot names were hardcoded in `StrategyBundle` fields
(`intern_slot`, `trader_slot`, `regime_slot`); the strategies refactor
replaces those with `Strategy { agents: Vec<AgentRef> }` where the role
label per AgentRef is free text.

The `xvn strategy` CLI verb manages strategy bundles and is NOT renamed.
The `xvn setup` CLI verb (config init) is NOT renamed — it remains the
verb form.

**Migration notes:**
- DB migration `0002_rename_setup_to_cycle.sql` renamed the `setups` table to
  `cycles` and `setup_id` to `cycle_id` across all six referencing tables
  (briefings, decisions, risk_outcomes, executions, traces).
- The `xvn ab-compare --setups` argument is now `--cycles`. Pre-launch
  breaking change.
- Pre-rename git tag: `pre-rename-baseline`.

## Build & test

```bash
cargo build --workspace
cargo test --workspace
```

The workspace expects `cargo` on PATH from `~/.cargo/bin`.

### Local Rust build cache discipline

Rust build output is generated and can become very large. The active local
checkout may keep its root `target/` directory when preparing deploy images, but
agents must avoid creating duplicate `target/` trees in temporary clones,
review branches, or Claude worktrees.

- Prefer building from the main checkout root when possible.
- Before running `cargo` from any temporary worktree or copied checkout, set a
  shared target directory instead of letting Cargo create a local `target/`:

  ```bash
  export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
  ```

- Do not commit, preserve, or archive `target/` directories. They are rebuildable
  artifacts.
- If a temporary worktree is no longer active, remove its `target/` before
  leaving the task. Keep `/Users/edkennedy/Code/xvision/target` only when the
  user is actively preparing a local build/deploy image.

## Docker

Slim runtime image of the `xvn` CLI lives at `ghcr.io/latentwill/xvision`.
Two tags: `:latest` (default-members; no on-chain identity stack) and
`:identity` (workspace build including `xvision-identity`).

Local builds:

```bash
DOCKER_BUILDKIT=1 docker build -t xvision:dev .
docker compose run --rm xvn --version
```

See `docker/README.md` for env vars and mounts.

## Team coordination

Parallel agent/worker coordination lives under `team/`. Start with:

- `team/board.md` — active execution board (one line per active track).
- `team/MANIFEST.md` — top-level pointers.
- `team/CONDUCTOR.md` — conductor role + daily checklist.
- `team/OWNERSHIP.md` — file-glob → owning track.
- `team/CONFLICT_ZONES.md` — single-writer file registry.
- `team/contracts/<track>.md` — per-track contract (allowed/forbidden paths,
  interfaces, verification, acceptance).
- `team/briefings/_template.md` — sync-before-work ritual.

Process spec: `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`.

Run `bash scripts/board-lint.sh` before pushing a contract edit.

The dated 2026-05-13 execution board is archived under
`team/archive/2026-05-16-migration/`. Do not revive it as live work.

## Active plans

See `docs/superpowers/plans/` and `docs/superpowers/specs/` for executable
implementation plans/specs. The current wave intake is
`team/intake/2026-05-16-eval-review-and-v2a.md`.

Next-wave roadmap source: `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`.
The conductor decomposes one wave at a time; do not freelance contracts from
that list without going through intake.

## Frontend UI rule: no popups

The dashboard SPA does not use popups, modals, sheets, popovers, or any
overlay that steals focus or paints over the primary surface.
Confirmations, detail views, agent windows, settings panels, error
recovery flows, share dialogs — everything routes, docks, rails,
accordions, tabs, or inline-expands.

Exceptions:
- Toasts (transient, non-focus-stealing feedback). Allowed.
- Native browser primitives we cannot reasonably replace (file picker,
  print dialog). Avoid where possible; do not invent new ones.

Why: popups destroy the spatial mental model of the app, are hostile to
keyboard navigation, deep-linking, and screen-sharing, and are a sign of
weak information architecture — the question they answer should have a
home in the actual layout.

Adopted 2026-05-17 via
`docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`.
A separate track will audit existing `Dialog`/`Modal`/`Sheet`/`Popover`
usage in `frontend/web/src/` and migrate each.
