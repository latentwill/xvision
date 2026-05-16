import { startUdsServer } from "./transport/uds-server.js"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "./version.js"
import { setCallbackSocketPath } from "./transport/callback-client.js"

async function main(): Promise<void> {
  const args = process.argv.slice(2)

  if (args[0] === "--version") {
    console.log(JSON.stringify({ protocol_version: PROTOCOL_VERSION, sidecar_version: SIDECAR_VERSION }))
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
