# Intake — 2026-05-17 — QA operator fix sprint

Operator walk-through findings (Ed, 2026-05-17). All items came in via a
single QA run across the live dashboard surfaces; mostly UI polish + a few
data-fidelity bugs in the eval/trace pipeline plus three discussion items
that need operator direction.

## Source

Operator walk-through, 2026-05-17. Verbatim findings preserved at the bottom
of this file. No runtime validation was executed during the review.

## Already covered (do not respawn)

Three items in the operator's list are already in flight on
`ux-polish-eval-list-and-snapshot` (contract:
`team/contracts/ux-polish-eval-list-and-snapshot.md`, intake:
`team/intake/2026-05-17-ux-polish-eval-list-and-snapshot.md`):

- Chart snapshot on Home shows eval title + date + "latest eval" framing.
- Eval list Scenario/Strategy columns show display names instead of raw ids.
- Scroll indicator on Eval list horizontal axis.

These remain on the active board under that contract.

## Operator-direction items (Decisions board)

Four items require operator decision before any code is written. They land
on `team/decisions.md` rather than as ready contracts:

- Multi-agent strategies expansion (ideonomy direction).
- Multi-asset scenarios.
- How templates drive strategies.
- Agents having multiple agents — design intent clarification.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| A | P2 | Strategy agents page uses popup window; should be accordion. Create+Attach Agent should merge into one box with dropdown for existing | `qa-strategy-popup-to-accordion` |
| B | P3 | Missing whitespace on top of first agent box + manifest box on strategy detail | `qa-ui-micro-fixes` |
| C | P3 | "Run Eval" box redundant (button alone does the trick) | `qa-ui-micro-fixes` |
| D | P3 | Agents page: when adding agent, checkmark used as delete button — should be x or trash | `qa-ui-micro-fixes` |
| E | P2 | Remove per-agent `max_tokens` setting entirely; rely on model-library values | `qa-remove-agent-max-tokens` |
| F | P3 | Remove `POST-HOC⇄LIVE` topbar toggle | `qa-remove-post-hoc-live-toggle` |
| G | P2 | Token cost for OpenRouter models is wrong; pull pricing (and max tokens) from OpenRouter API into model library | `qa-openrouter-pricing-pull` |
| H | P2 | Eval Bar status capsule says "Completed" while the eval is still running (it's labeling the span, not the run) | `qa-eval-running-status-streaming` |
| I | P2 | Running animation not visible in the eval; streaming capsule redundant with running capsule and not animated; trace needs current streaming event surfaced (SSE already wired?) | `qa-eval-running-status-streaming` |
| J | P2 | Trace span shows model as `claude-opus-4-7` instead of the agent's actual model; spans don't show actual agent calls or prompts — only top-level labels | `qa-eval-trace-fidelity` |
| K | P3 | Arrow icons on the eval trace strip too small to read | `qa-eval-trace-fidelity` |
| L | P2 | No way to download entire trace JSON — only per-span | `qa-trace-json-download` |
| M | P1 | Eval run errored `[unclassified] error decoding response body: EOF while parsing a value at line 1145 column 0` and the error did NOT appear in the trace. Operator also uncertain whether trace actually wraps the real LLM call path | `qa-trace-error-surfacing` |

Nine tracks total. All leaves; all parallel-safe with the caveats noted in
each contract.

## Track summaries

### `qa-strategy-popup-to-accordion` (P2, leaf)

The strategy detail page currently opens an agent-attach popup window —
violates the no-popups rule (CLAUDE.md, adopted 2026-05-17). Replace with
an inline accordion / flip-down panel. While there, merge the two attach
surfaces ("Create and Attach Agent" + "Attach Existing Agent") into a
single accordion with a dropdown that lets the operator either pick from
existing library agents or scroll/create a new one inline.

Scope: `frontend/web/src/routes/strategies-new.tsx` and any
component under `frontend/web/src/components/strategy/` that renders the
attach affordance. No engine work.

### `qa-ui-micro-fixes` (P3, leaf)

Three small visual fixes in one PR:

- Add top-of-card whitespace on the first agent box + the manifest box on
  the strategy detail surface (parity with sibling cards).
- Remove the "Run Eval" container card — the inline button is enough; the
  card adds chrome without information.
- On the Agents page Add Agent flow, replace the checkmark "delete" icon
  with `x` or a trash glyph. (Checkmark reads as "confirm", not "remove".)

Scope: pure CSS / JSX cleanup on `strategies-new.tsx` + `agents.tsx` +
`agents-edit.tsx`. No state shape changes.

### `qa-remove-agent-max-tokens` (P2, leaf)

Remove the per-agent `max_tokens` setting from the UI and serializer
defaults. Rely on the model-library values landed by the closed-out
`q15-agent-max-tokens-from-model` track (PR #185). Make sure the engine
falls back to the model-library cap when the persisted agent record has
no override. Need to confirm whether the field stays in the wire schema
for backwards-compat or is removed end-to-end (see Open coordination).

Scope: `frontend/web/src/components/agent/AgentForm.tsx`,
`frontend/web/src/components/agent/SlotForm.tsx`, engine eval dispatcher
fallback.

### `qa-remove-post-hoc-live-toggle` (P3, leaf)

Remove the `POST-HOC⇄LIVE` topbar mode toggle and its trace-dock store
hook. The toggle is at
`frontend/web/src/features/agent-runs/TopbarModeToggle.tsx`; corresponding
store: `frontend/web/src/stores/trace-dock.ts`. Remove the toggle, the
`mode` state, and any conditional rendering that branches on it; default
to whichever behavior the run actually is (live runs stream, completed
runs show post-hoc data).

### `qa-openrouter-pricing-pull` (P2, leaf)

OpenRouter exposes per-model pricing in its `/models` API. Extend the
model-library pull (already pulling max tokens per
`q15-agent-max-tokens-from-model` #185) to also persist input/output
$/Mtok pricing per model. Recompute eval-run token cost in the engine
using the persisted pricing instead of whatever hardcoded fallback is
producing the wrong number today. Anthropic/OpenAI pricing should already
be accurate; this is OpenRouter-specific.

Scope: `crates/xvision-engine/src/llm/registry.rs` (or whichever module
owns the OpenRouter `/models` ingestion) + eval cost calc.

### `qa-eval-trace-fidelity` (P2, leaf)

Two related fidelity bugs on the trace strip:

- Spans display the strategy's default model (`claude-opus-4-7`) instead
  of the per-agent model the slot actually called. Read the per-call
  model from the agent-run-observability event payload (the bus emits the
  model in `tool_call.started` / `slot.completed` events).
- Spans show high-level labels (`plan`, `review`) but not the underlying
  agent calls or prompts. Surface at least the prompt preview + completion
  preview (already in the schema landed by `agent-run-observability-schema`
  #200) inside the span detail.

Plus a small visual nit: enlarge the arrow icons on the trace strip; they
are currently illegible.

Scope: trace-strip rendering under `frontend/web/src/features/agent-runs/`
+ whichever API route shapes the trace payload. Engine-side: confirm the
events carry per-call model id (they do per the schema).

### `qa-eval-running-status-streaming` (P2, leaf)

Three connected status/animation bugs on the eval surface:

- Eval Bar status capsule shows "Completed" while the eval is still
  running. The capsule appears to be labeling the most recent span's
  state, not the run's lifecycle state. Source the pill from
  `run.status` (e.g. `running` / `completed` / `errored`), not the trailing
  span.
- The "running" animation that `eval-running-animation` (PR #193) added
  is not visible during an active run. Probably a CSS / state plumbing
  regression on the eval-run detail surface.
- A separate "streaming" capsule shows up alongside the "running" pill
  and is redundant; collapse into one animated pill driven by run state.
- While we're in here, confirm the SSE channel from
  `agent-run-observability` Phase A actually surfaces the current
  streaming event (token-level or tool-call-level) in the trace; if
  not, wire it in. (Phase A's UI was a separate leaf, so this may be a
  scope-overlap — call out in the contract.)

Scope: eval-runs detail components +
`frontend/web/src/components/primitives/Pill.tsx`. Be careful not to
re-open `Pill.tsx` which was closed by `eval-running-animation`; if a
non-trivial change is needed there, the contract must reclaim ownership.

### `qa-trace-json-download` (P2, leaf)

Add a download button on the trace dock that exports the entire run's
trace as JSON (every span + every event), not just the currently selected
span. Reuse the agent-run-observability event store (`xvision-observability`
crate) — there's already a per-run query for retention/janitor purposes;
add a JSON-export route on the dashboard side.

Scope: a new dashboard route + a download button in the trace dock UI.

### `qa-trace-error-surfacing` (P1, leaf)

Operator hit `[unclassified] error decoding response body: EOF while
parsing a value at line 1145 column 0` on an eval run and it never
appeared in the trace. Plus a higher-order concern: not knowing whether
the trace dock is even wrapping the real Anthropic/LLM call path or
some upstream layer.

Two deliverables:

1. **Error events.** LLM-call failures (provider 5xx, body decode
   error, timeout, classifier output) must land on the
   agent-run-observability bus as trace events carrying the error
   class + message + model id + stop reason, and render with a visible
   error indicator on the failing span in the trace dock.
2. **Trace coverage audit.** Confirm in writing (in
   `team/status/qa-trace-error-surfacing.md`) that the span emission
   wraps the real provider call site, with file:line refs. If the
   wrapping is wrong or off-target, this contract is allowed to fix it
   in `agent/execute.rs` + `llm/**`.

Scope overlaps with the qa-2026-05-17 wave (`qa-execute-slot-cap`
touches `execute.rs`; `qa-role-normalization` touches the executors).
Coordinate disjoint regions; stack via `stacking:` if needed.

## Out of scope

- The four discussion items routed to `team/decisions.md`.
- Anything covered by `ux-polish-eval-list-and-snapshot`.
- New auth or new migrations — none of these tracks need a migration. If
  one is genuinely needed (e.g. for OpenRouter pricing storage), the track
  must reserve a migration number through `team/MANIFEST.md` before adding
  schema changes.
- Architectural rework of the agent-run-observability event bus. Each
  track consumes existing events; if a needed event isn't on the bus yet,
  surface it via `team/queue/` as a coordination note instead of redesigning
  the bus.

## Open coordination notes

- `qa-remove-agent-max-tokens` and `qa-openrouter-pricing-pull` both touch
  the model-library / agent config plumbing. They are kept as separate
  tracks because their concerns are orthogonal (remove a field vs. add a
  field), but workers must coordinate via `team/queue/` if both land
  changes to the same agent serializer.
- `qa-eval-trace-fidelity`, `qa-eval-running-status-streaming`,
  `qa-trace-json-download`, and `qa-trace-error-surfacing` all touch
  the trace dock UI. They are split by surface (per-span content vs.
  run-status pill vs. download button vs. error badge + provider
  call-site wrapping) so each contract claims a disjoint file set.
  `qa-trace-error-surfacing` is the only one of the four that may
  edit engine code; the other three are frontend-leaning. Coordinate
  rebases through `team/queue/`.
- `qa-remove-post-hoc-live-toggle` removes
  `frontend/web/src/features/agent-runs/TopbarModeToggle.tsx`. The
  trace-fidelity and running-status tracks should not also remove this
  file — pass through.
- All trace-related tracks may need cross-references to the
  `agent-run-observability` Phase B contracts on the Reserved list of
  `team/board.md`. If Phase B is decomposed mid-wave, those contracts
  should be checked for path overlaps before claiming.

## Verbatim operator list (preserved for reference)

> Chart Snapshot on Home should show eval title / date and show that it's the latest eval. *[covered by `ux-polish-eval-list-and-snapshot`]*
>
> In Eval list "Scenario" column needs to show scenario title, not Scenario ID. Same with strategy. *[covered]*
>
> Needs to be more obvious that there are scrollable items in Eval list horizontal axis. *[covered]*
>
> Strategy agents there is no white space on top of the first box, same with manifest actually.
>
> In strategy agents, remove pop up window and properly serve data in accordion (flip down menu).
>
> Create and Attach Agent and "Attach Existing Agent" should be merged into one box with a drop down for existing agents.
>
> Run Eval box is redundant and not needed, button does the trick.
>
> Agents can have multiple agents? On Agents page, when adding agent, checkmark is delete button should be x or trash. Not sure on thinking behind multi agent agents. *[decisions.md + qa-ui-micro-fixes]*
>
> Follow up — multi agent strategies expansion (ideonomy). *[decisions.md]*
>
> Follow up — multi asset strategies on scenarios? *[decisions.md]*
>
> Follow up — how templates drive strategies. *[decisions.md]*
>
> Can we remove max token setting all together from agent? Should be super easy. We don't want to run into situations where say for example 4096 is set when model can do 384k+.
>
> REMOVE: POST-HOC⇄LIVE.
>
> Eval Bar Capsule says "Completed" when eval is still running.
>
> Token cost is NOT ACCURATE for model from OpenRouter; pricing info needs to be pulled too.
>
> Arrow icons on the eval trace strip are way too small to read.
>
> In trace shows model as claude-opus-4-7 instead of the agent model.
>
> Running animation is not visible in the eval; trace needs to show current streaming event.
>
> Span needs to show actual agent calls? Decision shows real values but no prompts in the trace.
>
> Streaming capsule in eval is redundant with running capsule, and also not animated.
>
> Need way to download entire trace JSON, not just span.
>
> Also: eval run errored `[unclassified] error decoding response body: EOF while parsing a value at line 1145 column 0` — note DID NOT SHOW IN TRACE! These errors need to be in trace so we can debug. I'm not even sure what trace is doing right now, I hope it doesn't actually call anthropic. *[qa-trace-error-surfacing]*
