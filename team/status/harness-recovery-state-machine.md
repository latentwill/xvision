# Status — harness-recovery-state-machine

- **Contract**: `team/contracts/harness-recovery-state-machine.md`
- **Branch**: `task/harness-recovery-state-machine`
- **Worktree**: `.worktrees/harness-recovery-state-machine`
- **Status**: pr-open
- **Claimed**: 2026-05-18
- **PR opened**: 2026-05-18 (#298)

## Result

- New `recovery` module with typed `FailureClass` + `RecoveryOutcome` enums in `crates/xvision-engine/src/eval/executor/recovery.rs`.
- `classify_run_failure` (the legacy `&'static str` surface in `crates/xvision-engine/src/eval/executor/mod.rs`) now delegates to `recovery::classify(...).tag()` with explicit fixups that pin the pre-F-5 wire format. Fixups cover MalformedJson (split back to `invalid_json` / `provider_decode`), ToolTimeout (→ `unclassified`), ContextOverflow (→ `provider_http_error` or `unclassified` per legacy needle), and EmptyData (→ `unclassified`).
- Regression tests in `crates/xvision-engine/src/eval/executor/mod.rs::tests` pin each wire-format projection.
- `ObsEmitter::emit_recovery_attempt` emits `SpanKind::RecoveryAttempt` with typed `SpanAttributes` (`run_id`, `retry_count`) merged with a `recovery` sub-object.
- Folds in the deferred `agent-error-feedback-non-broker-errors` follow-up from PR #286.

## Verification

- `cargo test --lib -p xvision-engine eval::executor` — 54 tests pass.
- `cargo build --workspace` — passes.
- `cargo doc -p xvision-engine --no-deps` — passes; the unresolved `RecoveryDispatcher` intra-doc link flagged in review was fixed by rewording the module doc (no such type exists; the dispatcher logic lives in the executor's recovery loop).

## Review

- PR #298 review (2026-05-18) flagged three items: wire-format drift on `context_overflow` / `empty_data` (Medium), missing contract/status files referenced by the PR body (Medium), and a broken rustdoc intra-doc link to `RecoveryDispatcher` (Low). All three addressed in a follow-up commit on this branch.
