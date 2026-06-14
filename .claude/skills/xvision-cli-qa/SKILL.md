---
name: xvision-cli-qa
description: Use when QAing the xvision app through direct API or CLI calls, especially to verify Strategy, Scenario, and Eval create/edit/delete behavior, detect manifest drift, duplicate records, or invalid model resolution, and collect raw HTTP evidence without the browser UI.
---

# xvision CLI QA

## Overview

This skill covers xvision QA from the raw HTTP surface. Use it when the browser is unnecessary, flaky, or you need the contract-level truth behind the UI.

## When to Use

Use this when:
- verifying list/detail endpoints mirror the UI
- testing create, edit, delete, archive, attach, detach, validate, and launch flows
- checking scenario duplication or stale data at the API layer
- diagnosing eval failures caused by provider/model resolution
- comparing raw JSON responses before investigating UI behavior

Do **not** use this for visual layout checks, control alignment, or wizard screenshot review; use the chat-rail UI skill for that.

## Core Loop

1. **Discover the routes**
   - Hit list, detail, and mutation endpoints directly.
   - Confirm HTTP methods and allowed verbs with `OPTIONS` when needed.

2. **Create a disposable resource**
   - Prefer a temp scenario or temp eval run.
   - Use real payload shapes from existing objects.

3. **Mutate the smallest stable unit**
   - Strategy: attach/detach roles, validate, inspect manifest drift.
   - Scenario: create, delete, and confirm the list view updates.
   - Eval: create, poll, inspect failure reason, then delete.

4. **Verify the contract, not the story**
   - Compare request payloads, response bodies, and status codes.
   - If the API says success but the object is inconsistent, treat it as a bug.

5. **Clean up**
   - Delete temp scenarios and eval runs.
   - Record whether cleanup endpoints exist or are missing.

## Quick Checks

### Strategy
- `GET /api/strategy/:id`
- `PATCH /api/strategy/:id` — editable manifest metadata, including display name, summary, asset universe, cadence, and color
- `PUT /api/strategy/:id/filter` / `DELETE /api/strategy/:id/filter` — strategy-level deterministic filter artifact
- `POST /api/strategy/:id/agents`
- `PATCH /api/strategy/:id/agents/:role`
- `DELETE /api/strategy/:id/agents/:role`
- `POST /api/strategy/:id/validate` — returns `eval_ready` + `warnings[]` + `errors[]`
- `POST /api/strategy` with `prompt` body — atomic mode: creates Agent + Strategy in one call
- `POST /api/strategy` with `hypothesis` block — attaches a `Hypothesis`
  to the strategy (`family`, `statement`, `target_regimes[]`, `avoid_regimes[]`)

CLI peers:
- `xvn strategy new --prompt … --json` (atomic mode)
- `xvn strategy new --family … --hypothesis … --target-regime … --avoid-regime …`
- `xvn strategy add-agent / remove-agent / set-pipeline / migrate-agents`
- `xvn strategy validate <id>`
- `xvn strategy filter-catalog --json`
- `xvn strategy set-filter <id> --from-json <path>`

Watch for:
- manifest fields disagreeing with slot prompts
- validation passing despite drift
- `eval_ready: true` returned while warnings/errors describe blockers
- prompt text that claims a filter exists while the strategy has no saved filter artifact
- strategy cadence/asset universe mismatches with the intended scenario
- atomic mode partially succeeding (Agent created, Strategy not, or vice versa)
- missing delete/archive for the strategy entity itself
- `temperature` not threading through to the live agent slot
  (`AgentSlot.temperature`, commit `ad9b1f7`)

### Scenario
- `POST /api/scenarios`
- `GET /api/scenarios`
- `GET /api/scenarios/:id`
- `DELETE /api/scenarios/:id`
- `POST /api/scenarios/:id/classify` — auto-derive regime labels from bars
- `POST /api/scenarios/:id/set-regime` — operator-authored labels
- `GET /api/scenarios/select?asset=…&timeframe=…&count=…` — comparable set query

CLI peers:
- `xvn scenario classify <id>` / `--all` / `--force`
- `xvn scenario set-regime <id> --regime … --volatility … --direction …`
- `xvn scenario select --asset … --timeframe … --count …`
- `xvn scenario inspect <id> --card`

