# Intake — 2026-05-19 — Eval traces end-to-end audit: tool failures, prompt drift, inefficiency, rule-leakage

Operator request (Ed, 2026-05-19): comprehensive sweep of every eval run,
agent run, JSON trace, and log inside the `xvn-app` container to surface
tool failures, inefficiencies, prompt-forming errors, lack-of-clarity
issues, and anything else worth fixing before the next intake wave.

Read-only audit; nothing was changed in the container.

## Source

- Container: `xvn-app` on this dev server, image `xvision:deploy-latest`.
- DB: `docker cp xvn-app:/data/xvn.db /tmp/xvn.db` (5,632 KB,
  2026-05-19T14:27Z snapshot). 56 `eval_runs`, 56 `agent_runs`, 2,967
  `spans`, 2,757 `model_calls`, 88 `eval_findings`, 15 `agents`,
  1,425 `api_audit` rows.
- Blobs: `/data/agent_runs/blobs/` — 5,568 sha256-keyed payload blobs
  (prompt + response bodies for eval model calls).
- Logs: `docker logs xvn-app --since 24h` for the same window.
- Strategies / config dirs: `/strategies`, `/config`, `/data/strategies`,
  `/data/probes`.

## Already in flight (do not respawn)

- `harness-prompt-hash-real-digest` (F-1, harness obs audit) —
  `prompt_hash` is faked as `eval:{run}:{span}`; once real digests land,
  prompt-version provenance becomes possible. F-4 of *this* intake
  depends on it.
- `harness-prompt-version-field` (F-3, harness obs audit) — adds
  `prompt_version` content-hash + migration. Covers part of what this
  audit observed re: opaque `prompt_version` strings on `agent_slots`.
- `harness-span-attrs-populate` (F-2, harness obs audit) — fills the
  empty `attributes_json` bag.
- `harness-span-taxonomy-extension` (F-4) + `harness-recovery-state-machine`
  (F-5) — adds the four missing span kinds and a typed recovery
  dispatcher. F-5 is the *agent*-loop recovery path; this intake's
  F-1 below is the *eval-executor* concurrency/backoff path — they are
  adjacent, not the same code.
- `findings-postprocess-provider-routing` (F-5 of
  `2026-05-19-qa-validate-draft-cadence-false-positive.md`) — already
  filed for the `claude-haiku-4-5-20251001` OpenRouter 400. Re-confirmed
  here: `api_audit` shows 4 occurrences across 4 distinct eval runs on
  2026-05-18 and 2026-05-19T02:34. Not respawned.
- `qa-retention-prompt-storage-bug` (queue note) — `prompt_payload_ref`
  is always `None` in the *agent harness* path. Eval-runner path *does*
  write blobs (I sampled them); the asymmetry between the two recorder
  paths should be reconciled when this lands.
- `harness-recovery-state-machine` covers agent-loop recovery only; this
  intake's F-1 + F-2 cover the **eval executor** which lives in
  `xvision_engine::eval::executor` and has its own un-retried failure
  path.

## V2 roadmap items (not contracts here)

