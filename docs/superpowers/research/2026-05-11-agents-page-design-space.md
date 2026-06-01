# Agents page — design-space exploration

> **What this is:** Research artifact preserving the design-space expansion
> for the xvision "Agents" page, conducted 2026-05-11 via `ideonomy-plain`.
> Used as input to `docs/superpowers/plans/2026-05-11-agents-page-v1.md`,
> which distills the v1 minimum from this surface.

**Method tuple drawn:** operators = (organon-construction, negation);
organon = state-machine; dimension prompts = (cyclicity, predictability,
modularity).

**Originating question:** what belongs on the Agents page beyond the
confirmed primitives (load skills, select providers/models, enter system
prompts)?

**Originating constraints from the operator (2026-05-11):**

- Users can add their own slots, name their own slots — no enforced slot
  vocabulary. Templates exist as examples, not constraints.
- Most agents will be single-agent. Multi-slot composition is opt-in,
  additive, not the default.
- Temperature is **not** on v1 — surfaces too much knobbiness, too confusing
  before the user has shipped one agent.

---

## Opposites of "an agent page"

Negation pass: definitional properties of "an agent page" → what each
negation produces. Each row is a feature direction the page can grow into.

```
| Property negated                            | Resulting feature direction                                            |
|---------------------------------------------|------------------------------------------------------------------------|
| Shows reusable templates                    | Also shows DEPLOYED INSTANCES of those templates                       |
| One-at-a-time editing                       | Bulk operations (rename N, attach skill to N, run eval on N)           |
| Edit-only                                   | Run / test / replay / debug surfaces inline                            |
| Visual / form-based                         | Also a markdown view (program.md-shaped, Karpathy unit)                |
| Operator creates and edits                  | AutoOptimizer creates and edits (agent-authored agents)               |
| Single-user view                            | Comparison across users / leaderboard (marketplace tie-in)             |
| Agents are static config                    | Agents have history, versions, lineage, attestations                   |
| The user chooses everything                 | Defaults seeded from a baseline; user overrides what matters           |
| Configures a single agent                   | Composes N agents into a strategy graph                                |
```

Several map to existing items in `FOLLOWUPS.md`:

- "agent-authored agents" → SLF9 (evening Karpathy loop)
- "lineage" → SLF8 (`program.md` versioning + parent hash on chain)
- "leaderboard across users" → F34 (ERC-8004 reputation leaderboards)
- "agents have history" → existing eval-runs surface

---

## Agent state machine

The page is fundamentally a state-machine viewer + editor. An agent
occupies one state at a time; available actions on the page are gated by
which state it's in.

States:

- **Drafting** — operator composing slots / prompts / skills; not runnable
- **Validated** — passes validators (token budget, fixtures, risk envelope coherent); ready to test
- **Testing** — running against fixtures or backtest data; receiving findings
- **Sealed** — config locked, attestation written, ERC-8004 NFT minted; the parent record for any mutations
- **Forward-paper** — running against paper broker (Alpaca paper for spot, Orderly testnet for perps); real fills, simulated money
- **Live** — running against live capital; subject to kill switch + budget caps
- **Halted** — kill switch fired / risk-veto streak / operator paused; positions either flat or frozen
- **Retired** — taken out of rotation; archived; queryable, not runnable
- **Mutated** — variant proposed by autooptimizer; parent stays, child enters Drafting

Transitions:

```
Drafting       -> Validated     : passes engine::api::strategy::validate()
Validated      -> Drafting      : operator edits a slot
Validated      -> Testing       : operator hits "Run eval"
Testing        -> Validated     : eval completes (any outcome)
Validated      -> Sealed        : operator hits "Seal" AND eval-attestation present
Sealed         -> Forward-paper : operator hits "Forward-paper" with broker chosen
Forward-paper  -> Live          : operator funds account + flips to live
Live           -> Halted        : kill switch / risk-veto streak / budget breach
Halted         -> Live          : operator hits "Unhalt" after diagnosis
Halted         -> Retired       : operator decides to archive
Live           -> Retired       : operator de-provisions
Forward-paper  -> Retired       : operator de-provisions
Sealed         -> Mutated*      : autooptimizer proposes child (parent unchanged; * is the child)
Any state      -> Drafting      : "Fork to draft" — creates new agent_id, preserves lineage
```

Forbidden transitions (must go through an intermediate state):

