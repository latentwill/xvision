import type { Agent } from "@cline/sdk"
import type { CumulativeUsage } from "./budget.js"
import { emptyUsage } from "./budget.js"
import type { TrajectoryFrame } from "./frame-types.js"
import type { FrameRecorder } from "./frame-recorder.js"

export interface BudgetLimits {
  max_input_tokens: number
  max_output_tokens: number
  max_wall_ms: number
}

export interface StartRunConfig {
  provider_id: string
  model_id: string
  api_key?: string
  base_url?: string
  system_prompt: string
  allowed_tools: string[]
  budget_limits: BudgetLimits
  /**
   * JSON schema the agent's `submit_decision` payload must match. Required by
   * the start_run validator whenever `allowed_tools` includes `submit_decision`;
   * used as the lifecycle tool's `inputSchema`.
   */
  decision_schema?: Record<string, unknown>
  /**
   * Runtime context that should be attached to the final decision span, such as
   * active positions and portfolio state observed before the prompt was sent.
   * The decision payload may still override these fields when it includes a
   * fresher snapshot.
   */
  decision_context?: Record<string, unknown>
  /**
   * When true, the model-wrapper tap records a `Request` frame + one frame per
   * `AgentModelEvent`, and tool-shim records a `ToolResult` frame for every
   * tool execution. Frames are emitted via `emitFrame` (non-droppable) so the
   * Rust side can persist them to the trajectory store.
   *
   * Opt-in: recording is disabled by default to avoid overhead on runs that
   * don't need replay fidelity (e.g. paper-trading, live runs where the caller
   * hasn't opted in). Set to true for backtests and any run requiring Stage 3
   * replay.
   */
  record?: boolean
  /**
   * The slot role this run records under (e.g. "trader"). Stamped on every
   * trajectory frame envelope as `slot_role` so the Rust consumer keys frames
   * to the matching recording. Free-form (slot names are user-defined per the
   * terminology lock). Only meaningful when `record` is true; defaults to
   * `"default"` in the recorder when omitted.
   */
  slot_role?: string
  /**
   * Optional native reasoning-effort request forwarded from the Rust engine.
   * When absent, `provider-model.ts` follows Cline SDK catalog defaults. When
   * present, `"none"` maps to Cline's `reasoning.enabled=false` shape; non-none
   * efforts are suppressed for local/generic providers that reject `thinking`.
   */
  reasoning_effort?: string
}

export interface Session {
  run_id: string
  config: StartRunConfig
  agent: Agent | null
  created_at_ms: number
  /**
   * Cumulative token usage across every step in this run. Updated by
   * `session.step` after each step the agent completes, then compared
   * against `config.budget_limits` to enforce the per-run token caps.
   */
  usage: CumulativeUsage
  /**
   * JSON the agent submitted via the `submit_decision` lifecycle tool, if any.
   * Captured locally by build-agent.ts's tool callback; surfaced on
   * `StepResult.decision_json` after the step completes.
   */
  decisionJson?: string
  /**
   * Recorded trajectory frames loaded for replay. When set, the next
   * `session.step` drives the agent with a buildReplayModel instead of
   * a live provider or mock. Set via `session.replay_load`.
   */
  replayFrames?: TrajectoryFrame[]
  /**
   * The FrameRecorder for this run, retained when recording is enabled so
   * `session.step` can advance `step_index` via `recorder.beginStep()` per
   * step. Undefined when `config.record` is not true.
   */
  recorder?: FrameRecorder
  /**
   * Set to true once a terminal `event.run_finished` has been emitted for this
   * run (e.g. from the error catch path in `session.step`). Guards against
   * double-emission: `session.end_run` checks this flag and skips the emit
   * when the run already has a terminal event on the wire.
   */
  runFinishedEmitted?: boolean
}

