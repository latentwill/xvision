# Q15 — Eval resilience + per-object data contracts

Date: 2026-05-16
Status: Draft for implementation
Intake: `team/intake/2026-05-16-q15.md`

Covers the three meatier QA15 items that need a written design before
contracting:

- Agent max-tokens should auto-match the selected model and avoid empty-output failures.
- Scenario eval must pre-load warmup bars so bar-1 decisions have real indicator context.
- Eval objects need a full JSON export suitable for QA round-trip.

The three trivial-leaf items (granularity dropdown, retry button, per-object JSON for
strategy/scenario/agent) are decomposed straight from the intake without a spec.

## 1. Agent max-tokens from model

### Problem

`AgentSlot.max_tokens` is a free integer field defaulting to 1000. Operators
who don't tune it hit the failure mode shown in QA15 item 5:

```
trader output truncated at MaxTokens before any text was emitted
(stop_reason=MaxTokens, input_tokens=422, output_tokens=1000,
 raw_excerpt="<empty>")
```

This is severe for reasoning/thinking models (Claude Sonnet 4.6, DeepSeek
R1, GPT o-series) where hidden reasoning eats the entire output budget
before any visible text emerges.

### Design

1. Extend the provider/model registry record to carry:
   - `output_token_ceiling`: hard provider cap (e.g. 8192 for Sonnet 4.6).
   - `reasoning_token_default`: 0 for non-reasoning, ≥10000 for thinking models.
   - `recommended_visible_output`: target visible-text budget (e.g. 2048).
2. `AgentSlot.max_tokens` becomes optional. When unset:
   - For non-reasoning models: `recommended_visible_output`.
   - For reasoning models: `recommended_visible_output + reasoning_token_default`.
3. Existing explicit values continue to be honored, clamped to
   `output_token_ceiling`.
4. The agent UI window shows the effective value next to an "Auto from model"
   pill; switching models updates the placeholder live.
5. When a run fails with `TraderFailureKind::Truncated` AND `raw_excerpt`
   is empty AND the model is in the reasoning class, the failure surface
   includes the actionable hint "raise max_tokens or pick a non-reasoning
   model" rather than a generic truncation message.

### Out of scope

- Per-arm thinking-budget metering (separate cost-accounting track).
- Streaming partial trader output (a later resilience track).
- New model entries beyond what's currently in the registry.

### Acceptance

- A fresh agent built against Sonnet 4.6 with no manual max_tokens runs to
  visible text on the QA15 reproducer scenario without truncation.
- A fresh agent built against DeepSeek V3 with no manual max_tokens still
  caps at the model's known ceiling (does not regress quality).
- The agent UI shows the effective value and source ("Auto" vs "Manual").

## 2. Scenario warmup bars

### Problem

QA15 item 4: a 30-bar 1d scenario where the strategy uses a 30-bar EMA
cannot make a decision until the very last bar — every prior decision is
"insufficient data". This matches the QA15 reproducer transcript:

```
Apr 16: No EMA cross evident from single bar...
Apr 17: insufficient indicator data (EMA lines missing)...
Apr 18: No crossover signal (EMA5 below EMA13)...
```

Prior context: PR #177 removed the artificial 200-bar warmup gate that was
preventing short-window evals from starting at all. That fix was correct
but incomplete — the executor now starts at bar 1 with no indicator
history at all. We need actual prior bars, not the artificial gate back.

### Design

1. Scenario record gains an optional `warmup_bars: u32` field.
2. Backtest executor, before iterating decision bars, fetches
   `warmup_bars` worth of bars **immediately before** the scenario start
   from the bars cache, joins them to the head of the bar series passed
   to indicators, and marks them `is_warmup = true`.
3. Decision loop iterates only `is_warmup = false` bars. Indicators have
   already absorbed warmup.
4. Strategy authoring can declare a `min_warmup_bars: u32` derived from
   the indicator config (e.g. the longest EMA period × 2). Eval preflight
   warns when `scenario.warmup_bars < strategy.min_warmup_bars` and
   suggests bumping the scenario.
5. Default `warmup_bars` for new scenarios = 200, matching the old gate
   value but configurable.
