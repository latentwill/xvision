# V2D cortex memory — tier split, leakage protection, and the F+L+T design

> Design discussion captured 2026-05-21 during a `/grill-me` pass on the
> V2D agent-memory wave (`docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md`).
> This note is the source for Phase 1.5 (tier split + scaffolding) and
> Phase 6 (operator-facing docs).

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

## Why a query-time scenario-id filter is the wrong fix

The first instinct is to tag every memory item with the scenario it
came from and exclude same-scenario hits. This breaks down quickly:

- **Scenarios are date ranges over historical data, not abstract
  identities.** Two scenarios can overlap. One can be a strict subset
  of another. Both can replay the same bars under different names.
- **You want cross-scenario generalization.** A pattern learned from
  the 2020 covid crash should inform agents handling the 2024 flash
  crash. Filtering by scenario_id alone either blocks that (over-strict)
  or admits it accidentally (depending on naming).
- **Query-time filters depend on every caller remembering them.**
  Structural separation is more robust than disciplined filtering.

The cortex architecture has a more principled answer.

## The cortex tier split

A canonical cortex memory model distinguishes (at minimum) two tiers.
This note uses operator-friendly names for both:

| Tier | Internal name | Operator-facing name | Content shape | Provenance | Recalled by agent? | Lifetime |
|---|---|---|---|---|---|---|
| Episodic | `tier='observation'` | **Observations** | Concrete observation: "at run R, scenario S, cycle C, saw X-shaped bars, decided Y" | Mandatory: `(run_id, scenario_id, cycle_idx)` | **No** | Persistent for analysis; never reach the LLM during decision-time recall |
| Semantic | `tier='pattern'` | **Patterns** | Abstracted pattern: "X-shaped bars + high vol → fade tends to work; n=14, +37bps mean" | No scenario provenance; carries `training_window_end` instead | **Yes** | Persistent in `agent:<agent_id>` / `global`; recalled at dispatch time |

### On terminology

The cortex literature uses "Resources" (episodic) and "Skills"
(semantic). Both collide badly with existing xvision concepts:

- "Resources" is overloaded with cloud / hardware resources.
- "Skills" already exists as `AgentSlot.skill_ids: Vec<String>` — a
  v1.1 placeholder for the agent tool/prompt-fragment registry.

