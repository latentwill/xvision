# Driving xvn as an agent

`xvn` is designed for an autonomous agent to drive end-to-end without bolting
Python around it. Every verb that produces state has a `--json` shape; every
blocking operation has a `--wait` posture; every failure is typed. The bar for
"fluent" is: an agent can run a full strategy-iteration loop without scraping
stdout or guessing state.

This page is the operator contract for that loop. It distills verbatim feedback
from an agent that drove xvn end-to-end and had to glue the workflow together
with Python: POST strategy / agent / eval requests, poll until runs finish,
normalize metrics, count decisions and action types, compare scenarios, preserve
IDs. None of that should be necessary.

## The loop

A canonical strategy-iteration loop looks like this:

**1. Author hypothesis**

```
xvn strategy create \
  --prompt prompt.md \
  --name "ETH 4h compression sniper v3" \
  --provider openrouter \
  --model kimi-k2 \
  --role trader \
  --asset ETH/USD \
  --timeframe 4h \
  --json
```

Returns `{"strategy_id": "...", "agent_id": "...", "eval_ready": true, "provider": "openrouter", "model": "kimi-k2", "warnings": []}`.
`eval_ready: false` means warnings are non-empty; inspect and fix before
proceeding. The `--prompt` flag activates atomic mode: strategy + agent +
slot binding happen in one transaction.

**2. Validate**

```
xvn strategy validate <strategy_id> --scenario <scenario_id> --json
```

Returns `{"strategy_id": "...", "eval_ready": true, "expected_decisions": 49, "asset": "ETH/USD", "timeframe": "4h", "warmup_bars": 200, "warnings": [], "errors": []}`.
Exit code is non-zero when `eval_ready` is false. Run this before spending eval
tokens; it checks agents, provider/model config, scenario asset/timeframe
alignment, and warmup adequacy.

**3. Pick comparator set**

```
xvn scenario select \
  --assets ETH/USD,BTC/USD,SOL/USD \
  --timeframe 4h \
  --target-decisions 49 \
  --count 4 \
  --regimes bull,bear,range,crash \
  --json
```

Or, to force a common decision count across scenarios:

```
xvn scenario select \
  --same-decisions \
  --max-decisions 105 \
  --count 4 \
  --json
```

Returns a JSON array of scenario rows with `id`, `name`, `asset`, `timeframe`,
`decision_count`, and matched regime tags. Feed the `id` fields into the next
step.

**4. Run + persist**

```
xvn experiment run \
  --name compression-sniper-v3-cross-regime \
  --question "Does tighter compression logic generalize across regimes?" \
  --strategy <strategy_id> \
  --scenarios <id1>,<id2>,<id3>,<id4> \
  --decision-budget 49 \
  --wait \
  --compare \
  --markdown \
  --json
```

Blocks until all runs reach a terminal state. Emits a single
`ExperimentRunOutput` JSON object. To run the batch without the experiment
ledger wrapper:

```
xvn eval batch run \
  --strategy <strategy_id> \
  --scenarios <id1>,<id2>,<id3>,<id4> \
  --mode backtest \
  --wait \
  --json
```

**5. Read compare + review**

`ExperimentRunOutput` fields an agent needs:

- `experiment_id` — stable ledger id for later updates
- `batch_id` — links to per-run status
- `result.profitable_count`, `result.best_scenario`, `result.worst_scenario`
- `result.runs[*]` — per-scenario `return_pct`, `sharpe`, `drawdown_pct`,
  `decisions`, `status`
- `compare_markdown` — GFM table, present when `--compare --markdown` is set

For richer per-run behavior (action distribution, avg bars held, failure mode),
fetch each run with:

```
xvn eval show <run_id> --behavior --json
```

**6. Update ledger**

```
xvn experiment update <experiment_id> \
  --conclusion "Selectivity improved; no positive edge. Needs regime gate." \
  --next-recommendation "Tighten SOL variant; add invalidation exit." \
  --json
```

**7. Repeat with next hypothesis**

Revise `prompt.md`, increment strategy version, re-run from step 1. The
experiment ledger accumulates the history; `xvn experiment ls --json` returns
all rows.

## `--json` everywhere

Every list / get / create / run / validate / compare verb supports `--json`
(or `--format json` / `--format json-compact` on object-get commands). The JSON
shapes are stable and safe for chaining via `jq`. Do NOT regex stdout — use
the exit code and the JSON object.

Examples:

```
xvn strategy ls --json
xvn scenario ls --json
xvn eval list --strategy <id> --json
xvn eval show <run_id> --json
xvn eval compare --batch <batch_id> --json
xvn experiment ls --json
xvn experiment show <exp_id> --json
```