6. CLI: `xvn scenario create --warmup-bars N` and `xvn scenario update
   --warmup-bars N`. UI: a "Context bars" field in scenario authoring with
   a small helper text linking to the strategy's `min_warmup_bars`.

### Edge cases

- Bars cache miss for the warmup window: preflight fails with a clear
  "bars cache does not extend back N bars before scenario start; run `xvn
  bars fetch ...` first" message.
- Symbol delisted before scenario start: warmup window allowed to be
  shorter than requested if cache reports the symbol's first-seen bar;
  surfaced as a non-blocking warning.

### Acceptance

- The QA15 reproducer scenario (1d, ~30 bars) with EMA5/EMA13 strategy
  produces a real crossover decision at bar 1 once `warmup_bars >= 13`.
- Eval preflight surfaces the warmup mismatch warning when
  `warmup_bars < strategy.min_warmup_bars`.
- `xvn scenario create --warmup-bars 200` round-trips through the API.

## 3. Eval full JSON export

### Problem

QA15 item 6 (eval half): operators want eval runs serialized as a single
JSON object that contains everything needed to reproduce or audit the run.
Today the run-detail page renders fields piecemeal; CLI emits limited
human text; nothing is a complete export.

The user's stated use case is "feed it back in for QA" — round-trip
attaching the JSON to a bug report or replay tool.

### Design

A single `EvalRunExport` shape:

```jsonc
{
  "schema_version": "1",
  "run": { /* eval_runs row, all columns */ },
  "scenario": { /* full scenario including warmup metadata */ },
  "strategy": { /* full strategy bundle including all AgentRefs resolved */ },
  "agents": [ /* resolved Agent + AgentSlot per AgentRef */ ],
  "metrics": { /* metrics_json verbatim */ },
  "decisions": [
    {
      "ix": 0,
      "ts": "...",
      "bar": { "open": ..., "close": ..., "ts": ... },
      "trader_input": { /* prompt + tools */ },
      "trader_output_raw": "<provider raw>",
      "trader_output_parsed": { /* TraderDecision */ },
      "risk_decision": { /* RiskDecision */ },
      "fill": { /* paper fill record */ },
      "errors": [ /* TraderOutputError, etc. */ ]
    }
  ],
  "equity_samples": [ /* eval_equity_samples rows */ ],
  "events": [ /* eval_events rows */ ],
  "errors": [ /* eval_errors rows */ ],
  "reviews": [ /* eval_reviews + linked findings */ ],
  "provider_diagnostics": [ /* stop_reason, token counts, raw_excerpt per call */ ]
}
```

Exposed via:

- `GET /api/eval/runs/:id/export` (Content-Type: application/json)
- `xvn eval export <run_id> [--output run.json]` — writes to stdout by default.
- Run-detail UI gets a "Download JSON" button.

Tests must include a round-trip canary: export → parse → assert all
top-level keys present and `decisions[].ix` is contiguous.

### Out of scope

- Object-level JSON for `strategy`, `scenario`, `agent` — covered by the
  `q15-object-json-output` leaf track. That track standardizes the per-object
  shape consumed by this export.
- Binary attachments (charts, screenshots).
- Streaming export for in-flight runs (export is for terminal runs).

### Acceptance

- A completed run from the QA15 reproducer scenario exports as a single
  JSON file ≥ all fields above and parses without error.
- `xvn eval export <run_id>` and `GET /api/eval/runs/:id/export` return
  byte-identical output for the same run.
- Round-trip canary test passes in CI.

## Sequencing

1. `q15-agent-max-tokens-from-model` and `q15-scenario-warmup-bars` can run
   in parallel (different layers).
2. `q15-eval-json-export` waits on neither but should land before the
   per-object JSON leaf so the export shape can reference the standardized
   per-object shapes.
3. The three leaves (granularity dropdown, retry button, per-object JSON)
   land independently.

## Risks

- **Warmup bars on cache-miss surface.** Without a clear preflight, operators
  hit confusing "indicators missing" runs. The preflight error is the
  single highest-leverage UX in this wave.
- **Token defaults regressing existing tuned agents.** Mitigation: explicit
  `max_tokens` values are honored unchanged; only unset values pull from
  the model.
- **Export shape churn.** Pin `schema_version: "1"`. Future breaking changes
  bump the version and keep a migration helper.
