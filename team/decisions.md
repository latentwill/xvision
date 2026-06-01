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
- **Cross-symbol Pattern recall policy** (added 2026-05-21, folded in
  from the V2D memory triage). The cortex-memory plan defaults to
  refusing cross-symbol Pattern recall — a Pattern learned while
  trading BTC stays on BTC. Multi-asset enablement forces the question:
  do we keep that conservative default, or expose a per-slot
  "cross-symbol blend" toggle? Default position is *keep conservative*
  until an operator can articulate a concrete pair where blending
  helps. Track decisions made on this sub-question here, not in V2D
  intake.

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

### D5 — Memory scope: items the platform will not build

**Raised:** 2026-05-21 (operator triage of the V2D deferred list).
**Status:** **resolved at intake** — recorded here so the items stop
resurfacing in design discussions.

The V2D intake at `team/intake/2026-05-21-v2d-agent-memory.md` lists
several "Out of this intake" items as *deferred to v1.1* (build later if
operator data asks for them). Operator pass 2026-05-21 hardened a
subset of those from *deferred* to *not building, ever, absent a
specific complaint*. The rule of restraint: the absence of these
features is enforcing useful constraints; building them invites scope
creep and confused product framing.

**Will not build (default-no):**

- **Cross-namespace recall blending** (a slot in `agent_scoped` mode
  also surfacing `global` matches). The V2D intake parks this as "v1.1
  if operators ask." Operator position: scoping by `(agent, slot)` is a
  *feature*, not a limitation. Blending leaks pattern signal across
  strategies that should not share it (the cross-symbol case in D2 is
  the same shape). No track, no follow-up — if a future operator wants
  this, they reopen here.
- **Embedder configuration UI** (a UI surface dedicated to embedder
  selection independent of the slot's provider/model). The current
  implicit shape — embedder follows the slot's `provider` + `model`
  with a single `default_embedder` fallback in `memory.toml` — is
  defensible. No UI track until a real complaint.
- **Memory diff CLI** (`xvn memory diff --before <date> --after
  <date>`). Build on demand if QA hits a case that needs it; trivial
  when needed, premature otherwise.
- **mem0 / Honcho / mempalace adapters.** The store API is intentionally
  narrow (open / upsert / query / forget) so a future adapter could
  wrap any of these — but the operator's stated direction is Rust-native
  in-process. Wrapping a third-party memory backend would dilute that
  contract without solving a real workflow gap.
- **`cortex-http` sidecar** + **cross-host memory sharing.** V2D intake
  defers both behind F28 plugin architecture. Hardened here: the
  in-process crate is the contract; revisit only if a multi-deployment
  customer materialises (probably never for the operator workflow this
  platform targets).
- **Embedding model swap migration CLI.** V2D intake calls this a "v1.1
  chore." Hardened: build only when an operator actually swaps
  embedders. Until then, the absence of this tool is enforcing "pick
  one embedder, stick with it" — which is the right default.

**V3 candidates (deferred *to autooptimizer*, not killed):**

- **Tool-driven memory** (`memory_recall` / `memory_write` exposed as
  agent tools). V2D ships auto-recall + auto-write as Decision 5. Tool
  surface is the autooptimizer's natural consumer; let V3 shape the
  tool contract based on actual mutation-loop needs, not pre-build it
  now.
- **TTL / time decay / LRU eviction.** V2D ships operator-driven
  forget. V3 autooptimizer is when memory volume becomes
  load-bearing; that's where the janitor design earns its keep.

**Kept as small v1.2 tracks** (see
`team/intake/2026-05-21-memory-safety-and-observability.md`):

- Memory forget undo / soft-delete with grace period.
- Memory-aware findings in eval review.
- Per-decision memory provenance in the trace.

**Memory across strategy versions** is recorded as a resolved D-row in
this file (resolution: memory follows `agent_id`, not strategy hash —
matches V2D Decision 4). Republishing a strategy without changing its
agent keeps memory; republishing with a new agent does not.

**Discussion:** —
**Resolution:** resolved 2026-05-21. Items above will not appear as
follow-ups on `FOLLOWUPS.md` or in future intakes unless a specific
operator complaint reopens them via a new D-row.

## Resolved

### D6 — Memory across strategy versions

**Raised:** 2026-05-21 (operator triage).
**Question:** Does a republished strategy carry forward its
agent-scoped memory?

**Resolution:** Memory follows `agent_id`, not strategy hash. V2D
Decision 4 names `agent:<agent_id>` as the scope key. Republishing a
strategy without changing its agent keeps the memory bucket;
republishing with a new agent does not. This matches the locked V2D
slot-level toggle design. Revisit only if marketplace publishing
introduces a clean-slate guarantee that conflicts with carry-forward.

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
