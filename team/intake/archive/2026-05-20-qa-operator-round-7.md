# Intake — 2026-05-20 — QA operator round 7

Operator QA pass 2026-05-20 across the dashboard SPA — eval inspector,
list pages, capsule, trace inspector, eval summary. Nine items. Lands
on the **immediate board** (`team/board.md`), not V2.

## Source

Operator session, 2026-05-20 morning. Followed the QA Round 5/6 cadence;
queues alongside the round-6 items (`team/intake/2026-05-19-qa-operator-round-6.md`)
not yet claimed.

## Findings

### F-1 — Eval inspector top bar: link to strategy + scenario + agents

The eval inspector page (`/eval-runs/<id>`) doesn't expose which
strategy or scenario the run was launched against — the operator
has to leave the page or guess from the title. Add a top-bar surface
that:

- Links to the strategy used (`/strategies/<id>`).
- Links to the scenario used (`/scenarios/<id>`).
- Shows the agent objects attached to that strategy (the
  `Strategy.agents: Vec<AgentRef>` list, each linking to
  `/agents/<id>`).

Constraints: no popups (`/CLAUDE.md`); inline pills/breadcrumbs in
the top bar. Data is already on the run record — `EvalRunExport`
includes the `strategy` + `scenario` slots plus `agents[]`.
Verification: open any completed run; top bar shows strategy
name + scenario name + N agent chips, every chip routes correctly.

### F-2 — Search and filter on lists never landed

Operator expected the standardized list component (search + filter +
"Recently added" default sort + page size) to be on the eval / strategy /
scenario / agent lists. It is not. Reviewed:

- `team/intake/2026-05-19-list-component-design-intake.md` —
  phase-0 design intake, package at `docs/design/FilterSearchLists.zip`,
  needs a spec under `docs/superpowers/specs/` before contracts can open.
- `team/board.md` Reserved entry: `list-component-spec` (phase-0).
- Recent landings 2026-05-19 evening → 2026-05-20 morning: CLI agent
  workbench wave-c #370, wave-d #374, eval-run polling reduction #368,
  runstore-finalize #375, MCP parity, observability blob GC, V2E
  intake bundle, terminology renumber 022→024, AgentSlot.temperature
  thread-through, skills refresh #379. **No list-component PR.**

So the gap is real: the work hasn't been done yet, just intaked. Ask:
decompose `list-component-spec` into the actual spec doc + the first
implementation track (eval-runs list, since that's where round-7 starts),
or — if waiting for the unified component blocks too much — ship a
quick search box + "Recently added" default sort on the eval-runs
list now and migrate it to the standard component when it lands.
Confirm direction with operator before the conductor decomposes.

### F-3 — Default sort: most recent at the top

Across every list (eval runs, strategies, scenarios, agents,
experiments), the default sort should be "most recent first."
The list-component intake already commits to this ("'Recently added'
is option 1 and the default everywhere",
`team/intake/2026-05-19-list-component-design-intake.md:64`), but
since the unified component hasn't landed yet, audit each existing
list page and confirm/fix the default. Where the API returns a
sortable field (`created_at`, `started_at`, `updated_at`), bind the
default to it; don't sort client-side over a paginated slice.

### F-4 — Items-per-page on long lists

Long lists need a page-size control (e.g. 25 / 50 / 100, default 50).
Today the eval-runs list returns N rows from the API with no operator
control over page size and no pagination UI. Wire this in lockstep
with F-3 so the default sort + page size land together. Where the
backend already supports `limit` / `offset`, expose them; where it
doesn't, add the support (single endpoint touch; document the API
addition).

### F-5 — "PAYLOAD REF" label is too vague in prompt / trace

In the prompt / trace inspector, content blocks rendered as
`PAYLOAD REF <hash>` give the operator no signal about *what* the
ref is. Replace the generic "PAYLOAD REF" label with a
user-identifiable descriptor:

- For prompt blocks: the role + a short summary of the content
  (e.g. `system prompt — trader/v3`, `tool result — bars[1h, 480 rows]`).
- For tool calls: tool name + key args
  (e.g. `compute_indicator(rsi, 14)`).
- For decisions: `TraderDecision · BUY 0.4 BTC`.

Keep the hash as a secondary affordance (copy button or tooltip) so
the operator can still cross-reference, but the primary label must
be human-readable. Where the ref points at a blob we haven't fetched
yet, fall back to a typed placeholder (`prompt blob · 12 KB`) rather
than a bare hash.

### F-6 — Capsule eval short title not clickable

In the multi-eval capsule, the short eval title is read-only.
Should link to `/eval-runs/<id>` so the operator can jump straight
to the inspector. Today the capsule only switches focus
(`onSwitchFocus(r)` in
`frontend/web/src/features/agent-runs/EvalCapsule.tsx`); add a
proper anchor (or `<Link>`) on the short title.