Watch for:
- duplicate records returned by the list endpoint
- required fields that differ from the UI form's apparent defaults
- cleanup confirming `404` after delete
- `classify` overwriting operator-set labels without `--force`
- `regime_derived` flag flipping incorrectly between auto-derive and
  operator-set paths
- `select` returning scenarios whose decision count drifts wildly
  from the requested `--target-decisions`

### Eval
- `POST /api/eval/runs`
- `GET /api/eval/runs`
- `GET /api/eval/runs/:id`
- `GET /api/eval/runs/:id/results`
- `GET /api/eval/runs/:id/export` — canonical `EvalRunExport` (q15 §3);
  byte-identical to `xvn eval export <run_id>`
- `POST /api/eval/runs/:id/attest`
- `POST /api/eval/runs/:id/review`
- `GET /api/v2/charts/annotated/:run_id` — reads persisted review
  annotations for real run ids; `/demo` remains fixture-backed
- `POST /api/eval/runs/validate` — preflight without launching
- `POST /api/eval/batch` — multi-scenario batch
- `DELETE /api/eval/runs/:id`
- `GET /api/eval/compare?ids=…` — `ComparisonReport`; includes
  `strategy_name` when the strategy manifest is available, while retaining
  `id` and `agent_id` for addressing

CLI peers:
- `xvn eval run / list / show / results / watch / scenarios`
- `xvn eval run --auto-fire-review --max-review-annotations 8` and
  `xvn eval show <run_id>` for annotation auto-fire state
- `xvn eval compare … --markdown --sort sharpe` — table and markdown modes
  show readable strategy labels plus adjacent ids
- `xvn eval batch --strategy <id> --scenarios sc_a,sc_b,sc_c --wait`
- `xvn eval validate / attest / export / review`

Watch for:
- model/provider resolution mismatches
- runs queuing successfully and then failing on the first decision
- invalid JSON from the trader slot
- an eval launched before the safe agent path was exercised:
  provider readiness → `strategy diagnostics` → `eval validate` → `eval run`
- empty `filter_events` / `filter_summaries` on a run that was supposed to test the XVN filter subsystem
- conclusions drawn from synthesized rows (`noop_skip`, graph-gated trader skips, early-stop inheritance) without separating them from direct model decisions
- `EvalRunExport` JSON not byte-identical between `GET /export` and
  `xvn eval export <run_id> --output …`
- batch endpoint returning success when a subset of runs failed to enqueue
- compare `Baseline (buy_hold)` column missing or NaN for runs that
  should have a baseline arm
- compare views showing raw strategy ids where `strategy_name` is populated
- short run id labels (`shortRunId`) collapsing two distinct runs to
  the same display string

### Inline Filter DSL
- `xvn strategy filter-catalog --json` — machine-readable catalog for
  chat rail and CLI agents
- `xvn strategy set-filter <strategy_id> --from-json <path>` — installs
  a deterministic inline gate and switches the strategy to `filter_gated`

QA payloads should include required fields `display_name`,
`asset_scope`, `timeframe`, and `conditions`. For LLM-triggered gates,
also exercise optional `fire` metadata:

```json
{
  "fire": {
    "reason": "trend_breakout",
    "priority": 0.85,
    "tags": ["trend", "breakout"],
    "context": ["close", "opening_range_high_30", "adx_14", "rvol_tod_20"]
  }
}
```

Watch for:
- invalid indicator aliases not normalized to catalog tokens
- `crosses_above` / `crosses_below` accepting numeric RHS
- missing top-level filter fields returning vague "internal error"
- `fire.context` indicators missing from trace attrs or trader briefing
- catalog JSON missing new tokens such as `rvol_tod_<period>`,
  `volume_zscore_<period>`, and `opening_range_high_<minutes>`
- **Filter-gated agent** behavior being mislabeled as rules-only merely
  because many candles are skipped; if the filter passes, a configured
  agent/model must still be callable
- **Rules-only mechanical** behavior being reported as broken missing-agent
  state when the strategy intentionally avoids model calls
- **Agent-direct** behavior being presented as filtered because prompt wording
  mentions a filter, while no saved filter artifact exists

### Experiment ledger
- `POST /api/experiments`
- `GET /api/experiments`
- `GET /api/experiments/:id`
- `PATCH /api/experiments/:id`
- `POST /api/experiments/:id/run` — orchestrate pick → batch → bind → result_json

