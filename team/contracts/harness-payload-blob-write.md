---
track: harness-payload-blob-write
lane: integration
wave: qa-operator-2026-05-19
worktree: .worktrees/harness-payload-blob-write
branch: task/harness-payload-blob-write
base: origin/main
status: ready
depends_on: []   # PR #277 (real-hash digest) + PR #282 (placeholder copy) already merged
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/observability.rs
  - crates/xvision-engine/tests/agent_observability_blob.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/recorder.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/src/blobs.rs
  - crates/xvision-observability/src/redactor.rs
  - crates/xvision-observability/src/config.rs
  - crates/xvision-observability/src/retention.rs
  - frontend/web/**
interfaces_used:
  - ObsEmitter::emit_model_call_finished
  - RunEvent::ModelCallFinished / ModelCallFinishedEvent
  - BlobStore::write / BlobRef
  - RetentionMode / RetentionConfig
  - PayloadRedactor
verification:
  - cargo test -p xvision-engine --test agent_observability_blob
  - cargo test -p xvision-engine
  - cargo test -p xvision-observability
  - cargo test -p xvision-dashboard
acceptance:
  - On a `full_debug` run, `prompt_payload_ref` and `response_payload_ref`
    on the persisted `ModelCallFinishedEvent` are `Some(BlobRef)` and the
    referenced blobs decode back to the prompt request body + completion
    text. No silent `try/catch` shim swallowing write failures
    (`feedback_alpha_root_cause`).
  - The dashboard fetch route at `/api/agent-runs/:run_id/spans/:span_id/blob`
    (already present, owned by `agent-run-observability-blob-fetch-route` —
    merged via #244) returns the prompt and completion bytes on a full_debug
    run created after the fix.
  - SpanInspector renders the prompt body and completion body for a
    full_debug run created after the fix lands, without the "re-run to
    capture" placeholder. (Verified by a component test that mocks the
    cached `AgentRunDetail` with `retention_mode: "full_debug"` and a
    populated `prompt_payload_ref`.)
  - On a `hash_only` run, both refs remain `None`. SpanInspector keeps
    showing the hash-only placeholder copy. The hash-only placeholder
    test path in `SpanInspector.test.tsx` continues to pass without
    modification.
  - On a `redacted` run, the prompt payload runs through the existing
    `PayloadRedactor` before write — the stored blob is the redacted
    form, not the raw request. A regression test confirms secrets are
    scrubbed in the persisted blob.
  - New regression test `crates/xvision-engine/tests/agent_observability_blob.rs`
    asserts the round-trip per retention mode (full_debug populates refs +
    blob content matches; hash_only and redacted behave as above).
  - No changes to `crates/xvision-observability/src/blobs.rs` itself —
    `BlobStore::write` is unchanged. This track only wires it in.
  - No new migration. The `prompt_payload_ref` / `response_payload_ref`
    columns already exist on the sqlite spans table from the original
    Phase-A observability wave.
parallel_safe: false
parallel_conflicts:
  - "Anyone editing crates/xvision-engine/src/agent/execute.rs or observability.rs concurrently: those files were multi-owner during the qa-2026-05-18 wave (harness-prompt-hash-real-digest + agent-error-feedback-self-healing, both merged). The ownership rows should be released as part of the conductor's daily cleanup before this track claims them. Confirm OWNERSHIP.md shows this track as sole owner before opening the PR."
---

# Scope

Wire the producer side of the prompt/response payload blob write that PR
#282's investigation identified as the actual root cause of the operator-
visible "prompt body not captured for this run — re-run to capture"
placeholder in the trace dock. PR #282 made the placeholder copy honest;
PR #277 added real SHA-256 hashes. Neither wired up `BlobStore::write` for
the prompt/response payloads — `crates/xvision-engine/src/agent/observability.rs:255-256`
still hardcodes `prompt_payload_ref: None` / `response_payload_ref: None`,
and `BlobStore::write` has **zero production callers** today (only tests).

Two viable architectures — the worker picks one and documents the choice
in the status note before opening the PR:

**Option A (producer-side write).** `ObsEmitter` gains a `BlobStore` handle
via `with_blob_store(store)`. `emit_model_call_finished` writes both blobs
synchronously and publishes the event with the resulting `BlobRef`s.

**Option B (consumer-side write).** `ModelCallFinishedEvent` carries the
raw payload bytes (under a retention gate on the producer side); the sqlite
recorder writes the blob and the sqlite row atomically in the same handler.

Option B is cleaner (no race between event-publish and blob-write, atomic
sqlite + blob), but adds payload bytes to the event payload — non-trivial
for memory budget on long prompts. Option A keeps events small. The
worker's call. Either way, the retention gate must run on the producer
(it owns the policy), and the redactor must run before the bytes hit
disk in `redacted` mode.

Anchor reading:

- PR #282's investigation (in its merged body) — full root-cause trace.
- PR #277 status note — the hash-half of this work (already shipped).
- `team/intake/2026-05-19-qa-operator-round-4.md` "Round-4 addendum"
  section, item 6.

# Out of scope

- Changes to `BlobStore` itself. The component is finished.
- Schema changes. `prompt_payload_ref` and `response_payload_ref` columns
  already exist.
- Redactor logic changes. Use the existing `PayloadRedactor`.
- Retention-mode redesign. The existing `RetentionMode` (full_debug /
  hash_only / redacted) is the right vocabulary.
- Frontend changes. The blob-fetch route + SpanInspector blob-render path
  are already wired and exercised by tests; this track must not edit
  `frontend/web/**`. If a SpanInspector behavior regresses, file a
  follow-up — do not silently patch.
- Streaming live-prompt path. Prompts don't need streaming (they're
  finalized before the model call); response streaming via
  `emit_assistant_text_delta` is already shipped.
- Migration of pre-fix sqlite rows. Old runs stay placeholder-rendered;
  this track only fixes new runs created post-merge.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-payload-blob-write status
git -C .worktrees/harness-payload-blob-write log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/harness-payload-blob-write
#   - base is up to date with origin/main
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-payload-blob-write \
  -b task/harness-payload-blob-write origin/main
```

Before editing `agent/execute.rs` or `agent/observability.rs`, verify
`team/OWNERSHIP.md` shows this track as sole owner of those rows
(harness-prompt-hash-real-digest + agent-error-feedback-self-healing
ownership should have been released by conductor cleanup; if not, file
a queue note before starting).

# Notes

Append checkpoints / PR links below. The status note's architecture
choice (Option A vs B) is acceptance-bearing — do not collapse it into
the PR description.
