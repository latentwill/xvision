// Mock xvision-agentd sidecar for Stage 1 engine integration tests.
//
// Pure Node stdlib (net + readline) — NO @cline/sdk, no npm deps — so the
// Rust `AgentClient::spawn` supervisor (which runs `node <bin> --socket
// <path>` and waits for a `{"event":"ready"}` line on stderr) can drive a
// faithful start_run -> step -> end_run lifecycle without building the real
// TypeScript sidecar.
//
// Behaviour is configured via a per-socket JSON config file at
// `<socketPath>.cfg` (so parallel Rust tests with distinct temp sockets
// never race on a shared process env). The config covers the happy path,
// the missing-decision path, the crash-mid-step path, and the
// non-JSON-decision path:
//
//   decisionJson   JSON string returned as StepResult.decision_json on
//                  `session.step`. Defaults to a valid hold. The literal
//                  "OMIT" means: complete the step but do NOT include
//                  decision_json (agent never submitted). The literal
//                  "NOTJSON" means: return a decision_json that is not
//                  valid JSON.
//   stepStatus     StepResult.status (default "completed"). Set to e.g.
//                  "aborted" to exercise the not-completed path.
//   crashOnStep    when true, the process exits hard on the first
//                  `session.step` (sidecar crash mid-step).
//   recordStepsPath optional JSONL path. When set, every `session.step`
//                  prompt is appended as `{ run_id, prompt }` so Rust
//                  integration tests can assert on Cline briefing content
//                  without a real provider.
//
// Stage 3 (replay) additions:
//   requireReplay  when true, a `session.step` that has NOT been preceded by
//                  a `session.replay_load` for that run_id panics the process
//                  (exit 9). This is the no-live-provider guard the replay
//                  bit-stability test relies on: if the executor ever drove a
//                  live step during replay, the mock would crash loudly.
//   replayInjectError  when set to "replay_frames_exhausted" or
//                  "replay_divergence", a replayed `session.step` returns that
//                  reason on `error` (and omits decision_json) so the Rust
//                  side can exercise the exhaustion / divergence aborts.
//
// On `session.replay_load` the mock stores the supplied frames keyed by
// run_id. On a subsequent `session.step` for a run that has loaded frames,
// the mock REPLAYS: it extracts the recorded `submit_decision` tool-call
// input from the frames and returns it verbatim as decision_json, with
// usage summed from any `Usage` frames — deterministically, with no
// "live provider" code path touched.
//
// The mock enforces run_id idempotency: a second `session.start_run` with a
// run_id already seen returns a JSON-RPC error (the Stage 1 item-2 dedup
// contract the real store.ts implements).

const net = require("net");
const readline = require("readline");
const fs = require("fs");
const path = require("path");