- Drafting → Sealed (must validate first)
- Drafting → Live (must validate → test → seal → paper → live)
- Sealed → Drafting (sealed is immutable; "edit" = "fork to a new draft")
- Live → Validated (live must halt first; no silent re-entry into testing)

Absorbing-ish states:

- **Sealed**: config is immutable. Agent itself can move on (paper, live), but the configuration won't change. Mutations spawn new children.
- **Retired**: terminal for this `agent_id`. Queryable; not runnable.

What the state machine surfaces for UX:

- Current state must be prominent at top of page (banner-sized).
- Action buttons are state-gated.
- Halted state needs its own restore flow with a diagnostic step.
- Lineage view (Mutated edges form a tree) is its own visual.
- Forking from Sealed is the canonical "edit" path — page should make this obvious.

---

## Features by dimension

### Cyclicity — what to surface about agent rhythms

Cyclicity: is the agent one-shot or rhythmic? Both — design-time is
one-shot-per-version (with lineage), run-time is periodic.

Run-time cadence surfaces:

- Next-run countdown (`Next cycle in 04:23`)
- Last-run summary (cycle_id, verdict, P&L delta, link to trace)
- Cycle-frequency knob (1m / 5m / 1h / daily / event-triggered)
- Pause / resume toggle independent of Halted state
- One-shot trigger ("Run one cycle now against current market")
- Cycle history strip: last N cycle outcomes as colored markers
- Background mutation status ("AutoOptimizer last visited: 4h ago; 2 candidates pending review")

Design-time lineage surfaces:

- Parent agent_id (link)
- Children: operator forks + autooptimizer mutations
- Diff against parent — what changed between this version and forebear
- Sealed-attestation hash + chain-explorer link

### Predictability — determinism controls

Predictability: how deterministic vs stochastic is behavior? LLM-driven
agents are highly stochastic; xvision has knobs to constrain.

Per-slot determinism controls:

- Temperature (0.0 – 1.0)
- top_p
- Random seed (with "lock seed" affordance for debugging)
- Decoding strategy (greedy / sampled / beam if provider supports)

**v1 explicitly defers all of these.** Operator decision 2026-05-11: too
confusing before the user has shipped one agent. Re-introduce post-v1 as
an "Advanced" section.

Variance surfacing (also deferred):

- "Variance scan" — run slot against N fixtures with M seeds; show output spread
- Confidence calibration display
- Adversarial probe — run slot against contrarian fixtures, show flip rate

Reproducibility hooks:

- "Replay" — re-run a past cycle with same seed + briefing, verify same output (model-drift detector)

### Modularity — composition view

Modularity: how recombinable are the parts? Maximum modularity is the
design goal per the operator's framing ("agent archetype or saved profile").

Slot composition view (**revised post-operator-clarification**):

- An agent has zero or more **named slots**. Slot names are user-defined
  free text — "intern", "trader", "risk", "executor" are example
  conventions, not required.
- Each slot binds: skill set, provider, model, system prompt, tools allowed, max tokens.
- Default agent is single-slot (one prompt, one model).
- "Add slot" is an explicit affordance — opt-in, not default.

Reusable-template features:

- "Promote to template" — save an agent's slot loadout as a reusable template
- Template gallery — browse templates by intended use (market-maker / mean-rev / momentum / arb)
- Template includes: slot loadout, default risk envelope, recommended providers (operator can override)
- Diff-from-template — see how a deployed agent has drifted from its template
- Re-base — move an agent's "parent template" pointer to a newer template version (with diff preview)

Skill loadout:

- Skills are pluggable modules per slot
- Per-slot: enabled/disabled, configured parameters, version pin
- Skill marketplace integration — discover skills from other operators
- Custom skills — upload / link to local skill directory

Composition into strategies:

- Strategy is the noun: bundles N agents into a pipeline
- "Strategy" page is downstream of the Agents page — references agents by `agent_id`
- One agent can appear in many strategies (its config is the immutable template; the strategy parameterizes which capital / instruments / cadence)

---

## Synthesis — full feature checklist

Flat enumeration, ranked by build-order leverage. Items marked `*` touch
concepts that don't fully exist yet (autooptimizer loop, ERC-8004
attestation, skill marketplace, leaderboard).

**Tier 1 — page is incomplete without these:**

