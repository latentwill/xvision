# Intake — 2026-05-18 — QA operator round 3 (agent / wizard / runtime feedback)

Operator findings (Ed, 2026-05-18, after the round-2 wave landed). This
batch is heavier on **agent / runtime** behaviour than the prior two
rounds — wizard tool-call failures, strategy-template enforcement, a
trader-output case-sensitivity rejection, broker errors that hard-kill
runs instead of surfacing back to the agent for self-healing — plus
two UX nits (auto-titled chat history, always-visible scrollbars) and
a live-refresh gap on the strategies list.

## Source

Operator chat / wizard session, 2026-05-18 (verbatim findings preserved
at the bottom of this file). Several items reference real `run_id`s
(`01KRWHHBR8FVKM1NVJPQXD4D4B`, `01KRWHY535HCYE14DFPWC7QEGG`) that the
worker can pull from observability for repro.

## Already in flight (do not respawn)

- The trace dock per-call cost display fix shipped in PR #257
  (`qa-budget-cost-precision`). The deeper "per-call cost_usd is
  hardcoded `None` at emission" gap is queued at
  `team/queue/qa-budget-cost-precision__20260518T031142Z__per-call-cost-not-emitted.md`.
- Round-2 tracks are still in flight; this batch does not overlap their
  scope.

## V2 roadmap items (not contracts here)

- A full chat-history persistence layer (multi-session, server-side,
  pinning / archiving / search) is the V2 chat surface, not the
  auto-title scoped here. The auto-title is a small client-side fix
  for the existing history list. The V2 expansion is on
  `team/board-v2.md`.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P3 | Conversation-history list shows only the timestamp — auto-generate a 3-7 word title from the chat content (small-model summarizer, ChatGPT-style) | `chat-history-auto-title` |
| 2 | P1 | Wizard told operator "the API does require a template to create a strategy" — strategy-create path should accept a no-template draft; templates stay as reference examples for the agent, not a hard requirement | `wizard-strategy-template-optional` |
| 3 | P2 | Creating a strategy via the chat rail on the strategies page does not refresh the list — operator must manually reload. Lists need to react to the strategy-created event live | `chat-rail-strategy-list-refresh` |
| 4 | P1 | `create_scenario` tool rejected Qwen's input with `missing field time_window` (x4), `missing field initial`, and `unknown variant type` (calendar) — and the wizard tool-use loop hit the 12-iteration cap. Strengthen the normalizer to repair the shapes Qwen actually produces; surface the failure as one classified error instead of looping | `wizard-scenario-create-tool-repair` |
| 5 | P3 | "Added 100 bars context in scenario, but it still says: Estimated bars to fetch: 0" — bars-estimate UI calc doesn't read the context-bars input | `scenario-bars-estimate-ui` |
| 6 | P1 | Qwen 3.6 trader produced `"action": "Hold"` (title-cased) and the eval errored with `trader output action must be one of long_open, short_open, flat, hold (got Hold)`. Accept case-insensitively (normalize to lowercase before the strict match) or repair in `trader_output.rs` | `trader-output-action-case-insensitive` |
| 7 | P2 | Eval inspector decisions list has macOS auto-hide scrollbars — operator can't tell there's more content below the box. Make scrollbars always-visible on overflow surfaces (decisions list first, then audit other overflow boxes) | `ui-scrollbars-always-visible` |
| 8 | P1 | Broker `[broker_insufficient_funds]` error hard-killed the run instead of being surfaced to the agent as a tool-call error the agent can react to. Engine errors of this class should round-trip back to the agent and into the trace; runs only terminate on un-recoverable errors | `agent-error-feedback-self-healing` |

Eight new tracks. Three integration (touch engine /
dashboard wiring), five leaves. The auto-title and bars-estimate
items are pure frontend.

## Track summaries

### `chat-history-auto-title` (P3, leaf)

After the first 2-3 turns of a new chat thread, generate a 3-7-word
title via a cheap-model dispatch and persist it on the conversation
record. Replace the date-only label in the history list with this
title; keep the timestamp as a secondary subtitle.

**Standard practice reference:** ChatGPT / Claude / Gemini all use the
same pattern — fire a `summarize this conversation in <=7 words, no
quotes` prompt against a small model (haiku, gpt-4o-mini, qwen-7b)
after the first model response, then patch the conversation record.
Title regenerates if the chat radically pivots — but v1 should just
title-once-and-stick (avoid the re-summarize churn).

