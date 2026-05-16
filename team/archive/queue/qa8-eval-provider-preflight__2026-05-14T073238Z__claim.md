---
track: qa8-eval-provider-preflight
owner: codex
branch: qa8-eval-provider-preflight
worktree: /root/deploy/xvision/.worktrees/qa8-eval-provider-preflight
claimed_at: 2026-05-14T07:32:38Z
status: in-progress
---

# Scope

Prevent Web UI eval and wizard flows from launching with unconfigured `openai`/`anthropic` defaults. Require configured provider/model selection or a clear zero-provider setup action.

# Verification Plan

- Add/extend focused frontend tests for eval launch preflight.
- Use frontend tests/typecheck where possible.
- Do not run local cargo on this deploy host per `CLAUDE.md`.
