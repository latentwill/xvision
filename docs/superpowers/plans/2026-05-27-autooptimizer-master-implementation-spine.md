# AutoOptimizer master implementation plan — the spine

> Date: 2026-05-27
> Status: live; this is the canonical plan-of-plans for the
> autooptimizer build
> Owner: project-level (conductor reads this to sequence waves)
> Read order if new to autooptimizer: this doc → terminology lock →
> the layer you're working on

## What this document is

The autooptimizer is xvision's self-improvement loop: an overnight
process that proposes tweaks to trading strategies, paper-tests them
against ground truth, gates on numeric improvement, captures
qualitative findings blind to the metrics, and grows a content-
addressed lineage of survivors over time. It's the largest single
piece of xvision and spans seven layers across the engine, the
memory substrate, the dashboard, the CLI, the skills, the marketplace
plugin, and a forthcoming candidate-generation upgrade.

Because the work spans seven layers and a dozen+ sub-plans, no one
plan tells the whole story. This document is the spine: it names
every sub-plan by path, identifies the gaps that aren't planned yet,
sequences the rollout into phases with explicit dependencies, and
defines the acceptance gate at each phase boundary. The conductor
reads this to decide what wave runs next; the operator reads this to
see where everything stands; a new contributor reads this to know
where their work fits.

When this doc and a sub-plan disagree, the sub-plan wins for its own
scope and the spine gets amended.

## Part 1 — The architecture in seven layers

The autooptimizer is a stack. Each layer has a clear contract with
the layer below it and a clear consumer above. Layers can ship
independently as long as the contract holds.

```
┌──────────────────────────────────────────────────────────────────┐
│ Layer 7 — Marketplace plugin (downstream consumer; own spine)    │
│           Reads CycleSeal artifacts → publishes lineage on-chain │
├──────────────────────────────────────────────────────────────────┤
│ Layer 6 — Terminology & UX surface (just shipped)                │
│           Operator-facing names + design language across all     │
│           CLI / UI / SSE / docs                                  │
├──────────────────────────────────────────────────────────────────┤
│ Layer 5 — Dashboard surfaces (live cycle, genealogy, ladder)     │
│           AR-3 plan                                              │
├──────────────────────────────────────────────────────────────────┤
│ Layer 4 — Cycle orchestrator + judge + sanity-check evals        │
│           AR-2 plan                                              │
├──────────────────────────────────────────────────────────────────┤
│ Layer 3 — Mutator + lineage store + numeric gate + CycleSeal     │
│           AR-1 plan (partially shipped)                          │
├──────────────────────────────────────────────────────────────────┤
│ Layer 2 — Cortex memory + DSPy/DSRs flywheel                     │
│           Observation/Pattern substrate, F+L+T leakage protection│
├──────────────────────────────────────────────────────────────────┤
│ Layer 1 — Eval engine (paper-test executor, scenario fixtures,   │
│           findings extractor, metrics)                           │
└──────────────────────────────────────────────────────────────────┘
```

Each layer's responsibilities:

**Layer 1 — Eval engine.** Deterministic paper-test executor over a
pinned scenario fixture. Produces metrics summaries (Sharpe,
drawdown, profit factor, regime-tagged trade tape). Consumed by AR-1
for both the day window and the untouched test period. This is the
ground-truth signal the whole loop depends on.

**Layer 2 — Cortex memory + DSPy/DSRs flywheel.** Observation and
Pattern tiers with F+L+T leakage protection. The autooptimizer
distills Observations into Patterns; the DSRs offline optimizer
compiles instruction + demos from Pattern cohorts; the loop closes
when the next decision cycle recalls a Pattern. Cortex provides the
storage substrate.

**Layer 3 — Mutator + lineage + gate + seal (AR-1).** Proposes one
mutation, paper-tests it on day + untouched windows, runs the
numeric gate, commits to content-addressed lineage, emits a
CycleSeal. The atomic unit of the loop. Partially shipped today.

