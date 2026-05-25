import { describe, it, expect, beforeEach } from "vitest"
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { buildAgent } from "../src/session/build-agent.js"
import { MOCK_PROVIDER_ID } from "../src/testing/mock-provider.js"
import { resetRegistry } from "../src/methods/tool-registry.js"

// Forbidden auth surface: anything implying consumer-subscription / OAuth auth.
// The Anthropic Agent SDK license (which Cline inherits) forbids authenticating
// via consumer subscriptions (Claude Pro/Max OAuth) rather than API keys.
const FORBIDDEN = /oauth|sessionKey|sessionToken|subscriptionToken|claude_?pro|claude_?max|refreshToken/i

function scan(rel: string): string[] {
  const p = fileURLToPath(new URL(rel, import.meta.url))
  return readFileSync(p, "utf8").split("\n").filter((l) => FORBIDDEN.test(l))
}

describe("license guard: API-key auth only", () => {
  beforeEach(() => resetRegistry())

  it("build-agent.ts source contains no OAuth/subscription auth surface", () => {
    const offending = scan("../src/session/build-agent.ts")
    expect(offending, `OAuth/subscription surface found:\n${offending.join("\n")}`).toEqual([])
  })

  it("store.ts StartRunConfig source carries no OAuth/subscription field", () => {
    const offending = scan("../src/session/store.ts")
    expect(offending, `OAuth/subscription field found:\n${offending.join("\n")}`).toEqual([])
  })

  it("buildAgent constructs from an apiKey-only config (no session/oauth token)", () => {
    const agent = buildAgent({
      provider_id: MOCK_PROVIDER_ID,
      model_id: "mock",
      api_key: "sk-test",
      system_prompt: "decide",
      allowed_tools: [],
      budget_limits: { max_input_tokens: 1, max_output_tokens: 1, max_wall_ms: 1 },
    })
    expect(agent).toBeDefined()
  })
})
