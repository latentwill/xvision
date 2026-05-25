# Cline Provider Matrix — xvision provider IDs → Cline gateway IDs

> **§6 documentation** for the cline-live-followups track.
> Evidence gathered: 2026-05-25.

---

## Summary

The Stage 1 Cline runtime unification introduced the
`xvision-agentd` TypeScript sidecar (`xvision-agentd/src/`), which wraps the
`@cline/sdk` v0.0.41 `Agent` class. Provider identity flows from the Rust
engine to the sidecar as a plain `provider_id: String` field in the
`session.start_run` JSON-RPC call. The sidecar passes it verbatim as the
`providerId` argument to `new Agent({ providerId, modelId, apiKey, baseUrl })`.

**There is no static mapping table in xvision source code.**
The xvision `provider_id` IS the Cline gateway `providerId` — the two
namespaces are intentionally unified. The `@cline/llms` library inside the
SDK does maintain a small alias layer (described below), and an unmapped
`provider_id` falls through without an error at the xvision layer: the SDK
accepts an arbitrary string and errors only if the underlying provider
implementation cannot be resolved at model-construction time.

---

## xvision ProviderKind vs. Cline gateway provider IDs

xvision has two layers of provider identity that operators need to understand.

### Layer 1 — xvision `ProviderKind` (config.toml `kind` field)

Defined in `crates/xvision-core/src/config.rs:68`:

```
pub enum ProviderKind {
    Anthropic,      // serialised as "anthropic"
    OpenaiCompat,   // serialised as "openai-compat"
    LocalCandle,    // serialised as "local-candle"
}
```

This is the shape of the `[[providers]]` block in `config/default.toml`:

```toml
[[providers]]
name = "my-anthropic"   # operator-chosen name; becomes the xvision provider_id
kind = "anthropic"
base_url = "https://api.anthropic.com"
api_key_env = "ANTHROPIC_API_KEY"
```

`ProviderKind` determines which wire dispatcher the eval/pipeline engine uses
for direct LLM calls (`AnthropicDispatch` vs. `OpenaiCompatDispatch`). It does
NOT flow to the Cline sidecar.

### Layer 2 — xvision `provider_id` sent to `xvision-agentd`

When the Cline sidecar path is active (Wave 3 integration), the engine
sends `provider_id` via the `StartRunParams` struct
(`crates/xvision-agent-client/src/protocol.rs:88`). That `provider_id` is
the **operator-chosen `name`** from the `[[providers]]` config block, not the
`kind`. The engine resolves the `name` through
`crates/xvision-engine/src/api/settings/providers.rs` and
`crates/xvision-engine/src/eval/executor/` before dispatching.

---

## Provider Matrix

The table below documents which `provider_id` strings the sidecar forwards to
the Cline gateway, and what built-in ID the SDK maps them to.

| xvision `provider_id` (sent in `session.start_run`) | Cline `@cline/llms` built-in provider | Auth mechanism | Notes |
|---|---|---|---|
| `"anthropic"` | `BUILT_IN_PROVIDER.ANTHROPIC` = `"anthropic"` | `apiKey` header | Direct match. Production primary. |
| `"deepseek"` | `BUILT_IN_PROVIDER.DEEPSEEK` = `"deepseek"` | `apiKey` | Direct match. OpenAI-compat wire. |
| `"openai"` | `BUILT_IN_PROVIDER.OPENAI_NATIVE` = `"openai-native"` | `apiKey` | **Alias**: `@cline/llms` normalises `"openai"` → `"openai-native"` internally (see alias table below). |
| `"openrouter"` | `BUILT_IN_PROVIDER.OPENROUTER` = `"openrouter"` | `apiKey` | Direct match. |
| `"groq"` | `BUILT_IN_PROVIDER.GROQ` = `"groq"` | `apiKey` | Direct match. |
| `"together"` | `BUILT_IN_PROVIDER.TOGETHER` = `"together"` | `apiKey` | Direct match. Alias `"togetherai"` → `"together"` exists in SDK. |
| `"mistral"` | `BUILT_IN_PROVIDER.MISTRAL` = `"mistral"` | `apiKey` | Direct match. |
| `"gemini"` | `BUILT_IN_PROVIDER.GEMINI` = `"gemini"` | `apiKey` | Direct match. |
| `"ollama"` | `BUILT_IN_PROVIDER.OLLAMA` = `"ollama"` | none / `baseUrl` | Direct match. Usually local; `baseUrl` required. |
| `"xvision-mock"` | n/a — intercepted before reaching SDK | none | Test-only. `buildAgent` in the sidecar returns a `MockModel` and never calls `new Agent({ providerId })`. See `xvision-agentd/src/testing/mock-provider.ts:61`. |
| any other string | forwarded verbatim to `@cline/llms` | operator-supplied | Resolves only if `@cline/llms` v0.0.41 knows the ID. Unknown IDs error at model-construction time inside the SDK — not at the xvision layer. |

