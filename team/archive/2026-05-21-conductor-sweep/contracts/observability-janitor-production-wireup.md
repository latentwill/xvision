---
track: observability-janitor-production-wireup
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/observability-janitor-production-wireup
branch: task/observability-janitor-production-wireup
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/lib.rs                       # engine boot — spawn janitor if not already
  - crates/xvision-engine/src/api/eval.rs                  # eval subsystem startup integration
  - crates/xvision-observability/src/janitor.rs            # only if a small API tweak is needed (e.g. expose a config-from-env helper); avoid behavioural changes
  - crates/xvision-observability/src/lib.rs                # re-exports if missing
  - crates/xvision-dashboard/src/server.rs                 # dashboard boot — spawn janitor if engine boot doesn't already
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision_observability::janitor::{Janitor, JanitorConfig}
  - xvision_observability::blobs::BlobStore
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -p xvision-observability -p xvision-dashboard -- -D warnings
  - cargo test -p xvision-engine janitor
  - cargo test -p xvision-observability janitor
acceptance:
  - The retention janitor in `crates/xvision-observability/src/janitor.rs` (already implemented, with TTL + max-bytes eviction) is spawned at engine boot if it isn't already. Find existing spawn sites via `rg -n 'Janitor::|spawn.*janitor' crates/` — the audit found zero hits in production code paths, so this is wiring up dormant infrastructure.
  - Defaults (set in code, overridable via env or engine config):
    * `payload_ttl_days = 14` (eval payload blobs persist 2 weeks).
    * `max_payload_bytes = 4_000_000_000` (4 GB ceiling on the blob store).
    * Periodic interval: 1 hour.
  - The janitor's `JanitorStats` (rows nulled, files deleted) is logged at `info` once per pass.
  - A safety check on startup: if the blob store dir doesn't exist, the janitor logs and exits silently; it does not panic.
  - Tests:
    * Integration test: starting the engine with a populated blob store + an aged row (timestamp older than TTL) results in the `*_payload_ref` columns being nulled and the unreferenced blob file removed within one tick.
    * Integration test: starting with `max_payload_bytes` lower than the current store size evicts blob files in mtime-ascending order until under the threshold, and corresponding `*_payload_ref` columns are nulled BEFORE the file is deleted (matches the existing janitor contract docs).
  - The audit's `5,568 blobs in /data/agent_runs/blobs/` is bounded going forward by these defaults.
---

# Scope

Intake F-11 (sub-bullet: blob GC) of
`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The janitor already exists with TTL + max-bytes eviction logic
(`crates/xvision-observability/src/janitor.rs`). The audit confirmed
**it is not being spawned in any production codepath** — `rg
'Janitor::' crates/xvision-engine crates/xvision-dashboard` returns
empty. This contract wires it up at engine boot with sensible defaults
and adds two end-to-end tests so a future refactor doesn't quietly
un-wire it.

This is the smallest carve from the F-11 grab-bag. Other F-11 items
(bundle→agent map, cost_usd population, eval_reviews runner,
api_audit polling reduction) are deferred to future batches.

# Out of scope

- Touching `janitor.rs`'s eviction algorithm.
- Migration changes.
- Frontend / UI work.
- Persisting JanitorStats to a metrics table.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/observability-janitor-production-wireup status
git -C .worktrees/observability-janitor-production-wireup log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/observability-janitor-production-wireup -b task/observability-janitor-production-wireup origin/main
```

# Notes

If you find the janitor *is* spawned somewhere and the audit missed it,
the contract becomes "verify defaults match the acceptance values, add
the missing tests, and call it done." Document what you found in the PR
body.
