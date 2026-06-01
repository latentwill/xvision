# AutoOptimizer designer reference — every label, every button, every word

> Status: locked 2026-05-27 (terminology lock + amendment)
> For: anyone designing, auditing, or implementing any autooptimizer,
> memory, or flywheel UI surface
> This doc is self-contained. You should not need to read anything
> else to design or QA a screen.

## How to use this document

This is the single reference for every piece of operator-facing copy
on the autooptimizer, memory, and flywheel surfaces. If you are
designing a new screen, auditing an existing one, or writing copy
for a related feature, use the vocabulary defined here.

If a label, button, or message in this doc doesn't fit the design
context you're working in, push back — flag the conflict and we
update this doc. Don't invent new vocabulary on the spot. The whole
point is consistency.

If a piece of copy you need isn't covered here, that's a gap in this
doc — flag it and we add it.

---

## Part 1 — The two-surface principle

Every concept has two names: a developer-surface name (used in code,
API fields, database columns, spec documents) and an operator-surface
name (used in everything the user reads — UI labels, CLI flags,
help text, error messages, docs). They are deliberately different.

As a designer or design-adjacent engineer, you almost never touch
developer-surface names. You read API field names like `gate_verdict`
and `delta_holdout` from the data layer and map them to operator
labels like "Gate decision: Kept" and "Untouched-period improvement"
at the render layer.

Cryptographic primitives — BLAKE3, Ed25519, Merkle, canonical JSON —
never appear on an operator surface. Ever. If you see one, it's a
defect.

---

## Part 2 — Complete vocabulary glossary

