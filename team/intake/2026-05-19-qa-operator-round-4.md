# Intake — 2026-05-19 — QA operator round 4 (inspector polish, eval-id regression, CLI-job bridge, clone/validate gaps)

Operator findings (Ed, 2026-05-19, batch "QA22"). Mix of inspector
polish nits, a chart-snapshot regression that shrank the chart into a
tiny squished frame, an eval-id surfacing regression from a prior
"remove the ID" sweep that went too far, an MCP/CLI-job bridge gap
where `eval_run_*` job IDs come back "not found" from the agent's
`get_cli_job_output` calls, and two validation gaps (clone-to-edit
producing a non-editable strategy; CLI/UI allowing a strategy with
zero agents).

## Source

Operator chat / dashboard session, 2026-05-19. Verbatim findings
preserved at the bottom. The chat-rail excerpt includes real tool-call
traces from the wizard hitting `cli job 'eval_run_*' not found` on
`get_cli_job` / `get_cli_job_output`.

## Already in flight / queue notes

- Round-3 tracks (`wizard-strategy-template-optional`,
  `wizard-scenario-create-tool-repair`, `trader-output-action-case-insensitive`,
  `ui-scrollbars-always-visible`, `agent-error-feedback-self-healing`,
  etc.) are still in flight; this batch doesn't overlap their scope.
- The eval-id "remove the truncated id from the inspector header" cleanup
  that this batch reverses landed recently — round-4 explicitly says
  "removing the eval ID went way too far" — so the track here is a
  *partial revert + redesign*, not a respawn of that work.

## V2 roadmap items (not contracts here)

- "What happens to the trace capsule when multiple evals are running?"
  is a design question, not a contract — see
  `trace-capsule-multi-eval-behavior` below for the spike scope. Full
  multi-stream capsule UX lives on `team/board-v2.md`.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P2 | Chart snap renders the chart tiny / squished. Operator wants it converted to a button-triggered action with a fixed sane render size | `eval-inspector-chart-snap-button` |
| 2 | P1 | Eval ID was removed too aggressively. Needs to appear at top of every Inspector (just below title, not as prominent as title) and must not be truncated anywhere (including the overall eval list — today `shortId()` truncates to 10 chars) | `eval-id-resurface-no-truncate` |
| 3 | P1 | Chat rail / MCP `get_cli_job` + `get_cli_job_output` return `cli job 'eval_run_*' not found`. Eval runs spawned via `run_eval` do not register as fetchable CLI jobs for the agent. Wizard hits this in tight loops | `mcp-eval-run-job-bridge` |
| 4 | P2 | A "Sell" decision is labelled "SHORT" in the inspector even when the agent is just flatting / selling, not opening a short. Mapping of `short_open` → "SELL" + label collapses `flat` vs `short_open` distinction | `decision-side-label-sell-vs-short` |
| 5 | P2 | Strategy summary top panel in the eval inspector should show total money gained / lost (absolute terminal-PnL in account currency, not just % return) | `eval-inspector-total-pnl-summary` |
| 6 | P1 | Paper eval inspector is missing PnL columns / summary that the backtest inspector has; buy/sell orders also do not render in the paper view | `paper-eval-inspector-parity` |
| 7 | P2 | "Back to list" affordance is only on the eval inspector. Strategies, scenarios, agents, and agent-runs detail routes lack it | `inspector-back-to-list-buttons` |
| 8 | P2 | In the trace capsule overlay, clicking "full-screen trace" should auto-minimize the capsule (the capsule and the full-screen view are redundant when both are open) | `trace-capsule-fullscreen-minimize` |
| 9 | P3 (spike) | Undefined behaviour: what does the trace capsule do when multiple evals are running concurrently? Stack, switch, list, hide? Needs a tiny design spike + a default | `trace-capsule-multi-eval-behavior` |
| 10 | P1 | Cloning a strategy produces a record the operator cannot edit. Server-side clone marks the new row as a draft (`published_at = None`), but the SPA still treats it as locked — frontend permission/edit-mode gate is wrong for clones | `strategy-clone-editable-frontend` |
| 11 | P1 | Strategy can be created with zero agents via both the CLI and the UI. At least one agent should be a hard validation block. Today `strategies/validate.rs` accepts an empty `agents` list when legacy slot fields are filled, which leaks the loophole | `strategy-require-at-least-one-agent` |

