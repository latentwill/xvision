---
track: eval-provider-error-classify-retry
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-provider-error-classify-retry
branch: task/eval-provider-error-classify-retry
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/llm/**
  - crates/xvision-engine/src/eval/executor/mod.rs   # only the `classify_run_failure` arm
  - crates/xvision-engine/src/api/search/**
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision-engine::llm::openai_compat::OpenAiCompatError (or equivalent)
  - xvision-engine::eval::executor::classify_run_failure
parallel_safe: true
parallel_conflicts:
  - eval-run-watchdog-and-stuck-running (also reads/writes eval/executor/mod.rs — keep edits to disjoint arms; coordinate via queue note if both land same wave)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine --test llm
  - cargo test -p xvision-engine eval::executor::tests::classify_run_failure
acceptance:
  - A new typed variant `MissingChoicesArray` (or equivalent) is added to the OpenAI-compat error enum; the existing string-match in `classify_run_failure` either becomes a typed-error match or is dropped in favour of the new typed error. Today the case falls through to `[unclassified]` and the run is failed.
  - The LLM call path retries on `429` honouring `X-RateLimit-Reset` (with jitter, max 3 attempts) **and** on `MissingChoicesArray` (3 attempts, exponential backoff base 500ms). After exhausting retries the typed error is bubbled up with the retry count attached.
  - `crates/xvision-engine/src/api/search.rs` (or wherever the upsert path lives — found via `xvision_engine::api::search: search index upsert (run) failed error=delete prior row`) uses a single atomic `INSERT … ON CONFLICT DO UPDATE` query instead of the delete-then-insert that raced with eval finalize.
  - Tests:
    * Unit test that a 429 with a `X-RateLimit-Reset` header retries after the reset window and succeeds on the 2nd attempt.
    * Unit test that a 200 OK body missing `choices` is classified as `MissingChoicesArray` and retried, succeeding on retry.
    * Unit test that the upsert path runs without `delete prior row` errors when called twice in succession for the same run_id (i.e. behaves idempotently).
  - No behaviour change to other classifier arms; existing `provider_http_error` runs still fail the same way they do today when their root cause is non-transient.
---

# Scope

Intake F-2 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.
Two distinct provider-error symptoms from the audit:

1. Two runs failed with `[unclassified] OpenAI-compat response missing
   'choices' array` — a gateway transient from OpenRouter — and were
   treated as fatal because `classify_run_failure` doesn't recognise the
   string.
2. The `xvision_engine::api::search: search index upsert (run) failed
   error=delete prior row` warning shows the search-index upsert is
   non-atomic — it deletes then inserts and races with eval finalize.

Both are leaf-S fixes; bundling them keeps the surface area small.

The 429-honour-Reset behaviour is *also* listed under F-1
(`eval-launch-concurrency-and-429-backoff`). F-1 owns the global
concurrency cap + serialized-write hotspot; this contract is allowed to
land the request-level retry independently since F-1's launch-time gate
is a separate concern. Coordinate via queue note when both land.

# Out of scope

- Global concurrency cap on `eval.start` (that's F-1's job).
- Serialize-write hotspot fix on `eval_runs` finalize (also F-1).
- Replacing the provider abstraction or rewriting `openai_compat.rs`.
- Anything in `crates/xvision-engine/migrations/` — this is pure
  in-memory + code-path work.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-provider-error-classify-retry status
git -C .worktrees/eval-provider-error-classify-retry log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-provider-error-classify-retry -b task/eval-provider-error-classify-retry origin/main
```

# Notes

Append checkpoints below. Do not edit the frontmatter above the line
without a contract-update PR.
