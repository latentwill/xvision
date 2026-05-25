/**
 * provider-model.ts — build a wrappable AgentModel for real providers via
 * the @cline/llms DefaultGateway.
 *
 * This is the "registration plumbing" deferred from Stage 1. It fills the gap
 * in build-agent.ts where the real-provider path previously let Cline's Agent
 * construct an internal AgentModel that `wrapAgentModel` could not intercept.
 *
 * Gateway flow:
 *   1. createGateway() — fresh gateway with all built-in provider registrations.
 *   2. configureProvider({ providerId, apiKey, baseUrl }) — supply credentials
 *      and optional base URL overrides for the specific provider.
 *   3. createAgentModel({ providerId, modelId }) — returns an AgentModel whose
 *      stream() method can be wrapped by model-wrapper.ts.
 *
 * Provider ID mapping:
 *   The xvision engine uses Cline's own provider IDs (from the LLM settings
 *   UI), which are passed through unchanged. The gateway's built-in provider
 *   registrations use those same IDs (confirmed: "anthropic", "openai-native",
 *   "openai-compatible", "deepseek", etc.) — so no translation layer is needed.
 *   If a provider_id is not known to the gateway (e.g. a custom openai-compat
 *   endpoint), the caller should pass `base_url` to route it through the
 *   "openai-compatible" family. This function throws clearly when the gateway
 *   cannot construct a model, so the caller can surface a meaningful error.
 *
 * A fresh gateway is created per Agent construction — there is no shared state.
 * This is intentional: each run has its own credentials/baseUrl and must not
 * share gateway config with concurrent runs.
 */

import type { AgentModel } from "./model-wrapper.js"

// Accessed via @cline/sdk's re-export of @cline/core which re-exports @cline/llms.
// The re-export path is: @cline/sdk → @cline/core → Llms namespace → @cline/llms.
// TypeScript tsconfig has skipLibCheck:true so structural typing is fine here.
//
// Note: we import via @cline/sdk (the only declared dependency) and access the
// Llms namespace. The @cline/llms types flow through correctly.
import { Llms } from "@cline/sdk"

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
  const providerId =
    opts.providerId === "openai-compat" ? "openai-compatible" : opts.providerId

  // Create a fresh gateway. Built-in providers are auto-registered.
  const gateway = Llms.createGateway()

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
  const knownProviders = gateway.listProviders().map((p: { id: string }) => p.id)
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