## Exit codes

Non-zero on validation drift, failed runs, missing resources, or provider
errors. Do not infer success from string matching — read the exit code.

| Code | Enum | Meaning |
|------|------|---------|
| 0 | `Success` | Command completed |
| 2 | `Usage` | Caller-fixable: bad flag, malformed input, unknown enum variant, eval-not-ready |
| 3 | `Auth` | Missing or invalid credential (e.g. `ANTHROPIC_API_KEY`) |
| 4 | `NotFound` | Referenced resource does not exist (strategy id, run id, scenario id) |
| 5 | `Upstream` | LLM API / broker / network / filesystem / database error |
| 7 | `Conflict` | State collision (e.g. duplicate name on rename) |

`Upstream` (5) signals "external system failure; retry may help."
`NotFound` (4) signals "the id is wrong; don't retry unchanged."
`Usage` (2) signals "fix the arguments."

## `--wait` vs polling

Long-running verbs split into two shapes.

**`xvn eval run` always blocks** until the run reaches a terminal state and
then prints the final `Run` object. Pair with `--json` to get a single
machine-readable object on exit — no polling required:

```
xvn eval run --strategy <id> --scenario <id> --json
```

**`xvn eval batch run` and `xvn experiment run` take `--wait`** to switch
into the same blocking shape (otherwise they return a batch/experiment id
and complete asynchronously). With `--wait --json` they emit a clean
terminal-state object on exit:

```
xvn eval batch run --strategy <id> --scenarios <ids> --wait --json
xvn experiment run --name <slug> --strategy <id> --scenarios <ids> --wait --json
```

**Without `--wait`** (batch / experiment only): the verb returns immediately
with an id. Poll state with:

```
xvn eval batch status <batch_id> --json
```

`xvn eval watch <run_id>` prints a status line on each poll interval. It is
designed for human monitoring in a terminal, not for agent consumption; use
`xvn eval show` with a polling loop instead. SSE streaming is not part of the
CLI surface.

## Idempotency

Batch runs are coalesced by fingerprint (`agent_id`, `scenario_id`, `mode`,
`params_override`). If the same batch is submitted twice — for example, after a
transient `Upstream` failure — you will get the in-flight or completed batch id
back rather than a duplicate run. Do not build dedup logic on top of the CLI.

## Error classes

Failed runs carry a typed behavior summary on `BehaviorSummary.primary_failure_mode`
and per-run `status` fields. The canonical status values are:

- `completed` — run finished; metrics are present
- `failed` — terminal failure; check `error` field for detail
- `cancelled` — operator-cancelled
- `queued` / `running` — non-terminal; should not appear after `--wait`

Within a completed run, `behavior_summary.primary_failure_mode` classifies the
agent's decision pattern (e.g. `late_entries`, `held_through_invalidation`,
`direct_flips`). Branch on this field rather than the raw metric numbers when
diagnosing why a run underperformed.

When a batch has mixed outcomes — some `completed`, some `failed` — the batch
itself does not return a non-zero exit code; inspect `result.runs[*].status`
per scenario.

## Don'ts

**Do not shell around the CLI to glue state.** The intake feedback was exactly
"I had to bolt Python around the CLI" — that is the failure mode this surface
exists to prevent. If a workflow step requires you to write a Python loop
around `xvn`, it means the step is missing from the CLI surface; file it
as a track.

**Do not bypass `xvn` and write to `$XVN_HOME` directly.** Migrations and
validators run through the CLI seam. Direct writes to the SQLite flight-recorder
(`insert_decision`, `insert_briefing`, `insert_trace`) break replay and
metrics reproducibility and are intentionally not surfaced as CLI primitives.

**Do not use general dashboard CRUD endpoints as an agent surface.** The CLI
wraps it and is the agreed contract for most workflows. Two exceptions carry a
stability promise: the chat-rail event stream (see below) and the optimizer
convenience endpoint (`POST /api/optimize/run`, see "Running the optimizer").

**Do not use the MCP surface without explicit operator authorization.** The MCP
indicator server (`xvn-mcp`) is no longer part of the recommended agent surface
as of 2026-05-10. The MCP crate remains in the workspace for external MCP
clients but is not on the hot path. See
[CLI non-surfaced](/docs?slug=cli-non-surfaced) for the full footgun inventory.


## Running the optimizer from an agent

The optimizer flywheel (autooptimizer) has a convenience HTTP endpoint
designed for agents. It accepts a minimal JSON body and synthesizes
defaults so the caller does not need flywheel internals like embeddings,
namespaces, or pattern text.

### Quick launch

