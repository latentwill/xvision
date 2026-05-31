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
  decisionContext?: Record<string, unknown>,
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
        const summary = summarizeDecisionInput(input, decisionContext)
        emitDecisionRecorded({
          span_id: newSpanId(),
          run_id: runId,
          action: summary.action,
          outcome: summary.outcome,
          ...(summary.asset !== undefined ? { asset: summary.asset } : {}),
          ...(summary.active_positions !== undefined ? { active_positions: summary.active_positions } : {}),
          ...(summary.portfolio !== undefined ? { portfolio: summary.portfolio } : {}),
          decision_json: json,
        })
      }
      return { ok: true }
    },
  })
}

function summarizeDecisionInput(input: unknown, decisionContext?: Record<string, unknown>): {
  action: string
  outcome: "bought" | "sold" | "closed" | "held" | "unknown"
  asset?: string
  active_positions?: unknown
  portfolio?: unknown
} {
  const obj = isRecord(input) ? input : {}
  const context = decisionContext ?? {}
  const rawAction =
    stringField(obj, "action") ??
    stringField(obj, "decision") ??
    stringField(obj, "side") ??
    stringField(obj, "order_side") ??
    "unknown"
  const activePositions = extractActivePositions(obj) ?? extractActivePositions(context)
  const portfolio = isRecord(obj.portfolio)
    ? obj.portfolio
    : isRecord(context.portfolio)
      ? context.portfolio
      : undefined
  const asset =
    stringField(obj, "asset") ??
    stringField(obj, "symbol") ??
    stringField(context, "asset") ??
    stringField(context, "symbol")
  return {
    action: rawAction,
    outcome: classifyDecisionOutcome(rawAction.toLowerCase(), activePositions),
    ...(asset ? { asset } : {}),
    ...(activePositions !== undefined ? { active_positions: activePositions } : {}),
    ...(portfolio !== undefined ? { portfolio } : {}),
  }
}

function classifyDecisionOutcome(action: string, activePositions?: unknown): "bought" | "sold" | "closed" | "held" | "unknown" {
  if (["buy", "bought", "long", "long_open", "open_long"].includes(action)) return "bought"
  if (["sell", "sold", "short", "short_open", "open_short"].includes(action)) return "sold"
  if (["close", "closed", "exit", "close_long", "close_short"].includes(action)) return "closed"
  if (action === "flat") return hasActivePosition(activePositions) ? "closed" : "held"
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

function hasActivePosition(value: unknown): boolean {
  if (value == null) return false
  if (typeof value === "number") return value !== 0
  if (typeof value === "string") {
    const normalized = value.trim().toLowerCase()
    if (!normalized || ["0", "none", "flat", "no_position"].includes(normalized)) return false
    return true
  }
  if (Array.isArray(value)) return value.some(hasActivePosition)
  if (!isRecord(value)) return false

  for (const key of ["qty", "quantity", "size", "position", "net_position", "amount"]) {
    const raw = value[key]
    if (typeof raw === "number") return raw !== 0
    if (typeof raw === "string") {
      const parsed = Number(raw)
      if (Number.isFinite(parsed)) return parsed !== 0
    }
  }

  return Object.values(value).some(hasActivePosition)
}

function stringField(obj: Record<string, unknown>, key: string): string | undefined {
  const value = obj[key]
  return typeof value === "string" && value.trim().length > 0 ? value : undefined
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value)
}
