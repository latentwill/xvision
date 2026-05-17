# Comprehensive codebase review - 2026-05-17

## Scope

Reviewed the active product code under `crates/`, `frontend/web`, `xvision-agentd`, `xtask`, `probes`, dashboard routes, engine APIs, eval executors, LLM dispatch, CLI command surfaces, settings/secrets handling, streaming/chart paths, scenario/strategy storage, and agent sidecar code.

Excluded generated/cached/non-active workspace material such as `.claude/` worktrees, vendored skill repositories, `target/`, `node_modules/`, and generated frontend type leaves except where their callers form API contracts.

No tests or runtime validation were executed during this review.

## Findings

### P1 - Engine slot tool-use loop can run forever

Files:

- `crates/xvision-engine/src/agent/execute.rs`
- `crates/xvision-engine/src/agent/pipeline.rs`
- `crates/xvision-engine/src/eval/executor/backtest.rs`
- `crates/xvision-engine/src/eval/executor/paper.rs`

`execute_slot` drives model tool calls in an unconditional `loop` and only returns when the response has no tool calls or the stop reason is `EndTurn` / `MaxTokens`. There is no iteration cap, wall-clock cap, or per-slot tool-call cap. A model that repeatedly emits `ToolUse` will keep invoking tools and re-calling the model indefinitely.

This is especially risky because eval pipelines call `execute_slot` through `run_pipeline`, so a single pathological trader/regime/intern response can wedge a whole backtest/paper run and continue spending upstream LLM/tool budget. The dashboard wizard has a `MAX_TOOL_LOOP_ITERATIONS` cap, but the engine slot executor does not.

Recommended fix:

Add a bounded loop counter to `execute_slot`, probably configurable but with a conservative default like 8-12 iterations. On exhaustion, return a typed error that includes slot role, model, tool names requested, accumulated token counts, and last stop reason.

### P1 - Agent sidecar accepts budget limits but does not enforce them

Files:

- `xvision-agentd/src/methods/session.ts`
- `xvision-agentd/src/session/store.ts`
- `xvision-agentd/src/session/build-agent.ts`
- `crates/xvision-agent-client/src/protocol.rs`

`session.start_run` validates `budget_limits.max_input_tokens`, `max_output_tokens`, and `max_wall_ms`, stores them, and returns success. `session.step` then calls `agent.run` / `agent.continue` without applying those limits, without a timeout, and without checking returned usage against the token caps.

That makes the budget contract misleading: callers can believe a run is bounded while the sidecar can exceed wall-clock and token budgets. This is a regression-prone control-plane bug, not just missing telemetry.

Recommended fix:

Enforce `max_wall_ms` with an abortable timeout around each step or run. Enforce token ceilings either by passing supported budget options into `@cline/sdk` if available, or by checking cumulative/result usage and aborting subsequent steps once caps are exceeded. Return `status: "aborted"` with a budget-specific error.

### P1 - Dashboard remote CLI endpoint can execute high-impact `xvn` commands when server is exposed

Files:

- `crates/xvision-dashboard/src/server.rs`
- `crates/xvision-dashboard/src/routes/cli.rs`
- `crates/xvision-dashboard/src/cli_jobs/runner.rs`
- `crates/xvision-cli/src/commands/dashboard.rs`
- `crates/xvision-cli/src/commands/fire_trade.rs`

The dashboard has no authentication layer. `/api/cli/jobs` accepts an arbitrary `argv` array and executes the configured `xvn` binary. Validation only rejects first arguments `dashboard` and `mcp`; high-impact commands such as `fire-trade`, provider mutation, eval launch, cache deletion, and other workspace-changing commands remain callable.

The default bind address is `127.0.0.1:8788`, which limits the blast radius for local-only use. But `xvn dashboard serve --bind 0.0.0.0:...` is supported, and any Tailscale/reverse-proxy deployment would expose unauthenticated command execution within the `xvn` command surface.

Recommended fix:

Treat remote CLI as a privileged endpoint. Add auth before any non-local bind, and replace the denylist with an allowlist of safe job templates needed by the UI, such as bars fetch. Block `fire-trade`, provider/secret mutation, destructive settings, dashboard spawning, and arbitrary subcommands unless an explicit local-only developer mode is enabled.

### P2 - Destructive dashboard APIs rely on a frontend-embedded confirm token

Files:

- `crates/xvision-engine/src/api/settings/danger.rs`
- `crates/xvision-dashboard/src/routes/settings/danger.rs`
- `frontend/web/src/api/settings.ts`
- `frontend/web/src/routes/settings/danger.tsx`

Danger operations require backend JSON `{ "confirm": "yes-i-am-sure" }`. The frontend asks the operator to type `DELETE`, but the typed phrase is only a local UI enablement guard; the API wrapper always sends the backend token constant. Because the token is shipped in the frontend bundle and routes are unauthenticated, the backend confirm string does not provide a meaningful server-side safety check to any client that can reach the dashboard.

