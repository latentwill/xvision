# Decisions — operator-direction items

Items that need operator (Ed) direction before any code is written. The
conductor parks them here so they don't pollute `team/board.md` as
spurious ready contracts. Each entry should be moved off this file once
resolved — either into a new intake (if it becomes implementable work) or
into `decisions/` as an ADR (if it becomes a binding architectural
decision).

> This file is conductor-owned. Workers may add comments under an item's
> "Discussion" sub-heading; the resolution line is conductor-only.

## Open

### D1 — Multi-agent strategies: expansion model (ideonomy)

**Raised:** 2026-05-17 (operator walk-through).
**Question:** Strategies today carry `Vec<AgentRef>` with free-text role
labels (intern/trader/risk/executor as a *convention*). How do we want to
expand this beyond the current 3–4 slot mental model? Suggestion in the
walk-through: run an ideonomy pass to surface variant pipeline shapes
(reviewer ↔ debater ↔ adjudicator, multi-trader vote, regime gate +
specialist routers, etc.) and pick a small set to standardize as
templates.

**Needed from operator:**
- Should the expansion be driven by a fixed set of canonical templates
  (selected from ideonomy output) or stay fully freeform with templates
  as starting points?
- If templates: how many, and named how?
- Does this need to land before or after `agent-run-observability` Phase B?

**Discussion:** —
**Resolution:** unresolved.

### D2 — Multi-asset scenarios

**Raised:** 2026-05-17 (operator walk-through).
**Question:** Scenarios today appear to be single-symbol (one asset, one
date range, one bar granularity). Operator floated multi-asset scenarios
(basket evaluation, cross-asset correlation regimes, pairs).

**Needed from operator:**
- Concrete shape: does a "multi-asset scenario" mean N parallel runs
  joined into one result, or a single run that the strategy sees as a
  multi-symbol bar feed?
- Pricing data assumptions — do existing scenario datasets already
  include the secondary assets, or is a data-ingestion track needed
  first?
- Backwards-compat: does the current `Scenario` shape extend, or does
  this need a v2 type?

**Discussion:** —
**Resolution:** unresolved.

### D3 — How templates drive strategies

**Raised:** 2026-05-17 (operator walk-through).
**Question:** Templates (`crates/xvision-engine/src/strategies/templates.rs`,
the `xvn example` artifacts from `v2a-example-artifacts` #205) exist as
starter strategies, but the relationship between "template" and "live
strategy" is unclear in the UI. Open questions:

- Are templates immutable seed records you fork, or live records that
  user strategies stay derived from (and inherit updates)?
- Should the agents library expose templates as a separate concept from
  user-authored agents, or is everything one bag?
- How do template-driven defaults interact with the in-progress agent
  composition refactor (`agents-page-v1` + `strategies-refactor-agent-composition`)?

**Needed from operator:**
- Pick the fork vs. inherit model.
- Decide whether templates surface as a distinct UI tab or as a chooser
  inside the existing flows.

**Discussion:** —
**Resolution:** unresolved.

### D4 — "Agents having multiple agents" — design intent clarification

**Raised:** 2026-05-17 (operator walk-through).
**Question:** During the walk-through the operator noted "Agents can have
multiple agents?" on the Agents page when adding an agent, and was unsure
of the thinking behind multi-agent agents.

The terminology rename (locked 2026-05-10, see `/CLAUDE.md`) defines
`Agent` as a *reusable agent template* — a per-prompt+model+skills
record. A `Strategy` carries `Vec<AgentRef>` to those templates. The
Agents page (post-2026-05-12 refactor) edits Agent templates; it should
not be nesting agents inside agents.

**Likely cause:** UI is showing slot composition fields on the Agent
template form, or the Strategy detail's add-agent flow is leaking into
the Agents page. Need operator to point at the specific UI state so we
can diagnose.

**Needed from operator:**
- A screenshot or specific page state showing where "Agents can have
  multiple agents" appears.
- Confirmation of intent: should an Agent template ever contain
  sub-agents, or is this purely a UI labeling / leaked-composition bug?

**Discussion:** —
**Resolution:** unresolved.

## Resolved

(none yet)

## Lifecycle

- New items append under **Open** with a `Dn` numeric tag, raised date,
  question, what's needed from operator, and an empty discussion +
  resolution.
- When the operator answers, the conductor:
  - Writes the resolution line.
  - Moves the entry under **Resolved** with the resolution date.
  - If the resolution is implementable, files a new intake under
    `team/intake/<date>-<slug>.md` and decomposes.
  - If the resolution is architectural, writes an ADR in `decisions/`.