Every operator-surface term, alphabetical. Each entry: what it means,
where you'll see it, and what to call the developer-surface concept
behind it (in case you're matching API fields to display labels).

**Active**
A Pattern that's currently being recalled at decision time, or an
autooptimizer run whose Pattern was activated. Renders as a status
badge. Developer field: `promotion_state = "active"`.

**Background patterns**
Patterns the operator has already taught the agent, recalled and
shown as context when training a new agent version. Developer field:
`prior_pattern_ids[]`.

**Baseline today's score** / **Baseline untouched-period score**
The existing strategy's score on today's data / on the untouched
test period. Used in gate-decision forms. Developer fields:
`parent_day_score`, `parent_holdout_score`.

**Candidate today's score** / **Candidate untouched-period score**
The proposed new strategy's score on the same two windows.
Developer fields: `child_day_score`, `child_holdout_score`.

**Capture new examples**
The "capture fresh demonstrations from a live run" option when
configuring a new agent version's training. Developer enum value:
`demo_source = "fresh"`.

**Child agent**
The new agent version produced by a training run. Same word
developer-side: `child_agent_id`.

**Cycle**
One decision-cycle iteration — briefing → decision → outcome. Same
word developer-side; this is locked in CLAUDE.md.

**Cycle proof**
The cryptographic fingerprint over an evening run's full output,
used to prove the run wasn't tampered with. Developer term: Merkle
root.

**Decision** (in the gate context)
The result of the gate — either "Kept" or "Dropped." Developer
field: `gate_verdict` (`"passed"` / `"failed"`).

**Distill** (verb)
Turn an Observation cohort into a candidate Pattern. CLI verb:
`xvn memory distill`. Developer term: promote (from the
Observation-cohort-to-Pattern flow).

**Dropped**
The gate decision when a candidate didn't beat the baseline by
enough. Renders as a status badge. Developer value:
`gate_verdict = "failed"`.

**Evening run**
The nightly orchestrated cycle of the autooptimizer. Developer
term: evening cycle.

**Evening summary**
The signed artifact recording everything that happened in one
evening run. Developer term: CycleSeal.

**Evening summary signed**
SSE event display label for `cycle_sealed`. Means the evening run
finished and its summary was signed.

**Examples**
Demonstrations of agent behavior used in training a new version.
Developer term: demos.

**Example source**
Where the examples come from when training a new version — either
"Use saved examples" (from a snapshot) or "Capture new examples"
(record fresh). Developer field: `demo_source`.

**Experiment**
A proposed change to a strategy that the autooptimizer tests.
Developer term: Mutation / MutationDiff.

**Experiment dropped**
SSE event display label for `mutation_rejected`. The gate said no.

**Experiment flagged for review**
SSE event display label for `mutation_quarantined`. The gate said
yes but the reverse-mutation check raised a flag.

**Experiment kept**
SSE event display label for `mutation_committed`. The gate said
yes and the experiment entered the lineage as Active.

**Experiment proposed**
SSE event display label for `mutation_proposed`. The experiment
writer just wrote a candidate.

**Experiment writer**
The LLM (Haiku) that proposes experiments. Developer term: Mutator.

**Flywheel**
The continuous loop of observation → pattern → optimization →
recall. Same word developer-side.

**Forgotten**
A Pattern that's been retired but is still inside the grace window
(can be restored). Renders as a status badge. Developer value:
`promotion_state = "forgotten"`.

**Gate** (noun)
The check that decides whether a candidate is Kept or Dropped. Same
word developer-side; the verb structure (`xvn autooptimizer gate`)
also stays unchanged.

**Gate decision**
The result of running the gate. Two values: Kept or Dropped.
Developer field: `gate_verdict`.

**Generations deep**
How many parent → child agent training generations a lineage has
gone through. Renders as a metric on the Flywheel panel. Developer
field: `average_lineage_depth`.

**Global** (namespace value)
Memory scope: shared across all agents on this workspace. Same
value developer-side.

**Honesty check**
A deliberately broken strategy injected into each evening run. If
the gate accepts experiments against it, the gate is fitting noise.
Developer term: null-result canary.

**Honesty check result**
SSE event display label for `canary_outcome`. Whether the honesty
check caught any false accepts that evening.

**Kept**
The gate decision when a candidate beat the baseline by enough.
Renders as a status badge. Developer value:
`gate_verdict = "passed"`.

**Kind** (in memory context)
The type of memory item — Observation or Pattern. CLI flag: `--kind`.
Developer term: tier.

**Lineage**
The genealogy of related strategy versions. Same word developer-side.

**Lineage proof**
The cryptographic fingerprint covering a lineage's track record,
used for auditable provenance. Developer term: counterfactual-chain
Merkle root.

**Minimum improvement**
The smallest Sharpe gain that the gate counts as real. CLI flag:
`--min-improvement`. Developer terms: epsilon, ε.

**Namespace**
Memory scope — either `global` or `agent:<id>`. Same word
developer-side (this term was reviewed and kept; namespaces don't
get renamed to "scope").

**New branch added**
SSE event display label for `lineage_forked`. A new child has been
added to the lineage tree.

**Observation**
An auto-captured record of one cycle's decision and outcome. Same
word developer-side.

**Off** (memory mode)
This agent doesn't read or write memory. Same value developer-side:
`memory_mode = "Off"`.

**Operator sign-off**
Explicit operator approval of something the system won't allow by
default — e.g., a Pattern with no training cutoff. Developer term:
operator attestation.

**Parent agent**
The agent version a training run was targeting. Developer field:
`target_agent_id`.

**Pattern**
A distilled or operator-attested insight that's recalled at
decision time. Same word developer-side.

**Proposer scoreboard**
Per-cycle metrics on the experiment writer itself — how often its
experiments get Kept, how its predictions compare to outcomes.
Developer term: Mutator-skill ladder.

**Proposer scoreboard updated**
SSE event display label for `ladder_snapshot`. The experiment
writer's metrics were just refreshed.

**Rejected**
A lineage node that didn't pass the gate. Renders as a status badge
in the genealogy view. Developer value: `LineageStatus = "Ghost"`.

**Retire** (verb), **Retired**
Soft-delete a Pattern. CLI verb: `xvn memory retire`,
`xvn autooptimizer retire`. Developer term: demote.

**Reverse-mutation check**
A sanity check: take a candidate that passed the gate, reverse the
mutation, paper-test it. If the reverse scores the same, the
original might have been noise. Developer term: inversion-pair eval.

**Reviewer finished notes**
SSE event display label for `judge_wrote_finding`. The LLM
reviewer just finished writing the qualitative finding for a kept
experiment.

**Session**
A multi-cycle operator-bounded run. Same word developer-side.

**Session ground rules**
The operator-signed pre-commitment of cycle config, minimum
improvement, untouched test period, and signing seeds. Developer
term: SessionCommitment.

**Shared across all agents** (memory mode)
The memory mode value where this agent reads and writes the global
namespace. Developer enum value: `memory_mode = "Global"`.

**Staged**
A Pattern that's been distilled and is awaiting the gate; the gate
decides whether to activate it. Renders as a status badge.
Developer value: `promotion_state = "staged"`.

**Status** (memory context)
The Pattern's lifecycle state — Active / Staged / Forgotten. CLI
flag: `--status`. Developer field: `promotion_state`.

**Strategy fingerprint**
A short identifier (8 hex chars displayed, full 64 chars copy-on-click)
that uniquely identifies a strategy version. Developer term:
`bundle_hash` (BLAKE3 over canonical-JSON-serialized bundle).

**Suspect**
A lineage node that passed the gate but was flagged by the
reverse-mutation check. Renders as a status badge in the genealogy
view. Developer value: `LineageStatus = "Quarantined"`.

**Test embedding vector (advanced)**
The raw JSON embedding input, hidden behind an "Advanced" disclosure.
Used only for deterministic tests. Developer flag: `--embedding-json`.

**Testing experiment**
SSE event display label for `mutation_evaluating`. A candidate is
being paper-tested right now.

**This agent only** (memory mode)
The memory mode value where this agent has its own private memory
pool. Developer enum value: `memory_mode = "AgentScoped"`.

**Today's score**
The score on today's market data (the day window). Used as one of
two windows that the gate evaluates. Developer field: `delta_day`
for improvements, `parent_day_score`/`child_day_score` for raw
scores.

**Train new version**
The action that creates a new child agent version via training.
Developer label: "Mint Child."

**Training data ends**
The cutoff date for a Pattern's training data. Patterns are only
recalled in scenarios that start strictly after this date.
Developer field: `training_window_end`.

**Training / Validation / Untouched test** (the three splits)
The standard split of agent training data. Developer terms:
train / dev / holdout.

**Training run** / **Training run history**
A run of the optimizer that produces a new child agent version.
Developer term: optimization / `optimization_id`.

**Untouched test period**
A window of market history the strategy never trained on, used to
evaluate generalization. Developer term: holdout window.

**Variety score**
Per-cycle measure of how varied the experiment proposals are. A
falling variety score means mode collapse. Developer term:
diversity-decay rate.

**Variety score updated**
SSE event display label for `diversity_updated`.

---

## Part 3 — Buttons (every action surface)

### Memory page — Patterns tab

- **+ Add Pattern** — primary action button at top-right of the
  Patterns list. Opens the Add Pattern modal. (unchanged from current)
- **Activate** — per-Pattern row action for staged Patterns. Action:
  flip the Pattern's status to active.
- **Retire** — per-Pattern row action for active or staged Patterns.
  Action: soft-delete the Pattern (forgotten with grace window).

### Add Pattern modal

- **Cancel** — secondary action; closes the modal.
- **Add Pattern** — primary action; submits the form. Pending state:
  **Saving…**

### Memory page — Observations tab

No action buttons on this tab — Observations are read-only.

### Forget dialog (workspace mode)

- **Cancel** — secondary; closes the dialog without acting.
- **Confirm forget** — primary; soft-deletes every memory item in
  the namespace. Pending state: **Forgetting…**

### Memory tab on agent page

Same buttons as the Memory page, plus the agent-only optimization
form below.

### Optimization form (agent page, "Train new version" panel)

- **Train new version** — primary action. Action: spawn a new child
  agent version with the configured training run. Pending state:
  **Training…**

### AutoOptimizer runs panel (Latest / History)

- **Activate** — per-run row action for runs whose Pattern is in
  status Staged after the gate has been recorded as Kept. Action:
  flip the Pattern to active.
- **Retire** — per-run row action. Action: soft-delete the Pattern.

### AutoOptimizer gate form (per-run, when the gate hasn't been recorded yet)

- **Record gate decision** — primary action. Action: submit the
  numeric scores and finding, record the gate verdict. Pending
  state: **Recording…**

### Optimization gate form (per-training-run, when the gate hasn't been recorded)

- **Record gate decision for {short-id}** — primary action. Action:
  submit numeric scores, record the gate verdict for this training
  run. Pending state: **Recording…**

### Flywheel panel (no buttons currently — view-only)

### Live cycle viewer (future, AR-3)

When the live cycle view ships, no buttons by default — it's a
view-only stream. Future buttons (if added): **Pause stream** /
**Resume stream**, **Clear log**.

---

## Part 4 — Status badges

Every badge value with the literal display string and the developer
value behind it. Use sentence case for badge display.

### Pattern lifecycle badges

| Display | Developer value | When it appears |
|---|---|---|
| Active | `promotion_state = "active"` | Pattern is being recalled at decision time |
| Staged | `promotion_state = "staged"` | Pattern is awaiting the gate |
| Forgotten | `promotion_state = "forgotten"` (or `forgotten_at` set) | Pattern was retired; inside grace window |

### Gate decision badges

| Display | Developer value | When it appears |
|---|---|---|
| Kept | `gate_verdict = "passed"` | Gate accepted the candidate |
| Dropped | `gate_verdict = "failed"` | Gate rejected the candidate |
| (no badge) | `gate_verdict IS NULL` | Gate hasn't been recorded yet |

### AutoOptimizer run status badges

| Display | Developer value | When it appears |
|---|---|---|
| Staged | `promotion_state = "staged"` | Run produced a Pattern, awaiting gate |
| Active | `promotion_state = "promoted"` | Pattern is active in recall |
| Retired | `promotion_state = "demoted"` | Pattern was retired |

Note the dev-value `promoted` renders as Active in the badge (matches
the Pattern lifecycle vocabulary) and `demoted` renders as Retired.
This is intentional consolidation.

### Lineage node status badges (future, AR-2 / AR-3 dashboard genealogy view)

| Display | Developer value | When it appears |
|---|---|---|
| Active | `LineageStatus::Active` | Passed gate, in the keeper tree |
| Rejected | `LineageStatus::Ghost` | Failed gate; kept in genealogy for audit |
| Suspect | `LineageStatus::Quarantined` | Passed gate but flagged by reverse-mutation check |

### Optimization status badges (Training run history)

The training run's `status` field renders verbatim (e.g., "completed",
"in_progress", "failed") — these are operational, not lifecycle.
Sentence-case them when displaying.

