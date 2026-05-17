import * as net from "node:net"
import { encodeNdjson } from "./ndjson.js"
import { activeRunId } from "../session/active-run.js"

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
/** True when the most recent write crossed the high-water mark and we
 * have not yet seen the buffer drain back below the low-water mark.
 * Used to debounce overload notifications so we emit one "high" event
 * per excursion (and one "cleared" event when it drains). */
let bufferHighWaterHit = false
/** Re-entrancy guard: emitting `event.overloaded` itself calls
 * `emitNotification`, which would loop on itself if the new write also
 * crosses the threshold. */
let emittingOverloaded = false

const DEFAULT_HIGH_WATER_BYTES = 64 * 1024

function getHighWaterBytes(): number {
  const raw = process.env["XVISION_EVENT_BUFFER_HIGH_WATER"]
  if (!raw) return DEFAULT_HIGH_WATER_BYTES
  const n = Number.parseInt(raw, 10)
  if (!Number.isFinite(n) || n <= 0) return DEFAULT_HIGH_WATER_BYTES
  return n
}

export function setEventSocketPath(p: string | undefined): void {
  eventSocketPath = p
  // Drop any cached connection from a previous configuration.
  if (conn) {
    conn.destroy()
    conn = undefined
  }
  connecting = undefined
  bufferHighWaterHit = false
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
 * Fire-and-forget notification. Never throws into the caller.
 *
 * Backpressure: writes go through the OS send buffer + Node's internal
 * queue. After each write we sample `socket.writableLength`. When it
 * crosses the configured high-water mark (default 64 KiB, tunable via
 * `XVISION_EVENT_BUFFER_HIGH_WATER`), we emit a single
 * `event.overloaded` notification with `note: "outbound buffer high"`.
 * When the buffer drains below 50% of the threshold on a subsequent
 * write, we emit a follow-up `event.overloaded` with
 * `note: "outbound buffer cleared"` and reset the flag.
 *
 * `dropped` is always 0 because we never actually drop a notification —
 * Node will queue indefinitely. The field is part of the wire shape so
 * Rust can render the warn line consistently with future "we did have
 * to drop" cases (e.g. when the bus rejects under back-pressure).
 */
export async function emitNotification(method: string, params: unknown): Promise<void> {
  const s = await getConn()
  if (!s) return
  const msg = encodeNdjson({ jsonrpc: "2.0", method, params })
  s.write(msg)
  checkBackpressure(s, method)
}

function checkBackpressure(s: net.Socket, method: string): void {
  // Avoid re-entering: the overload notification itself goes through
  // emitNotification, and we don't want the threshold check on that
  // write to trigger another overload event.
  if (emittingOverloaded) return
  if (method === "event.overloaded") return
  const highWater = getHighWaterBytes()
  const depth = s.writableLength
  if (!bufferHighWaterHit && depth > highWater) {
    bufferHighWaterHit = true
    emittingOverloaded = true
    try {
      const msg = encodeNdjson({
        jsonrpc: "2.0",
        method: "event.overloaded",
        params: {
          run_id: activeRunId() ?? "",
          dropped: 0,
          note: "outbound buffer high",
        },
      })
      s.write(msg)
    } finally {
      emittingOverloaded = false
    }
  } else if (bufferHighWaterHit && depth < highWater / 2) {
    bufferHighWaterHit = false
    emittingOverloaded = true
    try {
      const msg = encodeNdjson({
        jsonrpc: "2.0",
        method: "event.overloaded",
        params: {
          run_id: activeRunId() ?? "",
          dropped: 0,
          note: "outbound buffer cleared",
        },
      })
      s.write(msg)
    } finally {
      emittingOverloaded = false
    }
  }
}

/** Test-only — drop the cached connection so a re-setup starts clean. */
export function resetForTesting(): void {
  if (conn) {
    conn.destroy()
    conn = undefined
  }
  connecting = undefined
  eventSocketPath = undefined
  bufferHighWaterHit = false
  emittingOverloaded = false
}
