import { startUdsServer } from "./transport/uds-server.js"
import { CLINE_SDK_VERSION, PROTOCOL_VERSION, SIDECAR_VERSION } from "./version.js"
import { setCallbackSocketPath } from "./transport/callback-client.js"
import { setEventSocketPath } from "./transport/event-client.js"
import { installMockProvider, setMockScript } from "./testing/mock-provider.js"

async function main(): Promise<void> {
  const args = process.argv.slice(2)

  if (args[0] === "--version") {
    console.log(JSON.stringify({ protocol_version: PROTOCOL_VERSION, sidecar_version: SIDECAR_VERSION, cline_sdk_version: CLINE_SDK_VERSION }))
    process.exit(0)
  }

  const socketIdx = args.indexOf("--socket")
  if (socketIdx === -1 || !args[socketIdx + 1]) {
    console.error("xvision-agentd: missing --socket <path>")
    process.exit(2)
  }
  const socketPath = args[socketIdx + 1]!

  const cbIdx = args.indexOf("--callback-socket")
  if (cbIdx !== -1 && args[cbIdx + 1]) {
    setCallbackSocketPath(args[cbIdx + 1])
  }

  const evIdx = args.indexOf("--event-socket")
  if (evIdx !== -1 && args[evIdx + 1]) {
    setEventSocketPath(args[evIdx + 1])
  }

  // Test-only: install a deterministic mock-model script before sessions
  // can start. Gated by env var so production builds never trigger this
  // path.
  //
  // `XVISION_TEST_MOCK_SCRIPT` (when set to a JSON-encoded `MockTurn[]`)
  // overrides the default script. This lets a built-sidecar integration test
  // drive a recordable run hermetically — e.g. a `submit_decision` tool turn
  // so `execute_slot_cline` receives a structured decision through the real
  // Agent loop + emit.ts/frame-recorder.ts path. Falls back to the canonical
  // echo+done script when the var is absent / unparseable.
  if (process.env.XVISION_TEST_MOCK_PROVIDER === "1") {
    installMockProvider()
    let scripted = false
    const raw = process.env.XVISION_TEST_MOCK_SCRIPT
    if (raw) {
      try {
        const turns = JSON.parse(raw)
        if (Array.isArray(turns)) {
          setMockScript(turns)
          scripted = true
        }
      } catch {
        // Unparseable override → fall through to the default script.
      }
    }
    if (!scripted) {
      setMockScript([
        { toolCall: { name: "echo", input: { msg: "from-sidecar" } } },
        { text: "done" },
      ])
    }
  }

  const server = await startUdsServer(socketPath)
  const shutdown = async (): Promise<void> => {
    await server.close()
    process.exit(0)
  }
  process.on("SIGTERM", shutdown)
  process.on("SIGINT", shutdown)

  // Parent-PID liveness monitor. Exit if our parent goes away.
  // .unref() lets the interval not keep the event loop alive on its own —
  // graceful shutdown via SIGTERM still works.
  const parentPid = process.ppid
  setInterval(() => {
    try {
      process.kill(parentPid, 0)
    } catch {
      void shutdown()
    }
  }, 1000).unref()

  // Structured "ready" log on stderr so the Rust supervisor can sync.
  process.stderr.write(JSON.stringify({ event: "ready", socket: socketPath }) + "\n")
}

void main()
