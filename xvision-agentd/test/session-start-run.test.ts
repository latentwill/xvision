import { describe, it, expect, beforeEach } from "vitest"
import { handleSessionStartRun, handleSessionEndRun, __setStoreForTesting } from "../src/methods/session.js"
import { createStore } from "../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../src/methods/tool-registry.js"

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
  run_id: "run-1",
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "you are helpful",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 30000 },
}

describe("session.start_run", () => {
  beforeEach(() => {
    resetRegistry()
    handleToolRegistrySet({ tools: [TOOL_DESC] })
    __setStoreForTesting(createStore({ now: () => 100 }))
  })

  it("returns run_id on success", () => {
    const r = handleSessionStartRun(VALID_PARAMS)
    expect(r).toEqual({ run_id: "run-1", started_at_ms: 100 })
  })

  it("rejects missing run_id", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, run_id: undefined })).toThrow(TypeError)
  })

  it("rejects missing provider_id", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, provider_id: undefined })).toThrow(TypeError)
  })

  it("rejects empty allowed_tools", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, allowed_tools: [] })).toThrow(TypeError)
  })

  it("rejects a tool name not in the registry", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, allowed_tools: ["not_registered"] }))
      .toThrow(/unknown tool/)
  })

  it("rejects duplicate run_id", () => {
    handleSessionStartRun(VALID_PARAMS)
    expect(() => handleSessionStartRun(VALID_PARAMS)).toThrow(/already exists/)
  })
})

describe("session.end_run", () => {
  beforeEach(() => {
    resetRegistry()
    handleToolRegistrySet({ tools: [TOOL_DESC] })
    __setStoreForTesting(createStore({ now: () => 100 }))
  })

  it("ends an existing session", () => {
    handleSessionStartRun(VALID_PARAMS)
    expect(handleSessionEndRun({ run_id: "run-1" })).toEqual({ ended: true })
  })

  it("returns ended=false for an unknown run", () => {
    expect(handleSessionEndRun({ run_id: "missing" })).toEqual({ ended: false })
  })

  it("rejects missing run_id", () => {
    expect(() => handleSessionEndRun({})).toThrow(TypeError)
  })
})
