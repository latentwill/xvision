---
track: qa-strategy-id-path-safety
lane: leaf
wave: qa-2026-05-17
worktree: .worktrees/qa-strategy-id-path-safety
branch: task/qa-strategy-id-path-safety
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/src/strategies/id.rs
  - crates/xvision-engine/src/authoring.rs
  - crates/xvision-engine/src/api/strategy.rs
  - crates/xvision-engine/tests/strategy_id_path_safety.rs
forbidden_paths:
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/strategies/agent_ref.rs
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/migrations/**
  - frontend/**
interfaces_used:
  - "xvision_engine::strategies::store::FilesystemStore"
  - "xvision_engine::strategies::store::path_for"
  - "xvision_engine::api::strategy — load/delete/update entry points"
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build -p xvision-engine
  - cargo test -p xvision-engine strategies::store
  - cargo test -p xvision-engine --test strategy_id_path_safety
acceptance:
  - "A `validate_strategy_id_for_path` helper (or typed `StrategyId`) rejects strategy IDs that don't match a strict filename-safe pattern (e.g. `^[A-Za-z0-9_-]+$`)"
  - "Every `FilesystemStore` operation that joins an ID into a path (load, delete, update, exists, path_for) validates the ID first and returns a typed error on failure — no path traversal slips through"
  - "API/MCP/tool surfaces (`authoring.rs`, `api/strategy.rs`) propagate the validation error as a 4xx-style response, not a generic 500"
  - "Regression tests cover: `../`, leading/trailing path separators, `.`, `..`, embedded slashes, backslashes, NUL bytes, and (a) confirm rejection and (b) confirm the strategy store root is unchanged after the rejected operation"
  - "Existing happy-path tests (ULID-shaped IDs) still pass"
  - "Existing strategy fixtures and on-disk store layouts are unchanged — this is an input validation track, not a storage migration"
---

# Scope

Implements remediation step 5 of `qa/2026-05-17-comprehensive-codebase-review.md`
("Strategy filesystem store does not constrain IDs before joining paths").
Adds a path-safe validation layer before any `FilesystemStore` operation
that joins a caller-supplied ID into a filesystem path.

Most creation paths use ULIDs and most HTTP routes constrain the segment
shape, but the store abstraction itself has no invariant. This track
introduces the invariant at the lowest layer (`store.rs`) and propagates
the typed error through callers.

# Out of scope

- Migrating existing on-disk strategy filenames or layouts. ULID-shaped
  IDs already match the strict pattern.
- Renaming or moving the strategy store root.
- Adding validation to scenario or other entity stores (only strategies
  were flagged; sibling stores can be a follow-up if a similar issue is
  found).
- Role normalization — owned by `qa-role-normalization`.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-strategy-id-path-safety \
  -b task/qa-strategy-id-path-safety origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git -C .worktrees/qa-strategy-id-path-safety status
```

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- Prefer a typed `StrategyId(String)` newtype with a `try_new` constructor
  that runs the regex once. Then `FilesystemStore` accepts `&StrategyId`
  rather than `&str`. This collapses validation to a single boundary.
- If the newtype is too invasive for one PR, a free function
  `validate_strategy_id_for_path(&str) -> Result<&str, StoreError>` called
  at the top of every store method is acceptable.
- The fixed `.json` suffix in `path_for` does not fully defend against
  `../` traversal because traversal still resolves to a sibling `.json`
  file. The validation must reject path separators outright.
- Add a brief comment near the validator citing the QA finding ID
  (P3-strategy-id) so future readers know why the regex is so strict.