- Current state banner with state-gated action buttons
- Slot-loadout panel (per slot: skill, provider, model, system prompt, tools, max tokens)
- "Promote to template" + template gallery
- "Fork to draft" (canonical edit path from a sealed agent)
- Lineage view: parent + children, with diff
- Last-run summary + cycle history strip
- Per-slot live preview (run slot on fixture, see output)
- Validation rail: warnings, token estimate, missing fixtures, risk-envelope coherence
- Risk envelope as a separate first-class section (caps, leverage, drawdown stops)
- Run eval / Run one cycle now

**Tier 2 — visible asymmetry without these:**

- Variance scan
- Confidence calibration display
- Diff-from-template (drift indicator)
- Background mutation status from autooptimizer *
- Cycle-frequency knob with event-triggered option
- Pause / resume toggle independent of Halted
- Adversarial probe
- Replay (deterministic re-run)

**Tier 3 — depend on adjacent surfaces:**

- Sealed-attestation hash + ERC-8004 chain link *
- Re-base (move template pointer)
- Skill marketplace integration *
- Custom-skill upload / link
- Leaderboard / cross-user comparison *
- `program.md` markdown view (Karpathy unit) — pairs with autooptimizer
- Bulk operations across N agents

**Tier 4 — unexpected directions worth a separate think:**

- Agent-authored agents: page becomes a *review surface* for autooptimizer mutations as much as an authoring surface
- Live-vs-paper twin view: same agent_id running in two surfaces simultaneously, surfacing divergence in real time
- "Adversarial agent" slot: a separate agent whose job is to find weaknesses in another agent's decisions
- Plain-language summary: every agent has a generated 100-word description for sharing / marketplace / quick-recall
- Time-budget guard: per-cycle wall-clock cap; fall back to deterministic baseline if exceeded

---

## What was learned

The state-machine framing made the **Sealed state** load-bearing — it's
the substrate that gets attested, mutated, papered, and lived. Most
CRUD-shaped agent pages hide this because they assume "edit in place" as
the default. Here, edit-in-place is forbidden once an agent matters.

The negation pass surfaced a structural ambiguity in the "agent" noun:

- "Agent template" — immutable configuration; reusable design artifact
- "Agent instance" — a deployed running entity with capital and history

The two need different views and different action sets. Operator
clarification (post-brainstorm): slots are user-named, no enforced
vocabulary, single-agent is the default. This collapses the "template"
view further — most templates will be a single-slot agent definition with
no exotic composition.

**Downstream domain-model impact** (flagged by the operator):

- The current `StrategyBundle` has fixed slot names (intern / trader /
  risk / executor) per `CLAUDE.md` terminology. Under the new model, a
  strategy is a composition of N agents (each a self-contained
  prompt+model+skills unit) rather than a fixed-slot pipeline.
- Eval semantics need re-anchoring: a backtest currently runs a
  `StrategyBundle`; it will now run a strategy that references agents.
  Per-agent metrics become a thing alongside per-cycle and per-strategy
  metrics.
- The pipeline-stage names (intern, trader, risk, executor) remain valid
  conventions for users to adopt, but are no longer enforced.

These changes are tracked in the v1 plan (`docs/superpowers/plans/2026-05-11-agents-page-v1.md`)
under "Downstream impact" and intentionally NOT executed as part of v1
— v1 is the Agents page as a thin standalone surface, with the strategy
+ eval refactor sequenced after.

---

## Not surfaced, worth a follow-up tuple

Three dimensions the picker didn't draw that probably matter:

- **Cost/economics**: per-cycle LLM spend, per-cycle gas spend, breakeven analysis, cost-per-decision ratio. Operator-side concern; could become a "Cost" panel.
- **Regulatory**: which jurisdictions can run this agent; attestation implications; KYC/AML constraints if the agent represents a fund-like vehicle.
- **Time-horizon**: intraday vs swing vs position trading — cadence touches this but doesn't go deep on horizon-as-identity. A horizon dim would surface different agent shapes (high-frequency micro-agents vs slow-think position agents).

Worth a second ideonomy pass with `{economics, regulatoriness, time-horizon}` if the v1 reveals these become load-bearing.

---

## Method-tuple metadata

For reproducibility / audit of the brainstorm:

- Picker invoked at: ~2026-05-11
- Operators: `organon-construction`, `negation`
- Organon: `state-machine`
- Dimension prompts: `cyclicity`, `predictability`, `modularity`
- Picker source: `~/.claude/skills/ideonomy-plain/bin/pick`
