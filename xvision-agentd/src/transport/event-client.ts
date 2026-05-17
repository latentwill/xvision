import * as net from "node:net"
import { encodeNdjson } from "./ndjson.js"

/**
 * One-way notification channel from sidecar → Rust client.
 *
 * Separate from the callback socket (which is request/response for
 * `tool.invoke`). Events here are id-less JSON-RPC 2.0 notifications;
 * the Rust side translates each to a `RunEvent` and publishes to the
 * Phase-A `RunEventBus`.
 *
 * Connection model: persistent client socket, opened lazily on the first
 * emit, kept alive for the lifetime of the sidecar process. On write
 * error, the socket is dropped and the next emit reconnects (best-effort
 * — observability is non-blocking, so we never throw into the agent
 * loop). If the event socket was never configured (Rust client did not
 * pass --event-socket), emits are silent no-ops.
 */

let eventSocketPath: string | undefined
let conn: net.Socket | undefined
let connecting: Promise<net.Socket> | undefined

export function setEventSocketPath(p: string | undefined): void {
  eventSocketPath = p
  // Drop any cached connection from a previous configuration.
  if (conn) {
    conn.destroy()
    conn = undefined
  }
  connecting = undefined
}

export function isEventSocketConfigured(): boolean {
  return eventSocketPath !== undefined
}

async function getConn(): Promise<net.Socket | undefined> {
  if (!eventSocketPath) return undefined
  if (conn && !conn.destroyed) return conn
  if (connecting) return connecting

  connecting = new Promise<net.Socket>((resolve, reject) => {
    const s = net.createConnection(eventSocketPath!)
    s.once("connect", () => {
      s.removeListener("error", reject)
      // Don't keep the event loop alive on this socket alone.
      s.unref()
      resolve(s)
    })
    s.once("error", reject)
  })

  try {
    conn = await connecting
    // Drop on close so the next emit reconnects.
    conn.on("close", () => {
      conn = undefined
    })
    conn.on("error", () => {
      // Errors after connect — surface as close; emit is best-effort.
      if (conn) conn.destroy()
      conn = undefined
    })
    connecting = undefined
    return conn
  } catch {
    connecting = undefined
    return undefined
  }
}

/**
 * Fire-and-forget notification. Never throws into the caller. Backpressure
 * note: writes go through the TCP/UDS send buffer. If the consumer is slow
 * and the buffer fills, Node will buffer in memory until `socket.write()`
 * returns false; we currently do not surface that backpressure. A future
 * follow-up may track outbound queue depth and emit `event.overloaded` to
 * the Rust side when a threshold is hit (per the plan's BackpressureDropped
 * surface).
 */
export async function emitNotification(method: string, params: unknown): Promise<void> {
  const s = await getConn()
  if (!s) return
  const msg = encodeNdjson({ jsonrpc: "2.0", method, params })
  s.write(msg)
}

/** Test-only — drop the cached connection so a re-setup starts clean. */
export function resetForTesting(): void {
  if (conn) {
    conn.destroy()
    conn = undefined
  }
  connecting = undefined
  eventSocketPath = undefined
}
