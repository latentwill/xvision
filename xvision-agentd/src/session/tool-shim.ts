import { createTool, type AgentTool } from "@cline/sdk"
import { callRust } from "../transport/callback-client.js"
import type { ToolDescriptor } from "../methods/tool-registry.js"
import {
  emitToolCallStarted,
  emitToolCallFinished,
  emitToolCallFailed,
  newSpanId,
  hashJson,
} from "./emit.js"
import { activeRunId } from "./active-run.js"

export type { ToolDescriptor }

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
    execute: async (input: unknown) => {
      const runId = activeRunId()
      const spanId = newSpanId()
      if (runId) {
        emitToolCallStarted({
          span_id: spanId,
          run_id: runId,
          tool_name: d.name,
          input_hash: hashJson(input),
        })
      }
      try {
        const out = await callRust(d.name, input as Record<string, unknown>)
        if (runId) {
          emitToolCallFinished({
            span_id: spanId,
            run_id: runId,
            output_hash: hashJson(out),
          })
        }
        return out
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        if (runId) {
          emitToolCallFailed({
            span_id: spanId,
            run_id: runId,
            error: message,
          })
        }
        // Per Cline SDK rule: return errors as data, do not throw —
        // throwing counts as a "mistake" against the agent's mistake limit.
        return { error: message }
      }
    },
  })
}
