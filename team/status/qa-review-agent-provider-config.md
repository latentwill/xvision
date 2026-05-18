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

## Chosen path (same-kind substitution + skip-with-remediation)

Initial implementation (first PR revision) used "first configured
provider" which fails at the wire layer when the substituted provider
is a different `ProviderKind` than the seeded profile's model id
expects (e.g. dispatching `claude-sonnet-4-6` to an OpenAI-compatible
endpoint → 404). The `local-candle` test masked the bug because mock
dispatch ignores the model.

Revised resolution order:
1. Exact provider-name match wins (no regression path).
2. Same-`ProviderKind` substitution: when the named provider isn't
   configured but a configured provider has the same kind (e.g.
   operator's "anthropic-prod" key resolves the seeded
   "anthropic"-pinned profile), substitute it with a
   `tracing::warn!` naming the substitution. Model ids remain valid
   because wire format matches.
3. Otherwise, return a clearer `ApiError::Validation` listing the
   configured providers and naming the requested provider so the
   operator knows what to add (e.g. "configured: openrouter, openai
   — add an `anthropic`-kind provider to run this review").

Cross-kind substitution is explicitly refused. The
`inferred_kind_for_provider_name` helper hardcodes the
name→kind convention for known providers (`anthropic`, `openrouter`,
`openai`, `openai-compat`, `local-candle`) and returns `None` for
operator-defined names so unknown names fall through to the skip path
rather than getting silently guessed at.

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
- 2026-05-18 — first revision landed (first-configured-provider
  fallback) at PR #256.
- 2026-05-18 — review feedback: cross-kind substitution would send
  Anthropic model ids to OpenAI endpoints; `local-candle` test
  masked the bug. Revised to same-kind substitution +
  skip-with-remediation. Contract updated to remove the
  acceptance ambiguity (PR #254).
- 2026-05-18 — 13 tests pass in `routes::eval::review::tests`
  (3 new + 1 unit test). Pre-existing failures in
  `crates/xvision-dashboard/tests/http.rs` (4 tests) and 18 clippy
  errors in the crate's existing code are unchanged by this PR
  (verified against `origin/main`).

## PR

(pending)