CLI peers: `xvn experiment new / ls / show / update / run`

Watch for:
- `experiment run --decision-budget` silently capping run length (it
  must be metadata-only — the pipeline still runs every cadence-gated
  decision per scenario)
- selector mode (`--assets / --timeframe / --regimes / --count`)
  returning a scenario set that violates the requested constraints
- the bound `run_ids` slot on the experiment row not reflecting the
  actual batch
- `result_json` summary diverging from the per-run `EvalRunExport`

### Agent records
- `GET /api/agents/:id` — shape matches `agents[]` slot inside `EvalRunExport`

CLI peer: `xvn agent get <agent_id>`

### Capability diagnostics (launch readiness)
CLI surfaces (no dedicated HTTP endpoint exercised here):
- `xvn strategy diagnostics <id> [--json]` — whole-strategy launch gate;
  exits **14** (`OptValidation`) when not launchable, **4** (`NotFound`) for an
  unknown id. JSON carries `launchable`, `required_capabilities[]`,
  `required_unmet[]` (each with typed `status.kind`), `optimizable[]`.
- `xvn agent inspect <id> --diagnostics [--json]` — per-capability state
  (`has_prompt`, `has_model_binding`, `required_tools`, `runtime_supported`,
  `optimizable`); exits 0 for a resolved agent.

Watch for:
- a strategy reported `launchable: true` while a required capability is missing
  a prompt / model / tool — diagnostics must NOT pass an incomplete strategy
- a **Rules-only mechanical** strategy being confused with an incomplete
  agent-backed strategy; no-agent mechanical execution should be explicit in
  the strategy/reporting surface, not inferred from missing model config
- exit code drift: not-launchable must be **14**, not 2; unknown id must be **4**
- typed `status.kind` not matching the unmet reason
  (`missing_tool` / `missing_prompt` / `missing_model_binding` / `unsupported`)
- `optimizable[]` listing a non-`trader`/`filter` capability (only those have
  DSPy signatures today)
- `agent inspect --diagnostics` failing non-zero just because a capability is
  incomplete (state-only; it must exit 0 for a resolved agent)

### Optimizer CLI (`xvn optimize`)

> `xvn optimizer` is **deprecated** and delegates to `xvn optimize`. QA the `xvn optimize` surface directly.

CLI surface:
- `xvn optimize run --strategy <id> [--cycles N] [--mock]`
- `xvn optimize run-cycle [--strategy <id>] [--mock] [--session-id ...] [--budget N] [--provider P] [--model M] [--day-start DATE] [--day-end DATE] [--baseline-start DATE] [--baseline-end DATE] [--objective METRIC] [--experiments-per-cycle N]`
- `xvn optimize mutate-once --parent-bundle-hash <hex> [--config PATH] [--cycle-id ID] [--dry-run] [--db PATH] [--blob-dir PATH] [--mock]`
- `xvn optimize demo [--fixture PATH] [-v]`
- `xvn optimize inspect <run-id> [--json]`

Watch for:
- **`xvn optimizer` not printing a deprecation warning** — every `xvn optimizer` sub-command must emit a deprecation notice before delegating
- **`xvn optimizer` sub-commands not delegating correctly** — `run-cycle`, `mutate-once`, `demo`, `inspect`, `ls`, `gate`, `activate`, `retire`, `lineage` must all delegate to the `xvn optimize` equivalents
- **removed sub-commands still responding** — `memory-demos`, `memory-demos-gate`, `accept-as-child-agent`, `revert-accepted`, `export-demos`, `import-demos`, `explain-missing-data` must return a not-found / unrecognized-subcommand error, not silently succeed
- `xvn optimize run` without `--strategy` — strategy ID is required; must fail with a clear error
- `--dry-run` on `mutate-once` mutating the store (it must validate only)
- **mock mode** (`--mock`) writing real state to the DB or blob dir
- `xvn optimize inspect` returning 404 for an unknown run-id (must not panic or return 0)

### Chat rail (unified stream + safety)
- `GET /api/chat-rail/sessions/:id/stream?after_seq=<n>` — replay past the
  cursor → `replay_complete{last_seq}` → live tail (default `after_seq=-1`)
