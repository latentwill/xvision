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
//
// The mock enforces run_id idempotency: a second `session.start_run` with a
// run_id already seen returns a JSON-RPC error (the Stage 1 item-2 dedup
// contract the real store.ts implements).

const net = require("net");
const readline = require("readline");
const fs = require("fs");

function argVal(flag) {
  const i = process.argv.indexOf(flag);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

const socketPath = argVal("--socket");
if (!socketPath) {
  process.stderr.write("mock_agentd: missing --socket\n");
  process.exit(2);
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

const seenRunIds = new Set();

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
        resp = ok(id, { run_id: runId, started_at_ms: 1 });
        break;
      }
      case "session.step": {
        if (crashOnStep) {
          // Sidecar crash mid-step: drop the connection and exit hard so the
          // Rust transport surfaces a typed crash/transport error.
          conn.destroy();
          process.exit(7);
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