Eleven tracks. Three integration (CLI-job bridge, paper-eval parity,
strategy-clone gate), seven frontend leaves, one design spike.

## Track summaries

### `eval-inspector-chart-snap-button` (P2, frontend leaf)

Operator describes the current chart-snapshot rendering inside the
eval inspector as "tiny and squished". The fix: make the snapshot an
explicit button-triggered action (don't auto-render) and give the
rendered chart a fixed sane aspect / size when it does render.

Starting points:
- `frontend/web/src/routes/eval-runs-detail.tsx:227-237` — chart render section.
- `frontend/web/src/components/chart/LiveChart.tsx:80` — snapshot status display.
- `frontend/web/src/components/chart/RunChart.tsx` — the chart component itself.

Verify the cause first — could be a flexbox/grid container clamping the
canvas, an aspect-ratio constraint, or a `RunChart` prop sizing it from
a small parent. Behind the button-gate is the contracted UX; the
sizing bug is the underlying defect.

### `eval-id-resurface-no-truncate` (P1, frontend leaf)

Partial revert + redesign of a prior cleanup that stripped the eval id
from the inspector header. Two concrete asks from the operator:

1. **Surface the full eval id at the top of every Inspector**
   (eval-run, strategy, scenario, agent, agent-run), placed just below
   the title with weaker visual weight (smaller, muted). Not as
   prominent as the title.
2. **Stop truncating the eval id anywhere it shows.** The eval-runs
   list today renders `shortId(row.id)` which truncates to 10 chars
   (`frontend/web/src/lib/run-display.ts:67-69`,
   `frontend/web/src/routes/eval-runs.tsx:385`,
   `:359,492`). Show full id; use horizontal scrolling / monospace
   wrapping in the cell, not truncation. The full id must round-trip
   through copy-paste.

Audit pass: any other surface that renders `shortId()` for an eval id
(trace dock, chat rail, observability table) — the no-truncate rule
applies there too. `shortId()` may legitimately stay for *other* ids
(decision ids, span ids); scope to eval ids only.

### `mcp-eval-run-job-bridge` (P1, integration)

The wizard's chat rail (and any MCP client calling the same surface)
sees this pattern:

```
run_eval → returns job id eval_run_XKI6IWGw5aFZXsqkW3a3
get_cli_job(eval_run_XKI6IWGw5aFZXsqkW3a3) → "cli job ... not found"
get_cli_job_output(eval_run_XKI6IWGw5aFZXsqkW3a3) → "cli job ... not found"
```

Two failures observed in the verbatim trace
(`eval_run_PKmkXjluX5Doj097FEE6`, `eval_run_XKI6IWGw5aFZXsqkW3a3`).

The agent cannot read back the result of an eval it just kicked off.
The CLI-job store
(`crates/xvision-dashboard/src/cli_jobs/store.rs:258`) emits the
"not found" error; the agent-facing handler is
`crates/xvision-dashboard/src/wizard_loop.rs:808-820`; the prefix
allowlist is in `crates/xvision-dashboard/src/cli_jobs/allowlist.rs`.

Diagnosis path:
1. Does `run_eval` actually register a CLI-job entry with id
   `eval_run_<ULID>`, or does the eval runner use a separate registry
   (eval_runs table) that the cli-jobs allowlist doesn't bridge?
2. If separate: add a bridge so `get_cli_job` / `get_cli_job_output`
   accept `eval_run_*` ids and fetch from the eval-runs registry
   (status + tail-of-log) rather than the cli-jobs store.
3. Confirm the prefix is in the allowlist if it should be there.

Acceptance: agent kicks off `run_eval`, polls `get_cli_job`, sees
status transitions queued → running → done, then `get_cli_job_output`
returns either the eval summary JSON or a structured pointer to the
eval-runs detail.

### `decision-side-label-sell-vs-short` (P2, frontend leaf)

`frontend/web/src/routes/eval-runs-detail.tsx:729` — `decisionKind()`
maps `short_open` → `sell`; `:750-755` — `decisionActionLabel()`
renders the resulting "sell" kind as the literal string "SHORT" (or
similar). Operator sees "SHORT" on plain flat-out / sell decisions
that aren't opening a short.

Fix: distinguish three cases:
- `long_open` → "BUY" (open long)
- `short_open` → "SHORT" (open short)
- `flat` (close a long) → "SELL"
- `flat` (close a short) → "COVER"
- `hold` → "HOLD"

`flat` is direction-dependent on the prior position; resolve against
the running position when rendering. Tests: each branch with a
canonical decision row.

### `eval-inspector-total-pnl-summary` (P2, frontend leaf)

Add an absolute PnL field to the strategy summary top panel of the
eval inspector. Backtest already computes terminal balance vs initial;
surface `(terminal - initial)` in account currency next to the
existing % return / equity-curve summary stats. Source path
likely `frontend/web/src/routes/eval-runs-detail.tsx` (top summary
section near `:219` and the decisions table headers `:624,655-657`).

If a similar field already exists for backtest but not paper, fold
into `paper-eval-inspector-parity` instead of duplicating.

### `paper-eval-inspector-parity` (P1, integration)

Two observed gaps when viewing a paper eval inspector vs a backtest
eval inspector:
1. PnL column / summary missing — backtest shows it, paper doesn't.
2. Buy / sell orders don't render at all in the paper view.

Diagnosis path:
- `crates/xvision-engine/src/api/eval.rs:669,1067` — RunMode dispatch
  differs for Paper vs Backtest. Confirm whether paper runs persist
  decision rows, fills, and equity snapshots to the same tables /
  endpoints as backtest.
- Frontend: confirm the eval-runs-detail loader hits the right API
  for paper runs. If the data is present but the loader requests a
  backtest-only endpoint, that's a one-line fix; if the data isn't
  being persisted in the paper path, this is an engine-side gap.

Acceptance: a paper eval inspector shows the same decisions table,
order/fill list, and PnL summary as a backtest inspector. The
underlying mode label is the only visual difference.

### `inspector-back-to-list-buttons` (P2, frontend leaf)

Eval-runs detail has the "← Back to runs" affordance
(`frontend/web/src/routes/eval-runs-detail.tsx:806`). Strategies,
scenarios, agents, and agent-runs detail routes do not. Add a
consistent "← Back to <plural>" link in the inspector header for
every detail route.

Surfaces to patch:
- `frontend/web/src/routes/strategies-detail.tsx`
- `frontend/web/src/routes/scenarios-detail.tsx`
- `frontend/web/src/routes/agent-runs-detail.tsx`
- Any other `*-detail.tsx` that doesn't already have one.

Use the eval-runs detail's existing button as the visual + behaviour
reference (it should navigate to the list route, not browser-history
`back` — operators may have deep-linked).

### `trace-capsule-fullscreen-minimize` (P2, frontend leaf)

The trace capsule overlay has a "full-screen trace" affordance.
Clicking it should auto-minimize the capsule, because the full-screen
trace view shows the same content. The current behaviour leaves both
visible, redundantly.

Starting points:
- `frontend/web/src/features/agent-runs/TraceDock.tsx`
- `frontend/web/src/stores/trace-dock.ts`

Wire the "go full-screen" handler to set the capsule's
expanded/visible state to minimized.

Per the workspace no-popups rule (`/CLAUDE.md`), the trace capsule
already exists as a dock, not an overlay; if any part of it is using
a modal/sheet, factor that into the fix.

### `trace-capsule-multi-eval-behavior` (P3, spike)

Open design question: what happens to the capsule when multiple evals
are running concurrently? Options to consider:
- **Stack** — one capsule per active eval, vertically stacked.
- **Tabs** — single capsule, tab per active eval.
- **Pinned + list** — pinned capsule for the currently-focused eval,
  with a small "N other evals running" pill that opens a list.

Deliverable: a one-page design note in
`docs/superpowers/specs/2026-05-19-trace-capsule-multi-eval.md`
recommending one of the above, plus an issue / followup ticket. No
code yet — the implementation track follows the design decision.

### `strategy-clone-editable-frontend` (P1, integration)

Server-side clone is already doing the right thing — `clone_strategy()`
in `crates/xvision-engine/src/api/strategy.rs:475-518` sets
`published_at = None`, marking the new row as a draft
(`:486`). So the engine-side contract permits editing.

The frontend must be gating "is this editable" on something other
than `published_at`. Likely candidates:
- The clone copies `agent_id` (token id) over and the SPA treats any
  row with an `agent_id` as locked (minted) regardless of
  `published_at`.
- A separate `is_clone` / `parent_id` field is flipping a "locked"
  view-mode in the strategies detail route.

Audit the strategies detail route + the clone result handler; confirm
the cloned row enters draft mode in the SPA and the inline-edit
affordances are enabled. Test: clone a strategy from chat rail and
from `/strategies` button, verify each lands editable.

### `strategy-require-at-least-one-agent` (P1, integration)

`crates/xvision-engine/src/strategies/validate.rs:10,33-41` already
emits the right error message ("strategy must have at least one agent
or filled LLM slot") but the **`or filled LLM slot`** branch leaks
the loophole — a strategy with empty `agents: []` and a legacy slot
filled (intern_slot etc., which the 2026-05-12 refactor said are
free-text role labels, not the source of truth) passes validation.

Two halves:
1. Tighten validation: require `agents.len() >= 1`. Drop the legacy
   slot-fallback branch. (Pre-rename breaking-change era is over;
   no need for that compat shim per the `/CLAUDE.md` guardrail.)
2. CLI surface: `crates/xvision-cli/src/commands/strategy.rs:291`
   (legacy slot migration handler) should refuse to create a
   strategy without at least one resolved agent, with a clear error
   message + a hint at `xvn agent create` / `--agent`.

The UI's create flow needs the same gate — disable the "create
strategy" submit button until at least one agent is wired. Tests:
both CLI and API routes reject empty-agents creation; existing
strategies with empty `agents` (if any in fixtures) get a migration
path or are flagged as invalid in the eval pipeline.

Out of scope: prescribing *which* agents — only "at least one".

## Verbatim findings

> QA22

> Chart snap is broken, it makers the chart tiny and squished. Maybe
> make it a button and fix function.

> Removing the Eval ID went way too far now it doesnt appear anywhere
> in the eval! Need to surface the id, in each view. Right now it only
> appears in the overall evaluation list as a truncated id. It should
> not be truncated anywhere, and it should show up at top of Inspector
> (but not be as prominent as title, just below title).

> Chat Rail: I have attempted to trigger a backtest review for the
> Macro MACD-RSI Weekly Trader strategy against the BTC 30-Day Jan
> 2025 scenario.
> It appears I cannot access the completed eval run data directly
> through the standard CLI job interface. When I try to run the
> evaluation, the system acknowledges the request, but the resulting
> job ID is not being recognized by the output fetcher, likely because
> the background execution process handle is not immediately available
> or is being managed outside the standard job queue for this session.
> Would you like me to try triggering the eval for the other strategy,
> Multi-Factor Logic Agent, to see if we can get a successful read
> from that, or is there a specific metric you were looking for from
> the MACD-RSI run?
> + list_strategies completed
> + list_scenarios completed
> + Eval action complete
> ! get_cli_job failed: cli job 'eval_run_PKmkXjluX5Doj097FEE6' not found
> ! get_cli_job_output failed: cli job 'eval_run_PKmkXjluX5Doj097FEE6' not found
> + Eval action complete
> + fetch_bars completed
> + get_cli_job completed
> + Eval action complete
> ! get_cli_job failed: cli job 'eval_run_XKI6IWGw5aFZXsqkW3a3' not found
> ! get_cli_job_output failed: cli job 'eval_run_XKI6IWGw5aFZXsqkW3a3' not found

> Sell always says short, even when agent is not shorting, just selling?

> Include total money gained or loss on strategy summary top panel in
> inspector

> PnL shows on backtest, not on paper

> Buy sell orders dont show

> Need back to eval list button for eval inspector (need back to list
> button for all inspectors)

> In trace capsule overlay clicking full screen trace should minimize
> the capsule (since it is now redundant)

> What happens to capsule when multiple evals are running?

> Clone to edit (cant edit clone)

> Hard block for strategy created with no agent for CLI or UI. At
> least 1 agent required.
