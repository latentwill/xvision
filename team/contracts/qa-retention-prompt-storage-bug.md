---
track: qa-retention-prompt-storage-bug
lane: leaf
wave: qa-operator-2026-05-18
worktree: .worktrees/qa-retention-prompt-storage-bug
branch: task/qa-retention-prompt-storage-bug
base: origin/main
status: pr-open
depends_on: []   # observability-retention-default-full-debug shipped via #252
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-observability/src/redactor.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/export.rs
  - crates/xvision-observability/tests/**
  - crates/xvision-dashboard/src/routes/agent_runs.rs
  - crates/xvision-dashboard/tests/agent_runs_blob_route.rs
  - frontend/web/src/features/agent-runs/SpanInspector.tsx
  - frontend/web/src/features/agent-runs/SpanInspector.test.tsx
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-observability/src/config.rs
  - crates/xvision-observability/src/retention.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/bus.rs
interfaces_used:
  - RetentionMode / RetentionConfig
  - PayloadRedactor
  - SpanInspector blob-fetch slice
parallel_safe: false
parallel_conflicts:
  - "agent-run-observability-blob-fetch-route: also owns crates/xvision-dashboard/src/routes/agent_runs.rs and SpanInspector.tsx. Single-writer claim is held there. Stack on its branch and coordinate via team/queue/."
verification:
  - cargo test -p xvision-observability
  - cargo test -p xvision-dashboard
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run SpanInspector agent-runs
acceptance:
  - Written investigation note in `team/status/qa-retention-prompt-storage-bug.md`
    identifying the root cause of the asymmetry: prompts are redacted
    while responses are stored, even under `full_debug`. Identify
    whether the bug is (a) redactor mode-gating, (b) sqlite write
    column mismatch, (c) dashboard read route projection, or
    (d) SpanInspector rendering keyed on the wrong field.
  - Fix the root cause (no try/catch silencing, no fallback shim) per
    the `feedback_alpha_root_cause` memory.
  - A regression test in `crates/xvision-observability/tests/` (or
    `crates/xvision-dashboard/tests/`) exercises the full round-trip:
    a full_debug run is recorded, the dashboard fetch route returns
    both prompt and response payloads, the SpanInspector renders both.
  - SpanInspector no longer renders the "redacted prompt — body not
    stored on disk" placeholder for runs that were configured with
    `full_debug` retention.
  - If stale rows in the local sqlite were written under the old
    `hash_only` default and cannot be retroactively backfilled, the
    status note documents this AND surfaces a one-line operator
    notice ("runs created before <date> may show redacted prompts —
    re-run to capture") in the SpanInspector fallback path. Do not
    silently render empty.
---

# Scope

Operator reported 2026-05-18: with retention set to `full_debug`, the
agent-run trace dock surfaces response bodies but the prompt fields
still render as redacted. That's asymmetric — either the redactor is
firing on prompts only, the storage path is dropping prompts, the
dashboard read route is projecting only response columns, or the
SpanInspector is keyed on the wrong field.

This is investigative first, fix second. Walk the path end-to-end:

1. Reproduce on a fresh run under `full_debug` (the default since #252).
2. Trace the prompt payload from emit → redactor → sqlite write →
   dashboard fetch route → SpanInspector render.
3. Identify which layer drops it.
4. Fix the root cause and add a round-trip test.

# Out of scope

- Flipping the default retention mode — shipped via #252.
- Adding new retention modes or schema columns.
- Backfilling pre-fix rows. The acceptance allows a one-line operator
  notice for stale rows.
- Streaming text passthrough — shipped via #253; that's a separate
  live-frame issue, not a stored-payload issue.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/qa-retention-prompt-storage-bug status
git -C .worktrees/qa-retention-prompt-storage-bug log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/qa-retention-prompt-storage-bug \
  -b task/qa-retention-prompt-storage-bug origin/main
```

# Notes

Append checkpoints / PR links below. The status note's investigation
section is acceptance-bearing — do not collapse it into the PR
description.