- `POST /api/chat-rail/sessions/:id/mode` `{ "mode": "research"|"act" }`
- `GET/PUT /api/chat-rail/tool-policy` (`{ scope?, tool_name, enabled, auto_approve }`)
- `GET/PUT /api/chat-rail/focus`
- `GET /api/chat-rail/sessions/:id/checkpoints`,
  `POST /api/chat-rail/checkpoints/:cid/restore`

Watch for:
- **write tool executing in research mode** — it must be denied BEFORE execution
  and emit a `tool_denied` row; a side effect (strategy/scenario mutated) is a
  hard bug
- a **spoofed client mode** bypassing enforcement — the server reads the
  persisted mode column, not anything the client asserts at execution time
- `set mode` accepting an invalid value (must validate to research/act), or
  returning anything but 404 for an unknown session id
- tool-policy three-state drift: Disabled tool offered to the model or running;
  Ask tool (`enabled=true, auto_approve=false`) auto-running without approval
- an **unknown tool** not failing safe to write
- **policy-denial events on the wrong stream** — a policy denial produces typed
  `tool_denied` + `error_policy_denied` events on the **unified session stream**
  (`GET /api/chat-rail/sessions/:id/stream`) only.  The legacy
  `POST /api/chat-rail/chat` SSE carries only a `tool_result`(denied) shim and
  never emits these typed frames.  Harnesses instrumenting the legacy SSE will
  never observe policy denials; they must read the unified stream.  If a harness
  reports "no tool_denied event seen," check which stream it is subscribing to
  before filing a denial-emission bug.
- stream not replaying idempotently on reconnect, gaps in `seq`, or duplicate
  `event_id`s changing row state (reducer must order/dedupe on `(session_id, seq)`)
- `replay_complete` missing or carrying the wrong `last_seq`
- **focus path traversal** — `..`, absolute paths, separator-bearing scope
  components writing outside `$XVN_HOME/scopes/`
- restore of an unknown checkpoint not returning 404, or a failed restore
  (missing blob) mutating state instead of emitting `checkpoint_restore_failed`
- a strategy restore that is NOT byte-identical to the pre-mutation state
- delete-session not cascading its persisted events

### Agent-run observability
- `GET /api/obs/retention`, `PATCH /api/obs/retention`
- `POST /api/obs/janitor`
- `GET /api/runs/:run_id/inspect` — materializes `xvn_run.json` + `xvn_report.md`

CLI peers: `xvn obs retention / janitor`, `xvn run inspect <run_id>`

### Provider
- `GET /api/providers`
- `GET /api/providers/:name`
- `POST /api/providers/:name/check`
- `POST /api/providers/:name/refresh-models` — hits `/v1/models`
- `GET /api/providers/:name/models` — cached only, no network

Watch for:
- the dashboard's model dropdown deduping a provider's catalog — the
  list must be shown verbatim per provider (user feedback memory:
  `feedback_no_model_dedup.md`)

## Common Bugs

- strategy validates while prompts and manifest still disagree
- atomic-mode strategy create returns 200 with `eval_ready: false`
  but no `warnings` / `errors` explaining why
- eval uses an upstream model ID that does not exist for the configured provider
- a "filtered" strategy exists only as prompt language; no filter artifact is attached
- eval result analysis treats early-stop / `noop_skip` rows as direct trader decisions
- duplicate scenarios appear in dropdowns because the API already has duplicate rows
- strategy-level deletion is absent even though agent-role mutation exists
- `classify --all` silently skips scenarios because `regime_derived = false`
  even when the operator expected a fresh sweep (use `--force`)
- `eval batch` reports success but one of the underlying runs is in `failed`
- `strategy diagnostics` reports `launchable: true` for an incomplete strategy,
  or returns exit 2/0 instead of 14 when not launchable
- a no-agent rules-only strategy is presented as a broken/missing-agent eval,
  or a broken/missing-agent strategy is excused as rules-only without an
  explicit mechanical-mode declaration
- `xvn optimizer` sub-commands not printing a deprecation notice before delegating to `xvn optimize`
- `xvn optimize run` accepting a call without `--strategy` (strategy ID is required)
- removed sub-commands (`memory-demos`, `accept-as-child-agent`, `export-demos`, `import-demos`, `explain-missing-data`) responding with exit 0 instead of a not-found error
- a chat-rail write tool runs in research mode, or a spoofed client mode
  bypasses the persisted-mode enforcement
