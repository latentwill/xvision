# V2D cortex memory — tier split and look-ahead leakage

> Design discussion captured 2026-05-21 during a `/grill-me` pass on the
> V2D agent-memory wave (`docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`).
> This note becomes the source for the operator-facing docs that ship as
> Phase 6 of V2D.

## Problem statement

V2D as originally decomposed (`team/intake/2026-05-21-v2d-agent-memory.md`)
ships a single-tier vector store: every post-dispatch decision is
embedded and written into the slot's namespace (`agent:<agent_id>` or
`global`), and recall at the next decision is top-k cosine over that
namespace. This shape works for live agents learning from production
but **leaks future knowledge in backtests**.

Concrete failure mode:

1. Operator runs scenario `flash-crash-2024-08` with agent A.
2. At cycle 47, the trader sees a steep drop and decides to fade. The
   recorder writes "saw 18% drop over 4 bars, fading" into
   `agent:A` memory.
3. Operator re-runs the same scenario.
4. At cycle 47 of the replay, the same bars produce a similar embedding
   query. Cosine recall surfaces the cycle-47 memory from run #1 — the
   agent's `system_prompt` now contains a prior decision made *with full
   knowledge of what happens at this cycle*.

This is not memory. It's look-ahead via cosine similarity. Every
replay of the same scenario gets it. The bigger N is, the more
specifically the recalled item matches what's about to happen.

F-6 (`InputsPolicy::Causal`) already strips wall-clock timestamps from
`bar_history` to defeat the LLM's ability to recognize specific dates.
Single-tier memory hands that defeat back: even with `bar_index`
substitution, the recalled prior decision encodes the future.

## Why a query-time filter is the wrong fix

The natural patch is to tag every memory item with
`(scenario_id, cycle_idx, run_id)` and exclude same-tag hits on
recall. This works for the narrowest leakage (same scenario, same
cycle) but doesn't capture the deeper issue: even cross-scenario, the
recorded text often encodes *specific* observations rather than
abstract patterns. A backtest of scenario B can recall a verbatim
decision from scenario A, leak the specific levels at which A made its
decision, and bias B's reasoning in ways that compound over many runs.

More importantly: query-time filtering relies on every consumer
remembering to apply the filter. The cortex architecture has a
structural answer — make the *type* of the row determine what gets
recalled.

## The cortex tier split

A canonical cortex memory model distinguishes (at minimum) two tiers:

| Tier | Content shape | Provenance | Recalled by agent? | Lifetime |
|---|---|---|---|---|
| **Resources** (episodic) | Concrete observation: "at run R, scenario S, cycle C, saw X-shaped bars, decided Y" | Mandatory: `(run_id, scenario_id, cycle_idx)` baked into the row | **No** | Persistent for analysis; never reach the LLM during decision-time recall |
| **Skills** (semantic / procedural) | Abstracted pattern: "X-shaped bars + high vol → fade tends to work; n=14, +37bps mean" | None (provenance stripped) | **Yes** | Persistent in `agent:<agent_id>` / `global`; recalled at dispatch time |

The agent never reads Resources at decision time. Resources are the
input substrate for an offline distillation pass that produces Skills.
The same recall seam in `execute_slot` becomes a *Skills-only* query —
the dispatcher cannot accidentally surface raw observations because the
storage layer doesn't expose them on the read path the LLM sees.

Critically, the agent's "this happened before, could happen again"
intuition is preserved — Skills *are* exactly that pattern, just
provenance-stripped. What's blocked is verbatim replay of a specific
scenario's cycle.

## Mapping to V2D code

### Schema (migration 027 update)

Before merge, `agent_slots.memory_mode` lands as planned. The
`xvision-memory` crate's `memory_items` table gains a `tier` column:

```sql
ALTER TABLE memory_items ADD COLUMN tier TEXT NOT NULL DEFAULT 'resource';
-- New columns for Resource provenance (NULL for Skills).
ALTER TABLE memory_items ADD COLUMN run_id TEXT;
ALTER TABLE memory_items ADD COLUMN scenario_id TEXT;
ALTER TABLE memory_items ADD COLUMN cycle_idx INTEGER;
CREATE INDEX idx_memory_items_tier_namespace ON memory_items(tier, namespace);
```

`tier ∈ { 'resource', 'skill' }`. Resources are populated by the
auto-recorder; Skills by an explicit distillation pass.

### Recorder (Phase 3 update)

`MemoryRecorder::record` writes `tier = Resource` and populates
`run_id` / `scenario_id` / `cycle_idx` from `SlotInput`. Provenance
is required — a Resource without provenance is a programmer error,
not a fallback.

### Recall (Phase 3 update)

`MemoryRecorder::recall` filters `WHERE tier = 'skill' AND namespace
= ?`. The query intentionally cannot surface Resources, even if the
caller misspells a filter — the type of the row gates the read path.

### v1.0 reality

Until a distillation pass exists, the Skills tier is empty for every
operator. v1 ships with:

- Auto-recorder writing Resources (so the autoresearcher has data to
  consume later).
- Recall returns empty for every query (Skills tier is empty).
- The UI selector still works — the operator picks off/global/
  agent_scoped to choose *which Skills namespace* the agent would read
  from, even though it's empty in v1.
- A `memory_disabled_skills_empty` event surfaces in eval review when
  a non-Off slot's recall returns zero hits — explains the empty UI.

This is the *safe shape*. Memory is wired end-to-end, no leakage is
possible, and the autoresearcher can build on it without retrofit.

## Autoresearcher (V3) interplay

The cortex tier split surfaces a meaningful V3 design constraint:
**the autoresearcher is the distillation pass**. Specifically:

- **Read** — autoresearcher consumes Resources across many runs to
  spot patterns (mutator's "what happened the last 50 times this
  shape appeared?" question).
- **Propose** — candidate Skills with sample counts and outcome stats.
- **Judge** — held-out scenarios validate whether the Skill is
  predictive vs spurious.
- **Promote** — validated Skills are written into the Skills tier
  with provenance stripped.
- **Edit / supersede** — a Skill that fails recent backtests gets
  demoted or replaced. The autoresearcher needs UPDATE / DELETE
  access to the Skills tier; the auto-recorder has INSERT access to
  Resources only.

This implies a stronger API contract than V2D Decision 5 anticipated:

- `MemoryStore::upsert` (auto-recorder) — INSERT into `tier=resource`.
  Rejects `tier=skill` writes (caller must use `upsert_skill` to make
  the intent explicit).
- `MemoryStore::upsert_skill` — INSERT or REPLACE into `tier=skill`.
  Used by the distillation pass / autoresearcher / manual CLI.
- `MemoryStore::demote_skill` — DELETE from `tier=skill` (autoresearcher
  retirement path).
- `MemoryStore::forget` — already exists, applies to whatever the
  namespace's content is.

These additions belong on the V2D crate's API in v1 even if no caller
uses them yet, so V3 can build on a stable surface.

**V3 board note (proposed addition to `team/board-v2.md`):**

> Autoresearcher's mutator/judge/promote loop *is* the cortex
> distillation pass. The autoresearcher gains write access to the
> Skills tier (via `MemoryStore::upsert_skill` /
> `MemoryStore::demote_skill`) and read access to Resources. The
> editing semantics (create / supersede / retire) need to land before
> the first nightly autoresearcher run that targets a Skills-consuming
> agent — otherwise the loop writes nothing back and is purely
> evaluative.

## What about manual skill seeding?

Until V3 lands the autoresearcher, operators may want to seed Skills
by hand — "I know mean-reversion works on the 4h SOL chart between
9pm and midnight; remember that". Two options:

- **v1**: ship Skills tier empty, no seeding UI. Recall always returns
  nothing. Operators wait for V3.
- **v1.1**: `xvn memory add-skill --namespace global "<text>"` CLI
  and a small UI form on the operator profile. Bypasses the
  autoresearcher.

The board V2D notes from this discussion accept the v1.1 shape — V2D
itself stays at "auto-recorder writes Resources, recall reads Skills,
Skills empty unless seeded". The seeding surface is a small follow-up
contract.

## Run-mode gating

Even with the tier split, one open question remains: **do backtests
write Resources at all?** Two views:

- **Yes — Resources are the autoresearcher's substrate.** Backtests
  are exactly where the autoresearcher learns. Resources must
  accumulate from backtest runs or there's nothing to distill.
- **No — backtests are i.i.d. by design.** Production-only Resources
  match how memory works in real-time agents and avoids any chance
  the autoresearcher learns from synthetic patterns.

Resolved: **yes, backtests write Resources** (the autoresearcher
needs them), but Resources tag `run_id` so the autoresearcher can
filter by run mode at distillation time. A future autoresearcher
configuration can say "only distill from paper/live, never from
backtest" if the synthetic-pattern concern bites.

This is a configuration knob on the autoresearcher, not a hard rule in
the recorder.

## Operator mental model (for the docs)

Boiled down for the V2D user-facing docs (Phase 6):

> **Memory in xvision is two-layered.** Your agents *write* observations
> as they run — what they saw, what they decided, when. These
> observations stay in the engine's memory store for analysis. Your
> agents *read* skills — distilled patterns that the autoresearcher
> (or you, manually) has validated as predictive. Reading is one-way:
> agents never see raw observations during a decision, only the
> patterns those observations have been distilled into. That's why
> running a backtest twice doesn't give the agent foreknowledge of
> the outcome.

## What this changes in the Phase plan

V2D's plan grows a Phase 1.5 (between current Phases 1 and 2):

- **Phase 1.5** — extend the `xvision-memory` crate with the
  `tier` column + Resource provenance fields + tier-discriminated
  read/write API.
- **Phase 2** — schema migration 027 adds the column (rebuild path
  for SQLite); existing tests get the `tier='skill'` exclusion
  filter on recall.
- **Phase 3** — Phase 3's recorder code already exists; needs
  amendment to write `tier=Resource` with provenance.
- **Phase 6 (new)** — operator-facing docs (this note becomes the
  source) under `docs/v2d-memory-overview.md` or the equivalent
  user-facing location.

## Acceptance criteria additions

- Backtest run R writes only `tier=resource` rows.
- Backtest replay R' recalls zero rows for a non-Off slot (Skills is
  empty in v1).
- `MemoryStore::upsert` rejects `tier=skill` writes — caller must use
  the explicit `upsert_skill` API.
- Recall queries `WHERE tier='skill'`; cannot accidentally surface
  Resources.
- Provenance fields are non-NULL on every Resource row.
- Provenance fields are NULL on every Skill row (Skills are
  abstractions, not attributions).

## Open questions deferred to follow-ups

- **Manual skill seeding UX.** `xvn memory add-skill` CLI design is
  a small follow-up contract; not blocking V2D.
- **Distillation pass spec.** Belongs in V3 autoresearcher intake;
  V2D ships the API surface, not the implementation.
- **Skill editing — supersede vs replace.** When the autoresearcher
  invalidates an old skill, does it delete or mark deprecated?
  Recommend: soft-delete via a `deprecated_at` timestamp so
  longitudinal autoresearcher logs can compare before/after.
- **Operator audit of Resources.** Operators may want to review what
  was recorded — `xvn memory list --tier resource --run <run_id>`.
  Small CLI; not blocking V2D.