**Layer 4 — Cycle orchestrator + judge + sanity-checks (AR-2).**
Runs N mutations per evening across M parents, injects an honesty
check, runs the reverse-mutation noise filter, computes diversity
metrics, dispatches the blind LLM judge for Findings on kept
experiments, manages cycle-level state (parent policy, token caps,
loosening schedule).

**Layer 5 — Dashboard surfaces (AR-3).** Five canonical views: live
evening run viewer, genealogy tree, experiment diff inspector,
proposer scoreboard, ladder with provenance. SSE-driven real-time
updates from the cycle orchestrator.

**Layer 6 — Terminology & UX surface (just shipped; rollout
in flight).** Operator-facing vocabulary lock with full design
reference. Sweeps CLI flags, UI labels, SSE display labels, skills,
MANUAL.md, and the dashboard wiki.

**Layer 7 — Marketplace plugin (downstream consumer).** Reads
CycleSeals, anchors Merkle roots on-chain, mints NFT lineage
manifests. Has its own spine (`docs/superpowers/plans/2026-05-26-marketplace-program-strategy.md`).
Out of scope for this spine except as the contract consumer of
CycleSeal.

## Part 2 — Current status snapshot

State of each layer as of 2026-05-27.

| Layer | Status | Key artifacts |
|---|---|---|
| L1 — Eval engine | Shipped (foundation); ongoing accuracy work | Spec + plan landed; eval-trace-surface and execution-accuracy waves in flight per `docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md` |
| L2 — Cortex memory + DSRs flywheel | V2D shipped (in-house adapter); cortex-core direct dep is the chosen Path A integration | `crates/xvision-memory/` is live; `crates/xvision-engine/src/agent/memory_recorder.rs` integrated; `xvn memory ls/promote/...` CLI shipped; migrations 0001-0005 applied |
| L3 — AR-1 mutator+lineage+gate+seal | Partially shipped (offline distill path), key cryptographic pieces planned but not yet built | `crates/xvision-engine/src/api/autooptimizer.rs` exists (deterministic distill); `crates/xvision-cli/src/commands/autooptimizer.rs` shipped; migrations 0003-0005 applied; full AR-1 plan at `docs/superpowers/plans/2026-05-09-autooptimizer-1-mutator-lineage-gate-seal.md` (3797 lines, ~Phase A complete) |
| L4 — AR-2 cycle+judge+evals | Planned, not yet implemented | Plan at `docs/superpowers/plans/2026-05-09-autooptimizer-2-cycle-judge-evals.md` |
| L5 — AR-3 dashboard | Planned, not yet implemented as a unified surface (some pieces in `frontend/web/src/features/memory/` ship adjacent value) | Plan at `docs/superpowers/plans/2026-05-09-autooptimizer-3-dashboard.md` |
| L6 — Terminology & UX | Lock + design reference shipped; 5 implementation handoffs ready, rollout in flight | Lock at `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`; designer reference at `docs/design/2026-05-27-autooptimizer-designer-reference.md`; wave intake at `team/intake/2026-05-27-autooptimizer-terminology-rollout.md` |
| L7 — Marketplace plugin | Has its own spine and active waves | Spec at `docs/superpowers/specs/2026-05-09-marketplace-plugin-design.md`; program strategy at `docs/superpowers/plans/2026-05-26-marketplace-program-strategy.md` |

## Part 3 — The phased spine sequence

Six phases. Phases overlap where dependencies permit. Each phase
ends with a hard acceptance gate; the next phase doesn't start until
the gate passes.

### Phase 0 — Foundations (shipped)

- L1 eval engine paper-test path
- L2 memory layer V2D (Observation/Pattern, F+L+T, recorder, recall)
- L3 partial: the offline deterministic distill path
  (`xvn autooptimizer run/gate/promote/inspect/ls`)

