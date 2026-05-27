# Frontend handoff — autoresearcher plain-language rename

> For: frontend designer / engineer picking up the autoresearcher UI rename
> Date: 2026-05-27
> Source of truth: `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
> Background (skim only if curious): `docs/superpowers/notes/2026-05-27-autoresearcher-plain-language-audit.md`

## TL;DR

We're renaming about 80 user-visible strings across the Memory page, the
Memory tab on the agent page, and the Flywheel surface. The goal is to
get research-paper and cryptography jargon (epsilon, holdout, merkle,
mutator, ghost, quarantined, etc.) off the user's screen and replace
them with plain English that matches how the operator actually thinks
about the work. No API contract changes, no routing changes, no schema
changes — display-string-only.

Done = every string in the tables below has been swapped, the new
`ShortHash` component is in place for the long ULIDs and hashes, the
operator can take the existing flows without encountering any of the
banned terms (full list in §"Out of scope" below), and screenshots
match the acceptance gallery (§"Acceptance criteria").

## Project context (skip if you've shipped to xvision before)

xvision is a trading platform with an "autoresearcher" — a nightly
loop that proposes tweaks to trading strategies, paper-tests them, and
keeps the survivors. The Memory page lets the operator inspect what
the loop has learned (Patterns) and observe what the loop has captured
(Observations). The Flywheel panel shows the loop's tempo over time.

The current UI labels grew out of the engineering spec, so they read
like the spec — "Mutation," "Mutator," "Quarantined," "Epsilon,"
"Parent Holdout," "Merkle root." We did an audit, the operator
approved a set of renames, and this handoff is the result. Same data
underneath; just the words the user reads.

## The two-surface rule (important context for every decision)

We split every concept into two names: a **developer-surface name**
(used in code, API fields, the spec) and an **operator-surface name**
(used in display strings). They are not the same word and that is
intentional. As a frontend engineer:

- API response field names DO NOT CHANGE. You'll still read
  `gate_verdict`, `delta_holdout`, `promotion_state`, `bundle_hash`,
  `optimization_id` from the API. The component layer maps those
  field names to the new display labels.
- TypeScript type definitions DO NOT CHANGE. Keep importing
  `AutoresearchRun`, `FlywheelLineageItem`, `FlywheelVelocity`, etc.
  as they are.
- Only the rendered string changes. Where the JSX today says
  `<span>Gate {run.gate_verdict}</span>` and `gate_verdict` is
  `"passed"`, render `<span>Decision: Kept</span>` instead.
- A small mapping helper (or a tiny constants module) is fine for
  the verdict mapping; don't put the strings inline.

## Files in scope

- `frontend/web/src/features/memory/MemorySurface.tsx` — the big one,
  ~1700 lines. Holds the Patterns tab, Observations tab, the Add
  Pattern modal, the Forget dialog, the Flywheel panel, the Latest
  Lineage and Autoresearch Runs sections, and the optimization form.
  Roughly 70 of the 80 string changes are here.
- `frontend/web/src/features/memory/MemoryPage.tsx` — small wrapper
  page. Topbar copy needs one update.
- `frontend/web/src/components/agent/MemoryTab.tsx` — thin wrapper
  over MemorySurface in `mode="agent"`. Should need zero changes
  itself (it inherits all strings from MemorySurface).
- `frontend/web/src/routes/agents-flywheel.tsx` — agent-scoped
  flywheel route. Topbar copy + back-link label only.

## Files explicitly NOT in scope

Leave these alone:

- `frontend/web/src/api/flywheel.ts`, `memory.ts`, anything under
  `src/api/` — API field names are developer surface.
- `src/api/types.gen/` — generated TypeScript types; regenerated from
  Rust.
- Any route path (don't rename `/memory`, `/agents/:id/flywheel`,
  etc.).
- The static SPA at `crates/xvision-dashboard/static/` — that's a
  separate vanilla-JS surface for the live cycle viewer; a different
  patch handles those labels.
- Any Rust file, any SQLite migration, any spec doc.

## Section 1 — Status badge values

The lowest-risk rename. Do this first to get a feel for the codebase.

| Where (file:approx line) | Today | Replace with |
|---|---|---|
| MemorySurface.tsx ~742, ~754 — autoresearch run badge | `staged`, `promoted`, `demoted` | `Staged`, `Active`, `Retired` *(sentence-case the badge and use "Active" instead of "promoted" so it matches the Patterns lifecycle vocabulary)* |
| MemorySurface.tsx ~754, ~462 — gate verdict | `passed` / `failed` | `Kept` / `Dropped` |
| MemorySurface.tsx ~1227 — Pattern lifecycle badge | `active` / `staged` / `forgotten` | `Active` / `Staged` / `Forgotten` *(sentence-case only; values already plain)* |
| Future: lineage node status (not in UI today but coming with AR-2) | `Active` / `Ghost` / `Quarantined` | `Active` / `Rejected` / `Suspect` |

Implementation pattern: a small `formatVerdict()` and
`formatPromotionState()` in `src/features/memory/labels.ts` (new
file). Components import and call those instead of rendering the raw
field. Keeps the mapping in one place when we add languages or
A/B test labels later.

## Section 2 — Form labels (the gate forms)

This is the most jargon-dense block. The two gate forms (autoresearch
run gate, and optimization gate) share the same field set.

| Where (MemorySurface.tsx line) | Today | Replace with |
|---|---|---|
| ~509, ~876 | `Parent Holdout` | `Baseline untouched-period score` |
| ~529, ~861 | `Child Holdout` | `Candidate untouched-period score` |
| ~881 (autoresearch gate form) | `Parent Day` | `Baseline today's score` |
| ~886 (autoresearch gate form) | `Child Day` | `Candidate today's score` |
| ~549, ~891 | `Epsilon` | `Minimum improvement (Sharpe)` |
| (any helper text near these fields explaining "epsilon = tolerance") | "Epsilon is the minimum delta…" | "Minimum improvement is the smallest Sharpe gain that counts as real." |

