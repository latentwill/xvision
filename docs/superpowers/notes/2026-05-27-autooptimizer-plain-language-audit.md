# AutoOptimizer plain-language audit — 2026-05-27

> Status: operator-approved 2026-05-27; superseded by terminology lock
> at `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`
> Companion specs: `2026-05-09-karpathy-autooptimizer-design.md`,
> `2026-05-24-cortex-memory-cline-dspy-flywheels.md`
> AR plans this affects: AR-1 (mutator+lineage+gate+seal), AR-2 (cycle
> orchestrator+judge+canary+inversion+diversity), AR-3 (dashboard surfaces)
> Decisions locked: see "Resolved decisions" section below; full
> developer↔operator mapping in the terminology lock doc above.

## Why this pass exists

The autooptimizer spec uses research-paper and cryptography vocabulary
throughout: merkle roots, content hashes, Ed25519 signatures, ε
pre-commitment, BLAKE3, inversion-pair eval, null-result canary,
diversity-decay rate, mutator-skill ladder. The marketplace pass already
proved that surface vocabulary drives adoption — once "merkle tree" was
gone from settings copy the connect-wallet flow stopped reading as
crypto-native and started reading as receipts. Doing the same exercise
now, before AR-2 ships orchestrator output and AR-3 ships the
dashboard, is cheaper than renaming after release.

## The two-surface principle (proposed addition to CLAUDE.md)

Every concept in the autooptimizer has two valid names: the
**developer-surface name** (in code, in specs, in db columns, in API
type fields) and the **operator-surface name** (in CLI flags and help
text, in UI labels, in SSE event display names, in error messages, in
docs operators read).

- **Developer surface stays precise.** `LineageStatus::Quarantined`,
  `BLAKE3`, `Ed25519`, `MerkleRoot`, `epsilon`, `holdout_window` —
  these terms carry exact technical meaning. Keep them in
  `crates/xvision-engine/src/autooptimizer/`, in
  `docs/superpowers/specs/`, in SQLite column names. Removing them
  weakens the spec and makes type signatures lie.
- **Operator surface becomes plain.** No operator should ever read the
  word BLAKE3 or Ed25519. The word "merkle" should not exist on the
  operator's screen. Replace with concrete, descriptive language tied
  to the operator's mental model.
- **The mapping is explicit.** Every renamed term gets a one-line
  footnote in the spec saying "operator-facing as `<plain name>`," so
  developers reading the spec know what surface the operator sees.

If we accept the principle, the rename only touches: CLI clap macros &
help text, frontend TSX label strings, SSE event display formatters,
the user-facing parts of `MANUAL.md` and the dashboard wiki, and the
two skill files under `.claude/skills/xvision/`. It does not touch
Rust types, SQLite schemas, API field names, internal module names,
or spec terminology in spec documents.

## How to read this audit

Each section groups terms by theme. For each term:
- **Where it appears today** (CLI flag / UI label / SSE event / status
  badge / table column the operator sees / etc.)
