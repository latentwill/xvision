import { registerMethod } from "./index.js"
import {
  getDefaultStore,
  type SessionStore,
  type StartRunConfig,
  type BudgetLimits,
} from "../session/store.js"
import { handleToolRegistryGet } from "./tool-registry.js"
import { buildAgent } from "../session/build-agent.js"
import {
  emitRunStarted,
  emitRunFinished,
  emitModelCallFinished,
  emitError,
  newSpanId,
} from "../session/emit.js"
import {
  setActiveRun,
  clearActiveRun,
} from "../session/active-run.js"

let store: SessionStore = getDefaultStore()

// Test-only — lets vitest swap in an isolated store.
export function __setStoreForTesting(s: SessionStore): void {
  store = s
}

interface StartRunParams {
  run_id?: unknown
  provider_id?: unknown
  model_id?: unknown
  api_key?: unknown
  base_url?: unknown
  system_prompt?: unknown
  allowed_tools?: unknown
  budget_limits?: unknown
}

interface StartRunResult {
  run_id: string
  started_at_ms: number
}

interface EndRunParams {
  run_id?: unknown
}

interface EndRunResult {
  ended: boolean
}

export function handleSessionStartRun(raw: unknown): StartRunResult {
  const p = (raw ?? {}) as StartRunParams
  const config = validateStartRun(p)
  // Verify every allowed tool exists in the registry.
  const reg = handleToolRegistryGet()
  const known = new Set(reg.tools.map(t => t.name))
  for (const name of config.allowed_tools) {
    if (!known.has(name)) throw new TypeError(`unknown tool in allowed_tools: ${name}`)
  }
  const s = store.create(p.run_id as string, config)
  emitRunStarted({
    run_id: s.run_id,
    // The sidecar doesn't see an "objective" field — that's set by the
    // Rust caller before start_run. We pass the system_prompt as a
    // human-readable label; the Rust side may overwrite with a richer
    // objective in its translation layer.
    objective: config.system_prompt,
    started_at_ms: s.created_at_ms,
    provider_id: config.provider_id,
    model_id: config.model_id,
  })
  return { run_id: s.run_id, started_at_ms: s.created_at_ms }
}

export function handleSessionEndRun(raw: unknown): EndRunResult {
  const p = (raw ?? {}) as EndRunParams
  if (typeof p.run_id !== "string" || p.run_id.length === 0)
    throw new TypeError("params.run_id must be a non-empty string")
  const ended = store.end(p.run_id)
  if (ended) {
    emitRunFinished({
      run_id: p.run_id,
      status: "completed",
      finished_at_ms: Date.now(),
    })
  }
  return { ended }
}

function validateStartRun(p: StartRunParams): StartRunConfig {
  if (typeof p.run_id !== "string" || p.run_id.length === 0)
    throw new TypeError("params.run_id must be a non-empty string")
  if (typeof p.provider_id !== "string" || p.provider_id.length === 0)
    throw new TypeError("params.provider_id must be a non-empty string")
  if (typeof p.model_id !== "string" || p.model_id.length === 0)
    throw new TypeError("params.model_id must be a non-empty string")
  if (typeof p.system_prompt !== "string")
    throw new TypeError("params.system_prompt must be a string")
  if (!Array.isArray(p.allowed_tools) || p.allowed_tools.length === 0)
    throw new TypeError("params.allowed_tools must be a non-empty array of strings")
  for (const t of p.allowed_tools) {
    if (typeof t !== "string") throw new TypeError("allowed_tools entries must be strings")
  }
  if (p.api_key !== undefined && typeof p.api_key !== "string")
    throw new TypeError("params.api_key must be a string when present")
  if (p.base_url !== undefined && typeof p.base_url !== "string")
    throw new TypeError("params.base_url must be a string when present")
  const limits = validateBudget(p.budget_limits)
  // exactOptionalPropertyTypes: spread the optional fields only when present.
  return {
    provider_id: p.provider_id,
    model_id: p.model_id,
    ...(typeof p.api_key === "string" ? { api_key: p.api_key } : {}),
    ...(typeof p.base_url === "string" ? { base_url: p.base_url } : {}),
    system_prompt: p.system_prompt,
    allowed_tools: p.allowed_tools as string[],
    budget_limits: limits,
  }
}

function validateBudget(raw: unknown): BudgetLimits {
  if (typeof raw !== "object" || raw === null) throw new TypeError("params.budget_limits must be an object")
  const b = raw as Record<string, unknown>
  for (const k of ["max_input_tokens", "max_output_tokens", "max_wall_ms"]) {
    const v = b[k]
    if (typeof v !== "number" || !Number.isInteger(v) || v <= 0)
      throw new TypeError(`budget_limits.${k} must be a positive integer`)
  }
  return {
    max_input_tokens: b.max_input_tokens as number,
    max_output_tokens: b.max_output_tokens as number,
    max_wall_ms: b.max_wall_ms as number,
  }
}

interface StepParams {
  run_id?: unknown
  prompt?: unknown
}

interface StepResult {
  status: "completed" | "aborted" | "failed"
  output_text: string
  iterations: number
  usage: {
    input_tokens: number
    output_tokens: number
    cache_read_tokens: number
    cache_write_tokens: number
    total_cost?: number
  }
  error?: string
}

export async function handleSessionStep(raw: unknown): Promise<StepResult> {
  const p = (raw ?? {}) as StepParams
  if (typeof p.run_id !== "string" || p.run_id.length === 0)
    throw new TypeError("params.run_id must be a non-empty string")
  if (typeof p.prompt !== "string")
    throw new TypeError("params.prompt must be a string")

  const session = store.get(p.run_id)
  if (!session) throw new Error(`session not found: ${p.run_id}`)

  // Lazy: build the Agent on first step.
  if (!session.agent) {
    const agent = buildAgent(session.config)
    store.attachAgent(p.run_id, agent)
  }
  const agent = session.agent!

  const runId = p.run_id
  const stepSpanId = newSpanId()
  setActiveRun(runId, session.config.provider_id, session.config.model_id)
  try {
    const result = agent.hasRun
      ? await agent.continue(p.prompt)
      : await agent.run(p.prompt)

    emitModelCallFinished({
      span_id: stepSpanId,
      run_id: runId,
      provider: session.config.provider_id,
      model: session.config.model_id,
      input_tokens: result.usage.inputTokens,
      output_tokens: result.usage.outputTokens,
      total_cost: typeof result.usage.totalCost === "number" ? result.usage.totalCost : undefined,
    })

    if (result.error) {
      emitError({
        run_id: runId,
        message: result.error.message,
        severity: "error",
      })
    }

    // exactOptionalPropertyTypes: omit total_cost / error when undefined.
    return {
      status: result.status,
      output_text: result.outputText,
      iterations: result.iterations,
      usage: {
        input_tokens: result.usage.inputTokens,
        output_tokens: result.usage.outputTokens,
        cache_read_tokens: result.usage.cacheReadTokens ?? 0,
        cache_write_tokens: result.usage.cacheWriteTokens ?? 0,
        ...(typeof result.usage.totalCost === "number" ? { total_cost: result.usage.totalCost } : {}),
      },
      ...(result.error ? { error: result.error.message } : {}),
    }
  } catch (err) {
    emitError({
      run_id: runId,
      message: err instanceof Error ? err.message : String(err),
      severity: "error",
    })
    throw err
  } finally {
    clearActiveRun()
  }
}

registerMethod("session.start_run", (p) => handleSessionStartRun(p))
registerMethod("session.end_run", (p) => handleSessionEndRun(p))
registerMethod("session.step", (p) => handleSessionStep(p))
