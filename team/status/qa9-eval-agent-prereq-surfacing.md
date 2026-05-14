---
track: qa9-eval-agent-prereq-surfacing
worktree: /root/deploy/xvision/.worktrees/qa9-eval-agent-prereq-surfacing
branch: qa9-eval-agent-prereq-surfacing
base: main
phase: verified
last_updated: 2026-05-14T09:06:35Z
owner: codex
---

# What changed

- Added Strategy Inspector regression coverage for agentless strategies:
  missing AgentRefs are surfaced before eval launch and the eval launcher links
  are withheld.
- Gated the top Inspector eval action on attached strategy agents.
- Gated the Inspector "Run eval" side card on attached strategy agents and
  anchored the user to the Strategy agents section when setup is missing.

# Verification

- Passed: `corepack pnpm --dir frontend/web test -- authoring-risk` (9 tests)
- Passed: `corepack pnpm --dir frontend/web test -- authoring-risk eval-runs` (19 tests)
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `git diff --check`

# Notes

- Rust checks were not run on this deploy host because `CLAUDE.md` forbids
  `cargo`, `cargo build`, `cargo check`, and `cargo test` here.
- Branch was unstacked from `qa8-eval-provider-preflight` onto `main` after
  the prerequisite QA9 branches merged.