We use **Observations / Patterns** in operator-facing surfaces (UI
labels, CLI verbs, docs, this note's prose) and reserve `Tier::Observation` /
`Tier::Pattern` for the internal Rust enum so the codebase stays
self-documenting. The cortex tradition is honored in the design
language; the public vocabulary is plain English.

## Three layers of protection — F + L + T

A single mechanism is not enough. V2D Phase 1.5 stacks three:

### F — structural (tier split)

The agent never reads Observations at decision time. Observations are
the input substrate for an offline distillation pass that produces
Patterns. The same recall seam in `execute_slot` becomes a
*Patterns-only* query — the dispatcher cannot accidentally surface
raw observations because the storage layer doesn't expose them on
the read path the LLM sees.

```sql
-- Recall (the only thing execute_slot calls)
SELECT ... FROM memory_items
WHERE namespace = ?
  AND tier = 'pattern'
  AND (training_window_end IS NULL OR training_window_end < ?);
```

If a future caller fat-fingers and tries to read Observations from the
recall path, they get back nothing — the SQL refuses to return them.

### L — rhetorical (case-law framing on recall)

Even with the tier split, a Pattern that landed on a stretch of
specific text is dangerous if the LLM treats it as ground truth.
Case-law jurisdictions solve a similar problem: a precedent isn't
authority by itself, it's an artifact a judge must explain the
applicability of.

`MemoryRecorder::render_recalled_patterns` (renamed from
`render_prior_observations`) wraps each retrieved Pattern in a small
template:

```
<prior_observations>
A prior decision noted: "X-shaped bars at high vol → fade tends to work
(n=14)". Consider whether this situation matches the present cycle.

A prior decision noted: "Stop tightened pre-event". Consider whether
this situation matches the present cycle.
</prior_observations>
```

The LLM is now framing the recall as precedent-to-be-reasoned-about,
not as fact-to-inherit. The cost is one prompt template change in
the recorder; the benefit is belt-and-suspenders protection against
both (a) accidentally over-specific Patterns from a future
autoresearcher bug and (b) adversarial prompt-injection in recorded
text.

### T — temporal (training-window filter)

Even a well-distilled, generalizable Pattern carries leakage risk if
it was trained on data that overlaps the scenario currently being
replayed. Standard backtest discipline says: at simulated time *t*,
the agent may only use knowledge derived from data before *t*.

Patterns carry one extra column:

```sql
ALTER TABLE memory_items ADD COLUMN training_window_end TEXT;
-- ISO 8601 datetime. REQUIRED on Patterns from automated distillation
-- (autoresearcher). NULL on operator-attested manual seeds and on
-- Observations.
```

Recall filters:

```sql
WHERE training_window_end IS NULL
   OR training_window_end < <current scenario start>
```

The dispatcher passes `current_scenario_start` into recall (sourced
from `SlotInput.scenario_id` → `Scenario::start_date` lookup, plumbed
via Phase 3's threading).

**What T preserves:**

- A Pattern distilled from 2020 covid crash + 2022 Luna crash data
  has `training_window_end = 2022`. Safe to recall in any scenario
  starting after 2022, including Aug 2024 flash-crash replays. The
  agent sees the historical-drops pattern and applies it.
- Operator wisdom (manual seed): `training_window_end = NULL`. Recalled
  in every scenario — the operator attested to it; if they want a
  scenario-locked seed, they explicitly set the field.

**What T blocks:**

- A Pattern with `training_window_end >= scenario.start`. Excluded
  from recall during that scenario, even if cosine similarity is
  high. This is the conservative choice — accepting a small
  false-positive rate of dropping marginal patterns in favor of
  hard look-ahead safety.

**What T does NOT cover (deferred on purpose):**

- Cross-symbol leakage. A Pattern trained on BTC Aug 2024 and recalled
  during an ETH Aug 2024 replay is blocked by T (training_end
  overlaps). Conservative; operators can override by manual seed if
  they want cross-symbol patterns.
- Partial overlap windows (some training bars are before scenario
  start, some during). T excludes the whole Pattern. Tighter
  per-bar filtering is V3 / follow-up territory.
- Forward-distilled experiments ("what if we had this future
  knowledge"). T blocks them. Operator manually sets
  `training_window_end = NULL` to override.

## Mapping to V2D code

### Schema (Phase 1.5)

The `xvision-memory` crate's own `memory_items` table (not the engine
migration) gets extended *before* Phase 1.5 ships. Engine migration
027 (V2D's claim) is unaffected — it only adds `agent_slots.memory_mode`.

```sql
ALTER TABLE memory_items ADD COLUMN tier TEXT NOT NULL DEFAULT 'observation';
ALTER TABLE memory_items ADD COLUMN run_id TEXT;
ALTER TABLE memory_items ADD COLUMN scenario_id TEXT;
ALTER TABLE memory_items ADD COLUMN cycle_idx INTEGER;
ALTER TABLE memory_items ADD COLUMN training_window_end TEXT;
CREATE INDEX idx_memory_items_tier_namespace ON memory_items(tier, namespace);
CREATE INDEX idx_memory_items_training_window ON memory_items(training_window_end);
```

`tier ∈ { 'observation', 'pattern' }`.

### Store API (Phase 1.5)

```rust
impl MemoryStore {
    /// Episodic write — auto-recorder calls this.
    /// Asserts provenance is Some on all three fields.
    /// Asserts training_window_end is None.
    pub async fn upsert_observation(&self, item: &MemoryItem) -> Result<()>;

    /// Semantic write — distillation pass / manual seed calls this.
    /// Asserts provenance is None on run/scenario/cycle.
    /// training_window_end may be Some(date) or None (operator wisdom).
    pub async fn upsert_pattern(&self, item: &MemoryItem) -> Result<()>;

    /// Autoresearcher Pattern retirement.
    pub async fn demote_pattern(&self, id: &str) -> Result<u64>;

    /// Dispatcher recall — Patterns only, time-window filtered.
    /// `current_scenario_start = None` skips the temporal filter
    ///   (live/paper mode; no replay risk).
    pub async fn query(
        &self,
        namespace: &str,
        query_embedding: &[f32],
        k: usize,
        current_scenario_start: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<Vec<MemoryMatch>>;
}
```

The legacy `upsert` method is removed — callers must pick a tier.

### Recorder (Phase 3 update — small)

`MemoryRecorder::record` calls `upsert_observation`. Provenance fields
are required and pulled from `SlotInput.run_id` / `scenario_id` /
`cycle_idx` (already plumbed).

`MemoryRecorder::recall` calls `query` with `current_scenario_start`
sourced from the scenario lookup.

`MemoryRecorder::render_recalled_patterns` (renamed from
`render_prior_observations`) wraps each result in the L template.

### Frontend (Phase 4 update — small)

The Memory panel in eval-review and the AgentForm selector use the
**Observations / Patterns** vocabulary. The internal `MemoryMode`
enum (off/global/agent_scoped) is unchanged — the tier split is below
the API the UI consumes.

## V1 behavior

- Auto-recorder writes Observations as runs proceed.
- Recall returns Patterns. **Patterns tier is empty in v1.**
- Every recall returns zero hits. The Memory panel renders empty
  states. The agent sees nothing in `<prior_observations>`.
- No leakage is possible because the read path the LLM sees is
  Patterns-only, and Patterns has no rows.

Patterns populate when:

- **v1.1**: a manual seeding CLI ships (`xvn memory add-pattern
  --namespace global "<text>" [--training-end <date>]`).
- **V3**: the autoresearcher's distillation pass produces and
  promotes Patterns from accumulated Observations.

## Autoresearcher (V3) interplay

The cortex tier split surfaces a meaningful V3 design constraint:
**the autoresearcher is the distillation pass**. Specifically:

- **Read** Observations across many runs to spot patterns
  (mutator's "what happened the last 50 times this shape appeared?"
  question).
- **Propose** candidate Patterns with sample counts and outcome stats.
- **Judge** with held-out scenarios — does the Pattern predict on
  data the autoresearcher hasn't seen?
- **Promote** validated Patterns into the Patterns tier with
  `training_window_end` set to the latest bar timestamp across all
  contributing Observations.
- **Edit / supersede** Patterns that fail recent backtests via
  `demote_pattern`.

The auto-recorder has INSERT access to Observations only. The
autoresearcher has full access (INSERT/DELETE on both tiers, plus
the only path that legitimately writes Patterns).

**V3 board entry (`team/board-v2.md` item 11a):**

> Autoresearcher's mutator/judge/promote loop *is* the cortex
> distillation pass. The autoresearcher gains write access to the
> Patterns tier (via `MemoryStore::upsert_pattern` /
> `demote_pattern`) and read access to Observations. Editing
> semantics (create / supersede / retire) need to land before the
> first nightly autoresearcher run that targets a Pattern-consuming
> agent — otherwise the loop is purely evaluative and nothing
> accumulates.

## Operator mental model (for the docs)

Boiled down for the V2D user-facing docs (Phase 6):

> **Memory in xvision is two-layered.** Your agents *write*
> **Observations** as they run — what they saw, what they decided,
> when. Observations stay in the engine's memory store for analysis.
> Your agents *read* **Patterns** — distilled insights that the
> autoresearcher (or you, manually) has validated as predictive.
> Reading is one-way: agents never see raw Observations during a
> decision, only the Patterns those Observations have been distilled
> into. Patterns also carry the date their training data ended, so
> the agent never sees a Pattern learned from data inside the
> historical window it's currently replaying. That's why running a
> backtest twice doesn't give the agent foreknowledge of the outcome.

## What this changes in the Phase plan

V2D's plan grows two phases:

- **Phase 1.5** — extend the `xvision-memory` crate with `tier` +
  provenance + `training_window_end` columns, tier-discriminated
  read/write API, time-window filter, case-law wrapper.
- **Phase 6** — operator-facing docs (this note's "Operator mental
  model" section becomes the seed) at `docs/v2d-memory-overview.md`
  with a hook in `MANUAL.md` and a Memory subsection in the V2A
  overview.

Phase 3's recorder gets a small amendment to call
`upsert_observation` and `render_recalled_patterns`. Phase 4's UI
adopts the Observations / Patterns vocabulary. The merge stays
blocked until Phases 1.5 and 6 land.

## Acceptance criteria additions

- Backtest run R writes only `tier=observation` rows.
- Backtest replay R' recalls zero rows for a non-Off slot
  (Patterns is empty in v1).
- `upsert_observation` rejects a write where provenance is missing
  or `training_window_end` is set.
- `upsert_pattern` rejects a write where any provenance field is
  set (Patterns are abstractions, not attributions).
- Recall queries `WHERE tier='pattern' AND (training_window_end IS
  NULL OR training_window_end < ?)`; cannot accidentally surface
  Observations or temporally-leaky Patterns.
- The recall path wraps each result in the case-law template before
  prepending to `system_prompt`.
- UI labels use "Observations" and "Patterns"; the cortex literature
  terms appear only in code identifiers and this note.

## Open questions deferred to follow-ups

- **Manual Pattern seeding UX.** `xvn memory add-pattern` CLI
  design is a small follow-up contract; not blocking V2D.
- **Distillation pass spec.** Belongs in V3 autoresearcher intake;
  V2D ships the API surface, not the implementation.
- **Pattern editing — supersede vs replace.** When the
  autoresearcher invalidates an old Pattern, does it delete or mark
  deprecated? Recommend: soft-delete via a `deprecated_at`
  timestamp so longitudinal autoresearcher logs can compare
  before/after.
- **Operator audit of Observations.** Operators may want to review
  what was recorded — `xvn memory list --tier observation --run
  <run_id>`. Small CLI; not blocking V2D.
- **Cross-symbol Pattern policy.** Pattern learned on BTC; recalled
  during ETH backtest. Currently blocked by T if windows overlap;
  conservative. Operator override path needed in v1.1 if cross-symbol
  patterns prove useful.