- **Why it needs renaming** (or why it's fine as-is)
- **Proposed operator-facing name**
- **Verdict**: ✅ KEEP · 🔄 RENAME · ❓ DECIDE (needs operator input)

## Section 1: Evolution & lineage machinery

The autooptimizer is fundamentally an evolutionary loop — parents
spawn children, children get judged, survivors reproduce. The
evolutionary metaphor is intuitive and the operator pulls genealogy
mental models from biology and version control. Most of this block
should keep its current names with one critical rename around the
status enum.

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Lineage | Dashboard tab, CLI `xvn flywheel lineage`, API responses | ✅ KEEP | Lineage | Clear evolutionary meaning; one-word; reads on a projector |
| Mutation | Spec/plan term, future SSE events `mutation_proposed`, `mutation_committed` | 🔄 RENAME (operator-side only) | "Experiment" | A "mutation" sounds genetic-engineering ominous; an "experiment" sounds like research, which is what we're doing. Rust types stay `Mutation` / `MutationDiff`. |
| Mutator | Spec, future CLI output, dashboard live-cycle view | 🔄 RENAME (operator-side only) | "Proposer" or "Experiment writer" | The thing that writes the candidate; "mutator" implies it changes things, "proposer" implies it suggests one. Rust stays `Mutator`. |
| LineageStatus::Active | Status badge in dashboard, future API field | ✅ KEEP | "Active" | Already clear |
| LineageStatus::Ghost | Status badge, dashboard genealogy nodes (faded) | 🔄 RENAME | "Rejected" | "Ghost" is dev jargon for "kept in graph but not counted"; "Rejected" tells the operator what happened |
| LineageStatus::Quarantined | Status badge for inversion-pair-flagged | 🔄 RENAME | "Suspect" or "Held for review" | "Quarantined" carries medical/security connotations; "Suspect" matches the underlying meaning (gate passed but inversion check raised a flag) |
| Mutator-skill ladder | Dashboard view #4 in spec §9 | 🔄 RENAME | "Proposer scoreboard" or "Experiment-writer track record" | "Mutator-skill ladder" is three pieces of jargon in one phrase. The view shows how good the LLM is at proposing experiments that survive. |
| Parent / Child (bundle) | All over CLI + UI | ✅ KEEP | Parent / Child | Evolution metaphor lands cleanly |
| Cycle | Locked in CLAUDE.md, used throughout | ✅ KEEP | Cycle | Already in the term lock |
| Evening cycle | Spec language for "the nightly run of the loop" | ❓ DECIDE | "Evening run" or "Overnight session" | Both work; "evening run" matches `xvn ... run` verb pattern |

## Section 2: Cryptographic provenance

This is the marketplace-rename territory. The spec correctly uses
BLAKE3, Merkle root, Ed25519, content-hash, canonical JSON — these
are real cryptographic constructions and the spec needs them.
*Operators must never see them.* Every appearance of these terms on
an operator surface is a defect after this audit lands.

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| BLAKE3 | Spec, never user-facing today (good) | ✅ KEEP (spec only) | — | Never expose. If a fingerprint hash appears in UI it's just "ID." |
| Content hash / bundle_hash | API field, dashboard genealogy node ids, lineage diffs | 🔄 RENAME (operator-side) | "Strategy fingerprint" or "Strategy ID" | The thing the operator sees is a 64-character hex string identifying a strategy version. "Fingerprint" is the universal plain word for "unique short ID derived from content." |
| Merkle root | Spec, future dashboard seal-detail view, future marketplace anchor | 🔄 RENAME | "Cycle proof" or "Evening proof" | The merkle root proves the cycle's contents weren't tampered with. "Proof" is the operator word for "this is what it was." |
| CycleSeal | Spec, dashboard "Cycle sealed" event, future seal-detail view | 🔄 RENAME | "Evening summary" or "Cycle receipt" | A seal is a cryptographic notion; a receipt is what an operator reads. The artifact itself is "what happened this evening with proof attached." |
| SessionCommitment | Spec, future settings page, dashboard topbar (session info) | 🔄 RENAME | "Session ground rules" or "Session sign-off" | The pre-commitment is "here are the rules I'm playing by tonight, signed by me." |
| Operator signature | Spec, future seal-detail "verified" badge | 🔄 RENAME | "Your signature" or "Operator sign-off" | "Operator signature" is half plain, half technical; pick one. |
| Operator key / Ed25519 | Spec, never user-facing today (good) | ✅ KEEP (spec only) | — | Never expose. If we need to refer to "the key file" in CLI output, say "your signing key." |
| Canonical JSON | Spec only | ✅ KEEP | — | Never user-facing |
| Signed at | Dashboard timestamp field | ✅ KEEP | "Signed at" | Plain English already |

## Section 3: Gate / decision machinery

The numeric gate is the heart of the loop. Current vocabulary is
heavily research-paper (ε, Δ-Sharpe, holdout). All of it survives in
the spec; none of it should reach the operator unchanged. The CLI
flags here are the most jargon-dense surface in the entire codebase
right now (`--gate-epsilon`, `--parent-day-score`,
`--child-holdout-score`).

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Gate | CLI `xvn autooptimizer gate`, dashboard "Record Gate" button, SSE | 🔄 RENAME | "Check" or "Decision" | "Gate" is OK but "check" is more obvious. Both verbs work; let's pick "check" for the verb and "decision" for the noun. CLI becomes `xvn autooptimizer check`. |
| Gate verdict | API field, UI display "Gate passed/failed" | 🔄 RENAME | "Verdict" → "Decision" or "Outcome" | "Verdict" sounds courtroom-formal. The two values become "Kept" and "Dropped" instead of "Passed" and "Failed" (matches what actually happens to the candidate). |
| Gate epsilon / `--gate-epsilon` / "Epsilon" form label | CLI flag, dashboard form label (×8 locations) | 🔄 RENAME | "Minimum improvement" / `--min-improvement` | The single ugliest term in the surface. Operator has to look up that ε means "tolerance threshold." Just say "minimum improvement (Sharpe)." |
| Delta day / `--parent-day-score` / `--child-day-score` | CLI flags, dashboard form labels | 🔄 RENAME | "Today's score" (parent + child) / `--baseline-today-score`, `--candidate-today-score` | "Parent day" means "the existing strategy's score on today's data"; operator-facing it's "baseline today vs candidate today." |
| Delta holdout / `--parent-holdout-score` / `--child-holdout-score` | CLI flags, dashboard form labels | 🔄 RENAME | "Untouched-period score" / `--baseline-untouched-score`, `--candidate-untouched-score` | "Holdout" is statistics jargon. The point is: a window of market history the strategy never trained on. "Untouched period" or "unseen period" both work; "untouched" is more honest about what makes it a holdout. |
| Holdout window | Spec config, dashboard topbar info | 🔄 RENAME | "Untouched test period" | Same reason as above. Whatever we pick must be used consistently — the operator should never see both "holdout" and "untouched." |
| Holdout split / "70/15/15" | Dashboard form field for optimization | 🔄 RENAME | "Training / dev / test split" with explanation tooltip | "Holdout split" hides what the three numbers mean |
| Pre-commitment | Spec §7, future session-init copy | 🔄 RENAME | "Session ground rules (locked at start)" | A pre-commitment is a promise made before the experiment runs. "Ground rules locked at start" tells the operator what it does. |
| Baseline score / Candidate score | CLI flags `--baseline-score` / `--candidate-score` | ✅ KEEP | Same | Already plain |
| Min delta / `--min-delta` | CLI flag | 🔄 RENAME | "Minimum improvement" / `--min-improvement` | Match the renamed epsilon |

## Section 4: Loop control & sanity-check evaluations

The five novel evals from spec §8 (counterfactual-chain Merkle root,
null-result canary, inversion-pair, mutator-skill ladder,
embedding-divergence diversity-decay) are all good ideas with bad
names. Three of them are user-facing (canary alarms, inversion flags,
diversity decay charts) and need renaming.

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Null-result canary | Spec §8.1, future SSE `canary_outcome`, future dashboard alarm | 🔄 RENAME | "Honesty check" or "Decoy strategy" | A canary is mining-industry jargon for "an early warning signal." We inject a deliberately broken parent; if the gate accepts mutations against it, the gate is lying. "Honesty check" describes what it tests. |
| Canary outcome / canary_runs table | SSE event, table name | 🔄 RENAME (operator-side) | "Honesty check result" / SSE event `honesty_check_result` | Match the rename. SQLite table can stay `autooptimizer_canary_runs` (developer surface). |
| Inversion-pair eval | Spec §5.3, future SSE `mutation_quarantined` reason | 🔄 RENAME | "Reverse-mutation check" or "Noise check" | "Inversion" is technical; "reverse" is plain. The point: if reversing the mutation gives the same score, the original mutation might have just been noise. |
| Diversity-decay rate | Spec §8.3, future dashboard sparkline | 🔄 RENAME | "Variety score" or "Sameness drift" | "Diversity-decay" is two abstractions stacked. The chart shows whether siblings are becoming more similar over time. "Variety score" (high = explorations stay varied, low = mode collapse) is the operator-meaningful version. |
| Counterfactual-chain Merkle root | Spec §6.2, future lineage detail | 🔄 RENAME | "Lineage proof" | Already covered under section 2; matches the "Cycle proof" pattern. |
| Embedding-divergence | Spec §8.3, technical detail | ✅ KEEP (spec only) | — | The mechanism powering "variety score" doesn't need to surface |

## Section 5: Memory layer (Observation / Pattern / Tier / F+L+T)

The cortex-memory plan already locked Observation and Pattern as
tier names. Both work as operator surface. The pain points here are
"tier" itself (opaque), `promotion_state` (gerundy), `training_window_end`
(jargon disguised as English), and "demote" (sounds like punishment).

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Observation | UI tab, CLI `xvn memory ls --tier observation`, API field | ✅ KEEP | Observation | Plain English already, lock in CLAUDE.md confirms |
| Pattern | UI tab, CLI `xvn memory add-pattern`, API field, count of "Active patterns" | ✅ KEEP | Pattern | Same |
| Tier | CLI flag `--tier`, internal enum | 🔄 RENAME (CLI surface) | "Kind" / `--kind` | "Tier" implies hierarchy (Tier 1, Tier 2). The two values are just two kinds of memory items. |
| promotion_state | CLI flag `--promotion-state`, API field, dashboard filter dropdown | 🔄 RENAME | "Status" / `--status` | "Promotion state" is two abstractions. The three values (staged, active, forgotten) are statuses. |
| Staged | Pattern status badge, dashboard filter option | ✅ KEEP | Staged | Already plain |
| Active | Pattern status badge | ✅ KEEP | Active | Already plain |
| Forgotten | Pattern status badge, dashboard filter option | ✅ KEEP | Forgotten | Already plain (and gentle — better than "Deleted") |
| Demote | CLI `xvn memory demote`, UI "Demote" button | 🔄 RENAME | "Retire" | "Demote" punishes the pattern; "Retire" tells the operator the pattern is no longer in active use. Same data behavior. |
| Promote | CLI `xvn memory promote`, UI "Promote" button | 🔄 RENAME | "Activate" | The UI already uses "Activate" in places; consolidate on one verb. CLI becomes `xvn memory activate` (which already exists for the by-id form). |
| training_window_end | API field, UI label "Training data ends", form label | 🔄 RENAME | "Knowledge cutoff" or "Training data ends" (the UI label is already plain, the API field and CLI flag are not) | UI label "Training data ends" is fine. CLI flag `--training-end` is fine. The API field `training_window_end` and spec usage should stay (developer surface). |
| F+L+T | Spec / notes only | ✅ KEEP (spec only) | — | This is a design-principle name, never user-facing |
| Case-law framing | Spec / notes only, internal prompt wrapping | ✅ KEEP (spec only) | — | Never user-facing |
| Namespace | UI label, CLI `--namespace`, status displays | ❓ DECIDE | "Scope" / `--scope` or keep "Namespace" | "Namespace" is programming jargon. "Scope" is plainer and matches what the values do (`global` = everywhere, `agent:<id>` = scoped to one agent). But `namespace` is so widespread in the codebase + docs that the rename is expensive. Worth doing only if operator confirms the term confuses people. |
| Memory mode (Off / Global / AgentScoped) | Internal enum, future settings UI | 🔄 RENAME (UI only) | "Memory" with three options: "Off" / "Shared across all agents" / "This agent only" | Self-explanatory replacements |

## Section 6: Session and cycle machinery

Mostly fine. The locked CLAUDE.md vocabulary (cycle, session) is
operator-readable. One real issue: session_id and cycle_id appear as
26-character ULIDs on screen, which look like serial numbers but
aren't readable. We need a display name strategy.

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Session | CLI, future UI topbar | ✅ KEEP | Session | Clear |
| Cycle | CLI, UI, locked term | ✅ KEEP | Cycle | Locked |
| Run | CLI `xvn autooptimizer run`, UI "Runs" count | ✅ KEEP | Run | Clear |
| session_id (ULID) | API field, UI display | 🔄 RENAME (UI surface) | Show as "Session #<short>" using first 6 chars of ULID with copy-on-click for full ID | Operators don't process 26-character strings visually; give them a recognizable short label. |
| cycle_id (ULID) | Same | 🔄 RENAME (UI surface) | "Cycle #<short>" same pattern | Same |
| run_id (ULID) | Same | 🔄 RENAME (UI surface) | "Run #<short>" same pattern | Same |
| bundle_hash (64-hex) | Dashboard genealogy nodes, lineage rows | 🔄 RENAME (UI surface) | "Strategy <short>" using first 8 hex chars, copy full on click | Same |

## Section 7: Flywheel / lineage view

"Flywheel" is borrowed business-strategy vocabulary (Bezos popularized
it). It's slightly jargon-y but lands as concrete imagery — a spinning
wheel that gains momentum — and it gives the surface a memorable name.
Keep it. The pain in this section is around "optimization" terminology
which is research-code dense.

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Flywheel | Dashboard tab, CLI `xvn flywheel`, API endpoints | ✅ KEEP | Flywheel | Memorable, concrete, already widely used |
| Velocity | CLI `xvn flywheel velocity`, UI metric labels | ✅ KEEP | Velocity | Plain English |
| Status (flywheel status) | CLI `xvn flywheel status` | ✅ KEEP | Status | Plain |
| Lineage depth | UI metric label "Lineage Depth" | 🔄 RENAME | "Generations deep" or "Evolution depth" | "Lineage depth" reads as "how deep is the lineage measurement" rather than "how many generations of agents have evolved." |
| Optimization / optimization_id | CLI `xvn flywheel lineage` output, UI "Optimization History" header, API fields | 🔄 RENAME (operator surface) | "Training run" / "Optimizer run" | "Optimization" is research terminology. What the operator sees is: we trained a new version of the agent. "Training run" is the plain term. |
| Target agent / target_agent_id | Lineage CLI output, UI display | 🔄 RENAME | "Parent agent" | The thing the optimization is targeting is the agent it's trying to improve; that's the parent. |
| Child agent / child_agent_id | Same | ✅ KEEP | Child agent | Already plain |
| Demos / Demo Source / Demo Patterns | UI form labels for child agent optimization | 🔄 RENAME | "Examples" / "Example source" / "Example patterns" | "Demo" is DSPy-internal terminology (the framework uses "demonstrations"). The operator-meaningful term is "examples." |
| Frozen / Fresh (Demo Source options) | UI dropdown options | 🔄 RENAME | "Use saved examples" / "Capture new examples" | The current options require knowing what "frozen" means in this context |
| Priors / Prior Patterns / Use recalled Pattern priors | UI checkbox + metric | 🔄 RENAME | "Background patterns" or "Recalled context patterns" / checkbox: "Include patterns I've already learned" | "Priors" is statistical vocabulary; "background patterns" is the operator-readable version |
| Train / Dev / Holdout (split, demos, observation_count, hash) | UI labels, API fields, CLI output | 🔄 RENAME (operator surface) | "Training / Validation / Untouched test" | Match the section-3 rename of holdout |
| Mint Child / Mint button | UI button label | 🔄 RENAME | "Train new version" | "Mint" carries cryptocurrency baggage. The button creates a trained child agent. |

## Section 8: Optimization parameters (sub-surface of flywheel)

Most of these renames already appeared in section 7; consolidating
the remaining few here.

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Embedding / embedding_json / `--embedding-json` | CLI flag, UI label "Embedding JSON" | ❓ DECIDE | Keep flag name but rename UI label to "Test embedding vector (advanced)" | The flag is intentionally power-user only (used for deterministic tests). UI label exposing raw JSON is a defect — should be hidden behind an "Advanced" disclosure. |
| Embedder / embedder_id | CLI warning text "no embedder configured" | 🔄 RENAME | "embedding provider" (so warning reads "no embedding provider configured") | "Embedder" is internal jargon; "embedding provider" matches the existing "model provider" language |

## Section 9: Attestation

This block already does most things right. One ugly piece: the CLI
flag `--attest-null-window` packs three concepts into a hyphenated
phrase.

| Today | Where | Verdict | Proposed | Why |
|---|---|---|---|---|
| Operator attestation | UI form section, API type | 🔄 RENAME | "Operator sign-off" or "Manual approval" | "Attestation" is legal/audit jargon |
| `--attest-null-window` | CLI flag on `xvn memory add-pattern` | 🔄 RENAME | `--no-training-cutoff` or `--confirm-no-cutoff` | The current flag means "I confirm this pattern has no training cutoff date." Make the flag say what it means. |
| Operator initials / `--operator-initials` | CLI flag, UI form label | ✅ KEEP | Operator initials | Already plain |
| training_window_end (NULL) / "null window" | Concept in attestation flow | 🔄 RENAME (operator surface) | "No training cutoff" | The thing the operator is attesting to is that they explicitly allow the pattern to be recalled in every scenario regardless of date. Plain English: "no training cutoff." |

## Section 10: CLI verb structure

The current verb tree is well-organized. Some renames worth proposing:

| Today | Verdict | Proposed | Why |
|---|---|---|---|
| `xvn autooptimizer run` | ✅ KEEP | Same | Plain |
| `xvn autooptimizer gate` | 🔄 RENAME | `xvn autooptimizer check` | Section 3 rename |
| `xvn autooptimizer promote` | 🔄 RENAME | `xvn autooptimizer activate` (or `... approve`) | Match memory-layer rename |
| `xvn autooptimizer demote` | 🔄 RENAME | `xvn autooptimizer retire` | Same |
| `xvn autooptimizer inspect` | ✅ KEEP | Same | Plain |
| `xvn autooptimizer ls` | ✅ KEEP | Same | Standard CLI verb |
| `xvn memory ls` | ✅ KEEP | Same | Standard |
| `xvn memory promote` | 🔄 RENAME | `xvn memory activate` | Consolidate verbs |
| `xvn memory demote` | 🔄 RENAME | `xvn memory retire` | Consolidate |
| `xvn memory forget` / `xvn memory undo-forget` | ✅ KEEP | Same | "Forget" + "undo-forget" is plain and obvious |
| `xvn memory add-pattern` | ✅ KEEP | Same | Plain |
| `xvn flywheel status` / `velocity` / `lineage` | ✅ KEEP | Same | All plain |

## Section 11: SSE event display names

The event names themselves (`mutation_proposed`, `cycle_sealed`, etc.)
are developer surface — they're wire-protocol identifiers. What the
operator sees is the **display label** the dashboard renders. Here
are proposed display labels per event (the wire names stay):

| Wire event name (keep) | Operator display label (rename) |
|---|---|
| `cycle_started` | "Evening run started" |
| `mutation_proposed` | "Experiment proposed" |
| `mutation_evaluating` | "Testing experiment" |
| `mutation_committed` | "Experiment kept" |
| `mutation_rejected` | "Experiment dropped" |
| `mutation_quarantined` | "Experiment flagged for review" |
| `lineage_forked` | "New branch added" |
| `judge_wrote_finding` | "Reviewer finished notes" |
| `canary_outcome` | "Honesty check result" |
| `diversity_updated` | "Variety score updated" |
| `ladder_snapshot` | "Proposer scoreboard updated" |
| `cycle_sealed` | "Evening summary signed" |
| `cycle_failed` | "Evening run failed" |

## Resolved decisions (2026-05-27)

Operator decisions on the five open questions, locked:

1. **"Mutation" → "Experiment".** Operator-side. Karpathy's literal
   framing wins. Rust types and spec layer keep `Mutation` /
   `MutationDiff`.

2. **"Mutator" → "Experiment writer".** Operator-side. Matches the
   "Experiment" rename. Rust type stays `Mutator`. SSE event
   `mutation_proposed` display label becomes "Experiment proposed";
   wire name unchanged.

3. **"Canary" → "Honesty check".** Operator-side. Tells the operator
   what the mechanism tests, not what it is. SQLite table stays
   `autooptimizer_canary_runs`; SSE wire name stays `canary_outcome`
   with display label "Honesty check result".

4. **"Namespace" stays.** No rename. Term is widespread, value is
   unclear. If operator confusion shows up later we revisit.

5. **CLI verb `gate` stays.** No rename to `check`. Touches too many
   downstream surfaces (the autooptimizer-ops skill, operator scripts,
   the gate flag families). The form label "Gate" in UI stays;
   verdict values still become "Kept" / "Dropped" per section 3.

All other proposed renames from sections 1–10 are approved as drafted.
The full canonical mapping lives in
`docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`.

## Post-resolution amendments (2026-05-27)

After the initial lock landed, the CLI handoff review caught one
problem with the §10 entry that consolidated `xvn memory promote`
onto `xvn memory activate`: the existing `xvn memory activate <id>`
verb already has different semantics (single-Pattern form by id),
and folding the cohort-distillation flag set into the same verb
would muddle the call sites. The operator approved an amendment:

- `xvn memory promote --ids ... --text ...` (Observation cohort →
  Pattern) → `xvn memory distill --ids ... --text ...` (its own
  verb).
- `xvn memory activate <id>` (single-Pattern form) stays unchanged.
- `xvn memory promote` ships as a hidden alias for
  `xvn memory distill` for one release.

The amendment is reflected in the terminology lock's §5 and §10
tables and in the lock's "Amendments" section. The five handoff
docs were updated in the same pass.

## Next steps after this audit lands

Once you've reviewed and decided the open questions, the rename
deliverables fall out as:

1. **Append a "Operator-facing names" column to the CLAUDE.md
   terminology table.** This is the new term lock; it codifies that
   developer and operator surfaces can use different names for the
   same concept and lists every accepted pair.

2. **Spec amendment to `2026-05-09-karpathy-autooptimizer-design.md`.**
   Add a §0.1 "Operator vocabulary" with the rename table; footnote
   every first appearance of a renamed term in §3–§9 with the
   operator-facing version.

3. **Three implementation patches.** One per surface:
   - **CLI patch**: rename clap commands/flags/help text in
     `crates/xvision-cli/src/commands/autooptimizer.rs` and
     `crates/xvision-cli/src/commands/flywheel.rs`. Keep old verbs as
     hidden aliases for one release. Update
     `crates/xvision-cli/tests/cli_surface_snapshot.json`.
   - **Frontend patch**: rename TSX label strings in
     `frontend/web/src/features/memory/MemorySurface.tsx`,
     `MemoryPage.tsx`, and the agents-flywheel route. No API changes.
   - **Dashboard / SSE patch**: add a display-label registry in
     `crates/xvision-dashboard/src/sse.rs` that maps wire event names
     to operator display labels. Update `static/js/bus.js`.

4. **Skill files.** Update `.claude/skills/xvision/autooptimizer-ops/SKILL.md`
   and `.claude/skills/xvision/flywheel-ops/SKILL.md` to use the new
   operator-facing CLI verbs.

5. **MANUAL.md and dashboard wiki** sweep for old terminology.

The CLI rename should ship with deprecation aliases so existing
operator workflows don't break. Frontend and SSE label renames are
cosmetic and ship together.

## Appendix A — CLI surface inventory (condensed)

Full source: subagent inventory from this session.

- `xvn autooptimizer run | ls | inspect | gate | promote | demote` — 6
  subcommands. ~25 flags total. Flags carrying the most jargon today:
  `--gate-epsilon`, `--parent-day-score`, `--child-day-score`,
  `--parent-holdout-score`, `--child-holdout-score`,
  `--finding-blinded-metrics`, `--judge-model`, `--qualitative-finding-json`.
- `xvn flywheel status | velocity | lineage` — 3 subcommands. Output
  includes `optimization_id`, `target_agent_id`, `child_agent_id`,
  hash strings, `gate verdict`, `delta_dev`, `delta_holdout`, lineage
  depth float.
- `xvn memory ls | namespaces | show | add-pattern | promote |
  activate | demote | rm | forget | undo-forget` — 10 subcommands.
  Flags include `--tier`, `--promotion-state`, `--training-end`,
  `--attest-null-window`, `--operator-initials`, `--include-forgotten`,
  `--forgotten-only`.

## Appendix B — UI surface inventory (condensed)

Full source: subagent inventory from this session.

- **Pages**: Memory page (`/memory`), Memory tab on agent page,
  Agents flywheel route (`/agents/:id/flywheel`).
- **Card titles**: "Memory", "Flywheel".
- **Sub-tabs**: "Patterns", "Observations".
- **Buttons that need rename**: "+ Add Pattern" (keep), "Activate"
  (keep), "Demote" → "Retire", "Promote" → "Activate", "Mint Child" →
  "Train new version", "Record Optimization Gate" → "Record check",
  "Forget all memory…" (keep), "Confirm forget" (keep).
- **Status badges**: "Active" (keep), "Staged" (keep), "Forgotten"
  (keep), "passed"/"failed" → "Kept"/"Dropped", "Ghost" → "Rejected",
  "Quarantined" → "Suspect".
- **Form labels with jargon**: "Epsilon" → "Minimum improvement",
  "Parent Day"/"Child Day" → "Baseline today"/"Candidate today",
  "Parent Holdout"/"Child Holdout" → "Baseline untouched"/"Candidate
  untouched", "Demo Source: Frozen/Fresh" → "Example source: Use
  saved/Capture new", "Demo Patterns" → "Example patterns", "Prior
  Patterns" → "Background patterns", "Holdout Demos" → "Untouched
  examples", "Embedding JSON" → "Test embedding vector (advanced)".
- **Metric labels**: "Obs / 7d", "Promoted / 7d", "Demoted / 7d",
  "Children / 7d", "Lineage Depth" → "Generations deep".
- **Empty / error states**: All current copy is plain and stays.

## Appendix C — Spec terminology inventory (condensed)

Full source: subagent inventory from this session. Lists 85+ named
concepts across the autooptimizer spec, AR-1/2/3 plans, and
cortex-memory plan. Of those:

- **30 are user-facing** (appear in CLI, UI, or operator-readable
  docs) — these are the renames proposed in sections 1–10 above.
- **55 are spec/developer-internal** (Rust types, SQLite columns,
  cryptographic primitives, internal prompt-engineering concepts).
  These keep their current names.

The full mapping will land as the "Operator-facing names" column in
the CLAUDE.md terminology table once decisions on the open questions
are made.
