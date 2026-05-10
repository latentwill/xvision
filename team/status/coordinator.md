---
track: coordinator
worktree: /Users/edkennedy/Code/xvision (main)
branch: main
phase: phase-a-two-prs-open
last_updated: 2026-05-10T07:05:47Z
---

# What I'm doing right now

Two Phase A PRs open, both independent of each other and ready for review:

- **PR #4** — Engine API Foundation (Plan #3): https://github.com/latentwill/xvision/pull/4
  Branch: `feature/engine-api-foundation`. 38 tests pass.
- **PR #5** — Broker Surface (Plan 2c §Task 7 extraction): https://github.com/latentwill/xvision/pull/5
  Branch: `feature/broker-surface-trait`. 25 tests pass + 2 ignored live tests.

Together these unblock eval-engine (Plan #5) for Phase B.

# Blocked on

Operator merge review for PR #4 and PR #5. Once PR #4 merges, the rest of
Phase B (chat-rail, command-palette, llm-providers, settings, strategy-2a-mcp,
strategy-2b-skills) becomes available. Once both #4 and #5 merge, eval-engine
can begin.

# Next up for this session

The third Phase A track — `frontend-foundation` — has independent scaffolding
work (Vite/Tailwind/Tokens/Shell) that can start NOW without waiting on
either PR. Phase B work for that track waits for PR #4.

Reasonable next moves:
1. Pick up `frontend-foundation` Phase A scaffolding in this session — Vite/
   Tailwind/Tokens/Shell from prototype, plus `xvision-dashboard` axum crate
   skeleton with stubbed routes. No backend dependency.
2. OR start preparing `eval-engine` plan execution (read-only — actually
   implementing has to wait for both PRs to merge).
3. OR await operator merge review and react.

# Tracks ready for external CLI pickup

- `frontend-foundation` — Phase A scaffolding can start NOW; Phase B (the
  `/strategies` API integration) waits for PR #4 merge.

To spawn an external CLI on it:
```
cd /Users/edkennedy/Code/xvision/.worktrees/frontend-foundation
claude
# inside Claude:
#   1. Read team/MANIFEST.md
#   2. Read team/briefings/frontend-foundation.md
#   3. Begin Phase A work
```
