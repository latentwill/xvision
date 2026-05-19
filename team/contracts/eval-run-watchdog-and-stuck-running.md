---
track: eval-run-watchdog-and-stuck-running
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-run-watchdog-and-stuck-running
branch: task/eval-run-watchdog-and-stuck-running
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/watchdog.rs            # new file
  - crates/xvision-engine/src/eval/mod.rs                 # only the `pub mod watchdog;` declaration + spawn hook
  - crates/xvision-engine/src/eval/executor/mod.rs        # only the boot-sweep call + watchdog lifecycle hooks; NO classifier edits
  - crates/xvision-engine/tests/**
  - crates/xvision-engine/src/config.rs                   # if it carries `eval` config; add `max_run_duration_secs` knob
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision-engine::eval::executor::finalize_run (or the existing finalize-as-failed path used by the 429 handler)
  - xvision-engine::eval::EvalRunStore (or whatever wraps `eval_runs`)
parallel_safe: true
parallel_conflicts:
  - eval-provider-error-classify-retry (also touches eval/executor/mod.rs — keep edits disjoint; the classifier arm vs the watchdog hook are different sections)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::watchdog
  - cargo test -p xvision-engine eval::executor
acceptance:
  - A new `eval::watchdog` module exposes a background task that periodically (configurable, default 30s) scans `eval_runs` for rows where `status='running'` and `started_at < now() - max_run_duration_secs` (default 30min). Matching rows are finalized as `status='failed'`, `error='timeout'`, `completed_at=now()`. The same code path that today writes `failed` on a provider error is reused (no second finalize implementation).
  - A one-shot **boot sweep** runs on engine startup that performs the same finalize for any pre-existing `running` rows (so a container restart cleans up orphaned runs from a previous process). The audit's `01KS0A5DP8KZVQJ03TCKGKYJVN` (started 14:27:45Z with no progress) is the prototypical case.
  - `max_run_duration_secs` is configurable per scenario via an optional override on `eval_runs.params_override_json` (or wherever scenario-level overrides live); falls back to the global engine setting if absent.
  - Tests:
    * Unit test: a row inserted with `status='running'` and `started_at` older than the threshold is finalized to `failed` with the expected `error` string after one watchdog tick.
    * Unit test: a row started within the threshold is left alone.
    * Unit test: the boot sweep finalizes a pre-existing stuck row on startup.
    * Race test (best-effort): two ticks of the watchdog on the same stuck row do not double-write (idempotent finalize).
  - No new migrations.
  - The slow-statement warning observed in the audit (`UPDATE eval_runs SET status='failed' … elapsed=1.029s`) is acknowledged but not fixed here — that hotspot is F-1's territory.
---

# Scope

Intake F-3 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.
Engine-side watchdog plus boot-sweep so `running` rows can't survive
past `max_run_duration` and can't survive a container restart.

This is a leaf-S fix; the only reason it isn't a one-liner is that the
boot sweep needs to compose cleanly with the existing finalize path
(don't write a second one).

# Out of scope

- Concurrency caps / 429 backoff (F-1).
- Provider error classification (F-2).
- Any frontend changes — the dashboard already renders `failed` rows;
  it just hasn't been getting them for stuck `running` rows.
- The `slow statement` UPDATE hotspot on `eval_runs.status` — F-1.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-run-watchdog-and-stuck-running status
git -C .worktrees/eval-run-watchdog-and-stuck-running log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-run-watchdog-and-stuck-running -b task/eval-run-watchdog-and-stuck-running origin/main
```

# Notes

Append checkpoints below. Do not edit the frontmatter above the line
without a contract-update PR.
