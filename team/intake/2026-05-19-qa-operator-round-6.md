# Intake — 2026-05-19 — QA operator round 6 (calendar whitespace, scenario-runs name, agent usage panel non-functional)

Operator findings (Ed, 2026-05-19, batch "QA23"). Three issues spanning
visual polish on the scenario-form calendar, a missing run-name in the
scenario inspector's Runs tab, and the "Where this agent is used" panel
on the Agents page silently rendering empty-state copy because the
underlying engine API stubs were never replaced after the strategies
refactor landed.

## Source

Operator chat / dashboard session, 2026-05-19. The agent-usage panel
finding includes verbatim placeholder copy that confirms the stubs in
`crates/xvision-engine/src/api/agents.rs:313` and `:325` are still in
place — they each return `Ok(Vec::new())` with a TODO comment that
explicitly says "until the strategies refactor lands and strategies
start referencing agents." That refactor has landed (see
`docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md`),
so the stubs are now ship-ready to replace with the real query.

## Triage status — 2026-05-21 conductor re-triage

All three tracks still open and valid; none superseded by 2026-05-20 /
2026-05-21 waves. The intake was briefly moved to
`team/intake/archive/` during the 2026-05-21 conductor sweep — that was
premature; restored to active intake on 2026-05-21 by the conductor
re-triage of QA rounds 4/6/7. Ready to decompose into board contracts
the next time the conductor opens a wave:

- `agent-usage-panel-wire-deployed-and-runs` (P1, integration) — the
  backend stubs at `crates/xvision-engine/src/api/agents.rs:313` and
  `:325` still return `Ok(Vec::new())` with a TODO citing the
  strategies refactor; that refactor has landed, so the stubs are
  ship-ready to replace. Concrete and self-contained.
- `scenario-form-calendar-whitespace` (P2, frontend CSS leaf).
- `scenario-runs-tab-show-eval-name` (P2, frontend display leaf with a
  small backend hop).

## Already in flight / queue notes

- The list-component design package (`docs/design/FilterSearchLists.zip`)
  is filed as a separate intake at
  `team/intake/2026-05-19-list-component-design-intake.md` — it's a
  planning intake (needs a spec under `docs/superpowers/specs/` before
  contracts open) and isn't part of this round.
- Round 5 (`2026-05-19-qa-validate-draft-cadence-false-positive.md`)
  bundled F-1/F-2/F-3/F-5 in PR #316 and F-4 separately; this round
  intentionally re-uses F-N numbering scoped to this intake only.

## V2 roadmap items (not contracts here)

None — all three findings are scoped to existing surfaces.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P2 | The scenario-form's `InlineRangeBar` calendar mounts with zero margin around it inside the wizard's vertical stack; the calendar abuts the row above and the form below with no breathing room. The mobile `MobileInlineCard` variant has the same issue. Operator describes it as "seems to be zero margin around it." | `scenario-form-calendar-whitespace` |
| 2 | P2 | The scenario inspector's Runs tab (`frontend/web/src/routes/scenarios-detail.tsx:406-470`) renders run ULIDs and strategy ids in the Run / Strategy columns. The operator wants the **eval name** (and/or strategy name) surfaced — operators reading the list shouldn't have to click into every run to identify it. | `scenario-runs-tab-show-eval-name` |
| 3 | P1 | The "Where this agent is used" card on the agent edit page (`frontend/web/src/components/agent/AgentForm.tsx:368-432`) always renders the empty-state copy `"Not deployed in any strategy yet."` and `"No runs yet. Eval-run attribution lands when strategies start referencing agents."` regardless of how many strategies reference the agent or how many eval runs use it. The backend stubs at `crates/xvision-engine/src/api/agents.rs:313,325` still return `Vec::new()` with a TODO that the strategies refactor has invalidated. | `agent-usage-panel-wire-deployed-and-runs` |

Three tracks. One CSS-only leaf, one frontend leaf with a backend hop
(scenario run rows need eval-name), one integration that ships engine
query bodies + verifies the frontend renders them.

## Track summaries

### `scenario-form-calendar-whitespace` (P2, frontend leaf)

Operator finding: "evaluate white space around calendar where it is
put in. Seems to be zero margin around it."

The calendar mounts at
`frontend/web/src/components/scenario/ScenarioForm.tsx:268-289`:

```tsx
<div className="hidden sm:block">
  <InlineRangeBar startIso={from} endIso={to} … />
</div>
<div className="block sm:hidden">
  <MobileInlineCard startIso={from} endIso={to} … />
</div>
```

Neither wrapper sets vertical spacing. The previous sibling is a
`<Row>` ending in a `<Field label="Quote">` and the next sibling is
another `<Row>`. Both render with the parent form's column gap, but
the two unwrapped calendar divs land outside that flow.

Audit pass:

1. Confirm parent layout. The form uses a CSS column gap or vertical
   stack — figure out which and whether the calendar divs are excluded.
2. Fix scope: either add `my-3` / `my-4` to both wrappers (or a wrapper
   class like `scenario-form-calendar-block`), or fold them into the
   same vertical-rhythm container the surrounding `<Row>`s sit in.
3. Verify desktop **and** mobile — the mobile variant's
   `<MobileInlineCard>` should also breathe.

Visual reference: any other `<Card>`-style form region in the same
file for the rhythm.

Don't change `InlineRangeBar` or `MobileInlineCard` themselves; the
fix is layout glue, not component refactor.

### `scenario-runs-tab-show-eval-name` (P2, frontend leaf + small backend hop)

Operator finding: "Add eval name to 'Runs' on Scenario inspector."

The `RunsTab` (`scenarios-detail.tsx:408-470`) calls `listRuns()` and
filters by `scenario_id`. The table renders five columns: Run,
Strategy, Mode, Status, Completed. The Run column shows the raw ULID
and the Strategy column shows the agent_id.