- V2A "ease of use" — agent config validation at save time (this
  intake's F-4) and engine-side guardrails (F-7) directly serve the V2A
  promise that an operator can't accidentally ship a misconfigured
  agent.
- V2B (cost / scale) — prompt-cache + rolling-window context (F-8) and
  early-stop on lazy-flat degeneracy (F-9) are the two biggest
  inefficiency wins surfaced by this audit.

## Findings → tracks

| # | Severity | Finding | Track |
|---|---|---|---|
| F-1 | P0 | At 2026-05-19T14:22:52–14:23:07Z, **27 eval runs launched in a 15-second window** all hit `google/gemini-3.1-flash-lite` on OpenRouter and tripped its 450 RPM limit. Each run had already streamed 180k–980k input tokens before the executor gave up — **~18.3M input tokens burned** for zero successful decisions. The 429 body even returns `X-RateLimit-Reset=1779200640000` but `xvision::eval::executor` aborts on the first non-success status with no retry, no backoff, and no global concurrency cap. Compounding: 27 concurrent failure-finalize writes serialized through SQLite and triggered the `slow statement` alert (`UPDATE eval_runs SET status='failed' … elapsed=1.029s`). | `eval-launch-concurrency-and-429-backoff` |
| F-2 | P1 | Two runs (`01KS09WJ64SBNC35GHVK7HQYK6`, `01KS09WFVT5YMA8VHPW0PB2NV0`) failed with `[unclassified] OpenAI-compat response missing 'choices' array` — almost certainly an OpenRouter transient gateway response, not a logic error. Classifier (`classify_run_failure`, `eval/executor/mod.rs:48`) doesn't recognize it, so it's treated as fatal with no retry. One log row also shows `xvision_engine::api::search: search index upsert (run) failed error=delete prior row` (`run_id=01KS09WVDZH1F01TW8527RXYED`) — non-atomic upsert path racing with eval finalize. | `provider-error-classify-retry` |
| F-3 | P1 | `eval_run 01KS0A5DP8KZVQJ03TCKGKYJVN` (started 14:27:45Z) is still `status='running'` 3+ minutes later with no `completed_at`, no `error`, 2 spans, 0 tokens. No engine-side watchdog promotes a stalled `running` row to `failed`. UI presumably keeps spinning. The `slow statement` warning around the F-1 storm also showed `rows_affected=0` for the finalize update because the row had already failed via another path — the same race exists for orphaned `running` rows. | `eval-run-watchdog-and-stuck-running` |
| F-4 | P1 | Multiple agent-config integrity failures visible across `agents` / `agent_slots` / actual prompt blobs: (a) **`SOL 4h trend breakout trader agent`** has a 5,653-char system prompt that opens with `"You are a single-agent ETH/USD 4-hour swing trader"` — name says SOL, prompt says ETH, prompt blob `686473…` confirms inputs are `"asset":"ETH/USD"`; (b) **`Macro MACD-RSI Weekly Trader`** and **`Multi-Factor Logic Agent`** both ship the 129-char default placeholder (`"You are a trading agent. Decide based on the inputs provided. …"`), same `prompt_version=41ac7a4abb2e51a5` — these are effectively unconfigured; (c) three `agent_slots` carry `max_tokens=0` but the actual outbound request blob has `max_tokens: None` regardless — the slot value isn't being forwarded to the provider call at all; (d) `api_audit` shows two `strategy.get` 404s where the target was `01KS00F1SQ159EDJK1ABX57HQX` — the **`agent_id`** of `SOL 4h trend breakout trader agent`, not a strategy id. Caller is conflating namespaces. | `agent-config-validate-on-save` |
| F-5 | P1 | Prompt ↔ tool ↔ schema three-way drift across all 2,757 model calls. (a) Every outbound prompt blob has `"tools": []`; `model_calls.tool_calls_requested` is empty in **every row** — yet several system prompts (e.g. SOL 4h: *"You may call `indicator_panel` at most once per decision … `ohlcv_history` + `indicator_panel`"*) instruct the model to use tools that are not registered. (b) The SOL 4h prompt explicitly lists `exit` as an allowed action; the `response_schema` enum is `["long_open","short_open","flat","hold"]` — strict-JSON would reject `exit`, and indeed 0 `exit` decisions exist across all 56 runs. (c) `max_tokens` and `temperature` from `agent_slots` are passed as `None` to the provider, so per-slot tuning is silently discarded (see F-4(c)). | `prompt-tool-schema-drift` |
| F-6 | P1 | The "v4 causal" agents (`v4 causal breakout`, `v4 causal shock fade`, `v4 causal trend pullback`) include the prompt rule *"Do not use timestamp, calendar date, run position, decision_index, or memorized scenario windows"* — but every user message the engine sends them contains **literal `"decision_index": N`** *and* a `"timestamp"` field on every OHLCV bar (e.g. `"timestamp": "2026-04-08T20:00:00Z"`). The agent cannot comply because the harness leaks the very fields the prompt forbids. Two existing agents (`BTC 1h timestamp trend rider v2`, `BTC 1h timestamp swing oracle v3`) are *deliberate* timestamp-cheat oracles — they need a `tag/role: oracle` label so causal comparisons don't include them by accident. | `causal-input-sanitization-and-oracle-tagging` |
| F-7 | P1 | Hard rule violations observed across many runs despite explicit prompt language ("Never add to a position", "Never flip long↔short in one step"). Worst cases: 26 consecutive `long_open` in run `01KRZ18JTMZ1S7W1MBKC1PNNSJ`; 22 consecutive `short_open` in `01KRZKG8A1FHTBE88NPWTVQVYS`; 12 one-step long↔short flips in the same run. Cause is a mix of weak model (`gemini-3.1-flash-lite` is the *only* model used across all 2,757 calls) and the prompt-only enforcement of trade hygiene with no portfolio-state context being shown turn-over-turn. Move these from prompt rules to engine-side guardrails in the executor, returning `flat` or rejecting the order before broker call, and surface a `supervisor_note(severity=warn)` per violation. | `engine-trade-guardrails` |
| F-8 | P2 | Context bloat. Every model call resends the **full 200-bar OHLCV history** as JSON — avg input 22.6k–23.8k tokens, avg output 63 tokens (input:output ratio **~360:1**). One run (`01KS03Z0BRCTDM1MX8BRRGMQP5`, BTC 30-day Jan 2025 backtest, 720 decisions) burned 16,996,450 input tokens / 30,461 output / ~15min wall clock. No prompt-caching directive is sent; no rolling window; no delta encoding. Two cheap wins: (1) send `system_prompt` + `bar_history[:-1]` with provider-side `cache_control` so only the newest bar varies turn-over-turn; (2) when an agent's lookback is e.g. 50 bars, stop sending 200. Also: **`google/gemini-3.1-flash-lite` is the only model in use** for every call across every agent — the slot-level `model` field is being honored, but every slot is configured to the same lowest tier. Either upgrade the slots that need judgment or accept that the "Macro MACD-RSI Weekly Trader" is running on the cheapest tier. | `prompt-cache-and-rolling-window` |
| F-9 | P2 | Same-justification flat degeneracy. Across all runs, the most common justification appears 15× verbatim (*"No overextended shock candle detected for mean reversion."*); 14× (*"No clear volatility expansion or breakout signal."*). Within a single run (`01KS03Z0BRCTDM1MX8BRRGMQP5`) the first 20 decisions are all `flat` with conviction ≤ 0.2 and near-identical justifications — ~460k input tokens spent to produce 20 copies of the same "I don't know". Cheap heuristic: if the last K decisions are all `flat` with conviction ≤ τ and no portfolio change, downshift cadence (skip the next M bars or batch-decide every M-th bar) until a state change. Saves tokens and stops eval reports from being padded with no-ops. | `early-stop-on-flat-degeneracy` |
| F-10 | P3 | `chat_messages` (session `01KRXXHPRBKYKVEM2Q1VBS2YJ4`) shows the assistant repeatedly calling `get_cli_job(job_id="eval_run_XKI6IWGw5aFZXsqkW3a3")` and receiving `{"error":"cli job 'eval_run_…' not found"}`, then calling `get_cli_job_output` with the same bad id, then retrying — never recovering. The tool result is in the model's context but the convergence guard is missing. Same anti-pattern flagged in `chat-rail-validate-retry-budget` (F-3 of yesterday's intake) — different surface, same loop. Coordinate so the retry-budget guard covers any `*_get_cli_job*` tool too. | `chat-rail-tool-id-validation` |
| F-11 | P3 | Observability hygiene that's not big enough for its own contract: (a) `eval_runs.agent_id` is documented as a "bundle artifact hash" but there is no `bundle → agents.agent_id` lookup table or join path — finding the agent that produced a run requires regex'ing `agent_runs.objective`; (b) `model_calls.cost_usd` is NULL across all 2,757 rows, so the dashboard can't show $$ burned by the F-1 storm; (c) `eval_reviews` schema is defined (`verdict`, `score`, `summary`, `raw_output_json`) but the table is **empty** — the intended review workflow isn't wired up; (d) `api_audit` shows `eval.get_run` called 890× vs 64 `eval.start` — UI is polling per-millisecond instead of streaming; (e) `/data/agent_runs/blobs/` has 5,568 blobs with no observed GC against `agent_runs.retention_mode`; (f) `tool_calls`, `events`, `supervisor_notes`, `approvals`, `sandbox_results`, `checkpoints`, `artifacts` are all **0 rows** for eval-driven runs — Phase-B recorders for these surfaces aren't emitting in the eval path (the agent-harness side is covered by the in-flight harness obs work; the *eval-executor* side needs its own wire-up). | `eval-observability-followups` |