```
POST /api/optimize/run
Content-Type: application/json

{"agent_id": "<agent_ulid>"}
```

Returns an `AutoOptimizerRunDto` JSON object with the run id, namespace,
pattern_id, and promotion state. The endpoint is an exception to the
"don't use the dashboard API" rule — it carries the same stability
promise as the chat rail endpoints.

### Full fields

| Field | Type | Default | Description |
|---|---|---|---|
| `agent_id` | string | required | Agent ULID whose flywheel namespace will be optimized. |
| `pattern_text` | string | `"Auto-optimized pattern for agent <id>"` | The pattern/prompt text to optimize. |
| `active` | bool | `true` | Whether the resulting Pattern is immediately recall-active. |
| `limit` | int | `50` | Max Observation rows to distill into the pattern. |
| `min_observations` | int | `2` | Minimum Observation count required to produce a Pattern. |

### CLI equivalent

The same workflow is available via the CLI:

```bash
# Strategy-level optimization (cycle optimizer):
xvn optimize run --strategy <strategy-id> --mock
xvn optimize run --strategy <strategy-id> --provider ollama  # cross-provider

# Flywheel (memory distillation) optimization:
xvn optimize memory-demos --target-agent-id <agent-id> --json
```

Both CLI and HTTP paths are **offline-only** — they never trade live.
**Do not mint on-chain or place real orders without an explicit op-mode flag.**
`xvision-identity` (register, post_reputation) and live order submission via
`xvn fire-trade --venue orderly` against mainnet are deliberately out of the
default `xvn` binary. See `docs/cli-non-surfaced.md`.

## Driving the chat rail (conversational surface)

The sections above cover the **automation** surface: typed `xvn` verbs with
`--json` shapes for a non-interactive loop. The **chat rail** is the separate
conversational surface — a persistent session in the dashboard where an agent
(or a human) drives xvision in natural language, with a unified event stream and
server-enforced safety. Its HTTP endpoints are stable enough to drive directly,
unlike the general dashboard CRUD API.

### Unified event stream

A session emits one ordered, replayable event log. Connect with:

```
GET /api/chat-rail/sessions/:id/stream?after_seq=<n>
```

`after_seq` defaults to `-1` (replay everything). The stream **replays**
persisted events past the cursor, emits a `replay_complete` marker carrying the
`last_seq` it replayed, then **tails live** events. Reconnect after a drop by
passing the last `seq` you saw as `after_seq` — replay is idempotent because
every event has a stable `event_id` and a monotonic per-session `seq`. A gap in
`seq` means a dropped event.

Every row is a `UnifiedEvent` envelope (`event_id`, `session_id`, `run_id?`,
`span_id?`, `parent_event_id?`, `seq`, `ts`, `scope`, `actor`, `source`,
`blob_hash?`, `payload`). The `payload` is adjacently tagged
(`{ "kind": "...", "data": { … } }`); the SSE `event:` name is the same
snake_case `kind`. Key kinds:

- Session lifecycle: `session_created`, `session_resumed`, `session_interrupted`,
  `session_completed`, `session_failed`.
- Assistant output: `assistant_message_started`, `assistant_token_delta`,
  `assistant_content_block`, `assistant_message_done`.
- Tool lifecycle: `tool_requested`, `tool_policy_checked`, `tool_approved`,
  `tool_started`, `tool_delta`, `tool_finished`, `tool_failed`,
  `tool_cancelled`, `tool_denied`.
- Checkpoints: `checkpoint_created`, `checkpoint_restored`,
  `checkpoint_restore_failed`.
- Focus chain: `focus_loaded`, `focus_edited`, `focus_injected`.
- Optimization (surfaced live from offline runs): `optimization_candidate_started`,
  `optimization_candidate_metric` (carries `split`), `optimization_candidate_selected`,
  `optimization_completed`.
- Typed errors (never silent): `error_missing_capability`, `error_missing_tool`,
  `error_invalid_schema`, `error_provider_unavailable`, `error_policy_denied`,
  `error_persistence_failed`.

Terminal events (`run_finished`, `run_interrupted`, `session_completed`,
`session_failed`) close the stream. Order + dedupe on `(session_id, seq)`; do
not assume one event kind per row id.

### Research / Act mode

A session is in one of two modes, server-enforced:

```
POST /api/chat-rail/sessions/:id/mode   { "mode": "research" | "act" }
```

- **research** — read tools auto-run; **write tools are denied** before they
  execute. Safe for exploration.
- **act** — write tools are allowed, subject to the tool policy below.

