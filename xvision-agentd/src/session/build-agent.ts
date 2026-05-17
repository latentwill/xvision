import { Agent } from "@cline/sdk"
import { shimRegistryToTools } from "./tool-shim.js"
import { handleToolRegistryGet } from "../methods/tool-registry.js"
import { MOCK_PROVIDER_ID, buildMockModel } from "../testing/mock-provider.js"
import type { StartRunConfig } from "./store.js"

export function buildAgent(config: StartRunConfig, opts: { allowWrites?: boolean } = {}): Agent {
  const reg = handleToolRegistryGet()
  const tools = shimRegistryToTools(reg.tools, config.allowed_tools, {
    allowWrites: opts.allowWrites ?? false,
  })

  if (config.provider_id === MOCK_PROVIDER_ID) {
    return new Agent({
      model: buildMockModel(),
      systemPrompt: config.system_prompt,
      tools,
    })
  }

  return new Agent({
    providerId: config.provider_id,
    modelId: config.model_id,
    apiKey: config.api_key,
    baseUrl: config.base_url,
    systemPrompt: config.system_prompt,
    tools,
  })
}
