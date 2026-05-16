---
track: qa8-scenario-display-name-contract
owner: codex
branch: codex/predeploy-eval-scenario-guardrails
worktree: /root/deploy/xvision/.worktrees/predeploy-eval-scenario-guardrails
claimed_at: 2026-05-14T07:22:01Z
status: in-progress
---

# Scope

Fix scenario creation/tooling so custom scenarios always carry a required display name and missing-name validation is actionable.

# Verification Plan

- Add focused regression coverage for the missing scenario display-name path.
- Run non-Rust checks locally where possible.
- Do not run `cargo`, `cargo build`, `cargo check`, or `cargo test` on this deploy host per `CLAUDE.md`; leave Rust verification to CI/non-deploy.
