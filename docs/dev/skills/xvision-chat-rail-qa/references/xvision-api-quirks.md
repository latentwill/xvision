# xvision API Quirks

Observed during QA against `https://xvn.tail2bb69.ts.net`.

## Strategy

- `GET /api/strategy/:id` returns the manifest plus slots and attached agents.
- `POST /api/strategy/:id/agents` expects at least:
  - `agent_id`
  - `role`
- `PATCH /api/strategy/:id/agents/:role` expects `new_role`.
- `DELETE /api/strategy/:id/agents/:role` removes the attachment.
- `POST /api/strategy/:id/validate` can return `ok: true` even if the manifest and slot prompts disagree.
- Strategy-level deletion/archive routes were not found on the observed API surface.
- `/strategies/:id` is the canonical inspector route; `/authoring/:id` is a compatibility alias.
- `PATCH /api/strategy/:id` supports editable manifest metadata including display name, summary, asset universe, cadence, and color.
- Strategy filters are real artifacts on the strategy. Prompt wording that describes a filter does not imply filter events will exist.

### Example mismatch

Manifest showed:
- `asset_universe: ["ETH/USD"]`
- `decision_cadence_minutes: 15`

Slots showed:
- BTC/USD
- 6-hour candle prompts

Validation still returned success.

## Scenario

- `POST /api/scenarios` required fields included at least:
  - `source`
  - `display_name`
  - `description`
  - `data_source`
- `DELETE /api/scenarios/:id` returned `204` and the record disappeared from `GET /api/scenarios/:id`.
- The scenario list endpoint returned duplicate rows for the same visible scenario name during QA.

## Eval

- `POST /api/eval/runs` required at least:
  - `agent_id`
  - `scenario_id`
  - `mode`
- Runs could queue successfully and then fail immediately on the first decision.
- A failure observed during QA was:
  - `OpenAI-compat API error 400 Bad Request at https://openrouter.ai/api/v1/chat/completions`
  - `anthropic.claude-sonnet-4.6 is not a valid model ID`
- Another failure mode observed:
  - `run ... decision 0: trader output is invalid JSON: expected value at line 1 column 1`
- Filter QA should inspect `filter_events` and `filter_summaries`; empty values mean the XVN filter system did not visibly participate.
- Eval detail may include synthesized rows from `noop_skip`, graph gating, or early-stop inheritance. Do not treat every decision row as a direct model decision.

## Cleanup

- `DELETE /api/eval/runs/:id` returned `204` for the test run used in QA.
- Temporary scenario delete also returned `204`.

## Useful Status Codes

- `201` create scenario
- `202` enqueue eval run
- `200` fetch / mutate / validate strategy attachments
- `204` successful delete
- `404` missing route or already-deleted object
- `405` method not allowed
- `422` schema/body missing required field
