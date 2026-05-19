# Intake — 2026-05-19 — QA operator round 4 (re-run, strategy edit, review 400)

Operator findings (Ed, 2026-05-19) after the round-3 wave landed and the
F-6/F-7 harness observability tracks wrapped. Three items, all on
post-create / post-run mutability surfaces that today are one-way doors:
completed evals can't be re-run, strategies can't be edited after
creation, and the review-agent endpoint returns 400 on a completed run
without a useful surfaced reason.

## Source

Operator session, 2026-05-19. Verbatim console capture for the review
400 is preserved at the bottom of this file (real run id
`01KRXY73XAE2NR65YVKJZ28JBK`). Items 1 and 2 are product gaps reported
verbally — confirmed against backend route surface
(`crates/xvision-dashboard/src/server.rs` / `routes/eval_runs.rs` /
`routes/strategies.rs`).

## Already in flight (do not respawn)

- The retry-on-cancelled gap was scoped under
  `qa-eval-action-lifecycle` (PR #260, merged 2026-05-18). That track
  expanded the retry-eligible set to `failed | cancelled`. It does **not**
  cover `completed`, which is item 1 here. Confirm the contract for item 1
  treats this as an additive scope, not a respawn.
- `qa-review-agent-provider-config` (PR #256, merged 2026-05-18) made
  provider misconfiguration on the review agent surface as a classified
  remediation error, not a raw 400. Item 3 below post-dates that merge —
  the 400 surfacing today is either a different validation branch or a
  regression. Investigate before scoping a fix.

## V2 roadmap items (not contracts here)

- A general "strategies are first-class editable artifacts" surface
  (versioning, audit trail, draft vs published) is V2/V3 territory. The
  intake here is scoped to **editing the existing fields the create
  flow already exposes** (title, description, top-level metadata) — not
  to a strategy-versioning system.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| 1 | P2 | Cannot re-run a completed eval. Today `POST /api/eval/runs/:id/retry` returns 400 unless source status is `failed` or `cancelled`. Operator wants to re-run a completed run with the same `(agent_id, scenario_id, mode, params_override)` inputs to get a fresh trace / re-test a fix. | `eval-rerun-from-completed` |
| 2 | P1 | Cannot edit a strategy after creation. `/api/strategy/:id` only exposes `GET` + `DELETE`; slot, agents, pipeline, and risk have `PUT`/`PATCH`, but top-level fields (title, description, tags) have no update route at all. Operator hits this on any typo in the create wizard — only escape is delete-and-recreate. | `strategy-edit-top-level-fields` |
| 3 | P1 | Review-agent button on a **completed** eval run (id `01KRXY73XAE2NR65YVKJZ28JBK`) returns `POST /api/eval/runs/<id>/review → 400 Bad Request`. The frontend surfaces no actionable error; the operator-visible state is "click does nothing". Either the backend Validation branch lacks a remediation hook (post-#256 regression) OR a new failure mode is firing that #256 didn't anticipate. | `eval-review-400-diagnose` |

Three new tracks. Two integration (engine/dashboard route work for items
1+2), one diagnosis-then-fix (item 3 — start as investigation, scope the
fix once the failing branch is known).

## Track summaries

### `eval-rerun-from-completed` (P2, integration)

Today the retry route gates on source status. From
`crates/xvision-dashboard/src/routes/eval_runs.rs:109`:

> Returns `202 Accepted` with the freshly-persisted `RunDetail`
> (status = `Queued`). `400` if the source isn't in a `failed` state;
> idempotent on the source's `(agent_id, scenario_id, mode)`
> fingerprint while a previous retry is still queued or running.

After PR #260 (`qa-eval-action-lifecycle`) the eligible set was widened
to `failed | cancelled`. The frontend reflects this:

```ts
// frontend/web/src/routes/eval-runs-detail.tsx:368
const canRetry = summary.status === "failed" || summary.status === "cancelled";
```

Operator wants a `completed` run to be re-runnable too — the goal is "re-test
the same agent against the same scenario, get a fresh trace, see if the
result is stable". This is **not** A/B compare (different agents/scenarios)
and **not** a fingerprint-dedup case (the operator explicitly wants a new
run).

Concrete scope:

1. Engine: relax `eval::retry`'s status gate to include `completed`.
   Keep the running-fingerprint idempotency for queued/running retries
   so a double-click can't fan out new runs. Decide whether the
   `failed → retry` ergonomics (auto-classify the failure cause, link
   the lineage) need to extend to `completed → rerun` — for completed,
   the lineage is just "this was a deliberate rerun".
2. Dashboard route doc-comment: re-document the 400 conditions.
3. Frontend: widen `canRetry` to include `completed`. Decide whether
   the button label should be "Rerun" instead of "Retry" when the
   source is `completed` (Retry implies failure; Rerun is what the
   operator actually wants). At a minimum, surface a tooltip
   distinguishing the two semantics.
4. Test: the `eval-runs-detail` test suite already covers
   failed/cancelled retry — add `completed` to that matrix.

Out of scope: queue-level deduplication policy changes, multi-run
batching, lineage UI beyond what the existing summary already shows.

### `strategy-edit-top-level-fields` (P1, integration)

`crates/xvision-dashboard/src/server.rs:47-66` registers:

```rust
.route("/api/strategy/:id", get(strategies::get).delete(strategies::delete))
.route("/api/strategy/:id/slot/:role",  put(strategies::put_slot))
.route("/api/strategy/:id/agents",      post(strategies::post_add_agent))
.route("/api/strategy/:id/agents/:role", delete(...).patch(...))
.route("/api/strategy/:id/pipeline",    put(strategies::put_pipeline))
.route("/api/strategy/:id/risk",        put(strategies::put_risk))
```

No `PUT`/`PATCH` on `/api/strategy/:id` for top-level fields (title,
description, tags, default-model). The wizard's `create_strategy_draft`
tool persists these on creation, but there is no edit path. Operator
can only fix a typo by deleting and recreating, which loses the strategy
id and breaks any agent run that references it.

Concrete scope:

1. Engine: add `strategy::update_metadata(id, patch)` taking a
   partial `{ title?, description?, tags?, default_model? }`. Validate
   the same constraints `post_create` enforces (title non-empty, unique
   if the create path uniqueness-checks). Do **not** allow editing
   `agent_id` / created_at / agents / risk / pipeline through this
   route — those have their own dedicated routes.
2. Dashboard: register `PATCH /api/strategy/:id` (preferred — semantic
   match: partial update) routing to `strategies::patch_metadata`.
3. Frontend: add an inline-edit affordance for title + description
   on `/strategies/:id`. Per the no-popup rule, this must be inline
   (text-input replaces the heading on click) — no modal, no sheet.
4. Test: HTTP-level test for the patch route + a cycle-id-stable
   round-trip (edit title, confirm subsequent runs still resolve the
   same strategy_id == agent_id ULID).

Open questions for the contract writer:

- Does the marketplace pivot (`agent_id` becomes NFT token id) make
  the title a publish-time-frozen attribute? If so, editing it
  pre-publish is fine but should be locked post-mint. Check ADR
  0010. Reasonable v1 answer: top-level fields are editable
  pre-publish, no-op post-mint.
- Should the existing `xvn strategy` CLI gain an `edit` verb in the
  same wave? Probably yes, for parity, but not blocking.

### `eval-review-400-diagnose` (P1, investigation→fix)

Operator hit this on a completed run. Console capture:

```
POST https://xvn.tail2bb69.ts.net/api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK/review 400 (Bad Request)
[xvn:api] api.request.error { route: '/eval-runs/01KRXY73XAE2NR65YVKJZ28JBK',
  trace_id: '4d5d9287', request_id: 'e924be5a', method: 'POST',
  path: '/api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK/review' }
```

The frontend request body is fixed:
`{ agent_profile_id: <selected>, force: true }`
(`frontend/web/src/features/eval-runs/review/ReviewPanel.tsx:52`).

The dashboard route has multiple Validation→400 branches in
`crates/xvision-dashboard/src/routes/eval/review.rs`:

- Line 109: `agent profile <id> is disabled`
- Line 259: agent profile disabled (engine-side)
- Line 261: `RunNotCompleted` — should not fire here, run is
  completed, but worth verifying the status the route sees matches
  the status the UI sees.
- Line 296: `load config` failed.
- Line 338: skip-with-remediation (provider not configured →
  classified error, the path #256 made operator-readable).

The 400 here did **not** surface a remediation message — either:

- The frontend isn't reading `body.error` / `body.code` (regression
  in the review panel's error display).
- The Validation branch firing predates / bypasses the remediation
  formatter that #256 added.
- A different ApiError type is being mapped to 400 with an empty
  body.

Work plan (investigation → patch):

1. Reproduce on `xvn.tail2bb69.ts.net` against the same run id with
   `curl -i ... | jq` to capture the JSON response body (status code,
   `error.code`, `error.message`). Compare against the operator's
   browser console.
2. Inspect `DashboardError` → response serialization
   (`crates/xvision-dashboard/src/error.rs`) for the exact shape on
   the failing branch.
3. If the body has a useful message but the UI is dropping it:
   patch `ReviewPanel.tsx` to surface `error.code` + `error.message`
   not just `error.message ?? String(error)`.
4. If the branch firing genuinely lacks a remediation hook:
   add one in line with the #256 pattern.
5. Add a regression test against the failure mode discovered.

Out of scope: deeper review-engine refactors, multi-profile review
fan-out, the V2 user-configurable review agent (still on
`team/board-v2.md`).

This track starts as investigation. Acceptance criterion is that
clicking "Review with: <agent>" on a completed run either succeeds OR
shows an operator-actionable error (provider not configured, agent
disabled, etc.) — never the silent 400 the operator sees today.

## Verbatim findings

> Cannot retry completed eval
> Cannot edit strategy after being created title etc
>
> Review agent still gives
> POST https://xvn.tail2bb69.ts.net/api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK/review 400 (Bad Request)
>   ... [stack trace]
> [xvn:api] api.request.error {route: '/eval-runs/01KRXY73XAE2NR65YVKJZ28JBK',
>   trace_id: '4d5d9287', request_id: 'e924be5a', method: 'POST',
>   path: '/api/eval/runs/01KRXY73XAE2NR65YVKJZ28JBK/review', …}
> [xvn:mutation] mutation.error {route: '/eval-runs/01KRXY73XAE2NR65YVKJZ28JBK',
>   mutation_key: undefined, error: {…}}

## Round-4 addendum — 2026-05-19 session

Three additional findings from the same operator session, after the
round-4 items above were captured. Two new product gaps (one frontend,
one engine) and one unfinished-handoff carry-over from the harness wave
that surfaces as a user-visible regression in the trace dock.

### Findings → tracks (addendum)

| # | Severity | Finding | Track |
|---|---|---|---|
| 4 | P2 | Dynamic-import chunk hash mismatch after deploy. Operator hit `TypeError: Failed to fetch dynamically imported module: https://xvn.tail2bb69.ts.net/assets/scenarios-new-5cnT8cD7.js` navigating to the scenarios route. Hard refresh fixes it (new build = new `index.html` with new hashes). One-off symptom, but the SPA does nothing automatic — the operator has to know to refresh. | `stale-chunk-import-retry` |
| 5 | P1 | Eval loops on a deterministic broker rejection. A paper-venue ETH/USD run repeatedly emitted size ≈0.00274 ETH (~$6 notional), each cycle rejected by Alpaca with `error_class=broker_min_order_size` ("cost basis must be >= minimal amount of order 10"). Run kept producing identical rejections until the operator cancelled. No abort path on repeated identical broker errors; no pre-submit gate on `min_notional`. | `eval-broker-error-circuit-breaker` (+ root-cause `risk-gate-min-notional`) |
| 6 | P1 | `full_debug` trace dock still shows `prompt body not captured for this run — re-run to capture` on every run. PR #282 made the placeholder copy honest; PR #277 wired real SHA-256 hashes. Neither shipped the actual payload-blob write — `crates/xvision-engine/src/agent/observability.rs:255-256` still hardcodes `prompt_payload_ref: None` / `response_payload_ref: None`, and `BlobStore::write` has zero production callers. The handoff queued from #282 was partially executed by #277 (hash digest) but the blob-write half was dropped. | `harness-payload-blob-write` |

### Track summaries (addendum)

#### `stale-chunk-import-retry` (P2, frontend)

Vite SPA emits hashed chunk filenames. After a deploy, an open tab still
holds the old `index.html` referencing the old hash; navigating to a
lazy-loaded route fails with `Failed to fetch dynamically imported
module`. Refresh recovers because the new `index.html` ships new hashes.

Concrete scope:

1. Add a global `import()` error trap on lazy route boundaries (or in
   the router's error boundary): on `TypeError: Failed to fetch
   dynamically imported module`, hard-reload the page once per
   session (guard via `sessionStorage` to avoid reload loops).
2. Surface a one-shot toast on the post-reload load: "App was updated
   — reloaded to the latest version" so the operator knows what
   happened.
3. Test: a unit-level test against the error-boundary detecting the
   chunk-import error class; a manual repro doc in the track status
   note covering the deploy → open-tab → navigate flow.

Out of scope: build-id polling on focus/route-change (better UX —
prompt to update before failure, not after — but a larger track; defer
to a follow-up unless this fix lands trivially).

Note: this is **operator-visible** but recoverable with refresh. P2,
not P1.

#### `eval-broker-error-circuit-breaker` (P1, engine — paired with `risk-gate-min-notional`)

Today the eval loop submits each decision-cycle's order without any
ratchet on repeated rejection. The operator-reported run produced
identical `broker_min_order_size` rejections every cycle (ETH/USD,
~0.00274 size, ~$6 notional, $10 minimum), with no abort path.

Two paired fixes — both worth doing; the root-cause one is higher
leverage:

**Root cause — `risk-gate-min-notional` (preferred):** the risk gate
should know each venue's `min_notional` and veto pre-submit. Today the
order is sized by the trader/risk path with no awareness of the
broker's deterministic constraint, so every cycle pays the broker
round-trip to learn what we already know. Concrete scope:

1. Add `min_notional_usd: Option<f64>` to the broker capability /
   venue config. Paper Alpaca = 10.0; live Alpaca = 1.0 (verify
   against Alpaca docs); other venues per their published minimums.
2. Risk gate: if `intended_price * qty < min_notional`, emit a
   `RiskDecision::Vetoed { reason: "below_venue_min_notional", … }`
   and skip the broker call. Surface as a normal risk-veto in the
   run, not a broker error.
3. Optionally: a "round up to min" mode (config flag, default off)
   that bumps qty to the minimum-notional threshold instead of
   vetoing — useful in paper, less useful in live.

**Defense-in-depth — `eval-broker-error-circuit-breaker`:** even with
the risk-gate fix, a different deterministic broker error class could
surface in the future. The eval run loop should abort on repeated
identical errors so a misconfigured run doesn't burn the operator's
session. Concrete scope:

1. In the eval run loop, track `consecutive_count` per `error_class`
   on broker-rejected outcomes (severity ≥ warn, outcome = rejected).
2. On threshold (N=3, configurable via run config), abort the run
   with `RunStatus::Failed { reason: "repeated_broker_error",
   error_class, count, last_error_message }`.
3. Run summary surfaces this: trace dock shows the abort cause; eval
   list shows the run as failed with a one-line classified reason.
4. Reset the counter on a successful broker outcome (`ok` /
   `accepted` / `filled`) — don't accumulate across the run.

Out of scope for both: queue-level retry, batching, multi-venue
failover, sizing policy redesign.

Tradeoff on the circuit-breaker: a hard threshold can kill a run that
would have self-corrected on the next decision cycle if the sizing
logic adapts on its own. Mitigation — gate on `severity ≥ warn` AND
identical `error_class` (different error classes don't accumulate
together); `degraded` and partial fills don't trip.

Order of operations: ship the root-cause `risk-gate-min-notional`
first (eliminates the immediate operator-reported failure mode); the
circuit-breaker can land in parallel or one wave later as the safety
net for unknown future deterministic broker errors.

#### `harness-payload-blob-write` (P1, harness — unfinished handoff)

The trace dock has been showing the "re-run to capture" placeholder
for prompt + response bodies on every `full_debug` run since PR #282
made the copy honest. Investigation in #282 identified the producer
gap (`prompt_payload_ref: None` / `response_payload_ref: None`
hardcoded) and queued the wire-in to the harness wave; PR #277 picked
up the hash-digest half but the blob-write half was dropped.

`crates/xvision-observability/src/blobs.rs::BlobStore::write` has
**zero production callers** — only tests. `BlobStore` is a finished
component waiting for a producer.

Concrete scope:

1. **`crates/xvision-engine/src/agent/execute.rs`** — at the same
   call site where #277 computes `prompt_hash` from `&req` and
   `response_hash` from `assistant_text`, additionally call
   `BlobStore::write` for the serialized request payload and the
   accumulated response text when `retention == FullDebug`. Capture
   the returned `BlobRef`s.
2. **`crates/xvision-engine/src/agent/observability.rs`** —
   `emit_model_call_finished` already carries `prompt_payload_ref`
   and `response_payload_ref` fields on `ModelCallFinishedEvent`.
   Update the signature to accept `Option<BlobRef>` for each, thread
   them through from the caller in `execute.rs`, and drop the
   hardcoded `None` literals at lines 255-256.
3. **Retention gate** — only write under `FullDebug`. Under
   `redacted` and `hash_only`, leave the refs `None` (the placeholder
   copy in `SpanInspector.tsx` already handles those modes correctly
   per the table in #282's PR body). Under `redacted`, the prompt
   payload must go through the existing `PayloadRedactor` before
   write (do not bypass redaction even on full_debug-equivalent
   paths).
4. **Dashboard fetch route** — verify
   `crates/xvision-dashboard/src/routes/agent_runs.rs::blob` already
   serves `BlobRef` lookups end-to-end. The route exists (it's in
   the scope of #282's allowed_paths) but with no production
   producer it's been exercised only by tests.
5. **Test** — `crates/xvision-engine/tests/agent_observability_blob.rs`
   mirroring the hash test pattern:
   - full_debug run writes both blobs; refs are populated on the
     emitted event; sqlite has the refs; dashboard fetch route
     returns the bodies; SpanInspector renders them (component test).
   - hash_only run leaves both refs `None`; SpanInspector still
     shows the hash-only placeholder.
   - redacted run: prompt payload runs through the redactor before
     write; secrets are scrubbed in the persisted blob.

Out of scope: blob retention/cleanup policy (a separate observability
track), schema changes (`blob_ref` columns already exist on the
sqlite `spans` table), the V2 streaming-prompt live path (response
already has one via `emit_assistant_text_delta`; prompts don't need
streaming).

This is the **highest-leverage** of the three addendum items —
without it the trace dock cannot show what the agent was actually
told and what the model actually returned, which is the operator's
primary debugging surface.

Acceptance: SpanInspector renders the actual prompt body and
completion body on `full_debug` runs, with no "re-run to capture"
placeholder firing. Hashes and bodies both populated for runs created
after the fix lands.

## Verbatim findings (addendum)

> QA Intake: Unexpected Application Error!
> Failed to fetch dynamically imported module:
>   https://xvn.tail2bb69.ts.net/assets/scenarios-new-5cnT8cD7.js
> TypeError: Failed to fetch dynamically imported module:
>   https://xvn.tail2bb69.ts.net/assets/scenarios-new-5cnT8cD7.js
> Note on refresh this worked fine!

> Still getting this error, and agent does not correct issue, just
> keeps hitting this. How can we stop the eval if it gets the same
> error multiple times in a row?
> ```
> "error_class": "broker_min_order_size",
> "error_message": "paper eval submit_order failed: ... alpaca
>   create_order: rejected by venue: HTTP status 403 Forbidden: cost
>   basis must be >= minimal amount of order 10"
> ```
> (full broker.call span on run `01KRZ18JTMZ1S7W1MBKC1PNNSJ`,
> decision_index=11, ETH/USD, paper venue)

> one more error: PROMPT
> hash: sha256:d089c54e9451f83104d74ee4e20c5ebea3ba663692dd0ae1a1553cb8bef0562d
> prompt body not captured for this run — re-run to capture
> RESPONSE
> hash: sha256:a22089bd7e69e8ed4da2d94187d163f8c5458b61b26223d55938b8a0416948bf
> completion body not captured for this run — re-run to capture
> — in trace :(
