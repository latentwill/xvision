---
track: qa8-inspector-agent-model-picker
worktree: /root/deploy/xvision/.worktrees/qa8-inspector-agent-model-picker
branch: qa8-inspector-agent-model-picker
phase: implemented-verified
last_updated: 2026-05-13T14:56:30Z
---

# What I'm Doing Right Now

Implemented the Inspector add-agent model-picker fix. The Strategy Inspector
now keeps the existing attach-AgentRef selector and adds a compact
create-and-attach path that reuses the shared `ModelPicker` over configured
providers/models, creates a single-slot Agent, then attaches it to the current
strategy role.

# Blocked On

nothing

# Next Up

- [x] Create isolated worktree and branch.
- [x] Record claim/status.
- [x] Inspect Inspector add-agent UI and chat rail model picker.
- [x] Add failing regression for OpenRouter/DeepSeek agent creation from Inspector.
- [x] Implement inline create-and-attach flow.
- [x] Run focused frontend tests and typecheck.

# Verification

- Red: `corepack pnpm --dir frontend/web test -- authoring-risk` failed on
  missing `New agent name` / create-and-attach controls.
- Green: `corepack pnpm --dir frontend/web test -- authoring-risk` passed
  5 tests.
- `corepack pnpm --dir frontend/web typecheck` passed.
- `corepack pnpm --dir frontend/web test` passed: 16 files, 40 tests.
- `git diff --check` passed.
- Rust/Cargo verification intentionally not run on this deploy host per
  `CLAUDE.md`.