**Gate**: paper-test runs end-to-end and produces identical metrics
on two consecutive runs; memory recorder writes Observations
post-dispatch; offline distill produces a staged Pattern from a
cohort. Status: **passed.**

### Phase 1 — Cryptographic substrate (AR-1 completion)

Complete the AR-1 plan's remaining tasks: content-hash + blob store
+ session commitment + LLM mutator + bundle program-view + full
numeric gate + CycleSeal artifact + Merkle root computation.

Sub-plan: `docs/superpowers/plans/2026-05-09-autooptimizer-1-mutator-lineage-gate-seal.md`
(3797 lines, decomposed into ~14 tasks across phases A/B/C).

**Allowed paths**: `crates/xvision-engine/src/autooptimizer/**` (new),
`crates/xvision-engine/src/bundle/program_view.rs` (new),
`crates/xvision-engine/migrations/003_autooptimizer.sql` (new),
`crates/xvision-cli/src/commands/autooptimizer.rs` (extend),
`config/autooptimizer.toml.example` (new).

**Forbidden**: `crates/xvision-engine/src/marketplace/**` (lives
behind feature flag in L7); any frontend; any spec doc.

**Dependencies**: L1 eval engine complete (✓).

**Gate**: `xvn autooptimizer session-init` writes operator-signed
pre-commitment; `xvn autooptimizer mutate-once <parent>` proposes one
LLM mutation, paper-tests it on day + untouched windows, runs the
gate, commits Active or Ghost to lineage, emits a CycleSeal with
Merkle root + operator signature. Two consecutive runs against the
same seed produce identical Merkle roots.

### Phase 2 — Cycle orchestration + sanity checks (AR-2)

Wrap the per-mutation atomic unit from Phase 1 in a nightly
orchestrator that runs N mutations across M parents, injects the
honesty check, runs the reverse-mutation noise filter, computes
diversity metrics, dispatches the blind LLM judge for Findings on
kept experiments, and manages cycle-level state.

Sub-plan: `docs/superpowers/plans/2026-05-09-autooptimizer-2-cycle-judge-evals.md`.

**Allowed paths**: `crates/xvision-engine/src/autooptimizer/cycle.rs`,
`/canary.rs`, `/inversion.rs`, `/diversity.rs`, `/judge.rs`,
`/parent_policy.rs`, `/progress.rs` (SSE event emitters);
`crates/xvision-cli/src/commands/autooptimizer.rs` (add
`evening-cycle` verb); `crates/xvision-engine/migrations/004_autooptimizer_evals.sql`.

**Dependencies**: Phase 1 gate passed.

**Gate**: `xvn autooptimizer evening-cycle` runs end-to-end:
selects N parents, injects 1 honesty check, proposes M mutations
each, gates, runs the reverse-mutation check on keepers, dispatches
the judge blind to metrics, computes diversity, writes the
CycleSeal. SSE events emitted on the orchestrator's `ProgressChannel`.
Three consecutive runs against the same seed produce identical
lineage shapes (modulo paper-test wall-clock noise inside CI bounds).

### Phase 3 — Dashboard surfaces (AR-3)

Land the five canonical views. Decoupled from the orchestrator via
the SSE channel + SQLite reads, so the dashboard can render against
saved cycle outputs even when the orchestrator isn't running.

Sub-plan: `docs/superpowers/plans/2026-05-09-autooptimizer-3-dashboard.md`.

**Allowed paths**: new crate `crates/xvision-dashboard/` (axum +
vanilla SPA); `crates/xvision-cli/src/commands/dashboard.rs` (thin
CLI wrapper).

**Dependencies**: Phase 2 gate passed.

**Gate**: `xvn dashboard serve` boots at `http://localhost:7777`;
running an evening cycle in parallel streams events into the live
viewer in real time; the genealogy tree renders past cycles from
SQLite; the diff inspector opens on click; the proposer scoreboard
and ladder-with-provenance render against the most recent ladder
snapshot.

