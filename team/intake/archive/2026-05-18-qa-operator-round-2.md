# Intake — 2026-05-18 — QA operator fix sprint, round 2

Operator walk-through findings (Ed, 2026-05-18). Second pass over the
dashboard surfaces after the 2026-05-17 QA wave landed. Most items are
trace/eval polish bugs that survived round 1, plus a few data-fidelity
bugs in decisions/PnL/cost display and one P1 review-agent regression.

## Source

Operator walk-through, 2026-05-18. Verbatim findings preserved at the
bottom of this file. No runtime validation was executed during the
review.

## Already in flight (do not respawn)

Three items overlap existing contracts:

- **"Stop eval button still big"** and
  **"run/strategy/scenario/View agent trace → redundant info, remove"**
  are both covered by `eval-inspector-header-polish`
  (`team/contracts/eval-inspector-header-polish.md`, status `ready`).
  Operator reports these were not yet fixed — that contract is `ready`,
  not `merged`. A queue note has been filed so the worker who claims
  it confirms scope still matches the user-visible state.
- **"Still showing redacted spans despite full_debug. Responses show up
  but not prompts!"** partially overlaps
  `observability-retention-default-full-debug` (status `claimed`). That
  contract only flips the default. The asymmetric prompt-vs-response
  bug is a separate retention/storage issue and gets its own track
  (`qa-retention-prompt-storage-bug`).
- **"Need way to add the actual call body when streaming"** — already
  covered by `model-call-streaming-text-passthrough` (status `claimed`).
  No new track needed.

## V2 roadmap items (not contracts)

