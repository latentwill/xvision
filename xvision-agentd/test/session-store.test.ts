import { describe, it, expect, beforeEach } from "vitest"
import { createStore, type Session, type StartRunConfig } from "../src/session/store.js"

const CONFIG: StartRunConfig = {
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "You are helpful.",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 30000 },
}

describe("session store", () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore({ now: () => 1_700_000_000_000 })
  })

  it("creates and retrieves a session", () => {
    const s = store.create("run-1", CONFIG)
    expect(s.run_id).toBe("run-1")
    expect(s.config).toEqual(CONFIG)
    expect(s.agent).toBeNull()
    expect(s.created_at_ms).toBe(1_700_000_000_000)
    expect(store.get("run-1")).toBe(s)
  })

  it("rejects duplicate run ids", () => {
    store.create("run-1", CONFIG)
    expect(() => store.create("run-1", CONFIG)).toThrow(/already exists/)
  })

  it("returns undefined for unknown runs", () => {
    expect(store.get("missing")).toBeUndefined()
  })

  it("ends a session and removes it", () => {
    store.create("run-1", CONFIG)
    expect(store.end("run-1")).toBe(true)
    expect(store.get("run-1")).toBeUndefined()
  })

  it("returns false when ending an unknown run", () => {
    expect(store.end("missing")).toBe(false)
  })

  it("attachAgent stores the lazy agent without replacing the session", () => {
    const s = store.create("run-1", CONFIG)
    const fakeAgent = { mock: true } as unknown as Session["agent"]
    store.attachAgent("run-1", fakeAgent)
    expect(store.get("run-1")?.agent).toBe(fakeAgent)
    expect(store.get("run-1")).toBe(s)
  })
})