function argVal(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

const socketPath = argVal("--socket");
if (!socketPath) {
  process.stderr.write("mock_agentd: missing --socket\n");
  process.exit(2);
}

// §2-B: the Rust client passes --event-socket when it spawns us with a
// recording sink (spawn_with_event_sink). In record mode we connect to it
// as a client and emit `event.trajectory_frame` notifications — the EXACT
// envelopes the real sidecar's emit.ts produces — so the Rust event sink
// persists them into the TrajectoryStore. This keeps the eval-side
// recording test hermetic (no real LLM / network) while still exercising
// the full record→persist path through the live spawn_with_event_sink
// wiring. The frames come from the shared golden fixture so the wire shape
// cannot drift from the Rust parser.
const eventSocketPath = argVal("--event-socket");
let eventConn = null;
function getEventConn(cb) {
  if (!eventSocketPath) return cb(null);
  if (eventConn && !eventConn.destroyed) return cb(eventConn);
  const s = net.createConnection(eventSocketPath, () => {
    eventConn = s;
    cb(s);
  });
  s.on("error", () => cb(null));
}
function emitNotification(method, params) {
  getEventConn((s) => {
    if (!s) return;
    s.write(JSON.stringify({ jsonrpc: "2.0", method, params }) + "\n");
  });
}

// Golden trajectory envelopes — the representative recording the mock
// replays on the event socket in record mode. Loaded from the shared
// fixture so emit shape stays in lockstep with the Rust parser + vitest.
let goldenEnvelopes = [];
try {
  const fixture = JSON.parse(
    fs.readFileSync(path.join(__dirname, "trajectory_golden_envelopes.json"), "utf8"),
  );
  goldenEnvelopes = fixture.envelopes ?? [];
} catch (e) {
  // Fixture missing → record mode emits nothing (test will catch the gap).
}

let cfg = {};
try {
  cfg = JSON.parse(fs.readFileSync(socketPath + ".cfg", "utf8"));
} catch (e) {
  // No config file → all defaults (happy path).
}

const decisionJson = cfg.decisionJson ?? '{"action":"hold","conviction":0.5,"justification":"mock cline decision"}';
const stepStatus = cfg.stepStatus ?? "completed";
const crashOnStep = cfg.crashOnStep === true;
const requireReplay = cfg.requireReplay === true;
const replayInjectError = cfg.replayInjectError ?? null;
const recordStepsPath = cfg.recordStepsPath ?? null;

const seenRunIds = new Set();
// run_id -> { frames: [...] } loaded via session.replay_load.
const loadedReplays = new Map();
// run_id -> { record: bool, slot_role: string } captured at start_run, used
// to emit trajectory frames on step when recording (§2-B).
const recordingRuns = new Map();

// Extract the recorded submit_decision payload from a frame list — the
// last ToolCallDelta whose tool_name is "submit_decision".
function recordedDecision(frames) {
  for (let i = frames.length - 1; i >= 0; i--) {
    const f = frames[i];
    if (f && f.kind === "ToolCallDelta" && f.tool_name === "submit_decision" && f.input) {
      return f.input;
    }
  }
  return null;
}

// Sum token usage from Usage frames.
function replayUsage(frames) {
  const usage = { input_tokens: 0, output_tokens: 0, cache_read_tokens: 0, cache_write_tokens: 0 };
  for (const f of frames) {
    if (f && f.kind === "Usage") {
      usage.input_tokens += f.input_tokens ?? 0;
      usage.output_tokens += f.output_tokens ?? 0;
      usage.cache_read_tokens += f.cache_read_tokens ?? 0;
      usage.cache_write_tokens += f.cache_write_tokens ?? 0;
    }
  }
  return usage;
}

function recordStep(runId, prompt) {
  if (!recordStepsPath) return;
  fs.appendFileSync(
    recordStepsPath,
    JSON.stringify({ run_id: runId, prompt: typeof prompt === "string" ? prompt : "" }) + "\n",
  );
}

function ok(id, result) {
  return JSON.stringify({ jsonrpc: "2.0", id, result });
}
function err(id, code, message) {
  return JSON.stringify({ jsonrpc: "2.0", id, error: { code, message } });
}

const server = net.createServer((conn) => {
  const rl = readline.createInterface({ input: conn });
  rl.on("line", (line) => {
    if (!line.trim()) return;
    let req;
    try {
      req = JSON.parse(line);
    } catch (e) {
      return;
    }
    const id = req.id;
    const method = req.method;
    let resp;
    switch (method) {
      case "runtime.health":
        resp = ok(id, {
          protocol_version: "0.1.0",
          sidecar_version: "mock-0.0.1",
          cline_sdk_version: "mock-cline",
          status: "ok",
        });
        break;
      case "tool.registry.set":
        resp = ok(id, { count: (req.params?.tools ?? []).length, registry_hash: "mockhash" });
        break;
      case "session.start_run": {
        const runId = req.params?.run_id;
        if (seenRunIds.has(runId)) {
          // Idempotency / dedup: a retried start_run with the same run_id
          // is rejected, never double-executed (item 2).
          resp = err(id, -32010, `run_id already active: ${runId}`);
          break;
        }
        seenRunIds.add(runId);
        // §2-B: capture record + slot_role so `step` can emit frames.
        if (req.params?.record === true) {
          recordingRuns.set(runId, {
            record: true,
            slot_role: typeof req.params?.slot_role === "string" ? req.params.slot_role : "default",
          });
        }
        resp = ok(id, { run_id: runId, started_at_ms: 1 });
        break;
      }
      case "session.replay_load": {
        const runId = req.params?.run_id;
        const frames = req.params?.frames ?? [];
        loadedReplays.set(runId, { frames });
        resp = ok(id, { loaded: frames.length });
        break;
      }
      case "session.step": {
        if (crashOnStep) {
          // Sidecar crash mid-step: drop the connection and exit hard so the
          // Rust transport surfaces a typed crash/transport error.
          conn.destroy();
          process.exit(7);
        }
        const runId = req.params?.run_id;
        recordStep(runId, req.params?.prompt);
        const replay = loadedReplays.get(runId);

        if (replay) {
          // REPLAY PATH — deterministic, no live provider touched.
          if (replayInjectError) {
            // Simulate frame exhaustion / divergence detected by the
            // replay model: aborted step with a replay-specific reason,
            // no decision_json.
            resp = ok(id, {
              status: "aborted",
              output_text: "",
              iterations: 1,
              usage: replayUsage(replay.frames),
              error: replayInjectError,
            });
            break;
          }
          const decision = recordedDecision(replay.frames);
          const result = {
            status: "completed",
            output_text: "",
            iterations: 1,
            usage: replayUsage(replay.frames),
          };
          if (decision !== null) {
            result.decision_json = JSON.stringify(decision);
          }
          resp = ok(id, result);
          break;
        }

        if (requireReplay) {
          // No frames were loaded for this run yet a step was issued: the
          // executor took a LIVE path during a replay test. Crash loudly so
          // the bit-stability test fails hard instead of silently passing.
          process.stderr.write(
            "mock_agentd: live step during replay (no frames loaded for run " + runId + ")\n"
          );
          process.exit(9);
        }

        // §2-B record mode: emit the golden trajectory frames on the event
        // socket, stamped with THIS run's run_id + slot_role, so the Rust
        // event sink persists them into the recording keyed by the matching
        // slot_role (footgun c). Frames are emitted before the step result
        // so the persist append completes deterministically.
        const recState = recordingRuns.get(runId);
        if (recState && recState.record) {
          for (const env of goldenEnvelopes) {
            emitNotification("event.trajectory_frame", {
              run_id: runId,
              slot_role: recState.slot_role,
              step_index: env.step_index,
              frame_index: env.frame_index,
              frame: env.frame,
            });
          }
        }

        const usage = {
          input_tokens: 11,
          output_tokens: 7,
          cache_read_tokens: 0,
          cache_write_tokens: 0,
        };
        const result = {
          status: stepStatus,
          output_text: "",
          iterations: 1,
          usage,
        };
        if (decisionJson === "NOTJSON") {
          result.decision_json = "this is not json {";
        } else if (decisionJson !== "OMIT") {
          result.decision_json = decisionJson;
        }
        resp = ok(id, result);
        break;
      }
      case "session.end_run":
        resp = ok(id, { ended: true });
        break;
      default:
        resp = err(id, -32601, `unknown method: ${method}`);
    }
    conn.write(resp + "\n");
  });
});

server.listen(socketPath, () => {
  // The supervisor blocks on this exact structured line before connecting.
  process.stderr.write(JSON.stringify({ event: "ready" }) + "\n");
});

process.on("SIGTERM", () => process.exit(0));
process.on("SIGINT", () => process.exit(0));