Note: the labels are getting longer. If a label of "Baseline
untouched-period score" wraps awkwardly in the form column, switch
the form layout to label-on-top instead of label-on-left. Don't
abbreviate to fit the old column width — the whole point is to be
readable.

## Section 3 — Button labels

| Where (MemorySurface.tsx line) | Today | Replace with |
|---|---|---|
| ~595 (optimization gate) | `Record Optimization Gate {id}` | `Record gate decision for {id}` |
| ~931 (autoresearch gate) | `Record Gate {run.id}` | `Record gate decision` (drop the ID — it's redundant with the row context) |
| ~803 | `Promote` | `Activate` |
| ~816 | `Demote` | `Retire` |
| ~692 | `Mint Child` | `Train new version` |
| ~1096 (Patterns tab) | `+ Add Pattern` | (keep) |
| Forget dialog ~1670 | `Confirm forget` | (keep) |
| Add Pattern modal submit | `Add Pattern` / `Saving…` | (keep) |

The pending-state strings (`Recording…`, `Activating…`, `Retiring…`)
should follow the same verb. If today it's `Promoting…`, change to
`Activating…`.

## Section 4 — Metric labels (Flywheel panel)

| Where (MemorySurface.tsx line) | Today | Replace with |
|---|---|---|
| ~394 | `Observations` | (keep) |
| ~395 | `Active` | (keep) |
| ~396 | `Staged` | (keep) |
| ~397 | `Forgotten` | (keep) |
| ~398 | `Runs` | (keep) |
| ~404 | `Obs / 7d` | (keep) |
| ~405 | `Promoted / 7d` | `Activated / 7d` |
| ~406 | `Demoted / 7d` | `Retired / 7d` |
| ~407 | `Children / 7d` | `New versions / 7d` |
| ~421 | `Lineage Depth` | `Generations deep` |

## Section 5 — Tab names and section headers

| Where (MemorySurface.tsx line) | Today | Replace with |
|---|---|---|
| ~978 (Tab 1) | `Patterns` | (keep) |
| ~979 (Tab 2) | `Observations` | (keep) |
| ~386 (Flywheel card title) | `Flywheel` | (keep) |
| ~430 (section header) | `Latest Lineage` | (keep) |
| ~430 (section header, full history) | `Optimization History` | `Training run history` |
| ~723 (section header) | `Recent Autoresearch Runs` | (keep) |
| ~723 (section header, full history) | `Autoresearch History` | (keep) |
| MemoryPage.tsx topbar sub | `Global namespace · Operator patterns and autoresearcher observations` | `Global namespace · Operator patterns and observations from the evening run` |
| agents-flywheel.tsx topbar title | `Flywheel` | (keep) |
| agents-flywheel.tsx back link | `Back to agent` | (keep) |

## Section 6 — Empty states, errors, helper text

| Where (MemorySurface.tsx line) | Today | Replace with |
|---|---|---|
| ~735 (autoresearch empty) | `No autoresearch runs yet.` | (keep) |
| ~1100s (Patterns empty, lifecycle=all) | `No patterns yet for {namespace}. Use "+ Add Pattern" to seed one.` | (keep) |
| ~1500s (Observations empty, agent) | `No observations yet for this agent.` | (keep) |
| ~1500s (Observations empty, workspace) | `No observations yet for the global namespace.` | (keep) |
| ~1524 (Observations info) | `Observations are read-only. Use "Forget all memory" to clear.` | (keep) |
| ~1670 (Forget dialog) | `This will soft-delete {n} memory item(s) from namespace …` | (keep) |
| Add Pattern alert ~1275 | `Patterns are matched to decision context via vector similarity, so an agent's provider (or a configured default) must support embeddings…` | (keep — but check that "embedder" doesn't appear inside; if it does, change to "embedding provider") |
| Helper text under Training data ends ~1383 | (current copy is fine; just make sure no jargon snuck in around "training window end" — that phrase should be "training data ends" in user copy) | (keep, audit for accidental jargon) |

## Section 7 — SSE event display labels (future, AR-2/AR-3)

Today the dashboard doesn't render live cycle events — that ships
with AR-2/AR-3. When that view lands, the wire event names below stay
as JSON payload identifiers; the dashboard renders the display label
column instead. Adding this as a `formatEventName(wire: string):
string` helper now (with the mapping below) means the AR-3 work just
calls the helper.

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

## Section 8 — New component: `<ShortHash>`

There's an existing `shortHash()` helper at MemorySurface.tsx:1700
that just truncates. We want to extract it into a proper component
that handles the "show short, copy full on click" pattern everywhere
the UI currently renders a raw ULID or 64-character hex string.

Proposed API:

```tsx
// src/components/ShortHash.tsx
type ShortHashProps = {
  value: string | null | undefined;
  // How many characters of the prefix to show.
  // Default: 8 for hashes (64-hex), 6 for ULIDs (26 chars).
  length?: number;
  // Optional prefix label, e.g. "Strategy" → renders "Strategy abc12345"
  label?: string;
  // Optional fallback for null/undefined. Default "—"
  fallback?: string;
  // Visual style: 'mono' (default, font-mono) or 'inline'
  variant?: "mono" | "inline";
};
```

Behavior:
- Renders `{label} {value.slice(0, length)}…` in monospace by default.
- On click: copies the full value to the clipboard, shows a toast
  (or inline checkmark for ~1s) saying "Copied".
- On hover: shows the full value as a tooltip (use the existing
  tooltip primitive — don't add a new one).
- If value is null/undefined, renders the fallback (not clickable).

Where to use it:
- Everywhere `shortHash(...)` is called in MemorySurface.tsx today
  (the `holdout {shortHash(...)} · train {shortHash(...)} · dev
  {shortHash(...)}` line).
- Every place a `bundle_hash` (64-hex string) is rendered.
- Every place a `session_id`, `cycle_id`, `run_id`,
  `optimization_id` (26-char ULID) is rendered as raw text.

After the migration, the standalone `shortHash()` helper at
MemorySurface.tsx:1700 can be deleted.

## Section 9 — Memory Mode dropdown copy

When the settings UI exposes the per-agent memory mode (currently
internal enum `Off` / `Global` / `AgentScoped`), use these labels:

| Value | Label | Helper text |
|---|---|---|
| `Off` | Off | This agent doesn't read or write memory. |
| `Global` | Shared across all agents | This agent reads and writes the shared global memory. |
| `AgentScoped` | This agent only | This agent has its own private memory pool. |

This is in scope if the settings UI is being touched as part of the
same patch; otherwise it's a future ticket.

## Acceptance criteria

The rename is done when:

1. Every string in tables §1–§6 has been changed.
2. No instance of these terms appears on a user-visible screen
   (search the rendered DOM, not the source): `Epsilon`, `Holdout`,
   `Mutation`, `Mutator`, `Ghost`, `Quarantined`, `Merkle`,
   `BLAKE3`, `Ed25519`, `Promoted`, `Demoted`, `Mint`, `Demos` (in
   user-facing copy), `Priors` (in user-facing copy).
3. `ShortHash` component exists at `src/components/ShortHash.tsx`,
   has unit tests (renders short, copies full on click, handles
   null), and replaces every raw-hash and raw-ULID render in the
   memory + flywheel surfaces.
4. The `formatVerdict` and `formatPromotionState` helpers exist at
   `src/features/memory/labels.ts` and are used in every place a
   verdict or promotion state was previously interpolated raw.
5. Screenshot diff: take fresh screenshots of (a) the Memory page
   workspace mode with at least one Pattern and one Observation
   visible, (b) the Memory tab on an agent page with the Flywheel
   panel rendered, (c) the Add Pattern modal open, (d) the Forget
   dialog open, (e) the agent-scoped flywheel page with at least one
   lineage row visible and one autoresearch run visible. Compare
   against the today-snapshots in the existing test fixtures.

## Test paths

- Existing Vitest suites that touch these surfaces:
  `frontend/web/src/features/memory/MemoryPage.test.tsx`,
  `frontend/web/src/components/agent/MemoryTab.test.tsx`,
  `frontend/web/src/routes/agents-flywheel.test.tsx`. Any test that
  asserts on the old display strings needs its expectations updated
  to the new strings — that's the point.
- `frontend/web/src/api/flywheel.test.ts` should NOT need changes
  (it tests the API client, not display).
- Add new unit tests for `ShortHash` and the label helpers.

## Out of scope (do not change in this patch)

- CLI verbs and flags (`xvn autoresearch gate`, `--gate-epsilon`,
  etc.) — handled by a separate CLI rename patch.
- API field names — read-only contract for the frontend.
- Route paths — would break deep links.
- Spec/plan/notes docs — those use developer-surface vocabulary on
  purpose.
- Status enum values in TypeScript (e.g., `promotion_state: "staged"
  | "promoted" | "demoted"`) — these match API field values; only
  the display strings change.

## Things to push back on

If any of these strike you as wrong, flag before shipping — your
copywriting judgment matters more than my proposals:

1. "Untouched-period score" for `delta_holdout`. The exact phrase
   was the operator's preferred shorthand; if it reads weird in
   context, "untouched test score" or "out-of-sample score" are
   acceptable alternatives.
2. "Train new version" for `Mint Child`. If the surrounding form
   makes it ambiguous what "version" means, "Train new agent
   version" works too.
3. "Generations deep" for `Lineage Depth`. The metric is a float
   (e.g., 2.4 generations); if "generations deep" reads awkward with
   a decimal, "Average generation" or "Evolution depth" are
   acceptable.
4. The badge value `Retired` for what was `demoted`. If the operator
   workflow needs to distinguish "I demoted this" from "the system
   forgot this," we should preserve the distinction.

## Reference

- Canonical terminology lock (the source of truth that drove this
  handoff): `docs/superpowers/specs/2026-05-27-autoresearcher-terminology-lock.md`
- Background audit with full rationale per term:
  `docs/superpowers/notes/2026-05-27-autoresearcher-plain-language-audit.md`
- Project-wide terminology conventions: `/CLAUDE.md` §Terminology
- xvision frontend design conventions: `frontend/DESIGN.md` (no
  popups rule, no right-side boxes when chat rail is visible —
  relevant if any of these renames push form layouts into needing
  more horizontal room)
