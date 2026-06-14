import { Agent, type AgentTool } from "@cline/sdk"
import { shimRegistryToTools } from "./tool-shim.js"

const CONFLICTING_JSON_RE =
  /output[^\n]*json[^\n]*only|strict\s+json|json\s+only|output\s+json/i

const CORRECTION_NOTE =
  "\n\nIMPORTANT: Ignore any earlier instructions to output raw JSON. You MUST call the submit_decision tool to submit your decision — outputting JSON text is not accepted and will fail the cycle."

const CORRECTION_MARKER = "You MUST call the submit_decision tool"

export function sanitizeSystemPrompt(prompt: string): string {
  if (prompt.includes(CORRECTION_MARKER)) return prompt
  return CONFLICTING_JSON_RE.test(prompt) ? prompt + CORRECTION_NOTE : prompt
}
import { handleToolRegistryGet } from "../methods/tool-registry.js"
import { MOCK_PROVIDER_ID, buildMockModel } from "../testing/mock-provider.js"
import { wrapAgentModel } from "./model-wrapper.js"
import { buildProviderModel } from "./provider-model.js"
import { createFrameRecorder, type FrameRecorder } from "./frame-recorder.js"
import { SUBMIT_DECISION_TOOL, buildSubmitDecisionTool } from "./submit-decision.js"
import { buildReplayModel } from "./replay-model.js"
import type { TrajectoryFrame } from "./frame-types.js"
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
  /**
   * When set, the agent is constructed with a buildReplayModel driven by these
   * frames instead of a live provider or mock. Zero network — pure in-memory
   * replay. Takes priority over provider_id / mock detection.
   */
  replayFrames?: TrajectoryFrame[]
  /**
   * Invoked with the FrameRecorder when recording is enabled (`config.record
   * === true`). Lets the caller (session.step) retain the recorder so it can
   * advance `step_index` via `recorder.beginStep()` per step. Not called when
   * recording is disabled.
   */
  onRecorder?: (recorder: FrameRecorder) => void
}

export function buildAgent(config: StartRunConfig, opts: BuildAgentOptions = {}): Agent {
  const systemPrompt = sanitizeSystemPrompt(config.system_prompt)
  const reg = handleToolRegistryGet()
  // `submit_decision` is a built-in lifecycle tool, not a registry-backed Rust
  // callback — exclude it from the registry shim (which would throw on an
  // unknown name) and append it separately so it captures locally.
  const registryNames = config.allowed_tools.filter((n) => n !== SUBMIT_DECISION_TOOL)

  // Build a FrameRecorder when recording is enabled. The same recorder instance
  // is shared between the model wrapper (records Request + AgentModelEvent frames)
  // and the tool shim (records ToolResult frames) so all frames for one step flow
  // through a single ordered sequence. The recorder stamps `slot_role` on every
  // emitted frame envelope so the Rust consumer keys frames to the matching
  // recording.
  const recorder =
    config.record === true
      ? createFrameRecorder(config.slot_role !== undefined ? { slotRole: config.slot_role } : {})
      : undefined
  if (recorder && opts.onRecorder) {
    opts.onRecorder(recorder)
  }

  const tools: AgentTool[] = shimRegistryToTools(reg.tools, registryNames, {
    allowWrites: opts.allowWrites ?? false,
    ...(recorder ? { recorder } : {}),
  })
  if (config.allowed_tools.includes(SUBMIT_DECISION_TOOL) && opts.captureDecision) {
    tools.push(
      buildSubmitDecisionTool(
        config.decision_schema ?? { type: "object", additionalProperties: true },
        opts.captureDecision,
        config.decision_context,
      ),
    )
  }

  // -----------------------------------------------------------------------
  // Replay branch: if replay frames are loaded, drive the agent from
  // the recording with zero network calls. Takes priority over the mock
  // and real-provider paths.
  // -----------------------------------------------------------------------
  if (opts.replayFrames && opts.replayFrames.length > 0) {
    const replayModel = buildReplayModel(opts.replayFrames)
    // Wrap with the observability tap so ModelCallStarted/Finished spans are
    // still emitted during replay. Pass the recorder if recording is also
    // enabled (unusual but safe).
    const wrapped = wrapAgentModel(replayModel, {
      provider: "xvision-replay",
      model: config.model_id,
      ...(recorder ? { recorder } : {}),
    })
    return new Agent({
      model: wrapped,
      systemPrompt,
      tools,
    })
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
      systemPrompt,
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
    ...(config.reasoning_effort !== undefined
      ? { reasoning: { effort: config.reasoning_effort as "low" | "medium" | "high" } }
      : {}),
  })

  const wrapped = wrapAgentModel(innerModel, {
    provider: config.provider_id,
    model: config.model_id,
    ...(recorder ? { recorder } : {}),
  })

  return new Agent({
    model: wrapped,
    systemPrompt,
    tools,
  })
}
