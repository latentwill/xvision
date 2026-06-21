import { describe, expect, it } from "vitest"

import {
  buildProviderModel,
  resolveGatewayProviderId,
  resolveGatewayReasoning,
} from "../../src/session/provider-model.js"

const KNOWN_PROVIDERS = [
  "openai-native",
  "anthropic",
  "deepseek",
  "groq",
  "litellm",
  "openrouter",
]

describe("buildProviderModel — Cline gateway provider registration", () => {
  it("routes the legacy 'openai-compatible' family id to a registered OpenRouter provider", () => {
    expect(
      resolveGatewayProviderId(
        "openai-compatible",
        "https://openrouter.ai/api/v1",
        KNOWN_PROVIDERS,
      ),
    ).toBe("openrouter")

    const model = buildProviderModel({
      providerId: "openai-compatible",
      modelId: "deepseek/deepseek-v4-flash",
      apiKey: "sk-test",
      baseUrl: "https://openrouter.ai/api/v1",
    })
    expect(typeof model?.stream).toBe("function")
  })

  it("routes the older 'openai-compat' spelling to a registered provider", () => {
    expect(
      resolveGatewayProviderId("openai-compat", "https://api.deepseek.com/v1", KNOWN_PROVIDERS),
    ).toBe("deepseek")

    const model = buildProviderModel({
      providerId: "openai-compat",
      modelId: "deepseek-chat",
      apiKey: "sk-test",
      baseUrl: "https://api.deepseek.com/v1",
    })
    expect(typeof model?.stream).toBe("function")
  })

  it("uses litellm as the generic carrier for custom OpenAI-compatible endpoints", () => {
    expect(
      resolveGatewayProviderId("openai-compatible", "https://proxy.example/v1", KNOWN_PROVIDERS),
    ).toBe("litellm")

    const model = buildProviderModel({
      providerId: "openai-compatible",
      modelId: "custom-model",
      apiKey: "sk-test",
      baseUrl: "https://proxy.example/v1",
    })
    expect(typeof model?.stream).toBe("function")
  })

  it("rescues unknown provider ids with a base_url through the generic carrier", () => {
    expect(
      resolveGatewayProviderId("some-unknown-svc", "https://proxy.example/v1", KNOWN_PROVIDERS),
    ).toBe("litellm")

    const model = buildProviderModel({
      providerId: "some-unknown-svc",
      modelId: "custom-model",
      apiKey: "sk-test",
      baseUrl: "https://proxy.example/v1",
    })
    expect(typeof model?.stream).toBe("function")
  })

  it("passes built-in provider ids through unchanged", () => {
    const model = buildProviderModel({
      providerId: "anthropic",
      modelId: "claude-opus-4-7",
      apiKey: "sk-ant-test",
    })
    expect(typeof model?.stream).toBe("function")
  })

  it("throws a clear error when the OpenAI-compatible family id has no base_url", () => {
    expect(() =>
      buildProviderModel({
        providerId: "openai-compatible",
        modelId: "m",
      }),
    ).toThrow(/requires a base_url/i)
  })

  it("throws a clear error when an unknown provider_id has no base_url", () => {
    expect(() =>
      buildProviderModel({
        providerId: "some-unknown-svc",
        modelId: "m",
      }),
    ).toThrow(/not registered.*base_url/i)
  })
})

describe("resolveGatewayReasoning — provider-aware Cline reasoning options", () => {
  it("omits native reasoning for Ollama even when the engine sent an effort", () => {
    expect(
      resolveGatewayReasoning({
        providerId: "ollama",
        modelId: "deepseek-r1",
        reasoningEffort: "medium",
      }),
    ).toBeUndefined()
  })

  it("omits native reasoning for local OpenAI-compatible carriers", () => {
    expect(
      resolveGatewayReasoning({
        providerId: "litellm",
        modelId: "deepseek-r1",
        baseUrl: "http://127.0.0.1:8080/v1",
        reasoningEffort: "medium",
      }),
    ).toBeUndefined()
  })

  it("omits native reasoning for non-loopback generic OpenAI-compatible carriers", () => {
    expect(
      resolveGatewayReasoning({
        providerId: "litellm",
        modelId: "deepseek-r1",
        baseUrl: "http://ollama:11434/v1",
        reasoningEffort: "medium",
      }),
    ).toBeUndefined()
  })

  it("maps explicit none to Cline's reasoning disable shape, including Ollama", () => {
    expect(
      resolveGatewayReasoning({
        providerId: "ollama",
        modelId: "qwen3:4b",
        reasoningEffort: "none",
      }),
    ).toEqual({ enabled: false })
    expect(
      resolveGatewayReasoning({
        providerId: "deepseek",
        modelId: "deepseek-r1",
        reasoningEffort: "none",
      }),
    ).toEqual({ enabled: false })
  })

  it("keeps native effort for non-local providers", () => {
    expect(
      resolveGatewayReasoning({
        providerId: "deepseek",
        modelId: "deepseek-r1",
        reasoningEffort: "medium",
      }),
    ).toEqual({ effort: "medium" })
  })

  it("defaults to medium only from Cline catalog reasoning capability", () => {
    expect(
      resolveGatewayReasoning(
        {
          providerId: "anthropic",
          modelId: "claude-sonnet-4-6",
        },
        [{ id: "claude-sonnet-4-6", capabilities: ["text", "reasoning"] }],
      ),
    ).toEqual({ effort: "medium" })

    expect(
      resolveGatewayReasoning(
        {
          providerId: "anthropic",
          modelId: "claude-haiku-4-5",
        },
        [{ id: "claude-haiku-4-5", capabilities: ["text"] }],
      ),
    ).toBeUndefined()
  })

  it("does not default catalog reasoning on suppressed local carriers", () => {
    expect(
      resolveGatewayReasoning(
        {
          providerId: "litellm",
          modelId: "qwen3:4b",
        },
        [{ id: "qwen3:4b", capabilities: ["text", "reasoning"] }],
      ),
    ).toBeUndefined()
  })
})
