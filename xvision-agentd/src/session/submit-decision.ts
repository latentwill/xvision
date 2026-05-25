import { createTool, type AgentTool } from "@cline/sdk"

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
      capture(JSON.stringify(input))
      return { ok: true }
    },
  })
}
