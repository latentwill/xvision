---
track: seed-scaffolding-cleanup
lane: leaf
wave: eval-honesty-2026-05-21
worktree: .worktrees/agent-a398202a6680ed205
branch: task/seed-scaffolding-cleanup
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - team/contracts/seed-scaffolding-cleanup.md
  - crates/xvision-engine/tests/seeded_artifacts.rs
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - frontend/**
  - crates/xvision-engine/src/agents/store.rs
  - crates/xvision-engine/src/strategies/store.rs
interfaces_used:
  - xvision_engine::strategies::templates::example_strategies
  - xvision_engine::strategies::templates::example_scenarios
  - xvision_engine::agents::templates::builtin_templates
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy --workspace -- -D warnings
  - cargo test --workspace
acceptance:
  - The `example_strategies()` list does not contain any entry with display_name "Test Strategy", name "Template-ish Agent", or name "Template Mean Reversion Agent".
  - The `builtin_templates()` agent template list does not contain any entry with name "Template-ish Agent" or "Template Mean Reversion Agent".
  - The new `seeded_artifacts` integration test in `crates/xvision-engine/tests/` asserts that none of the three placeholder names appear in seeded strategy or agent template output.
  - Existing operator workspaces are NOT touched; no migration is added. The cleanup is seed-side only.
---

# Scope

Remove three scaffolding artifacts from the seed path that were mistaken for
shipped templates during a QA rerun on `xvnej-app`. The three artifacts:

- Strategy `Test Strategy` (id `01KS204HPAWFXP72TXVXNCTYPX`) — leftover
  scaffolding, not a production candidate.
- Agent `Template-ish Agent` (id `01KS1VD3V3PNYN4E8B1BCJ4N11`) — generic
  discretionary trader placeholder.
- Agent `Template Mean Reversion Agent` (id `01KS1VDHFKZGJJ59S7XQPCA45C`)
  whose entire prompt was "You are a mean reversion trader. Look for fade and
  reversion in the briefing." — placeholder, not a real playbook.

These showed up in the eval catalog alongside real strategies and were mistaken
for shipped templates. Investigation confirmed they are DB-only artifacts
(already in existing workspaces) and do NOT currently appear in any Rust seed
source — the seed code already correctly limits seeding to the three
`example-` prefixed strategies. This track adds a regression test that
explicitly asserts the scaffold names cannot re-enter the seed path.

Note: existing operator workspaces may already have these rows in `xvn.db`.
Cleanup is seed-side only — new workspaces will never receive them.
Operators wishing to clean existing rows can use `xvn agent archive <id>`
and `xvn strategy archive <id>` manually.

Source: `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`, track
`seed-scaffolding-cleanup`.

# Out of scope

- Migrations — existing workspace data is not touched.
- Frontend — no UI changes.
- Archive mechanism — already exists; this track only locks the seed path.
- The four canonical strategy templates (`trend_follower`, `breakout`,
  `mean_reversion`, etc.) — not placeholders, not touched.
- The nine canonical agent starter templates in `agents/templates.rs` — all
  are legitimate; not touched.
- Any QA pinned-fixture sets that reference these names by a stable fixture
  id (none found — no rename needed).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .claude/worktrees/agent-a398202a6680ed205 status
git -C .claude/worktrees/agent-a398202a6680ed205 log --oneline -3 origin/main..HEAD
```

# Notes

2026-05-21: Investigation confirmed the three artifacts are absent from Rust
source. The seed path in `crates/xvision-engine/src/strategies/templates.rs`
already only emits `example-trend-follower`, `example-mean-reversion`, and
`example-breakout`. The agent template registry in
`crates/xvision-engine/src/agents/templates.rs` has nine legitimate starters,
none matching the placeholder names. This track adds a named regression test
`seeded_artifacts.rs` so any future re-introduction of those names is caught
at CI time.
