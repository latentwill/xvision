# AutoOptimizer terminology lock — 2026-05-27

> **Amendment 2026-06-01 — autoresearcher → optimizer rename.** The
> subsystem this doc calls the "autoresearcher" was renamed. Its
> **developer-surface codename** is now `autooptimizer` / `AutoOptimizer`
> (Rust module `autooptimizer/`, types `AutoOptimizer*`, tables
> `autooptimizer_*`, routes `/api/autooptimizer/*`, frontend
> `features/autooptimizer/`). Its **operator-surface name** is now
> **"Optimizer"** (dashboard nav + titles, CLI verb `xvn optimizer`,
> SSE display labels, MANUAL.md, wiki). Wherever this doc's rows show a
> developer name of `autoresearch*`/`Autoresearch*`, read `autooptimizer*`/
> `AutoOptimizer*`; wherever an operator name was "Autoresearcher", read
> "Optimizer". The codename is deliberately `autooptimizer` (not bare
> `optimizer`) to stay distinct from the unrelated DSPy prompt-optimizer
> (`xvn optimize`, `optimization_*`, `xvision-dspy::Optimizer`), which is
> NOT renamed.

> Status: locked 2026-05-27; amended same day (memory verb structure —
> see "Amendments" section at the bottom)
> Operator approval for both the initial lock and the amendment
> recorded in the companion audit doc.
> Companion: `docs/superpowers/notes/2026-05-27-autooptimizer-plain-language-audit.md`
> Affects: AR-1 / AR-2 / AR-3 implementation plans, the karpathy
> autooptimizer spec, the cortex-memory + DSPy flywheels spec, the
> autooptimizer-ops and flywheel-ops skills, MANUAL.md, dashboard wiki.
> Cross-reference: extends the project-wide terminology table at
> `/CLAUDE.md` for the autooptimizer subsurface.

## The two-surface principle

Every named concept in the autooptimizer has two valid names:

- **Developer-surface name.** Used in Rust types, SQLite columns,
  spec documents, API type fields, internal module names, and
  CI-enforced public API. Precise and technical. Never weakened to
  protect operator readability.
- **Operator-surface name.** Used in CLI flags and help text, UI
  labels, SSE event display names, error messages, MANUAL.md, the
  dashboard wiki, and every doc operators read. Plain English. Tied
  to operator mental models, not to the underlying mechanism.

Diverging from these names should require a written rationale.
Adding a new operator-facing concept requires a new row in the
mapping table below.

## Mapping conventions

| Symbol | Meaning |
|---|---|
| `dev` | Developer-surface name (code, spec, schema) |
| `ops` | Operator-surface name (CLI, UI, ops docs, SSE display labels) |
| — | No rename applies on this surface |

When both columns hold the same value the term is plain enough to
serve both audiences and was deliberately kept.

## Section 1 — Evolution & lineage

| dev | ops |
|---|---|
| Lineage | Lineage |
| Mutation, `MutationDiff` | Experiment |
| Mutator | Experiment writer |
| `LineageStatus::Active` | Active |
| `LineageStatus::Ghost` | Rejected |
| `LineageStatus::Quarantined` | Suspect |
| Mutator-skill ladder | Proposer scoreboard |
| Parent / Child bundle | Parent / Child |
| Cycle | Cycle |
| Evening cycle | Evening run |

## Section 2 — Cryptographic provenance

> **[REMOVED 2026-06-01]** The provenance layer described in this section
> has been deleted. See
> `docs/superpowers/specs/2026-06-01-remove-autooptimizer-crypto-provenance-design.md`.
> The operator-surface rename pairs in the remaining sections remain in force.

| dev | ops |
|---|---|
| `ContentHash`, `bundle_hash` (64-hex) | Strategy fingerprint (short ID + copy-on-click) |

## Section 3 — Gate / decision

| dev | ops |
|---|---|
| Gate, `xvn autooptimizer gate` | Gate (CLI verb stays; noun "Gate" stays in UI form labels) |
| `GateDecision::Approved` / `::Rejected` | Kept / Dropped |
| `gate_verdict` (passed/failed) | Decision (Kept/Dropped) |
| epsilon, ε, `--gate-epsilon`, `--min-delta` | Minimum improvement, `--min-improvement` |
| `delta_day`, `--parent-day-score`, `--child-day-score` | Today's score (baseline/candidate), `--baseline-today-score`, `--candidate-today-score` |
| `delta_holdout`, `--parent-holdout-score`, `--child-holdout-score` | Untouched-period score, `--baseline-untouched-score`, `--candidate-untouched-score` |
| Holdout window | Untouched test period |
| Holdout split (e.g. "70/15/15") | Training / Validation / Untouched test split |
| Pre-commitment | Session ground rules (locked at start) |
| `--baseline-score`, `--candidate-score` | Same |

