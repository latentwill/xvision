---
track: remote-cli-orphan-recovery
worktree: /root/deploy/xvision/.worktrees/remote-cli-orphan-recovery
branch: remote-cli-orphan-recovery-clean
phase: phase-b-pr-open
last_updated: 2026-05-13T02:32:06Z
owner: codex
---

# What I'm doing right now

PR [#99](https://github.com/latentwill/xvision/pull/99) is open for the
execution-board remote CLI orphan recovery gap.

## Progress

- [x] Created board-specified worktree and branch.
- [x] Confirmed the core `/api/cli/jobs*` backend already exists.
- [x] Added regression tests for restart recovery.
- [x] Added store-level recovery for queued/running CLI jobs.
- [x] Wired recovery into dashboard startup.
- [ ] Cargo verification in CI/non-deploy environment.

# Blocked on

Operator review/merge of PR #99, plus cargo verification in CI/non-deploy.

CI/non-deploy verification command:

```bash
cargo test -p xvision-dashboard cli_jobs -- --nocapture
```
