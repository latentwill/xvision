import { registerMethod } from "./index.js"
import {
  getDefaultStore,
  type SessionStore,
  type StartRunConfig,
  type BudgetLimits,
} from "../session/store.js"
import type { TrajectoryFrame } from "../session/frame-types.js"
import { handleToolRegistryGet } from "./tool-registry.js"
import { buildAgent } from "../session/build-agent.js"
import {
  armWallTimer,
  checkTokenCapsAfterStep,
  checkTokenCapsBeforeStep,
  remainingWallMs,
  type BudgetAbortReason,
  type WallTimer,
} from "../session/budget.js"
import {
  emitRunStarted,
  emitRunFinished,
  emitError,
} from "../session/emit.js"
import {
  setActiveRun,
  clearActiveRun,
} from "../session/active-run.js"
import { SUBMIT_DECISION_TOOL } from "../session/submit-decision.js"

let store: SessionStore = getDefaultStore()

// Test-only — lets vitest swap in an isolated store.
export function __setStoreForTesting(s: SessionStore): void {
  store = s
}

interface BudgetTimerOverrides {
  schedule?: typeof setTimeout
  cancel?: typeof clearTimeout
}

let timerOverrides: BudgetTimerOverrides = {}

/**
 * Test-only — inject a timer scheduler so budget enforcement is
 * deterministic. Production code uses `setTimeout`/`clearTimeout`.
 * The wall-clock *reference* time comes from `store.now()`, not from
 * here, so the same clock that stamped `created_at_ms` also drives the
 * elapsed-time calculation.
 */
export function __setBudgetClockForTesting(overrides: BudgetTimerOverrides): void {
  timerOverrides = overrides
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
  decision_schema?: unknown
  decision_context?: unknown
  /** Optional — enables trajectory frame recording for this run. */
  record?: unknown
  /** Optional — slot role stamped on recorded trajectory frames. */
  slot_role?: unknown
  /** Optional — reasoning effort hint for CoT models ("low"|"medium"|"high"|"none"). */
  reasoning_effort?: unknown
}

interface StartRunResult {
  run_id: string
  started_at_ms: number
}

interface EndRunParams {
  run_id?: unknown
  /**
   * Terminal status for the run. Defaults to `"completed"` when omitted.
   * Pass `"cancelled"` when the run was budget/wall-aborted; `"failed"` for
   * unrecoverable errors. Must be one of the strings `parse_run_status` on
   * the Rust side recognises: `"completed"`, `"failed"`, `"cancelled"`.
   */
  status?: unknown
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
    // submit_decision is a built-in lifecycle tool, not registry-backed.
    if (name === SUBMIT_DECISION_TOOL) continue
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
    ...(config.record === true ? { trajectory_mode: "record" as const } : {}),
  })
  return { run_id: s.run_id, started_at_ms: s.created_at_ms }
}