Two fixes:

1. **Eval name (the primary ask).** Eval runs don't carry a `name`
   today — operators identify runs by the strategy + scenario + start
   timestamp. The most useful surfacing here is:
   - rename the Run column to "Eval" (or "Run") and put the
     strategy display_name as the headline cell text, with the run
     ULID below it as a smaller monospace line, OR
   - add a new "Eval" column whose value is the strategy
     display_name resolved via `xvision_engine::api::strategy::get(strategy_id).name`
     (or whichever field is the canonical display name).
2. **Strategy name in the Strategy column.** Today it's `r.agent_id`
   which is a ULID. Resolve to the strategy's display_name (or render
   `<display_name>` over a muted `<id>`).

API surface: the front-end can either (a) batch-resolve strategy ids
into names client-side via the existing `listStrategies()`-style call
in `frontend/web/src/api/strategy.ts`, or (b) extend the eval-runs
list payload to include a `strategy_display_name` field. Option (a)
keeps the change frontend-only; (b) is the cleaner shape and is a
small extension on `xvision_engine::api::eval::list`. Pick (a) first
unless an audit shows (b) saves more than one round-trip per page.

Out of scope: a true "eval name" persisted field on RunSummary. Today
the strategy + scenario + started_at is identity enough — naming runs
is a v2 ask, not this round.

### `agent-usage-panel-wire-deployed-and-runs` (P1, integration)

Operator finding: "This is non functional under agents: Where this
agent is used / Deployed in strategies / Not deployed in any strategy
yet. Reference this agent from a strategy's authoring page to link it.
/ Recent runs / No runs yet. Eval-run attribution lands when
strategies start referencing agents."

The frontend at `frontend/web/src/components/agent/AgentForm.tsx:368-432`
correctly queries `deployedInStrategies(agentId)` and
`recentRuns(agentId, 5)` and renders empty-state copy when both return
empty arrays. Those API calls hit
`crates/xvision-dashboard/src/routes/agents.rs:115,123` which delegate
to engine-side functions in
`crates/xvision-engine/src/api/agents.rs`.

Both engine functions are stubs:

```rust
// crates/xvision-engine/src/api/agents.rs:313
/// V1 stub — returns empty until the strategies refactor lands and
/// strategies start referencing agents.
pub async fn deployed_in(_ctx: &ApiContext, _agent_id: &str) -> ApiResult<Vec<StrategyRef>> {
    Ok(Vec::new())
}

// :325
/// V1 stub — returns empty until eval-runs are attributed to agents.
pub async fn recent_runs(_ctx: &ApiContext, _agent_id: &str, _limit: u32) -> ApiResult<Vec<RunRef>> {
    Ok(Vec::new())
}
```

The strategies refactor (`docs/superpowers/plans/2026-05-12-strategies-refactor-agent-composition.md`)
landed; strategies now carry `Vec<AgentRef>` with `agent_id` per ref.
And eval-runs persist `agent_id` (the strategy id under the locked
terminology) via `crates/xvision-engine/src/eval/store.rs`. Both
queries are now answerable.

Two halves:

1. **`deployed_in`** — scan `StrategyStore` for strategies whose
   `agents: Vec<AgentRef>` contains an `AgentRef { agent_id, … }`
   matching the requested agent id. Return `Vec<StrategyRef { strategy_id, name }>`.
   The strategy store today loads all bundles from disk (`store().list()` +
   per-id load). Fine for v1; revisit indexing once the strategies
   table gets a join column.
2. **`recent_runs`** — there's an indirection: eval-runs reference a
   strategy id, not an agent id. To answer "recent runs for this
   agent" the query has to (a) find strategies that reference the
   agent (same logic as `deployed_in`) and (b) list eval-runs for any
   of those strategy ids, sorted by `started_at` descending, capped
   at `limit`. Use `RunStore::list_by_agent_ids(&[…])` if one exists,
   or build the union from per-strategy `list_runs(agent_id=…)` calls.

Acceptance: open the Agents page for an agent that's referenced by at
least one strategy; the "Deployed in strategies" column lists those
strategy display_names; if any of those strategies has at least one
completed eval-run, the "Recent runs" column shows the last 5 with
their status. Both empty-state messages still render correctly when
neither condition holds.

Tests to add:

- `crates/xvision-engine/src/api/agents.rs` — unit test that builds
  two strategies referencing the same agent and asserts `deployed_in`
  returns both StrategyRefs.
- `crates/xvision-engine/src/api/agents.rs` — unit test that builds
  one strategy referencing an agent and one completed eval-run on
  that strategy; `recent_runs(agent_id, 5)` returns one row.
- `frontend/web/src/components/agent/agents.test.tsx` already mocks
  both API functions; add a positive-case test where the mocks return
  one item each and the panel renders them (the existing tests only
  cover empty-state).

Out of scope: a new SQL index, surface in the strategies table, or
eval-runs schema change. Today's bundle-loop scan is acceptable while
the data model stabilises.

## Verbatim findings

> Create items:
>
> 1) evaluate white space around calendar where it is put in. Seems
>    to be zero margin around it.
>
> 2) Intake the SearchFilterLists.zip design component for lists and
>    turn into intake for plan writing.
>    (Filed separately as
>    `team/intake/2026-05-19-list-component-design-intake.md`.)
>
> 3) Add eval name to "Runs" on Scenario inspector.
>
> 4) This is non functional under agents: Where this agent is used /
>    Deployed in strategies / Not deployed in any strategy yet.
>    Reference this agent from a strategy's authoring page to link it.
>    / Recent runs / No runs yet. Eval-run attribution lands when
>    strategies start referencing agents.
