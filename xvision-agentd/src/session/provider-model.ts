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
 *   @cline/llms@0.0.41 did not register "openai-compatible" as a gateway
 *   provider id. @cline/llms@0.0.47+ DOES register it (defaulting to
 *   https://api.openai.com/v1 when no base_url is supplied). xvision's callers
 *   of openai-compatible are self-hosted endpoints (Ollama, vLLM, proxies) and
 *   MUST supply an explicit base_url — silently routing to api.openai.com would
 *   be a regression. resolveGatewayProviderId therefore intercepts the
 *   "openai-compatible" / "openai-compat" alias BEFORE the knownProviders
 *   shortcut, enforcing base_url presence and routing to a concrete registered
 *   id (openrouter, deepseek, litellm, …) based on the base_url. Concrete
 *   registered ids include "openrouter", "deepseek", "groq", "litellm", etc.
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
  /**
   * Reasoning options forwarded to the gateway as `GatewayModelHandleOptions.reasoning`.
   * Supports CoT models (e.g. deepseek-r1 via Ollama) where setting effort to "medium"
   * prevents reasoning tokens from starving the JSON answer.
   */
  reasoning?: {
    enabled?: boolean
    effort?: "low" | "medium" | "high"
    budgetTokens?: number
  }
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

  // IMPORTANT: check OPENAI_COMPAT_ALIASES BEFORE the knownProviders shortcut.
  //
  // @cline/llms ≥ 0.0.47 registers "openai-compatible" as a concrete gateway
  // provider id (defaulting to https://api.openai.com/v1 when no base_url is
  // supplied). xvision's openai-compatible callers are self-hosted endpoints
  // (Ollama, vLLM, custom proxies) — each run requires its OWN base_url.
  // Letting the gateway silently default to api.openai.com would be a silent
  // misdirection regression, so we intercept the alias family FIRST and
  // require an explicit base_url, regardless of whether the underlying
  // gateway happens to have the id registered.
  if (OPENAI_COMPAT_ALIASES.has(normalizedProviderId)) {
    return providerIdForOpenAiCompatibleBaseUrl(baseUrl, knownProviders)
  }

  if (knownProviders.includes(normalizedProviderId)) return normalizedProviderId

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
  // The optional second arg forwards reasoning options (effort, budgetTokens)
  // to the gateway for CoT models that support native reasoning_effort (e.g.
  // deepseek-r1 via Ollama).
  let model: AgentModel
  try {
    model = gateway.createAgentModel(
      { providerId, modelId },
      opts.reasoning ? { reasoning: opts.reasoning } : undefined,
    ) as AgentModel
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