---

## Part 5 — Form labels with helper text

### Add Pattern modal

| Field | Label | Helper text | Required? |
|---|---|---|---|
| text | Text | (placeholder: "e.g. Mean-revert entries fail on FOMC days; sit out announcement bars.") | yes |
| training_end | Training data ends | "Optional. The date your training data ends (end-of-day UTC at submit). Scenarios starting *after* this date will recall this Pattern; scenarios overlapping or earlier exclude it (look-ahead protection). Blank dates require operator sign-off and recall in *every* scenario." | no |
| confirm-no-cutoff checkbox | I confirm this Pattern has no training cutoff and may be recalled in every scenario. | (shown only when Training data ends is empty) | yes when no cutoff |
| operator_initials | Operator initials | (shown only when the confirm checkbox is checked) | yes when no cutoff |
| namespace | Namespace | (no helper; dropdown in agent mode, static `global` in workspace mode) | yes (defaulted) |

Modal-level alert (when no embedder is configured):
- Heading: **Requires an embedding provider.**
- Body: "Patterns are matched to decision context via vector
  similarity, so an agent's provider (or a configured default) must
  support embeddings. Without one, this Pattern is stored but never
  recalled — check Settings → Providers, or watch eval-review for a
  `memory_disabled_no_embedder` event after the next run."

