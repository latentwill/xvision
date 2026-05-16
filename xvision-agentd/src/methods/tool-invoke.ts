import { registerMethod } from "./index.js"
import { callRust } from "../transport/callback-client.js"

interface InvokeParams { name?: unknown; input?: unknown }

export async function handleToolInvoke(params: unknown): Promise<unknown> {
  const p = (params ?? {}) as InvokeParams
  if (typeof p.name !== "string") throw new TypeError("params.name must be string")
  if (typeof p.input !== "object" || p.input === null) throw new TypeError("params.input must be object")
  return await callRust(p.name, p.input)
}

registerMethod("tool.invoke", (p) => handleToolInvoke(p))
