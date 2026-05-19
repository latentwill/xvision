# Queue note — `ModelCallFinishedEvent` emit hardcodes `prompt/response_payload_ref: None`; nothing writes bodies to the blob store

**From:** `qa-retention-prompt-storage-bug` (worker)
**To:** conductor — route to `harness-prompt-hash-real-digest` (or a
new tightly-scoped track) once the harness wave is ungated.
**Filed:** 2026-05-18 05:24 UTC
**Severity:** P1 — operator-facing data is missing from the trace dock
under `full_debug` retention.

## What

The agent harness never writes prompt or response bodies to the
observability blob store, and never populates the corresponding
event refs. Specifically:

```rust
// crates/xvision-engine/src/agent/observability.rs:235-261
pub async fn emit_model_call_finished(
    &self,
    span_id: &str,
    provider: &str,
    model: &str,
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
    cost_usd: Option<f64>,
) {
    let prompt_hash = format!("eval:{run}:{span}", run = self.run_id, span = span_id);
    self.bus
        .publish(RunEvent::ModelCallFinished(ModelCallFinishedEvent {
            span_id: span_id.to_string(),
            provider: provider.to_string(),
            model: model.to_string(),
            input_token_count: input_tokens.map(i64::from),
            output_token_count: output_tokens.map(i64::from),
            cost_usd,
            prompt_hash,
            response_hash: None,
            prompt_payload_ref: None,          // <-- never set
            response_payload_ref: None,        // <-- never set
            tool_calls_requested: None,
            capability_path: None,
        }))
        .await;
}
```

And:

```rust
// crates/xvision-observability/src/blobs.rs (production callers of BlobStore::write: 0)
```

The blob store exists and has tests, but no production code calls
`write()`. As a consequence:

- Every `model_calls` row in the observability sqlite carries null
  `prompt_payload_ref` and `response_payload_ref`.
- The dashboard's `PayloadRefDetails` blob-preview UI never has
  a ref to fetch.
- The trace dock prompt cell renders a placeholder regardless of
  retention mode (the operator's "prompts redacted despite
  full_debug" complaint).

Responses are visible only because they stream live via
`emit_assistant_text_delta` and the frontend reconstructs the body
from accumulated chunks — completely separate from the persisted
payload path.

## Why this matters

Under `full_debug` retention the operator expects to see prompts AND
responses in the trace dock. Today they see responses (via live
streaming) and a placeholder for prompts (because the producer
never wrote them). PR for this contract corrects the **copy** so the
placeholder is honest about the cause, but the underlying body is
still missing. Until the producer writes the body, the operator
cannot inspect the prompt their model received.

This blocks meaningful debugging of model behaviour under
`full_debug` — which is supposed to be the all-information mode.

## Suggested smallest closing fix

1. Add a `BlobStore` reference to `ObsEmitter` (alongside `bus` and
   `run_id`). Construct it from the resolved `ObservabilityConfig`
   in the eval handler that already builds the emitter.
2. Extend `emit_model_call_finished` to accept the prompt + response
   text:
   ```rust
   pub async fn emit_model_call_finished(
       &self,
       span_id: &str,
       provider: &str,
       model: &str,
       input_tokens: Option<u32>,
       output_tokens: Option<u32>,
       cost_usd: Option<f64>,
       prompt_text: Option<&str>,
       response_text: Option<&str>,
   ) { ... }
   ```
3. Inside, when `retention.allow_assistant_body()` returns true
   (i.e. `full_debug + store_responses`), write each non-empty body
   to the blob store and use the returned ref for the corresponding
   event field. When suppressed, emit `None` (current behaviour).
4. Update the single caller in
   `crates/xvision-engine/src/agent/execute.rs:221-228` to pass the
   bodies it already has in scope.
5. Once the producer-side `prompt_hash` work in
   `harness-prompt-hash-real-digest` lands (replaces the
   `eval:<run>:<span>` placeholder), the same change point can
   compute the real digest and the new payload ref in one pass —
   they share the same data-clone moment.

The producer change is roughly 30-50 lines + an integration test.
The blob-fetch UI already exists end-to-end
(`agent-run-observability-blob-fetch-route` shipped in #244), so the
frontend will start resolving payload refs immediately for new runs.

## Test coverage that should land with the fix

- `crates/xvision-engine/tests/agent_observability_payload_refs.rs`:
  a full_debug run with non-empty prompt + response produces a
  `ModelCallFinishedEvent` whose `prompt_payload_ref` and
  `response_payload_ref` both resolve via `BlobStore::read` to the
  original bodies. A `hash_only` run produces both refs as `None`.
- Update the existing
  `crates/xvision-observability/tests/agent_runs_blob_route.rs`
  fixture to exercise a `prompt_payload_ref` round-trip (today it
  only seeds blob entries by hand).

## Routing suggestion

`harness-prompt-hash-real-digest` already owns
`crates/xvision-engine/src/agent/observability.rs` (per
`team/OWNERSHIP.md` after the 2026-05-18 sweep) and is on the GATED
harness wave. Once the operator's image build ships and the wave is
ungated, fold this work into that track (or open a small dedicated
leaf `harness-payload-write-bodies` if the maintainer prefers
narrower scope). The two changes share the same emit point and
should land together.

Until that lands, the `qa-retention-prompt-storage-bug` PR keeps
the operator-visible placeholder honest about *why* the body isn't
on screen.
