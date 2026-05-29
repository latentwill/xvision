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
      action: "hold",
      asset: "BTC",
      active_positions: [{ asset: "BTC", qty: 0 }],
    } } }])
    handleSessionStartRun({
      run_id: "r-order",
      provider_id: "xvision-mock",
      model_id: "mock",
      system_prompt: "decide",
      allowed_tools: ["submit_decision"],
      decision_schema: { type: "object", additionalProperties: true },
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