Eleven tracks. F-1 + F-3 are the two operator-actionable bleed-stoppers
(no more 18M-token rate-limit storms, no more stuck-`running` UI). F-4 +
F-5 + F-6 + F-7 are the integrity bucket — once they land, the
correctness of every eval going forward is materially higher. F-8 + F-9
are the cost bucket. F-2, F-10, F-11 are hardening.

## Track summaries

### F-1 `eval-launch-concurrency-and-429-backoff` (P0, integration, M)

Three changes coordinated in `crates/xvision-engine/src/eval/executor/`:

1. **Concurrency cap** at run-launch level. New config (default 4)
   gates `eval.start` so no more than N runs are in `running` against
   the same `(provider, model)` slot. The 27-in-15s burst that wasted
   18.3M tokens would have queued instead.
2. **429 handler** on the model-call path. When the provider returns
   `429` with an `X-RateLimit-Reset` header, sleep until reset (with
   jitter), retry up to 3 times before failing the run. Bubble a typed
   `RateLimited { reset_at }` error so the executor doesn't have to
   regex the body.
3. **Serialize-write hotspot.** The `slow statement` was the eval-runs
   finalize UPDATE under 27-way contention. Batch the finalize through
   a single writer task or move `eval_runs.status` to a separate
   write-optimized path; the current schema can stay.

