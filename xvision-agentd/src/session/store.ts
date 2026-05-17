import type { Agent } from "@cline/sdk"

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
}

export interface SessionStore {
  create(run_id: string, config: StartRunConfig): Session
  get(run_id: string): Session | undefined
  attachAgent(run_id: string, agent: Agent): void
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
      }
      sessions.set(run_id, session)
      return session
    },
    get(run_id) {
      return sessions.get(run_id)
    },
    attachAgent(run_id, agent) {
      const s = sessions.get(run_id)
      if (!s) throw new Error(`session not found: ${run_id}`)
      s.agent = agent
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
