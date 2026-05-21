# Agent daemon (`xvision-agentd`)

`xvision-agentd` is the TypeScript daemon that fronts model dispatch for
the engine. It runs as a child of the `xvn` binary, accepts JSON-RPC
2.0 requests over a Unix Domain Socket (UDS) framed as NDJSON, and
emits run-lifecycle events back to the engine over a separate event
socket. Every model call the engine makes during an `xvn eval run` or
`xvn experiment run` is routed through this daemon — keeps provider
SDKs (Anthropic, OpenAI, OpenRouter) out of the Rust binary and gives
us a single TypeScript surface for tool-shim plumbing.

Source: `xvision-agentd/src/`. Bundled with the binary; no separate
install.

---

## When you'd talk to it directly

Most of the time you don't — the engine drives it. You only need this
page if you're:

- writing an alternative client (different runtime / language) that
  drives the same daemon,
- debugging a session that misbehaved and want to replay the wire
  traffic by hand,
- adding a new method (start here to understand what the daemon
  already exposes).

The `xvn` CLI hides the daemon entirely.

---

## Launch

The daemon is launched by the engine as part of `xvn eval run` or
`xvn experiment run`. If you're running it standalone:

```bash
node xvision-agentd/dist/index.js --socket /tmp/agentd.sock \
  [--callback-socket /tmp/agentd-cb.sock] \
  [--event-socket /tmp/agentd-events.sock]
```

Flags:

- `--socket <path>` — required. The UDS path the daemon listens on
  for JSON-RPC requests.
- `--callback-socket <path>` — optional. The UDS path where the
  daemon sends tool-invoke callbacks to the parent client.
- `--event-socket <path>` — optional. The UDS path where the daemon
  emits run-lifecycle events (see Events below).

Version probe (does not start a server):

```bash
node xvision-agentd/dist/index.js --version
# {"protocol_version":"<...>","sidecar_version":"<...>","cline_sdk_version":"0.0.41"}
```

The daemon also installs a parent-PID liveness monitor — it shuts
down within ~1s of the parent process dying.

---

## Transport

- **Socket:** Unix Domain Socket (path supplied via `--socket`).
- **Framing:** NDJSON. Every message is a complete JSON object on a
  single line, terminated by `\n`. The decoder rejects multi-line
  JSON.
- **Protocol:** JSON-RPC 2.0. Every request must carry
  `jsonrpc: "2.0"`, `method: <string>`, `params: <object | array>`,
  and `id: <number | string | null>`. Responses are
  `{ jsonrpc: "2.0", id, result?, error? }`.

Parse errors emit a JSON-RPC error with code `RPC_ERROR_CODES.ParseError`
(-32700). Unknown methods get `MethodNotFound` (-32601). Bad params
emit `InvalidParams` (-32602) with the validation message.

---

## Methods

The method registry is in `xvision-agentd/src/methods/`. Five methods
are exposed today:

### `runtime.health`

Liveness + version probe. Cheap (no I/O).

Params: `{}` (empty object).

Result:

```json
{
  "protocol_version": "<string>",
  "sidecar_version":  "<string>",
  "cline_sdk_version":"<string>",
  "status":           "ok"
}
```

### `tool.registry.set` / `tool.registry.get`

Manages the daemon's allowed-tools registry. The Rust client populates
this at session start — only registered tools may appear in a
`session.start_run`'s `allowed_tools` list.

- `tool.registry.set` — params: `{ tools: [ { name, description, schema } ] }`.
  Replaces the registry wholesale.
- `tool.registry.get` — params: `{}`. Result: `{ tools: [...] }`.

### `tool.invoke`

Invokes a registered tool. Used by the model wrapper during a step.
The daemon forwards the call back to the Rust client over the
callback socket (or invokes a built-in TypeScript implementation if
one is registered). Result is `{ output_hash: <sha256-hex>, output: <unknown> }`.

### `session.start_run`

Begins a model-driven session. Params (validated; bad values → 
`InvalidParams`):

```json
{
  "run_id":         "<non-empty string>",
  "provider_id":    "<non-empty string>",
  "model_id":       "<non-empty string>",
  "system_prompt":  "<string>",
  "allowed_tools":  ["<tool-name>", ...],
  "api_key":        "<string>",     // optional
  "base_url":       "<string>",     // optional, OpenAI-compat providers
  "budget_limits": {
    "max_input_tokens":  <positive int>,
    "max_output_tokens": <positive int>,
    "max_wall_ms":       <positive int>
  }
}
```

Result:

```json
{ "run_id": "<echoed>", "started_at_ms": <epoch ms> }
```

Side effect: emits `event.run_started` on the event socket.

### `session.step`

Drive the session one model-turn forward. Params:

```json
{ "run_id": "<string>", "prompt": "<string>" }
```

Blocks until the model loop terminates (text-only response or
`max_iterations` reached). Result:

```json
{
  "status": "completed" | "aborted" | "failed",
  "output_text": "<string>",
  "iterations": <int>,
  "usage": {
    "input_tokens":  <int>,
    "output_tokens": <int>,
    "cache_read_tokens": <int>,
    "cache_write_tokens": <int>,
    "total_cost": <float>?
  },
  "error": "<string>"?
}
```

