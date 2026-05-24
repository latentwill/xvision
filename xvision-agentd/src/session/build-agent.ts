import { Agent, type AgentTool } from "@cline/sdk"
import { shimRegistryToTools } from "./tool-shim.js"
import { handleToolRegistryGet } from "../methods/tool-registry.js"
import { MOCK_PROVIDER_ID, buildMockModel } from "../testing/mock-provider.js"
import { wrapAgentModel } from "./model-wrapper.js"
import { SUBMIT_DECISION_TOOL, buildSubmitDecisionTool } from "./submit-decision.js"
import type { StartRunConfig } from "./store.js"

export interface BuildAgentOptions {
  allowWrites?: boolean
  /**
   * When the run's `allowed_tools` includes `submit_decision`, this callback
   * receives the JSON the agent submits. The decision is captured locally in
   * the sidecar (not routed to Rust via `callRust`) and the call completes the
   * run. See `submit-decision.ts`.
   */
  captureDecision?: (json: string) => void
}

export function buildAgent(config: StartRunConfig, opts: BuildAgentOptions = {}): Agent {
  const reg = handleToolRegistryGet()
  // `submit_decision` is a built-in lifecycle tool, not a registry-backed Rust
  // callback — exclude it from the registry shim (which would throw on an
  // unknown name) and append it separately so it captures locally.
  const registryNames = config.allowed_tools.filter((n) => n !== SUBMIT_DECISION_TOOL)
  const tools: AgentTool[] = shimRegistryToTools(reg.tools, registryNames, {
    allowWrites: opts.allowWrites ?? false,
  })
  if (config.allowed_tools.includes(SUBMIT_DECISION_TOOL) && opts.captureDecision) {
    tools.push(
      buildSubmitDecisionTool(
        config.decision_schema ?? { type: "object", additionalProperties: true },
        opts.captureDecision,
      ),
    )
  }

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

  // Real-provider path: Cline's `Agent` constructs an internal `AgentModel`
  // from `providerId` + `modelId`. Stage 2 replaces this with a pre-built,
  // wrappable model via the (confirmed public) `createGateway()` /
  // `DefaultGateway.createAgentModel(selection)` API in `@cline/llms`, so the
  // model-wrapper tap can record real-provider frames. Until then, real runs
  // emit a per-step aggregate ModelCall span from `methods/session.ts`.
  return new Agent({
    providerId: config.provider_id,
    modelId: config.model_id,
    apiKey: config.api_key,
    baseUrl: config.base_url,
    systemPrompt: config.system_prompt,
    tools,
  })
}
