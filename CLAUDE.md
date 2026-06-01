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

### Operator-facing names (autooptimizer subsurface)

**Top-level name (locked 2026-06-01, autoresearcher → optimizer rename).**
The subsystem formerly called the "autoresearcher" has two names:

| Surface | Name |
|---|---|
| Developer-surface (Rust module `autooptimizer/`, types `AutoOptimizer*`, SQLite tables `autooptimizer_*`, HTTP routes `/api/autooptimizer/*`, frontend `features/autooptimizer/`, MCP types, internal field names) | `autooptimizer` / `AutoOptimizer` |
| Operator-surface (dashboard nav/menu + page titles, CLI verb `xvn optimizer`, SSE display labels, MANUAL.md, dashboard wiki) | **Optimizer** |

The codename is deliberately `autooptimizer` (NOT `optimizer`) to stay
distinct from the **pre-existing, unrelated DSPy prompt-optimizer**
(`xvision-dspy`, the engine `optimization/` module, `optimization_*`
tables, the `xvn optimize` verb, `Optimizer`/`OptimizerKind`/`OptimizerModel`/
`OptimizerResult`). Never rename DSPy `optimize`/`optimization`/`Optimizer*`
tokens, and never let the autooptimizer codename collapse into bare
`optimizer` in code.

The autooptimizer, memory, and flywheel surfaces follow a two-name
convention: developer-surface names (in Rust types, SQLite columns,
spec docs, API field names) stay precise and technical, while
operator-surface names (in CLI flags and help text, UI labels, SSE
display labels, MANUAL.md, dashboard wiki) are plain-language. The
two-name pairs are locked at
`docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`.

Examples: `Mutation` → "Experiment"; `Mutator` → "Experiment writer";
`LineageStatus::Ghost` → "Rejected"; `LineageStatus::Quarantined` →
"Suspect"; `CycleSeal` → "Evening summary"; `Merkle root` → "Cycle
proof"; `--gate-epsilon` → `--min-improvement`; `--parent-holdout-score`
→ `--baseline-untouched-score`; null-result canary → "honesty check".

Any new operator-facing concept on these surfaces requires a row in
the lock doc. Cryptographic primitives (BLAKE3, Ed25519, "merkle,"
canonical JSON) must never appear on an operator surface.

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

### Disk hygiene (shared cargo target — check & self-clean)

The shared cargo target dir has no garbage collection: `target/debug/deps`
accumulates artifacts from every branch/profile ever built across all the
concurrent agents and grows tens of GB until the volume fills and unrelated
builds fail with `No space left on device`.

Two guards keep it bounded:

- **`incremental = false`** in `.cargo/config.toml` — stops the
  `target/debug/incremental` dir (which has spiked to 30+ GB) from forming.
- **`scripts/cargo-disk-guard.sh`** — checks free space and, when below the
  threshold (`XVISION_DISK_MIN_GB`, default 12), self-cleans in tiers:
  incremental → `cargo-sweep` artifacts >7 days old → full deps/build drop as a
  last resort. Run it any time; it's a no-op when space is fine.

**Build through the wrapper, not bare cargo**, so the guard runs first:

```bash
scripts/cargo build --workspace      # guards, then execs real cargo
scripts/cargo test  -p xvision-engine
scripts/cargo-disk-guard.sh --check  # report free space (exit 3 if low)
```

The `pre-commit` hook also prints a non-blocking low-space warning. None of this
nukes source — only rebuildable artifacts.

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

## Worktree isolation (enforced)

This clone is shared by multiple concurrent agents (claude, codex, 100x runs,
human). **Never do branch/feature work in the main checkout
(`/Users/edkennedy/Code/xvision`).** Checking out a branch there while another
agent is working causes HEAD/branch collisions, surprise force-pushes, and
tangled auto-commits. The main checkout stays on `main` for pulling/inspection
only.

All branch work — including overnight agents and `100x run` — must happen in an
isolated worktree:

```bash
git worktree add .worktrees/<name> -b <branch>     # or agent-conductor (worktreeRoot=.worktrees)
cd .worktrees/<name>
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"   # avoid duplicate target/ trees
```

This is enforced by a tracked `pre-commit` hook (`.githooks/pre-commit`) that
blocks commits on non-`main` branches in the primary checkout. Activate it once
per clone (the setting is shared across all worktrees):

```bash
scripts/setup-hooks.sh        # sets core.hooksPath=.githooks
```

Deliberate, rare main-checkout commits: `XVISION_ALLOW_MAIN_COMMIT=1 git commit …`.

Also: `100x run` auto-commits/pushes and can trigger an auto-PR+merge, and it
sweeps any uncommitted WIP into its commit — run it from a clean worktree, never
the main checkout with live WIP.

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

## Frontend layout rule: no right-side boxes when the chat rail is visible

The dashboard SPA's desktop shell is a three-pane grid: left sidebar, center
column, right chat rail (`DesktopThreePaneShell`,
`grid-cols-[220px_minmax(0,1fr)_auto]`). Detail pages must NOT add a
fourth column / right sidebar / floating side card. The chat rail
already occupies the right edge; a `col-span-4` sidebar on top of it
shrinks the center column where the chart, decisions, and primary
content live.

Practical rule:

- The default detail-page layout is a single full-width column
  (`space-y-5`, no `grid-cols-12`).
- Auxiliary boxes (META / run config strips, review panels, action
  rows, persona pickers) go INLINE — above or below the center
  content, full-width, ideally as a horizontal row of chips rather
  than a stacked card.
- Side panels are allowed on routes that don't carry the chat rail
  (e.g. the chart-lab playground, settings, or pre-rail standalone
  views). Anywhere `<Layout>` is rendered, treat the right column as
  reserved.
- If you need to surface contextual help/reviews that don't merit a
  full strip, route them through a dock or accordion (consistent with
  the no-popups rule below).

Adopted 2026-05-26 (QA30). The first migration sweep moved the
eval-runs-detail page off `grid-cols-12 lg:col-span-8 / lg:col-span-4`
to a single-column layout. Future PRs that introduce a sidebar must
either route around the chat rail entirely or convert to an inline
strip.

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
- `<MListSheet>` (mobile list filters only) — bottom sheet rendered
  under `<MListCard>` on the phone breakpoint. Operator-approved
  2026-05-20 in `docs/superpowers/specs/2026-05-20-standard-list-component.md`
  Decision 3. Scoped to this single component; the rule continues to
  apply everywhere else.

Why: popups destroy the spatial mental model of the app, are hostile to
keyboard navigation, deep-linking, and screen-sharing, and are a sign of
weak information architecture — the question they answer should have a
home in the actual layout.

Adopted 2026-05-17 via
`docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`.
A separate track will audit existing `Dialog`/`Modal`/`Sheet`/`Popover`
usage in `frontend/web/src/` and migrate each.