Enforcement reads the **persisted mode column** before every write tool — the
client cannot assert its own mode at execution time. A write tool attempted in
research mode is denied and emits typed events with a stable code
(e.g. `write_tool_in_research_mode`); the tool never runs. Setting an invalid
mode is a validation error; an unknown session id is `404`.

#### Where to observe policy denials

A policy denial produces **two** typed events on the **unified session stream**
(`GET /api/chat-rail/sessions/:id/stream`):

| SSE `event:` name | Payload kind | When |
|---|---|---|
| `tool_denied` | `ToolDenied` | Tool-level denial row; carries `tool_name` and stable `code`. |
| `error_policy_denied` | `ErrorPolicyDenied` | Typed error row; same `code` for programmatic branching. |

The **legacy** `POST /api/chat-rail/chat` SSE carries only a
`tool_result`(denied) shim — it does **not** emit `tool_denied` or
`error_policy_denied` frames. **Harnesses that instrument the legacy SSE will
never see the typed denial events.** To observe policy denials, consume the
unified stream at `/api/chat-rail/sessions/:id/stream`.

### Three-state tool policy

Per-scope, per-tool policy overrides the class default:

```
GET /api/chat-rail/tool-policy?scope=<scope>          # omit scope ⇒ global
PUT /api/chat-rail/tool-policy   { "scope"?, "tool_name", "enabled", "auto_approve" }
```

The `(enabled, auto_approve)` pair encodes three states:

| State | enabled | auto_approve | Behaviour |
|---|---|---|---|
| Auto | true | true | Runs without prompting. |
| Ask | true | false | Needs approval before running. |
| Disabled | false | — | Hidden from the model; denied if called anyway. |

A tool absent from the list uses its **class default**: read tools default to
Auto, **write tools default to Auto** (they auto-run in Act mode with no approval
round-trip; Research mode denies all write tools regardless of policy), and
Dangerous-class tools default to Disabled. To require an approval prompt for a
write tool, set its policy to the **Ask** state (`auto_approve:false`). Note
`run_eval` is Write-class, so by default it launches a backtest in Act mode
without a confirmation step. The classifier fails safe — an unknown tool is
treated as a write. Policy is keyed by scope and scopes are isolated.

### Focus chain

Each scope has a durable `focus.md` the rail re-injects into context each turn —
a persistent intent/working-set note that survives across turns and reconnects:

```
GET /api/chat-rail/focus?scope_kind=<kind>&scope_id=<id>
PUT /api/chat-rail/focus   { focus content }
```

The file lives at `$XVN_HOME/scopes/<scope_kind>/<scope_id>/focus.md` (a
workspace-scoped focus uses a sentinel id). Path components are validated:
absolute paths, separators, `..` traversal, empty/dot/NUL components are
rejected before any I/O, so a focus write can never escape `$XVN_HOME/scopes/`.
Editing the focus changes its content hash and emits a `focus_edited` event;
re-injection each turn emits `focus_injected`.

### Checkpoints & restore

The rail writes a checkpoint before each mutating tool runs, so a session can be
rewound:

```
GET  /api/chat-rail/sessions/:id/checkpoints        # newest first
POST /api/chat-rail/checkpoints/:cid/restore
```

Restore rewinds the captured artifacts (e.g. `strategy`, `agent_slot`,
`policy`, `focus`) to the snapshot — a strategy restore is **byte-identical** to
its pre-mutation state — and emits a `checkpoint_restored` row listing what was
rewound. A restore of an unknown checkpoint is `404` and is non-destructive; a
restore whose blob is missing emits a typed `checkpoint_restore_failed` and
changes nothing.

### What stays on the automation CLI

The rail is the *conversational* surface; the `xvn` automation loop above is
still the contract for headless batch/eval/experiment work. Don't route a
non-interactive eval loop through the rail, and don't regex the SSE stream —
order and dedupe on `(session_id, seq)` and branch on the typed `kind`.

## Cross-references

- [Strategies](/docs?slug=strategies) — strategy anatomy, atomic create, and
  hypothesis manifest fields
- [Scenarios](/docs?slug=scenarios) — scenario library, select, and regime tags
- [Eval Runs](/docs?slug=eval-runs) — run lifecycle, metrics, behavior summary,
  and review pipeline
- [Experiments](/docs?slug=experiments) — experiment ledger, result_json
  structure, and update contract
- [CLI Reference](/docs?slug=cli-reference) — complete flag inventory for every
  `xvn` verb
- [Agents](/docs?slug=agents) — capabilities, diagnostics readiness, and the
  Improve-this-agent flow
- [Optimizer](/docs?slug=optimizer) — offline prompt/demo optimization and
  the optimization event kinds surfaced in the rail
