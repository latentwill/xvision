---
track: qa-eval-retry-params-override
status: pr-open
date: 2026-05-17
worker: claude (opus 4.7)
worktree: .worktrees/qa-eval-retry-params-override
branch: task/qa-eval-retry-params-override
pr: 228
pr_url: https://github.com/latentwill/xvision/pull/228
commits:
  - c9d1a5fff7d573a5d7fc56be6a614c65cd46716c
---

## 2026-05-17 — claimed

Picked up from QA wave 2026-05-17. Implementing fix path: include
`params_override` equality in the retry idempotency sibling predicate, so the
in-flight sibling lookup matches the documented `(agent_id, scenario_id, mode,
params_override)` key.

`params_override` is `Option<serde_json::Value>` — comparing JSON values via
`==` is reflexive on equivalent values regardless of object key order
(`serde_json::Value::Object` is backed by a `Map<String, Value>`, so equality
is by keys+values). No canonicalization step needed.

## 2026-05-17 — pr-open

PR #228 opened: https://github.com/latentwill/xvision/pull/228

Commit: `c9d1a5fff7d573a5d7fc56be6a614c65cd46716c` on
`task/qa-eval-retry-params-override`.

### Changes

- `crates/xvision-engine/src/api/eval.rs` — added `r.params_override ==
  source.params_override` to the in-flight sibling predicate (line ~368);
  expanded the doc comment on `retry()` to spell out the
  different-override-does-not-coalesce behavior.
- `crates/xvision-engine/tests/eval_retry_idempotency.rs` — new test
  binary with five regression tests covering both arms of the contract
  (no-coalesce on different/None-asymmetric overrides; coalesce on
  matching/both-None/reordered-key overrides).

### Verification

```
cargo build -p xvision-engine                          # clean
cargo test -p xvision-engine --test eval_retry_idempotency
  test result: ok. 5 passed; 0 failed
cargo test -p xvision-engine --test api_eval
  test result: ok. 17 passed; 0 failed
```

No regressions on the existing `retry_returns_inflight_sibling_idempotently`
test (true-duplicate sibling still coalesces).
