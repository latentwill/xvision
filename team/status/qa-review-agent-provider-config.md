# qa-review-agent-provider-config — status

**Contract:** `team/contracts/qa-review-agent-provider-config.md`
**Branch:** `task/qa-review-agent-provider-config`
**Worktree:** `.worktrees/qa-review-agent-provider-config`
**Claimed:** 2026-05-18
**Status:** in-progress

## Investigation snapshot

The user-facing error message originates here:

- `crates/xvision-dashboard/src/routes/eval/review.rs:297-306` —
  `build_dispatch_for_profile()` reads `cfg.providers` and does an
  exact-match `find(|p| p.name == profile.provider)`. When the named
  provider is absent, it returns
  `"agent profile '<id>' references provider '<provider>' which is
  not configured in Settings → Providers."` and the dashboard route
  surfaces it as `ApiError::Validation` (HTTP 400/422).

The `agent_profiles` table is seeded by `crates/xvision-engine/migrations/016_eval_reviews.sql:109`
and pins `research-agent` (and friends) to `provider = 'anthropic'`.
That seed isn't operator-tunable in V1, so any dashboard without an
Anthropic provider configured can't run the review pass.

## Chosen fallback (provider-aware default)

When `cfg.providers` doesn't contain `profile.provider` but contains
at least one other provider, log `tracing::warn!` naming the
requested provider and the chosen fallback, then resolve dispatch
against the fallback. When `cfg.providers` is empty, return a
clearer `ApiError::Validation` ("no LLM provider configured in
Settings → Providers") instead of naming a specific provider the
operator may not even know is referenced.

Rationale: the operator triggered the review; silently skipping or
declining to run is hostile. A warn-on-substitute keeps the review
running while making the divergence visible in server logs.

## Out-of-scope confirmations

- No new agent-run-observability event variants. Substitution is a
  `tracing::warn!` only.
- No engine-side changes. The fix is entirely in
  `crates/xvision-dashboard/src/routes/eval/review.rs`.
- No frontend changes.
- No migration changes; the seed data stays as-is.

## Checkpoints

- 2026-05-18 — investigation + contract path correction landed via
  conductor PR #254.
- 2026-05-18 — worker branch created.
- 2026-05-18 — fix landed: `build_dispatch_for_profile` falls back
  to first configured provider with `tracing::warn!` substitution;
  clearer error when zero providers configured.
- 2026-05-18 — 11 tests pass in `routes::eval::review::tests`
  (including 2 new regression tests). Pre-existing failures in
  `crates/xvision-dashboard/tests/http.rs` (4 tests) and 18 clippy
  errors in the crate's existing code are unchanged by this PR
  (verified against `origin/main`).

## PR

(pending)