### Patterns tab filter controls

| Control | Label | Options |
|---|---|---|
| namespace dropdown (agent mode) | Namespace | `agent:{id}`, `global` |
| namespace static (workspace mode) | Namespace | `global` (shown as code block) |
| status dropdown | Status | all live, active, staged, forgotten |

### Observations tab filter controls

| Control | Label | Placeholder |
|---|---|---|
| scenario filter | Scenario id | filter by scenario |
| run filter | Run id | filter by run |

Info banner at the top of the Observations tab:
"Observations are read-only. Use 'Forget all memory' to clear."

### AutoOptimizer gate form (per-run)

| Field | Label |
|---|---|
| parent_day_score | Baseline today's score |
| child_day_score | Candidate today's score |
| parent_holdout_score | Baseline untouched-period score |
| child_holdout_score | Candidate untouched-period score |
| gate_epsilon | Minimum improvement (Sharpe) |

Helper text near the form: "Minimum improvement is the smallest
Sharpe gain that counts as real. The gate returns Kept only if the
candidate beats the baseline by at least this much on both windows."

### Optimization gate form (per-training-run)

| Field | Label |
|---|---|
| parent_dev_score | Baseline validation score |
| child_dev_score | Candidate validation score |
| parent_holdout_score | Baseline untouched-period score |
| child_holdout_score | Candidate untouched-period score |
| gate_epsilon | Minimum improvement |