Frontend-led. Backend may need a tiny `PATCH /api/wizard/threads/:id`
to persist the title; if no thread/conversation table exists today,
file a queue note rather than building one in this contract.

### `wizard-strategy-template-optional` (P1, integration)

Today the wizard's `create_strategy_draft` tool schema requires
`template` (`wizard_loop.rs:1440`) and the underlying authoring path
calls into `authoring::create_strategy_from_template`. Operator wants
templates to be **reference examples for the agent**, not a hard
gate — agents should be able to create a blank/minimal strategy and
fill its fields directly via `set_*` tools.

Two halves:

1. Wizard tool schema: drop `template` from `required`, accept
   `template: null` or omitted, default to a minimal blank strategy
   when no template is named.
2. Underlying `authoring::` path: provide a `create_blank_strategy`
   (or `create_strategy_from_template(None)`) variant that produces
   the same shape a no-op template would.

Keep the existing template path working unchanged — this is purely
relaxing the requirement, not removing templates.

### `chat-rail-strategy-list-refresh` (P2, leaf)

Creating a strategy through the chat rail on `/strategies` invalidates
the React Query cache locally but the list view doesn't re-fetch
until the operator manually reloads. Likely the chat rail's tool
result handler isn't dispatching the right `queryClient.invalidateQueries`
call, OR the list query key doesn't match what the chat rail invalidates.

Audit the chat rail's strategy-create / strategy-update / strategy-delete
result handlers and confirm they invalidate the strategies list query.
Same audit for `/scenarios`, `/agents`, and `/eval-runs` since the
operator complaint implies a category-wide gap.

### `wizard-scenario-create-tool-repair` (P1, integration)

`create_scenario` tool rejected Qwen's payloads four times with
`missing field time_window`, then `missing field initial`, then
`unknown variant type` for the calendar enum. Each failure pushed
the wizard one tool-call closer to the 12-iteration loop cap, after
which the run died with "wizard tool-use loop exceeded 12
iterations — model is stuck calling tools without responding".

Three concrete repair extensions to
`normalize_create_scenario_input` (`wizard_loop.rs:1112`):

1. **Always-synthesize `time_window` fallback.** Today
   `infer_time_window(display_name)` can return `None` and the
   object is sent without the field. Add a default ("last 90 days
   ending today, UTC") so the field is always present.
2. **Repair `capital` shape.** When the agent passes a shape that
   isn't `{ initial: number, currency: string }`, replace with the
   default rather than passing the broken shape through. Extend
   the existing `obj.entry("capital").or_insert_with(...)` to
   actively repair, not just default.
3. **Calendar tag-wrapper unwrap.** When the agent passes
   `calendar: { type: "Continuous24x7" }`, unwrap to bare
   `"Continuous24x7"` before serde sees it (mirrors `unknown
   variant 'type'`). Mirror the existing `normalize_enum_string`
   for tagged shapes.

Plus: when the tool-use loop terminates with the 12-iteration cap,
the operator-visible event should name the *last* tool error (not
just "model stuck") so the next debug pass starts at the failing
schema not a generic loop-detector message.

### `scenario-bars-estimate-ui` (P3, leaf)

`/scenarios/new` (or the chat-rail scenario card) shows
"Estimated bars to fetch: 0" even after the operator sets context
bars to 100. The bars-estimate calc isn't reading the
context-bars input. Audit the bars-estimate selector / memo and
fix the dependency.

### `trader-output-action-case-insensitive` (P1, integration)

Qwen 3.6 trader emits `"action": "Hold"` (title-cased). The strict
match in `crates/xvision-engine/src/eval/executor/trader_output.rs:336`
rejects anything not exactly `long_open|short_open|flat|hold` and
the run errors with `trader output action must be one of long_open,
short_open, flat, hold (got `Hold`)`.

Lowercase the agent-supplied action before the match (and update
the field-level diagnostic at line 346 to reflect the normalized
shape). This is a one-line fix plus tests; do NOT loosen the canonical
vocabulary downstream (the underlying enum stays exact).

Out of scope: pre-validating other trader fields (conviction
range etc.). That's a separate hardening pass.

### `ui-scrollbars-always-visible` (P2, leaf)

macOS auto-hide scrollbars hide the only affordance that tells
operators "more content below". Operator hit this on the eval
inspector decisions list specifically.

Two parts:

1. Eval inspector decisions list — explicit `overflow-y: auto;
   scrollbar-gutter: stable; --scrollbar-color: ...` styling that
   keeps the bar visible on macOS Safari / Chrome and not just on
   hover.
2. Audit the rest of the SPA's overflow surfaces (trace dock,
   span inspector, chat rail history, settings panels) and apply
   the same treatment.