## Section 4 — Loop control & sanity-check evaluations

| dev | ops |
|---|---|
| Null-result canary | Honesty check |
| `autooptimizer_canary_runs` table, `canary_outcome` SSE wire name | (schema/wire stay); display label "Honesty check result" |
| Inversion-pair eval | Reverse-mutation check |
| Diversity-decay rate | Variety score |
| Counterfactual-chain Merkle root | Lineage proof |
| Embedding-divergence | — (never user-facing) |

## Section 5 — Memory layer

| dev | ops |
|---|---|
| Observation | Observation |
| Pattern | Pattern |
| Tier, `--tier` | Kind, `--kind` (CLI surface only; struct/enum stays `Tier`) |
| `promotion_state`, `--promotion-state` | Status, `--status` |
| `Staged` / `Active` / `Forgotten` | Same |
| Demote, `xvn memory demote` | Retire, `xvn memory retire` |
| Promote (Observation cohort → Pattern), `xvn memory promote --ids ... --text ...` | Distill, `xvn memory distill --ids ... --text ...` |
| `xvn memory activate <id>` (single-Pattern form, exists today) | Unchanged — preserves the single-Pattern activation verb separately from the cohort distillation verb |
| `training_window_end` | Knowledge cutoff (UI label "Training data ends" already plain and stays) |
| F+L+T | — (spec only) |
| Case-law framing | — (spec only) |
| Namespace, `--namespace` | Namespace (no rename) |
| Memory mode `Off` / `Global` / `AgentScoped` | "Off" / "Shared across all agents" / "This agent only" |

## Section 6 — Session and cycle machinery

| dev | ops |
|---|---|
| Session | Session |
| Cycle | Cycle |
| Run | Run |
| `session_id` ULID (26 chars) | "Session #<short>" — first 6 chars of ULID, copy-on-click for full |
| `cycle_id` ULID | "Cycle #<short>" — same pattern |
| `run_id` ULID | "Run #<short>" — same pattern |
| `bundle_hash` (64-hex) | "Strategy <short>" — first 8 hex chars, copy-on-click for full |

## Section 7 — Flywheel / lineage view

| dev | ops |
|---|---|
| Flywheel | Flywheel |
| Velocity | Velocity |
| Flywheel status | Status |
| Lineage depth | Generations deep |
| Optimization, `optimization_id` | Training run |
| `target_agent_id` | Parent agent |
| `child_agent_id` | Child agent |
| Demos, demo source, demo patterns | Examples, example source, example patterns |
| Demo source `Frozen` / `Fresh` | Use saved examples / Capture new examples |
| Priors, prior patterns, "Use recalled Pattern priors" | Background patterns; checkbox "Include patterns I've already learned" |
| Train / Dev / Holdout (split, demos, counts, hashes) | Training / Validation / Untouched test |
| Mint Child (UI button) | Train new version |

## Section 8 — Optimization parameters

| dev | ops |
|---|---|
| Embedding, `--embedding-json` | (CLI flag name stays — power-user only); UI label "Embedding JSON" becomes "Test embedding vector (advanced)" behind a disclosure |
| Embedder, `embedder_id` | Embedding provider |

## Section 9 — Attestation

| dev | ops |
|---|---|
| Operator attestation | Operator sign-off |
| `--attest-null-window` | `--confirm-no-cutoff` |
| `--operator-initials` | Same |
| `training_window_end IS NULL` | No training cutoff |

## Section 10 — CLI verb structure

| dev | ops |
|---|---|
| `xvn autooptimizer run` | Same |
| `xvn autooptimizer gate` | Same (verb stays per resolved decision) |
| `xvn autooptimizer promote` | `xvn autooptimizer activate` |
| `xvn autooptimizer demote` | `xvn autooptimizer retire` |
| `xvn autooptimizer inspect` | Same |
| `xvn autooptimizer ls` | Same |
| `xvn memory ls` | Same |
| `xvn memory promote --ids ... --text ...` (Observation cohort → Pattern) | `xvn memory distill --ids ... --text ...` |
| `xvn memory activate <id>` (single-Pattern form) | Same (preserved unchanged; the existing single-Pattern activation verb stays separate from the distillation verb) |
| `xvn memory demote` | `xvn memory retire` |
| `xvn memory forget` / `undo-forget` | Same |
| `xvn memory add-pattern` | Same |
| `xvn flywheel status` / `velocity` / `lineage` | Same |

## Section 11 — SSE event display labels

