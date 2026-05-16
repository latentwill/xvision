import { describe, expect, it, beforeEach, afterEach } from "vitest"
import * as net from "node:net"
import * as os from "node:os"
import * as path from "node:path"
import * as fs from "node:fs/promises"
import { startUdsServer } from "../src/transport/uds-server.js"
import { encodeNdjson, NdjsonDecoder } from "../src/transport/ndjson.js"
import type { JsonRpcResponse } from "../src/transport/jsonrpc.js"

let socketPath: string
let server: { close: () => Promise<void> }
let dir: string

beforeEach(async () => {
  dir = await fs.mkdtemp(path.join(os.tmpdir(), "xvision-agentd-"))
  socketPath = path.join(dir, "sock")
  server = await startUdsServer(socketPath)
})

afterEach(async () => {
  await server.close()
  await fs.rm(dir, { recursive: true, force: true })
})

async function rpc<T>(method: string, params?: unknown): Promise<JsonRpcResponse<T>> {
  return new Promise((resolve, reject) => {
    const sock = net.createConnection(socketPath)
    const decoder = new NdjsonDecoder()
    decoder.on("message", (msg) => {
      sock.end()
      resolve(msg as JsonRpcResponse<T>)
    })
    decoder.on("error", reject)
    sock.on("data", (chunk) => decoder.push(chunk))
    sock.on("error", reject)
    sock.on("connect", () => {
      sock.write(encodeNdjson({ jsonrpc: "2.0", id: 1, method, params }))
    })
  })
}

describe("uds-server", () => {
  it("returns runtime.health result", async () => {
    const resp = await rpc<{ status: string }>("runtime.health")
    expect("result" in resp).toBe(true)
    if ("result" in resp) {
      expect(resp.result.status).toBe("ok")
    }
  })

  it("returns MethodNotFound for unknown methods", async () => {
    const resp = await rpc("does.not.exist")
    expect("error" in resp).toBe(true)
    if ("error" in resp) {
      expect(resp.error.code).toBe(-32601)
    }
  })

  it("returns ParseError on malformed input", async () => {
    const sock = net.createConnection(socketPath)
    const decoder = new NdjsonDecoder()
    const result: unknown = await new Promise((resolve, reject) => {
      decoder.on("message", (m) => {
        sock.end()
        resolve(m)
      })
      decoder.on("error", reject)
      sock.on("data", (c) => decoder.push(c))
      sock.on("connect", () => sock.write("not json\n"))
      sock.on("error", reject)
    })
    expect(result).toMatchObject({ error: { code: -32700 } })
  })

  it("does not respond to notifications", async () => {
    // A JSON-RPC 2.0 notification has no id field. The server MUST NOT reply.
    const received = await new Promise<boolean>((resolve) => {
      const sock = net.createConnection(socketPath)
      const decoder = new NdjsonDecoder()
      const timeout = setTimeout(() => {
        sock.end()
        resolve(false) // timeout fired first — no response received (correct)
      }, 100)
      decoder.on("message", () => {
        clearTimeout(timeout)
        sock.end()
        resolve(true) // a response arrived — incorrect per spec
      })
      sock.on("data", (chunk) => decoder.push(chunk))
      sock.on("error", () => {
        clearTimeout(timeout)
        resolve(false)
      })
      sock.on("connect", () => {
        // Send a notification: jsonrpc + method, no id
        sock.write(encodeNdjson({ jsonrpc: "2.0", method: "runtime.health" }))
      })
    })
    expect(received).toBe(false)
  })
})
