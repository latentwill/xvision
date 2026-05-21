# Handoff — xvision "what's next?" thread

> Temporary handoff doc. Move or delete when the next thread has consumed it.

**Date:** 2026-05-21 (session env clock shows 2026-05-22).
**Prior session focus:** Designed three interlocked tracks (Multi-Asset, Filter, Live) for xvision, condensed the spec/plan sprawl, identified parallelization opportunities.
**Next session focus (per user argument):** "What is next" — fresh thread to discuss prioritization, sequencing, and kickoff for the work the prior session designed. They have the plan; they need to decide what to start first.

---

## What the prior session produced

All artifacts in `/Users/edkennedy/Code/xvision`. Reference by path; do not re-read content unless a specific question requires it.

### Intakes (decision records)

- `team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md` — **the Live design intake**, 9 locked decisions, 4-track sequencing, multi-asset coordination, open questions deferred to track contracts. Treat as Live's spec.
- `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` — pre-existing; amended this session: row 78 now declares the `agent-graph-composition` track depends on `executor-refactor` and includes per-Filter `granularity`.

### Per-track condensed artifacts

**Multi-Asset:**
- Spec: `docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md` (existing; status header added — M1/M2/M3 mostly SHIPPED, residual = asset unlock)
- Plan: `docs/superpowers/plans/2026-05-21-multi-asset-alpaca-unlock.md` (new)
- Contract: queueable (not yet authored)

**Filter (was Watcher — terminology rename ran this session):**
- Spec: `docs/superpowers/specs/2026-05-21-filter-v1.md` (new — pulled from glamin-cortex-explorations worktree with Filter rename applied)
- Plan: `docs/superpowers/plans/2026-05-21-filter-v1.md` (new — pulled from worktree, exec-refactor dependency note added)
- Contract: `team/contracts/filter-v1.md` (new — pulled from worktree, declares `depends_on: [executor-refactor]`)

**Live (Alpaca):**
- Spec: same intake doc above
- Plan 1: `docs/superpowers/plans/2026-05-21-alpaca-live-1-executor-refactor.md` (new)
- Plan 2: `docs/superpowers/plans/2026-05-21-alpaca-live-2-bar-source.md` (new)
- Plan 3: `docs/superpowers/plans/2026-05-21-alpaca-live-3-launch-and-freeze.md` (new)
- Contracts: 3 queueable (executor-refactor, live-bar-source-alpaca, live-eval-launch-and-freeze)

### Superseded (do not implement against)

- `docs/superpowers/specs/2026-05-14-alpaca-paper-eval-surface-design.md` — PaperExecutor being deleted; superseded header in place
- `docs/superpowers/specs/2026-05-19-eval-evidence-and-agent-filters.md` — superseded header points readers at Filter v1 spec + eval-honesty intake

### Terminology rename done

Watcher → Filter applied case-preservingly across main (~133 substitutions, 6 files, 1 source-file rename). The worktree at `.worktrees/glamin-cortex-explorations/` was not touched — its substantive content is now re-authored in main; the branch is safe to delete. The branch's glamin-cortex notes file (exploration only) was discarded per user direction.

---

## Parallelization plan (locked this session)

**Phase 1 (start immediately — 3 parallel agents):**
1. `executor-refactor` (Live Plan 1)
2. `filter-v1` Stage 1 (Filter contract — pure crate add)
3. `multi-asset-alpaca-unlock` (Multi-Asset plan)

**Phase 2 (after exec-refactor — 2 parallel agents):** `filter-v1` Stage 2 + `live-bar-source-alpaca`

**Phase 3 (after Phase 2 — 2 parallel agents):** `filter-v1` Stage 3 + `live-eval-launch-and-freeze`

**Phase 4 (after Phase 3 — 2 parallel agents):** `filter-v1` Stage 4 (frontend) + Stage 5 (regression fixtures)

**Critical path:** `executor-refactor → live-bar-source-alpaca → live-eval-launch-and-freeze` (≈ Live works end-to-end).

**Hot file:** `crates/xvision-engine/src/eval/executor/mod.rs` — touched by 4-5 tracks. Land exec-refactor first; subsequent tracks land deltas on top. Same caution for `eval/run.rs`, `eval/scenario.rs`, `eval/store.rs`.

**Migration coordination:** 3 new migrations needed across the plan. Reserve consecutive numbers via `team/MANIFEST.md` before any plan starts.

---

## Open decisions for the "what's next" thread to resolve