**Full `BUILT_IN_PROVIDER` enum** (all IDs the SDK accepts natively at version
0.0.41) is in `@cline/llms/dist/providers/ids.d.ts`. The current 45-entry set
includes: `anthropic`, `claude-code`, `cline`, `openai-native`, `openai-codex`,
`openai-codex-cli`, `opencode`, `bedrock`, `vertex`, `gemini`, `ollama`,
`lmstudio`, `deepseek`, `xai`, `together`, `fireworks`, `groq`, `cerebras`,
`sambanova`, `nebius`, `baseten`, `requesty`, `litellm`, `huggingface`,
`vercel-ai-gateway`, `v0`, `aihubmix`, `hicap`, `nousResearch`,
`huawei-cloud-maas`, `wandb`, `xiaomi`, `kilo`, `zai`, `zai-coding-plan`,
`qwen`, `qwen-code`, `doubao`, `mistral`, `moonshot`, `asksage`, `minimax`,
`dify`, `oca`, `sapaicore`, `openrouter`.

---

## @cline/llms built-in alias table (v0.0.41)

The SDK normalises a small set of legacy / alternate spellings before looking
up the provider handler. Source:
`@cline/llms/dist/providers.browser.js` (`normalizeProviderId` function):

| Incoming string | Resolved to |
|---|---|
| `"openai"` | `"openai-native"` |
| `"togetherai"` | `"together"` |
| `"sap-ai-core"` | `"sapaicore"` |

All other strings are left unchanged. Normalisation happens inside the SDK;
xvision sends the original `provider_id` string and does not pre-normalise.

---

## How unmapped / unknown providers are handled

1. xvision-agentd validates only that `provider_id` is a non-empty string
   (`xvision-agentd/src/methods/session.ts:123–124`). No allowlist check.
2. The string is forwarded verbatim to `new Agent({ providerId: config.provider_id, … })`
   (`xvision-agentd/src/session/build-agent.ts:47`).
3. If the `@cline/llms` layer cannot resolve the `providerId` to a built-in
   handler, the SDK throws at `agent.run()` / `agent.continue()` time. The
   throw propagates out of `handleSessionStep`, the sidecar emits an
   `event.error` notification, and the Rust caller surfaces it as a
   `StepResult { status: "failed", error: "<sdk error message>" }`.
4. There is no silent passthrough or fallback. The failure is immediate and
   observable in the xvision trace log under `xvision::agent_client`.

---

## Where the mapping lives in code

| Concern | File | Key symbol(s) |
|---|---|---|
| xvision `ProviderKind` enum (config layer) | `crates/xvision-core/src/config.rs:68` | `ProviderKind::{Anthropic,OpenaiCompat,LocalCandle}` |
| `StartRunParams.provider_id` sent to sidecar | `crates/xvision-agent-client/src/protocol.rs:88` | `StartRunParams` |
| Sidecar `provider_id` validation | `xvision-agentd/src/methods/session.ts:123` | `validateStartRun` |
| Sidecar `provider_id` → Cline `Agent` | `xvision-agentd/src/session/build-agent.ts:47` | `buildAgent` (real-provider path) |
| Mock provider interception | `xvision-agentd/src/testing/mock-provider.ts:61` | `MOCK_PROVIDER_ID = "xvision-mock"` |
| Cline built-in provider IDs (SDK) | `node_modules/@cline/llms/dist/providers/ids.d.ts` | `BUILT_IN_PROVIDER` enum |
| Cline provider alias normalisation (SDK) | `node_modules/@cline/llms/dist/providers.browser.js` | `normalizeProviderId` / alias map `C` |

---

## How to add a new provider mapping

No xvision code change is needed if the target provider is already in
`@cline/llms` v0.0.41's `BUILT_IN_PROVIDER` enum (see full list above):

1. Add a `[[providers]]` entry in the operator's `config/default.toml` with
   the provider's canonical `@cline/llms` ID as the `name` field (e.g.
   `name = "groq"`).
2. Set `api_key_env` to the env var holding the key.
3. The sidecar will forward `name` as `providerId` and the SDK will resolve it.

If the target provider is **not** in `BUILT_IN_PROVIDER` (a new provider that
`@cline/sdk` 0.0.41 doesn't know), the operator needs to either:
- Wait for an `@cline/sdk` version bump that adds the provider, then update
  `xvision-agentd/package.json`; or
- Register a custom handler via the `AgentRuntimeConfigWithModel` path
  (pass a pre-built `AgentModel` rather than `providerId`) — this is the
  same mechanism the mock provider uses and requires sidecar code changes
  in `build-agent.ts`.

---

## Design note — no static map in xvision source

The current design deliberately avoids a duplicated mapping table. The
`provider_id` that the Rust engine writes into `StartRunParams` is the same
string that `@cline/llms` expects as `providerId`. Operators configure their
provider names to match the SDK's canonical IDs; the sidecar acts as a
transparent relay. The only exception is the `"xvision-mock"` sentinel, which
is intercepted before the SDK is invoked.

This relay design means the matrix above tracks a third-party enum
(`BUILT_IN_PROVIDER`). When `@cline/sdk` bumps its version, update the
full-list section above and re-check the alias table.