### Phase 4 — Candidate-generation upgrade (autoreason tournament)

**Currently unplanned. Needs a spec.** Per the earlier evaluation
(`docs/superpowers/notes/2026-05-27-autooptimizer-plain-language-audit.md`
context: the three-repo comparison), the autoreason tournament
addresses three known failure modes in the current single-shot
mutator: prompt bias, scope creep, and lack of restraint.

Proposed scope: replace the single-shot mutator inside Layer 3 with a
three-candidate tournament — incumbent (no change), adversarial
revision, synthesis — judged blind via Borda count. "Do nothing"
becomes a first-class option. The numeric gate from Layer 3 still
runs after the tournament picks a winner; tournament is purely a
candidate-generation strategy upgrade.

**Sub-plan**: needs to be written. Proposed path:
`docs/superpowers/specs/2026-05-30-autoreason-tournament-integration-design.md`
(spec) and a follow-on implementation plan once the spec lands.

**Dependencies**: Phase 1 (the tournament wraps the mutator that
Phase 1 ships). Can spec and implement in parallel with Phases 2/3
as long as the AR-1 mutator API has a stable shape; lands as an
amendment to AR-1.

**Gate**: tournament produces three candidates per round; numeric
gate is run against the tournament winner; "incumbent" wins are
allowed and recorded as null-change accepted; against a synthetic
test cohort the tournament beats single-shot on either the Sharpe
gain rate or the rejection-of-noise rate (acceptance criterion to
be defined in the spec).

### Phase 5 — Terminology & UX rollout (in flight)

Five tracks shipping per the existing wave intake.

Wave intake: `team/intake/2026-05-27-autooptimizer-terminology-rollout.md`.

Tracks:
1. Spec amendment — `docs/design/2026-05-27-autooptimizer-spec-amendment-handoff.md`
2. Frontend label rename — `docs/design/2026-05-27-autooptimizer-frontend-rename-handoff.md`
3. SSE display-label registry — `docs/design/2026-05-27-autooptimizer-sse-registry-handoff.md`
4. CLI rename — `docs/design/2026-05-27-autooptimizer-cli-rename-handoff.md`
5. Skills + docs sweep — `docs/design/2026-05-27-autooptimizer-skills-docs-sweep-handoff.md`

Designer reference (the design language doc):
`docs/design/2026-05-27-autooptimizer-designer-reference.md`.

**Dependencies**: terminology lock landed (✓); Track 5 is blocked by
Track 4; the rest are independent. Can run alongside Phases 1–4.

**Gate**: every operator-visible surface (CLI help, UI labels, SSE
display labels, MANUAL.md, the wiki, the autooptimizer-ops skill)
passes the banned-words check from the designer reference Part 12;
fresh-operator smoke test passes (someone who hasn't used xvision
follows the standard flow without encountering banned terms).

### Phase 6 — Agent-side skill discipline (Murat skill-pack methodology)

**Currently unplanned. Needs a spec.** Per the earlier evaluation,
the Agent-Skills-for-Context-Engineering pack offers measurement
discipline that's directly applicable to xvision's own skills under
`.claude/skills/xvision/` and to per-slot prompts inside `AgentSlot`.

Proposed scope: adopt the router benchmark (does the right skill
fire on the right prompt?) and the body-hardening discipline
(ownership, do-not-activate routing, examples, gotchas) for xvision
skills. Add CI gates that fail on skill-body health regressions.

**Sub-plan**: needs to be written. Proposed path:
`docs/superpowers/specs/2026-06-XX-skill-discipline-adoption-design.md`.

**Dependencies**: none (orthogonal to the autooptimizer loop —
disciplines the agents the loop is improving, not the loop itself).
Can ship at any time.

**Gate**: every skill under `.claude/skills/xvision/` passes strict
health per the Murat validators; router benchmark produces a known
baseline accuracy on a fixture corpus; CI fails on regression.

