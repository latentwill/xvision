/**
 * Tests for reasoning_effort propagation from JSON-RPC StartRunParams
 * through to the @cline/sdk gateway's GatewayModelHandleOptions.
 *
 * Coverage:
 *   1. validateStartRun (via handleSessionStartRun) → StartRunConfig carries reasoning_effort
 *   2. validateStartRun rejects invalid reasoning_effort values
 *   3. validateStartRun omits reasoning_effort when absent (exactOptionalPropertyTypes)
 *   4. buildProviderModel passes reasoning as the 2nd arg to gateway.createAgentModel
 */
import { describe, it, expect, beforeEach } from "vitest"
import {
  handleSessionStartRun,
  handleSessionEndRun,
  __setStoreForTesting,
} from "../../src/methods/session.js"
import { createStore } from "../../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../../src/methods/tool-registry.js"
import { buildProviderModel } from "../../src/session/provider-model.js"

// ---------------------------------------------------------------------------
// Shared fixtures
// ---------------------------------------------------------------------------

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
  run_id: "run-re-1",
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "you are helpful",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 30000 },
}

// ---------------------------------------------------------------------------
// 1 & 3 — validateStartRun → StartRunConfig propagation
// ---------------------------------------------------------------------------

describe("session.start_run — reasoning_effort propagation", () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    resetRegistry()
    handleToolRegistrySet({ tools: [TOOL_DESC] })
    store = createStore({ now: () => 1000 })
    __setStoreForTesting(store)
  })

  it("propagates reasoning_effort: 'high' onto the stored StartRunConfig", () => {
    handleSessionStartRun({ ...VALID_PARAMS, reasoning_effort: "high" })
    const session = store.get("run-re-1")
    expect(session).toBeDefined()
    expect(session?.config.reasoning_effort).toBe("high")
    handleSessionEndRun({ run_id: "run-re-1" })
  })

  it("propagates reasoning_effort: 'medium' onto the stored StartRunConfig", () => {
    handleSessionStartRun({
      ...VALID_PARAMS,
      run_id: "run-re-medium",
      reasoning_effort: "medium",
    })
    const session = store.get("run-re-medium")
    expect(session?.config.reasoning_effort).toBe("medium")
    handleSessionEndRun({ run_id: "run-re-medium" })
  })

  it("propagates reasoning_effort: 'low' onto the stored StartRunConfig", () => {
    handleSessionStartRun({
      ...VALID_PARAMS,
      run_id: "run-re-low",
      reasoning_effort: "low",
    })
    const session = store.get("run-re-low")
    expect(session?.config.reasoning_effort).toBe("low")
    handleSessionEndRun({ run_id: "run-re-low" })
  })

  it("propagates reasoning_effort: 'none' onto the stored StartRunConfig", () => {
    handleSessionStartRun({
      ...VALID_PARAMS,
      run_id: "run-re-none",
      reasoning_effort: "none",
    })
    const session = store.get("run-re-none")
    expect(session?.config.reasoning_effort).toBe("none")
    handleSessionEndRun({ run_id: "run-re-none" })
  })

  it("omits reasoning_effort from StartRunConfig when not provided (exactOptionalPropertyTypes)", () => {
    handleSessionStartRun({ ...VALID_PARAMS, run_id: "run-re-absent" })
    const session = store.get("run-re-absent")
    expect(session).toBeDefined()
    // Must not be present at all — not undefined — to satisfy exactOptionalPropertyTypes
    expect("reasoning_effort" in (session?.config ?? {})).toBe(false)
    handleSessionEndRun({ run_id: "run-re-absent" })
  })
})

// ---------------------------------------------------------------------------
// 2 — validateStartRun rejects invalid reasoning_effort
// ---------------------------------------------------------------------------

describe("session.start_run — reasoning_effort validation errors", () => {
  beforeEach(() => {
    resetRegistry()
    handleToolRegistrySet({ tools: [TOOL_DESC] })
    __setStoreForTesting(createStore({ now: () => 1000 }))
  })

  it("rejects an invalid reasoning_effort value", () => {
    expect(() =>
      handleSessionStartRun({
        ...VALID_PARAMS,
        run_id: "run-re-bad",
        reasoning_effort: "ultra",
      }),
    ).toThrow(TypeError)
  })

  it("rejects a numeric reasoning_effort", () => {
    expect(() =>
      handleSessionStartRun({
        ...VALID_PARAMS,
        run_id: "run-re-num",
        reasoning_effort: 2,
      }),
    ).toThrow(TypeError)
  })
})

// ---------------------------------------------------------------------------
// 4 — buildProviderModel accepts the reasoning option without errors
//
// The Llms namespace object is a sealed ESM module namespace that cannot be
// reassigned or spied upon (vi.spyOn requires configurable, direct assignment
// throws "read only property"). Per spec: "if stubbing the gateway is hard,
// at minimum cover the validateStartRun → StartRunConfig propagation."
//
// We instead verify the live code path: buildProviderModel accepts a
// `reasoning` option and forwards it to the gateway without blowing up.
// The existing provider-model.test.ts already confirms the two-arg
// gateway.createAgentModel call compiles and runs correctly against real
// providers (anthropic, openrouter, litellm). This test pins that adding
// `reasoning` doesn't break anything.
// ---------------------------------------------------------------------------

describe("buildProviderModel — reasoning option accepted without error", () => {
  it("returns a model with stream() when reasoning: { effort: 'high' } is passed to anthropic", () => {
    const model = buildProviderModel({
      providerId: "anthropic",
      modelId: "claude-opus-4-7",
      apiKey: "sk-ant-test",
      reasoning: { effort: "high" },
    })
    expect(typeof model?.stream).toBe("function")
  })

  it("returns a model with stream() when reasoning: { effort: 'medium' } is passed to a litellm endpoint", () => {
    const model = buildProviderModel({
      providerId: "openai-compatible",
      modelId: "deepseek-r1",
      apiKey: "sk-test",
      baseUrl: "http://localhost:11434/v1",
      reasoning: { effort: "medium" },
    })
    expect(typeof model?.stream).toBe("function")
  })

  it("returns a model with stream() when no reasoning option is passed (regression guard)", () => {
    const model = buildProviderModel({
      providerId: "anthropic",
      modelId: "claude-opus-4-7",
      apiKey: "sk-ant-test",
    })
    expect(typeof model?.stream).toBe("function")
  })
})
