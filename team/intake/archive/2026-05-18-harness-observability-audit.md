# Intake — 2026-05-18 — Harness observability + reliability audit

> **GATED — do not start, do not merge, until the operator ships an image build first.**
>
> All seven findings (F-1..F-7), the open contract, and PR #277 are
> held until the next `scripts/deploy-image.sh` cut goes out. Reason:
> operator wants a known-good image of current state before any harness
> changes ship. Once the image is deployed and verified, flip the
> contract status back to `ready` (or merge PR #277 directly) and
> proceed with F-2 decomposition.
>
> Set: 2026-05-18 by operator.


Audit of the agent harness / observability / reliability stack against a
proposed "boring, strict, observable, hard to fool" target spec
(span taxonomy, deterministic validators, typed recovery playbooks).
Source prompt + full evaluation transcript: 2026-05-18 chat session
("Let's evaluate this observability improvement thread"). Audit covered
`xvision-observability`, the agent pipeline runner, intern backends, MCP
tool schemas, API contracts, prompt assembly, and trace artifact format.

This intake is **harness-only**. It does not propose any change to the
trading strategy prompt, the per-agent system prompt, or the trading-style
defaults. The harness should become the adult; the agent stays creative.

## Source

Internal evaluation, 2026-05-18 (Ed). Findings cite `file:line` against
`origin/main` at intake time. Re-verify before claiming each contract —
some of these areas are also touched by `agent-run-observability-followups`
and the round-2/round-3 QA waves.

## Already in flight (do not respawn)

- `qa-trace-broker-spans` — adds Buy/Sell/Close/Short broker calls as
  trace spans. Adjacent to F-2 below; coordinate.
- `qa-retention-prompt-storage-bug` — touches the redactor + payload
  storage asymmetry. Sets the lower bound for F-1 (prompt hashing) since
  it owns `redactor.rs` / `sqlite.rs` this wave.
- `agent-run-observability-blob-fetch-route` — adds the route that lets
  the dashboard hydrate `prompt_payload_ref` / `response_payload_ref`.
  Once F-1 lands real digests, the existing blob-fetch surface will start
  resolving for the first time.

## Out of scope for this wave

- Changes to the user-facing trading prompt (`AgentSlot.system_prompt`).
  The harness must work with any prompt the operator writes.
- Hardcoding a trading style (long/short bias, asset class, timeframe).
- Replacing `garde` with a different validator. Field-level discipline
  already works; the gap is the *unified pre-persist pass*.
- OpenTelemetry adoption — `otel_trace_id` / `otel_span_id` already exist
  on the span schema; this audit does not propose an OTel collector.
- A V2 chat-history / multi-session surface. Out of scope (V2 board).

## Findings → proposed tracks

Six tracks, sized from "hours" to "wave." F-codes for the shared track
namespace. The first two are small and unblock everything downstream;
3-6 are the typed-recovery + validator investment.

| # | Severity | Finding | Proposed track | Lane | Est. |
|---|---|---|---|---|---|
| F-1 | P1 | `ModelCallFinished.prompt_hash` is faked as `format!("eval:{run}:{span}")` (`crates/xvision-engine/src/agent/observability.rs:244`). Two identical prompts hash differently; dedup, cache-correctness, and prompt-version inference are impossible. `response_hash` is always `None`. | `harness-prompt-hash-real-digest` | leaf | S |
| F-2 | P2 | `SpanStartedEvent.attributes_json` is `Option<String>` but every emission site passes `None` (`crates/xvision-observability/src/bus_subscriber.rs:221` + engine-side). Schema is wired; bag is empty. Populating `run_id`, `agent_id`, `stage`, `model`, `provider`, `tool_name`, `retry_count` on existing spans is mechanical. | `harness-span-attrs-populate` | leaf | S |
| F-3 | P2 | No `prompt_version` field on `agent_slots`. Prompt updates are silent overwrites via `AgentStore::update()`. Once F-1 lands, content-hash the prompt and persist the digest. Requires a migration (coordinate via `team/MANIFEST.md` registry — note: registry shows 005 but `migrations/` is at 018; manifest is stale). | `harness-prompt-version-field` | foundation | M |
| F-4 | P1 | Four missing spans drive forensics: `tool.validate_input`, `tool.validate_output`, `recovery.attempt`, `state.transition`. The other gaps (`context.assemble`, `prompt.render`, `model.parse`, `tool.select`, `artifact.validate`, `recovery.failed`) are nice-to-have. Add the four as new `SpanKind` variants in `xvision-observability/src/types.rs` and emit them from the pipeline runner. | `harness-span-taxonomy-extension` | integration | M |
| F-5 | P1 | Recovery is essentially absent: 1 JSON-decode retry (`RESPONSE_DECODE_RETRIES=1`, `agent/llm.rs:117`) + 12-iter tool-use cap (`MAX_TOOL_LOOP_ITERATIONS=12`, `agent/execute.rs:30`). `classify_run_failure` (`eval/executor/mod.rs:48`) is a regex-on-error-string post-hoc classifier — promote it to a typed pre-recovery dispatcher with the six playbooks (MALFORMED_JSON, TOOL_TIMEOUT, SCHEMA_MISSING_FIELD, EMPTY_DATA, CONTEXT_OVERFLOW, REPEATED_TOOL_FAILURE). Every loop hard-capped. | `harness-recovery-state-machine` | integration | L |
| F-6 | P2 | `Strategy.mechanical_params: serde_json::Value` (`strategies/mod.rs:59`) is an untyped escape hatch — template-specific params skip all validation. `InternBriefing` and `RiskConfig` lack `#[serde(deny_unknown_fields)]`. Type `mechanical_params` per template; tighten serde discipline on trading types. | `harness-typed-mechanical-params` | integration | M |
| F-7 | P2 | Once F-2 / F-4 land, the trace dock will be noisier — many more spans per run (validate_input/output, recovery attempts, state transitions) and a populated attribute bag per span. Operators reviewing a run need a `Simple | Advanced` view toggle on the trace dock: **Simple** hides intermediate / instrumentation spans (`context.assemble`, `prompt.render`, `tool.validate_*`, `state.transition`) and collapses the attribute bag to one summary line per span; **Advanced** shows everything. Persisted per-user via the existing trace-dock store. Pure frontend; no backend dependency once the spans exist. **Gated on F-2 + F-4 — do not start until those land** (otherwise there's nothing to hide). | `trace-dock-simple-advanced-toggle` | leaf | S |

Seven tracks. Three leaves (F-1, F-2, F-7). One foundation needing a
migration (F-3). Three integration (F-4, F-5, F-6).

## Track summaries

### F-1 `harness-prompt-hash-real-digest` (P1, leaf, S)

Replace the synthetic `prompt_hash` in `ObsEmitter::emit_model_call_finished`
with a real SHA-256 of the assembled prompt (system_prompt + serialized
messages + tools description). Wire `response_hash` similarly from the
assistant text accumulator already built at `agent/execute.rs:204-219`.
Both hashes flow into `model_calls` and are exposed in the trace dock /
SpanInspector. Bug fix only — no new spans, no schema migration, no
behaviour change beyond the hash columns becoming meaningful.

This is filed as a ready contract this wave (see board).

### F-2 `harness-span-attrs-populate` (P2, leaf, S)

Define a typed `SpanAttributes` struct in `xvision-observability` with
optional fields (`run_id`, `agent_id`, `stage`, `model`, `provider`,
`tool_name`, `retry_count`, `prompt_version`). Serialize to the existing
`attributes_json` column. Update emission sites in
`crates/xvision-engine/src/agent/observability.rs` and the bus subscriber
to populate the bag from the data already in scope at each call site.
No schema migration — the column is already JSON. SpanInspector renders
the bag as a key-value grid.

### F-3 `harness-prompt-version-field` (P2, foundation, M)

Add `prompt_version TEXT NOT NULL DEFAULT ''` to `agent_slots` (migration
019 if reserved this wave — coordinate via `team/MANIFEST.md` registry,
which needs a stale-registry refresh anyway). On insert/update, compute
`sha256(system_prompt + response_schema_json)[..16]` as the version.
Surface in the agent edit UI as a stable identifier. Once F-2 ships,
include `prompt_version` in span attributes so traces can be partitioned
by exact prompt content.

### F-4 `harness-span-taxonomy-extension` (P1, integration, M)

Extend `SpanKind` in `xvision-observability/src/types.rs` with four new
variants: `ToolValidateInput`, `ToolValidateOutput`, `RecoveryAttempt`,
`StateTransition`. Emit them from the pipeline runner. `tool.validate_*`
brackets each tool call with the typed-schema check (today there is none —
see F-6 for the validator itself). `state.transition` fires on every
`RunStatus` change in `RunStore::update_status`. `recovery.attempt` is
the seam F-5 hooks into.

### F-5 `harness-recovery-state-machine` (P1, integration, L)

Promote `classify_run_failure` from regex-on-error-string to a typed
pre-recovery dispatcher. New enum
`FailureClass { MalformedJson, ToolTimeout, SchemaMissingField, EmptyData, ContextOverflow, RepeatedToolFailure, Unrecoverable }`
with a bounded recovery policy per variant:

- `MalformedJson` → repair-prompt the model once with the parse error,
  then fail closed.
- `ToolTimeout` → retry the same tool once with backoff; on second
  failure surface as `ToolCallFailed` to the agent (let it self-heal)
  and emit `recovery.failed`.
- `SchemaMissingField` → targeted patch prompt (only the missing fields,
  not the whole response), once.
- `EmptyData` → emit `data_availability_failure` and stop the cycle.
- `ContextOverflow` → summarize history via cheap-model dispatch, retry
  once. Hard cap on summarize budget.
- `RepeatedToolFailure` → block the exact `(tool_name, input_hash)` pair
  for the rest of the run. Counter lives in pipeline scope; resets on
  next cycle.

Every transition emits `recovery.attempt` / `recovery.failed` spans
(needs F-4 to land first or stack). Every loop has a maximum count.

Adjacent: `agent-error-feedback-self-healing` (round-3 QA) already
covers the broker-error self-healing path. This track generalizes the
pattern across all six classes.

### F-6 `harness-typed-mechanical-params` (P2, integration, M)

Replace `Strategy.mechanical_params: serde_json::Value` with a typed
enum keyed on template id (one variant per template, each with its own
typed struct). Add `#[serde(deny_unknown_fields)]` to `InternBriefing`,
`RiskConfig`, `RiskCaps`. Add cross-field invariants (e.g., TP > SL for
long positions) via garde's custom validators. The validator pass is
called once before persistence — single seam, not scattered.

### F-7 `trace-dock-simple-advanced-toggle` (P2, leaf, S — gated)

After F-2 (attribute-bag populate) and F-4 (four new span kinds) land,
the trace dock surfaces materially more spans + per-span metadata. A
`Simple | Advanced` segmented toggle in the trace dock header lets
operators triage runs without drowning in instrumentation noise.

- **Simple** (default): hide spans of kind `context.assemble`,
  `prompt.render`, `tool.validate_input`, `tool.validate_output`,
  `state.transition`. Collapse the `SpanInspector` attribute bag to a
  one-line summary (`agent · model · tool · retry=N`). Recovery spans
  (`recovery.attempt` / `recovery.failed`) are visible in both modes —
  they always matter.
- **Advanced**: show all spans, full attribute bag, raw `error_json`.

Persistence: piggyback on the existing trace-dock zustand store
(`frontend/web/src/stores/trace-dock.ts`) — one new boolean
`advanced_view` with the same persistence semantics as the dock-height
slider. Default `false`.

Implementation hint: filter at the `AgentRunIndentedTimeline` /
`AgentRunRailTree` render boundary; do not refetch — the bag is already
on the client.

**Hard gate:** do not start before F-2 + F-4 merge. Without the new
span kinds and the populated attribute bag, "Simple" and "Advanced"
render identically and the toggle is dead UI. Track stays as a
documented follow-up in this intake until then.

## Audit detail

Full audit transcript with file:line evidence preserved in the chat
session. Key gap counts:

- Span taxonomy: 11 spans exist vs 15-stage target (~55% landed; intermediate
  stages absent).
- Deterministic validators: field-level garde discipline strong (~50%);
  unified pre-persist pass missing.
- Failure-mode playbooks: ~15% — two hard caps + one post-hoc string
  classifier. No typed state machine.

Verbatim findings:

> Critical bug: `model_calls.prompt_hash` is faked as
> `format!("eval:{run}:{span}")`. Two identical prompts hash differently.
> Dedup and cache-correctness are impossible until this is real.

> `SpanStartedEvent.attributes_json` is `Option<String>` and tests
> uniformly pass `None`. The schema comment explicitly calls it a "Small
> attribute bag (NOT the full payload)." Schema is wired; bag is empty.

> Recovery is essentially absent. Two hard caps
> (`RESPONSE_DECODE_RETRIES=1`, `MAX_TOOL_LOOP_ITERATIONS=12`) and one
> post-hoc classifier. No typed recovery state machine, no targeted
> patching, no tool-input dedup, no context-overflow handling.

## Sequencing

1. F-1 first. Unblocks everything that depends on real prompt content.
2. F-2 in parallel with F-1 (different files).
3. F-3 after F-1 lands and migration 019 is reserved.
4. F-4 + F-5 stack — F-5 emits the spans F-4 defines.
5. F-6 independent; can land any time after F-2.
6. F-7 gated — only contract once F-2 + F-4 are merged.

F-1 is filed as a contract this wave. F-2 through F-7 wait for the
conductor's next decomposition pass.
