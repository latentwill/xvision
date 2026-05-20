---
track: strategy-ideas-tool-surface
status: done
branch: task/strategy-ideas-tool-surface
worktree: .worktrees/strategy-ideas-tool-surface
last_updated: 2026-05-21
---

# strategy-ideas-tool-surface — DONE

V2F closer (wave 3, final V2F track). Adds the `list_strategy_ideas`
wizard tool that queries the prepopulated `library/templates/**`
content and returns idea summaries the wizard can quote back to the
user.

## Files touched

- `crates/xvision-engine/src/strategies_folder/ideas.rs` (NEW) — module
  with `list_ideas()`, `IdeaFilter`, `IdeaSummary`, helpers, + inline
  unit tests for normalization / panel-ref extraction / clip / summary
  derivation (5 tests).
- `crates/xvision-engine/src/strategies_folder/mod.rs` — `pub mod ideas;`.
- `crates/xvision-engine/tests/strategies_folder_ideas.rs` (NEW) — 5
  integration tests: category filter, indicator filter, missing
  library, malformed JSON skip, limit clamping.
- `crates/xvision-dashboard/src/wizard_loop.rs` — registered the
  `list_strategy_ideas` ToolDefinition + dispatcher arm + a
  `wizard_lists_strategy_ideas_for_ema_category` integration test that
  runs `prepop::init` and asserts the wizard tool returns >= 3 EMA
  ideas with valid `source_rel_path` and `category=ema`.

## Verification

```
cargo test -p xvision-engine --test strategies_folder_ideas
# 5 passed; 0 failed

cargo test -p xvision-engine --lib strategies_folder::ideas
# 5 passed; 0 failed (inline unit tests)

cargo test -p xvision-dashboard
# all suites green (119 + 14 other suites, 0 failed)

cargo build -p xvision-engine -p xvision-dashboard
# clean
```

## Deviations

- The contract said `indicator` filters against "an entry in the JSON
  template's `indicators` field". The actual `xvision.strategy_template.v1`
  shape has no `indicators` field — instead we derive the
  `IdeaSummary.indicators` vec from `required_tools` plus any
  `IndicatorPanel.<x>` / `OnchainPanel.<x>` / `PriceFrame.<x>`
  references inside `sections.inputs`. Match is substring + case
  insensitive against this derived list. Documented in the module
  rustdoc.

- The contract said `summary` is the template's `description` field if
  present. Current templates use `plain_summary` and `sections.thesis`;
  `derive_summary` falls through `description → plain_summary →
  sections.thesis → sections.decision_rule` so future schema
  additions don't break the surface.

## Closes V2F

Depends on PR #414 (foundation), PR #419 (prepopulation), PR #420
(import) — together with this PR these four close the V2F plan
`docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`.
