# Free Model Catalog

**Observed:** 2026-09-04 (UTC)

This is a dated snapshot of models advertised as free by the provider catalogs
checked on 2026-09-04. Free access is not necessarily unlimited: quotas,
rate limits, account access, regional availability, routing, and provider
rollouts can change.

## OpenCode Zen

The live OpenCode Zen catalog exposed these free model IDs:

- `deepseek-v4-flash-free`
- `muse-spark-1.3-contributor-free`
- `muse-spark-1.2-contributor-free`
- `mimo-v2.5-free`
- `ling-3.0-flash-fin-free`
- `nemotron-3-ultra-free`
- `nemotron-3.5-lightning-free`
- `laguna-s-2.1-free`

These are OpenCode Zen IDs. They are separate from similarly named OpenRouter
IDs and should not be assumed to have the same price or access policy.

## OpenRouter

The live OpenRouter catalog exposed these explicit `:free` model IDs:

- `inclusionai/ling-3.0-flash-fin:free`
- `dots-studio/dots-3-note-preview:free`
- `liquid/lfm-2.5-2.6b:free`
- `nvidia/nemotron-3.5-lightning:free`
- `thinkingmachines/inkling-small:free`
- `thinkingmachines/inkling:free`
- `poolside/laguna-s-2.1:free`
- `poolside/laguna-xs-2.1:free`
- `cohere/north-mini-code:free`
- `z-ai/glm-5.2:free`
- `nvidia/nemotron-3.5-content-safety:free`
- `nvidia/nemotron-3-ultra-550b-a55b:free`
- `minimax/minimax-m3:free`
- `nvidia/nemotron-3-nano-omni-30b-a3b-reasoning:free`
- `google/gemma-4-26b-a4b-it:free`
- `google/gemma-4-31b-it:free`
- `minimax/minimax-m2.7:free`
- `nvidia/nemotron-3-super-120b-a12b:free`

OpenRouter also exposed the dynamic free-pool alias:

- `openrouter/free`

### Muse 1.3 note

OpenRouter listed `meta/muse-spark-1.3` and
`meta/muse-spark-1.3-contributor`, but neither had a `:free` variant in this
snapshot. Their observed pricing was:

- `meta/muse-spark-1.3`: $1.25/M input, $4.25/M output
- `meta/muse-spark-1.3-contributor`: $0.10/M input, $0.20/M output

The `muse-spark-1.3-contributor-free` ID above belongs to OpenCode Zen, not
OpenRouter.

## Ollama / local models

These models were available through the configured Ollama endpoint. They have
no per-token API price, but do consume the endpoint host's compute and storage:

- `lfm2.5:8b`
- `hf.co/unsloth/Qwen3-4B-Instruct-2507-GGUF:UD-Q4_K_XL`
- `gemma4:26b-mlx`
- `llama3.2-vision:latest`
- `hf.co/unsloth/gemma-4-E4B-it-GGUF:Q4_K_M`
- `hf.co/SandLogicTechnologies/Fino1-8B-GGUF:Q5_K_M`
- `deepseek-r1:8b`
- `hf.co/prithivMLmods/VibeThinker-3B-GGUF:Q4_K_M`

## 9Router

9Router is a separate local router/aggregator, not OpenRouter. It was not
installed or configured in the xvn dev environment when this snapshot was
made. Its available models depend on which provider connections are added to
the 9Router instance.

The 9Router project documentation identifies these free or free-tier
connections:

- Kiro AI
- OpenCode Free
- Vertex AI (introductory Google Cloud credits, where applicable)

Those connections should be catalogued separately from OpenRouter and
OpenCode Zen when a 9Router instance is connected.

## Sources

- OpenCode Zen model endpoint: <https://opencode.ai/zen/v1/models>
- OpenRouter model catalog: <https://openrouter.ai/api/v1/models>
- OpenRouter endpoint metadata: <https://openrouter.ai/api/v1/models/meta/muse-spark-1.3/endpoints>
- OpenRouter Contributor endpoint metadata: <https://openrouter.ai/api/v1/models/meta/muse-spark-1.3-contributor/endpoints>
- 9Router project: <https://github.com/decolua/9router>
- 9Router website: <https://9router.com/>