export function handleSessionEndRun(raw: unknown): EndRunResult {
  const p = (raw ?? {}) as EndRunParams
  if (typeof p.run_id !== "string" || p.run_id.length === 0)
    throw new TypeError("params.run_id must be a non-empty string")

  // Validate the optional status field.
  const VALID_TERMINAL_STATUSES = ["completed", "failed", "cancelled"] as const
  type TerminalStatus = typeof VALID_TERMINAL_STATUSES[number]
  let terminalStatus: TerminalStatus = "completed"
  if (p.status !== undefined) {
    if (!VALID_TERMINAL_STATUSES.includes(p.status as TerminalStatus))
      throw new TypeError(
        `params.status must be one of: ${VALID_TERMINAL_STATUSES.join(", ")} (got ${JSON.stringify(p.status)})`,
      )
    terminalStatus = p.status as TerminalStatus
  }

  // Guard: if the catch path in session.step already emitted a terminal
  // run_finished for this run, skip the emit here to avoid double-emission.
  const alreadyEmitted = store.isRunFinishedEmitted(p.run_id)
  const ended = store.end(p.run_id)
  if (ended && !alreadyEmitted) {
    emitRunFinished({
      run_id: p.run_id,
      status: terminalStatus,
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
  // `submit_decision` requires a non-array object `decision_schema` describing
  // the structured decision the agent must submit.
  const wantsSubmitDecision = (p.allowed_tools as string[]).includes(SUBMIT_DECISION_TOOL)
  const decisionSchemaOk =
    typeof p.decision_schema === "object" &&
    p.decision_schema !== null &&
    !Array.isArray(p.decision_schema)
  if (wantsSubmitDecision && !decisionSchemaOk)
    throw new TypeError(
      "params.decision_schema must be a non-array object when allowed_tools includes submit_decision",
    )
  const decisionContextOk =
    typeof p.decision_context === "object" &&
    p.decision_context !== null &&
    !Array.isArray(p.decision_context)
  if (p.decision_context !== undefined && !decisionContextOk)
    throw new TypeError("params.decision_context must be a non-array object when present")
  if (p.api_key !== undefined && typeof p.api_key !== "string")
    throw new TypeError("params.api_key must be a string when present")
  if (p.base_url !== undefined && typeof p.base_url !== "string")
    throw new TypeError("params.base_url must be a string when present")
  if (p.record !== undefined && typeof p.record !== "boolean")
    throw new TypeError("params.record must be a boolean when present")
  if (p.slot_role !== undefined && (typeof p.slot_role !== "string" || p.slot_role.length === 0))
    throw new TypeError("params.slot_role must be a non-empty string when present")
  const VALID_REASONING_EFFORTS = ["low", "medium", "high", "none"] as const
  if (p.reasoning_effort !== undefined) {
    if (!VALID_REASONING_EFFORTS.includes(p.reasoning_effort as typeof VALID_REASONING_EFFORTS[number]))
      throw new TypeError(
        `params.reasoning_effort must be one of: ${VALID_REASONING_EFFORTS.join(", ")} when present (got ${JSON.stringify(p.reasoning_effort)})`,
      )
  }
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
    ...(decisionSchemaOk ? { decision_schema: p.decision_schema as Record<string, unknown> } : {}),
    ...(decisionContextOk ? { decision_context: p.decision_context as Record<string, unknown> } : {}),
    ...(typeof p.record === "boolean" ? { record: p.record } : {}),
    ...(typeof p.slot_role === "string" && p.slot_role.length > 0 ? { slot_role: p.slot_role } : {}),
    ...(typeof p.reasoning_effort === "string" ? { reasoning_effort: p.reasoning_effort } : {}),
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

// ---------------------------------------------------------------------------
// session.replay_load
// ---------------------------------------------------------------------------

interface ReplayLoadParams {
  run_id?: unknown
  frames?: unknown
}

interface ReplayLoadResult {
  loaded: number
}

/**
 * Load recorded TrajectoryFrames onto a session so the next `session.step`
 * runs the agent against a replay model built from those frames (zero network).
 *
 * Contract (SHARED RPC contract — Rust client side must match exactly):
 *   method:  "session.replay_load"
 *   params:  { run_id: string, frames: TrajectoryFrame[] }
 *   result:  { loaded: number }
 *
 * After replay_load, `session.step` will call buildAgent with the loaded frames
 * as `replayFrames`, bypassing any live provider. The Agent re-runs its full
 * control-flow loop against the replay model.
 */
export function handleSessionReplayLoad(raw: unknown): ReplayLoadResult {
  const p = (raw ?? {}) as ReplayLoadParams
  if (typeof p.run_id !== "string" || p.run_id.length === 0)
    throw new TypeError("params.run_id must be a non-empty string")
  if (!Array.isArray(p.frames))
    throw new TypeError("params.frames must be an array of TrajectoryFrame objects")

  // Validate each frame has at minimum a `kind` string field; full structural
  // validation is left to the replay model (it ignores unknown fields).
  const frames = p.frames as unknown[]
  for (let i = 0; i < frames.length; i++) {
    const f = frames[i]
    if (typeof f !== "object" || f === null || !("kind" in f) || typeof (f as Record<string, unknown>)["kind"] !== "string") {
      throw new TypeError(`params.frames[${i}] must be an object with a string "kind" field`)
    }
  }

  const session = store.get(p.run_id)
  if (!session) throw new Error(`session not found: ${p.run_id}`)

  // Store frames on the session. If an agent is already attached (e.g. from a
  // prior step), null it out so the next step rebuilds with the replay model.
  store.setReplayFrames(p.run_id, frames as TrajectoryFrame[])
  // Force agent rebuild on next step so the replay model is installed fresh.
  if (session.agent !== null) {
    session.agent = null
  }

  return { loaded: frames.length }
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
  /** Decision JSON from `submit_decision`, or final JSON text for weak tool-callers. */
  decision_json?: string
}

/**
 * Synthesize an aborted-step result without invoking the agent. Used
 * when a token cap was already exhausted by a previous step and the next
 * step must short-circuit.
 */
function abortedStepResult(reason: BudgetAbortReason): StepResult {
  return {
    status: "aborted",
    output_text: "",
    iterations: 0,
    usage: {
      input_tokens: 0,
      output_tokens: 0,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
    },
    error: reason,
  }
}

function schemaPropertyNames(schema: Record<string, unknown> | undefined): string[] {
  const props = schema?.properties
  if (!props || typeof props !== "object" || Array.isArray(props)) return []
  return Object.keys(props as Record<string, unknown>)
}

function objectMatchesDecisionSchema(
  value: unknown,
  schema: Record<string, unknown> | undefined,
  depth = 0,
): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value) || depth > 2) return false
  const propertyNames = schemaPropertyNames(schema)
  const obj = value as Record<string, unknown>
  if (
    propertyNames.length === 0 ||
    propertyNames.some((name) => Object.prototype.hasOwnProperty.call(obj, name))
  ) {
    return true
  }

  for (const key of ["arguments", "parameters", "decision", "trader_output"]) {
    if (objectMatchesDecisionSchema(obj[key], schema, depth + 1)) return true
  }

  for (const key of ["output", "text", "content", "response"]) {
    const nested = obj[key]
    if (typeof nested !== "string") continue
    const span = extractFinalJsonObject(nested)
    if (!span) continue
    try {
      if (objectMatchesDecisionSchema(JSON.parse(span.text), schema, depth + 1)) return true
    } catch {
      // Ignore malformed wrapper strings.
    }
  }

  return false
}

interface JsonObjectSpan {
  text: string
  start: number
  end: number
}

function extractJsonObjectSpans(raw: string): JsonObjectSpan[] {
  const objects: JsonObjectSpan[] = []
  let cursor = 0

  while (cursor < raw.length) {
    const start = raw.indexOf("{", cursor)
    if (start === -1) break

    let depth = 0
    let inString = false
    let escaped = false
    let end = -1

    for (let i = start; i < raw.length; i++) {
      const c = raw[i]
      if (inString) {
        if (escaped) {
          escaped = false
        } else if (c === "\\") {
          escaped = true
        } else if (c === "\"") {
          inString = false
        }
        continue
      }

      if (c === "\"") {
        inString = true
      } else if (c === "{") {
        depth += 1
      } else if (c === "}") {
        depth -= 1
        if (depth === 0) {
          end = i + 1
          break
        }
      }
    }

    if (end !== -1) {
      objects.push({ text: raw.slice(start, end), start, end })
      cursor = end
    } else {
      cursor = start + 1
    }
  }

  return objects
}

function finalJsonSuffixIsAllowed(raw: string, end: number): boolean {
  const suffix = raw.slice(end).trim()
  return suffix === "" || suffix === "```"
}

function finalJsonPrefixIsAllowed(raw: string, start: number): boolean {
  const lines = raw
    .slice(0, start)
    .split(/\r?\n/)
    .map((line) => line.trim().toLowerCase())
    .filter((line) => line.length > 0)
  const line = lines.at(-1) ?? ""
  return (
    line.includes("submit_decision") ||
    line.includes("submitdecision") ||
    (line.includes("final") && (line.includes("decision") || line.includes("answer") || line.includes("json")))
  )
}

function extractFinalJsonObject(raw: string): JsonObjectSpan | undefined {
  const spans = extractJsonObjectSpans(raw)
  const span = spans.at(-1)
  if (!span) return undefined
  if (!finalJsonSuffixIsAllowed(raw, span.end) && !finalJsonPrefixIsAllowed(raw, span.start)) {
    return undefined
  }
  return span
}

function decisionJsonFromOutputText(raw: string, schema: Record<string, unknown> | undefined): string | undefined {
  const candidate = extractFinalJsonObject(raw)
  if (!candidate) return undefined
  try {
    const value = JSON.parse(candidate.text)
    if (objectMatchesDecisionSchema(value, schema)) return candidate.text
  } catch {
    // The final JSON-looking block is malformed; do not promote older
    // schema-shaped examples from the reasoning text into a decision.
  }
  return undefined
}

export async function handleSessionStep(raw: unknown): Promise<StepResult> {
  const p = (raw ?? {}) as StepParams
  if (typeof p.run_id !== "string" || p.run_id.length === 0)
    throw new TypeError("params.run_id must be a non-empty string")
  if (typeof p.prompt !== "string")
    throw new TypeError("params.prompt must be a string")

  const session = store.get(p.run_id)
  if (!session) throw new Error(`session not found: ${p.run_id}`)

  const limits = session.config.budget_limits

  // Pre-step token caps: if a previous step already pushed cumulative
  // input/output tokens over the cap, short-circuit without invoking the
  // agent. Root-cause failure: caller asked for one more step past the
  // contracted budget, and the sidecar must say so explicitly.
  const preTokenReason = checkTokenCapsBeforeStep(session.usage, limits)
  if (preTokenReason) return abortedStepResult(preTokenReason)

  // Pre-step wall budget: if `now - started_at >= max_wall_ms` we cannot
  // even start the step. Surface a wall-budget abort immediately. The
  // clock comes from the store so injected `now` (tests) stays consistent
  // with whatever clock stamped `created_at_ms`.
  const remaining = remainingWallMs(session.created_at_ms, limits, store.now())
  if (remaining <= 0) return abortedStepResult("budget_wall_ms_exceeded")

  const runId = p.run_id

  // §2-C review nit #2: agent construction (`buildAgent`) is INSIDE the try
  // below, not before it. `buildAgent` can throw synchronously on a bad
  // provider config (unknown provider id, missing credentials — see
  // `build-agent.ts`'s "throw a clear error rather than silently falling
  // back" path). If that throw escaped before the try, the run row would be
  // left OPEN (no terminal `run_finished`), defeating Gap #2's "always close
  // the run row" guarantee. By building inside the try, a synchronous
  // buildAgent throw flows into the catch block and emits a terminal
  // `run_finished{status:"failed"}` before re-throwing.
  //
  // `timer`/`onTimerFire` are declared (nullable) before the try because the
  // `finally` must clean them up — and they are only armed AFTER the agent
  // is successfully built, so when buildAgent throws they are still null and
  // the finally skips them.
  let timer: WallTimer | null = null
  let onTimerFire: (() => void) | null = null

  try {
    // Lazy: build the Agent on first step (or after replay_load resets it).
    // Wire submit_decision's local capture to this run's store slot so the
    // decision lands on the StepResult. Pass replay frames when loaded. When
    // recording is enabled, retain the FrameRecorder so we can advance
    // `step_index` per step below.
    if (!session.agent) {
      const replayFrames = store.getReplayFrames(runId)
      const built = buildAgent(session.config, {
        captureDecision: (json) => store.setDecisionJson(runId, json),
        onRecorder: (recorder) => store.setRecorder(runId, recorder),
        ...(replayFrames ? { replayFrames } : {}),
      })
      store.attachAgent(runId, built)
    }
    const agent = session.agent!

    // Advance the recording's step index for this `session.step` so frames
    // emitted during it land in their own (slot_role, step_index) group on
    // the Rust side. No-op when recording is disabled.
    store.getRecorder(runId)?.beginStep()

    // Both the mock-provider and real-provider paths are now wrapped by
    // `wrapAgentModel` in `build-agent.ts`, which emits per-`stream()`
    // ModelCallStarted + ModelCallFinished pairs. The aggregate span that
    // used to live here for the real-provider path has been removed now that
    // `buildProviderModel` + `wrapAgentModel` handle real providers too.
    setActiveRun(runId, session.config.provider_id, session.config.model_id)

    // Arm the wall-clock timer. When it fires we call `agent.abort()`,
    // which causes the in-flight `agent.run` / `agent.continue` to resolve
    // with `status: "aborted"` (see `AgentRunStatus` in @cline/shared).
    timer = armWallTimer(remaining, {
      ...(timerOverrides.schedule ? { schedule: timerOverrides.schedule } : {}),
      ...(timerOverrides.cancel ? { cancel: timerOverrides.cancel } : {}),
    })
    onTimerFire = (): void => {
      // `agent.abort` is idempotent; safe to call from the timer callback.
      try {
        agent.abort(new Error("budget_wall_ms_exceeded"))
      } catch {
        // Best-effort: if the SDK throws synchronously here, the awaited
        // promise will still settle below and we'll classify the result.
      }
    }
    timer.signal.addEventListener("abort", onTimerFire, { once: true })

    const result = agent.hasRun
      ? await agent.continue(p.prompt)
      : await agent.run(p.prompt)

    // Update cumulative usage and check post-step token caps.
    const cumulative = store.addUsage(p.run_id, {
      input_tokens: result.usage.inputTokens,
      output_tokens: result.usage.outputTokens,
    })

    // Classify the terminal status:
    //   1. SDK said `aborted` *and* our timer fired -> wall-budget exhaustion.
    //   2. Cumulative tokens now exceed a cap -> token exhaustion.
    //   3. SDK said `aborted` for an unrelated reason -> pass through.
    //   4. Otherwise -> pass the SDK's status through verbatim.
    let status: StepResult["status"] = result.status
    let errorMsg: string | undefined = result.error?.message
    if (result.status === "aborted" && timer?.fired()) {
      // `timer` is non-null here: it was armed earlier in this same try block
      // before `agent.run`/`continue` could resolve.
      errorMsg = "budget_wall_ms_exceeded"
    } else {
      const postTokenReason = checkTokenCapsAfterStep(cumulative, limits)
      if (postTokenReason) {
        // The step ran to completion or got aborted for another reason, but
        // its usage pushed us over the cap. Force the terminal status to
        // `aborted` so the caller stops sending more prompts. If the agent
        // already came back aborted, preserve that.
        status = "aborted"
        errorMsg = postTokenReason
      }
    }

    if (errorMsg) {
      emitError({
        run_id: runId,
        message: errorMsg,
        severity: "error",
      })
    }

    // Tool calls win. If a weak local model only emitted final JSON text,
    // promote that text to decision_json here so Rust sees a first-class
    // decision and does not need its output_text_json_scan recovery path.
    const decisionJson =
      store.getDecisionJson(runId) ??
      (session.config.allowed_tools.includes(SUBMIT_DECISION_TOOL)
        ? decisionJsonFromOutputText(result.outputText, session.config.decision_schema)
        : undefined)
    // exactOptionalPropertyTypes: omit total_cost / error / decision_json when undefined.
    return {
      status,
      output_text: result.outputText,
      iterations: result.iterations,
      usage: {
        input_tokens: result.usage.inputTokens,
        output_tokens: result.usage.outputTokens,
        cache_read_tokens: result.usage.cacheReadTokens ?? 0,
        cache_write_tokens: result.usage.cacheWriteTokens ?? 0,
        ...(typeof result.usage.totalCost === "number" ? { total_cost: result.usage.totalCost } : {}),
      },
      ...(errorMsg ? { error: errorMsg } : {}),
      ...(decisionJson ? { decision_json: decisionJson } : {}),
    }
  } catch (err) {
    const errMsg = err instanceof Error ? err.message : String(err)
    // Emit the error detail first so the Rust side has context.
    emitError({
      run_id: runId,
      message: errMsg,
      severity: "error",
    })
    // Always close the observability stream with a terminal run_finished so
    // the recorder's run row is never left open. We emit `failed` here because
    // an uncaught SDK exception is an unrecoverable error (distinct from a
    // budget abort which the Rust caller signals via end_run{status:cancelled}).
    // Latch the guard before emitting so end_run will not double-emit.
    store.markRunFinishedEmitted(runId)
    emitRunFinished({
      run_id: runId,
      status: "failed",
      finished_at_ms: Date.now(),
      error: errMsg,
    })
    throw err
  } finally {
    // `timer`/`onTimerFire` may be null if `buildAgent` (or an earlier
    // statement in the try) threw before the timer was armed — guard so the
    // cleanup itself never throws. `clearActiveRun` is always safe: it is a
    // no-op when no run is active (buildAgent throw → setActiveRun never ran).
    if (timer && onTimerFire) {
      timer.signal.removeEventListener("abort", onTimerFire)
    }
    timer?.clear()
    clearActiveRun()
  }
}

registerMethod("session.start_run", (p) => handleSessionStartRun(p))
registerMethod("session.end_run", (p) => handleSessionEndRun(p))
registerMethod("session.step", (p) => handleSessionStep(p))
registerMethod("session.replay_load", (p) => handleSessionReplayLoad(p))