- **"User should be able to set up their own review agent in v2 (part of
  expanding and evaluating agent types piece)"** — routes to
  `team/board-v2.md` follow-ups section. A user-configurable review
  agent is part of the agent-types expansion arc; do not freelance a
  contract until V2B/V2C intake reaches it.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P1 | Cancelled run still shows capsule with timer kept running (timer doesn't honor cancel) | `qa-eval-action-lifecycle` |
| 2 | P2 | Capsule shows on eval/other pages after navigating from inspector, disappears on refresh (capsule state leaks across routes) | `qa-eval-action-lifecycle` |
| 3 | P3 | Latest run chart on Home needs evaluation name on top of "latest eval" framing | `qa-ui-polish-round2` |
| 4 | P3 | Agents page has "Show archived" toggle but no delete affordance | `qa-ui-polish-round2` |
| 5 | P1 | Can't retry a cancelled eval (Retry button no-op or hidden when `status=cancelled`) | `qa-eval-action-lifecycle` |
| 6 | P2 | Need a Delete-eval action in the eval inspector (today the only path is the list view) | `qa-eval-action-lifecycle` |
| 7 | P1 | Full-debug retention not surfacing prompts (responses appear; prompts redacted/missing — asymmetric) | `qa-retention-prompt-storage-bug` |
| 8 | P2 | Broker calls (Buy / Sell / Close / Short) must emit trace spans, not only the model calls | `qa-trace-broker-spans` |
| 9 | P3 | Streaming icon shows up twice in the trace span inspector | `qa-ui-polish-round2` |
| 10 | P3 | Retention warning copy is too much; remove the loud warning surface | `qa-ui-polish-round2` |
| 11 | P2 | Trace dock "Full" button and the fullscreen arrows are redundant. Replace with a resizable dock | `qa-trace-dock-resizable` |
| 12 | P2 | On Decisions: CLOSE and HOLD ambiguity (e.g. short opens then next bar CLOSE flat is unclear). Add per-span open-position tracking | `qa-decisions-position-pnl` |
| 13 | P2 | TradingView chart titles did not show up (missing title overlay on the chart pane) | `qa-ui-polish-round2` |
| 14 | P2 | Short sale does not show fill on the decisions row (broker fill not propagating into the row) | `qa-trace-broker-spans` |
| 15 | P2 | Budget cost shows `$0.0000` for cheap models — need more decimal places AND validate token prices flow end-to-end | `qa-budget-cost-precision` |
| 16 | P2 | PnL not filling in on decisions (orders closing not reflected on the decision rows) | `qa-decisions-position-pnl` |
| 17 | P1 | Only 29 decisions show up for a 30-day strategy (off-by-one, last bar dropped, or pagination cap) | `qa-decisions-30day-count` |
| 18 | P1 | Review agents broken: "agent profile `research-agent` references provider `anthropic` which is not configured in Settings → Providers" | `qa-review-agent-provider-config` |

Nine new tracks. All but one are leaves. `qa-trace-broker-spans` and
`qa-decisions-30day-count` are integration tracks because they touch the
engine.

## Track summaries

### `qa-eval-action-lifecycle` (P1, leaf)

Four related bugs on the eval lifecycle / row-action surface:

- Cancelled run capsule's timer keeps counting (#1) — capsule state
  doesn't transition from `running` to `cancelled` on the timer
  source.
- Capsule appears on eval list and other pages after navigating from
  inspector and disappears on refresh (#2) — capsule store is keeping
  a stale running ref alive after the inspector unmounts. Suggests
  a route-scoped subscription that should be tied to a route guard or
  a store reset on unmount.
- Retry on cancelled run is broken (#5) — `cancelled` lifecycle state
  must be re-runnable; today the Retry affordance either no-ops or is
  hidden.
- Add Delete-eval action in the eval inspector (#6) — today delete is
  only on the list. Inspector needs symmetric Stop / Retry / Delete /
  Download buttons (the Stop/Retry width fix is owned by
  `eval-inspector-header-polish`; this track adds the Delete button).

Frontend-only unless the Retry-on-cancelled flow uncovers a backend
gate (e.g. eval-runs API rejects re-run of `cancelled` rows). If so,
the contract may pull a small slice of `crates/xvision-engine/src/api/eval.rs`
under coordination with `observability-retention-default-full-debug`.

### `qa-retention-prompt-storage-bug` (P1, leaf)

Operator set retention to `full_debug` and confirmed responses show
up but prompts do not. That is asymmetric — either:

- The prompt redactor is firing even in `full_debug` mode (bug in
  `xvision-observability/src/redactor.rs` mode-gating), OR
- The prompt payload is being written but the SpanInspector "redacted
  prompt" fallback path is rendering instead of the body (UI keying
  bug), OR
- Migration 018 stored prompts in a column the dashboard read route
  doesn't surface.

Investigate, file the root cause in `team/status/qa-retention-prompt-storage-bug.md`,
then fix. Includes a regression test that asserts a full-debug run
round-trips both prompt and response bodies through the dashboard
fetch route.

Depends on `observability-retention-default-full-debug` (claimed). If
that contract changes the default and the bug disappears for new runs
but stale `hash_only` rows still render incorrectly in the UI,
document the data-flush requirement in the status note.

### `qa-review-agent-provider-config` (P1, leaf)

Operator hit: "agent profile `research-agent` references provider
`anthropic` which is not configured in Settings → Providers." Review
agents (the agent-run reviewer pass) currently reference a hardcoded
`anthropic` provider key but the installed dashboard has no Anthropic
provider configured. Two outcomes acceptable for v1 fix:

1. **Skip-or-warn fallback.** If the referenced provider isn't
   configured, the review pass logs a single warning and degrades
   gracefully (no review output) rather than failing the run.
2. **Provider-aware default.** Resolve `research-agent` against
   whichever provider has at least one configured model; if none,
   degrade per (1).

Out of scope (V2 work): operator-configurable review-agent profile UI.
That is roadmap, parked on `team/board-v2.md`.

### `qa-decisions-30day-count` (P1, integration)

Operator reports a 30-day strategy produced only 29 decisions. Likely
off-by-one in the scenario bar-slicing (inclusive vs exclusive end
date), or the eval loop drops the last bar, or pagination caps at 29.
Investigate, fix the root cause (no silencing), add a test asserting
that a 30-bar scenario yields 30 decisions.

Likely surface: `crates/xvision-engine/src/eval/dispatcher.rs` and
`crates/xvision-engine/src/eval/executor/backtest.rs`. May touch
scenario bar iteration in `crates/xvision-engine/src/data/`. Contract
allows engine work but no migrations.

### `qa-trace-broker-spans` (P2, integration)

Trace currently shows model.call spans but not broker calls. Operator
wants Buy / Sell / Close / Short submissions visible as spans (#8),
including short-sale fills (#14, which is the visible symptom of the
same emission gap — the executor knows the fill happened but it
doesn't reach the trace).

Add `broker_call.{started,finished,failed}` events on the
agent-run-observability bus from `crates/xvision-execution/**` (Alpaca,
Orderly) and from `crates/xvision-engine/src/eval/executor/paper.rs`.
Frontend renders them as a new span kind with side (Buy/Sell/Close/Short),
qty, intended price, fill price, status.

Coordinates with `alpaca-paper-crypto-submit` (ready) which is rewiring
the same broker surface for paper crypto. Either stack or wait;
contract declares stacking explicitly when claimed.

### `qa-decisions-position-pnl` (P2, leaf)

Two related decisions-surface bugs:

- CLOSE / HOLD rows are ambiguous because the row doesn't show the
  open-position state (#12). After a short-open then a CLOSE flat
  bar, the operator can't tell if the position is still open. Add a
  per-row "open positions" cell that lists active positions at the
  end of that bar (symbol, side, qty, entry, mark, unrealized).
- PnL not filling in on close (#16) — order closes don't propagate
  unrealized → realized PnL into the decision row.

Likely surface: decisions rendering under `frontend/web/src/features/decisions/`
or wherever the decisions table lives, plus the engine path that
computes per-decision PnL (`crates/xvision-engine/src/eval/`). If the
position state is already in the eval result and the bug is purely
display, this stays a leaf. If position state needs to be computed
afresh, scope creeps to integration — file a contract update.

### `qa-budget-cost-precision` (P2, leaf)

Budget surface shows `$0.0000` for cheap models. Two concerns:

- **Display precision.** Per-call costs at OpenRouter prices can be
  in the $0.000001 range. Round-to-4-decimals truncates them to
  zero. Switch the per-call cost cell to a smart formatter (e.g.
  4-significant-figures with scientific notation for very small
  values, or `<$0.0001` with full precision on hover).
- **Validation.** Confirm token prices are actually flowing from the
  model library into the cost computation. `qa-openrouter-pricing-pull`
  added the pricing pull from OpenRouter; this contract verifies it
  is being consumed by the budget UI and that the per-call event
  carries token counts the cost calc can multiply. If a gap exists,
  fix it.

Frontend-only display fix; the validation half is investigative.

### `qa-trace-dock-resizable` (P2, leaf)

The trace dock has a "Full" button and a fullscreen-arrows button
that operator finds redundant. Replace with a resizable dock: a drag
handle on the top edge that the user can grab to set the dock height
(persisted to local storage). Keep one fullscreen affordance (the
pop-out to `/agent-runs/:runId` route), drop the "Full" duplicate.

Stacks behind `trace-dock-ux-polish` (claimed). Contract declares
stacking explicitly when claimed; rebases after that lands.

### `qa-ui-polish-round2` (P3, leaf bundle)

Five small visual fixes in one PR (none worth their own track):

- Latest run chart on Home shows evaluation name (#3). May overlap
  the `ux-polish-eval-list-and-snapshot` track — confirm the bug
  is on the latest-run chart specifically vs. the eval list rows.
- Agents page: add a Delete affordance next to / inside the
  "Show archived" view (#4). Today archive → can't delete.
- Trace span inspector: remove the duplicate streaming icon (#9).
- Trace UI: remove the loud retention warning surface (#10). See
  user feedback memory `feedback_no_privacy_overkill` — retention
  card on Settings stays minimal; the trace-dock warning is the
  one to remove here.
- TradingView chart titles missing (#13) — add the title overlay
  to the chart pane.

## Out of scope

- Anything covered by `eval-inspector-header-polish`,
  `observability-retention-default-full-debug`, or
  `model-call-streaming-text-passthrough` (existing contracts).
- Architectural rework of the agent-run-observability bus. New
  events (broker_call) extend; the bus shape is fixed.
- New migrations unless `qa-trace-broker-spans` or
  `qa-decisions-30day-count` discovers a missing column. Either
  must reserve a migration number through `team/MANIFEST.md` first.
- User-configurable review-agent profiles (V2 roadmap; parked on
  `team/board-v2.md`).

## Open coordination notes

- `qa-trace-broker-spans` and `alpaca-paper-crypto-submit` both edit
  `crates/xvision-execution/**` and `crates/xvision-engine/src/eval/executor/paper.rs`.
  `paper.rs` is a CONFLICT_ZONE row owned by alpaca-paper-crypto-submit.
  qa-trace-broker-spans must declare `stacking: declared:alpaca-paper-crypto-submit`
  before claiming.
- `qa-trace-dock-resizable` and `trace-dock-ux-polish` both edit
  `frontend/web/src/features/agent-runs/TraceDock.tsx`. Must stack.
- `qa-eval-action-lifecycle` and `eval-inspector-header-polish` both
  edit `frontend/web/src/routes/eval-runs-detail.tsx` (already an
  OWNERSHIP row claimed by eval-inspector-header-polish). Stack or
  wait.
- `qa-retention-prompt-storage-bug` depends on
  `observability-retention-default-full-debug`. Workers should sync
  via `team/queue/` before either lands.
- `qa-ui-polish-round2` may overlap `ux-polish-eval-list-and-snapshot`
  on the latest-run chart label. Confirm disjoint files before
  claiming. If they truly overlap, fold #3 into the existing track
  and drop from the bundle.

## Verbatim operator list (preserved for reference)

> Cancelled run still showed in capsule as a timer kept running
>
> Capsule shows up on eval screen or other pages when navigating from inspector, disappears on refresh
>
> Latest run chart needs evaluation name
>
> "Show archived" on agents but no delete
>
> Cant retry cancelled eval
>
> Not fixed: run 01KRVFZ3YJ...strategy 01KRQGPDHF...scenario sc_01KRQGQ...View agent trace → - redundant info, remove
>
> Stop eval button still big
>
> Need option to delete eval in eval inspector
>
> Does redacted retention even work?
>
> Still showing redacted spans despite setting full debug. Maybe need to flush db? Responses show up but not prompts!
>
> Buy / Sell / Close etc calls need to be on trace, not just the model calls
>
> Streaming icon shows up twice in trace span inspector
>
> Retention warning is too much, remove it.
>
> For trace dock: "Full" and "full screen arrows" are not really equivalent. Would prefer to have the dock resizable.
>
> On Decisions: CLOSE and HOLD are unclear, for example if short sell opens then next candle is CLOSE flat it is not clear that position is still open. Need some sort of position tracking data per span that shows all open positions.
>
> Trading View Chart Titles did not show up.
>
> Short Sale does not show fill.
>
> Need to add more decimal places for low cost api models in budget - validate that token prices are flowing because cost just shows $0.0000
>
> PnL not filling on decisions I think? Cant see orders closing.
>
> Only 29 decisions showed up for 30 day strategy
>
> Review agents are broken: request: agent profile `research-agent` references provider `anthropic` which is not configured in Settings → Providers.
>
> User should be able to set up their own review agent in v2 (part of expanding and evaluating agent types piece)
