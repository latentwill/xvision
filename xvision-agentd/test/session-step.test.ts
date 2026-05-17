import { describe, it, expect, beforeEach, vi } from "vitest"
import {
  handleSessionStartRun,
  handleSessionStep,
  handleSessionEndRun,
  __setStoreForTesting,
} from "../src/methods/session.js"
import { createStore } from "../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../src/methods/tool-registry.js"
import * as callbackClient from "../src/transport/callback-client.js"
import { installMockProvider, setMockScript, resetMockScript } from "../src/testing/mock-provider.js"

const ECHO_DESC = {
  name: "echo",
  version: "1.0.0",
  description: "echoes its input back",
  input_schema: { type: "object", properties: { msg: { type: "string" } }, required: ["msg"] },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "pure",
  requires_approval: false,
}

const PARAMS = {
  run_id: "run-step-1",
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "you are a test",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 30000 },
}

describe("session.step", () => {
  beforeEach(() => {
    installMockProvider()
    resetMockScript()
    resetRegistry()
    handleToolRegistrySet({ tools: [ECHO_DESC] })
    __setStoreForTesting(createStore({ now: () => 1 }))
    vi.restoreAllMocks()
  })

  it("returns assistant text when the model emits text and finishes", async () => {
    setMockScript([{ text: "hello from the model" }])
    handleSessionStartRun(PARAMS)
    const r = await handleSessionStep({ run_id: "run-step-1", prompt: "hi" })
    expect(r.status).toBe("completed")
    expect(r.output_text).toContain("hello from the model")
    expect(r.usage.input_tokens).toBeGreaterThan(0)
    handleSessionEndRun({ run_id: "run-step-1" })
  })

  it("round-trips tool calls through callRust", async () => {
    const spy = vi.spyOn(callbackClient, "callRust").mockResolvedValue({ echoed: "hi" })
    setMockScript([
      { toolCall: { name: "echo", input: { msg: "hi" } } },
      { text: "did it" },
    ])
    handleSessionStartRun(PARAMS)
    const r = await handleSessionStep({ run_id: "run-step-1", prompt: "go" })
    expect(spy).toHaveBeenCalledWith("echo", { msg: "hi" })
    expect(r.status).toBe("completed")
    expect(r.output_text).toContain("did it")
    handleSessionEndRun({ run_id: "run-step-1" })
  })

  it("rejects an unknown run_id", async () => {
    await expect(handleSessionStep({ run_id: "no-such-run", prompt: "x" }))
      .rejects.toThrow(/session not found/)
  })

  it("rejects missing prompt", async () => {
    handleSessionStartRun(PARAMS)
    await expect(handleSessionStep({ run_id: "run-step-1" }))
      .rejects.toThrow(TypeError)
    handleSessionEndRun({ run_id: "run-step-1" })
  })

  it("uses agent.continue on a second step", async () => {
    setMockScript([{ text: "first" }, { text: "second" }])
    handleSessionStartRun(PARAMS)
    const r1 = await handleSessionStep({ run_id: "run-step-1", prompt: "one" })
    const r2 = await handleSessionStep({ run_id: "run-step-1", prompt: "two" })
    expect(r1.output_text).toContain("first")
    expect(r2.output_text).toContain("second")
    expect(r2.iterations).toBeGreaterThanOrEqual(r1.iterations)
    handleSessionEndRun({ run_id: "run-step-1" })
  })
})
