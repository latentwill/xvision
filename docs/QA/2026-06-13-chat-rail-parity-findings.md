# Chat Rail Parity & Functionality Test — Findings for Coding Agent

**Date:** 2026-06-13
**Target:** `https://xvn.tail2bb69.ts.net/` (xvn personal node, `extndly-dev`), xvn 0.21.0
**Driver:** Playwright (headless Chromium) driving the real dashboard chat rail UI + the documented
`/api/chat-rail/*` SSE endpoints as an oracle.
**Model under test:** `deepseek-v4-pro` via the **deepseek** provider (not ollama), as requested.
Cross-checked latency against `gemini-3.1-flash-lite`.
**Scope:** Asked questions, reviewed, created scenarios/strategies/filters, validated readiness.
**Explicitly avoided:** launching the optimizer or any `eval run` (an optimizer is in flight).
Verified safe: **0 evals launched** (eval-run count stayed at 26; 0 queued/running after tests).

Harness + raw artifacts live in `/root/xvn-work/chatrail-test/`
(`lib.mjs`, `battery_think.mjs`, `battery_act.mjs`, `results-think.json`, `results-act.json`,
`stream_chat.py`, `shots/*.png`).

---

## TL;DR for triage

| # | Severity | Area | One-liner |
|---|----------|------|-----------|
| 1 | **High** | Dashboard API | `GET /api/eval/runs?status=queued,running` → 400; landing "active runs" widget is broken |
| 2 | **High (safety/policy)** | Chat tool policy | `run_eval` is class `Write`+`auto_approve` → **ACT mode launches a backtest with no confirmation**; docs say "write = Ask" |
| 3 | **High (perf)** | Chat rail | Latency balloons with session history: same query 24s in a fresh session → **122s timeout** in a long one |
| 4 | **Med** | Chat tool | `get_eval_run` fails ("2× in a row… • null") on `failed`/null-metric runs → agent circuit-breaks ("operator review needed"); breaks "compare my recent runs" |
| 5 | **Med (parity)** | Chat tools | No `list_providers` tool — chat can't authoritatively list providers/models (CLI `xvn provider list`) |
| 6 | **Med (parity)** | Chat tools | No `list_agents`/`get_agent`/`agent inspect` tool — chat reconstructs the agent library by inference |
| 7 | **Med (parity)** | Chat tools | No **filter-catalog** tool — agent flails (8 file reads) guessing DSL tokens before `set_filter` |
| 8 | **Med (parity)** | Chat tools | No read-only **diagnostics / eval-validate**; only `validate_draft` (Write) → readiness can't be checked in research mode |
| 9 | **Low** | Chat tools | No scenario `classify` / `set_regime` / `select` / `clone` / `archive` tools (CLI has them) |
| 10 | **Low** | Contract | Policy denial surfaces as a `tool_result` failure string, not the documented typed `error_policy_denied` SSE event |
| 11 | **Low** | UX | driver.js onboarding tour overlay intercepts all clicks on first load until dismissed |
| 13 | **Info** | get_strategy | `get_strategy` does not expose the agent's system prompt text |
| 14 | **Low** | Data quality | Created agent's system prompt said "Evaluate **BTC/USD**" while the manifest was **ETH/USD** (prior-turn context bleed) |
| 15 | **Med (parity)** | Chat tools | `update_manifest` can't set strategy **description / display_name** (only asset_universe + cadence); the inspector can |
| 12 | _Harness_ | — | "title-like non-answer" was a measurement artifact (session auto-title desynced my done-counter), **not** a product bug — see Harness caveat |

