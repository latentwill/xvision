---
track: watcher-v1-shape
lane: foundation
wave: post-v2d-eval-honesty
worktree: .worktrees/watcher-v1-shape
branch: task/watcher-v1-shape
base: origin/main
status: ready
depends_on: []
blocks:
  - watcher-v1-runtime
  - watcher-v1-export
  - watcher-v1-frontend
  - watcher-v1-fixtures
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/watcher.rs
  - crates/xvision-engine/src/strategies/watcher_validate.rs
  - crates/xvision-engine/src/strategies/mod.rs
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/eval/store.rs
  - crates/xvision-engine/migrations/032_strategy_watcher.sql
  - crates/xvision-engine/migrations/032_strategy_watcher.down.sql
  - crates/xvision-engine/tests/watcher_validate.rs
  - crates/xvision-engine/tests/strategy_serde_compat.rs
  - crates/xvision-engine/tests/fixtures/strategies/**
  - docs/superpowers/specs/2026-05-21-watcher-v1-shape.md
  - docs/superpowers/plans/2026-05-21-watcher-v1-implementation.md
forbidden_paths:
  - crates/xvision-engine/src/eval/executor/**
  - crates/xvision-engine/src/eval/export.rs
  - crates/xvision-engine/src/eval/watcher_eval.rs
  - crates/xvision-engine/src/api/**
  - crates/xvision-engine/src/strategies/agent_ref.rs
  - frontend/web/**
  - team/board.md
  - decisions/**
interfaces_used:
  - xvision_engine::strategies::Strategy
  - xvision_engine::strategies::validate
  - xvision_core::market::IndicatorPanel
  - serde::{Serialize, Deserialize}
  - ts_rs::TS (behind ts-export feature)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-engine --tests watcher_validate strategy_serde_compat
  - cargo test --workspace
  - cargo fmt --check
  - cargo clippy --workspace -- -D warnings
  - bash scripts/board-lint.sh
acceptance:
  - Strategy parses with serde defaults activation_mode=every_bar and watcher=None on every fixture under crates/xvision-engine/tests/fixtures/strategies/
  - Strategy with activation_mode=watcher_gated and a valid Watcher round-trips through serialize → SQL save → load → deserialize unchanged
  - Save API rejects each of the ten validation rules from the spec with a distinct stable error code (watcher.schema_version, watcher.asset, watcher.timeframe, watcher.indicator_unknown, watcher.op_range_required, watcher.cross_requires_indicators, watcher.limits_exceeded, watcher.no_conditions, watcher.activation_mode_requires_watcher, watcher.display_name_invalid)
  - Migration 032 is additive (only ADD COLUMN ... DEFAULT) and has a working down migration
  - No changes outside allowed_paths
---

# Scope

Land the watcher v1 *shape* layer: types, validation, serde, migration.
No runtime wiring, no export changes, no frontend. Implements Stage 1 of
`docs/superpowers/plans/2026-05-21-watcher-v1-implementation.md`,
following the normative spec at
`docs/superpowers/specs/2026-05-21-watcher-v1-shape.md`.

After this track merges, the system can persist watchers but does not
yet act on them. Stages 2–5 are separate tracks that depend on this one.

# Out of scope

- The per-bar evaluator and any wiring into the backtest loop.
  (Belongs to `watcher-v1-runtime`.)
- `WatcherEventV1` rows on the export and the `watcher_summary` block.
  (Belongs to `watcher-v1-export`.)
- Frontend types, the strategy-editor watcher card, and the
  run-review watcher panel. (Belongs to `watcher-v1-frontend`.)
- The acceptance-fixture watcher and golden export.
  (Belongs to `watcher-v1-fixtures`.)
- Any change to `AgentRef` shape. The `agent-graph-composition` track
  owns adding `kind`. Watcher v1 is field-on-strategy, not field-on-agent.
- Briefing schema changes beyond what serde defaults already permit.

If something here later proves wrong, push a contract-update PR before
any code PR.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/watcher-v1-shape status
git -C .worktrees/watcher-v1-shape log --oneline -3 origin/main..HEAD
# Confirm:
#   - clean working tree
#   - branch is task/watcher-v1-shape
#   - base is up to date with origin/main (or rebase planned)
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/watcher-v1-shape -b task/watcher-v1-shape origin/main
```

# Notes

- Spec normative; this plan is sequencing. Disagreements: spec wins.
- The v1 `IndicatorRef` set is locked to the fields
  `xvision_data::indicators::compute_panel_from_fixture` populates today
  (RSI14, SMA20/50/200, EMA12/26, Bollinger20/2, ATR14) plus the derived
  `AtrPct`. MACD is explicitly excluded — adding it requires its own
  intake.
- Determinism is a hard constraint at every stage. The validator forbids
  any future-bar reference; the runtime stages (separate tracks) carry
  the byte-equal-rerun tests.
