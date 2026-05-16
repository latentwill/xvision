import { describe, expect, it } from "vitest"
import { handleToolInvoke } from "../src/methods/tool-invoke.js"
import { setCallbackSocketPath } from "../src/transport/callback-client.js"

describe("tool.invoke params validation", () => {
  it("rejects missing name", async () => {
    setCallbackSocketPath(undefined)
    await expect(handleToolInvoke({ input: {} })).rejects.toThrow(/name/)
  })
  it("rejects missing input", async () => {
    setCallbackSocketPath(undefined)
    await expect(handleToolInvoke({ name: "x" })).rejects.toThrow(/input/)
  })
  it("rejects when callback socket unconfigured", async () => {
    setCallbackSocketPath(undefined)
    await expect(handleToolInvoke({ name: "x", input: {} })).rejects.toThrow(/callback socket/)
  })
  it("rejects array-typed input", async () => {
    setCallbackSocketPath(undefined)
    await expect(handleToolInvoke({ name: "x", input: [] })).rejects.toThrow(/non-array object/)
  })
})
