import { createTool, type AgentTool } from "@cline/sdk"
import { activeRunId } from "./active-run.js"
import { emitDecisionRecorded, newSpanId } from "./emit.js"

/**
 * Name of the built-in lifecycle tool the agent calls to emit its final
 * structured decision.
 *
 * It is NOT a registry-backed Rust callback: the sidecar captures the payload
 * locally and the call completes the run (`lifecycle.completesRun`). Routing it
 * through the registry/`callRust` path would send the final decision back to
 * Rust as a tool invocation instead of capturing it here, which is wrong — the
 * decision is the run's output, not a side-effecting tool call.
 */
export const SUBMIT_DECISION_TOOL = "submit_decision"

/**
 * Build the `submit_decision` lifecycle tool.
 *
 * @param inputSchema the slot's response JSON schema (from
 *   `StartRunParams.decision_schema`); constrains what the agent may submit.
 * @param capture invoked exactly once with the serialized decision; wired by
 *   the caller to `SessionStore.setDecisionJson`.
 */
export function buildSubmitDecisionTool(
  inputSchema: Record<string, unknown>,
  capture: (json: string) => void,
): AgentTool {
  return createTool({
    name: SUBMIT_DECISION_TOOL,
    description:
      "Submit your final structured decision as a single JSON object matching " +
      "the provided schema. Call this exactly once; the call completes the run.",
    inputSchema,
    lifecycle: { completesRun: true },
    execute: async (input: unknown) => {
      const json = JSON.stringify(input)
      capture(json)
      const runId = activeRunId()
      if (runId) {
        const summary = summarizeDecisionInput(input)
        emitDecisionRecorded({
          span_id: newSpanId(),
          run_id: runId,
          action: summary.action,
          outcome: summary.outcome,
          ...(summary.asset !== undefined ? { asset: summary.asset } : {}),
          ...(summary.active_positions !== undefined ? { active_positions: summary.active_positions } : {}),
          decision_json: json,
        })
      }
      return { ok: true }
    },
  })
}

function summarizeDecisionInput(input: unknown): {
  action: string
  outcome: "bought" | "sold" | "closed" | "held" | "unknown"
  asset?: string
  active_positions?: unknown
} {
  const obj = isRecord(input) ? input : {}
  const rawAction =
    stringField(obj, "action") ??
    stringField(obj, "decision") ??
    stringField(obj, "side") ??
    stringField(obj, "order_side") ??
    "unknown"
  const activePositions = extractActivePositions(obj)
  return {
    action: rawAction,
    outcome: classifyDecisionOutcome(rawAction.toLowerCase()),
    ...(stringField(obj, "asset") ?? stringField(obj, "symbol")
      ? { asset: (stringField(obj, "asset") ?? stringField(obj, "symbol"))! }
      : {}),
    ...(activePositions !== undefined ? { active_positions: activePositions } : {}),
  }
}

function classifyDecisionOutcome(action: string): "bought" | "sold" | "closed" | "held" | "unknown" {
  if (["buy", "bought", "long", "long_open", "open_long"].includes(action)) return "bought"
  if (["sell", "sold", "short", "short_open", "open_short"].includes(action)) return "sold"
  if (["close", "closed", "exit", "flat", "close_long", "close_short"].includes(action)) return "closed"
  if (["hold", "held", "noop", "no_op", "none"].includes(action)) return "held"
  return "unknown"
}

function extractActivePositions(obj: Record<string, unknown>): unknown {
  if ("active_positions" in obj) return obj.active_positions
  if ("activePositions" in obj) return obj.activePositions
  if ("positions" in obj) return obj.positions
  if ("position" in obj) return obj.position
  if (isRecord(obj.portfolio)) {
    if ("active_positions" in obj.portfolio) return obj.portfolio.active_positions
    if ("activePositions" in obj.portfolio) return obj.portfolio.activePositions
    if ("positions" in obj.portfolio) return obj.portfolio.positions
  }
  return undefined
}

function stringField(obj: Record<string, unknown>, key: string): string | undefined {
  const value = obj[key]
  return typeof value === "string" && value.trim().length > 0 ? value : undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}
