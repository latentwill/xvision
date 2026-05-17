---
track: qa-role-normalization
status: ready-for-review
last_update: 2026-05-17
worker: Claude (xvision conductor session)
commits:
  - ced0b55 — qa: normalize AgentRef.role at every comparison site
---

## Outcome

Single commit on `task/qa-role-normalization`. Branch local, not pushed.

## Verification

All commands from the contract's `verification:` block green (run with
`PATH=$HOME/.cargo/bin:$PATH CARGO_TARGET_DIR=$HOME/.cargo-target/xvision`):

| Command | Result |
|---|---|
| `cargo build -p xvision-engine` | clean (3 pre-existing dead-code warnings in `api/eval.rs`, unrelated) |
| `cargo test -p xvision-engine --lib strategies::` | 25 passed, 0 failed |
| `cargo test -p xvision-engine --lib agent::pipeline` | 3 passed, 0 failed |
| `cargo test -p xvision-engine --lib eval::executor::backtest` | 9 passed (incl. 2 new `trader_model_id_*` tests) |
| `cargo test -p xvision-engine --lib eval::executor::paper` | 1 new `role_tests` module added; passes |
| `cargo test -p xvision-engine --test role_normalization` | 7 passed |

The contract's verification list mentioned `cargo test -p xvision-engine strategies::validate` etc. as raw test args; cargo CLI doesn't accept multiple `[TESTNAME]` positional args, so I ran each filter as a separate invocation. Same coverage.

## Findings produced

- Confirmed the three drift sites from the original investigation
  (validate.rs:212 trim-but-don't-persist;
  pipeline.rs:127-vs-145 case-insensitive-vs-sensitive split;
  eval/executor/{backtest,paper}.rs:trader_model_id missing trim).
- Chose lowercase canonical form (rationale in status above and inline
  in `agent_ref.rs::canonical_role`).
- Added serde deserialize/serialize on role fields so disk
  round-trips guarantee canonical form going forward — no migration.

## Out-of-scope reminders honored

- No edits to `crates/xvision-engine/src/api/eval.rs` (owned by
  `qa-eval-retry-params-override`).
- No edits to `crates/xvision-engine/src/agent/execute.rs` (owned by
  `qa-execute-slot-cap`).
- No CLI changes.
- No migration to scrub stored whitespace-padded data — serde
  normalize-on-deserialize self-heals on next read.

## Ready for

PR open by conductor when the wave is being merged. Single-writer
claims on `eval/executor/{backtest,paper}.rs` should be released
after merge.

# Status

## Plan

Canonical form is **trimmed + lowercase** (rationale: matches existing
intern/trader/risk/executor convention from CLAUDE.md and minimizes
touch points compared to preserve-case-everywhere).

Implementation:

1. `strategies/agent_ref.rs`:
   - Add `pub fn canonical_role(&str) -> String` (single source of truth).
   - Add `impl AgentRef::canonical_role(&self) -> String`.
   - Add a `#[serde(deserialize_with = ...)]` and `serialize_with` on
     `AgentRef.role`, `PipelineEdge.from_role`, `PipelineEdge.to_role`
     so disk → engine round-trips canonical. Backwards-compat for old
     whitespace-padded data: silently normalize on load rather than
     reject (rejecting locks the operator out of seeing their own data).
2. `strategies/validate.rs`:
   - Empty check uses `canonical_role(&agent.role).is_empty()`.
   - Role set holds `canonical_role(&agent.role)`; edge lookups use the
     same canonical form.
3. `agent/pipeline.rs::run_agent_pipeline`:
   - Both the trader-detection AND the match-arm use
     `canonical_role(&resolved.role)`.
   - Output-key naming uses the canonical form too so the JSON keys
     don't carry whitespace.
4. `eval/executor/{backtest,paper}.rs::trader_model_id`:
   - Replace `r.role.eq_ignore_ascii_case("trader")` with
     `canonical_role(&r.role) == "trader"`.
5. `tests/role_normalization.rs`:
   - (a) Attached `Trader` / `TRADER` / `" trader "` all produce
     populated `PipelineOutputs.trader` (mock LLM dispatch).
   - (b) Whitespace-padded roles round-trip to canonical form (load
     normalizes; saved-then-loaded comes back trimmed).
   - (c) `trader_model_id` returns the trader's model for all variants.

## Drift sources confirmed

- `validate.rs:212` — `agent.role.trim()` borrowed into role set but
  persisted role retains whitespace.
- `pipeline.rs:127` — `trim().eq_ignore_ascii_case("trader")` selects
  the trader-output schema correctly.
- `pipeline.rs:145-149` — `match resolved.role.trim() { "trader" }` is
  case-sensitive (no `to_ascii_lowercase()`), so attached `Trader` runs
  as trader but the match-arm drops the result. **This is QA finding
  #5.**
- `eval/executor/backtest.rs:655` and `paper.rs:146` —
  `eq_ignore_ascii_case("trader")` but no `trim()`. Padded variants
  miss the reasoning-class truncation hint. **This is QA finding #7.**

## Out-of-scope reminders

- `crates/xvision-engine/src/api/eval.rs` belongs to
  `qa-eval-retry-params-override`. Don't touch.
- `crates/xvision-engine/src/agent/execute.rs` belongs to
  `qa-execute-slot-cap`. Don't touch.
- No CLI changes (`crates/xvision-cli/**` not in allowed_paths) — the
  serde-deserialize normalization handles CLI-authored data after
  round-trip.