Probably a small global CSS rule plus a couple of per-component
opt-ins. Keep the design subtle — visible bar, not chunky.

### `agent-error-feedback-self-healing` (P1, integration)

Operator's run died with `[broker_insufficient_funds] paper eval
submit_order failed: ... HTTP status 403 Forbidden: insufficient
balance for USD (requested: 2487.87, available: 1807.38)`. The
correct behaviour is **not** to kill the run — the agent should
receive this as a tool-call error and re-decide (smaller size,
flat, or close-first). Only un-recoverable failures (provider
auth gone, db unreachable) should terminate.

Concrete changes:

1. Engine — classify broker errors into `recoverable` vs `fatal`.
   `insufficient_funds`, `rate_limited`, `position_already_open`,
   `min_order_size`, `market_closed` are recoverable. `auth_failed`,
   `network_unreachable_after_retries`, `unsupported_asset` are
   fatal.
2. Wiring — surface recoverable errors back into the agent's next
   turn as a tool-result with `is_error: true` and a structured
   diagnostic the agent can read (not just a stack trace). Trace
   span records the error + the agent's follow-up decision.
3. Trace — render the broker-call error span with severity = `warn`
   (not `error`) when recoverable, so operators can see the
   self-healing chain in the trace dock.

This stacks on `qa-trace-broker-spans` (which adds the broker-call
spans in the first place). Wait for that to merge, then rebase.

Out of scope: retry budgets / circuit-breakers (separate hardening).
The first goal is just "don't kill the run on a recoverable broker
error; show the chain in the trace; let the agent decide".

## Verbatim findings

> Add chat title summary to conversation history instead of simply date
> (agent called for chat should summarize chat, follow standard practice
> for this web search for how to do it)
>
> Chatrail/wizard: "The API does require a template to create a
> strategy" - we do not want the api to restrict agent to templates,
> templates should only be for reference for the agent examples.
> NOT strict.
>
> Creating a strategy in chat rail on strategies screen and the listing
> does not refresh with the strategy. Only shows up on manual refresh.
> Need updates to push live on all lists.
>
> Agent had issues calling tools to make scenario (Qwen):
> [stream error: wizard tool-use loop exceeded 12 iterations — model is
>  stuck calling tools without responding]
> create_scenario failed: missing field `time_window` (x4)
> create_scenario failed: missing field `initial`
> create_scenario failed: unknown variant `type`, expected one of
>   `Continuous24x7`, `UsEquities`, `Custom`
> Calling create_scenario ... [loop]
>
> Added 100 bars context in scenario, but it still says:
> Estimated bars to fetch: 0
>
> Qwer 3.6 eval attempt: ERROR
> [invalid_field] run 01KRWHHBR8FVKM1NVJPQXD4D4B decision 0:
>   trader_output[invalid_field]: trader output action must be one of
>   long_open, short_open, flat, hold (got `Hold`)
>   (stop_reason=EndTurn, input_tokens=12082, output_tokens=1298,
>    raw_excerpt={"action":"Hold","conviction":0.7,
>    "justification":"BTC remains in an established downtrend with no
>    clear reversal signals, making it prudent to maintain the current
>    flat position and wait for stabilization."})
>
> Scrollbar needs to be on by default on decisions in the eval
> inspector once it is longer than box (should the the same for all
> scrollbars so they are visible, otherwise no indicator for user that
> there is more data in the box…)
>
> Error showed up like this: needs to show up in trace and be fed back
> to the agent to adjust their response. In fact all errors like this
> need to be fed to agent? Seems it just kills the run right now and
> does not allow for self healing:
> [broker_insufficient_funds] paper eval submit_order failed:
>   run_id=01KRWHY535HCYE14DFPWC7QEGG decision_index=55 asset=BTC/USD
>   action=long_open side=Buy size=0.03341447973962235
>   reference_price_usd=74454.93: alpaca create_order: rejected by
>   venue: HTTP status 403 Forbidden: insufficient balance for USD
>   (requested: 2487.87, available: 1807.38)
