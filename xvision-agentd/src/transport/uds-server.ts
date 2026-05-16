import * as net from "node:net"
import { NdjsonDecoder, encodeNdjson } from "./ndjson.js"
import {
  JsonRpcRequest,
  JsonRpcResponse,
  RPC_ERROR_CODES,
} from "./jsonrpc.js"
import { handleRuntimeHealth } from "../methods/runtime-health.js"

export interface UdsServerHandle {
  close(): Promise<void>
}

type MethodHandler = (params: unknown) => Promise<unknown> | unknown

const methods: Record<string, MethodHandler> = {
  "runtime.health": () => handleRuntimeHealth(),
}

export async function startUdsServer(socketPath: string): Promise<UdsServerHandle> {
  const server = net.createServer((conn) => {
    const decoder = new NdjsonDecoder()
    decoder.on("message", async (raw) => {
      const resp = await dispatch(raw)
      if (resp) conn.write(encodeNdjson(resp))
    })
    decoder.on("error", (_err) => {
      conn.write(encodeNdjson({
        jsonrpc: "2.0",
        id: null,
        error: { code: RPC_ERROR_CODES.ParseError, message: "parse error" },
      }))
    })
    conn.on("data", (chunk) => decoder.push(chunk))
    conn.on("error", () => { /* swallow; client may close abruptly */ })
  })

  await new Promise<void>((resolve, reject) => {
    server.once("error", reject)
    server.listen(socketPath, () => {
      server.off("error", reject)
      resolve()
    })
  })

  return {
    async close() {
      await new Promise<void>((resolve) => server.close(() => resolve()))
    },
  }
}

async function dispatch(raw: unknown): Promise<JsonRpcResponse | null> {
  if (
    typeof raw !== "object" ||
    raw === null ||
    (raw as { jsonrpc?: unknown }).jsonrpc !== "2.0"
  ) {
    return {
      jsonrpc: "2.0",
      id: null,
      error: { code: RPC_ERROR_CODES.InvalidRequest, message: "invalid request" },
    }
  }
  const req = raw as JsonRpcRequest
  const handler = methods[req.method]
  if (!handler) {
    return {
      jsonrpc: "2.0",
      id: req.id,
      error: { code: RPC_ERROR_CODES.MethodNotFound, message: `unknown method: ${req.method}` },
    }
  }
  try {
    const result = await handler(req.params)
    return { jsonrpc: "2.0", id: req.id, result }
  } catch (err) {
    return {
      jsonrpc: "2.0",
      id: req.id,
      error: {
        code: RPC_ERROR_CODES.InternalError,
        message: err instanceof Error ? err.message : String(err),
      },
    }
  }
}
