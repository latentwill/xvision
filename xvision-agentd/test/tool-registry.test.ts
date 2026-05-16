import { describe, expect, it } from "vitest"
import { resetRegistry, handleToolRegistrySet, handleToolRegistryGet } from "../src/methods/tool-registry.js"

const sample = {
  name: "ohlcv",
  version: "1.0.0",
  description: "OHLCV history",
  input_schema: { type: "object" },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "external_read",
  requires_approval: false,
}

describe("tool.registry", () => {
  it("starts empty", () => {
    resetRegistry()
    const r = handleToolRegistryGet()
    expect(r.tools).toEqual([])
  })

  it("set returns count and a stable hash", () => {
    resetRegistry()
    const r1 = handleToolRegistrySet({ tools: [sample] })
    expect(r1.count).toBe(1)
    expect(r1.registry_hash).toMatch(/^[a-f0-9]{64}$/)
    const r2 = handleToolRegistrySet({ tools: [sample] })
    expect(r2.registry_hash).toBe(r1.registry_hash)
  })

  it("get returns the last-set tools", () => {
    resetRegistry()
    handleToolRegistrySet({ tools: [sample] })
    const got = handleToolRegistryGet()
    expect(got.tools).toHaveLength(1)
    expect(got.tools[0]?.name).toBe("ohlcv")
  })

  it("rejects malformed descriptors", () => {
    resetRegistry()
    expect(() => handleToolRegistrySet({ tools: [{ name: "x" } as never] })).toThrow()
  })

  it("rejects invalid side_effect_level value", () => {
    resetRegistry()
    const bad = { ...sample, side_effect_level: "garbage" }
    expect(() => handleToolRegistrySet({ tools: [bad] })).toThrow(/side_effect_level must be one of/)
  })

  it("rejects non-positive, non-integer, or oversized timeout_ms", () => {
    resetRegistry()
    for (const t of [0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY, 600_001]) {
      const bad = { ...sample, timeout_ms: t }
      expect(() => handleToolRegistrySet({ tools: [bad] })).toThrow(/timeout_ms/)
    }
  })

  it("rejects array-typed schemas", () => {
    resetRegistry()
    const badIn = { ...sample, input_schema: [] }
    expect(() => handleToolRegistrySet({ tools: [badIn] })).toThrow(/input_schema must be a non-array object/)
    const badOut = { ...sample, output_schema: [] }
    expect(() => handleToolRegistrySet({ tools: [badOut] })).toThrow(/output_schema must be a non-array object/)
  })
})
