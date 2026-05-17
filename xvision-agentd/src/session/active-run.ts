/**
 * Module-local registry of "which run am I currently inside" so the
 * tool-shim (which the Cline Agent calls) can stamp event notifications
 * with the right `run_id`. Set by session.step before invoking the
 * agent; cleared when the agent returns.
 *
 * Why module-local: tool execute() runs synchronously inside the agent
 * loop on the same call stack as session.step, but is not threaded
 * through agent arguments. Passing context as a global is the cheapest
 * fix without rewriting the tool-shim's createTool contract.
 *
 * Concurrency: the sidecar serializes step requests per run_id; multiple
 * concurrent runs across different sessions would race here. v1 the
 * dashboard / engine drives one session.step at a time per sidecar
 * process — this matches the current Cline runtime which is not
 * thread-safe across concurrent agent.run() calls anyway. Multi-run
 * concurrency is a Cline-migration follow-up.
 */

let currentRunId: string | undefined
let currentProvider: string | undefined
let currentModel: string | undefined

export function setActiveRun(run_id: string, provider_id: string, model_id: string): void {
  currentRunId = run_id
  currentProvider = provider_id
  currentModel = model_id
}

export function clearActiveRun(): void {
  currentRunId = undefined
  currentProvider = undefined
  currentModel = undefined
}

export function activeRunId(): string | undefined {
  return currentRunId
}

export function activeProvider(): string | undefined {
  return currentProvider
}

export function activeModel(): string | undefined {
  return currentModel
}
