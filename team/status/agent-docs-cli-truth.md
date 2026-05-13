---
track: agent-docs-cli-truth
worktree: /root/deploy/xvision/.worktrees/agent-docs-cli-truth
branch: agent-docs-cli-truth
phase: local-docs-verified
last_updated: 2026-05-13T01:58:04Z
owner: codex
---

# What I'm Doing Right Now

Claimed the execution-board docs/help truth track. Fixed stale agent-facing docs
against source truth: README quickstart, README/frontend route table,
Claude-skill terminology, architecture reference terminology, and MANUAL
incident-response commands.

# Blocked On

The board's Rust verification command,
`cargo test -p xvision-cli help_cli -- --nocapture`, is not run on this deploy
host per `CLAUDE.md`.

# Next Up

Local non-Rust verification passed:

- `bash scripts/check_agent_docs.sh`
- `git diff --check`

Run the cargo help test in CI or a non-deploy development environment before
merge.
