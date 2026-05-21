# Intake — 2026-05-19 — Eval traces end-to-end audit

**Status: Open — ready for agent pickup. See §Agent brief below.**

Source: live audit of `xvn-app` container — 56 eval runs, 56 agent runs,
2,967 spans, 2,757 model calls. Triggered by 18.3M token burn in a 15-second
burst on 2026-05-19T14:22.

Some findings may overlap with V2E (eval accuracy & trace surface, 7 contracts
merged) and the eval-honesty wave (#448 smell-tests, #449 log-collapse,
#450 provider-attestation, #452/#468 provider-preflight). Items marked
`[needs verify]` should be checked against archived contracts in
`team/archive/2026-05-21-conductor-sweep/contracts/` and
`team/archive/2026-05-19-sweep/contracts/` before opening contracts.

---

## Sequencing

```
P0 first:    F-1  (rate-limit bleed-stopper)
P1 parallel: F-2, F-3  (provider retry, stuck-running watchdog)
P1 stack:    F-4 → F-5  (config validation → schema drift lint)
P1 indep:    F-6, F-7   (causal/oracle tagging, trade guardrails)
P2 indep:    F-8, F-9   (prompt cache + window, flat degeneracy skip)
P3 tail:     F-10, F-11 (chat-rail tool-id, observability sub-items)
```

---

## F-1 `eval-launch-concurrency-and-429-backoff` — P0, integration, M
**Status: open** — no named contract has landed for this.

27 eval runs launched in a 15-second burst all hit OpenRouter's 450 RPM limit;
~18.3M input tokens burned for zero decisions.

**Acceptance criteria:**
- `xvn eval run` (and dashboard "Run all") cap concurrent launches at
  `max_concurrent_evals` (default 4, configurable per provider+model slot).
- 429 responses trigger sleep-until-reset (parse `Retry-After` header) with
  3 retries + exponential backoff before surfacing as `provider_rate_limited`.
- `finalize` UPDATE hotspot serialized — concurrent finishes don't race.
- Test: launch 10 evals simultaneously against a mock 429 provider; confirm
  only 4 start immediately, others queue, all eventually complete.

**Files:** `crates/xvision-engine/src/eval/executor/`,
`crates/xvision-dashboard/src/routes/eval.rs`

---

## F-2 `provider-error-classify-retry` — P1, leaf, S
**Status: needs verify** — provider-attestation (#450) may cover classification
but not the specific `MissingChoicesArray` retry path.

Two runs failed with `[unclassified] OpenAI-compat response missing 'choices'
array` — gateway transient, not fatal.

**Acceptance criteria:**
- `MissingChoicesArray` added to typed error set; treated as retryable
  (3 attempts, exponential backoff), then `provider_error`.
- Search-upsert race: delete-then-insert replaced with single atomic
  `INSERT … ON CONFLICT DO UPDATE`.
- Test: mock provider returns `{}` twice then succeeds; run completes cleanly.

**Files:** `crates/xvision-engine/src/providers/`, OpenAI-compat response parser.

---

## F-3 `eval-run-watchdog-and-stuck-running` — P1, leaf, S
**Status: needs verify** — check V2E contracts for a watchdog sweep.

Run `01KS0A5DP8KZVQJ03TCKGKYJVN` stuck in `running` for 3+ minutes, no
`completed_at`, no watchdog.

**Acceptance criteria:**
- On engine boot: sweep `eval_runs` where `status = 'running'` and
  `started_at < now() - 30m`; mark failed with reason
  `'watchdog: stuck running at restart'`.
- Background: re-run sweep every 5 minutes.
- Test: insert a `running` row with `started_at` 31 minutes ago; start engine;
  assert row is `failed` after first sweep.

**Files:** engine startup / `crates/xvision-engine/src/server.rs`.

---

## F-4 `agent-config-validate-on-save` — P1, integration, M
**Status: needs verify** — provider-preflight (#452) covers provider
validation at launch; save-time config checks below are likely still open.

Integrity failures found in live audit:
- Agent named "SOL 4h" with prompt referencing "ETH/USD".
- Agents shipping default placeholder prompts.
- `max_tokens = 0` forwarded to provider (should mean "use provider default").
- Agent-id / strategy-id namespaces conflated in API calls.

**Acceptance criteria:**
- Save: prompt length ≥ 200 chars (reject with actionable error).
- Save: if name contains asset ticker, prompt must reference it (warn only).
- `max_tokens = 0` → omit field from provider request entirely.
- Agent-id and strategy-id are non-interchangeable in all API handlers;
  add guard.
- One test per rule.

**Files:** `crates/xvision-engine/src/api/agents.rs`,
`crates/xvision-engine/src/providers/`.

---

## F-5 `prompt-tool-schema-drift` — P1, leaf, S
**Status: open** — no contract found.

Every outbound prompt blob has `"tools": []`, yet prompts instruct the model
to use tools that aren't registered.

**Acceptance criteria:**
- At agent slot save: parse action enum from prompt text; compare against
  registered tool/action schema; reject if prompt references unregistered
  tool or action enum disagrees with response schema.
- Lint runs at save time (not eval launch).
- Test: save slot with prompt containing `"use the close_position tool"`;
  assert rejected with `E_PROMPT_TOOL_DRIFT` + field path.

**Files:** `crates/xvision-engine/src/api/agents.rs`, new lint module.

---

## F-6 `causal-input-sanitization-and-oracle-tagging` — P1, integration, S
**Status: open** — no contract found.

"v4 causal" agents forbid timestamp/decision_index in prompt, but executor
leaks these fields into every user message.

**Acceptance criteria:**
- `causal: bool` field on agent slot; when true, executor strips `timestamp`
  and `decision_index` from user message before sending to provider.
- `oracle: bool` tag for agents using future/timestamp-based signals. Eval
  compare view shows oracle/causal badge so comparisons don't silently mix them.
- Tests: causal slot receives message without timestamp; non-causal intact.

**Files:** `crates/xvision-engine/src/eval/executor/`,
`crates/xvision-engine/src/api/agents.rs`.

---

## F-7 `engine-trade-guardrails` — P1, integration, M
**Status: open** — eval-honesty was about stub detection, not position guards.

Live violations: 26 consecutive `long_open`, 22 consecutive `short_open`,
12 one-step `long_open → short_open` flips with no flat.

**Acceptance criteria:**
- Executor enforces before processing trader decision:
  - No-pyramid: `long_open` while already long → reject, emit
    `supervisor_note(rule="no_pyramid", severity=warn)`.
  - No-flip: `long_open` while `short_open` active → force `flat` first
    (configurable: reject vs auto-flat).
- Rules apply to both backtest and paper paths.
- Guard counters in trace dock: `pyramid_blocks`, `flip_rejections`.
- Tests: 3 consecutive `long_open` with position already long; assert 2nd
  and 3rd rejected with supervisor note.

**Files:** `crates/xvision-engine/src/eval/executor/`,
`crates/xvision-core/src/trading.rs`.

---

## F-8 `prompt-cache-and-rolling-window` — P2, integration, M
**Status: open** — no contract found.

Avg input 22.6k–23.8k tokens, avg output 63 tokens (360:1 ratio).
One run burned 16.9M input tokens.

**Acceptance criteria:**
- Provider prompt cache on static prefix: system prompt marked cacheable
  (Anthropic: `cache_control: ephemeral`; OpenAI-compat where supported).
  Target ≥50% cache-hit rate on repeated evals of same agent.
- `bar_history_limit` knob (default 50, was effectively 200) caps rolling
  context window. Configurable per agent slot.
- Test: run same eval twice; second run input_tokens ≤ 60% of first.

**Files:** `crates/xvision-engine/src/providers/`,
`crates/xvision-engine/src/eval/executor/`, `config/default.toml`.

---

## F-9 `early-stop-on-flat-degeneracy` — P2, leaf, S
**Status: needs verify** — eval-honesty smell-tests (#448) included
"skip-LLM-when-no-legal-action"; check if that covers this pattern.

Run `01KS03Z0…` first 20 decisions all `flat`, conviction ≤ 0.2; ~460k tokens
spent on a degenerate run.

**Acceptance criteria:**
- If last K=8 decisions are `flat` AND `conviction ≤ 0.2` AND no portfolio
  change: skip next M=4 bars before re-querying.
- Emits `supervisor_note(rule="flat_degeneracy_skip", bars_skipped=M)`.
- K and M configurable (default 8, 4).
- Test: mock always returns `flat, conviction=0.1`; after 8 calls, assert
  4 bars skipped (no provider call); then called again on bar 13.

**Files:** `crates/xvision-engine/src/eval/executor/`.

---

## F-10 `chat-rail-tool-id-validation` — P3, leaf, S
**Status: open.**

Chat session repeated `get_cli_job(job_id="eval_run_XKI6IWGw5aFZXsqkW3a3")`
forever — wrong id namespace, silently "not found".

**Acceptance criteria:**
- Before calling `get_cli_job`: validate id shape (ULID, `eval_run_<ulid>`,
  `cli_job_<ulid>`). Fail-fast with `E_ID_NAMESPACE_MISMATCH` and suggestion
  ("eval_run_ prefix is an eval run id; use get_eval_run instead").
- Test: call with `eval_run_…` id; assert typed error + suggestion message.

**Files:** chat tools handler.

---

## F-11 `eval-observability-followups` — P3, integration, M
**Status: partially done by V2E** — verify sub-items before contracting.

Sub-items (verify each against V2E archive):
- (a) No `bundle → agents.agent_id` lookup table.
- (b) `model_calls.cost_usd` NULL across all 2,757 rows — may be in QA7 work.
- (c) `eval_reviews` table empty — schema exists, not wired.
- (d) Dashboard polling `eval.get_run` 890× vs 64 `eval.start` — likely open.
- (e) 5,568 blobs with no GC against `retention_mode` — likely open.
- (f) `tool_calls`, `events`, `supervisor_notes`, `approvals` all 0 rows —
  partially addressed by V2E trace foundation (#422).

**Acceptance criteria for confirmed-open sub-items:**
- (d) Frontend polls at most 1 req/sec (SSE preferred, or enforce min interval).
- (e) Blob GC janitor sweep at eval completion honours `retention_mode`.

**Files:** `crates/xvision-dashboard/src/routes/eval.rs`,
`crates/xvision-engine/src/janitor.rs`.

---

## Pre-work verification query

Run against live `xvn-app` container before opening contracts:
```sql
-- confirm no new stuck-running since last deploy
SELECT id, started_at, status
FROM eval_runs
WHERE status = 'running' AND started_at < datetime('now', '-10 minutes');

-- confirm cost_usd still null (F-11b)
SELECT COUNT(*) FROM model_calls WHERE cost_usd IS NULL;
```
