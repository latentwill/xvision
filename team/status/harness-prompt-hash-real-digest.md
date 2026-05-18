# Status — harness-prompt-hash-real-digest

- **Contract**: `team/contracts/harness-prompt-hash-real-digest.md`
- **Branch**: `task/harness-prompt-hash-real-digest`
- **Worktree**: `.worktrees/harness-prompt-hash-real-digest`
- **Status**: blocked (PR #277 held from merge; operator wants an image build of pre-harness state first)
- **Claimed**: 2026-05-18
- **PR opened**: 2026-05-18
- **Blocked**: 2026-05-18 — re-open by flipping back to `pr-open` once the image ships

## Result

- `compute_prompt_hash` / `compute_response_hash` pub helpers added to `crates/xvision-engine/src/agent/observability.rs`.
- `emit_model_call_finished` signature accepts caller-computed hashes.
- Call site in `crates/xvision-engine/src/agent/execute.rs` computes the digest from `&req` before `dispatch.complete(req)` consumes it, and accumulates assistant text once for both the delta emission and the response hash.
- 11/11 new integration tests pass (`cargo test -p xvision-engine --test agent_observability_hash`).
- 4 pre-existing lib test failures on origin/main reproduced with WIP stashed — not introduced by this change. Noted in the PR body.

## Plan

1. Check `sha2` is already on the lockfile via `cargo tree -p xvision-engine`; add as dep only if missing.
2. In `crates/xvision-engine/src/agent/observability.rs`:
   - Add `compute_prompt_hash(req: &LlmRequest) -> String` helper. Returns `sha256:<hex>`.
   - Add `compute_response_hash(text: &str) -> String` helper.
   - Add private `PromptDigestInput` struct (`#[serde(deny_unknown_fields)]`, stable field order: system_prompt, messages, tools) — strips reasoning/thinking blocks from messages before hashing.
   - Change `emit_model_call_finished` signature to accept the prompt-hash + response-hash directly (caller-computed), so the bus event carries deterministic values and the hash work happens once at the call site (where the request is borrowed).
3. In `crates/xvision-engine/src/agent/execute.rs`:
   - Compute `prompt_hash` from `req` before the `dispatch.complete(req)` move.
   - Compute `response_hash` from the already-built `assistant_text` accumulator.
   - Pass both to the updated `emit_model_call_finished`.
4. New integration test `crates/xvision-engine/tests/agent_observability_hash.rs`:
   - Determinism: identical (system_prompt, messages, tools) → identical hash.
   - Sensitivity: differing system_prompt → different hash.
   - Reasoning-strip: messages with vs without thinking blocks → same hash.
   - Response: empty text → `None`; non-empty → `Some("sha256:...")`.
5. `cargo test -p xvision-engine` green.

## Notes

Append checkpoints below.
