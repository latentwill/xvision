---
track: qa-remove-agent-max-tokens
owner: claude (autonomous worker)
status: ready-for-review
last_update: 2026-05-17
---

## Current

Claimed 2026-05-17. Working in `.worktrees/qa-remove-agent-max-tokens` on
branch `task/qa-remove-agent-max-tokens` off `origin/main` (5b40959).

## Plan

1. Investigation phase (below).
2. Remove the `max_tokens` input + supporting `MaxTokensInput` component
   from `SlotForm.tsx`; drop the `BLANK_SLOT.max_tokens` mention from
   `AgentForm.tsx` (the wire schema field still exists for back-compat).
3. In `crates/xvision-engine/src/agent/execute.rs`, ignore
   `SlotInput.max_tokens` so the request always hands the LLM dispatcher
   `None`. That makes the Anthropic dispatcher fall back to
   `lookup_model(model).auto_max_tokens()` (the model-library cap) and
   the OpenAI-compat dispatcher omit the field (provider default), which
   is what the layered q15 design intends.
4. Update `agents.test.tsx`: drop tests asserting the input, keep the
   modelMetadata-table tests.
5. Add a Rust test exercising `execute_slot` with `max_tokens: None`
   succeeding through `MockDispatch` (no model-library cap reachable via
   mock, but the request body tests in `agent/llm.rs` already cover the
   Anthropic fallback).
6. Run verification commands per contract.

## Investigation

Prior art:

