import { describe, it, expect, vi, beforeEach } from "vitest"
import { shimRegistryToTools } from "../src/session/tool-shim.js"
import * as callbackClient from "../src/transport/callback-client.js"

const DESCRIPTORS = [
  {
    name: "echo",
    version: "1.0.0",
    description: "Returns its input unchanged.",
    input_schema: { type: "object", properties: { message: { type: "string" } }, required: ["message"] },
    output_schema: { type: "object" },
    timeout_ms: 5000,
    side_effect_level: "pure" as const,
    requires_approval: false,
  },
  {
    name: "write_file",
    version: "1.0.0",
    description: "Writes a file to disk.",
    input_schema: { type: "object" },
    output_schema: { type: "object" },
    timeout_ms: 5000,
    side_effect_level: "external_write" as const,
    requires_approval: false,
  },
]

describe("shimRegistryToTools", () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it("returns only allow-listed tools", () => {
    const tools = shimRegistryToTools(DESCRIPTORS, ["echo"], { allowWrites: false })
    expect(tools.map(t => t.name)).toEqual(["echo"])
  })

  it("skips external_write tools when allowWrites is false", () => {
    const tools = shimRegistryToTools(DESCRIPTORS, ["echo", "write_file"], { allowWrites: false })
    expect(tools.map(t => t.name)).toEqual(["echo"])
  })

  it("includes external_write tools when allowWrites is true", () => {
    const tools = shimRegistryToTools(DESCRIPTORS, ["echo", "write_file"], { allowWrites: true })
    expect(tools.map(t => t.name).sort()).toEqual(["echo", "write_file"])
  })

  it("each tool's execute proxies to callRust", async () => {
    const spy = vi.spyOn(callbackClient, "callRust").mockResolvedValue({ echoed: "hi" })
    const [echo] = shimRegistryToTools(DESCRIPTORS, ["echo"], { allowWrites: false })
    const result = await echo.execute({ message: "hi" }, {
      agentId: "a", conversationId: "c", iteration: 1,
    })
    expect(spy).toHaveBeenCalledWith("echo", { message: "hi" })
    expect(result).toEqual({ echoed: "hi" })
  })

  it("returns a structured error instead of throwing on Rust-side failure", async () => {
    vi.spyOn(callbackClient, "callRust").mockRejectedValue(new Error("rust unreachable"))
    const [echo] = shimRegistryToTools(DESCRIPTORS, ["echo"], { allowWrites: false })
    const result = await echo.execute({ message: "hi" }, {
      agentId: "a", conversationId: "c", iteration: 1,
    }) as { error?: string }
    expect(result.error).toContain("rust unreachable")
  })

  it("rejects unknown allowed_tools", () => {
    expect(() => shimRegistryToTools(DESCRIPTORS, ["does_not_exist"], { allowWrites: false }))
      .toThrow(/unknown tool/)
  })
})
