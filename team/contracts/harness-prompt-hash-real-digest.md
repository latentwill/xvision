---
track: harness-prompt-hash-real-digest
lane: leaf
wave: harness-observability-audit
worktree: .worktrees/harness-prompt-hash-real-digest
branch: task/harness-prompt-hash-real-digest
base: origin/main
status: blocked
depends_on: []
blocks:
  - harness-prompt-version-field   # not yet contracted; documented in intake
  - harness-span-attrs-populate    # not yet contracted; the attrs bag will carry prompt_hash
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/tests/agent_observability_hash.rs
  - crates/xvision-engine/Cargo.toml
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/**
  - crates/xvision-dashboard/**
  - frontend/web/**
interfaces_used:
  - xvision_observability::ModelCallFinishedEvent (prompt_hash + response_hash fields)
  - xvision_engine::agent::llm::LlmRequest (the assembled prompt)
  - xvision_engine::agent::llm::ContentBlock (assistant response text)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine
  - cargo test -p xvision-engine --test agent_observability_hash
  - cargo build -p xvision-engine
acceptance:
  - `ObsEmitter::emit_model_call_finished` no longer constructs `prompt_hash` from `run_id` + `span_id`. The signature accepts the actual hash (or accepts the data needed to compute it) — caller's choice; recommend a new helper `compute_prompt_hash(req: &LlmRequest) -> String` that returns `sha256:<hex>` (lowercase, 64 chars).
  - The hash input is deterministic and order-stable: `sha256(serde_json::to_vec(&PromptDigestInput { system_prompt, messages, tools })?)` where `PromptDigestInput` is a private struct with `#[serde(deny_unknown_fields)]` and a stable field order. Reasoning blocks / thinking tags MUST be stripped before hashing (they are non-deterministic across calls).
  - The hash is computed BEFORE `dispatch.complete(req)` consumes the request. Clone the needed slices, don't re-borrow after move.
  - `response_hash` is populated with `sha256:<hex>` of the assistant-text accumulation already built at `agent/execute.rs:204-219` (joined Text blocks only; ToolUse blocks excluded). `None` only when the response is empty (no text blocks at all).
  - Format prefix: both hashes use `sha256:` prefix to make the algorithm explicit and future-migratable. Example: `sha256:9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08`.
  - New integration test at `crates/xvision-engine/tests/agent_observability_hash.rs`:
    - Two distinct `LlmRequest`s with identical (system_prompt, messages, tools) produce identical `prompt_hash`.
    - Two `LlmRequest`s differing only in `system_prompt` produce different `prompt_hash`.
    - A `LlmRequest` with reasoning/thinking blocks in messages produces the same hash as the same request without those blocks (strip is effective).
    - An empty-text response produces `response_hash = None`; a non-empty text response produces `Some("sha256:...")`.
  - `sha2` (or `ring`) is the SHA-256 source. Prefer `sha2` if already in the workspace lockfile; only add a new dep if neither is present. Run `cargo tree -p xvision-engine | grep -E 'sha2|ring'` to check before adding.
  - No changes to the wire format of `ModelCallFinishedEvent` — `prompt_hash` and `response_hash` are already `String` and `Option<String>` respectively. The SQLite recorder, blob store, and dashboard projection layers are untouched by this PR.
  - Existing tests still pass. `cargo test -p xvision-engine` green.
  - No new spans, no new event variants, no migration. Strictly a content fix.
---

# Scope

> **BLOCKED 2026-05-18:** PR #277 is open and green but held from merge
> until the operator ships an image build of pre-harness state.
> Re-open by flipping `status:` back to `pr-open` once the image is
> deployed.

Bug fix. `ObsEmitter::emit_model_call_finished`
(`crates/xvision-engine/src/agent/observability.rs:244`) constructs
`prompt_hash` as `format!("eval:{run}:{span}")` — a string derivation
from the run id and span id, not a digest of the prompt content. The
consequence: two identical prompts in different runs (or different
slots of the same run) hash differently, and two different prompts in
the same span position hash identically when ids collide. This makes:

- prompt deduplication impossible,
- prompt-cache-hit detection impossible,
- `prompt_version` inference (filed as F-3 in the audit intake)
  impossible,
- A/B replay correctness unverifiable.

`response_hash` is hardcoded to `None`, so the same problem applies to
model outputs.

This contract replaces the synthetic hash with a real SHA-256 of the
assembled prompt and the accumulated response text, prefixed `sha256:`
so the algorithm is explicit. The trace dock's existing
`prompt_hash` / `response_hash` surface (already wired through
`crates/xvision-observability/src/sqlite.rs` and the dashboard
projection) starts showing meaningful values for the first time. No
schema change. No new spans. No behaviour change beyond the columns
becoming meaningful.

Reference: 2026-05-18 harness audit
(`team/intake/2026-05-18-harness-observability-audit.md`, F-1).

# Out of scope

- Storing the prompt blob — `prompt_payload_ref` is already populated
  conditionally by the retention layer; this PR doesn't change that.
- A `prompt_version` column on `agent_slots` — filed as F-3 in the
  intake; needs a migration and conductor migration-registry approval.
- Span attribute population — F-2, separate contract.
- Any new `SpanKind` variants — F-4, separate contract.
- Changes to `emit_assistant_text_delta` retention gating — that's a
  privacy surface owned by `qa-retention-prompt-storage-bug`.
- Touching `xvision-observability` crate internals — the wire format
  already accepts `String` + `Option<String>` for these fields.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-prompt-hash-real-digest status
git -C .worktrees/harness-prompt-hash-real-digest log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-prompt-hash-real-digest
#   - base is up to date with origin/main
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-prompt-hash-real-digest \
  -b task/harness-prompt-hash-real-digest origin/main
```

# Notes

Append checkpoints / PR links below.
