---
track: qa-strategy-id-path-safety
status: pr-open
last_update: 2026-05-17
worker: Claude (xvision worker session)
pr: https://github.com/latentwill/xvision/pull/231
commits:
  - ba0cf38 — qa: validate strategy id before joining filesystem paths
---

## Outcome

Single commit on `task/qa-strategy-id-path-safety`, pushed to origin,
PR #231 open against `main`.

## Verification

All commands from the contract's `verification:` block green (run with
`PATH=$HOME/.cargo/bin:$PATH CARGO_TARGET_DIR=$HOME/.cargo-target/xvision`):

| Command | Result |
|---|---|
| `cargo build -p xvision-engine` | clean (3 pre-existing dead-code warnings in `api/eval.rs`, unrelated) |
| `cargo test -p xvision-engine --lib strategies::store` | 8 passed |
| `cargo test -p xvision-engine --lib strategies::id` | 12 passed |
| `cargo test -p xvision-engine --test strategy_id_path_safety` | 17 passed |
| `cargo test -p xvision-engine --test strategy_store` | 2 passed (no regression) |
| `cargo test -p xvision-engine --lib api::strategy::tests` | 13 passed (no regression) |

Pre-existing lib failures in `authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
and `eval::postprocess::tests::*` reproduce on `origin/main` without
this PR's changes — unrelated to strategy-id validation.

## Implementation

1. `crates/xvision-engine/src/strategies/id.rs` (new): free function
   `validate_strategy_id_for_path(&str) -> Result<&str, StrategyIdError>`
   plus a typed `StrategyIdError` enum (`Empty`, `PathSeparator`,
   `NulByte`, `LeadingDot`, `ReservedSegment`, `DisallowedChar`).
   Separator/NUL checks run before the leading-dot check so a payload
   like `../escape` reports the load-bearing violation.
2. `crates/xvision-engine/src/strategies/mod.rs`: register the new module.
3. `crates/xvision-engine/src/strategies/store.rs`: `path_for` now
   returns `Result<PathBuf, StrategyIdError>` and is called by all
   three mutating methods. Adds unit tests proving the store root is
   unchanged after a rejection.
4. `crates/xvision-engine/src/api/strategy.rs`: added
   `strategy_id_validation_error` helper that downcasts an
   `anyhow::Error` chain to `StrategyIdError` and maps it to
   `ApiError::Validation`. Wired into `get_inner`, `delete_inner`, and
   `map_authoring_error` so every public surface returns a 4xx-style
   error instead of a 500 on traversal attempts.
5. `crates/xvision-engine/tests/strategy_id_path_safety.rs` (new):
   17 integration tests covering validator pattern + store-level
   rejection (including bait files in fixture-private parent dirs to
   prove no traversal write/read/unlink occurred).

## Out-of-scope (honored)

- No edits to `strategies/validate.rs` (owned by `qa-role-normalization`).
- No edits to `strategies/agent_ref.rs`.
- No edits to `api/eval.rs` (owned by `qa-eval-retry-params-override`).
- No migrations and no on-disk fixture changes — ULID-shaped ids
  already satisfy the pattern.
- No frontend changes.
