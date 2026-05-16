---
track: q15-object-json-output
worktree: .worktrees/q15-object-json-output
branch: task/q15-object-json-output
phase: in-progress
last_updated: 2026-05-16T10:18:20Z
owner: claude-opus-4-7
---

# What I'm doing right now

Implementing q15-object-json-output, the leaf that pairs with the
eval-export shape (#187 merged 2026-05-16):

1. `xvn agent get <id>` — new CLI top-level (no agent verb exists
   today). Emits the full `Agent` (with `Vec<AgentSlot>`).
2. `xvn strategy get <id>` / `xvn scenario get <id>` — add `get`
   visible-alias on the existing `show` subcommand (eval CLI already
   uses this pattern), plus a `--format` flag that defaults to `json`.
3. Shape-parity tests: serialize each via the CLI AND extract from a
   built `EvalRunExport` for the same record; structural compare.
4. Dashboard routes — `GET /api/{strategies,scenarios,agents}/:id`
   already exist and return the full struct via `Json<T>`. Add
   parity tests so a future refactor can't drift the route shape from
   the export shape silently.

# Blocked on

Nothing.

# Next up

Survey existing CLI `show` impls and confirm what they emit today;
make the change minimally additive (alias + flag).
