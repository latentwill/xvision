---
track: harness-recovery-context-overflow
lane: integration
wave: harness-observability-tail-2026-05-21
worktree: .worktrees/harness-recovery-context-overflow
branch: task/harness-recovery-context-overflow
base: origin/main
status: deferred
depends_on:
  - harness-recovery-state-machine
blocks: []
stacking: declared:harness-recovery-state-machine
allowed_paths:
  - crates/xvision-engine/src/agent/recovery.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/agent/llm.rs
  - crates/xvision-engine/src/agent/summarize.rs
  - crates/xvision-engine/tests/agent_recovery_context_overflow.rs
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/migrations/**
  - crates/xvision-observability/src/types.rs
  - crates/xvision-engine/src/agent/observability.rs
  - frontend/web/**
interfaces_used:
  - agent::recovery::FailureClass (NEW variant ContextOverflow; this contract adds it)
  - agent::recovery::RecoveryFamily::ContextOverflow (reserved by F-5 phase 1)
  - agent::llm::LlmDispatch (re-call seam with summarized history)
  - agent::llm::Message (conversation log shape; this contract introduces a synthetic "summary" message)
  - xvision_core::providers::Catalog (model lookup for the cheap-model dispatch)
  - ObsEmitter::emit_recovery_attempt / emit_recovery_failed
parallel_safe: false
parallel_conflicts:
  - "Single-writer on `agent/execute.rs` with `harness-prompt-hash-real-digest` (merged), `agent-error-feedback-self-healing` (merged), the executor refactor track. Coordinate via team/MANIFEST.md."
  - "Single-writer on `agent/recovery.rs` with sibling phase-2 contracts. Sequence runs MalformedJson → SchemaMissingField → ContextOverflow; this contract is last because it ADDS a FailureClass variant (the others only consume the existing surface)."
verification:
  - cargo test -p xvision-engine agent_recovery_context_overflow
  - cargo test -p xvision-engine --lib agent::recovery
  - cargo test -p xvision-engine --lib agent::execute
  - cargo clippy -p xvision-engine -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - **Source spec:** F-5 phase-2 follow-up. The audit text in `team/intake/archive/2026-05-18-harness-observability-audit.md` says: `ContextOverflow → summarize history via cheap-model dispatch, retry once. Hard cap on summarize budget.`
  - **New `FailureClass::ContextOverflow` variant** — F-5 phase 1 reserved the RecoveryFamily but did not add the FailureClass variant (no current classifier evidence triggers it). This contract adds it. Detection lives in `recovery::classify_from_string` looking for provider-side context-window errors: `"context length exceeded"`, `"context window"`, `"max_tokens exceeded"` plus the `OpenAiCompatError` HTTP 400 with body containing those phrases. Add a typed downcast path on `OpenAiCompatError` if a `ContextOverflow` variant is added there (this contract is allowed to extend `llm.rs`'s OpenAiCompatError; if so, also bump the `class_tag()` impl).
  - **Wire tag:** `"context_overflow"` is added to `FailureClass::tag()`. Persisted as `[context_overflow]` on `eval_runs.error` when the retry exhausts. Update the `classify_run_failure_adapter_preserves_wire_tags` test to cover the new tag.
  - **Cheap-model dispatch seam:** new `agent/summarize.rs` module with:
    - `pub async fn summarize_history(history: &[Message], catalog: &Catalog, max_input_tokens: u32) -> anyhow::Result<Message>` — calls the cheapest model in the catalog with a "summarize this conversation in <800 tokens, keep tool-call decisions and their outcomes, drop pleasantries" prompt. Returns a synthetic `Message { role: "user", content: ContentBlock::Text { text: "[history summarized] ..." } }` ready to splice in front of the latest user turn.
    - Hard cap: `max_input_tokens = 2000`. If `history` token count (estimated via a cheap char/4 heuristic — do not add tokenizer deps) exceeds the cap, truncate from the OLDEST end, keeping the last N turns intact. The truncated portion is what gets summarized; the recent turns are kept verbatim.
  - **Wire-up in `agent/execute.rs`:** when the dispatcher returns an error and `recovery::classify(err) == FailureClass::ContextOverflow`, the execute_slot loop:
    1. emits `recovery.attempt` with `class_tag="context_overflow"`, `retry_count=1`;
    2. calls `summarize::summarize_history(messages, catalog, 2000).await`;
    3. replaces `messages[..k]` (the prefix the summarizer covered) with the synthetic summary message; keeps the recent verbatim turns;
    4. retries the original dispatch ONCE;
    5. if the retry also fails, emits `recovery.failed` and propagates the second error.
  - **Provider catalog access:** `execute_slot` needs the `Catalog` to resolve the cheap-model dispatch. Add it to `SlotInput` as `pub catalog: Option<Arc<Catalog>>` (`Option` for backward compat with unit tests that don't need it; when `None`, the recovery short-circuits — no summarize, propagate original error). Update all existing `SlotInput` constructions in tests + the engine pipeline + the eval executors.
  - **Bounded recovery:** ONE retry. Summarize budget hard-capped at 2000 input tokens. The summarizer itself uses the cheapest model from the catalog (lowest `cost_usd_per_million_input_tokens`); no operator config knob. If the catalog has no models, fall back to no-op (propagate original error).
  - **Telemetry:** the summarize dispatch itself emits a normal `model.call` span via the existing ObsEmitter path (cheap-model dispatch is just another LLM call). No new SpanKind. The `recovery.attempt` span PRECEDES the summarize model.call so the trace dock shows the cause-effect ordering.
  - **Out of scope:**
    - Other recovery families (MalformedJson, SchemaMissingField — separate contracts).
    - A streaming summarize path. Block-and-wait.
    - Per-strategy summarize prompts. Hardcoded prompt in `summarize.rs`; if operators ask for customization later it's a separate spec.
    - Token-accurate counting. Char/4 heuristic only.
    - Caching of summaries across runs.
  - **Tests required:**
    - `tests/agent_recovery_context_overflow.rs`:
      - Classifier: `recovery::classify` returns `ContextOverflow` for the three target phrases.
      - Dispatcher integration: mock dispatcher errors with "context length exceeded" on 1st call, succeeds on 2nd call; one `recovery.attempt` span; a `model.call` span for the summarize dispatch (between the two trader spans).
      - Summarize helper unit tests: history under cap is returned verbatim; history over cap is truncated from oldest end before summarize; empty history returns empty.
      - Catalog=None path: no recovery, original error propagates.
      - Both attempts fail: `recovery.failed` emitted, original error surfaces (NOT the summarize error).
    - Update `classify_run_failure_adapter_preserves_wire_tags` with a `[context_overflow]` case.

# Scope

Third phase-2 follow-up. Adds the ContextOverflow recovery family AND
the `FailureClass::ContextOverflow` variant (which F-5 phase 1
deliberately omitted because there was no classifier evidence yet —
this contract owns adding the detection). Introduces a new
`agent/summarize.rs` module for the cheap-model history-summarize
dispatch.

The seam is on the agent loop (`execute_slot`), not the eval executor
— context overflow is a provider-level signal, not a trader-output
parse signal, so the recovery happens BEFORE the response reaches
`TraderOutput::parse_response`.

# Out of scope

- Other recovery families (MalformedJson, SchemaMissingField).
- A streaming summarize path.
- Per-strategy summarize prompts.
- Tokenizer-accurate counting.
- Cross-run summary caching.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-recovery-context-overflow status
git -C .worktrees/harness-recovery-context-overflow log --oneline -3 origin/main..HEAD
# Confirm:
#   - PR #499 (F-5 phase 1) MERGED
#   - `agent/execute.rs` single-writer: read team/MANIFEST.md
#   - `agent/recovery.rs` single-writer: confirm sibling phase-2
#     contracts (MalformedJson, SchemaMissingField) have merged before
#     this one starts, so the FailureClass variant addition lands
#     cleanly
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-recovery-context-overflow -b task/harness-recovery-context-overflow origin/main
```

# Notes

This contract is the most invasive of the three phase-2 follow-ups:
- Adds a new FailureClass variant (the others only consume existing
  variants).
- Adds a new `agent/summarize.rs` module.
- Threads a `Catalog` reference through `SlotInput` (touches every
  existing construction site).

Sequencing last in the phase-2 chain (after MalformedJson and
SchemaMissingField merge) so the `agent/recovery.rs` single-writer
serializes cleanly.

If the cheap-model dispatch turns out to need a token-accurate count
(operator complaint: summaries are over-budget), file a follow-up
contract to wire `tiktoken` or similar. Heuristic is acceptable for
phase 1.
