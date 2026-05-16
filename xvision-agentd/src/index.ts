// CLI entrypoint. Wave 1: prints version and exits.
// Wave 1 Task 4 wires up the JSON-RPC server.
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "./version.js"

const args = process.argv.slice(2)
if (args[0] === "--version") {
  console.log(JSON.stringify({ protocol_version: PROTOCOL_VERSION, sidecar_version: SIDECAR_VERSION }))
  process.exit(0)
}

console.error("xvision-agentd: no socket path provided")
process.exit(2)
