/**
 * xvision-mock — script-driven AgentModel for deterministic sidecar tests.
 *
 * Step 0 findings (2026-05-17):
 *   - `@cline/llms` has no global provider registry that the agent runtime
 *     reads.  `createGateway()` in @cline/agents creates a fresh DefaultGateway
 *     with its own per-instance registry on every Agent construction, so
 *     pre-registering a handler with `registerHandler()` / `registerProvider()`
 *     does NOT make it visible to a subsequently constructed Agent.
 *   - The correct low-level interface is `AgentModel` from `@cline/shared`:
 *       { stream(request): AsyncIterable<AgentModelEvent> }
 *     where AgentModelEvent =
 *       | { type: "text-delta";       text: string }
 *       | { type: "tool-call-delta";  toolCallId?: string; toolName?: string; input?: unknown }
 *       | { type: "usage";            usage: Partial<AgentUsage> }
 *       | { type: "finish";           reason: AgentModelFinishReason; error?: string }
 *   - Pass `model: buildMockModel()` to `new Agent(...)` instead of providerId.
 *
 * Public API (stable):
 *   setMockScript(turns)   — set the script for the next agent run
 *   resetMockScript()      — clear the script and reset cursor
 *   buildMockModel()       — return an AgentModel that plays the current script
 *   installMockProvider()  — no-op kept for backward-compat; call before each test
 *   MockTurn               — type alias
 */

import type { AgentModel, AgentModelEvent, AgentModelRequest } from "@cline/sdk"

// Public id Task 5's buildAgent can branch on without a magic string.
export const MOCK_PROVIDER_ID = "xvision-mock" as const

// ---------------------------------------------------------------------------
// Script state
// ---------------------------------------------------------------------------

export type MockTurn =
  | { text: string }
  | { toolCall: { name: string; input: unknown } }

let script: MockTurn[] = []
let cursor = 0

export function setMockScript(turns: MockTurn[]): void {
  script = turns
  cursor = 0
}

export function resetMockScript(): void {
  script = []
  cursor = 0
}

/**
 * No-op guard kept so tests can call `installMockProvider()` in `beforeEach`
 * without changes if the call site switches to providerId-based construction
 * in the future.
 */
export function installMockProvider(): void {
  // Nothing to register — we use AgentRuntimeConfigWithModel (the `model:` field).
}

// ---------------------------------------------------------------------------
// AgentModel implementation
// ---------------------------------------------------------------------------

/**
 * Returns a fresh `AgentModel` that advances through `script` on each
 * `stream()` call (one call per agent turn).
 */
export function buildMockModel(): AgentModel {
  return {
    async *stream(_request: AgentModelRequest): AsyncIterable<AgentModelEvent> {
      const idx = cursor++
      const turn = script[idx] ?? { text: "" }

      if ("text" in turn) {
        yield { type: "text-delta", text: turn.text }
        yield { type: "usage", usage: { inputTokens: 1, outputTokens: 1 } }
        yield { type: "finish", reason: "stop" }
        return
      }

      // Tool-call turn: emit a single tool-call-delta with the full input.
      // callId uses the *pre-increment* index so it matches the turn number.
      const callId = `tc-${idx}`
      yield {
        type: "tool-call-delta",
        toolCallId: callId,
        toolName: turn.toolCall.name,
        input: turn.toolCall.input,
      }
      yield { type: "usage", usage: { inputTokens: 1, outputTokens: 1 } }
      yield { type: "finish", reason: "tool-calls" }
    },
  }
}
