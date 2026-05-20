---
name: cycle-migration
description: Authoring SQLx migrations or schema changes in the xvision repo. Enforces the 2026-05-10 terminology lock (Strategy/Agent/cycle_id), the dual-migration-dir layout (xvision-core + xvision-engine), and migration safety conventions. Use when adding a file under `crates/*/migrations/`, renaming a column, or changing the cycle/agents/strategies schema. Read this skill before writing any `*.sql` under crates/.
user-invocable: false
---

# cycle-migration

Background guidance for schema changes in xvision. Two migration roots exist
and are independently applied; the terminology rename of 2026-05-10 locked
several names that future migrations must respect.

## Migration roots

- `crates/xvision-core/migrations/` — core trading entities (cycles,
  briefings, decisions, risk_outcomes, executions, traces, strategies, agents).
- `crates/xvision-engine/migrations/` — engine-side state (run history,
  scheduler bookkeeping). Do NOT put trading-domain tables here.

Pick the crate whose runtime owns the data. If unsure, ask the user before
adding a migration — adding to the wrong root means two databases drift.

## Naming

- Files: `NNNN_snake_case_description.sql` (4-digit zero-padded, monotonic
  within the crate's `migrations/` dir).
- One forward migration per file. No down-migrations — sqlx-migrate doesn't
  run them and we don't promise reversibility.
- New tables/columns MUST use the locked terminology (next section). PR
  reviewers will block migrations that reintroduce retired names.

## Locked terminology (from CLAUDE.md, 2026-05-10)

| Use | Forbidden |
|---|---|
| `cycle_id`, table `cycles` | ~~`setup_id`~~, ~~`setups`~~ |
| `agent_id` (string ULID) | ~~`strategy_id`~~ (when referring to the pipeline) |
| Table `strategies` (immutable pipeline config) | ~~`strategy_bundles`~~, ~~`bundles`~~ |
| Table `agents` + `agent_slots` (per-prompt+model+skills) | ~~`agent_templates`~~ |
| FK column `cycle_id` (NOT `setup_id`) | ~~`setup_id`~~ in any new table |
| `cycles_evaluated` (eval result count) | ~~`setups_evaluated`~~ |
| `PerStrategyVerdict` (wallet plan verdict) | ~~`Verdict`~~ (collides with `RiskDecision`) |

The slot-name fields (`intern`, `trader`, `risk`, `executor`) are conventions
only — DO NOT add CHECK constraints or enums that enforce them. Slot names
are user-defined free text per `Strategy { agents: Vec<AgentRef> }`.

Pre-rename baseline tag: `pre-rename-baseline`. Reference for what the
schema looked like before; not a target to revive.

## Migration safety

- SQLite (sqlx 0.8, runtime-tokio). Some PRAGMAs differ from Postgres; don't
  copy/paste Postgres migration patterns without testing.
- For column renames: SQLite needs the
  `ALTER TABLE ... RENAME COLUMN` form (>= 3.25). Workspace MSRV is fine.
- For table renames or schema rewrites where multiple FKs point in, follow
  the pattern in `0002_rename_setup_to_cycle.sql`:
  1. `PRAGMA foreign_keys = OFF;` at top.
  2. Rename the parent table.
  3. Rename FK columns in each child table (six tables in that migration).
  4. `PRAGMA foreign_keys = ON;` at bottom.
- NEVER drop a column that older code paths still read. Stage the deprecation
  across two releases if necessary; the workspace doesn't have feature-flag
  scaffolding so a single migration must remain backward-compatible with the
  prior binary for the rollout window.
- Verify with `cargo test -p xvision-core` (or `-p xvision-engine`). Both
  crates run their migrations on test setup.

## Rollout

- Bump `version` in `crates/xvision-core/migrations/README.md` (or engine
  equivalent) if it exists.
- Mention the migration in the PR body so the conductor can update the
  deploy notes.
- On the deploy host, migrations run on container start — do NOT try to
  apply them by hand via `sqlx-cli`. Deploy hosts have no Rust toolchain
  and no `cargo` (see CLAUDE.md deployment guardrails).

## Related

- `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md` —
  the source of the terminology lock.
- `docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md` —
  the strategies → agents refactor; explains why `agent_slots` is a list
  not fixed fields.
- Workspace CLAUDE.md "Terminology" table — single source of truth.
