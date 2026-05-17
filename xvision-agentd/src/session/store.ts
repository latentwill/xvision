import type { Agent } from "@cline/sdk"
import type { CumulativeUsage } from "./budget.js"
import { emptyUsage } from "./budget.js"

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
}

export interface SessionStore {
  create(run_id: string, config: StartRunConfig): Session
  get(run_id: string): Session | undefined
  attachAgent(run_id: string, agent: Agent): void
  /**
   * Add an observed step's usage to this run's cumulative totals. Called
   * by `session.step` after each successful or aborted step; budget caps
   * are then checked against the new totals.
   */
  addUsage(run_id: string, delta: Partial<CumulativeUsage>): CumulativeUsage
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