### `session.end_run`

Terminate the session. Params: `{ run_id }`. Result: `{ ended: bool }`.
Emits `event.run_finished` on the event socket.

---

## Events

When `--event-socket <path>` is supplied, the daemon emits JSON-RPC
**notifications** (no `id`, no response expected) on a separate socket
for each lifecycle event. Notification methods, kept in sync with the
Rust `RunEventSink::dispatch`:

| Method | Fired when | Key fields |
|---|---|---|
| `event.run_started`        | `session.start_run` returns ok | `run_id`, `objective`, `started_at_ms`, `provider_id`, `model_id` |
| `event.run_finished`       | `session.end_run` returns ended; also on step terminal status | `run_id`, `status` (`completed`/`failed`/`cancelled`), `finished_at_ms`, `error?` |
| `event.tool_call_started`  | Tool invoke begins | `span_id`, `run_id`, `tool_name`, `input_hash` |
| `event.tool_call_finished` | Tool invoke ok | `span_id`, `output_hash`, duration |
| `event.tool_call_failed`   | Tool invoke threw | `span_id`, error |
| `event.tool_call_cancelled`| Caller cancelled | `span_id` |
| `event.model_call_started` | Model dispatch begins | `span_id`, `run_id`, `provider_id`, `model_id`, `input_hash` |
| `event.model_call_finished`| Model dispatch ok | `span_id`, `output_hash`, token usage |
| `event.assistant_text_delta` | Streaming text chunk | `run_id`, `delta` |
| `event.overloaded`         | Provider 429 / overload signal | `run_id`, `provider_id` |
| `event.error`              | Unrecoverable session error | `run_id`, error |

Notifications follow JSON-RPC notification shape: `{ "jsonrpc": "2.0", "method": "event.run_started", "params": { ... } }`.

When the event buffer hits the high-water mark
(`XVISION_EVENT_BUFFER_HIGH_WATER`, default in
`transport/event-client.ts`), the daemon emits a single drop summary
and resumes once the buffer drains. Subscribers should treat events
as best-effort and reconcile against the engine's persisted
`agent_runs` / `spans` tables on reconnection.

---

## Tool-shim registry

Tools are registered via `tool.registry.set` before the first
`session.start_run` of a session. Each entry:

```json
{
  "name":        "<string>",     // referenced in `allowed_tools`
  "description": "<string>",     // shown to the model in tool descriptions
  "schema":      <JSON Schema>   // input shape; the model conforms to this
}
```

When the model emits a tool-call, the daemon:

1. Verifies `name` is in the run's `allowed_tools`.
2. Routes the call back through the callback socket (Rust runs the
   actual tool) or invokes a built-in TypeScript implementation.
3. Records the output, computes `output_hash = sha256(canonical_json)`,
   and feeds the result back to the model loop as the next message.
4. Emits `event.tool_call_started` and `event.tool_call_finished` (or
   `_failed` / `_cancelled`) on the event socket.

Adding a new built-in tool: drop a handler under
`xvision-agentd/src/session/tool-shim.ts` and register it from
`startUdsServer`. The handler signature is
`(input: unknown) => Promise<unknown>`. Built-ins still emit the
same span events so subscribers can't tell them apart from
callback-routed tools.

---

## Lifecycle reference (start to finish)

A minimal happy-path session from a client's perspective:

```
client → daemon       runtime.health                        // optional probe
client → daemon       tool.registry.set { tools: [...] }
client → daemon       session.start_run { run_id, ... }
daemon → event sock   event.run_started
client → daemon       session.step { run_id, prompt }
  daemon → event sock event.model_call_started
  daemon → event sock event.assistant_text_delta (stream)
  daemon → callback   tool.invoke (each tool call)
  daemon → event sock event.tool_call_started
  daemon → event sock event.tool_call_finished
  daemon → event sock event.model_call_finished
  (loop until terminal)
client ← daemon       step result { status, output_text, usage, ... }
client → daemon       session.end_run { run_id }
daemon → event sock   event.run_finished
```

The engine's Rust side is the canonical caller — see
`crates/xvision-engine/src/agent/` for how `LlmDispatch` translates
into these wire calls. A standalone client only needs the methods
above plus an NDJSON socket reader to drive the same loop.

---

## Test mode

For deterministic tests, set `XVISION_TEST_MOCK_PROVIDER=1` before
launching the daemon. A mock provider replaces the real model loop
with a hard-coded script (one tool call, one text response) so
end-to-end harness tests don't burn provider tokens. The mock is
declared in `xvision-agentd/src/testing/mock-provider.ts`. Production
builds never trigger this path.

---

## Where to learn more

- Daemon source: `xvision-agentd/src/`
- Engine-side translation layer: `crates/xvision-engine/src/agent/`
- Driving xvn as an agent: see [Driving xvn as an agent](/docs?slug=driving-xvn-as-an-agent)
- MCP surface (different from agentd — exposes verbs to chat-rail agents): see [MCP surface](/docs?slug=mcp)
