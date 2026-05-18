---
track: wizard-scenario-create-tool-repair
lane: integration
wave: qa-operator-2026-05-18-r3
worktree: .worktrees/wizard-scenario-create-tool-repair
branch: task/wizard-scenario-create-tool-repair
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/wizard_loop.rs
  - crates/xvision-dashboard/tests/wizard_loop.rs
  - crates/xvision-dashboard/tests/http.rs
  - crates/xvision-dashboard/prompts/wizard.md
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/scenarios/**
  - frontend/web/**
interfaces_used:
  - normalize_create_scenario_input (wizard_loop.rs)
  - Scenario / CapitalConfig / Calendar serde shapes (xvision-engine)
parallel_safe: false
parallel_conflicts:
  - "wizard-strategy-template-optional: also edits wizard_loop.rs. Coordinate disjoint regions; later claimant rebases."
verification:
  - cargo test -p xvision-dashboard -- wizard_loop create_scenario
  - cargo clippy -p xvision-dashboard -- -D warnings
acceptance:
  - `normalize_create_scenario_input` always populates `time_window`
    when the agent omits it — default to a 90-day window ending
    today (UTC). No path through the normalizer results in a
    payload reaching serde with `time_window` missing.
  - `capital` field is **repaired** (not just defaulted) when the
    agent supplies a malformed shape. If it's not an object, or is
    an object missing `initial` / `currency`, the normalizer replaces
    it with `{ initial: 100000.0, currency: "USD" }` and logs a
    `tracing::warn!` naming the bad input. Today the entry-or-insert
    only defaults when the key is absent.
  - `calendar` field unwraps a tagged shape (`{ "type":
    "Continuous24x7" }` → `"Continuous24x7"`) before passing to serde.
    Same treatment for `Custom` and `UsEquities` variants. The
    unknown-variant `type` error from Qwen no longer reproduces.
  - When the wizard tool-use loop hits the 12-iteration cap, the
    operator-visible event names the **last tool error** so the
    next debug pass starts at the failing schema, not the generic
    "model stuck calling tools without responding" message.
  - New unit tests in `wizard_loop.rs`:
    - `create_scenario_repairs_missing_time_window` (Qwen #1 repro)
    - `create_scenario_repairs_malformed_capital` (Qwen #2 repro)
    - `create_scenario_unwraps_tagged_calendar_variant` (Qwen #3 repro)
    - `tool_loop_cap_message_includes_last_tool_error` (#4)
  - Existing `create_scenario_recovers_missing_description_and_sol_q1_shape`
    test continues to pass.
---

# Scope

Operator (2026-05-18) hit a sequence of `create_scenario` failures
that pushed the wizard tool-use loop past the 12-iteration cap and
killed the run. The errors:

- `missing field 'time_window'` (x4)
- `missing field 'initial'` (capital subfield)
- `unknown variant 'type'`, expected one of `Continuous24x7`,
  `UsEquities`, `Custom` (calendar tag-wrapped instead of bare)
- Loop hit cap: `wizard tool-use loop exceeded 12 iterations —
  model is stuck calling tools without responding`

`normalize_create_scenario_input` in `wizard_loop.rs:1112` already
covers several shapes Qwen produces — `display_name`, `asset_class`,
`quote_currency`, `granularity`, `source` enum, asset arrays. It
does NOT cover the three above. Extend it. Plus surface the loop-cap
event with the last tool error so debugging starts in the right place.

# Out of scope

- Loosening the underlying Scenario serde shape (still strict).
- Raising or removing the 12-iteration loop cap. The cap is a
  safety mechanism; this contract only improves the failure message.
- Changing the agent prompt to be more strict about scenario shapes
  (the goal is *repair the bad input*, not lecture the model).
- Backend Scenario / Calendar type changes (separate concern).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/wizard-scenario-create-tool-repair status
git -C .worktrees/wizard-scenario-create-tool-repair log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/wizard-scenario-create-tool-repair \
  -b task/wizard-scenario-create-tool-repair origin/main
```

# Notes

Operator-supplied real `run_id` for repro lookup in observability:
the wizard session that hit this happened during the 2026-05-18
walk-through; no specific run_id captured but the four tool errors
above are verbatim from the transcript.

Coordinate with `wizard-strategy-template-optional` via team/queue/
(both edit wizard_loop.rs). Disjoint regions — see that contract's
notes for the line ranges.

Append checkpoints / PR links below.