- **policy-denial events not visible to a harness** — `tool_denied` and
  `error_policy_denied` are emitted on the **unified session stream** only
  (`/api/chat-rail/sessions/:id/stream`); the legacy `POST /api/chat-rail/chat`
  SSE carries only a `tool_result`(denied) shim.  Before filing a
  "denial events missing" bug, confirm the harness reads the unified stream,
  not the legacy SSE.

## Evidence to Capture

Always save:
- exact request payload
- response status code
- raw JSON body
- any error message from the run detail
- before/after list output for create/delete checks

## Reporting Format

- **Finding:**
- **Severity:**
- **Repro:**
- **Expected:**
- **Actual:**
- **Evidence:**
- **Likely cause:**
- **Recommendation:**

## wake_when_in_position — verify the default and sparse-decision behavior

`wake_when_in_position` controls whether the trader LLM is re-invoked while a
position is open. The runtime default is `"on_invalidation_or_target_only"`
(NOT `"always"` — the old per-bar-polling default was removed). When QAing a
filter-gated strategy, confirm the persisted value and the resulting decision
count rather than assuming.

- **Confirm the persisted default.** When `set-filter` omits the field, the
  saved filter must read `"on_invalidation_or_target_only"`:
  ```bash
  xvn strategy show <strategy_id> --json | jq '.filter.wake_when_in_position'
  # expect: "on_invalidation_or_target_only"
  ```
  If any surface (wiki, API echo, UI) reports `"always"` as the default, that
  is drift — the canonical source is
  `crates/xvision-filters/src/types.rs::default_wake_in_position`.
- **Sparse-decision runs are expected, not a bug.** With the default, the
  filter does NOT re-fire while a position is open, so a strategy whose entry
  condition stays true and has no distinct exit signal can complete a whole
  backtest with only 1–2 decisions. Before filing a "no decisions" finding,
  check the run detail for `filter_blocked` events with
  `reason = position_open` — that confirms the gate suppressed re-fires by
  design. The fix is a real exit signal or `risk.stop_loss_atr_multiple`, not
  changing the wake mode.
- **Decision-count sanity by mode.** The three tokens trade decisions for
  cost: `"never"` (fewest), `"on_invalidation_or_target_only"` (default),
  `"always"` (one trader-LLM call per in-position bar — most). One session
  recorded a 21-trade SORB strategy producing 14 decisions with `"never"` vs
  153 with `"on_invalidation_or_target_only"`. A huge decision jump after only
  a wake-mode change is the expected cost signature, not a regression.

## Indicator warmup requirements

Some indicators require more history than the standard warmup. Check before running evals:

| Indicator | Required warmup | Notes |
|---|---|---|
| `rvol_tod_N` | N × bars_per_day same-slot bars | For 15m: N×96; for 1h: N×24 |
| `highest_N` / `lowest_N` | N bars | May include current bar (verify runtime behavior) |
| `opening_range_high_30` | Requires market-open alignment | Fine for US equity 15m |

**Example:** `rvol_tod_20` on a 15m strategy needs 20×96=1920 bars of same-slot history. A 14-day 15m scenario has ~1344 bars — insufficient. Use `volume_zscore_20` instead (20-bar warmup only).

Run `xvn eval validate` against a scenario before launching — it prints warmup warnings.

## Prompt-level regime gates are unreliable

Qwen3-4B and similar models regularly ignore soft rules in the presence of concrete indicator values. Example: "if adx_14 > 25, output FLAT" was overridden ~33% of the time during eval sessions.

**Rule:** Regime gating MUST be in the filter DSL (deterministic) to be reliable. Prompt-only gates are advisory at best.

## Eval chain race condition

If a chain script is killed by PID, the background subprocess may continue running — launching duplicate evals. See the xvision-cli skill for the process group kill pattern. Always check `pgrep -f eval_chain` before starting a new chain.

## References

See `references/xvision-api-quirks.md` for the concrete endpoint quirks, payload shapes, and failure messages observed during QA.

---

*Skills owner: any track that adds or changes an `/api/*` route, the
corresponding `xvn` verb, Filter DSL contract, or a QA-critical operator
workflow is responsible for updating this file in the same PR. Last
refresh: 2026-06-13 (wake_when_in_position QA section: default verification,
sparse-decision/`filter_blocked` behavior, decision-count-by-mode sanity).*
