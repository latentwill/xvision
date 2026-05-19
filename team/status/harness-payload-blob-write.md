# Status — harness-payload-blob-write

- **Contract**: `team/contracts/harness-payload-blob-write.md`
- **Branch**: `task/harness-payload-blob-write`
- **Worktree**: `.worktrees/harness-payload-blob-write`
- **Status**: in-progress
- **Claimed**: 2026-05-19

## Architecture choice

**Option A (producer-side write).** `ObsEmitter` gains an optional
`BlobStore` handle plumbed in next to the existing `ObsRetentionPolicy`,
and `emit_model_call_finished` accepts the raw prompt/response bytes
along with the existing hash strings. The emitter applies retention
gating + redaction before calling `BlobStore::write`, then publishes
the resulting refs on `ModelCallFinishedEvent`.

### Why Option A over Option B

1. **No raw payloads on the broadcast/SSE channel.** Option B would
   route up to `max_payload_bytes` (default 200 KB) of prompt/completion
   text through `RunEventBus` and its `BroadcastSubscriber`, which
   feeds the dashboard SSE stream. The whole reason
   `AssistantTextDelta` has a retention gate at the producer
   (`ObsRetentionPolicy::apply_to_body`) is to keep raw bodies off
   that channel under anything but `FullDebug`. Adding payload bytes
   to a non-streaming event would re-introduce the same leak surface
   the existing gate exists to prevent.

2. **Scope fits `allowed_paths`.** Option B needs payload-aware writes
   on the recorder side, which would naturally land in `recorder.rs` or
   `bus_subscriber.rs` — neither in this contract's allowed paths.
   Option A localizes the change to `observability.rs` (engine) +
   `events.rs` + `sqlite.rs`, all on the allowlist.

3. **Producer already owns retention policy + the bytes.** The
   `LlmRequest` body and assistant text are materialized in
   `execute.rs` before the event is constructed; the retention policy
   is already on `ObsEmitter`. Bringing the redactor in next to it
   keeps the gate, the redactor, and the write site in one file —
   one place to audit when the redactor extends.

### Atomicity tradeoff (accepted)

Option A has a race window between blob-write and sqlite-row-write:
if the recorder crashes after `BlobStore::write` succeeds but before
the sqlite INSERT lands, the blob is orphaned. The janitor's
`expire_old_payload_refs` + filesystem-scan path already handles
orphan cleanup on a TTL — same cleanup loop that handles
post-retention-truncation orphans today. Acceptable for this track.

### Failure handling

`BlobStore::write` returning `Err` on a `full_debug` path surfaces as
an `error!` log + the event publishes with `prompt_payload_ref:
None` / `response_payload_ref: None` so the run still records.
We do NOT silently fall back to a no-op when full_debug was
requested — the operator gets a structured tracing event they can
grep. This matches `feedback_alpha_root_cause`: surface the failure,
don't suppress it. (The function signature stays `async fn -> ()`
because the existing call sites are wired that way and changing
to `Result` would need a wider API ripple than this contract allows.
The `error!` log is the supervisor channel.)

## Plan

1. Test first: `crates/xvision-engine/tests/agent_observability_blob.rs`
   asserts three retention modes:
   - `full_debug` → refs `Some(_)`, blob bytes decode to prompt + response.
   - `hash_only` → refs `None`.
   - `redacted` → refs `Some(_)`, blob content is the post-`Redactor` text
     (regression check on a hard-coded `sk-ant-` secret).
2. Add `BlobStore` field to `ObsEmitter` via `with_blob_store(store)`.
   Bring `Redactor` next to the retention policy gate.
3. Add `prompt_payload` + `response_payload` params to
   `emit_model_call_finished`; gate + redact + write; populate refs.
4. Wire `execute.rs` to pass the raw `LlmRequest` body
   (`serde_json::to_vec(&req)` of the same `PromptDigestInput`
   structure so the digest and the payload come from the same
   serialization) and the assistant text accumulator.
5. Wire `api/eval.rs` to call `with_blob_store(BlobStore::new(...))`
   on the `ObsEmitter` it constructs, using `ctx.obs_config`'s
   blob root.

## Notes

Append checkpoints below.
