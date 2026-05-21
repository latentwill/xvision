---
track: harness-recovery-schema-missing-field
lane: integration
wave: harness-observability-tail-2026-05-21
worktree: .worktrees/harness-recovery-schema-missing-field
branch: task/harness-recovery-schema-missing-field
base: origin/main
status: deferred
depends_on:
  - harness-recovery-state-machine
  - harness-recovery-malformed-json
blocks: []
stacking: declared:harness-recovery-malformed-json
allowed_paths:
  - crates/xvision-engine/src/agent/recovery.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/trader_output.rs
  - crates/xvision-engine/tests/agent_recovery_schema_missing_field.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/migrations/**
  - crates/xvision-observability/src/types.rs
  - crates/xvision-engine/src/agent/observability.rs
  - frontend/web/**
interfaces_used:
  - agent::recovery::FailureClass (TraderMissingField, TraderInvalidField)
  - agent::recovery::RecoveryFamily::SchemaMissingField
  - eval::executor::trader_output::TraderOutputError (carries kind + detail)
  - eval::executor::trader_output::TraderOutput::parse_response
  - agent::pipeline / agent::execute trader-slot re-call seam
  - ObsEmitter::emit_recovery_attempt / emit_recovery_failed
parallel_safe: false
parallel_conflicts:
  - "Single-writer on `paper.rs`, `backtest.rs`, `trader_output.rs` with the F-5 phase-1 + MalformedJson + ContextOverflow tracks. Sequence is enforced by depends_on chain."
  - "Single-writer on `agent/recovery.rs` with sibling phase-2 contracts."
verification:
  - cargo test -p xvision-engine agent_recovery_schema_missing_field
  - cargo test -p xvision-engine --lib agent::recovery
  - cargo test -p xvision-engine --lib eval::executor
  - cargo clippy -p xvision-engine -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - **Source spec:** F-5 phase-2 follow-up. The audit text lives in `team/intake/archive/2026-05-18-harness-observability-audit.md`. The MalformedJson contract should merge first so the repair-seam plumbing is in place to reuse.
  - **Seam:** same as MalformedJson — `paper.rs` ~line 918, after `TraderOutput::parse_response`. When `TraderOutputError.kind` is `MissingField` or `InvalidField`, dispatch ONE targeted patch retry.
  - **Targeted-patch shape:** unlike MalformedJson which re-asks for the whole response, SchemaMissingField asks only for the offending fields. Implementation:
    - Extract the missing/invalid field names from `TraderOutputError.detail`. The Display format is stable (e.g. `"missing required field: conviction"` for MissingField, `"invalid value for field action: 'BUY_BIG'"` for InvalidField). Parse the field name out via a typed helper added to `trader_output.rs` — do not regex-parse in the recovery module. Add a `pub fn problem_fields(&self) -> Vec<String>` on `TraderOutputError` that returns the field names extracted from the detail.
    - The repair message body says: "Your previous response was missing/invalid for fields: [<list>]. Re-emit only a single JSON object with those fields filled in correctly. Other fields you produced are accepted as-is — do not repeat them."
    - The model's second response is merged with the first: `serde_json::Value::Object` merge where the second response's keys override the first's. Then re-parse via `TraderOutput::parse_response` on the merged value.
  - **Bounded retry:** ONE patch attempt. If the merged value still fails to parse, propagate the ORIGINAL `TraderOutputError` (same fail-closed policy as MalformedJson).
  - **Span emission:** `recovery.attempt` with `class_tag = "missing_field"` or `"invalid_field"` and `retry_count = 1`. `recovery.failed` on second-attempt failure.
  - **Helper extraction:** the merge-and-reparse logic + the field-extraction helper land in `eval/executor/trader_output.rs` (allowed). The recovery module orchestrates the dispatch + reuses the helpers; the dispatch loop lives in `paper.rs`/`backtest.rs` next to the existing parse call.
  - **Out of scope:**
    - Changing the JSON schema or `TraderOutput` struct.
    - Recovery for InvalidJson / Truncated (MalformedJson contract owns those).
    - Recovery for ContextOverflow / EmptyData.
    - Multiple-attempt patches — one shot only.
    - "Smart" patches that try to infer the right value (e.g. autocorrect `BUY_BIG` → `BUY`). The patch only re-asks; it does not invent.
  - **Tests required:**
    - `tests/agent_recovery_schema_missing_field.rs`:
      - 1st-call returns JSON missing `conviction`, 2nd-call returns `{"conviction": 0.7}` → merge produces valid TraderOutput; one `recovery.attempt` span.
      - 1st-call returns invalid `action: "BUY_BIG"`, 2nd-call returns `{"action": "buy"}` → merged value parses; span emitted.
      - 1st-call missing 2 fields, 2nd-call also missing 1 → original error surfaces; `recovery.failed` emitted.
      - `TraderOutputError::problem_fields()` unit tests covering the Display-format extraction.
  - **Wire-shape stability:** `[missing_field]` / `[invalid_field]` prefixes unchanged.

# Scope

Second phase-2 follow-up. Recovery for the SchemaMissingField family
(TraderMissingField + TraderInvalidField). Pattern mirrors MalformedJson
but with a targeted patch instead of a full re-emit, plus a merge step.

# Out of scope

- Recovery for InvalidJson / Truncated (MalformedJson contract).
- Recovery for ContextOverflow / EmptyData (separate / not scheduled).
- Multiple-attempt patches — one shot only.
- Heuristic correction (autocorrect-style guesses).
- Schema or struct changes.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/harness-recovery-schema-missing-field status
git -C .worktrees/harness-recovery-schema-missing-field log --oneline -3 origin/main..HEAD
# Confirm:
#   - PR #499 (F-5 phase 1) MERGED
#   - `harness-recovery-malformed-json` MERGED (this contract reuses
#     the repair-dispatch helper that contract introduces)
#   - executor/paper.rs single-writer: check team/MANIFEST.md
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/harness-recovery-schema-missing-field -b task/harness-recovery-schema-missing-field origin/main
```

# Notes

Sequencing: depends on both F-5 phase 1 (the FailureClass surface) AND
the MalformedJson contract (the repair-dispatch helper pattern this
reuses). The merge-and-reparse logic is novel to this contract — make
sure the helper sits in `trader_output.rs` where existing tests already
cover the parse path.
