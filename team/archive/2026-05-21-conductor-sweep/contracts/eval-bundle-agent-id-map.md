---
track: eval-bundle-agent-id-map
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-bundle-agent-id-map
branch: task/eval-bundle-agent-id-map
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/store.rs                       # add agents.agent_id column to eval_runs read/write
  - crates/xvision-engine/src/eval/run.rs                         # populate agent_id (the long-lived ULID) at eval start
  - crates/xvision-engine/src/api/eval.rs                         # expose lookup helper through the API surface used by the dashboard
  - crates/xvision-engine/migrations/022_eval_runs_agents_agent_id.sql       # NEW
  - crates/xvision-engine/migrations/022_eval_runs_agents_agent_id.down.sql  # NEW
  - team/MANIFEST.md                                              # coordinate registry update — if eval-causal-input-sanitization (PR #354) already landed migration 020 and its MANIFEST update, this PR adds row "021 | eval-bundle-agent-id-map"
  - crates/xvision-engine/tests/**
forbidden_paths:
  - frontend/web/**
  - crates/xvision-engine/src/eval/executor/**            # the executor doesn't need to know about agents.agent_id resolution
interfaces_used:
  - xvision-engine::agents::AgentStore::get_by_bundle (may not exist — add if needed under `crates/xvision-engine/src/agents/**`; **expand allowed_paths via contract-update PR if so**)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::store
  - cargo test -p xvision-engine api::eval
acceptance:
  - Migration **021** adds `agents_agent_id TEXT` (nullable) column to `eval_runs`. Down migration drops it. The column is populated at eval-start time from the agent record's `agents.agent_id` (the long-lived ULID), in parallel with the existing `agent_id` column (which is the bundle artifact hash per `crates/xvision-engine/migrations/014_eval_agent_id.sql` and the eval_runs schema comment "bundle artifact hash"). Old rows have `NULL` here and a one-shot backfill is **out of scope** — the column starts populating from this migration forward.
  - `eval::run::start_eval_run` (or whichever function inserts into `eval_runs`) now resolves the calling agent's long-lived `agents.agent_id` and writes it alongside the bundle hash.
  - A new helper exposed through `api/eval.rs` — `lookup_agent_for_eval_run(run_id) -> Option<AgentSummary>` — returns the agent record bound to the run if `agents_agent_id` is populated, otherwise returns `None` (no fallback regex parsing of `agent_runs.objective`; old rows simply don't resolve).
  - MANIFEST.md migration registry adds row for 021. If eval-causal-input-sanitization (PR #354) has already merged and updated MANIFEST to reflect on-disk state through 020, this PR's diff is +1 row. If #354 hasn't merged yet, this PR's diff is the same +2-row update (006→020 catch-up + 021 reservation) and the **first to merge wins**; the second rebases.
  - Tests:
    * Migration up + down + up round-trip preserves rows (no data loss on a row that already has both `agent_id` and `agents_agent_id` populated).
    * Integration: start a new eval run and assert `eval_runs.agents_agent_id` matches the calling agent's `agents.agent_id`.
    * Integration: `lookup_agent_for_eval_run` returns `Some(...)` for a freshly-started run and `None` for an old row inserted directly into the DB with `agents_agent_id=NULL`.
  - No frontend change required — the dashboard's existing eval-run detail surface can pick up the new field whenever it next adds the column to its DTO (out of scope here).
---

# Scope

Intake F-11 (sub-bullet: bundle → agent map) of
`team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The audit found that `eval_runs.agent_id` is documented as a "bundle
artifact hash" but no `bundle → agents.agent_id` lookup exists; finding
the agent that produced a run requires regex-parsing
`agent_runs.objective` strings. This contract adds a sibling
`agents_agent_id` column on `eval_runs` that's populated at eval-start
time from the long-lived ULID and a clean lookup API.

# Out of scope

- Backfilling old rows. The audit's existing 56 runs stay with NULL in
  the new column; nobody is asking to map their bundle hashes back to
  agents post-hoc.
- The dashboard / frontend DTO surface for the new column.
- Touching the eval-executor codepath.

# Migration coordination

Claims migration **021**. The wave's other migration is **020**
(`eval-causal-input-sanitization`, PR #354). If 354 lands first, MANIFEST
is at 020 and this PR appends 021. If this PR lands first, MANIFEST
catch-up + 021 happens here; 354 then rebases its MANIFEST hunk.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-bundle-agent-id-map status
git -C .worktrees/eval-bundle-agent-id-map log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-bundle-agent-id-map -b task/eval-bundle-agent-id-map origin/main
```

# Notes

If `AgentStore::get_by_bundle` doesn't exist and you'd otherwise have to
add it, push back via a contract-update PR before expanding the diff —
F-5 (`agent-prompt-tool-schema-drift-lint`, PR #346) recently modified
`agents/store.rs` and the change should be coordinated.
