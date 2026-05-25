import { describe, it, expect, beforeEach } from "vitest"
import {
  handleSessionStartRun,
  handleSessionStep,
  __setStoreForTesting,
} from "../../src/methods/session.js"
import { createStore } from "../../src/session/store.js"
import { setMockScript, resetMockScript } from "../../src/testing/mock-provider.js"
import { resetRegistry } from "../../src/methods/tool-registry.js"

const BUDGET = { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 10000 }

describe("submit_decision lifecycle tool", () => {
  beforeEach(() => {
    resetMockScript()
    resetRegistry()
    __setStoreForTesting(createStore({ now: () => 0 }))
  })

  it("captures the decision payload from the submit_decision tool call", async () => {
    // Mock agent: one turn that calls submit_decision, then the run completes.
    setMockScript([{ toolCall: { name: "submit_decision", input: { action: "buy", size: 1 } } }])
    handleSessionStartRun({
      run_id: "r1",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: { type: "object", additionalProperties: true },
      budget_limits: BUDGET,
    })
    const r = await handleSessionStep({ run_id: "r1", prompt: "go" })
    expect(r.status).toBe("completed")
    expect(r.decision_json).toBeDefined()
    expect(JSON.parse(r.decision_json!)).toEqual({ action: "buy", size: 1 })
  })

  it("rejects start_run when submit_decision lacks a decision_schema", () => {
    expect(() =>
      handleSessionStartRun({
        run_id: "r2",
        provider_id: "xvision-mock",
        model_id: "mock",
        system_prompt: "decide",
        allowed_tools: ["submit_decision"],
        budget_limits: BUDGET,
      }),
    ).toThrow(/decision_schema/)
  })

  it("leaves decision_json undefined when the agent never submits", async () => {
    setMockScript([{ text: "thinking out loud, no decision" }])
    handleSessionStartRun({
      run_id: "r3",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: { type: "object", additionalProperties: true },
      budget_limits: BUDGET,
    })
    const r = await handleSessionStep({ run_id: "r3", prompt: "go" })
    expect(r.decision_json).toBeUndefined()
  })
})
