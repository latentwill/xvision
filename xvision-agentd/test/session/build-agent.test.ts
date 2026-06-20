import { describe, it, expect } from "vitest"
import { sanitizeSystemPrompt } from "../../src/session/build-agent.js"

const CORRECTION_FRAGMENT = "JSON text is also accepted"

describe("sanitizeSystemPrompt", () => {
  it("is a no-op for a clean prompt", () => {
    const prompt = "You are a trading analyst. Provide a clear decision."
    expect(sanitizeSystemPrompt(prompt)).toBe(prompt)
  })

  it("allows prompts that ask weak tool-callers to output json", () => {
    const prompt = "Output JSON only. Do not include any other text."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
    expect(result).not.toContain("raw JSON is not accepted")
    expect(result).not.toContain("outputting JSON text is not accepted")
  })

  it("does not turn strict json prompts into tool-only prompts", () => {
    const prompt = "You must respond in strict JSON format."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
    expect(result).not.toContain("You MUST call the submit_decision tool")
  })

  it("does not turn json-only prompts into tool-only prompts", () => {
    const prompt = "Respond with JSON only, no other text."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
    expect(result).not.toContain("You MUST call the submit_decision tool")
  })

  it("does not turn output json prompts into tool-only prompts", () => {
    const prompt = "Always output JSON as your response."
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
    expect(result).not.toContain("You MUST call the submit_decision tool")
  })

  it("is case-insensitive", () => {
    const prompt = "OUTPUT JSON ONLY"
    const result = sanitizeSystemPrompt(prompt)
    expect(result).toContain(CORRECTION_FRAGMENT)
    expect(result).not.toContain("You MUST call the submit_decision tool")
  })

  it("does not double-append when called twice", () => {
    const prompt = "Output JSON only."
    const once = sanitizeSystemPrompt(prompt)
    const twice = sanitizeSystemPrompt(once)
    const count = (twice.match(/JSON text is also accepted/g) ?? []).length
    expect(count).toBe(1)
  })
})
