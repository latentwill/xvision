---
track: trader-noop-skip
lane: leaf
wave: eval-honesty-2026-05-21
worktree: .worktrees/trader-noop-skip
branch: task/trader-noop-skip
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/agents/**
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/tests/trader_noop_skip.rs
  - team/contracts/trader-noop-skip.md
forbidden_paths:
  - frontend/**
  - crates/xvision-eval/**
interfaces_used:
  - AgentSlot
  - ResolvedAgentSlot
  - run_agent_pipeline
  - PipelineInputs
  - PipelineOutputs
  - LlmResponse
  - LlmDispatch
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine
acceptance:
  - when portfolio_state.position_size != 0 and noop_skip is None or Some(true), the LLM
    provider mock is NEVER called and PipelineOutputs.trader is Some with text containing
    "noop_skip"
  - when slot.noop_skip = Some(false), the LLM IS called even when position_size != 0
  - when portfolio_state.position_size == 0 (flat), the LLM IS called regardless of noop_skip
  - the synthesized trader output has action "hold", conviction 0.0, and justification
    containing "noop_skip"
  - total_input_tokens and total_output_tokens are both 0 for the skipped cycle
---

# Scope

Add a pre-LLM gate on the trader slot that skips the LLM call when the current
`portfolio_state` shows the portfolio already holds a position (non-zero
`position_size`), making only `hold` legal. When the gate fires, a synthesized
trader output with `action: hold`, `conviction: 0`, and
`justification: "noop_skip: ..."` is returned without calling the provider.
Provenance is recorded in the `justification` text so the trace/eval review
surface shows that the skip happened while preserving the strict trader-output
schema.

Per-slot opt-out: `AgentSlot.noop_skip: Option<bool>` defaults to `None`
(equivalent to `Some(true)` — skip enabled). Operators who want the LLM to run
in the zero-legal-actions corner set `noop_skip: false` explicitly.

Source intake: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` (row
"Skip the LLM call when the current `portfolio_state` allows zero legal
actions; opt-out per-slot").

# Out of scope

- Filter/Critic/Intern agent slots — only the `trader` role is gated.
- Modifying the risk gate or guardrail logic.
- Adding a feature flag — this is a pure additive opt-out.
- Persisting `noop_skip` to SQLite (follow-up migration when the UI exposes it).
- `crates/xvision-eval/**` — baselines are untouched.
- `frontend/**` — no UI changes in this track.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/trader-noop-skip status
git -C .worktrees/trader-noop-skip log --oneline -3 origin/main..HEAD
```

# Notes

Implementation adds two helpers to `agent/pipeline.rs`:

- `seed_has_zero_legal_opens(seed)` — inspects
  `seed["portfolio_state"]["position_size"]` for non-zero float; returns
  `false` (conservative — run the LLM) when the field is absent or
  non-numeric.
- `noop_skip_response()` — builds a zero-token `LlmResponse` with valid
  trader JSON and `noop_skip` provenance in the justification.

`noop_skip` is not yet persisted to SQLite — the field round-trips via
`#[serde(default)]` like `temperature`; all rows loaded from the store come
back `None` (= skip enabled) until a follow-up migration adds the column.