### Train new version form (agent page)

| Field | Label | Options / notes |
|---|---|---|
| child agent name | Child agent name | text input |
| demo source | Example source | Use saved examples / Capture new examples |
| split | Training / Validation / Untouched test split | text input (default `70/15/15`) — helper: "Three percentages summing to 100. Training examples shape the child agent; validation tunes; untouched test verifies generalization." |
| use priors checkbox | Include patterns I've already learned | (no helper needed) |

### Forget dialog

Title (agent mode): **Forget all memory for this agent?**
Title (workspace mode): **Forget all global memory?**
Body: "This will soft-delete {n} memory item(s) from namespace
`{namespace}`. Observations and Patterns alike. Items can be
restored during the configured grace window."

### Memory Mode settings (future surface)

When/if the per-agent memory mode is exposed in settings UI:

| Value | Display label | Helper text |
|---|---|---|
| Off | Off | This agent doesn't read or write memory. |
| Global | Shared across all agents | This agent reads and writes the shared global memory. |
| AgentScoped | This agent only | This agent has its own private memory pool. |

---

## Part 6 — Metric labels (Flywheel panel)

### Status section (cumulative totals)

| Display | Developer field |
|---|---|
| Observations | `observations` |
| Active | `active_patterns` |
| Staged | `staged_patterns` |
| Forgotten | `forgotten_patterns` |
| Runs | `autooptimizer_runs` |

### Velocity section (7-day windowed counts)

| Display | Developer field |
|---|---|
| Obs / 7d | `observations_captured` |
| Activated / 7d | `patterns_promoted` (yes — API field stays `promoted`; display label is "Activated") |
| Retired / 7d | `patterns_demoted` |
| New versions / 7d | `optimized_child_agents` |
| Generations deep | `average_lineage_depth` (rendered to 2 decimals) |

### Latest training run row (per row in "Latest Lineage" / "Training run history")

The row format:

```
{short training-run id} · parent {short parent-agent id} · child {short child-agent id or "none"}
training {N} / validation {N} / untouched test {N} · examples {N} · background patterns {N} · {status}
fingerprints: untouched {short hash} · training {short hash} · validation {short hash}
gate decision: {Kept|Dropped} · validation improvement: {value} · untouched improvement: {value}
{gate reason or finding text, if present}
```

All ID and hash short-forms use the `<ShortHash>` component (Part 9
below).

### AutoOptimizer run row (per run in "Recent AutoOptimizer Runs")

The row format:

```
{Pattern text — wrapped, primary visual emphasis}
{short run id} · {N} observations · {Status badge}
Gate decision: {Kept|Dropped} · {gate metric} · today {value} · untouched {value} · {gate reason or finding text}
```

