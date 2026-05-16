import * as net from "node:net"
import { NdjsonDecoder, encodeNdjson } from "./ndjson.js"
import {
  JsonRpcResponse,
  RPC_ERROR_CODES,
} from "./jsonrpc.js"
import "../methods/runtime-health.js"
import "../methods/tool-registry.js"
import { getMethodHandler } from "../methods/index.js"

export interface UdsServerHandle {
  close(): Promise<void>
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

  const msg = raw as Record<string, unknown>
  const method = msg["method"]
  const id = msg["id"]

  // Validate method field.
  if (typeof method !== "string") {
    return {
      jsonrpc: "2.0",
      id: null,
      error: { code: RPC_ERROR_CODES.InvalidRequest, message: "invalid request" },
    }
  }

  // Validate id field when present.
  if (id !== undefined && typeof id !== "number" && typeof id !== "string") {
    return {
      jsonrpc: "2.0",
      id: null,
      error: { code: RPC_ERROR_CODES.InvalidRequest, message: "invalid request" },
    }
  }

  // JSON-RPC 2.0 notifications (id absent) MUST NOT receive a response.
  // Return null to signal "no reply"; the caller skips the write.
  if (id === undefined) {
    const handler = getMethodHandler(method)
    if (handler) {
      try {
        await handler(msg["params"])
      } catch {
        // Notifications have no channel to report errors on; swallow.
      }
    }
    return null
  }

  const reqId = id as number | string
  const handler = getMethodHandler(method)
  if (!handler) {
    return {
      jsonrpc: "2.0",
      id: reqId,
      error: { code: RPC_ERROR_CODES.MethodNotFound, message: `unknown method: ${method}` },
    }
  }
  try {
    const result = await handler(msg["params"])
    return { jsonrpc: "2.0", id: reqId, result }
  } catch (err) {
    return {
      jsonrpc: "2.0",
      id: reqId,
      error: {
        code: RPC_ERROR_CODES.InternalError,
        message: err instanceof Error ? err.message : String(err),
      },
    }
  }
}
