import { createTool, type AgentTool } from "@cline/sdk"
import { callRust } from "../transport/callback-client.js"

export interface ToolDescriptor {
  name: string
  version: string
  description: string
  input_schema: unknown
  output_schema: unknown
  timeout_ms: number
  side_effect_level: "pure" | "read_only" | "external_read" | "external_write"
  requires_approval: boolean
}

export interface ShimOptions {
  allowWrites: boolean
}

export function shimRegistryToTools(
  descriptors: readonly ToolDescriptor[],
  allowedNames: readonly string[],
  opts: ShimOptions,
): AgentTool[] {
  const byName = new Map(descriptors.map(d => [d.name, d]))
  const out: AgentTool[] = []
  for (const name of allowedNames) {
    const d = byName.get(name)
    if (!d) throw new Error(`unknown tool in allow-list: ${name}`)
    if (d.side_effect_level === "external_write" && !opts.allowWrites) continue
    out.push(buildTool(d))
  }
  return out
}

function buildTool(d: ToolDescriptor): AgentTool {
  return createTool({
    name: d.name,
    description: d.description,
    // Cast: ToolDescriptor.input_schema is `unknown` on the wire; the
    // Wave-1 registry validator already enforced object shape.
    inputSchema: d.input_schema as Record<string, unknown>,
    timeoutMs: d.timeout_ms,
    execute: async (input) => {
      try {
        return await callRust(d.name, input as Record<string, unknown>)
      } catch (err) {
        // Per Cline SDK rule: return errors as data, do not throw —
        // throwing counts as a "mistake" against the agent's mistake limit.
        return { error: err instanceof Error ? err.message : String(err) }
      }
    },
  })
}