## Part 4 — Sub-plan registry

Every spec, plan, handoff, intake, and notes doc that's part of the
autooptimizer stack. Grouped by layer.

### Cross-cutting

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` | Plan | The next-wave roadmap source per CLAUDE.md; conductor decomposes one wave at a time from this |
| `docs/superpowers/plans/2026-05-27-autooptimizer-master-implementation-spine.md` | Plan | This document — the spine |

### Terminology & UX (Layer 6 / Phase 5)

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md` | Spec | Locked + amended (memory verb structure) |
| `docs/superpowers/notes/2026-05-27-autooptimizer-plain-language-audit.md` | Notes | Approved with resolved decisions + post-resolution amendment |
| `docs/design/2026-05-27-autooptimizer-designer-reference.md` | Design | Live reference for any UI work on these surfaces |
| `docs/design/2026-05-27-autooptimizer-frontend-rename-handoff.md` | Handoff | Track 2 ready to ship |
| `docs/design/2026-05-27-autooptimizer-cli-rename-handoff.md` | Handoff | Track 4 ready to ship |
| `docs/design/2026-05-27-autooptimizer-sse-registry-handoff.md` | Handoff | Track 3 ready to ship |
| `docs/design/2026-05-27-autooptimizer-skills-docs-sweep-handoff.md` | Handoff | Track 5 ready to ship (blocked by Track 4) |
| `docs/design/2026-05-27-autooptimizer-spec-amendment-handoff.md` | Handoff | Track 1 ready to ship |
| `team/intake/2026-05-27-autooptimizer-terminology-rollout.md` | Intake | Wave intake decomposing the rollout into five tracks |

### Eval engine (Layer 1 / Phase 0)

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/specs/2026-05-08-eval-engine-design.md` | Spec | Shipped (foundation) |
| `docs/superpowers/specs/2026-05-08-eval-engine-decisions-so-far.md` | Spec | Shipped (decision log) |
| `docs/superpowers/plans/2026-05-08-eval-engine-plan.md` | Plan | Shipped (foundation); ongoing accuracy work tracked under separate intake |
| `docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md` | Research | Backs the accuracy follow-on waves |

### Cortex memory + DSRs flywheel (Layer 2)

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/specs/2026-05-24-cortex-memory-cline-dspy-flywheels.md` | Spec | V2D shipped; cortex-core Path A is the chosen direct-dep integration |
| `docs/superpowers/plans/2026-05-11-cortex-memory-integration-plan.md` | Plan | Historical (early integration plan) |
| `docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md` | Plan | Active (current integration plan) |
| `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md` | Notes | The F+L+T leakage contract — hard physics |

### AR-1 mutator + lineage + gate + seal (Layer 3 / Phase 1)

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/specs/2026-05-09-karpathy-autooptimizer-design.md` | Spec | Locked; cross-references companion specs |
| `docs/superpowers/plans/2026-05-09-autooptimizer-1-mutator-lineage-gate-seal.md` | Plan | Partially executed (offline distill path live); cryptographic substrate + LLM mutator + CycleSeal remain |

### AR-2 cycle orchestrator + judge + evals (Layer 4 / Phase 2)

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/plans/2026-05-09-autooptimizer-2-cycle-judge-evals.md` | Plan | Planned, not yet started |