Out of scope: replacing the provider abstraction or moving off
SQLite. Anchor the concurrency cap to (provider, model) not just
provider — different model slugs have different RPM budgets.

### F-2 `provider-error-classify-retry` (P1, leaf, S)

Add `MissingChoicesArray` to the typed error set returned from
`xvision::llm::openai_compat`. Currently this body shape falls through
to `[unclassified]` (`eval/executor/mod.rs:48` regex classifier) and is
treated as fatal. Treat it as retryable (3 attempts, exponential
backoff) — it's a gateway transient in practice. Also fix the search
upsert race in `xvision_engine::api::search` to use a single atomic
upsert query instead of delete-then-insert.

### F-3 `eval-run-watchdog-and-stuck-running` (P1, leaf, S)

Engine-side watchdog: any `eval_runs` row in `running` for more than
`max_run_duration_secs` (default 30min, settable per scenario) is
finalized as `failed` with `error='timeout'` and `completed_at` set.
Reuses the same finalize path as F-1's serialized writer. Also
backfill: one-shot sweep on engine boot to clean up rows that survived
a previous container restart (`01KS0A5DP8KZVQJ03TCKGKYJVN` will be the
first customer).

### F-4 `agent-config-validate-on-save` (P1, integration, M)

A validation pass in `AgentStore::create` and `AgentStore::update_slot`
that refuses to save when:

1. The agent's name contains a recognized asset slug (`SOL`, `BTC`,
   `ETH`, …) that doesn't appear in `system_prompt`. F-4(a) above.
2. `system_prompt` matches the known default-placeholder content hash
   or is shorter than a minimum (e.g. 200 chars). Block save with an
   actionable error. F-4(b).
3. `max_tokens` / `temperature` from the slot are actually forwarded
   into the outbound provider request — current `prompt_builder`
   path drops them (the blob shows `max_tokens: None` regardless of
   slot value). Either add them at the dispatch boundary or remove the
   fields from `agent_slots` so the UI doesn't lie.
4. Caller-side: any code calling `strategy.get(id)` should pre-check
   `id` against the agent-id namespace and return a typed
   `WrongIdNamespace` error instead of a generic `not found`. F-4(d).

Touches `xvision-engine/src/agents/`, the wizard agent-edit path, and
the dispatch site in `xvision::llm`.

### F-5 `prompt-tool-schema-drift` (P1, leaf, S)

Static lint at `agent_slot.update`:

- If the system prompt mentions a tool name (`indicator_panel`,
  `ohlcv_history`, etc.) but the resolved tool registry for that slot
  is empty, block save with a "prompt references unregistered tool"
  error.
