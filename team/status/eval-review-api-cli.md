---
track: eval-review-api-cli
worktree: .worktrees/eval-review-api-cli
branch: task/eval-review-api-cli
phase: pr-open
last_updated: 2026-05-16T18:30:00Z
owner: claude-opus-4-7
---

# What I'm doing right now

PR ready. Dashboard routes + `xvn eval review` CLI both implemented and
green; engine review service plumbed end-to-end. All acceptance items
satisfied.

# Blocked on

Nothing. Conductor flip on `team/contracts/eval-review-api-cli.md`
`status: ready` → `pr-open` once the PR opens. The contract frontmatter
is conductor-owned; not touched from the worker side.

# Next up

Hand off to `eval-review-run-detail-ui` (the leaf that consumes these
routes from the SPA). The dashboard JSON shape is now stable:
`POST /api/eval/runs/:id/review { agent_profile_id, force? }` and
`GET /api/eval/runs/:id/reviews` / `GET /api/eval/reviews/:id` both
return `{ review, findings }`.

# Verification results

- `cargo test -p xvision-dashboard --lib routes::eval::review` — 9 passed.
- `cargo test -p xvision-cli --lib eval::review` — 7 passed.
- `cargo build -p xvision-dashboard -p xvision-cli` — clean.
- `cargo check -p xvision-dashboard -p xvision-cli` — clean (3 pre-existing
  dead-code warnings in `xvision-engine/src/api/eval.rs` test helpers,
  1 in `xvision-dashboard/src/wizard_loop.rs::wizard_tool_defs`, and a
  deprecated-`canonical_scenarios` warning in two test modules — all
  upstream of this track).
- `bash scripts/board-lint.sh` — 2 violations remain on
  `q15-eval-json-export` (branch/worktree drift); both pre-date this
  track and aren't in our allowed_paths. No new violations introduced.

# Surface area

Dashboard:
- `crates/xvision-dashboard/src/routes/eval/review.rs` (new) —
  `POST /api/eval/runs/:id/review`, `GET /api/eval/runs/:id/reviews`,
  `GET /api/eval/reviews/:id`. Idempotency skips `ReviewStatus::Failed`
  so a transient dispatch error doesn't permanently pin the operator
  to a failed row. Resolves `ReviewScenarioSummary` from the run's
  `scenario_id` so the engine payload carries id / name / asset /
  granularity / window context.
- `crates/xvision-dashboard/src/routes/eval/mod.rs` — route
  registration only.
- `crates/xvision-dashboard/src/routes/mod.rs`, `server.rs` — wire the
  new sub-router; `eval` is now a `mod` rather than a single file.
- `crates/xvision-dashboard/src/wizard_loop.rs` — touched only by the
  module-split rename (no behavior change).

CLI:
- `crates/xvision-cli/src/commands/eval/review.rs` (new) — `xvn eval
  review <run_id> --agent <profile> [--force] [--output <path>]
  [--format human|json]`. `--format` is a typed `clap::ValueEnum`
  (Human, Json) so unknown values error at parse time with a clap-rendered
  "possible values" message instead of silently falling back. Implies
  `--format json` when `--output` is set. Typed exit codes via
  `XvnExit`: missing run / missing profile → distinct codes.
- `crates/xvision-cli/src/commands/eval/mod.rs` — promoted `eval.rs`
  to a directory module with `review` subcommand.
- `crates/xvision-cli/tests/strategy_cli.rs` — adjusted one path the
  module split shifted.

Tests:
- 9 dashboard route tests (post happy-path, idempotency including
  `Failed` retry, scenario-summary resolution, unknown-profile 404,
  list newest-first, get-by-id, get unknown 404, scenario fallback to
  `None` on unknown run).
- 7 CLI tests (format-enum parse error, dispatch builder, unknown run,
  unknown profile, idempotency skipping failed, scenario-summary
  resolver, end-to-end through local-candle stub).