---

## Part 7 — Empty / loading / error states

### Patterns tab

- Loading: "Loading patterns…"
- Error: "Couldn't load patterns: {message}"
- Empty (no filter): "No patterns yet for {namespace}. Use '+ Add Pattern' to seed one."
- Empty (with status filter): "No {filter-value} patterns yet for {namespace}. Use '+ Add Pattern' to seed one."

### Observations tab

- Loading: "Loading observations…"
- Error: "Couldn't load observations: {message}"
- Empty (agent mode): "No observations yet for this agent."
- Empty (workspace mode): "No observations yet for the global namespace."

### Flywheel panel

- Error: "Couldn't load flywheel status: {message}"
- (No explicit empty state — metrics render as 0)

### AutoOptimizer runs panel

- Loading: "Loading runs…"
- Error: "Couldn't load runs: {message}"
- Empty: "No autooptimizer runs yet."

### Training run history

- (Empty state: section hides if list is empty in "Latest" mode;
  in "Full history" mode, render: "No training runs yet for {namespace}.")

---

## Part 8 — Tab and section headers

| Surface | Header | Notes |
|---|---|---|
| Memory page topbar title | Memory | |
| Memory page topbar sub | Global namespace · Operator patterns and observations from the evening run | |
| Memory page primary card title | Memory | |
| Patterns/Observations tab labels | Patterns / Observations | |
| Flywheel card title | Flywheel | |
| Latest training run section (compact mode) | Latest training run | |
| Training run history section (full mode) | Training run history | |
| Recent autooptimizer runs section (compact mode) | Recent autooptimizer runs | |
| AutoOptimizer history section (full mode) | AutoOptimizer history | |
| Train new version panel (agent mode only) | Train new version | |
| Agent page Memory tab title | Memory | |
| Agent flywheel route topbar title | Flywheel | |
| Agent flywheel route topbar sub | {agent id} | |
| Agent flywheel back link | Back to agent | |

---

## Part 9 — The `<ShortHash>` component

Anywhere the UI would render a raw 26-character ULID or 64-character
hex string, use this component instead.

### Props

```ts
type ShortHashProps = {
  value: string | null | undefined;
  length?: number;    // chars to show. Default: 8 for hashes, 6 for ULIDs
  label?: string;     // optional prefix, e.g. "Strategy" → "Strategy abc12345"
  fallback?: string;  // for null/undefined. Default "—"
  variant?: "mono" | "inline";  // default "mono"
};
```

### Behavior

- Renders `{label} {value.slice(0, length)}…` in font-mono by
  default.
- Click → copies full value to clipboard, shows 1-second inline
  checkmark or a toast "Copied".
- Hover → tooltip with the full value.
- Null/undefined → renders fallback (not clickable).

### Where to use

Use `<ShortHash>` everywhere these IDs appear:

| Context | Suggested `label` | Suggested `length` |
|---|---|---|
| `bundle_hash` (64-hex) | "Strategy" | 8 |
| `optimization_id` (ULID) | "Training run" | 6 |
| `session_id` (ULID) | "Session" | 6 |
| `cycle_id` (ULID) | "Cycle" | 6 |
| `run_id` (ULID, autooptimizer run) | "Run" | 6 |
| `target_agent_id` / `child_agent_id` (ULIDs) | "Agent" | 6 |
| `train_hash` / `dev_hash` / `holdout_hash` (64-hex) | "training" / "validation" / "untouched" | 8 |

---

## Part 10 — SSE event display labels (live cycle viewer)

When the live cycle view ships (AR-3), every event in the stream is
rendered using these labels. The wire event name is the developer
identifier; the display label is what the operator reads.

| Wire name | Display label |
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

Use the shared `displayLabel(event)` helper (Rust: `display_label`
in `crates/xvision-dashboard/src/sse.rs`; JS:
`crates/xvision-dashboard/static/js/bus.js`).

---

## Part 11 — Tone and voice

The operator-surface voice is a notch warmer than the engineering
spec but not casual. Some guidance:

