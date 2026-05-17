import { Agent } from "@cline/sdk"
import { shimRegistryToTools } from "./tool-shim.js"
import { handleToolRegistryGet } from "../methods/tool-registry.js"
import { MOCK_PROVIDER_ID, buildMockModel } from "../testing/mock-provider.js"
import { wrapAgentModel } from "./model-wrapper.js"
import type { StartRunConfig } from "./store.js"

export function buildAgent(config: StartRunConfig, opts: { allowWrites?: boolean } = {}): Agent {
  const reg = handleToolRegistryGet()
  const tools = shimRegistryToTools(reg.tools, config.allowed_tools, {
    allowWrites: opts.allowWrites ?? false,
  })

  if (config.provider_id === MOCK_PROVIDER_ID) {
    // Wrap the mock model with the observability tap so `text-delta`,
    // per-iteration `model_call_started`, and per-iteration
    // `model_call_finished` notifications get emitted in tests and
    // mock-driven smoke runs.
    const wrapped = wrapAgentModel(buildMockModel(), {
      provider: MOCK_PROVIDER_ID,
      model: config.model_id,
    })
    return new Agent({
      model: wrapped,
      systemPrompt: config.system_prompt,
      tools,
    })
  }

  // Real-provider path: Cline's `Agent` constructs an internal
  // `AgentModel` from `providerId` + `modelId` via its per-instance
  // `DefaultGateway` (see `mock-provider.ts` for the registry
  // discovery note). We cannot pre-build the model and pass it via
  // `model:` here without re-registering every provider handler on
  // our own `createGateway()` instance — that's a larger Cline
  // integration deferred to a follow-up.
  //
  // The wrapper code itself (`wrapAgentModel`) is provider-agnostic
  // and ready to use as soon as we have a pre-built model in hand;
  // the gap is purely the registration plumbing on the Cline side.
  // For now, real-provider runs emit a per-step aggregate
  // ModelCallStarted + ModelCallFinished pair from methods/session.ts
  // after `agent.run()` / `agent.continue()` returns. That preserves
  // production model-call coverage until the gateway model can be
  // wrapped directly.
  return new Agent({
    providerId: config.provider_id,
    modelId: config.model_id,
    apiKey: config.api_key,
    baseUrl: config.base_url,
    systemPrompt: config.system_prompt,
    tools,
  })
}
