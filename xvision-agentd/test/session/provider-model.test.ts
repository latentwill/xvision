import { describe, expect, it } from "vitest"

import {
  buildProviderModel,
  resolveGatewayProviderId,
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
