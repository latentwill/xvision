import { createTool, type AgentTool } from "@cline/sdk"
import { callRust } from "../transport/callback-client.js"
import type { ToolDescriptor } from "../methods/tool-registry.js"
import {
  emitToolCallStarted,
  emitToolCallFinished,
  emitToolCallFailed,
  emitToolCallCancelled,
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
    // Cline's `AgentToolContext` (typed as `{ signal?: AbortSignal, ... }`
    // in @cline/shared agent.d.ts v0.0.41) lets the runtime cancel an
    // in-flight tool execution. We tap that signal so the recorder sees
    // a `ToolCallCancelled` row when the agent (or supervisor) aborts.
    // If a future SDK version drops `signal`, the `?.addEventListener`
    // chain silently no-ops — the path is feature-gated by presence
    // rather than version checks.
    execute: async (input: unknown, context?: { signal?: AbortSignal }) => {
      const runId = activeRunId()
      const spanId = newSpanId()
      let cancelEmitted = false
      const signal = context?.signal
      const onAbort = () => {
        if (cancelEmitted || !runId) return
        cancelEmitted = true
        emitToolCallCancelled({
          span_id: spanId,
          run_id: runId,
          reason: typeof signal?.reason === "string"
            ? signal.reason
            : signal?.reason instanceof Error
              ? signal.reason.message
              : "aborted",
        })
      }
      if (runId) {
        emitToolCallStarted({
          span_id: spanId,
          run_id: runId,
          tool_name: d.name,
          input_hash: hashJson(input),
        })
      }
      if (signal && typeof signal.addEventListener === "function") {
        if (signal.aborted) {
          onAbort()
        } else {
          signal.addEventListener("abort", onAbort, { once: true })
        }
      }
      try {
        const out = await callRust(d.name, input as Record<string, unknown>)
        if (runId && !cancelEmitted) {
          emitToolCallFinished({
            span_id: spanId,
            run_id: runId,
            output_hash: hashJson(out),
          })
        }
        return out
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        if (runId && !cancelEmitted) {
          emitToolCallFailed({
            span_id: spanId,
            run_id: runId,
            error: message,
          })
        }
        // Per Cline SDK rule: return errors as data, do not throw —
        // throwing counts as a "mistake" against the agent's mistake limit.
        return { error: message }
      } finally {
        if (signal && typeof signal.removeEventListener === "function") {
          signal.removeEventListener("abort", onAbort)
        }
      }
    },
  })
}