export interface SessionStore {
  create(run_id: string, config: StartRunConfig): Session
  get(run_id: string): Session | undefined
  attachAgent(run_id: string, agent: Agent): void
  /** Retain the FrameRecorder for this run (recording-enabled runs only). */
  setRecorder(run_id: string, recorder: FrameRecorder): void
  /** Read the FrameRecorder for this run, if recording is enabled. */
  getRecorder(run_id: string): FrameRecorder | undefined
  /**
   * Add an observed step's usage to this run's cumulative totals. Called
   * by `session.step` after each successful or aborted step; budget caps
   * are then checked against the new totals.
   */
  addUsage(run_id: string, delta: Partial<CumulativeUsage>): CumulativeUsage
  /** Store the JSON submitted via `submit_decision` for this run. */
  setDecisionJson(run_id: string, json: string): void
  /** Read the `submit_decision` JSON captured for this run, if any. */
  getDecisionJson(run_id: string): string | undefined
  /**
   * Store replay frames for this run. After calling this, the next
   * `session.step` will drive the agent with a replay model built from
   * these frames instead of a live provider.
   */
  setReplayFrames(run_id: string, frames: TrajectoryFrame[]): void
  /** Read the replay frames loaded for this run, if any. */
  getReplayFrames(run_id: string): TrajectoryFrame[] | undefined
  /**
   * Latch the `runFinishedEmitted` flag for this run. Called from
   * `session.step`'s error catch path after emitting `event.run_finished`
   * so that the subsequent `session.end_run` call does not double-emit the
   * terminal event. Safe to call on a run that has already been ended
   * (it no-ops when the session is not found).
   */
  markRunFinishedEmitted(run_id: string): void
  /**
   * Return true if a terminal `event.run_finished` has already been emitted
   * for this run (i.e. `markRunFinishedEmitted` was called).
   */
  isRunFinishedEmitted(run_id: string): boolean
  /**
   * Current monotonic clock for this store. Budget enforcement reads
   * this so the same clock that stamped `created_at_ms` also computes
   * elapsed wall-clock time — keeps tests deterministic when a fake
   * `now` is injected via `createStore({ now })`.
   */
  now(): number
  end(run_id: string): boolean
}

export interface StoreOptions {
  now?: () => number
}

export function createStore(opts: StoreOptions = {}): SessionStore {
  const now = opts.now ?? (() => Date.now())
  const sessions = new Map<string, Session>()

  return {
    create(run_id, config) {
      if (sessions.has(run_id)) {
        throw new Error(`session already exists: ${run_id}`)
      }
      const session: Session = {
        run_id,
        config,
        agent: null,
        created_at_ms: now(),
        usage: emptyUsage(),
      }
      sessions.set(run_id, session)
      return session
    },
    get(run_id) {
      return sessions.get(run_id)
    },
    // Throws rather than returning false: called only after the JSON-RPC
    // handler has already confirmed the session exists; a missing session
    // here is a programmer error, not a caller error.
    attachAgent(run_id, agent) {
      const s = sessions.get(run_id)
      if (!s) throw new Error(`session not found: ${run_id}`)
      s.agent = agent
    },
    setRecorder(run_id, recorder) {
      const s = sessions.get(run_id)
      if (!s) throw new Error(`session not found: ${run_id}`)
      s.recorder = recorder
    },
    getRecorder(run_id) {
      return sessions.get(run_id)?.recorder
    },
    addUsage(run_id, delta) {
      const s = sessions.get(run_id)
      if (!s) throw new Error(`session not found: ${run_id}`)
      if (typeof delta.input_tokens === "number") {
        s.usage.input_tokens += delta.input_tokens
      }
      if (typeof delta.output_tokens === "number") {
        s.usage.output_tokens += delta.output_tokens
      }
      return { ...s.usage }
    },
    setDecisionJson(run_id, json) {
      const s = sessions.get(run_id)
      if (!s) throw new Error(`session not found: ${run_id}`)
      s.decisionJson = json
    },
    getDecisionJson(run_id) {
      return sessions.get(run_id)?.decisionJson
    },
    setReplayFrames(run_id, frames) {
      const s = sessions.get(run_id)
      if (!s) throw new Error(`session not found: ${run_id}`)
      s.replayFrames = frames
    },
    getReplayFrames(run_id) {
      return sessions.get(run_id)?.replayFrames
    },
    markRunFinishedEmitted(run_id) {
      const s = sessions.get(run_id)
      if (s) s.runFinishedEmitted = true
    },
    isRunFinishedEmitted(run_id) {
      return sessions.get(run_id)?.runFinishedEmitted === true
    },
    now() {
      return now()
    },
    end(run_id) {
      return sessions.delete(run_id)
    },
  }
}

// Module-level singleton used by JSON-RPC handlers.
// Tests construct their own via createStore() to isolate state.
let _defaultStore: SessionStore | null = null

export function getDefaultStore(): SessionStore {
  if (!_defaultStore) _defaultStore = createStore()
  return _defaultStore
}

// Test-only reset hook.
export function resetDefaultStoreForTesting(): void {
  _defaultStore = null
}