This overlaps with the unauthenticated dashboard risk above, but it is worth tracking separately because these endpoints perform `wipe_db` and `factory_reset`.

Recommended fix:

Either require the actual operator-entered phrase to be sent and checked server-side, or replace the static token with a short-lived server-issued challenge. Also gate danger routes behind authentication or local-only checks when the server is not bound to loopback.

### P2 - Attached `Trader` role passes eval validation but is dropped from pipeline outputs

Files:

- `crates/xvision-engine/src/api/eval.rs`
- `crates/xvision-engine/src/agent/pipeline.rs`

`validate_eval_trader_source` accepts attached agent roles using `resolved.role.trim().eq_ignore_ascii_case("trader")`. `run_agent_pipeline` also applies the trader JSON schema case-insensitively. But the final output capture uses a case-sensitive match on `resolved.role.trim()` and only assigns `PipelineOutputs.trader` for literal `"trader"`.

A strategy with an attached role like `Trader` or `TRADER` passes eval preflight, runs the LLM with the trader schema, and then reports `MissingResponse` because the output was not stored in the `trader` field.

Recommended fix:

Normalize the role once, e.g. `let role_key = resolved.role.trim().to_ascii_lowercase()`, and use it consistently for schema selection, accumulated output keys, and output assignment.

### P2 - Role validation allows whitespace variants that can bypass exact graph role references

Files:

- `crates/xvision-engine/src/strategies/validate.rs`
- `crates/xvision-engine/src/strategies/agent_ref.rs`

`validate_agent_pipeline` trims agent roles for emptiness and duplicate detection, but inserts the trimmed `&str` into the role set while leaving stored roles unnormalized. Graph edge validation then checks edge roles against the trimmed role set using exact strings. That means a stored role with leading/trailing spaces can be accepted, but downstream code alternates between trimmed, untrimmed, case-insensitive, and exact role comparisons.

This has already produced concrete drift in the eval trader path and is likely to cause more graph/sequential routing inconsistencies as graph execution lands.

Recommended fix:

Normalize `AgentRef.role` at mutation boundaries: trim, reject empty, and either preserve case with exact comparison everywhere or lowercase canonical role keys everywhere. Do not allow persisted roles with leading/trailing whitespace.

### P3 - Reasoning-class truncation hint misses accepted whitespace-padded trader roles

Files:

- `crates/xvision-engine/src/eval/executor/backtest.rs`
- `crates/xvision-engine/src/eval/executor/paper.rs`

`trader_model_id` looks for attached trader roles with `r.role.eq_ignore_ascii_case("trader")` but does not trim. Eval validation accepts `" trader "` after trimming, and the pipeline output assignment uses `resolved.role.trim()`. For those accepted strategies, trader-output errors still execute but lose the model-specific reasoning-class truncation hint.

Recommended fix:

Use `r.role.trim().eq_ignore_ascii_case("trader")` in both `trader_model_id` helpers, or centralize role normalization so this cannot drift.

### P3 - Strategy filesystem store does not constrain IDs before joining paths

Files:

- `crates/xvision-engine/src/strategies/store.rs`
- `crates/xvision-engine/src/authoring.rs`
- `crates/xvision-engine/src/api/strategy.rs`

`FilesystemStore::path_for` builds paths with `self.root.join(format!("{id}.json"))`. Most creation paths generate ULIDs, and HTTP route path segments reduce the obvious slash-in-ID cases. However, store load/delete/update helpers accept arbitrary string IDs from API/MCP/tool surfaces and never validate that IDs are simple filenames.

The fixed `.json` suffix limits some direct attacks, but `../` still resolves outside the strategy root for sibling `.json` files. More importantly, the storage abstraction itself has no invariant that strategy IDs are path-safe.

Recommended fix:

Introduce a `validate_strategy_id_for_path` helper or typed `StrategyId`, and require an ASCII filename-safe pattern such as `^[A-Za-z0-9_-]+$` before any filesystem operation. Also reject IDs containing path separators, `.` segments, or platform-specific separators.

### P3 - Eval retry idempotency ignores `params_override` despite documented contract

Files:

- `crates/xvision-engine/src/api/eval.rs`

The retry comment says idempotency is based on `(agent_id, scenario_id, mode, params_override)`, but the implementation searches queued/running siblings by only `agent_id`, `scenario_id`, and `mode`. If a different run with the same strategy/scenario/mode but different parameters is currently queued or running, retry returns that unrelated run instead of starting the failed run's exact parameter set.

Recommended fix:

Include `params_override` equality in the in-flight sibling predicate, or update the comment and API semantics if coalescing across parameter overrides is intentional.

### P3 - Hold chart markers can render at price zero when bar lookup misses

Files:

- `crates/xvision-engine/src/api/chart.rs`

`build_markers` uses `bar_close.get(&t).copied().unwrap_or(0.0)` for `hold` decisions. If a decision timestamp does not exactly match a loaded bar timestamp, the hold marker is emitted at price `0.0`, which can distort chart autoscaling and visually imply a market crash.

