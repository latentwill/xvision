/**
 * Tests for the xvision-mock AgentModel helper.
 *
 * Uses `model: buildMockModel()` (AgentRuntimeConfigWithModel) rather than
 * `providerId: "xvision-mock"` because @cline/agents creates a fresh
 * DefaultGateway per-Agent, making pre-registered providers invisible.
 */
import { describe, it, expect, beforeEach } from "vitest"
import { Agent, createTool } from "@cline/sdk"
import { installMockProvider, setMockScript, resetMockScript, buildMockModel } from "../../src/testing/mock-provider.js"

describe("xvision-mock provider", () => {
  beforeEach(() => {
    installMockProvider()
    resetMockScript()
  })

  it("emits assistant text and completes", async () => {
    setMockScript([{ text: "hello, world" }])

    const agent = new Agent({
      model: buildMockModel(),
      systemPrompt: "test",
      tools: [],
    })

    const result = await agent.run("ping")
    expect(result.status).toBe("completed")
    expect(result.outputText).toContain("hello, world")
  })

  it("scripts a tool call followed by a final text", async () => {
    const calls: Array<{ input: unknown }> = []

    const echoTool = createTool({
      name: "echo",
      description: "echoes",
      inputSchema: { type: "object", properties: { msg: { type: "string" } }, required: ["msg"] },
      execute: async (input: unknown) => {
        calls.push({ input })
        return { echoed: (input as { msg: string }).msg }
      },
    })

    setMockScript([
      { toolCall: { name: "echo", input: { msg: "hi" } } },
      { text: "done" },
    ])

    const agent = new Agent({
      model: buildMockModel(),
      systemPrompt: "test",
      tools: [echoTool],
    })

    const result = await agent.run("go")
    expect(result.status).toBe("completed")
    expect(calls).toEqual([{ input: { msg: "hi" } }])
    expect(result.outputText).toContain("done")
  })
})
