---
track: observability-review-fixes
lane: leaf
wave: agent-run-observability
worktree: .worktrees/observability-review-fixes
branch: task/observability-review-fixes
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/tests/event_bus_drop_oldest.rs
  - crates/xvision-observability/tests/event_bus_synthetic.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-cli/**
interfaces_used:
  - xvision_observability::bus::EventBus
  - xvision_observability::events::AgentRunEvent
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build -p xvision-observability -p xvision-cli -p xvision-engine
  - cargo test -p xvision-observability
acceptance:
  - Bus evicts OLDEST non-lifecycle event on overflow (not newest)
  - Lifecycle-critical events (RunStarted/RunFinished/RunInterrupted/SidecarError) never evicted; producer awaits if every queued event is lifecycle-critical
  - span_id → run_id map populated at publish time so evicted SpanStarted still attributes drops correctly
  - BackpressureDropped marker re-parks count if the queue is still full, surfacing on next consumed event
  - New tests fail against the pre-fix bus and pass against the new one
---

# Scope

Reviewer flagged three issues against the freshly-merged Phase A (#204). One
is a real correctness bug in the event bus; the other two were already fixed
on `main` (pre-merge snapshot misread).

Real fix: replace `tokio::sync::mpsc` (which drops newest on Full) with a
`VecDeque + Notify` ring buffer that drops oldest non-lifecycle event on
overflow, as the contract specifies.

PR: <https://github.com/latentwill/xvision/pull/207> (3 files, +590/-123).

# Out of scope

- Any change outside `crates/xvision-observability/src/bus.rs` and its tests.
- Migration changes.
- Phase B observability work (OTel export, Run Detail UI, xvn_run.json /
  xvn_report.md export) — see `docs/superpowers/plans/2026-05-17-agent-run-observability-ui-implementation-plan.md`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/observability-review-fixes status
git -C .worktrees/observability-review-fixes log --oneline -5 origin/main..HEAD
```

# Notes

- The two "already-fixed-on-main" findings:
  - `xvn obs retention set` uses `ObservabilityConfig::load_from_file(&path)`
    (file only, no env overlay) when seeding the rewrite —
    `crates/xvision-cli/src/commands/obs/retention.rs:113-124`.
  - `truncate_to_max_bytes` calls `null_refs_for_hash(...)` for each evicted
    blob — `crates/xvision-observability/src/janitor.rs:204-261`.
- Test coverage: 24 unit + 4 migration + 2 synthetic + 2 drop-oldest (new) +
  1 saturation + 9 janitor + 7 retention precedence + 2 compile_fail
  doctests = 51 total, green.
