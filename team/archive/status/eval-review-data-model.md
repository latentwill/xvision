---
track: eval-review-data-model
worktree: /Users/edkennedy/Code/xvision/.worktrees/eval-review-data-model
branch: codex-eval-review-data-model
phase: review
last_updated: 2026-05-16T03:00:00Z
---

# What I'm doing right now

Opening PR for the Eval Review Agent persistence foundation
(`docs/superpowers/specs/2026-05-15-eval-review-agent.md`). Scope is the
data-model layer only — engine, API, CLI, and UI tracks remain
downstream.

# Blocked on

nothing

# Next up

Hand off to the `eval-review-agent-engine` track, which can now build
review payload assembly + strict-JSON validation on top of the
persisted `agent_profiles` / `eval_reviews` / review-linked
`eval_findings` rows.

# Delivered

- Migration `016_eval_reviews.sql` (+ down): new `agent_profiles` table
  seeded with `fast-trader-agent` / `reasoning-agent` / `risk-agent` /
  `research-agent`; new `eval_reviews` table linked to `eval_runs` and
  `agent_profiles`.
- Migration `017_eval_findings_review_columns.sql` (+ down): adds
  `eval_review_id`, `type`, `confidence`, `title`, `description`,
  `recommendation`, `created_at` to `eval_findings`. All nullable so the
  existing extractor stays compatible. Loader gates on `eval_review_id`
  column presence (SQLite lacks `ALTER TABLE ADD COLUMN IF NOT EXISTS`).
- `EvalReview`, `AgentProfile`, `ReviewStatus`, `ReviewVerdict` types in
  `crates/xvision-engine/src/eval/review.rs`. Extended `Finding` with
  the v2 optional columns (serde `skip_serializing_if = "Option::is_none"`
  so legacy wire rows look unchanged).
- `RunStore` methods: `create_review`, `get_review`,
  `list_reviews_for_run`, `begin_review_running`, `complete_review`,
  `fail_review`, `list_agent_profiles`, `get_agent_profile`,
  `read_findings_for_review`. `record_finding` / `read_findings` now
  round-trip the v2 columns when populated.
- Tests in `tests/eval_review.rs` (13 cases): seed presence and
  idempotency, enabled-only filter, status machine, fail path,
  list ordering, legacy/v2 finding round-trip, review-scoped finding
  read, status/verdict enum round-trip. Existing
  `tests/eval_findings.rs` + `tests/api_eval_compare.rs` updated to
  apply 015/016/017 and to seed runs with the correct status-machine
  state.

# Verification

```
cargo test -p xvision-engine \
  --test eval_review --test eval_findings --test api_eval_compare \
  --test eval_store --test eval_progress --test eval_attestation \
  --test eval_executor_paper --test api_eval
```

All targeted suites pass. `api_eval_run` and `api_eval_attest` retain
pre-existing failures on `main` unrelated to this scope (test helpers
seed runs as `Completed` before calling `finalize`, which hits the
status-machine guard; and `api_eval_run` is missing migration 015 in
its helper). Out of scope for this track.
