import { describe, it, expect, beforeEach, afterEach } from "vitest"
import * as net from "node:net"
import { mkdtempSync, rmSync } from "node:fs"
import { tmpdir } from "node:os"
import * as path from "node:path"
import {
  handleSessionStartRun,
  handleSessionStep,
  __setStoreForTesting,
} from "../../src/methods/session.js"
import { createStore } from "../../src/session/store.js"
import { setMockScript, resetMockScript } from "../../src/testing/mock-provider.js"
import { resetRegistry } from "../../src/methods/tool-registry.js"
import { resetForTesting, setEventSocketPath } from "../../src/transport/event-client.js"

const BUDGET = { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 10000 }
const TRADER_SCHEMA = {
  type: "object",
  properties: {
    action: { type: "string" },
    conviction: { type: "number" },
    justification: { type: "string" },
  },
}


describe("submit_decision lifecycle tool", () => {
  let tmpDir: string | undefined

  beforeEach(() => {
    resetMockScript()
    resetRegistry()
    resetForTesting()
    __setStoreForTesting(createStore({ now: () => 0 }))
  })

  afterEach(() => {
    resetForTesting()
    if (tmpDir) {
      try { rmSync(tmpDir, { recursive: true, force: true }) } catch {}
      tmpDir = undefined
    }
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

  it("emits model prompt/response before the decision span", async () => {
    const received: Array<{ method: string; params: Record<string, unknown> }> = []
    tmpDir = mkdtempSync(path.join(tmpdir(), "xvision-submit-decision-"))
    const socketPath = path.join(tmpDir, "events.sock")
    let server: net.Server | undefined
    const accepted = new Promise<void>((resolve) => {
      server = net.createServer((conn) => {
        let buf = ""
        conn.on("data", (chunk) => {
          buf += chunk.toString("utf8")
          let idx: number
          while ((idx = buf.indexOf("\n")) !== -1) {
            const line = buf.slice(0, idx)
            buf = buf.slice(idx + 1)
            if (!line) continue
            const msg = JSON.parse(line) as { method: string; params: Record<string, unknown> }
            received.push(msg)
          }
        })
        resolve()
      })
      server.listen(socketPath)
    })
    setEventSocketPath(socketPath)

    setMockScript([{ toolCall: { name: "submit_decision", input: {
      action: "flat",
      asset: "BTC",
    } } }])
    handleSessionStartRun({
      run_id: "r-order",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: { type: "object", additionalProperties: true },
      decision_context: {
        active_positions: [{ asset: "BTC", qty: 0 }],
        portfolio: { cash: 1000 },
      },
      budget_limits: BUDGET,
    })
    const result = await handleSessionStep({ run_id: "r-order", prompt: "go" })
    expect(result.status).toBe("completed")
    await accepted
    await new Promise((r) => setTimeout(r, 50))

    const modelFinishedIndex = received.findIndex((m) => m.method === "event.model_call_finished")
    const decisionIndex = received.findIndex((m) => m.method === "event.decision_recorded")
    expect(modelFinishedIndex).toBeGreaterThanOrEqual(0)
    expect(decisionIndex).toBeGreaterThan(modelFinishedIndex)
    const modelFinished = received[modelFinishedIndex]!.params
    expect(modelFinished.prompt).toContain("go")
    expect(modelFinished.response).toContain("submit_decision")
    const decision = received[decisionIndex]!.params
    expect(decision.outcome).toBe("held")
    expect(decision.active_positions).toEqual([{ asset: "BTC", qty: 0 }])
    expect(decision.portfolio).toEqual({ cash: 1000 })
    server?.close()
  })

  it("classifies flat as closed when runtime context has an active position", async () => {
    const received: Array<{ method: string; params: Record<string, unknown> }> = []
    tmpDir = mkdtempSync(path.join(tmpdir(), "xvision-submit-decision-"))
    const socketPath = path.join(tmpDir, "events.sock")
    let server: net.Server | undefined
    const accepted = new Promise<void>((resolve) => {
      server = net.createServer((conn) => {
        let buf = ""
        conn.on("data", (chunk) => {
          buf += chunk.toString("utf8")
          let idx: number
          while ((idx = buf.indexOf("\n")) !== -1) {
            const line = buf.slice(0, idx)
            buf = buf.slice(idx + 1)
            if (!line) continue
            received.push(JSON.parse(line) as { method: string; params: Record<string, unknown> })
          }
        })
        resolve()
      })
      server.listen(socketPath)
    })
    setEventSocketPath(socketPath)

    setMockScript([{ toolCall: { name: "submit_decision", input: { action: "flat", asset: "BTC" } } }])
    handleSessionStartRun({
      run_id: "r-flat-close",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: { type: "object", additionalProperties: true },
      decision_context: { active_positions: [{ asset: "BTC", qty: 2 }] },
      budget_limits: BUDGET,
    })
    const result = await handleSessionStep({ run_id: "r-flat-close", prompt: "go" })
    expect(result.status).toBe("completed")
    await accepted
    await new Promise((r) => setTimeout(r, 50))

    const decision = received.find((m) => m.method === "event.decision_recorded")?.params
    expect(decision?.outcome).toBe("closed")
    expect(decision?.active_positions).toEqual([{ asset: "BTC", qty: 2 }])
    server?.close()
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

  it("rejects malformed decision_context", () => {
    expect(() =>
      handleSessionStartRun({
        run_id: "r-context",
        provider_id: "xvision-mock",
        model_id: "mock",
        system_prompt: "decide",
        allowed_tools: ["submit_decision"],
        decision_schema: { type: "object", additionalProperties: true },
        decision_context: [],
        budget_limits: BUDGET,
      }),
    ).toThrow(/decision_context/)
  })

  it("accepts final decision JSON text when the model does not call the tool", async () => {
    setMockScript([
      {
        text:
          "Reasoning before the final answer.\n" +
          "{\"action\":\"long_open\",\"conviction\":90,\"justification\":\"oversold bounce\"}",
      },
    ])
    handleSessionStartRun({
      run_id: "r-json-text",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: TRADER_SCHEMA,
      budget_limits: BUDGET,
    })
    const r = await handleSessionStep({ run_id: "r-json-text", prompt: "go" })
    expect(r.status).toBe("completed")
    expect(r.decision_json).toBeDefined()
    expect(JSON.parse(r.decision_json!)).toEqual({
      action: "long_open",
      conviction: 90,
      justification: "oversold bounce",
    })
  })

  it("accepts final submitDecision wrapper JSON text when the model does not call the tool", async () => {
    setMockScript([
      {
        text:
          "Final answer:\n" +
          "{\"name\":\"submitDecision\",\"arguments\":{\"action\":\"hold\",\"conviction\":75,\"justification\":\"wait for confirmation\"}}",
      },
    ])
    handleSessionStartRun({
      run_id: "r-wrapper-json-text",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: TRADER_SCHEMA,
      budget_limits: BUDGET,
    })
    const r = await handleSessionStep({ run_id: "r-wrapper-json-text", prompt: "go" })
    expect(r.status).toBe("completed")
    expect(r.decision_json).toBeDefined()
    expect(JSON.parse(r.decision_json!)).toEqual({
      name: "submitDecision",
      arguments: {
        action: "hold",
        conviction: 75,
        justification: "wait for confirmation",
      },
    })
  })

  it("accepts final-labeled decision JSON even with trailing prose", async () => {
    setMockScript([
      {
        text:
          "Long chain-of-thought omitted.\n" +
          "Final decision JSON:\n" +
          "{\"action\":\"long_open\",\"conviction\":88,\"justification\":\"ORB breakout confirmed\"}\n" +
          "I would submit this decision now.",
      },
    ])
    handleSessionStartRun({
      run_id: "r-final-labeled-json-text",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: TRADER_SCHEMA,
      budget_limits: BUDGET,
    })
    const r = await handleSessionStep({ run_id: "r-final-labeled-json-text", prompt: "go" })
    expect(r.status).toBe("completed")
    expect(r.decision_json).toBeDefined()
    expect(JSON.parse(r.decision_json!)).toEqual({
      action: "long_open",
      conviction: 88,
      justification: "ORB breakout confirmed",
    })
  })

  it("does not accept schema-shaped example JSON when it is not the final answer", async () => {
    setMockScript([
      {
        text:
          "I am not ready to decide. An example would be " +
          "{\"action\":\"hold\",\"conviction\":50,\"justification\":\"example only\"}. " +
          "Need more data before submitting.",
      },
    ])
    handleSessionStartRun({
      run_id: "r-example-json-text",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: TRADER_SCHEMA,
      budget_limits: BUDGET,
    })
    const r = await handleSessionStep({ run_id: "r-example-json-text", prompt: "go" })
    expect(r.status).toBe("completed")
    expect(r.decision_json).toBeUndefined()
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
