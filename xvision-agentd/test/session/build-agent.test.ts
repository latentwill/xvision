import { describe, it, expect } from "vitest"
import { sanitizeSystemPrompt } from "../../src/session/build-agent.js"

const CORRECTION_FRAGMENT = "You MUST call the submit_decision tool"

describe("sanitizeSystemPrompt", () => {
  it("is a no-op for a clean prompt", () => {
    const prompt = "You are a trading analyst. Provide a clear decision."
    expect(sanitizeSystemPrompt(prompt)).toBe(prompt)
  })

  it("appends correction when prompt says 'output json only'", () => {
    const prompt = "Output JSON only. Do not include any other text."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
  })

  it("appends correction when prompt says 'strict json'", () => {
    const prompt = "You must respond in strict JSON format."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
  })

  it("appends correction when prompt says 'json only'", () => {
    const prompt = "Respond with JSON only, no other text."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
  })

  it("appends correction when prompt says 'output json' (partial match)", () => {
    const prompt = "Always output JSON as your response."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
  })

  it("is case-insensitive", () => {
    const prompt = "OUTPUT JSON ONLY"
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
  })

  it("does not double-append when called twice", () => {
    const prompt = "Output JSON only."
    const once = sanitizeSystemPrompt(prompt)
    const twice = sanitizeSystemPrompt(once)
    const count = (twice.match(/You MUST call the submit_decision tool/g) ?? []).length
    expect(count).toBe(1)
  })
})
