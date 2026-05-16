import { createHash } from "node:crypto"
import { registerMethod } from "./index.js"

interface ToolDescriptor {
  name: string
  version: string
  description: string
  input_schema: unknown
  output_schema: unknown
  timeout_ms: number
  side_effect_level: "pure" | "read_only" | "external_read" | "external_write"
  requires_approval: boolean
}

interface ToolRegistrySetResult { count: number; registry_hash: string }
interface ToolRegistryGetResult { tools: ToolDescriptor[]; registry_hash: string }

let current: ToolDescriptor[] = []
let currentHash = sha256("")

export function resetRegistry(): void {
  current = []
  currentHash = sha256("")
}

export function handleToolRegistrySet(params: unknown): ToolRegistrySetResult {
  const tools = validate(params)
  current = tools.slice().sort((a, b) => a.name.localeCompare(b.name))
  currentHash = sha256(JSON.stringify(current))
  return { count: current.length, registry_hash: currentHash }
}

export function handleToolRegistryGet(): ToolRegistryGetResult {
  return { tools: current, registry_hash: currentHash }
}

function validate(params: unknown): ToolDescriptor[] {
  if (typeof params !== "object" || params === null) throw new TypeError("params must be an object")
  const p = params as { tools?: unknown }
  if (!Array.isArray(p.tools)) throw new TypeError("tools must be an array")
  for (const t of p.tools) {
    if (typeof t !== "object" || t === null) throw new TypeError("tool must be an object")
    const x = t as Record<string, unknown>
    for (const k of ["name", "version", "description", "side_effect_level"]) {
      if (typeof x[k] !== "string") throw new TypeError(`tool.${k} must be string`)
    }
    if (typeof x.timeout_ms !== "number") throw new TypeError("tool.timeout_ms must be number")
    if (typeof x.requires_approval !== "boolean") throw new TypeError("tool.requires_approval must be bool")
    if (typeof x.input_schema !== "object" || x.input_schema === null) throw new TypeError("tool.input_schema required")
    if (typeof x.output_schema !== "object" || x.output_schema === null) throw new TypeError("tool.output_schema required")
  }
  return p.tools as ToolDescriptor[]
}

function sha256(s: string): string {
  return createHash("sha256").update(s).digest("hex")
}

// Register the two methods. Side-effect import from uds-server.ts wires routing.
registerMethod("tool.registry.set", (p) => handleToolRegistrySet(p))
registerMethod("tool.registry.get", () => handleToolRegistryGet())