1. **Track contracts to author** (5 needed). Filter v1's done. Missing: `executor-refactor`, `live-bar-source-alpaca`, `live-eval-launch-and-freeze`, `multi-asset-alpaca-unlock`.
2. **CLI verb shape for Live launches** (open question in Live intake): `xvn eval run --mode=live ...` vs. dedicated `xvn live run ...`. Plan 3 currently recommends the flag form; user should confirm.
3. **`FillSink` error-class wire compatibility** (Plan 1 acceptance criterion): refactor must preserve the `classify_run_failure` taxonomy.
4. **Multi-Filter signal cardinality per cycle** (deferred to `agent-graph-composition`): if two Filters fire same bar, does the trader run once or twice? Affects `decision_limit` accounting.
5. **Worktree cleanup**: `.worktrees/glamin-cortex-explorations/` is safe to delete (`git worktree remove .worktrees/glamin-cortex-explorations && git branch -D <branch>`).
6. **Agent assignment** for Phase 1's three parallel workstreams.

---

## Recommended order of operations for the next thread

1. **Confirm Phase 1 scope.** Three workstreams or staged? If staging, single best starter is `executor-refactor` (foundation; owns the hot file).
2. **Author the four missing track contracts** — model each on `team/contracts/filter-v1.md`. A subagent can draft in parallel.
3. **Reserve migration numbers** in `team/MANIFEST.md`.
4. **Delete the worktree** (one-line cleanup, low-risk).
5. **Resolve open decisions 2–4** above before code starts.

---

## Suggested skills for the next session

- **`matt-pocock-skills:to-issues`** — convert plan files into independently-grabbable issues / contracts. Useful for drafting the 4 missing contracts.
- **`matt-pocock-skills:triage`** — issue-triage state machine. Useful for prioritizing which missing contract to author first.
- **`anthropic-skills:dispatching-parallel-agents`** — when ready to kick off Phase 1's three workstreams.
- **`anthropic-skills:executing-plans`** — once a single plan is picked up, execute task-by-task with review checkpoints.
- **`matt-pocock-skills:zoom-out`** — if the new thread wants broader codebase orientation before deciding.

Probably NOT needed: `brainstorming` (already exhausted), `grill-me` (decisions locked), `writing-plans` (plans written).

---

## Guardrails the next session must respect

From `/Users/edkennedy/Code/xvision/CLAUDE.md`:

- **No popups in the dashboard SPA.** Inline forms / routes / docks only.
- **Deploy guardrails.** Local image build preferred; never `cargo` on remote hosts; source `.op_env` before `gh`/`op`.
- **Terminology lock.** `cycle_id`, `Strategy`, `Agent`, **Filter** (newly added this session).
- **Cargo target discipline.** Use `CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` from temporary worktrees.
- **Team coordination files.** `team/board.md`, `team/MANIFEST.md`, `team/CONFLICT_ZONES.md`. Contracts go in `team/contracts/`.

---

## Sensitive info

None. No API keys, credentials, or PII in the conversation. Alpaca paper creds referenced only as env-var names (`ALPACA_API_KEY_ID` / `ALPACA_API_SECRET_KEY`); no values shipped.

---

## Pointer index

```
team/intake/2026-05-21-alpaca-live-eval-and-executor-refactor.md      Live spec / intake
team/intake/2026-05-21-eval-honesty-and-agent-graph.md                Filter umbrella intake

docs/superpowers/specs/2026-05-21-filter-v1.md                         Filter spec
docs/superpowers/specs/2026-05-11-custom-scenario-eval-design.md       Multi-Asset spec

docs/superpowers/plans/2026-05-21-filter-v1.md                         Filter plan, 5 stages
docs/superpowers/plans/2026-05-21-multi-asset-alpaca-unlock.md         Multi-Asset plan
docs/superpowers/plans/2026-05-21-alpaca-live-1-executor-refactor.md   Live Plan 1
docs/superpowers/plans/2026-05-21-alpaca-live-2-bar-source.md          Live Plan 2
docs/superpowers/plans/2026-05-21-alpaca-live-3-launch-and-freeze.md   Live Plan 3

team/contracts/filter-v1.md                                            Only contract written

CLAUDE.md                                                              Project guardrails
team/board.md, team/MANIFEST.md, team/CONFLICT_ZONES.md                Conductor coordination
FOLLOWUPS.md §F18/§F30/§F31                                            Related tracking
```
