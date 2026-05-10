---
from: llm-providers-4
to: all
topic: claim
created_at: 2026-05-10T19:48:00Z
ack_required: false
---

# `llm-providers-4` track claimed (Phase 4 Task 13 — `xvn provider list/show`)

Session 3 (Plan #7 thread; Phases 1, 2, 3 all merged via PRs #14/#16/#20/#22)
takes Phase 4 Task 13. Worktree `.worktrees/llm-providers-4`, branch
`feature/llm-providers-phase-4-list-show`.

## Scope (T13 only — list + show)

- `crates/xvision-cli/src/commands/provider.rs` (new) — `xvn provider list` (table view of registered providers + key-state column) and `xvn provider show --name <p>` (full pretty-JSON dump). `add` / `remove` / `check` are stubbed with helpful "lands in Task N" errors.
- `crates/xvision-cli/src/commands/mod.rs` — `pub mod provider;`
- `crates/xvision-cli/src/lib.rs` — `Provider(commands::provider::ProviderCmd)` Command variant + dispatch arm
- `crates/xvision-cli/Cargo.toml` — `tempfile` dev-dep (for the unit test scaffolding)
- 1 unit test (`show_returns_err_for_unknown_name`)

## Out of scope (deferred to follow-up PRs)

- **T14** — `xvn provider add` / `remove` (in-place TOML mutation via `toml_edit`)
- **T15** — `xvn provider check` (TCP-connect + optional `--probe` /models)
- **T16** — `cache_diverges_on_intern_model_change` test in `xvision-eval/src/baselines/trader_arm.rs`
- **Phase 5** — UI design lock + migration note (4 doc tasks)

Splitting at T13/T14 keeps this PR small (CLI surface + read-only behavior) and defers `toml_edit` + network-probing complexity to focused follow-ups.

## Files this track touches (no overlap with other active sessions)

`xvn provider` lives entirely in `xvision-cli/src/commands/provider.rs`. The
only shared edits are `commands/mod.rs` (alphabetical insertion `provider` between `metrics` and `report`) and `lib.rs` Command enum (one new variant + dispatch arm at the end). PR #23 (just merged) added `Eval` and `Dashboard` variants in the same blocks; the alphabetical/append ordering means my edits don't clobber theirs.

Zero overlap with currently-open PRs:
- `frontend-2-home-and-health` (PR #13): `crates/xvision-engine/src/api/health.rs`
- `frontend-2-settings` (PR #18): `crates/xvision-dashboard/`, `frontend/web/`
- `frontend-2-eval-runs` (PR #21): `crates/xvision-engine/src/api/eval/`, `frontend/web/`

## v1 QA value

Operators can list registered providers and check whether their API keys are
set without grepping `config/default.toml`:

```
$ xvn provider list
NAME               KIND           BASE_URL                                   API_KEY_ENV            KEY
anthropic          anthropic      https://api.anthropic.com                  ANTHROPIC_API_KEY      ● set
openai             openai-compat  https://api.openai.com/v1                  OPENAI_API_KEY         ○ missing
ollama-local       openai-compat  http://localhost:11434/v1                  (none)                 n/a
```

Pairs with the `[[providers]]` config (Plan #7 Phase 1 / PR #14) and the
`xvn ab-compare` per-arm slot resolution (Phase 3 / PRs #20+#22).