Recommended fix:

Skip hold markers without a matching bar close, or use the nearest prior bar close within the run granularity. If a fallback is used, include a diagnostic rather than silently plotting at zero.

## Additional review notes

No finding recorded for OpenAI-compatible tool-call parsing in the current local file: the present `OpenaiCompatDispatch` only creates a text block from non-empty `message.content` and separately preserves `tool_calls` as `ContentBlock::ToolUse`. The earlier reasoning-content fallback regression reported by the reviewer was not present in this workspace snapshot.

Secrets handling generally avoids returning cleartext keys through read APIs and writes provider/broker secrets with mode `0600` on Unix. The larger risk is endpoint reachability/auth, not accidental readback through normal settings APIs.

SQL access observed in reviewed areas generally uses bound parameters. The dynamic table-name deletion in `wipe_db` pulls table names from `sqlite_master` and quotes identifiers; the primary concern there is authorization/confirmation, not SQL injection.

Frontend React rendering did not show obvious `dangerouslySetInnerHTML` use in handwritten route code. Most user/provider/model text is rendered as React text nodes.

## Recommended remediation order

1. Add a hard iteration cap to `execute_slot`.
2. Enforce `xvision-agentd` budget limits.
3. Lock down `/api/cli/jobs` and danger routes before supporting any non-loopback dashboard deployment.
4. Normalize strategy/agent roles at mutation boundaries and fix current trader-role comparisons.
5. Add path-safe strategy ID validation in `FilesystemStore` callers.
6. Fix lower-severity eval retry and chart marker edge cases.

## Second-pass additions

### P2 - Concurrent chat appends can assign duplicate message sequence numbers

**Files:**

- `crates/xvision-engine/src/chat_session/store.rs`
- `crates/xvision-engine/migrations/003_chat_sessions.sql`

`ChatSessionStore::append` computes the next message sequence with `SELECT COALESCE(MAX(seq), -1) + 1 FROM chat_messages WHERE session_id = ?1` inside a transaction. The migration only creates a non-unique index on `(session_id, seq)`, so two concurrent appends for the same session can both observe the same max sequence and insert duplicate `seq` values.

Impact: chat replay/order can become nondeterministic for sessions with concurrent writers, and no database constraint detects the corruption. If downstream logic assumes `(session_id, seq)` is unique, this can also create unstable pagination or replay behavior.

Recommendation: add a `UNIQUE(session_id, seq)` constraint or unique index, then make append retry on unique-conflict. If strict per-session serialization is preferred, move sequence allocation behind an atomic counter row or explicit write lock.

### P3 - CLI job output path has an unbounded in-memory queue despite persisted byte caps

**Files:**

- `crates/xvision-dashboard/src/cli_jobs/runner.rs`
- `crates/xvision-dashboard/src/cli_jobs/store.rs`

The dashboard CLI runner reads stdout/stderr in background tasks and forwards every chunk through `mpsc::unbounded_channel`. Persistence is capped at `MAX_PERSISTED_STREAM_BYTES`, but the producer queue itself is not bounded. A child process that emits output faster than the runner can persist and broadcast events can grow memory without a practical limit.

Impact: a noisy `xvn` command or accidental output loop can cause dashboard memory growth even after the persisted stream has already reached the truncation cap. This is especially relevant because dashboard-triggered CLI commands are long-lived enough to be exposed through job polling/SSE-like status flows.

Recommendation: replace the unbounded channel with a bounded channel and apply backpressure to stream readers, or drop/coalesce output chunks after each stream reaches the persisted cap. If live streaming must continue after persistence truncates, enforce a separate bounded live-buffer policy.

### P3 - Observability bus has a stale synchronous publish path

**File:**

- `crates/xvision-observability/src/bus.rs`

`RunEventBus::try_publish` is currently definition-only in the searched codebase, but it does not mirror all producer-side bookkeeping done by async `publish`. In particular, async `publish` populates the span-to-run map for `SpanStarted` before enqueueing, while `try_publish` only calls `try_enqueue`.

Impact if reused: dropped span-keyed events published through `try_publish` can remain unattributed or be reported under the fallback unattributed path instead of the owning run.

Recommendation: either remove `try_publish` until there is a real caller, or update it to share the same producer-side preparation path as `publish` before attempting enqueue.

### Refactor notes from second pass

- The CLI output storage path computes `chunk_index` with `MAX(chunk_index) + 1` per stream. Today the runner appears to be the only writer per job, but a unique index on `(job_id, stream, chunk_index)` would make that invariant explicit and catch accidental multi-writer regressions.
- Several store layers rely on application-level path or ID hygiene before filesystem joins. The strategy store finding above is the highest-risk instance; the same pattern should be avoided in any future store APIs by validating IDs at the boundary and keeping path construction private.
- The observability bus has both async and sync enqueue APIs with slightly different behavior. Consolidating event-preparation logic would reduce future drift around backpressure accounting.