- Diff the prompt's stated `action` enum (regex
  `Allowed actions:\s*([a-z_,\s|]+)`) against the response schema; if
  they disagree (today: SOL 4h says `exit`, schema doesn't), block
  save.

Same machinery used by F-4 — single validator pass run at save time
and at run start.

### F-6 `causal-input-sanitization-and-oracle-tagging` (P1, integration, S)

1. Add a `causal: bool` flag (or `inputs_policy: causal | oracle |
   raw`) to `agent_slots`. When `causal`, the eval executor's input
   builder **strips** `timestamp` from every OHLCV bar (replacing with
   bar-relative `index`) and **strips** the top-level `decision_index`
   from the user message. Agents tagged `causal` then physically cannot
   read the forbidden fields, instead of being asked nicely not to.
2. Add a sibling `oracle` flag for the existing
   `BTC 1h timestamp trend rider v2` / `BTC 1h timestamp swing oracle
   v3` agents that *intentionally* use `decision_index` as ground
   truth. Eval-comparison reports should refuse to mix `causal` and
   `oracle` results in the same panel.

Touches `xvision-engine/src/eval/executor/input_builder.rs` (current
home of the bar JSON serializer) and the agent schema migration.

### F-7 `engine-trade-guardrails` (P1, integration, M)

Move "never add to a position" and "never flip long↔short in one
step" out of the prompt and into the executor:

- `engine.apply_decision(decision, portfolio)` checks the prior
  position; if the model returns `long_open` while a long is already
  open, convert to `hold` and emit a `supervisor_note(severity=warn,
  role=guard, content="pyramid blocked")`.
- Same for one-step flips: a `short_open` immediately after `long_open`
  becomes a forced `flat` first (close prior), with a guard note.
- Counters surface in the trace dock as a "guardrail interventions"
  pill on the run summary.

Side benefit: `supervisor_notes` finally has rows (currently 0). The
guard counter becomes a per-agent metric for the eval comparison
dashboard.

### F-8 `prompt-cache-and-rolling-window` (P2, integration, M)

Two independent levers, both safe in isolation:

1. **Provider prompt cache.** For OpenRouter + Anthropic models, set
   `cache_control: {"type":"ephemeral"}` on the static prefix
   (`system_prompt` + all but the last K bars of `bar_history`). For
   OpenAI-compat providers that don't support cache_control, no-op.
   With current shape (22.6k static / 0.1k dynamic per call), even a
   50% cache hit rate roughly halves spend.
2. **Rolling window.** Today every call sends 200 bars even when the
   agent's regime/structure lookback is 60. New `bar_history_limit`
   on the agent slot caps the window; the input builder slices to
   that length. Cuts the static prefix proportionally and works for
   providers without prompt caching.

Out of scope: changing `agent_slots.model` defaults away from
gemini-3.1-flash-lite — that's a separate "model upgrade plumbing"
decision. F-8 just makes it cheaper to keep using it.

### F-9 `early-stop-on-flat-degeneracy` (P2, leaf, S)

In the eval executor's per-decision loop: maintain a small rolling
window of the last `K=8` decisions. If all are `action=flat` with
`conviction <= 0.2` and no portfolio state change, skip the next `M=4`
bars (mark them as inherited `flat` with a `supervisor_note` for the
trace) before re-querying the model. Reset the counter on any
non-flat or any portfolio state change.

Saves ~25% of model spend on the worst runs (`01KS03Z0…`) and stops
the eval comparison table from being dominated by "wait, nothing
happened for 30 decisions" no-ops.

### F-10 `chat-rail-tool-id-validation` (P3, leaf, S)

In `xvision-dashboard/src/wizard_loop.rs` near the tool dispatch
site:

- Before calling `get_cli_job` / `get_cli_job_output`, validate the
  shape of `job_id` against the known id grammars (ULID or
  `eval_run_<id>` etc.). Fail-fast with a typed error the model can
  read in its `tool_result`.
- Hook into the same retry-budget guard `chat-rail-validate-retry-budget`
  (F-3, yesterday's intake) is adding for `validate_draft`. Same
  convergence guard, just covering one more tool.

Coordinate by sharing the retry-budget data structure across both
tools instead of duplicating it.

### F-11 `eval-observability-followups` (P3, integration, M)

Grab-bag of small fixes that are too cheap to bundle individually:

- **Bundle → agent map.** Persist `agents.agent_id` (the long-lived
  one) on `eval_runs` alongside the bundle hash; backfill from
  `agent_runs.objective` strings.
- **`cost_usd`.** Plug provider-side pricing into the
  `ModelCallFinished` event emit so `model_calls.cost_usd` is no
  longer NULL. Coordinate with `harness-prompt-hash-real-digest` (F-1
  of harness obs audit) — same event constructor.
- **`eval_reviews` runner.** Schema exists; nothing populates it.
  Wire a worker that reads `eval_findings` and produces a single
  `eval_reviews` row per run (verdict, score, summary). Gated on
  F-4-from-yesterday (`findings-postprocess-provider-routing`) since
  it shares the model.
- **api_audit polling.** Replace per-millisecond `eval.get_run` polls
  from the dashboard with an SSE stream off the existing event bus, or
  back the poll off to 1Hz with a cache.
- **Blob GC.** Background sweep that walks `/data/agent_runs/blobs/`
  and deletes blobs not referenced by any `model_calls.*_payload_ref`
  or `checkpoints.*_payload_ref`, respecting `agent_runs.retention_mode`.
- **Recorder wireup for eval path.** Once the harness-side recorder
  pipeline (F-1..F-4 in harness obs audit) lands, mirror those
  emissions from `xvision::eval::executor` so the eval path also
  populates `tool_calls`, `events`, `supervisor_notes`, etc. Today
  the eval path goes straight to `eval_decisions` / `eval_equity_samples`
  / `eval_findings` and skips the operational tables entirely.

## Audit detail

### F-1 — rate-limit storm timing

```
2026-05-19T14:22:52Z  run 01KS09WEV4EX6A9AERN816FB44 started
…
2026-05-19T14:23:07Z  run 01KS0A5DP8KZVQJ03TCKGKYJVN started (last in burst)
2026-05-19T14:23:39Z  WARN xvision::llm: OpenAI-compat API returned non-success
                       provider="openai-compat" status=429 Too Many Requests
                       body={"error":{"message":"Rate limit exceeded:
                          limit_rpm/google/gemini-3.1-flash-lite-20260507/…
                          High demand for google/gemini-3.1-flash-lite on
                          OpenRouter - limited to 450 requests per minute.
                          Please retry shortly.",
                          "metadata":{"headers":{"X-RateLimit-Limit":"450",
                          "X-RateLimit-Remaining":"0",
                          "X-RateLimit-Reset":"1779200640000"}}}}
2026-05-19T14:23:40Z  WARN sqlx::query: slow statement … elapsed=1.029s
                       db.statement="UPDATE eval_runs SET status='failed' …"
```

Sum of `actual_input_tokens` across the 27 failed rows: **18,263,948**.

### F-4 — agent vs prompt vs blob examples

```
agent_id=01KS00F1SQ159EDJK1ABX57HQX
name="SOL 4h trend breakout trader agent"
slot.max_tokens=0
slot.system_prompt[0:60]="You are a single-agent ETH/USD 4-hour swing trader."

prompt_blob 686473dbee…:
  system_prompt: "You are a single-agent SOL/USDT swing trader on 4-hour bars…"
  messages[0].content[0].text: '{"asset":"ETH/USD","decision_index":0,
                                  "market_data":{"asset":"ETH/USD","bar_history":[…]}}'
  max_tokens: null
  temperature: null
  tools: []
  response_schema.properties.action.enum:
    ["long_open","short_open","flat","hold"]   # no "exit"
```

(Two different system prompts surfaced for the same agent across
samples — SOL/USDT in one blob, ETH/USD in another. That alone
suggests the agent record was edited mid-window without bumping the
slot version. F-3 of the harness obs audit will fix the silent
overwrite once `prompt_version` becomes a real content hash.)

### F-6 — causal prompt vs leaked input fields

```
system_prompt (v4 causal breakout):
  "Absolute prohibitions:
   - Do not use timestamp, calendar date, run position, decision_index,
     or memorized scenario windows."

user message:
  '{"asset":"SOL/USD","decision_index":0,
     "market_data":{"asset":"SOL/USD","bar_history":[
        {"close":127.0205,"high":127.73,…,
         "timestamp":"2026-01-24T20:00:00Z","volume":2.015…},
        … 200 more …]}}'
```

### F-7 — rule violations by run

```
no-pyramid (consecutive long_open) — top offenders:
  01KRZ18JTMZ1S7W1MBKC1PNNSJ  26
  01KS03Z0BRCTDM1MX8BRRGMQP5  12
  01KRXY73XAE2NR65YVKJZ28JBK  12

no-pyramid (consecutive short_open):
  01KRZKG8A1FHTBE88NPWTVQVYS  22
  01KRZ18JTMZ1S7W1MBKC1PNNSJ  15

one-step long↔short flips:
  01KRZKG8A1FHTBE88NPWTVQVYS  12
  01KRXY73XAE2NR65YVKJZ28JBK   6
  01KS06W7BMM7H10TNX1TR34W9Q   4
```

## Sequencing

1. **F-1 first.** It's the only P0 and it directly stops bleed (token
   waste + DB contention). Self-contained; no dependencies.
2. **F-3 in parallel** — different file, no overlap with F-1.
3. **F-2 in parallel** — different file again.
4. **F-4 + F-5** stack — F-5's validator can share machinery with F-4.
   Land F-4 first since the schema/validator surface comes from there.
5. **F-6 + F-7** independent; either can land any time after F-4.
6. **F-8 + F-9** independent of F-1..F-7. F-8 is the bigger lever; F-9
   is cheap and good practice anyway.
7. **F-10 + F-11** hardening; queue behind everything else.

F-1 + F-3 + F-4 together kill the three most visible operator-facing
failure modes; everything else is integrity and cost work.
