---
track: memory-forget-undo-snapshot
lane: leaf
wave: memory-safety-and-observability-2026-05-22
worktree: .worktrees/memory-forget-undo-snapshot
branch: task/memory-forget-undo-snapshot
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-memory/src/store.rs
  - crates/xvision-memory/src/types.rs
  - crates/xvision-memory/src/lib.rs
  - crates/xvision-memory/Cargo.toml
  - crates/xvision-memory/tests/forget_undo.rs
  - crates/xvision-engine/src/api/memory.rs
  - crates/xvision-cli/src/commands/memory.rs
  - crates/xvision-cli/tests/memory_cli.rs
  - crates/xvision-dashboard/src/routes/memory.rs
  - docs/v2d-memory-overview.md
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-core/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision_memory::store::MemoryStore::forget (extend to soft-delete)
  - xvision_engine::api::memory::forget (add `restorable_until` to response)
  - xvision_engine::api::memory::undo_forget (NEW)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-memory --test forget_undo
  - cargo test -p xvision-cli --test memory_cli
  - cargo build -p xvision-dashboard
acceptance:
  - `xvn memory forget --namespace <ns>` marks rows with `forgotten_at` instead of DELETE
  - `xvn memory ls` skips rows with non-null `forgotten_at` unless `--include-forgotten`
  - `xvn memory undo-forget --namespace <ns>` restores any row whose `forgotten_at` is within `XVN_MEMORY_FORGET_GRACE_DAYS` (default 14); no-op outside the window
  - Janitor sweep hard-deletes rows whose `forgotten_at` is older than the grace window
  - `XVN_MEMORY_FORGET_GRACE_DAYS=0` collapses to prior behavior (immediate hard-delete) for opt-out
  - `docs/v2d-memory-overview.md` documents the new verb and env var
---

# Scope

Make `xvn memory forget` recoverable. Replace the DELETE in
`MemoryStore::forget` with a soft-delete (`forgotten_at` timestamp);
add a janitor pass that hard-deletes rows older than
`XVN_MEMORY_FORGET_GRACE_DAYS` (default 14); add a CLI/API
`undo-forget` verb that restores rows inside the grace window.

V2D shipped the destructive verb without an undo path. Operator
triage 2026-05-21 promoted this from V2D's deferred list as the
highest-value follow-up.

Source intake: `team/intake/2026-05-21-memory-safety-and-observability.md`.

The `xvision-memory` crate owns its own SQLite schema and can add the
column on next open — **no engine migration required**.

# Out of scope

- Tool-driven memory (V3 candidate; D5 kill list)
- TTL / time decay / LRU eviction (V3 candidate)
- Cross-namespace recall blending (D5 kill list)
- Embedder configuration UI (D5 kill list)
- Frontend UI for undo-forget — CLI + API surface only in v1; UI can layer later if operator demand surfaces

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/memory-forget-undo-snapshot -b task/memory-forget-undo-snapshot origin/main
```

# Notes

Default grace window picked to give an operator a working week + a
weekend to notice an accidental forget. The janitor pass that hard-
deletes expired rows piggybacks on existing memory janitor if one
exists; otherwise add it to the dashboard server's startup or to a
periodic task.
