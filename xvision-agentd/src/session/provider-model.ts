/**
 * provider-model.ts — build a wrappable AgentModel for real providers via
 * the @cline/llms DefaultGateway.
 *
 * Gateway flow:
 *   1. createGateway() — fresh gateway with all built-in provider registrations.
 *   2. configureProvider({ providerId, apiKey, baseUrl }) — supply credentials
 *      and optional base URL overrides for the specific provider.
 *   3. createAgentModel({ providerId, modelId }) — returns an AgentModel whose
 *      stream() method can be wrapped by model-wrapper.ts.
 *
 * Provider ID mapping:
 *   @cline/llms@0.0.41 does not register "openai-compatible" as a gateway
 *   provider id. It is a provider family / factory target; concrete registered
 *   ids include "openrouter", "deepseek", "groq", "litellm", etc. The xvision
 *   eval path historically sent "openai-compatible" / "openai-compat" for the
 *   whole OpenAI-compatible family, so this module normalizes those legacy ids
 *   to a concrete registered gateway provider before creating the AgentModel.
 *
 * A fresh gateway is created per Agent construction — there is no shared state.
 * Each run has its own credentials/baseUrl and must not share gateway config
 * with concurrent runs.
 */

import type { AgentModel } from "./model-wrapper.js"

// Accessed via @cline/sdk's re-export of @cline/core which re-exports @cline/llms.
// The re-export path is: @cline/sdk → @cline/core → Llms namespace → @cline/llms.
// TypeScript tsconfig has skipLibCheck:true so structural typing is fine here.
//
// Note: we import via @cline/sdk (the only declared dependency) and access the
// Llms namespace. The @cline/llms types flow through correctly.
import { Llms, OPENAI_COMPATIBLE_PROVIDERS } from "@cline/sdk"

const OPENAI_COMPAT_ALIASES = new Set(["openai-compatible", "openai-compat"])
const GENERIC_OPENAI_COMPAT_PROVIDER = "litellm"

export interface BuildProviderModelOptions {
  /** Provider ID as sent by the xvision engine (e.g. "anthropic", "openai-native"). */
  providerId: string
  /** Model ID (e.g. "claude-opus-4-7", "gpt-4o"). */
  modelId: string
  /** API key for the provider. May be absent for local providers (Ollama). */
  apiKey?: string
  /** Custom base URL — used for openai-compatible and self-hosted endpoints. */
  baseUrl?: string
}

function normalizeBaseUrl(value: string): string {
  return value.trim().replace(/\/+$/, "").toLowerCase()
}

function providerIdForOpenAiCompatibleBaseUrl(
  baseUrl: string | undefined,
  knownProviders: readonly string[],
): string {
  if (!baseUrl || baseUrl.trim().length === 0) {
    throw new Error(
      `provider_id "openai-compatible" requires a base_url because ` +
        `"openai-compatible" is a provider family, not a registered @cline/llms gateway id.`,
    )
  }

  const normalizedBaseUrl = normalizeBaseUrl(baseUrl)
  const compatProviders = OPENAI_COMPATIBLE_PROVIDERS as Record<
    string,
    { baseUrl?: string }
  >
  for (const [candidate, config] of Object.entries(compatProviders)) {
    if (!knownProviders.includes(candidate) || !config.baseUrl) continue
    if (normalizeBaseUrl(config.baseUrl) === normalizedBaseUrl) return candidate
  }

  if (knownProviders.includes(GENERIC_OPENAI_COMPAT_PROVIDER)) {
    return GENERIC_OPENAI_COMPAT_PROVIDER
  }

  throw new Error(
    `Cannot route OpenAI-compatible base_url "${baseUrl}": @cline/llms did not ` +
      `register the generic "${GENERIC_OPENAI_COMPAT_PROVIDER}" provider.`,
  )
}

export function resolveGatewayProviderId(
  providerId: string,
  baseUrl: string | undefined,
  knownProviders: readonly string[],
): string {
  const normalizedProviderId = Llms.normalizeProviderId(providerId)
  if (knownProviders.includes(normalizedProviderId)) return normalizedProviderId

  if (OPENAI_COMPAT_ALIASES.has(normalizedProviderId)) {
    return providerIdForOpenAiCompatibleBaseUrl(baseUrl, knownProviders)
  }

  // Backward-compatible rescue path for any older caller that sends an
  // operator-chosen OpenAI-compatible provider name alongside a base_url.
  if (baseUrl && baseUrl.trim().length > 0) {
    return providerIdForOpenAiCompatibleBaseUrl(baseUrl, knownProviders)
  }

  return normalizedProviderId
}

/**
 * Build a wrappable AgentModel for a real (non-mock) provider.
 *
 * Returns an `AgentModel` whose `stream()` method is the public gateway
 * implementation, ready to be passed to `wrapAgentModel(...)`.
 *
 * Throws with a clear message if:
 *   - The provider ID is not registered in the gateway's built-in list AND
 *     no baseUrl fallback applies.
 *   - The gateway itself throws during model construction.
 */
export function buildProviderModel(opts: BuildProviderModelOptions): AgentModel {
  const { modelId, apiKey, baseUrl } = opts

  // Create a fresh gateway. Built-in providers are auto-registered.
  const gateway = Llms.createGateway()
  const knownProviders = gateway.listProviders().map((p: { id: string }) => p.id)
  const providerId = resolveGatewayProviderId(opts.providerId, baseUrl, knownProviders)

  // Configure credentials and optional base URL for this provider.
  // configureProvider is idempotent per provider ID — safe to call once.
  const providerConfig: {
    providerId: string
    apiKey?: string
    baseUrl?: string
  } = { providerId }
  if (apiKey !== undefined) providerConfig.apiKey = apiKey
  if (baseUrl !== undefined) providerConfig.baseUrl = baseUrl
  gateway.configureProvider(providerConfig)

  // Verify the provider exists in the gateway's registry before attempting
  // createAgentModel, so we can give a clear error rather than letting the
  // gateway throw an opaque internal error.
  if (!knownProviders.includes(providerId)) {
    throw new Error(
      `provider_id "${providerId}" is not registered in the @cline/llms gateway. ` +
        `Known providers: ${knownProviders.slice(0, 10).join(", ")}…. ` +
        `For OpenAI-compatible endpoints, use provider_id "openai-compatible" with a base_url.`,
    )
  }

  // Build the AgentModel. The gateway returns a handle object with a stream()
  // method — structurally compatible with our AgentModel interface.
  let model: AgentModel
  try {
    model = gateway.createAgentModel({ providerId, modelId }) as AgentModel
  } catch (err) {
    throw new Error(
      `Failed to construct AgentModel for provider "${providerId}" model "${modelId}": ${
        err instanceof Error ? err.message : String(err)
      }`,
    )
  }

  if (typeof model?.stream !== "function") {
    throw new Error(
      `Gateway.createAgentModel returned an object without a stream() method ` +
        `for provider "${providerId}". This is a @cline/llms API contract violation.`,
    )
  }

  return model
}