Wire event names stay (they're a wire-protocol contract). The
dashboard renders these display labels:

| Wire name (dev) | Display label (ops) |
|---|---|
| `cycle_started` | Evening run started |
| `mutation_proposed` | Experiment proposed |
| `mutation_evaluating` | Testing experiment |
| `mutation_committed` | Experiment kept |
| `mutation_rejected` | Experiment dropped |
| `mutation_quarantined` | Experiment flagged for review |
| `lineage_forked` | New branch added |
| `judge_wrote_finding` | Reviewer finished notes |
| `canary_outcome` | Honesty check result |
| `diversity_updated` | Variety score updated |
| `ladder_snapshot` | Proposer scoreboard updated |
| `cycle_sealed` | Evening summary signed |
| `cycle_failed` | Evening run failed |

## Implementation deliverables (sequenced)

The lock above is the contract. The following patches realize it on
each surface. Each can ship independently; recommended sequence:

1. **Spec amendment** to
   `docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md`:
   add a §0.1 "Operator vocabulary" that points at this lock doc, and
   add a footnote at every first mention of a developer-surface term
   in §3–§9 saying "operator-facing as `<ops name>`."
2. **CLI patch** in `crates/xvision-cli/src/commands/autooptimizer.rs`,
   `flywheel.rs`, and `memory.rs`: rename verbs/flags/help text per
   sections 3/5/9/10. Keep old verb names and flag names as hidden
   aliases for one release. Update
   `crates/xvision-cli/tests/cli_surface_snapshot.json`.
3. **Frontend patch** in
   `frontend/web/src/features/memory/MemorySurface.tsx`,
   `MemoryPage.tsx`, and the agents-flywheel route: rename TSX label
   strings, button labels, status badges, form field labels per
   sections 1/3/5/7/8. No API changes (developer-surface field names
   stay).
4. **SSE display-label registry** in
   `crates/xvision-dashboard/src/sse.rs`: add a wire→display map per
   section 11. Update `crates/xvision-dashboard/static/js/bus.js` to
   read the display label from the event metadata.
5. **Short-display + copy-on-click** for ULIDs and bundle hashes
   (section 6): one shared React component used everywhere the UI
   currently renders a raw ULID or 64-hex string.
6. **Skill files**: update
   `.claude/skills/xvision/autooptimizer-ops/SKILL.md` and
   `flywheel-ops/SKILL.md` to use renamed CLI verbs and to drop
   developer-only vocabulary from the standard flow examples.
7. **MANUAL.md and dashboard wiki sweep**: replace developer-surface
   terms with operator-surface terms throughout.

The CLI rename ships with deprecation aliases. Frontend label
renames and SSE display-label patches are cosmetic and can land in
one PR together. The skill files and MANUAL.md sweep should land
together so external-facing documentation stays internally
consistent.

## Out of scope

- Rust type renames (`Mutation`, `Mutator`, `CycleSeal`, etc. all
  stay).
- SQLite schema renames (table and column names stay).
- API response field renames (frontend reads developer-surface field
  names and maps them to display labels in TSX).
- Spec-document content (the spec uses developer-surface vocabulary;
  only adds footnotes pointing at operator-surface names).
- CI-enforced public API (the dependency rule
  `autooptimizer/` cannot import `marketplace/` stays unchanged).
- Marketplace plugin terminology (already passed its own rename).

## Optimizer UI Overhaul wave (2026-06-07)

New terms introduced by the Optimizer UI Overhaul P1 feature wave. Codename
remains `autooptimizer`; nothing here touches the DSPy `optimize`/`Optimizer*`
namespace.

| dev | ops |
|---|---|
| `OptimizerSession` / `autooptimizer_session_state` | Run |
| `state: paused` / pause flag | Paused / Pause / Resume |
| `mode: once` | Single experiment |
| `mode: n_experiments` | N experiments |
| `mode: until_budget` | Until budget |
| `PhaseStarted` / `PhaseFinished` events | (phase labels, see rows below) |
| `writer_proposing` (phase label) | Writer drafting experiment |
| `eval_day_window` (phase label) | Backtesting today's window |
| `eval_untouched_window` (phase label) | Backtesting untouched period |
| `reverse_check` (phase label) | Reverse-mutation check |
| `gate_evaluating` (phase label) | Applying decision gate |
| `reviewer_running` (phase label) | Reviewer writing notes |
| `autooptimizer_schedules` table | Scheduled run |
| `flywheel_compiled` SSE event | Findings compiled into prompt pattern |
| `dspy_flywheel` surface (strip) | Prompt tuning |

## Amendments

### 2026-05-27 — Memory verb structure

Initial lock collapsed `xvn memory promote` (Observation-cohort
distillation) into `xvn memory activate` to consolidate verbs. The
CLI handoff review caught that the existing
`xvn memory activate <id>` form has different semantics (flips a
single staged Pattern to active by id), and folding the
cohort-distillation flags into the same verb would muddle the call
sites.

Amended: `xvn memory promote` is renamed to `xvn memory distill` as
its own verb. `xvn memory activate <id>` stays unchanged and keeps
its single-Pattern semantics. Both verbs ship; deprecation alias
`xvn memory promote` forwards to `xvn memory distill` for one
release.

Affected sections of this lock: §5 (Memory layer table), §10 (CLI
verb structure table). Operator approval recorded 2026-05-27.

### Amendment 2026-06-11 — optimizer CLI consolidation

The Optimizer subsystem's operator CLI was consolidated to a single home.

**Changes:**

- The cycle CLI verb changed from **`xvn optimizer`** to **`xvn optimize`**.
  Running `xvn optimize` with **no subcommand** runs the full cycle (aliases:
  `xvn optimize run` / `xvn optimize run-cycle`). Subcommands: `ls` (cycle
  history), `show <cycle_id>` (cycle detail), `lineage ls|show`, `unlock`
  (clear a wedged cycle lock), `mutate-once`, `demo`.
- The top-level **`xvn optimizer` verb was removed.** It no longer exists.
- The standalone **DSPy prompt-optimizer CLI verbs were removed** and folded
  into the cycle: `run --agent/--slot/--capability/--corpus/--optimizer/
  --metric`, `inspect --run`, `export-demos`, `import-demos`,
  `accept-as-child-agent`, `revert-accepted`, `explain-missing-data`,
  `memory-demos`, `memory-demos-gate`, and the old `distill/gate/activate/
  retire/promote/demote`. The DSPy flywheel now runs **inside the cycle
  automatically** and emits the `flywheel_compiled` / `CycleProgressEvent::
  FlywheelCompiled` event (already locked in the §"Optimizer UI Overhaul wave
  (2026-06-07)" table as "Findings compiled into prompt pattern").

**What did NOT change:** the developer-surface codename stays `autooptimizer` /
`AutoOptimizer` (module, types, `autooptimizer_*` tables, `/api/autooptimizer/*`
routes, frontend `features/autooptimizer/`). The DSPy **engine** `optimization/`
module and the `Optimizer*` types remain intact and are never renamed — only the
DSPy *CLI verbs* were removed.

**Rationale:** operator decision — one operator surface for the cycle, to reduce
agent confusion and wasted tokens spent disambiguating `optimizer` vs `optimize`
and the now-redundant standalone DSPy verbs.

**Supersession:** this supersedes the prior **"CLI verb `xvn optimizer`"** row
in the 2026-06-01 amendment (top of this doc) and §10 ("CLI verb structure").
Read every operator-surface reference to `xvn optimizer` as `xvn optimize`. The
`xvn autooptimizer …` developer-surface verb rows in §10 are unaffected by this
amendment (they concern the developer-surface verb naming, not the consolidated
operator surface).

Affected sections of this lock: the 2026-06-01 amendment header (top), §10 (CLI
verb structure). Cross-reference: the project `/CLAUDE.md` "Operator-facing
names (autooptimizer subsurface)" section carries the matching amendment.
Operator approval recorded 2026-06-11.

---

## Amendment 2026-06-13 — trader-failure resilience (errored bucket + circuit breaker)

New operator-facing concepts on the optimizer surface (per the "any new
operator-facing concept requires a row" rule). Developer-surface names stay
precise; operator-surface names are plain-language.

| dev | ops |
|---|---|
| `errored` session outcome / `errored_count` column | "Errored" — a cycle whose candidate eval crashed (distinct from "Dropped" = gate-rejected, and "No experiment produced" = nothing proposed) |
| `CycleProgressEvent::CandidateError`, wire `candidate_error` | "Candidate eval failed" (dashboard SSE label) |
| `ConsecutiveErrors` circuit breaker | consecutive-error halt (honest fail-fast) |
| `--max-consecutive-errors <N>` (CLI flag) | same (plain flag; default 3) |

**Rationale:** a single candidate's eval failure (e.g. the trader model returns
an invalid action or truncates mid-JSON) is recorded as `errored` and the
session continues; N consecutive candidate errors halt loudly (systemic
misconfiguration). Surfacing "Errored" distinctly from "Dropped" preserves the
honesty-check philosophy — operators can tell "evals are crashing" from
"experiment fizzled". See
`docs/superpowers/specs/2026-06-13-optimizer-trader-failure-resilience-design.md`.
