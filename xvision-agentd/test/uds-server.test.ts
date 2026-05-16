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

beforeEach(async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "xvision-agentd-"))
  socketPath = path.join(dir, "sock")
  server = await startUdsServer(socketPath)
})

afterEach(async () => {
  await server.close()
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
})
