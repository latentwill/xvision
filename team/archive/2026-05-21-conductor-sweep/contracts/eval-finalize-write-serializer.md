---
track: eval-finalize-write-serializer
lane: integration
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-finalize-write-serializer
branch: task/eval-finalize-write-serializer
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/store.rs                     # finalize methods
  - crates/xvision-engine/src/eval/finalize_writer.rs           # NEW — bounded mpsc serializer task
  - crates/xvision-engine/src/eval/mod.rs                       # `pub mod finalize_writer;`
  - crates/xvision-engine/src/api/mod.rs                        # ApiContext gets a FinalizeWriter handle
  - crates/xvision-engine/src/api/eval.rs                       # route writes through the serializer
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
  - crates/xvision-engine/src/eval/executor/**    # finalize is API-layer; executors call up
interfaces_used:
  - xvision-engine::eval::store::RunStore::fail_active (or equivalent)
  - tokio::sync::mpsc
parallel_safe: true
parallel_conflicts:
  - eval-bundle-agent-id-map (PR #359, F-11 — also touches eval/store.rs but for a column add; should be separate hunks)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::finalize_writer
  - cargo test -p xvision-engine eval::store
acceptance:
  - New `eval::finalize_writer` module exposes a `FinalizeWriter` with a single tokio task that owns a bounded `mpsc::Receiver<FinalizeMsg>`. `FinalizeMsg` variants: `MarkFailed { run_id, error, completed_at }`, `MarkCompleted { run_id, metrics, completed_at }`, optionally `MarkRunning { run_id }` if a similar hotspot exists.
  - The task processes messages serially and batches contiguous `MarkFailed` messages (within a 50ms window or a max-batch of 16) into a single `UPDATE eval_runs SET status = ... WHERE id IN (?, ?, ?)` query so 27 concurrent failures don't serialize one-by-one through sqlx.
  - `FinalizeWriter::send_*(...)` methods are async, send into the channel, and `.await` a `oneshot::Sender` reply so callers still see write success/failure typed.
  - `RunStore` keeps its existing methods for tests and one-off paths, but `api::eval` callers (start_run, watchdog, error-paths) route through `FinalizeWriter`.
  - Backpressure: channel bound = 256 by default (`XVN_FINALIZE_WRITER_CAP`). If full, `send_*` returns `Err(FinalizeError::QueueFull)` rather than blocking, so callers can log + retry on a fast path.
  - Tests:
    * Unit: 27 concurrent `send_mark_failed` calls finalize in ≤ ceil(27/16) batched UPDATEs (count via test sqlx hook or by sniffing query text).
    * Unit: a `oneshot` reply surfaces a typed error if the UPDATE fails (e.g. DB closed).
    * Integration: replay the audit's 27-runs-in-15s scenario; assert no `slow statement` warning fires (or use the test subscriber to capture and assert).
  - **Audit acceptance**: the 1.03s `slow statement` warning from `UPDATE eval_runs SET status='failed' …` under the 27-way contention storm no longer fires. Combined with F-1 (concurrency cap, PR #361 merged) this closes the audit's P0 incident root-cause chain.
---

# Scope

Intake F-1 deferred sub-track of
`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The original F-1 had two prongs: (a) launch-time concurrency cap and
(b) serialize the `eval_runs` finalize-write hotspot. F-1's first prong
landed in PR #361. This contract is the second prong.

The audit captured:
> 2026-05-19T14:23:40Z WARN sqlx::query: slow statement: execution
> time exceeded alert threshold … UPDATE eval_runs SET status='failed'
> … elapsed=1.029s

— under 27-way concurrent finalize-write contention. A single serialize
task that batches contiguous writes turns this into one bulk UPDATE.

# Out of scope

- Replacing SQLite or sharding.
- Replicating to a write-optimized path / outbox.
- Other finalize-adjacent writes (eval_decisions inserts go through
  a different path — leave alone).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-finalize-write-serializer status
git -C .worktrees/eval-finalize-write-serializer log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-finalize-write-serializer -b task/eval-finalize-write-serializer origin/main
```

# Notes

Keep `RunStore` methods as the canonical writer. `FinalizeWriter`
delegates to `RunStore` after batching. That keeps the existing test
surface and watchdog/finalize logic in one place.