- `169746d q15(agent-max-tokens)` (#185) — wired `Option<u32>` end-to-end:
  - `AgentSlot.max_tokens: Option<u32>` (`agents/model.rs`).
  - `SlotInput.max_tokens: Option<u32>` (`agent/execute.rs`).
  - `ResolvedAgentSlot.max_tokens` is set from
    `slot.resolve_max_tokens()` (`agent/pipeline.rs:193`) which returns
    the persisted value verbatim if `Some(n>0)` else `None`.
  - `agent/llm.rs::anthropic_request_body` falls back to
    `lookup_model(req.model).auto_max_tokens()` on `None`; OpenAI-compat
    omits `max_tokens` on `None`.
- The dispatcher referred to in the contract is `agent/execute.rs` (the
  file `crates/xvision-engine/src/eval/dispatcher.rs` doesn't exist; same
  for `eval/trader_output.rs` — actual path is
  `eval/executor/trader_output.rs`).

The current state therefore correctly handles unset values via the
model library. The bug surface the contract targets is: persisted
non-null `max_tokens` (e.g. an operator set 4096 once, then upgraded to
a 384k-output model) silently overrides the model-library cap. The
fix is to ignore `SlotInput.max_tokens` in `execute_slot`.

Allowed_paths discipline: `agent/execute.rs` is the only Rust file I
need to touch under the contract. I will NOT touch `agent/pipeline.rs`,
`agents/model.rs`, or the store layer — the persisted field stays on
disk and in the wire schema; only the engine read-path becomes
deliberate.

## Conflicts

- `crates/xvision-engine/src/agent/execute.rs` is multi-owner with
  `qa-execute-slot-cap`. I checked `team/queue/` and
  `team/status/` — no `qa-execute-slot-cap` status file exists at
  claim time, so I'm proceeding without stacking. I will touch only
  the max_tokens-relevant region (the `SlotInput { max_tokens }` field
  and the line that feeds it into `LlmRequest`).
- `frontend/web/src/components/agent/AgentForm.tsx` is multi-owner with
  `qa-strategy-popup-to-accordion` (Wave 2). No status file present —
  not started.
- `crates/xvision-engine/src/eval/dispatcher.rs` doesn't exist; the
  named conflict with `qa-openrouter-pricing-pull` is moot.

## Implementation summary

Frontend:
- `SlotForm.tsx`: removed the entire `MaxTokensInput` component, the
  `buildCatalogTooltip` helper, the catalog/`modelMetadata` imports it
  pulled in, the `slotProviderRow`/`slotProviderKind` derivations that
  only existed to feed it, and the `Max tokens` `<Field>` block in the
  bottom grid. Added a leading comment warning future refactors not to
  bring this back. Kept the wire `AgentSlot.max_tokens` field (loaded
  into local state, just not editable).
- `AgentForm.tsx`: kept `BLANK_SLOT.max_tokens: null` (the field still
  exists on `AgentSlot`); replaced the inline q15 comment with a
  rationale comment pointing at the removal.
- `routes/authoring.tsx`: the inline "create and attach agent" flow now
  sends `max_tokens: null` for new slots instead of seeding the removed
  `4096` override.
- `agents.test.tsx`: rewrote — kept the `modelMetadata table` describe
  block (7 tests; the editorial lookup still serves provider-catalog
  tooling), dropped the SlotForm-rendering UX tests that asserted the
  removed input. Net: 7 passing.

Engine (`crates/xvision-engine/src/agent/`):
- `execute.rs`: kept the `SlotInput.max_tokens` field for back-compat
  (callers in `pipeline.rs` + the in-tree integration tests still pass
  it) but `execute_slot` now always sets the dispatcher request's
  `max_tokens` to `None`. This makes `agent/llm.rs` resolve the cap
  via `lookup_model(model).auto_max_tokens()` on the Anthropic path,
  and omit the field entirely on the OpenAI-compat path (provider
  default). Doc-comment on the field marks it deprecated with the
  rationale. Added two `#[tokio::test]` cases using a local
  `RecordingDispatch` that captures the outgoing `LlmRequest`:
  - `execute_slot_ignores_persisted_max_tokens_and_hands_dispatcher_none`
    (`SlotInput.max_tokens: Some(4096)` → request has `None`).
  - `execute_slot_with_unset_max_tokens_hands_dispatcher_none`
    (`SlotInput.max_tokens: None` → request has `None`).

No migration. No wire-schema edit. No touches to
`agents/store.rs`, `agents/model.rs`, `agents/validate.rs`,
`agent/pipeline.rs`, the eval executors, or `types.gen/`.

## Verification

- `cargo test -p xvision-engine`: my code path passes — 258 lib tests +
  the 4 new agent::execute tests pass. The 4 pre-existing failures
  (`authoring::tests::validate_draft_reports_missing_agent_for_fresh_template`
  and three `eval::postprocess::tests::extract_and_record_*` cases) also
  fail on a clean `origin/main` checkout (verified by cloning the repo
  to `/tmp/` and running the same command). They are NOT regressions of
  this track; flagging for the conductor.
- `cargo test -p xvision-engine --test agent_slot --test llm_dispatch
  --test eval_executor_paper --test pipeline_inline --test api_eval_run
  --test eval_review --test api_strategy`: 4+9+13+13+18+2+4 = 63 passed,
  0 failed. All integration tests that touch `SlotInput.max_tokens` or
  `AgentSlot.max_tokens` keep passing because the struct field stays.
- `pnpm --dir frontend/web typecheck`: clean.
- `pnpm --dir frontend/web lint`: **no `lint` script exists in
  `frontend/web/package.json`** — only `dev`, `build`, `preview`,
  `typecheck`, `test`. The contract's verification list inherits a
  command from the template that this project hasn't adopted. Flagging
  for the conductor; not blocking.
- `pnpm --dir frontend/web test -- --run agents`: 7/7 passed.
- `pnpm --dir frontend/web build`: succeeded (vite emitted the SPA into
  the dashboard's static dir; `.gitkeep` restored after the build).

## Conflicts

- `crates/xvision-engine/src/agent/execute.rs` is multi-owner with
  `qa-execute-slot-cap`. Checked at claim time — no status file present;
  proceeded. Edits are localized to the `SlotInput.max_tokens`
  doc-comment, the new `dispatcher_max_tokens: Option<u32> = None`
  binding, the `LlmRequest { max_tokens: dispatcher_max_tokens }`
  line, and the new test cases. No other region touched.
- `frontend/web/src/components/agent/AgentForm.tsx` is multi-owner with
  `qa-strategy-popup-to-accordion`. Not started — only the
  `BLANK_SLOT` comment changed.
- `crates/xvision-engine/src/eval/dispatcher.rs` (listed in the
  contract's `allowed_paths`) does not exist in the repo. The
  contract's conflict with `qa-openrouter-pricing-pull` over this path
  is moot. The actual eval-side dispatcher logic lives in
  `agent/execute.rs` (touched) and `agent/llm.rs` (not touched).

## Commit

- `4cb4729` qa(agent-max-tokens): remove per-slot max_tokens UI; engine
  ignores persisted override

Branch: `task/qa-remove-agent-max-tokens`. Not pushed; no PR opened
(per worker contract). Worktree retained at
`.worktrees/qa-remove-agent-max-tokens` until the conductor merges /
closes the track.

## Outstanding flags for conductor

1. Contract `verification` lists `pnpm --dir frontend/web lint` but the
   project's `package.json` has no `lint` script. Either add an ESLint
   config + script, or strike the line from the contract template.
2. Four pre-existing `xvision-engine` test failures (one in
   `authoring`, three in `eval::postprocess`) reproduce on a clean
   `origin/main` checkout. Worth a triage track.
3. Contract `allowed_paths` lists
   `crates/xvision-engine/src/eval/dispatcher.rs` and
   `crates/xvision-engine/src/eval/trader_output.rs` — neither exists
   in the tree (the eval dispatcher lives in `agent/execute.rs`; the
   trader-output module is `eval/executor/trader_output.rs`). The
   conflict with `qa-openrouter-pricing-pull` over the bogus path is
   moot. Suggest reconciling the contract paths or amending the
   conflict zone.