> Note on #2/#3/#4 severities: the in-flight **optimizer** is itself producing `failed` eval runs
> (ollama `donchian-breakout-4h-ollama-dr1` emits `stop_loss_pct=0.03`, below the 0.1 min →
> `[invalid_field]`). The chat rail correctly *surfaced* this under "What needs my attention?",
> but its `get_eval_run` tool then chokes on those same failed runs (#4).

---

## Test methodology

- All turns issued through the **real chat rail input** in a headless Chromium session (`lib.mjs`
  instruments `window.fetch` to tee the `/api/chat-rail/chat` SSE stream → reliable per-turn
  capture of `tool_call` / `tool_result` / `error_*` / `token` / `done` events).
- THINK (research) battery: 15 read/ask/review turns (`battery_think.mjs`).
- ACT battery: 5 create/modify turns with explicit "do NOT run any eval" guards + a `run_eval`
  circuit-breaker (`battery_act.mjs`).
- Ground truth verified out-of-band via the remote CLI (`scripts/xvn-remote.py exec …`) and the
  REST API.
- The authoritative chat-rail tool registry was read from
  `GET /api/chat-rail/tool-policy/effective` (28 tools) and
  `crates/xvision-dashboard/src/wizard_loop.rs`.

---

## Findings (detail)

### 1. [High] Landing page "active runs" query 400s
`GET /api/eval/runs?status=queued,running` → `400 {"code":"validation","field":"status","message":"unknown run status 'queued,running'"}`.
The frontend sends a **comma-joined** status; the backend only accepts a single value, and
**also** rejects repeated `?status=queued&status=running`. Reproduced directly with curl:
- `?status=running` → 200
- `?status=queued` → 200
- `?status=queued,running` → 400
- `?status=queued&status=running` → 400

This fires on every dashboard load (visible as the `[xvn:query] query.error route:/` console error).
**Fix:** either backend accepts comma-separated/repeated `status` (preferred), or the frontend
issues separate requests / a different param. Frontend caller is the dashboard landing
("active runs"/"awaiting first eval") widget.

### 2. [High — safety] ACT mode runs evals with no confirmation
`crates/xvision-engine/src/chat_session/tool_policy.rs`:
- `classify("run_eval")` → `ToolClass::Write` (grouped with `create_*`, `fetch_bars`).
- `ToolPolicy::default_for(Write)` → `{ enabled: true, auto_approve: true }`.
- `decide("act", Write, {enabled,auto_approve})` → `AutoApproved` (no approval round-trip).

So in **ACT mode** the model can call `run_eval` and a backtest launches **silently**. The skill
and the inline route doc both claim the default is "write = **Ask**" /
"Write → enabled+needs-approval" — that contradicts the code (`auto_approve: true`). Confirmed via
`GET /api/chat-rail/tool-policy/effective`: every write tool shows `auto_approve: true`,
`is_override: false`. THINK/research mode *does* correctly deny it (see #10 / test `eval-denial`).
**Fix options:** make `run_eval` (and other "launch work" verbs) class `Dangerous` (disabled by
default) **or** default Write to `auto_approve:false` (needs-approval) to match the documented
contract, **or** correct the docs. Given an operator explicitly asked not to launch evals, the
silent-launch path is the risk.

### 3. [High — perf] Latency scales badly with session history
The chat replays full session history each turn. Same prompt
("Show the full configuration of wiki-donchian-breakout-4h…"):
- **Fresh session (`/api/chat-rail/sessions` → new id):** 23.9s, called `list_strategies`+`get_strategy`,
  produced a correct full config dump.
- **Long shared session (turn 3 of 15):** **122s → client timeout**, model never even called
  `get_strategy`.

deepseek-v4-pro on a *trivial* fresh turn is ~2.4–4.7s (measured directly against `/api/chat-rail/chat`),
so the provider is not the problem — accumulated context is. Several mid-battery turns ran
20–35s purely from history bloat. **Fix:** context windowing / history summarization / cap the
replayed transcript; consider a per-turn token budget. Without it, any real working session
becomes unusable after ~10–15 turns.

### 4. [Med] `get_eval_run` breaks on failed/empty runs
"Compare my two most recent eval runs" → the two most recent runs are `failed` (optimizer
`ec-day-*` runs). The chat tool reported:
> `get_eval_run` stuck — operator review needed. `get_eval_run` failed 2× in a row with the same error. Stopping so the operator can decide what to do. • **null**

The REST API returns those same runs fine (`GET /api/eval/runs/<id>` → 200 with a populated
`error` field and `sharpe/total_return_pct = null`). So the failure is in the **chat `get_eval_run`
tool layer** (likely null-metric deserialization), and the surfaced error is an unhelpful `null`.
Net effect: comparison/review flows hard-stop whenever recent runs include a failed one.
**Fix:** make `get_eval_run` tolerate `failed`/null-metric runs and return the run's `error`
string instead of `null`.

### 5–8. [Med — parity gaps] Missing read tools
The chat-rail tool registry (28 tools) has **no**:
- `list_providers` / provider introspection (#5) — model says *"I don't have a direct
  `list_providers` tool"* and reconstructs from strategy data. CLI: `xvn provider list/check/models`.
- `list_agents` / `get_agent` / agent diagnostics (#6) — *"I don't have a standalone `list_agents`
  tool"*; reconstructs the library by inference. CLI: `xvn agent get/inspect`.
- filter-catalog / DSL introspection (#7) — to answer "what filter tokens exist" the agent made
  **8 tool calls** (`read_strategies_file`×5, `list_strategies_folder`×2, `list_strategy_ideas`)
  guessing tokens from existing strategies. The skill explicitly says agents must consult
  `filter-catalog --json` before authoring; without the tool, `set_filter` authoring risks invalid
  tokens. CLI/route exist: `xvn strategy filter-catalog --json`, `/docs?slug=filter-dsl-catalog`.
- read-only diagnostics / eval-validate (#8) — the only readiness validator exposed is
  `validate_draft`, which is class **Write** → **denied in research mode**. So "is this strategy
  launch-ready?" cannot be answered read-only; the model fell back to a manual field-by-field check
  and noted *"validate_draft not run — I can't confirm formal invariants."* CLI: read-only
  `xvn strategy diagnostics`, `xvn eval validate`.

### 9. [Low — parity] Missing scenario authoring verbs
No chat tools for scenario `classify`, `set_regime`, `select`, `clone`, `archive`, `rm`, `validate`,
`diff`, `tree` — only `create_scenario`, `list_scenarios`, `get_scenario`. (CLI has the full set.)

### 10. [Low — contract] Policy denial isn't a typed `error_*` event
THINK-mode `run_eval` request → the SSE stream emitted **no** `error_policy_denied` event
(`streamErrors: []`); the denial arrived as a `tool_result` failure string:
> `run_eval` failed: Tool `run_eval` writes/mutates state and is blocked in research mode (read-only).
> Switch the session to Act mode to use it.

Same pattern for `validate_draft` (denied in research) and `resolve_strategy` not-found. The skill
documents typed kinds (`error_policy_denied`, `error_missing_tool`, …) that "never short-circuit
silently." Functionally safe and well-worded, but **clients keying on typed `error_*` events won't
see them** — they're delivered as tool-result failures. Reconcile docs vs implementation.

### 11. [Low — UX] Onboarding tour blocks interaction
A driver.js tour (`.driver-overlay` SVG) paints over the page on first load and **intercepts all
pointer events** until dismissed (Esc / close button). Automation and first-time users must
dismiss it before doing anything. (Likely localStorage-gated to first visit.)

### 12. [Harness artifact — not a product bug] Title-like non-answer
Turn `list-strategies-detail` recorded the string **"Complete workspace tool inventory listed"** as
its answer. This is almost certainly the **session auto-title** generation hitting the same
`/api/chat-rail/chat` endpoint and desyncing my done-counter — not the model failing to answer.
See **Harness caveat** below. Listed here only so the next reader doesn't mistake it for a defect.
(If you *do* want to confirm the auto-title path is correct, that's the thing to verify.)

### 13. [Info] `get_strategy` omits the agent system prompt
`get_strategy` returns agent id/role/provider/model but not the agent's prompt text:
*"The agent's system prompt isn't exposed through the available tooling."* Fine if intentional;
flag if chat-side prompt review is desired.

---

## What worked well (parity confirmed)

- Model switching to `deepseek-v4-pro` (deepseek provider) via the rail's MODEL picker. ✔
- `list_strategies`, `list_scenarios`, `get_scenario`, `list_eval_runs` — correct data. ✔
- Tool-call chips render with `· auto_approved` / `· ok`. ✔
- Not-found handling: `resolve_strategy` on a bogus id → clean "not found" + helpful listing. ✔
- Review flow (`review-strategy`): chained `list_eval_runs`+`get_strategy`+`get_eval_run`, produced a
  coherent hypothesis/coherence/launch-readiness review. ✔
- "What needs my attention?": chained 6 tool calls, correctly surfaced the optimizer's broken
  ollama eval runs, zero-trade runs, missing regime tags, duplicate scenarios. ✔ (Genuinely useful.)
- THINK/research mode correctly **denies** `run_eval` and `validate_draft` (read-only enforced). ✔

---

## ACT-mode results (create scenarios / strategies / filters)

Ran in a dedicated fresh session (`01KV0MQPAG…`, since deleted). `runEvalSeen: 0` — **no eval
launched**. Ground truth taken from the rendered tool-call **chips** + the CLI oracle (see harness
caveat below).

| Turn | Result | Evidence |
|------|--------|----------|
| create-scenario | ✔ works | `create_scenario` → `chatrail-test-btc-mar` (BTC/USD, 1h, 2025-03-01→10, $100k, Alpaca venue defaults auto-filled). Model called `create_scenario` **twice** (first attempt needed a fully-specified venue, retried) — only **one** scenario created, no duplicate. |
| create-strategy (atomic) | ✔ works | Fired `create_strategy → update_manifest → create_strategy_agent → set_mechanical_param ×5 → set_risk_config → validate_draft`. Created `chatrail-test-meanrev` (ETH/USD, 60-min cadence, RSI mean-reversion mechanical params, balanced risk). Agent bound to **`deepseek` / `deepseek-v4-pro`** as requested. ✔ |
| set-filter | ✘ **failed** | **152s client timeout; `set_filter` was never called; no filter artifact on the strategy** (`strategy show` has no filter field). The model spent the whole turn unable to construct the DSL (no filter-catalog tool — #7) in a long session (latency — #3). |
| validate (preflight) | ✔ works (ACT) | `validate_draft` runs in ACT mode → PASS report. Genuinely insightful: flagged "no filter ⇒ LLM wakes every candle (~216 calls for a 9-day 1h window)" and "balanced preset's 1.5% risk cap overrides the 2% mechanical param." |
| update-manifest (set description) | ✘ **parity gap** | `update_manifest` "only accepts `asset_universe` and `decision_cadence_minutes` — it doesn't expose a strategy-level description field." `plain_summary` and `display_name` are "not writable through any tool I have." (#15) |

### 14. [Low — data quality] Agent system prompt asset bleed
`chatrail-test-meanrev` was requested on **ETH/USD**. The manifest correctly stored
`asset_universe: ["ETH/USD"]`, but the generated trader agent's `system_prompt` reads
*"Evaluate **BTC/USD** on 60-minute bars."* — contaminated by the **BTC** scenario created in the
immediately-prior turn (shared-session context bleed). Manifest vs prompt disagree on the traded
asset. **Fix:** derive the agent prompt's asset from the strategy's own `asset_universe`, not from
ambient conversation context.

### 15. [Med — parity] Chat can't edit strategy description / display name
`update_manifest` exposes only `asset_universe` + `decision_cadence_minutes`. The dashboard
**inspector** Manifest card (per the operator skill) edits *display name, description, asset
universe, and cadence* — so the chat rail can't do what the inspector can. There is a
`plain_summary` manifest field but no tool writes it. **Fix:** extend `update_manifest`
(or add a tool) to set `display_name` / `plain_summary`.

---

## Harness caveat (so the coding agent doesn't chase ghosts)

My Playwright harness tees the `/api/chat-rail/chat` SSE stream and counts `done` events to detect
turn completion. The dashboard also appears to issue a **session auto-title generation** request to
the same endpoint after the first turn. That extra stream desynced my per-turn counter, which is
why `results-*.json` shows `toolCalls: []` for the `create-strategy`/`set-filter` turns and a
title-like string ("Complete workspace tool inventory listed") as the `list-strategies-detail`
answer. The **rendered chips and CLI oracle are authoritative** — those turns did fire their tools.
These specific artifacts are measurement noise, **not** product bugs. (The 122s/152s timeouts and
the empty `set_filter` result, however, are real — corroborated by the oracle.)

---

## Cleanup performed (per "clean up any strategies or scenarios it makes")

All test artifacts created during this session were deleted; workspace verified back to baseline
(12 strategies, 25 scenarios):

- Strategy `chatrail-test-meanrev` (`01KV0N9HZZJ7Z8A9H2QN431Q1M`) — `DELETE /api/strategy/:id?force=true` → 204; paired agent `01KV0N9J0B5YMAGYGQM2KSYMRJ` cascade-removed (now 404).
- Scenario `chatrail-test-eth-jan` (`sc_01KV0KQRHM05V5EHN5KAM4FF8Q`) — 204.
- Scenario `chatrail-test-btc-mar` (`sc_01KV0N982M4XVK26EZ721VETW5`) — 204.
- Chat sessions I created (`01KV0MQPAG…` ACT, `01KV0NBHXF…` diagnostic) — 204 each.
- **Left intact:** the user's pre-existing "Active page" session `01KTVSE53PGR0TB9JYA0ZRMNEC`
  (it predates this test and holds the user's own history). My 15 THINK test turns are appended
  there — there is no per-message delete; flag if you want it cleared.
- **No server config was modified** (the attempt to disable `run_eval` via tool-policy was
  intentionally abandoned; see #2 — handled by keeping eval-capable prompts in THINK mode instead).
