import { describe, it, expect, beforeEach, afterEach } from "vitest"
import * as net from "node:net"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import * as path from "node:path"
import {
  setEventSocketPath,
  resetForTesting,
  emitNotification,
} from "../src/transport/event-client.js"

/**
 * Spin up a local UDS server, point `event-client.ts` at it, and assert
 * notifications round-trip with the expected JSON-RPC shape.
 */
describe("event-client", () => {
  let tmpDir: string
  let socketPath: string

  beforeEach(() => {
    tmpDir = mkdtempSync(path.join(tmpdir(), "xvision-event-test-"))
    socketPath = path.join(tmpDir, "events.sock")
  })

  afterEach(() => {
    resetForTesting()
    try { rmSync(tmpDir, { recursive: true, force: true }) } catch {}
  })

  it("is a no-op when no socket is configured", async () => {
    // No configuration; calling emit should resolve without throwing.
    await emitNotification("event.test", { foo: 1 })
  })

  it("connects to the event socket and pushes notifications", async () => {
    const received: unknown[] = []
    const acceptedPromise = new Promise<void>((resolveAccept) => {
      const server = net.createServer((conn) => {
        let buf = ""
        conn.on("data", (chunk) => {
          buf += chunk.toString("utf8")
          let idx: number
          while ((idx = buf.indexOf("\n")) !== -1) {
            const line = buf.slice(0, idx)
            buf = buf.slice(idx + 1)
            if (line.length === 0) continue
            try {
              received.push(JSON.parse(line))
            } catch (e) {
              throw new Error(`bad line: ${line}: ${e}`)
            }
          }
        })
        resolveAccept()
      })
      server.listen(socketPath)
    })

    setEventSocketPath(socketPath)
    // First emit triggers the lazy connect; await it so the listener accepts.
    await emitNotification("event.run_started", {
      run_id: "r1",
      objective: "test",
      started_at_ms: 1_700_000_000_000,
      provider_id: "anthropic",
      model_id: "claude-opus-4-7",
    })
    await acceptedPromise

    await emitNotification("event.run_finished", {
      run_id: "r1",
      status: "completed",
      finished_at_ms: 1_700_000_010_000,
    })

    // Allow socket flush.
    await new Promise((r) => setTimeout(r, 50))

    expect(received.length).toBe(2)
    const first = received[0] as Record<string, unknown>
    expect(first.jsonrpc).toBe("2.0")
    expect(first.method).toBe("event.run_started")
    // JSON-RPC 2.0 notifications must NOT carry an `id` field.
    expect("id" in first).toBe(false)

    const second = received[1] as Record<string, unknown>
    expect(second.method).toBe("event.run_finished")
    expect("id" in second).toBe(false)
  })
})
