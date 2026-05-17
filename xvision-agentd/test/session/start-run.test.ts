/**
 * Lightweight assertions that `session.start_run` continues to validate
 * and store `budget_limits` as documented after the budget-enforcement
 * track lands. The original tests in `test/session-start-run.test.ts`
 * still own the bulk of the surface; this file just pins the contract's
 * first acceptance criterion.
 */
import { describe, it, expect, beforeEach } from "vitest"
import {
  handleSessionStartRun,
  handleSessionEndRun,
  __setStoreForTesting,
} from "../../src/methods/session.js"
import { createStore } from "../../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../../src/methods/tool-registry.js"

const TOOL_DESC = {
  name: "echo",
  version: "1.0.0",
  description: "echoes",
  input_schema: { type: "object" },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "pure",
  requires_approval: false,
}

const VALID_PARAMS = {
  run_id: "run-bgt",
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "you are helpful",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 10, max_output_tokens: 20, max_wall_ms: 30 },
}

describe("session.start_run after budget enforcement", () => {
  let store: ReturnType<typeof createStore>
  beforeEach(() => {
    resetRegistry()
    handleToolRegistrySet({ tools: [TOOL_DESC] })
    store = createStore({ now: () => 42 })
    __setStoreForTesting(store)
  })

  it("persists budget_limits verbatim on the session", () => {
    const r = handleSessionStartRun(VALID_PARAMS)
    expect(r).toEqual({ run_id: "run-bgt", started_at_ms: 42 })
    const s = store.get("run-bgt")
    expect(s).toBeDefined()
    expect(s?.config.budget_limits).toEqual(VALID_PARAMS.budget_limits)
    expect(s?.usage).toEqual({ input_tokens: 0, output_tokens: 0 })
    handleSessionEndRun({ run_id: "run-bgt" })
  })

  it("still rejects a negative or zero budget limit", () => {
    expect(() =>
      handleSessionStartRun({
        ...VALID_PARAMS,
        budget_limits: { ...VALID_PARAMS.budget_limits, max_wall_ms: 0 },
      }),
    ).toThrow(TypeError)
  })
})
