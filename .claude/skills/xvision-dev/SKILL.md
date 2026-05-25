---
name: xvision-dev
description: Orient an agent CONTRIBUTING CODE to the xvision repo — building, testing, navigating the Rust workspace + Vite SPA, following the team-coordination board, respecting deployment guardrails, and not breaking locked-in invariants. Use when the task involves editing crates/**, frontend/web/**, migrations, contracts, or CI/deploy scripts. Do NOT use this skill for end-user xvn CLI operation or operator runbooks — that's the `xvision` skill.
---

# xvision-dev

Engineering skill for contributors to the xvision repo. Pair it with the
`xvision` skill only when the task also touches end-user operator behaviour;
otherwise stay here.

`xvision` is a multistrategy trading-agent backtest harness — a Rust workspace
producing the `xvn` CLI binary plus a Vite SPA dashboard baked in by
`rust-embed`. Production lives behind Tailscale on `extndly-dev`.

## Where to look first (engineering docs)

Don't grep blindly. Canonical docs live in known places:

- `architecture.md` (repo root) — system shape + crate boundaries.
- `docs/superpowers/specs/` — design spec per major subsystem; pick the latest dated file matching your feature.
- `docs/superpowers/plans/` — executable implementation plans (dated). The wave being executed always has its plan here.
- `decisions/` — ADRs. ADR 0010 = 2026-05-05 marketplace pivot. ADR 0011 = CV substrate moved to xvision-play. ADR 0012 = in-app skills surface removed (use `/agents` instead).
- `FOLLOWUPS.md` — open engineering work. **F-codes** = shared track. **SLF-codes** = Strategy Loom hackathon track (branch `hackathon/turing`, deadline 2026-06-15).
- `MANUAL.md` — operator-side prerequisites; useful when adding features that need broker creds / Mantle minting.
- `docs/QA/2026-05-23-qa24-low-priority-followups.md` — parked low-priority QA24 product/API follow-ups.
- `team/MANIFEST.md` → `team/board.md` + `team/board-v2.md` — current execution board. Read before opening a new track.

## Build + test (workstation)

```bash
cargo build --workspace
cargo test --workspace
```

- Cargo from `~/.cargo/bin`.
- `xvision-identity` is opt-in (excluded from `default-members`). Build it explicitly: `cargo build -p xvision-identity`.
- Frontend lives under `frontend/web/` (Vite + TanStack Query + Radix + Tailwind + shadcn). `pnpm build` populates `crates/xvision-dashboard/static/`, which `rust-embed` bakes in at compile time.

### Local build-cache discipline

Rust artefacts grow fast. If you're working from a temporary worktree (under
`.worktrees/<slug>` or `/tmp/...`), redirect cargo's output instead of
creating another `target/` tree:

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
```

Never commit, archive, or preserve `target/` directories — they're rebuildable.
Only the main checkout root's `target/` is retained, and only while a build/deploy
image is being prepared.

## Crate layout (one-line each)

| Crate | Role |
|---|---|
| `xvision-core` | Shared types — `Strategy`, `Algorithm`, `Cycle`, `Briefing`, `TraderDecision`, `RiskDecision` |
| `xvision-engine` | Engine + API surface (`src/api/`), backtest runner, settings, search, bundle store, migrations |
| `xvision-cli` | `xvn` binary; subcommands under `src/commands/*.rs`, registered in `src/lib.rs` |
| `xvision-dashboard` | axum HTTP server + embedded SPA; routes in `src/routes/*.rs` |
| `xvision-eval` | Eval harness — A/B compare, baselines, gate logic |
| `xvision-intern` | Intern backends (`OpenAICompatIntern`, `AnthropicIntern`) |
| `xvision-mcp` | stdio MCP tool surface (indicators + health) |
| `xvision-execution` | Venue executors (Alpaca, Orderly) |
| `xvision-identity` | ERC-8004 IdentityRegistry / ReputationRegistry client (opt-in) |
| `xvision-observability` | Apache-2.0 observability crate; schema, redactor, blob store, event bus, retention/janitor. Owns `UnifiedEvent` (adjacently-tagged `{kind,data}`) — the chat-rail/trace-dock unified taxonomy. |
| `xvision-dspy` | **Offline-only** DSPy optimizer crate. **Excluded from `default-members`** so its ~93-package tree (`dspy-rs`, `rig-core`, arrow/parquet, foyer, hf-hub) never reaches the engine/dashboard/runtime image. |

Full pipeline + storage layout in [`references/architecture.md`](references/architecture.md).

## Team coordination (multi-agent workflow)

Multiple Claude sessions land work in parallel via `team/`:

- `team/board.md` — active execution board, one line per active track. Conductor-owned.
- `team/board-v2.md` — V2 roadmap board (V2A active, V2B+ not yet decomposed).
- `team/contracts/<slug>.md` — per-track contract (allowed_paths, forbidden_paths, status). Conductor owns frontmatter; worker owns body.
- `team/status/<slug>.md` — worker-owned current status. Read this before claiming a contract.
- `team/queue/<from>__<utc>__<topic>.md` — append-only inter-track messages.
- `team/OWNERSHIP.md` + `team/CONFLICT_ZONES.md` — file-glob owners and single-writer regions.
- `team/CONDUCTOR.md` — conductor role and daily checklist.

**Before pushing a contract edit:** `bash scripts/board-lint.sh` (must be green).
**Before starting a new track:** read the contract, read the briefing template
(`team/briefings/_template.md`), then write `team/status/<slug>.md` and flip the
contract status to `claimed`.

Process spec: `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`.

## Migrations

Migration numbers are reserved in `team/MANIFEST.md`'s Migration
registry. The conductor must register the next number before any
track edits `crates/xvision-engine/migrations/`. The chat-rail/DSPy/
strategy-agents wave landed **041–045** on its branch: 041 chat-session
rail-state columns, 042 session event log, 043 tool policies, 044
`chat_checkpoints` (named to avoid colliding with the migration-018
agent-run replay `checkpoints` table), 045 optimization store
(`optimization_runs` / `candidates` / `demos` / `snapshots` /
`agent_lineage`). Each is wired into `ApiContext::open` via a guarded,
idempotent `migrate_*` helper. Coordinate the next number via the board,
not by grabbing the next free integer — the 021/022/023/024 renumber
dance happened twice in two weeks because parallel tracks collided.

Every migration ships its `_down.sql` counterpart. Schema changes go
through the engine's migration system, not raw `psql`.

There are two migration dirs:
`crates/xvision-engine/migrations/` (engine-owned: cycles, briefings,
decisions, eval runs, scenario regime labels, experiments, …) and
`crates/xvision-core/migrations/` (core-owned: `0001_init`,
`0002_rename_setup_to_cycle`). Add a file to **one**, not both — the
runner reads each crate's dir for its own scope.

## Terminology (locked 2026-05-10, Option B rename)

Diverging from these names requires a written rationale. See
`/CLAUDE.md` for the full table; the load-bearing ones for code work:

| Concept | Use this | Don't use |
|---|---|---|
| Per-decision-cycle id | `cycle_id` | `setup_id` |
| Pre-mint strategy id | `agent_id` (ULID, becomes NFT token id) | `strategy_id` |
| Pipeline-config artifact | `Strategy` | `StrategyBundle`, `bundle` |
| Eval-baseline decision producer | `Algorithm` | `Strategy` |
| Cycles DB table | `cycles` | `setups` |
| Strategy's reference to a library agent | `AgentRef { agent_id, role }` | — |

Pipeline-role names (intern → trader → risk → executor) are
*conventions*, not hardcoded fields. After the 2026-05-12 strategies
refactor, slot names per `AgentRef` are free text. `AgentSlot` carries
an optional `temperature` field that is threaded through every call
site (commit `ad9b1f7`); any new agent-slot consumer must honor it.

## Deploy paths (two; local-build is preferred)

### Preferred — local image build, ship over SSH

Builds the Rust workspace + Vite SPA on your local machine and streams a
~150 MB runtime image to the target. Avoids GitHub Actions minutes and
avoids OOM on the 3.7 GiB `extndly-dev` host.

```bash
scripts/deploy-image.sh                          # build only
scripts/deploy-image.sh --push root@host         # build + transfer + docker load
scripts/deploy-image.sh --with-identity          # include xvision-identity (Mantle)
scripts/deploy-image.sh --platform linux/arm64   # ARM hosts
```

`--push` runs a remote-disk preflight before transferring the image
and a post-load cleanup after. Driven by the 2026-05-20 incident where
deploy succeeded at image-load but `xvn-app` entered a restart loop
because `/` was at 100% and SQLite couldn't write the migration
journal (PR #377, commit `8fd7d48`).

- **Preflight**: `ssh <host> df -P /` and refuse at ≥95% used (warn
  at ≥85%). Refusal prints the common reclaim targets:
  `journalctl --vacuum-size=200M`, `docker image prune -f`,
  inspecting `docker images xvision`, deleting stale
  `/root/deploy/xvision/.worktrees/*/target/` trees.
- **Cleanup**: post-load, the script drops the prior
  `:deploy-latest`'s sha tag iff (a) it points at a different image
  than what was just loaded **and** (b) no container still
  references it. Other `xvision:*` tags (including whatever
  `xvnej-app` is pinned to and `ghcr.io/*` mirrors) are untouched.

Env overrides:

```
XVN_DEPLOY_DISK_REFUSE_PCT=95      # default
XVN_DEPLOY_DISK_WARN_PCT=85        # default
XVN_DEPLOY_SKIP_DISK_CHECK=1       # bypass preflight
XVN_DEPLOY_SKIP_CLEANUP=1          # keep prior :deploy-latest tag
```

The same Hetzner host backs `xvn` (dev) and `xvnej` (prod); see
`project_xvn_xvnej_environments.md` + `project_xvn_host_disk_pressure.md`
(user memory).

### Fallback — GHCR via GitHub Actions

Use only when there's no local build host capable of the full Rust+Vite build.

```bash
scripts/deploy-ghcr.sh
# or
gh workflow run docker.yml --ref main \
  -f dockerfile=Dockerfile.deploy \
  -f build_identity=false \
  -f build_profile=release
```

Full deploy mechanics + pitfalls in [`references/deploy.md`](references/deploy.md).

## Hard guardrails (don't violate)

- **No `cargo` on remote/deploy hosts.** `extndly-dev` (3.7 GiB) has no toolchain and will OOM. Build locally or via GHCR.
- **No Docker image builds on remote/deploy hosts.** Build locally; ship the image.
- **`source .op_env` before `gh` / `op`** so GitHub + 1Password access come from the expected env.
- **Push workflow-file changes with the classic PAT** (1Password `Olympus / Github Classic Token (No Admin/Delete)`). Default `gh` auth on `extndly-dev` lacks `workflow` scope.
- **A/B cache pairing is tier-1.** Cache keys pair per `cycle_id` (formerly `setup_id`). Backtests / A/B compare require a deterministic intern backend — use `OpenAICompatIntern` or `AnthropicIntern`.
- **No DB mocks in integration tests.** Production migrations need real exercise — mocked tests have masked broken migrations before.
- **No backwards-compatibility shims for pre-rename names.** The setup→cycle rename was a pre-launch breaking change; don't re-introduce `setup_id` aliases.
- **Dark mode borders:** never `border-white` / `border-gray-100/200` / `#fff` on cards. Use `border-border` or muted tones with `dark:` variants. (Workspace-wide rule from `/CLAUDE.md`.)
- **Errors must be root-caused**, not silenced with try/catch or API-contract shims (user feedback memory).
- **Don't dedupe / normalize provider model lists.** Fix rendering instead (user feedback memory).
- **Strategy inspector canonical route:** new links should use `/strategies/:id`; `/authoring/:id` is a compatibility alias only.
- **Real filters are artifacts.** A prompt that says "filter" is not enough. Filter QA must attach a strategy filter and inspect filter summaries/events.
- **Eval decision provenance matters.** Keep direct model decisions distinguishable from `noop_skip`, graph-gated, and early-stop synthesized rows in UI/API work.
- **The engine and dashboard must stay DSPy-free.** Optimizer logic lives in `xvision-dspy` (excluded from `default-members`). The `optimization` store persists snapshots/demos as **opaque JSON**; `accept-as-child-agent` crosses the boundary as a plain instruction string only. Anything that makes `cargo tree -p xvision-engine` or `-p xvision-dashboard` show `dspy-rs`/`rig-core`/`xvision-dspy` is a regression. The live LLM path is raw `reqwest` (`LlmDispatch`) + the Cline sidecar — there is no workspace `rig-core` to "match"; it only enters transitively through `dspy-rs`.
- **Chat-rail Research/Act is server-enforced.** Write-tool gating reads the **persisted session mode column** before execution; never trust a client-asserted mode, and never let a write tool run in research mode. Tool policy is three-state `(enabled, auto_approve)` = Disabled/Ask/Auto with read=Auto, write=Ask class defaults; an unknown tool fails safe to write.
- **Optimizer accept is holdout-disciplined.** A snapshot selected on train-only data (no holdout split) must be refused at accept time; accept clones the parent (never mutates it) and records an `agent_lineage` edge.
- **Focus writes are path-confined.** `focus.md` lives under `$XVN_HOME/scopes/<kind>/<id>/`; reject absolute/`..`/separator/empty/NUL components before any I/O so a write can't escape `scopes/`.
- **`UnifiedEvent` is adjacently tagged** (`{ "kind", "data" }`), not internally tagged — several reused detail structs carry their own `kind` field and would collide. Keep the TS mirror and any new payload variant on the same `{kind,data}` shape; never silence a typed `Error*` event.
- **Conductor stays out of feature code.** If you're acting as conductor, only edit `team/**` and `scripts/board-*`. Otherwise you're a worker — claim a contract first.

## Deeper references

- [`references/architecture.md`](references/architecture.md) — crate layout, pipeline stages, storage layout, dashboard data flow.
- [`references/deploy.md`](references/deploy.md) — local-image and GHCR mechanics, the xvn / xvnej tailscale stacks, every pitfall that has actually bitten.
- [`references/team-workflow.md`](references/team-workflow.md) — contract / status / queue lifecycle and the daily conductor checklist.

---

*Skills owner: any track that changes the build/test/deploy story or
adds a load-bearing invariant is responsible for updating this file in
the same PR. Last refresh: 2026-05-24 (chat-rail safety invariants,
offline-only `xvision-dspy` crate, migrations 041–045).*
