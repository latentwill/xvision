export type MethodHandler = (params: unknown) => Promise<unknown> | unknown

const handlers: Map<string, MethodHandler> = new Map()

export function registerMethod(name: string, handler: MethodHandler): void {
  if (handlers.has(name)) {
    throw new Error(`method already registered: ${name}`)
  }
  handlers.set(name, handler)
}

export function getMethodHandler(name: string): MethodHandler | undefined {
  return handlers.get(name)
}

// Test-only — lets vitest's beforeEach reset state between cases.
export function resetMethodsForTesting(): void {
  handlers.clear()
}