### AR-3 dashboard (Layer 5 / Phase 3)

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/plans/2026-05-09-autooptimizer-3-dashboard.md` | Plan | Planned, not yet started; some adjacent UI shipped via the memory/flywheel React features |

### Autoreason tournament (Phase 4)

| Path | Type | Status |
|---|---|---|
| (none yet) | Spec | **Gap — needs to be written.** Recommended path: `docs/superpowers/specs/2026-05-30-autoreason-tournament-integration-design.md` |
| (none yet) | Plan | **Gap — written after spec lands.** Recommended path: `docs/superpowers/plans/2026-06-XX-autoreason-tournament-implementation.md` |

### Skill discipline (Phase 6)

| Path | Type | Status |
|---|---|---|
| (none yet) | Spec | **Gap — needs to be written.** Recommended path: `docs/superpowers/specs/2026-06-XX-skill-discipline-adoption-design.md` |
| (none yet) | Plan | **Gap — written after spec lands.** |

### Marketplace plugin (Layer 7 — downstream consumer; own spine)

| Path | Type | Status |
|---|---|---|
| `docs/superpowers/specs/2026-05-09-marketplace-plugin-design.md` | Spec | Companion to the autooptimizer spec; defines what consumes CycleSeals |
| `docs/superpowers/specs/2026-05-26-marketplace-phase1-metadata-data-contract.md` | Spec | Active |
| `docs/superpowers/plans/2026-05-26-marketplace-program-strategy.md` | Plan | Active program strategy |
| `docs/superpowers/plans/2026-05-26-marketplace-design-direction.md` | Plan | Active |
| `docs/superpowers/plans/2026-05-26-marketplace-phase-f0-foundation.md` and `f1-browse`, `f2-lineage`, `f3-creator`, `f5-sell`, `f6-receipt` | Plans | Active marketplace front-end phase plans |
| `docs/superpowers/plans/2026-05-26-marketplace-f-routes-integration-addendum.md` | Plan | Active addendum |
| `docs/superpowers/notes/2026-05-27-marketplace-contracts-phase-3-5-status.md` | Notes | Status snapshot |

The autooptimizer spine does not sequence marketplace work — that's
the marketplace spine's job. The only contract between them is the
CycleSeal artifact (defined in AR-1 plan §3.4 / spec §3.4).

## Part 5 — Cross-cutting concerns

### Terminology lock applies everywhere

Every layer's CLI flag, UI label, SSE display label, error message,
and operator-facing doc must use the operator-surface name from the
terminology lock. Developer surfaces (Rust types, SQLite columns,
spec docs, API field names) keep technical names. New concepts
require a new row in the lock; if a sub-plan introduces an
operator-facing term that isn't in the lock, the sub-plan is
blocked until the lock is amended.

### F+L+T leakage protection applies to every memory-touching change

Per `docs/superpowers/notes/2026-05-21-v2d-memory-cortex-tiers-and-leakage.md`:

- F (structural) — agents read only Patterns, never Observations
- L (rhetorical) — case-law framing at recall time
- T (temporal) — `training_window_end < scenario.start` filter

Any code path touching `recall()`, `query()`, or the
Observation/Pattern boundary requires a leakage regression test on a
scenario whose `training_window_end` overlaps the query window. No
exceptions.

### Acceptance gates use the "evidence before assertions" pattern

Per project convention (CLAUDE.md, the verification-before-completion
skill): every gate requires running verification commands and
confirming output before any "done" claim. The spine's phase gates
above are stated in operational terms (commands that pass, behaviors
that hold), not subjective terms ("looks good").

### Deployment guardrails apply to any cycle that runs against live data

Per CLAUDE.md deployment guardrails: never run `cargo` on remote
hosts; never build Docker images on small VPS; deploy from local
build host or GHCR. The evening cycle is offline by design, but any
machine that runs it needs to follow the cache-discipline rules
(don't leave `target/` trees behind in worktrees).

### Single-writer file registry

If any phase introduces a file in `team/CONFLICT_ZONES.md`, it must
be declared before work starts. The terminology rollout tracks
explicitly don't overlap any files; future phases must check.

## Part 6 — Gap analysis

What isn't planned yet, in priority order.

### Gap 1 — Autoreason tournament integration design

**Why it matters**: the current AR-1 mutator is single-shot
LLM-proposed-validator-checked-retried-twice. The autoreason evidence
(NousResearch, March 2026) shows that single-shot critique-and-revise
*degrades* weak models (Haiku 3.5 outputs shrunk 59-70% over 15
passes). The tournament structure (incumbent + adversarial + synthesis
judged blind) sidesteps this because adversarial revision and
synthesis run in parallel rather than serially. For xvision the
tournament should be straightforward to bolt onto AR-1's
`Mutator::propose()` API — but it needs a written spec covering
token budget (3× the single-shot cost; Haiku still cheap),
restraint primitive (incumbent as first-class option), judge panel
size (Borda count needs at least 3 fresh judges per round), and
acceptance criteria.

**Recommended action**: write
`docs/superpowers/specs/2026-05-30-autoreason-tournament-integration-design.md`
before Phase 4 work starts.

### Gap 2 — Skill discipline adoption design

**Why it matters**: xvision has ~30 skills under
`.claude/skills/xvision/` and per-slot prompts inside the agent
runtime. None of them have a router-accuracy benchmark, body-health
score, or strict activation criteria. The Murat skill-pack (v2.3.0,
2026-05-22) provides validators and a benchmark methodology
specifically for this. Adopting it would catch skill-bloat
regressions in CI and improve activation precision; both directly
benefit the agent-side of the flywheel that AR-2 will optimize.

**Recommended action**: write
`docs/superpowers/specs/2026-06-XX-skill-discipline-adoption-design.md`
before any new skill is added to `.claude/skills/xvision/`.

### Gap 3 — Eval engine accuracy hardening

**Why it matters**: the AR-1 numeric gate depends on
deterministic paper-test metrics. If the eval engine's
execution-accuracy work surfaces any non-determinism, the entire
gate's interpretation changes. There's existing research at
`docs/superpowers/research/2026-05-19-eval-data-and-execution-accuracy.md`
but no plan-of-work for the gap-closing.

**Recommended action**: separate intake — out of scope for this
spine because it's an L1 concern. Note as a dependency.

### Gap 4 — Marketplace handoff contract documentation

**Why it matters**: AR-1 emits CycleSeal; the marketplace plugin
consumes it. Today the contract is "the schema in `seal.rs`." If the
marketplace plugin's anchoring requirements change, the AR-1
SealWriter changes too. There's no shared contract spec.

**Recommended action**: small spec addition or a contract doc under
`team/contracts/cycle-seal-marketplace-contract.md` once both layers
have shipped enough to lock the interface.

### Gap 5 — Operator onboarding for the loop itself

**Why it matters**: once Phase 2 is live and the evening cycle is
running, the operator needs a first-run setup walkthrough: how to
pick the initial seed strategies, how to set the minimum-improvement
threshold for their risk tolerance, how to interpret the proposer
scoreboard, what to do when the honesty check fails. None of this
exists.

**Recommended action**: a MANUAL.md chapter that lands with Phase
3 (the dashboard) — when the operator surface exists, document how
to use it.

## Part 7 — Suggested execution order

Two parallelization paths depending on team size.

### Single-engineer rollout (sequential, ~4–6 weeks)

1. Phase 5 Track 1 (spec amendment) and Track 3 (SSE registry) — 1 day
2. Phase 1 remainder (AR-1 cryptographic substrate + LLM mutator +
   CycleSeal) — 2 weeks
3. Phase 5 Tracks 2, 4, 5 (frontend, CLI, docs sweep) — 1 week,
   interleaved with Phase 1 builds
4. Phase 4 spec (autoreason tournament design) — 2 days
5. Phase 2 (AR-2 cycle orchestrator + judge + evals) — 1.5 weeks
6. Phase 4 implementation (tournament integration into AR-1) — 1 week
7. Phase 3 (AR-3 dashboard) — 1.5 weeks
8. Phase 6 (skill discipline) — independent; ship when capacity
   allows

### Multi-engineer rollout (parallel, ~2–3 weeks)

- **Engineer A (Rust)**: Phase 1 → Phase 4 spec → Phase 2 → Phase 4
  implementation
- **Engineer B (Rust + frontend)**: Phase 5 Track 3 → Phase 3
- **Engineer C (frontend)**: Phase 5 Tracks 2, 4 — independent
- **Engineer D (writer or generalist)**: Phase 5 Tracks 1, 5 (5
  blocked by 4) and the gap-3/gap-5 docs

Phases 1 → 2 → 3 → 4 must sequence on the same engineer (or a tight
handoff) because Phase 4 amends AR-1 and Phase 3 depends on the SSE
events Phase 2 emits.

## Part 8 — How to use this when adding new work

If you're about to add a new spec, plan, or handoff to the
autooptimizer stack:

1. **Find the layer.** Map your work to one of the seven layers
   above. If it doesn't fit any layer, the spine needs amendment
   first — flag and discuss.
2. **Find the phase.** Map your work to one of the six phases. If
   it's adding to an existing phase (a new sub-plan inside a track),
   add a row to Part 4's registry. If it's a new phase, the spine
   needs amendment.
3. **Use the operator-surface vocabulary.** Every user-facing string
   in your work uses names from
   `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`.
   If you need a new operator-facing concept, amend the lock first.
4. **Add an entry to Part 4's registry.** Path, type (spec / plan /
   handoff / intake / notes), and current status.
5. **Define your acceptance gate in operational terms.** "Command X
   produces output Y" not "tested manually and works."
6. **Identify dependencies.** Which earlier phase/sub-plan must be
   complete before your work can start? Which later work depends on
   yours?
7. **If your work spans multiple existing files** (e.g., a rename
   that touches both CLI and frontend), produce one intake or
   coordinating doc — don't fork the conversation across N PRs.

## Part 9 — Risks and mitigations

| Risk | Mitigation |
|---|---|
| AR-1's cryptographic substrate slips and AR-2/AR-3/Phase 4 all wait | Phase 1 has a clear scope boundary; if it slips past 2 weeks, downgrade the gate to "deterministic distill path + lineage + numeric gate" without CycleSeal, ship Phase 2 against that, and circle back to CycleSeal for marketplace integration |
| Phase 4 (autoreason tournament) costs 3× the LLM budget and operator can't afford it | Tournament is opt-in via config; default to single-shot mutator (current behavior). Operators with budget headroom flip on tournament mode. Gate doesn't change either way. |
| Eval engine determinism regresses and Phase 1's gate becomes unreliable | Gap 3 hardening is a precondition; ship a determinism probe in CI before Phase 1 ships |
| Terminology rollout (Phase 5) ships partially and operators see mixed vocabulary | The wave intake mandates the fresh-operator smoke test; nothing closes until banned-words check passes on every shipped surface |
| F+L+T leakage protection regresses with the cortex-core Path A direct dependency | Every memory-touching change requires a leakage regression test on overlapping-window scenarios; CI enforces |
| Marketplace plugin downstream contract changes mid-flight | Pin the CycleSeal schema with a version field; marketplace publishes the minimum schema version it accepts; AR-1 SealWriter checks compatibility |
| One engineer holds context for Phase 1 → 2 → 3 → 4 chain | Sub-plan docs are designed to be self-contained handoffs; the spine is the integration map; pairing on the Phase 1 → 2 boundary mitigates the worst case |

## Part 10 — Reference

- Project-wide terminology and conventions: `/CLAUDE.md`
- Team coordination process: `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md`
- Active execution board: `team/board.md` and `team/board-v2.md`
- Next-wave roadmap source: `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md`
- All sub-plans and handoffs: see Part 4 registry above
- Cross-stack architecture overview (whole project): `architecture.md`
- Implementation guidance for picking up plans: the
  `superpowers:subagent-driven-development` or
  `superpowers:executing-plans` skills (mentioned at the top of each
  AR-N plan)