Constraint: no popups — this is a routing change, not a hover-card
preview.

### F-7 — Trace inspector: remove "Super" button, add "Trade" button

Two parts:

- **Remove**: the "Super" button on the trace inspector. Stale; no
  operator workflow needs it. Audit any callers / handlers and
  delete cleanly (not commented out — see
  `feedback_alpha_root_cause.md`).
- **Add**: a "Trade" button that surfaces the trader's decision +
  the broker action for the cycle the trace is bound to.
- **Investigate**: trade events seem to be missing from the trace
  now. Determine whether (a) the executor stage isn't emitting
  trace events, (b) the events emit but the trace view filters them
  out, or (c) the events emit and are kept but a recent UI change
  hid them. Likely candidates to check: recent harness span-attr
  work (PR #294, F-2), the typed-mechanical-params change (PR #302,
  F-6), `qa-trace-broker-spans` (PR #283), the trace-fullscreen
  redesign (PR #249). Don't add the "Trade" button until you know
  whether the underlying data is even arriving.

### F-8 — Add total API cost to eval summary

The eval summary card shows tokens but not total API cost. Add a
"Total cost (USD)" stat next to the tokens stat. Source: the same
`model_call_cost_usd` field the per-call cost view already uses;
sum across the run. Where a run has multiple agents, the summary
is the sum across all of them.

Cross-checks against the existing eval capsule cost display so the
capsule number and the summary number agree (no double-counting,
no missing slot).

### F-9 — More decimal points on small cost displays

Several places display cost as `$0.00` for runs where the actual
cost is e.g. `$0.0007` (Haiku, mini models, llama.cpp / vLLM
endpoints). Audit every cost surface:

- Eval capsule cost field.
- Eval summary cost (F-8).
- Per-call cost in trace.
- Compare table cost column.

Decide the display rule once and apply everywhere. Suggested:
4 significant figures or 6 decimal places, whichever is fewer
characters, plus an explicit `<$0.0001` floor for non-zero values
that would otherwise round to zero. Don't display "$0.00" for a
non-zero cost — that's an integrity bug, not a rounding choice.

## Verification (when contracts land)

For each track:

- Add or update a `frontend/web/` test (Vitest / Playwright) that
  exercises the changed surface.
- Run `pnpm --dir frontend/web typecheck && pnpm --dir frontend/web test --run`
  before merging.
- Verify against a live run in `xvn` dev (Tailscale node
  `https://xvn.tail2bb69.ts.net`) — not just unit tests. Record the
  before/after as a brief operator note in the contract status doc.
- No popups (`/CLAUDE.md` frontend rule).

## Decomposition guidance (for the conductor)

These cluster naturally:

- **List wave** (F-2, F-3, F-4) — decide unified-component vs.
  per-page quick fix first; either path uses one contract.
- **Eval inspector wave** (F-1, F-6, F-8, F-9) — small UI cluster
  around the inspector + capsule.
- **Trace wave** (F-5, F-7) — needs the trade-investigation step
  before the button add; F-5 (PAYLOAD REF labels) likely shares a
  rendering helper with the trace view, so co-locate.

## Non-goals / out of scope

- Building a brand-new list component in this round if the
  unified-component decomposition from F-2 is the chosen path —
  that's the design-handoff intake's job. Round-7 ships the operator
  outcome (sorted, paginated, searchable) by whichever path the
  conductor picks.
- Cost-model changes in the eval engine (V2E `eval-cost-model-per-bar-and-volume-share`
  is the engine track). Round-7 only touches display.
- Touching the trace event schema. F-7's "Trade" button assumes the
  underlying events arrive; if they don't, the fix is upstream of
  this intake.

## Related artifacts

- `frontend/web/src/routes/eval-run.tsx` (or wherever the
  inspector route lives) — top bar host for F-1.
- `frontend/web/src/features/agent-runs/EvalCapsule.tsx` — F-6
  click-through target.
- `frontend/web/src/features/.../TraceInspector.tsx` — F-5, F-7.
- `frontend/web/src/routes/eval-list.tsx` — F-2, F-3, F-4 host.
- `team/intake/2026-05-19-list-component-design-intake.md` — phase-0
  design intake F-2 depends on.
- `team/intake/2026-05-19-qa-operator-round-6.md` — sibling QA wave
  not yet claimed.
- Recent eval-list / trace / observability PRs to check during F-7
  investigation: #294 (harness-span-attrs), #302
  (harness-typed-mechanical-params), #283 (qa-trace-broker-spans),
  #249 (trace-fullscreen-redesign), #261 (qa-trace-dock-resizable).
