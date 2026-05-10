---
track: llm-providers-2
worktree: /Users/edkennedy/Code/xvision/.worktrees/llm-providers-2
branch: feature/llm-providers-phase-2
phase: phase-b-llm-providers-phase-2
last_updated: 2026-05-10T18:25:00Z
owner: claude-opus session 3 (fifth claim — docker #6, leverage #8, templates #11, providers Phase 1 #14 all merged)
---

# What I'm doing right now

Phase 2 of the LLM Providers plan. Continues the type-system foundation laid
in Phase 1 (PR #14, merged). Touches `xvision-core` (new `slot.rs`),
`xvision-eval/ab_compare.rs` (ArmKind + parser + auto-suffix), and a one-line
addition to `xvision-cli/commands/ab_compare.rs`.

## Plan task progress

- [ ] T5 `SlotRef` newtype + 6 tests
- [ ] T6 `ArmKind::Trader` → struct variant; 4 call-site updates; 1 new test
- [ ] T7 `parse_arm_spec` slot overrides; 6 new tests
- [ ] T8 `auto_suffix_arm_names`; 6 new tests; CLI wiring
- [ ] PR open + merge

# Blocked on

Nothing.

# Followup available for next session

- LLM Providers Phase 3 (ProviderRegistry + run_ab_compare wiring) — needs Phase 2
- LLM Providers Phase 4 (`xvn provider` CLI) — needs Phase 3
- LLM Providers Phase 5 (UI design lock)
- Plan 2a Phases 2A.A/2A.B/2A.C/2A.E (MCP server + verbs + tool dispatch + polish)
- Plan 2d (Dashboard + Wizard) — gates B.6/B.7/B.8
- Eval Phases 3.D/3.E (after 3.C settles)
