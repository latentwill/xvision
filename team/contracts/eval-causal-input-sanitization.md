---
track: eval-causal-input-sanitization
lane: integration
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-causal-input-sanitization
branch: task/eval-causal-input-sanitization
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/paper.rs       # bar_seed + ohlcv_to_json
  - crates/xvision-engine/src/eval/executor/backtest.rs    # mirror site for bar_history serialization
  - crates/xvision-engine/src/eval/executor/mod.rs         # only the top-level user-message construction (where decision_index is added) and call-sites of bar_seed
  - crates/xvision-engine/src/agents/**                    # new inputs_policy column on agent_slots
  - crates/xvision-engine/migrations/020_agent_slot_inputs_policy.sql        # NEW
  - crates/xvision-engine/migrations/020_agent_slot_inputs_policy.down.sql   # NEW
  - team/MANIFEST.md                                       # add row "020 | eval-causal-input-sanitization | merged" once PR opens
  - crates/xvision-engine/tests/**
forbidden_paths:
  - frontend/web/**
interfaces_used:
  - xvision-engine::eval::executor::{paper, backtest}::bar_seed (or its caller)
  - xvision-engine::agents::AgentStore
parallel_safe: true
parallel_conflicts:
  - engine-trade-guardrails-pyramid-flip-block (also edits paper.rs + backtest.rs but in apply-decision sections — disjoint from bar_seed)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::executor
acceptance:
  - **Migration 020** adds an `inputs_policy` column to `agent_slots` (TEXT NOT NULL DEFAULT 'raw'). Allowed values: `raw` (current behaviour), `causal` (strip timestamp + decision_index), `oracle` (preserve timestamp + decision_index but tag the run as oracle in any future eval-comparison report). Down migration drops the column. Update `team/MANIFEST.md` migration registry table — current state is stale (says next-free is 006 but migrations go to 019); reserve 020 for this track and note in the registry that 006/008/009 are unallocated historical gaps.
  - `crates/xvision-engine/src/eval/executor/paper.rs::bar_seed` (and the homologous block in `backtest.rs` around line ~397-419) gain a `inputs_policy: InputsPolicy` parameter (or read it from the resolved agent slot already in scope). When `policy == Causal`:
    * `ohlcv_to_json` emits `bar_index: usize` instead of `timestamp`. Bar index is relative to the start of `bar_history` (so index 0 is the oldest visible bar).
    * The top-level user message strips `decision_index` (this field is set in `mod.rs` — find the construction site via `rg '"decision_index"' crates/xvision-engine/src/eval/executor/`).
  - When `policy == Oracle`, behaviour is identical to `raw` today but the agent's run is marked as `oracle` via a new optional `inputs_policy` column on `eval_runs` (or via a tag on the resolved slot) so downstream eval-comparison tools can refuse to mix causal+oracle in the same report. This contract only persists the tag; report-side filtering is out of scope.
  - `AgentStore::create` / `AgentStore::update` accept and round-trip the new field. Default is `raw`.
  - Two existing agents in seeded data (`BTC 1h timestamp trend rider v2`, `BTC 1h timestamp swing oracle v3`) are flagged as `oracle` via a one-shot data fix in the migration. A safer route: don't auto-flag — just mention in the migration comment that these two should be re-saved as `oracle` from the UI/CLI.
  - Tests:
    * Unit test: an agent slot with `policy=Causal` produces a `bar_history` whose entries have `bar_index` and no `timestamp`, and a user message with no top-level `decision_index`.
    * Unit test: an agent slot with `policy=Raw` produces exactly today's JSON (regression guard).
    * Unit test: migration up + down + up round-trip preserves rows.
    * Integration test: round-trip an `AgentSlot` with each policy value through `AgentStore`.
  - Forbidden: changing `Ohlcv` itself, the broker call surface, or anything in frontend/web.
---

# Scope

Intake F-6 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The "v4 causal" trader prompts include:

> Absolute prohibitions: Do not use timestamp, calendar date, run
> position, decision_index, or memorized scenario windows.

But the harness sends:

```json
{"asset":"SOL/USD","decision_index":0,
 "market_data":{"asset":"SOL/USD","bar_history":[
   {"close":127.0205,"timestamp":"2026-01-24T20:00:00Z", ...},
   ... 199 more ...]}}
```

The agent cannot comply — `timestamp` and `decision_index` are literally
in its context. Fix: a per-slot `inputs_policy` enum that, when set to
`Causal`, strips those fields server-side so the policy is enforced by
construction rather than by prompt language.

`Oracle` is the converse for two existing deliberately-cheating agents
(`BTC 1h timestamp trend rider v2`, `BTC 1h timestamp swing oracle v3`) —
the schema labels them so eval comparisons can refuse to mix them with
causal agents.

# Out of scope

- Eval-comparison report filtering by policy (just persist the tag).
- Auto-rewriting existing seeded agents — re-save from UI/CLI.
- Anything in `frontend/web/**`.
- The bar history rolling-window size (that's F-8).

# Migration reservation

`crates/xvision-engine/migrations/` next-free number on disk is **020**
(despite MANIFEST.md still claiming 006 is next). This contract claims
020 and **also** updates the MANIFEST.md migration registry to reflect
on-disk state (006/008/009 are historical gaps).

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-causal-input-sanitization status
git -C .worktrees/eval-causal-input-sanitization log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-causal-input-sanitization -b task/eval-causal-input-sanitization origin/main
```

# Notes

Coordinate with `engine-trade-guardrails-pyramid-flip-block` (touches
the same two files but different functions — bar_seed vs apply-decision).
First to merge stays put; second rebases.
