import * as net from "node:net"
import { encodeNdjson, NdjsonDecoder } from "./ndjson.js"

let callbackSocketPath: string | undefined
let nextId = 1

export function setCallbackSocketPath(p: string | undefined): void {
  callbackSocketPath = p
}

export async function callRust(name: string, input: unknown): Promise<unknown> {
  if (!callbackSocketPath) throw new Error("callback socket not configured")
  return new Promise((resolve, reject) => {
    const sock = net.createConnection(callbackSocketPath!)
    const decoder = new NdjsonDecoder()
    decoder.on("message", (resp: unknown) => {
      sock.end()
      const r = resp as { result?: unknown; error?: { code: number; message: string } }
      if (r.error) reject(new Error(`${r.error.code}: ${r.error.message}`))
      else resolve(r.result)
    })
    decoder.on("error", reject)
    sock.on("data", (c) => decoder.push(c))
    sock.on("error", reject)
    sock.on("connect", () => {
      sock.write(
        encodeNdjson({ jsonrpc: "2.0", id: nextId++, method: "tool.invoke", params: { name, input } })
      )
    })
  })
}