- **Direct, not chatty.** "No patterns yet for {namespace}." not
  "Looks like there aren't any patterns here yet!"
- **Action-oriented buttons.** "Activate", "Retire", "Add Pattern",
  "Train new version" — verbs that name the action precisely.
- **Don't editorialize errors.** "Couldn't load patterns: {message}"
  not "Oops! Something went wrong while loading patterns." Show the
  reason if the API provides one; don't pad.
- **Honest about defaults.** "Optional. The date your training data
  ends." not "Leave blank if you don't know."
- **No exclamation marks** except in the most positive
  acknowledgements (e.g. a copy-to-clipboard "Copied!" is fine; an
  empty state isn't the place).
- **Sentence case for badges, buttons, and headers.** Title Case
  only for proper nouns and the topbar's main title.
- **Consistent verb pairs.** Activate ↔ Retire. Add ↔ Remove.
  Capture ↔ Use saved. Don't introduce a third synonym for any of
  these.
- **No emoji.** This was already a project convention, restating
  here for completeness.

---

## Part 12 — Banned terms (the QA pass)

If any of these appear on a user-visible screen, the patch isn't
done. Use a banned-words grep on the rendered DOM (not the source)
as the final check.

- `epsilon`, `ε`
- `holdout` (the word; the phrase "untouched test" is the
  replacement — but "holdout" alone is banned)
- `mutation`, `mutator`, `mutator-skill`
- `merkle`, `BLAKE3`, `Ed25519`, `canonical JSON`
- `ghost`, `quarantined` (as status names — the words elsewhere are
  fine)
- `promote`, `promotion`, `promoted` (in autooptimizer and memory
  contexts — fine elsewhere)
- `demote`, `demotion`, `demoted`
- `mint` (in the "create a child agent" sense — fine in marketplace
  context)
- `demos` (in operator copy — fine in code variable names)
- `priors` (in operator copy — same)
- `tier` (as a UI flag/field name — fine in spec/code)
- `promotion_state` (as a UI flag/field name — fine in API)
- `lineage depth` (the phrase — the metric is now "Generations deep")
- `null-result canary`, `canary` (in autooptimizer context — fine in
  CI/Kubernetes contexts elsewhere)
- `inversion-pair`
- `diversity-decay`
- `case-law framing` (this is a prompt-engineering design term;
  never surfaces to operator)
- `F+L+T` (same)
- `attestation`, `attest-null-window` (in user copy — operator-facing
  is "operator sign-off", "no training cutoff")
- `bundle_hash` (in user-visible copy — should always be a
  `<ShortHash>` with label "Strategy")
- `cycle_seal`, `session_commitment` (in user copy — should always
  be "Evening summary" and "Session ground rules")

---

## Part 13 — Reference

- **Canonical terminology lock** (source of truth):
  `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`
- **Audit and rationale** (the why behind each rename):
  `docs/superpowers/notes/2026-05-27-autooptimizer-plain-language-audit.md`
- **Frontend rename handoff** (file-by-file PR-shaped work):
  `docs/design/2026-05-27-autooptimizer-frontend-rename-handoff.md`
- **CLI rename handoff** (the CLI surface contract):
  `docs/design/2026-05-27-autooptimizer-cli-rename-handoff.md`
- **SSE display-label registry handoff** (live cycle viewer):
  `docs/design/2026-05-27-autooptimizer-sse-registry-handoff.md`
- **Skills + docs sweep handoff** (the documentation pass):
  `docs/design/2026-05-27-autooptimizer-skills-docs-sweep-handoff.md`
- **Spec amendment handoff** (the spec footnotes):
  `docs/design/2026-05-27-autooptimizer-spec-amendment-handoff.md`
- **Wave intake** (rollout sequencing):
  `team/intake/2026-05-27-autooptimizer-terminology-rollout.md`
- **Project-wide terminology convention**: `/CLAUDE.md` §Terminology
- **Frontend design conventions** (no popups, no right-side boxes):
  `frontend/DESIGN.md`
- **Mobile conventions**: `frontend/MOBILE.md`
