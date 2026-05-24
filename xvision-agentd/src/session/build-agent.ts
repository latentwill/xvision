import { Agent, type AgentTool } from "@cline/sdk"
import { shimRegistryToTools } from "./tool-shim.js"
import { handleToolRegistryGet } from "../methods/tool-registry.js"
import { MOCK_PROVIDER_ID, buildMockModel } from "../testing/mock-provider.js"
import { wrapAgentModel } from "./model-wrapper.js"
import { buildProviderModel } from "./provider-model.js"
import { createFrameRecorder } from "./frame-recorder.js"
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

  // Build a FrameRecorder when recording is enabled. The same recorder instance
  // is shared between the model wrapper (records Request + AgentModelEvent frames)
  // and the tool shim (records ToolResult frames) so all frames for one step flow
  // through a single ordered sequence.
  const recorder = config.record === true ? createFrameRecorder() : undefined

  const tools: AgentTool[] = shimRegistryToTools(reg.tools, registryNames, {
    allowWrites: opts.allowWrites ?? false,
    ...(recorder ? { recorder } : {}),
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
    // mock-driven smoke runs. Pass the recorder when enabled.
    const wrapped = wrapAgentModel(buildMockModel(), {
      provider: MOCK_PROVIDER_ID,
      model: config.model_id,
      ...(recorder ? { recorder } : {}),
    })
    return new Agent({
      model: wrapped,
      systemPrompt: config.system_prompt,
      tools,
    })
  }

  // Real-provider path: build a wrappable AgentModel via the @cline/llms
  // DefaultGateway, then apply wrapAgentModel so the model-wrapper tap fires
  // for live providers — enabling frame recording and per-iteration
  // ModelCallStarted/Finished spans.
  //
  // If gateway construction fails (unknown provider, missing credentials),
  // we throw a clear error rather than silently falling back to the
  // un-wrappable internal path. Silently falling back would mean recording
  // is enabled but no frames are recorded, which would produce a corrupt
  // (incomplete) recording that cannot be replayed.
  const innerModel = buildProviderModel({
    providerId: config.provider_id,
    modelId: config.model_id,
    ...(config.api_key !== undefined ? { apiKey: config.api_key } : {}),
    ...(config.base_url !== undefined ? { baseUrl: config.base_url } : {}),
  })

  const wrapped = wrapAgentModel(innerModel, {
    provider: config.provider_id,
    model: config.model_id,
    ...(recorder ? { recorder } : {}),
  })

  return new Agent({
    model: wrapped,
    systemPrompt: config.system_prompt,
    tools,
  })
}
