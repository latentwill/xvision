# Cline SDK Agent Replacement — Wave 2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Bring `@cline/sdk`'s `Agent` runtime into the `xvision-agentd` sidecar, expose a real session lifecycle (`session.start_run` / `session.step` / `session.end_run`) over JSON-RPC, wire the Wave-1 tool registry handshake to actual Cline custom tools that round-trip back to Rust, and prove the whole loop end-to-end with a deterministic mock provider so CI does not depend on live LLM calls.

**Architecture:** Wave 1 left a scaffolded sidecar with `runtime.health`, a `tool.registry.set` handshake, and a single-method `tool.invoke` smoke path that proxies back to Rust via the callback socket. Wave 2 replaces the smoke `tool.invoke` with a real session lifecycle. Sessions hold a lazily-instantiated Cline `Agent` per `run_id`; the agent's tool list is built from the registered tool descriptors (each tool shimmed via `createTool` to RPC back to Rust over the existing callback socket); a `registerProvider`-based mock plug-in supplies deterministic responses for CI integration tests.

**Tech Stack:** Node 22 + TypeScript + `@cline/sdk` ^latest stable + `vitest` (sidecar) · Rust 2021 + `tokio` + `serde_json` + existing `xvision-agent-client` crate.

**Licensing note:** Per direction on 2026-05-17, the licensing baseline (LICENSE/NOTICE/CONTRIBUTING/SECURITY/CODE_OF_CONDUCT files, `cargo-deny`, `license-checker`, CI license workflow — Wave 1 Task 2) remains a **deferred follow-up, not a gate** for this wave. F1–F4 in `docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md` are tracked separately. Wave 2 imports `@cline/sdk` directly.

---

## File Structure

### New (sidecar)

- `xvision-agentd/src/session/store.ts` — in-memory `Map<run_id, Session>` with create/get/end/list. Sessions hold the lazy `Agent`, the start_run config, and a `createdAt` timestamp.
- `xvision-agentd/src/session/build-agent.ts` — pure factory that takes a `StartRunParams` + the active tool descriptors and returns an `Agent` instance with the right provider config, system prompt, and the tool registry shimmed via `createTool`.
- `xvision-agentd/src/session/tool-shim.ts` — converts a `ToolDescriptor` to an `AgentTool` whose `execute` calls `callRust(name, input)` and returns the JSON result. Honors the side-effect policy from the descriptor (skip `external_write` unless explicitly allowed).
- `xvision-agentd/src/methods/session.ts` — three JSON-RPC handlers: `session.start_run`, `session.step`, `session.end_run`. Each calls the store + factory, with strict input validation.
- `xvision-agentd/test/session-store.test.ts` — store unit tests.
- `xvision-agentd/test/session-start-run.test.ts` — `session.start_run` validation + happy-path.
- `xvision-agentd/test/session-end-run.test.ts` — end_run + cleanup.
- `xvision-agentd/test/session-step.test.ts` — end-to-end step against the mock provider, exercising tool round-trip.
- `xvision-agentd/test/helpers/mock-provider.ts` — registers a `xvision-mock` provider via `@cline/llms`'s `registerProvider`, deterministic per-input outputs, scripted tool calls.

### Modified (sidecar)

- `xvision-agentd/src/version.ts` — read `@cline/sdk/package.json` version at startup so `runtime.health` reports the resolved SDK version instead of `"unbound"`.
- `xvision-agentd/src/methods/runtime-health.ts` — return the resolved `cline_sdk_version`.
- `xvision-agentd/src/transport/uds-server.ts` — register the three new session methods (side-effect import of `methods/session.js`).
- `xvision-agentd/src/index.ts` — no behavioral change; the session import is wired through `uds-server.ts`.
- `xvision-agentd/package.json` — add `@cline/sdk` runtime dependency, pinned. Update `cline_sdk_version` resolution.
- `xvision-agentd/test/version.test.ts` — update the test that currently asserts `"unbound"` to assert the actual resolved version string from `@cline/sdk/package.json`.

### New (Rust client)

- `crates/xvision-agent-client/tests/session_lifecycle.rs` — integration test that spawns the real sidecar, starts a run, calls step, verifies tool round-trip + final assistant text, ends the run. Gated by `XVISION_RUN_SIDECAR_TESTS=1` (consistent with the existing supervisor smoke test).

### Modified (Rust client)

- `crates/xvision-agent-client/src/protocol.rs` — add `StartRunParams`, `StartRunResult`, `StepParams`, `StepResult`, `EndRunParams`, `EndRunResult`, plus the `RunUsage` shape.
- `crates/xvision-agent-client/src/client.rs` — add `start_run`, `step`, `end_run` methods on `AgentClient`.
- `crates/xvision-agent-client/src/lib.rs` — re-export the new types.
- `crates/xvision-agent-client/src/tool_dispatch.rs` — no functional change; reuse Wave 1's `callRust` path unchanged.
- `crates/xvision-agent-client/tests/` — extend the mock-server fixture to handle the new methods for unit-level Rust tests.

### Out of scope for Wave 2

- Provider capability matrix in the protocol (Wave 3).
- `submit_decision` Cline custom tool wiring at the strategy level (Wave 3).
- Switching any real eval call site over (Wave 3).
- Observability convergence: piping `agent.subscribe()` events into the Rust event bus (Wave 4 — for now, Wave 2 records events into a per-session ring buffer that `session.step` returns alongside the result, but does not push to a Rust sink).
- MCP server config, skills runtime, snapshot/restore (Wave 4+).
- Licensing baseline (deferred follow-up — F1–F4).

---

## Task 1: Add `@cline/sdk` dependency + resolved version reporting

**Files:**
- Modify: `xvision-agentd/package.json`
- Modify: `xvision-agentd/src/version.ts`
- Modify: `xvision-agentd/src/methods/runtime-health.ts`
- Modify: `xvision-agentd/test/version.test.ts`
- Modify: `xvision-agentd/test/runtime-health.test.ts`

**Goal:** The sidecar runtime depends on `@cline/sdk`. `runtime.health` reports the real installed SDK version. Wave 1's version test that asserted `"unbound"` is updated to assert the resolved version. No `Agent` instantiation yet — just the import + version plumbing.

- [ ] **Step 1: Update the failing test for `runtime.health`'s sdk version**

Edit `xvision-agentd/test/runtime-health.test.ts`. Find the assertion that expects `cline_sdk_version: "unbound"` and change it to assert the value matches a semver pattern. The shape:

```ts
import { describe, it, expect } from "vitest"
import { handleRuntimeHealth } from "../src/methods/runtime-health.js"

describe("runtime.health", () => {
  it("reports a resolved @cline/sdk semver", async () => {
    const res = await handleRuntimeHealth() as { cline_sdk_version: string }
    expect(res.cline_sdk_version).toMatch(/^\d+\.\d+\.\d+/)
    expect(res.cline_sdk_version).not.toBe("unbound")
  })
})
```

Keep the other existing assertions (`protocol_version`, `sidecar_version`, `status: "ok"`).

- [ ] **Step 2: Run the test, expect fail**

Run: `pnpm --dir xvision-agentd test -- runtime-health`

Expected: FAIL — `received "unbound"`, expected a semver.

- [ ] **Step 3: Add `@cline/sdk` as a runtime dependency**

Run from the worktree root:

```bash
pnpm --dir xvision-agentd add @cline/sdk
pnpm --dir xvision-agentd install --frozen-lockfile
```

Verify `xvision-agentd/package.json` now lists `@cline/sdk` under `dependencies` with a concrete version (pin the resolved version, not a range — replace `^x.y.z` with `x.y.z`). Verify `xvision-agentd/pnpm-lock.yaml` updated.

- [ ] **Step 4: Resolve the SDK version at startup**

Replace the content of `xvision-agentd/src/version.ts`:

```ts
import { readFileSync } from "node:fs"
import { fileURLToPath } from "node:url"
import { dirname, resolve } from "node:path"

// JSON-RPC protocol version. Bumped manually. Wave 2 baseline.
export const PROTOCOL_VERSION = "0.1.0"

// Sidecar build version. Bumped manually.
export const SIDECAR_VERSION = "0.2.0"

// Resolved @cline/sdk version, read once at module load.
// Resolution strategy: locate the dependency's package.json relative to the
// resolved module entry. import.meta.resolve is async and not yet stable; we
// use require.resolve indirectly by reading the workspace-local copy.
function resolveClineSdkVersion(): string {
  try {
    const here = dirname(fileURLToPath(import.meta.url))
    // From dist/version.js (or src/version.ts in dev), node_modules is up at
    // xvision-agentd/node_modules/@cline/sdk/package.json.
    const pkgPath = resolve(here, "..", "node_modules", "@cline", "sdk", "package.json")
    const raw = readFileSync(pkgPath, "utf8")
    const pkg = JSON.parse(raw) as { version?: unknown }
    if (typeof pkg.version === "string" && /^\d+\.\d+\.\d+/.test(pkg.version)) {
      return pkg.version
    }
  } catch {
    // fall through
  }
  return "unknown"
}

export const CLINE_SDK_VERSION = resolveClineSdkVersion()
```

- [ ] **Step 5: Wire the resolved version into `runtime.health`**

Edit `xvision-agentd/src/methods/runtime-health.ts`. Replace the literal `"unbound"` with `CLINE_SDK_VERSION` imported from `../version.js`:

```ts
import { registerMethod } from "./index.js"
import { CLINE_SDK_VERSION, PROTOCOL_VERSION, SIDECAR_VERSION } from "../version.js"

interface RuntimeHealthResult {
  protocol_version: string
  sidecar_version: string
  cline_sdk_version: string
  status: "ok"
}

export function handleRuntimeHealth(): RuntimeHealthResult {
  return {
    protocol_version: PROTOCOL_VERSION,
    sidecar_version: SIDECAR_VERSION,
    cline_sdk_version: CLINE_SDK_VERSION,
    status: "ok",
  }
}

registerMethod("runtime.health", () => handleRuntimeHealth())
```

- [ ] **Step 6: Update the standalone `--version` test**

Edit `xvision-agentd/test/version.test.ts`. Wave 1's test spawned the built sidecar with `--version` and checked the JSON for `cline_sdk_version: "unbound"`. Update to assert the resolved version matches the same regex used in Step 1.

- [ ] **Step 7: Build and re-run the tests**

```bash
pnpm --dir xvision-agentd build
pnpm --dir xvision-agentd test
```

Expected: ALL PASS. The `runtime.health` test reports a real semver. The `--version` test reports the same.

- [ ] **Step 8: Smoke-test from the shell**

Run:

```bash
node xvision-agentd/dist/index.js --version
```

Expected stdout: a single JSON line like `{"protocol_version":"0.1.0","sidecar_version":"0.2.0","cline_sdk_version":"<actual semver>"}`. Confirm `cline_sdk_version` is not `"unbound"` and not `"unknown"`.

- [ ] **Step 9: Commit**

```bash
git add xvision-agentd/package.json xvision-agentd/pnpm-lock.yaml \
        xvision-agentd/src/version.ts xvision-agentd/src/methods/runtime-health.ts \
        xvision-agentd/test/version.test.ts xvision-agentd/test/runtime-health.test.ts
git commit -m "$(cat <<'EOF'
feat(agentd): import @cline/sdk and report resolved version in runtime.health

Wave 2 Task 1. Adds @cline/sdk as a runtime dependency and replaces the
"unbound" placeholder with the version read from the installed package.json
at module-load time. No Agent instantiation yet.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 2: Session store

**Files:**
- Create: `xvision-agentd/src/session/store.ts`
- Create: `xvision-agentd/test/session-store.test.ts`

**Goal:** A small in-memory store keyed by `run_id` that holds a `Session` record. The session record carries the start-run config, the lazy `Agent` (set on first step), the active tool descriptors, and timestamps. No JSON-RPC integration yet — pure data.

- [ ] **Step 1: Write the failing test**

Create `xvision-agentd/test/session-store.test.ts`:

```ts
import { describe, it, expect, beforeEach } from "vitest"
import { createStore, type Session, type StartRunConfig } from "../src/session/store.js"

const CONFIG: StartRunConfig = {
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "You are helpful.",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 30000 },
}

describe("session store", () => {
  let store: ReturnType<typeof createStore>

  beforeEach(() => {
    store = createStore({ now: () => 1_700_000_000_000 })
  })

  it("creates and retrieves a session", () => {
    const s = store.create("run-1", CONFIG)
    expect(s.run_id).toBe("run-1")
    expect(s.config).toEqual(CONFIG)
    expect(s.agent).toBeNull()
    expect(s.created_at_ms).toBe(1_700_000_000_000)
    expect(store.get("run-1")).toBe(s)
  })

  it("rejects duplicate run ids", () => {
    store.create("run-1", CONFIG)
    expect(() => store.create("run-1", CONFIG)).toThrow(/already exists/)
  })

  it("returns undefined for unknown runs", () => {
    expect(store.get("missing")).toBeUndefined()
  })

  it("ends a session and removes it", () => {
    store.create("run-1", CONFIG)
    expect(store.end("run-1")).toBe(true)
    expect(store.get("run-1")).toBeUndefined()
  })

  it("returns false when ending an unknown run", () => {
    expect(store.end("missing")).toBe(false)
  })

  it("attachAgent stores the lazy agent without replacing the session", () => {
    const s = store.create("run-1", CONFIG)
    const fakeAgent = { mock: true } as unknown as Session["agent"]
    store.attachAgent("run-1", fakeAgent)
    expect(store.get("run-1")?.agent).toBe(fakeAgent)
    expect(store.get("run-1")).toBe(s)
  })
})
```

- [ ] **Step 2: Run the test, expect fail**

```bash
pnpm --dir xvision-agentd test -- session-store
```

Expected: FAIL — `Cannot find module '../src/session/store.js'`.

- [ ] **Step 3: Implement the store**

Create `xvision-agentd/src/session/store.ts`:

```ts
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
```

- [ ] **Step 4: Run the test, expect pass**

```bash
pnpm --dir xvision-agentd test -- session-store
```

Expected: PASS — all six cases green.

- [ ] **Step 5: Commit**

```bash
git add xvision-agentd/src/session/store.ts xvision-agentd/test/session-store.test.ts
git commit -m "$(cat <<'EOF'
feat(agentd): in-memory session store keyed by run_id

Wave 2 Task 2. Pure data layer — no JSON-RPC wiring yet. Each session
holds the start-run config, a lazy Agent slot, and a created-at timestamp.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 3: Tool shim — `ToolDescriptor` → `AgentTool`

**Files:**
- Create: `xvision-agentd/src/session/tool-shim.ts`
- Create: `xvision-agentd/test/tool-shim.test.ts`

**Goal:** Convert the Wave 1 tool registry descriptors into `AgentTool` objects suitable for `new Agent({ tools: [...] })`. Each shimmed tool's `execute` callback proxies to Rust via the existing `callRust(name, input)` callback-socket path. `side_effect_level === "external_write"` tools are skipped unless `allowWrites` is true. Tool names are kept as-is (already `snake_case` from the registry validator).

- [ ] **Step 1: Write the failing test**

Create `xvision-agentd/test/tool-shim.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from "vitest"
import { shimRegistryToTools } from "../src/session/tool-shim.js"
import * as callbackClient from "../src/transport/callback-client.js"

const DESCRIPTORS = [
  {
    name: "echo",
    version: "1.0.0",
    description: "Returns its input unchanged.",
    input_schema: { type: "object", properties: { message: { type: "string" } }, required: ["message"] },
    output_schema: { type: "object" },
    timeout_ms: 5000,
    side_effect_level: "pure" as const,
    requires_approval: false,
  },
  {
    name: "write_file",
    version: "1.0.0",
    description: "Writes a file to disk.",
    input_schema: { type: "object" },
    output_schema: { type: "object" },
    timeout_ms: 5000,
    side_effect_level: "external_write" as const,
    requires_approval: false,
  },
]

describe("shimRegistryToTools", () => {
  beforeEach(() => {
    vi.restoreAllMocks()
  })

  it("returns only allow-listed tools", () => {
    const tools = shimRegistryToTools(DESCRIPTORS, ["echo"], { allowWrites: false })
    expect(tools.map(t => t.name)).toEqual(["echo"])
  })

  it("skips external_write tools when allowWrites is false", () => {
    const tools = shimRegistryToTools(DESCRIPTORS, ["echo", "write_file"], { allowWrites: false })
    expect(tools.map(t => t.name)).toEqual(["echo"])
  })

  it("includes external_write tools when allowWrites is true", () => {
    const tools = shimRegistryToTools(DESCRIPTORS, ["echo", "write_file"], { allowWrites: true })
    expect(tools.map(t => t.name).sort()).toEqual(["echo", "write_file"])
  })

  it("each tool's execute proxies to callRust", async () => {
    const spy = vi.spyOn(callbackClient, "callRust").mockResolvedValue({ echoed: "hi" })
    const [echo] = shimRegistryToTools(DESCRIPTORS, ["echo"], { allowWrites: false })
    const result = await echo.execute({ message: "hi" }, {
      agentId: "a", conversationId: "c", iteration: 1,
    })
    expect(spy).toHaveBeenCalledWith("echo", { message: "hi" })
    expect(result).toEqual({ echoed: "hi" })
  })

  it("returns a structured error instead of throwing on Rust-side failure", async () => {
    vi.spyOn(callbackClient, "callRust").mockRejectedValue(new Error("rust unreachable"))
    const [echo] = shimRegistryToTools(DESCRIPTORS, ["echo"], { allowWrites: false })
    const result = await echo.execute({ message: "hi" }, {
      agentId: "a", conversationId: "c", iteration: 1,
    }) as { error?: string }
    expect(result.error).toContain("rust unreachable")
  })

  it("rejects unknown allowed_tools", () => {
    expect(() => shimRegistryToTools(DESCRIPTORS, ["does_not_exist"], { allowWrites: false }))
      .toThrow(/unknown tool/)
  })
})
```

- [ ] **Step 2: Run, expect fail**

```bash
pnpm --dir xvision-agentd test -- tool-shim
```

Expected: FAIL — cannot find module.

- [ ] **Step 3: Implement the shim**

Create `xvision-agentd/src/session/tool-shim.ts`:

```ts
import { createTool, type AgentTool } from "@cline/sdk"
import { callRust } from "../transport/callback-client.js"

export interface ToolDescriptor {
  name: string
  version: string
  description: string
  input_schema: unknown
  output_schema: unknown
  timeout_ms: number
  side_effect_level: "pure" | "read_only" | "external_read" | "external_write"
  requires_approval: boolean
}

export interface ShimOptions {
  allowWrites: boolean
}

export function shimRegistryToTools(
  descriptors: readonly ToolDescriptor[],
  allowedNames: readonly string[],
  opts: ShimOptions,
): AgentTool[] {
  const byName = new Map(descriptors.map(d => [d.name, d]))
  const out: AgentTool[] = []
  for (const name of allowedNames) {
    const d = byName.get(name)
    if (!d) throw new Error(`unknown tool in allow-list: ${name}`)
    if (d.side_effect_level === "external_write" && !opts.allowWrites) continue
    out.push(buildTool(d))
  }
  return out
}

function buildTool(d: ToolDescriptor): AgentTool {
  return createTool({
    name: d.name,
    description: d.description,
    // Cast: ToolDescriptor.input_schema is `unknown` on the wire; the
    // Wave-1 registry validator already enforced object shape.
    inputSchema: d.input_schema as object,
    timeoutMs: d.timeout_ms,
    execute: async (input) => {
      try {
        return await callRust(d.name, input as Record<string, unknown>)
      } catch (err) {
        // Per Cline SDK rule: return errors as data, do not throw —
        // throwing counts as a "mistake" against the agent's mistake limit.
        return { error: err instanceof Error ? err.message : String(err) }
      }
    },
  })
}
```

- [ ] **Step 4: Run, expect pass**

```bash
pnpm --dir xvision-agentd test -- tool-shim
```

Expected: PASS — all six cases green.

- [ ] **Step 5: Commit**

```bash
git add xvision-agentd/src/session/tool-shim.ts xvision-agentd/test/tool-shim.test.ts
git commit -m "$(cat <<'EOF'
feat(agentd): tool-registry shim that builds Cline AgentTool[] for sessions

Wave 2 Task 3. Converts Wave-1 tool descriptors into @cline/sdk
createTool() shims whose execute() proxies to Rust via the callback
socket. external_write tools are filtered unless explicitly opted in.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 4: Mock provider for sidecar tests

**Files:**
- Create: `xvision-agentd/test/helpers/mock-provider.ts`
- Create: `xvision-agentd/test/helpers/mock-provider.test.ts`

**Goal:** A registerable `@cline/llms` provider called `xvision-mock` that emits deterministic outputs driven by scripts the test supplies via per-test inputs. Used by the session-step test (Task 6) and the Rust integration test (Task 10).

The provider script is a list of "turns." Each turn is either `{ text: string }` (assistant text + done) or `{ toolCall: { name: string; input: unknown } }` (request a tool call). The test sets the script before constructing the `Agent`.

- [ ] **Step 1: Write the failing test**

Create `xvision-agentd/test/helpers/mock-provider.test.ts`:

```ts
import { describe, it, expect, beforeEach } from "vitest"
import { Agent, createTool } from "@cline/sdk"
import { installMockProvider, setMockScript } from "./mock-provider.js"

describe("xvision-mock provider", () => {
  beforeEach(() => {
    installMockProvider()
  })

  it("emits assistant text and completes", async () => {
    setMockScript([{ text: "hello, world" }])
    const agent = new Agent({
      providerId: "xvision-mock",
      modelId: "mock-model",
      systemPrompt: "test",
      tools: [],
    })
    const result = await agent.run("ping")
    expect(result.status).toBe("completed")
    expect(result.outputText).toContain("hello, world")
  })

  it("scripts a tool call followed by a final text", async () => {
    const calls: Array<{ input: unknown }> = []
    const echoTool = createTool({
      name: "echo",
      description: "echoes",
      inputSchema: { type: "object", properties: { msg: { type: "string" } }, required: ["msg"] },
      execute: async (input) => {
        calls.push({ input })
        return { echoed: (input as { msg: string }).msg }
      },
    })

    setMockScript([
      { toolCall: { name: "echo", input: { msg: "hi" } } },
      { text: "done" },
    ])

    const agent = new Agent({
      providerId: "xvision-mock",
      modelId: "mock-model",
      systemPrompt: "test",
      tools: [echoTool],
    })

    const result = await agent.run("go")
    expect(result.status).toBe("completed")
    expect(calls).toEqual([{ input: { msg: "hi" } }])
    expect(result.outputText).toContain("done")
  })
})
```

- [ ] **Step 2: Run, expect fail**

```bash
pnpm --dir xvision-agentd test -- mock-provider
```

Expected: FAIL — cannot find module.

- [ ] **Step 3: Implement the mock provider**

The exact handler interface comes from `@cline/llms`'s `registerProvider` + `createHandler` exports. Build a handler that yields a streaming response shaped to satisfy `Agent.run()`: one assistant text block, or one tool-use block, depending on the next script entry.

Create `xvision-agentd/test/helpers/mock-provider.ts`:

```ts
import { registerProvider, createHandler } from "@cline/llms"

export type MockTurn =
  | { text: string }
  | { toolCall: { name: string; input: unknown } }

let script: MockTurn[] = []
let cursor = 0
let installed = false

export function installMockProvider(): void {
  if (installed) return
  installed = true
  registerProvider({
    id: "xvision-mock",
    name: "xvision mock provider",
    models: [{
      id: "mock-model",
      label: "Mock Model",
      contextWindow: 8000,
      inputPrice: 0,
      outputPrice: 0,
      supportsTools: true,
      supportsStreaming: true,
    }],
    handler: createHandler({
      async *stream(req) {
        // Pull next scripted turn. If the script is exhausted, emit empty
        // text so the agent finishes cleanly rather than hanging.
        const turn = script[cursor++] ?? { text: "" }

        if ("text" in turn) {
          yield { type: "text-start", id: "t1" }
          yield { type: "text-delta", id: "t1", text: turn.text }
          yield { type: "text-end", id: "t1" }
          yield { type: "finish", reason: "stop", usage: { inputTokens: 1, outputTokens: 1 } }
          return
        }

        const callId = `tc-${cursor}`
        yield { type: "tool-call", id: callId, toolName: turn.toolCall.name, input: turn.toolCall.input }
        yield { type: "finish", reason: "tool-use", usage: { inputTokens: 1, outputTokens: 1 } }
      },
    }),
  })
}

export function setMockScript(s: MockTurn[]): void {
  script = s
  cursor = 0
}

export function resetMockScript(): void {
  script = []
  cursor = 0
}
```

> **Verification note:** the exact event names (`text-start`, `text-delta`, `tool-call`, etc.) and the `createHandler` argument shape come from `@cline/llms`. Cross-check against the installed `@cline/llms/dist` types before running the test. If the field names differ (e.g. `event: "content_block_delta"`), update the yielded objects to match — the public contract is what `@cline/sdk`'s `Agent` consumes from a registered handler. The skill reference in `~/.claude/skills/cline-sdk/references/providers/REFERENCE.md` lines 182–217 lists `registerProvider` and `createHandler` as the canonical entry points.

- [ ] **Step 4: Run, expect pass**

```bash
pnpm --dir xvision-agentd test -- mock-provider
```

Expected: PASS — both cases green. If the handler shape was wrong, the test will surface the right field names via Cline's error messages; correct and re-run.

- [ ] **Step 5: Commit**

```bash
git add xvision-agentd/test/helpers/mock-provider.ts xvision-agentd/test/helpers/mock-provider.test.ts
git commit -m "$(cat <<'EOF'
test(agentd): xvision-mock @cline/llms provider for deterministic tests

Wave 2 Task 4. Test-only provider with a script-driven handler so sidecar
session tests and the Rust integration test can run without live API
keys.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 5: Implement `session.start_run` and `session.end_run`

**Files:**
- Create: `xvision-agentd/src/session/build-agent.ts`
- Create: `xvision-agentd/src/methods/session.ts`
- Modify: `xvision-agentd/src/transport/uds-server.ts`
- Create: `xvision-agentd/test/session-start-run.test.ts`
- Create: `xvision-agentd/test/session-end-run.test.ts`

**Goal:** JSON-RPC handlers for `session.start_run` and `session.end_run`. `start_run` validates the params, stores a session entry, but does **not** instantiate the `Agent` yet (lazy — Task 6 does that on first step). `end_run` aborts in-flight runs (none yet — placeholder) and removes the session.

- [ ] **Step 1: Failing test for `session.start_run`**

Create `xvision-agentd/test/session-start-run.test.ts`:

```ts
import { describe, it, expect, beforeEach } from "vitest"
import { handleSessionStartRun, handleSessionEndRun, __setStoreForTesting } from "../src/methods/session.js"
import { createStore } from "../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../src/methods/tool-registry.js"

const TOOL_DESC = {
  name: "echo",
  version: "1.0.0",
  description: "echoes",
  input_schema: { type: "object" },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "pure",
  requires_approval: false,
}

const VALID_PARAMS = {
  run_id: "run-1",
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "you are helpful",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 30000 },
}

describe("session.start_run", () => {
  beforeEach(() => {
    resetRegistry()
    handleToolRegistrySet({ tools: [TOOL_DESC] })
    __setStoreForTesting(createStore({ now: () => 100 }))
  })

  it("returns run_id on success", () => {
    const r = handleSessionStartRun(VALID_PARAMS)
    expect(r).toEqual({ run_id: "run-1", started_at_ms: 100 })
  })

  it("rejects missing run_id", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, run_id: undefined })).toThrow(TypeError)
  })

  it("rejects missing provider_id", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, provider_id: undefined })).toThrow(TypeError)
  })

  it("rejects empty allowed_tools", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, allowed_tools: [] })).toThrow(TypeError)
  })

  it("rejects a tool name not in the registry", () => {
    expect(() => handleSessionStartRun({ ...VALID_PARAMS, allowed_tools: ["not_registered"] }))
      .toThrow(/unknown tool/)
  })

  it("rejects duplicate run_id", () => {
    handleSessionStartRun(VALID_PARAMS)
    expect(() => handleSessionStartRun(VALID_PARAMS)).toThrow(/already exists/)
  })
})

describe("session.end_run", () => {
  beforeEach(() => {
    resetRegistry()
    handleToolRegistrySet({ tools: [TOOL_DESC] })
    __setStoreForTesting(createStore({ now: () => 100 }))
  })

  it("ends an existing session", () => {
    handleSessionStartRun(VALID_PARAMS)
    expect(handleSessionEndRun({ run_id: "run-1" })).toEqual({ ended: true })
  })

  it("returns ended=false for an unknown run", () => {
    expect(handleSessionEndRun({ run_id: "missing" })).toEqual({ ended: false })
  })

  it("rejects missing run_id", () => {
    expect(() => handleSessionEndRun({})).toThrow(TypeError)
  })
})
```

- [ ] **Step 2: Run, expect fail**

```bash
pnpm --dir xvision-agentd test -- session-start-run session-end-run
```

Expected: FAIL — cannot find module.

- [ ] **Step 3: Implement the `build-agent` factory (stub for Task 5; full wire-up in Task 6)**

Create `xvision-agentd/src/session/build-agent.ts`:

```ts
import { Agent } from "@cline/sdk"
import { shimRegistryToTools } from "./tool-shim.js"
import { handleToolRegistryGet } from "../methods/tool-registry.js"
import type { StartRunConfig } from "./store.js"

export function buildAgent(config: StartRunConfig, opts: { allowWrites?: boolean } = {}): Agent {
  const reg = handleToolRegistryGet()
  const tools = shimRegistryToTools(reg.tools, config.allowed_tools, {
    allowWrites: opts.allowWrites ?? false,
  })

  return new Agent({
    providerId: config.provider_id,
    modelId: config.model_id,
    apiKey: config.api_key,
    baseUrl: config.base_url,
    systemPrompt: config.system_prompt,
    tools,
  })
}
```

- [ ] **Step 4: Implement the session JSON-RPC handlers**

Create `xvision-agentd/src/methods/session.ts`:

```ts
import { registerMethod } from "./index.js"
import {
  getDefaultStore,
  type SessionStore,
  type StartRunConfig,
  type BudgetLimits,
} from "../session/store.js"
import { handleToolRegistryGet } from "./tool-registry.js"

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
  return { run_id: s.run_id, started_at_ms: s.created_at_ms }
}

export function handleSessionEndRun(raw: unknown): EndRunResult {
  const p = (raw ?? {}) as EndRunParams
  if (typeof p.run_id !== "string" || p.run_id.length === 0)
    throw new TypeError("params.run_id must be a non-empty string")
  const ended = store.end(p.run_id)
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
  return {
    provider_id: p.provider_id,
    model_id: p.model_id,
    api_key: p.api_key as string | undefined,
    base_url: p.base_url as string | undefined,
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

registerMethod("session.start_run", (p) => handleSessionStartRun(p))
registerMethod("session.end_run", (p) => handleSessionEndRun(p))
```

- [ ] **Step 5: Wire the session methods into the UDS server**

Edit `xvision-agentd/src/transport/uds-server.ts`. Add a side-effect import alongside the existing ones:

```ts
import "../methods/runtime-health.js"
import "../methods/tool-registry.js"
import "../methods/tool-invoke.js"
import "../methods/session.js"   // ← add this line
```

- [ ] **Step 6: Run, expect pass**

```bash
pnpm --dir xvision-agentd test -- session-start-run session-end-run
```

Expected: PASS — all nine cases green across the two files.

- [ ] **Step 7: Full test pass**

```bash
pnpm --dir xvision-agentd test
```

Expected: all previously passing tests still pass.

- [ ] **Step 8: Commit**

```bash
git add xvision-agentd/src/session/build-agent.ts xvision-agentd/src/methods/session.ts \
        xvision-agentd/src/transport/uds-server.ts \
        xvision-agentd/test/session-start-run.test.ts xvision-agentd/test/session-end-run.test.ts
git commit -m "$(cat <<'EOF'
feat(agentd): session.start_run and session.end_run JSON-RPC methods

Wave 2 Task 5. Validates start_run params against the active tool
registry; end_run removes the session entry. Agent is not instantiated
yet — Task 6 wires session.step.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 6: Implement `session.step` end-to-end against the mock provider

**Files:**
- Modify: `xvision-agentd/src/methods/session.ts`
- Create: `xvision-agentd/test/session-step.test.ts`

**Goal:** `session.step` looks up the session, lazily builds the `Agent` on first call, runs `agent.run(prompt)` (or `agent.continue(prompt)` if the agent has already run), and returns `{ output_text, status, iterations, usage }`. Tool callbacks round-trip through the shimmed tools from Task 3, which call `callRust` (Wave 1's callback socket). The end-to-end test uses the Task 4 mock provider so no real LLM call is made.

- [ ] **Step 1: Failing test for `session.step`**

Create `xvision-agentd/test/session-step.test.ts`:

```ts
import { describe, it, expect, beforeEach, vi } from "vitest"
import {
  handleSessionStartRun,
  handleSessionStep,
  handleSessionEndRun,
  __setStoreForTesting,
} from "../src/methods/session.js"
import { createStore } from "../src/session/store.js"
import { resetRegistry, handleToolRegistrySet } from "../src/methods/tool-registry.js"
import * as callbackClient from "../src/transport/callback-client.js"
import { installMockProvider, setMockScript, resetMockScript } from "./helpers/mock-provider.js"

const ECHO_DESC = {
  name: "echo",
  version: "1.0.0",
  description: "echoes its input back",
  input_schema: { type: "object", properties: { msg: { type: "string" } }, required: ["msg"] },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "pure",
  requires_approval: false,
}

const PARAMS = {
  run_id: "run-step-1",
  provider_id: "xvision-mock",
  model_id: "mock-model",
  api_key: "test",
  system_prompt: "you are a test",
  allowed_tools: ["echo"],
  budget_limits: { max_input_tokens: 1000, max_output_tokens: 1000, max_wall_ms: 30000 },
}

describe("session.step", () => {
  beforeEach(() => {
    installMockProvider()
    resetMockScript()
    resetRegistry()
    handleToolRegistrySet({ tools: [ECHO_DESC] })
    __setStoreForTesting(createStore({ now: () => 1 }))
    vi.restoreAllMocks()
  })

  it("returns assistant text when the model emits text and finishes", async () => {
    setMockScript([{ text: "hello from the model" }])
    handleSessionStartRun(PARAMS)
    const r = await handleSessionStep({ run_id: "run-step-1", prompt: "hi" })
    expect(r.status).toBe("completed")
    expect(r.output_text).toContain("hello from the model")
    expect(r.usage.input_tokens).toBeGreaterThan(0)
    handleSessionEndRun({ run_id: "run-step-1" })
  })

  it("round-trips tool calls through callRust", async () => {
    const spy = vi.spyOn(callbackClient, "callRust").mockResolvedValue({ echoed: "hi" })
    setMockScript([
      { toolCall: { name: "echo", input: { msg: "hi" } } },
      { text: "did it" },
    ])
    handleSessionStartRun(PARAMS)
    const r = await handleSessionStep({ run_id: "run-step-1", prompt: "go" })
    expect(spy).toHaveBeenCalledWith("echo", { msg: "hi" })
    expect(r.status).toBe("completed")
    expect(r.output_text).toContain("did it")
    handleSessionEndRun({ run_id: "run-step-1" })
  })

  it("rejects an unknown run_id", async () => {
    await expect(handleSessionStep({ run_id: "no-such-run", prompt: "x" }))
      .rejects.toThrow(/session not found/)
  })

  it("rejects missing prompt", async () => {
    handleSessionStartRun(PARAMS)
    await expect(handleSessionStep({ run_id: "run-step-1" }))
      .rejects.toThrow(TypeError)
    handleSessionEndRun({ run_id: "run-step-1" })
  })

  it("uses agent.continue on a second step", async () => {
    setMockScript([{ text: "first" }, { text: "second" }])
    handleSessionStartRun(PARAMS)
    const r1 = await handleSessionStep({ run_id: "run-step-1", prompt: "one" })
    const r2 = await handleSessionStep({ run_id: "run-step-1", prompt: "two" })
    expect(r1.output_text).toContain("first")
    expect(r2.output_text).toContain("second")
    expect(r2.iterations).toBeGreaterThanOrEqual(r1.iterations)
    handleSessionEndRun({ run_id: "run-step-1" })
  })
})
```

- [ ] **Step 2: Run, expect fail**

```bash
pnpm --dir xvision-agentd test -- session-step
```

Expected: FAIL — `handleSessionStep is not a function` (not exported yet).

- [ ] **Step 3: Implement `session.step` in `methods/session.ts`**

Edit `xvision-agentd/src/methods/session.ts`. Add at the bottom, before the `registerMethod` lines:

```ts
import { buildAgent } from "../session/build-agent.js"

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

  const result = agent.hasRun
    ? await agent.continue(p.prompt)
    : await agent.run(p.prompt)

  return {
    status: result.status,
    output_text: result.outputText,
    iterations: result.iterations,
    usage: {
      input_tokens: result.usage.inputTokens,
      output_tokens: result.usage.outputTokens,
      cache_read_tokens: result.usage.cacheReadTokens,
      cache_write_tokens: result.usage.cacheWriteTokens,
      total_cost: result.usage.totalCost,
    },
    error: result.error?.message,
  }
}
```

And register the method:

```ts
registerMethod("session.step", (p) => handleSessionStep(p))
```

(Place next to the other two `registerMethod` calls at the bottom.)

- [ ] **Step 4: Run, expect pass**

```bash
pnpm --dir xvision-agentd test -- session-step
```

Expected: PASS — all five cases green.

If any assertions about field names fail (e.g. `result.status` doesn't match `"completed"`), cross-check against `~/.claude/skills/cline-sdk/references/agent/api.md` lines 128–144 (the `AgentRunResult` shape) and adjust the mapping. Do not change the assertion — change the mapping in `handleSessionStep`.

- [ ] **Step 5: Full test pass**

```bash
pnpm --dir xvision-agentd test
```

Expected: every previously passing test still passes.

- [ ] **Step 6: Commit**

```bash
git add xvision-agentd/src/methods/session.ts xvision-agentd/test/session-step.test.ts
git commit -m "$(cat <<'EOF'
feat(agentd): session.step instantiates @cline/sdk Agent and runs end-to-end

Wave 2 Task 6. Lazy Agent build on first step, agent.continue() on
subsequent steps, tool calls round-trip via the Wave-1 callback socket.
Tested with a scripted mock provider for deterministic CI.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 7: Rust client — `start_run` / `step` / `end_run` types and methods

**Files:**
- Modify: `crates/xvision-agent-client/src/protocol.rs`
- Modify: `crates/xvision-agent-client/src/client.rs`
- Modify: `crates/xvision-agent-client/src/lib.rs`
- Modify: `crates/xvision-agent-client/tests/` (existing mock-server fixture; specific file noted in step 4)

**Goal:** Strongly-typed Rust API for the three new methods. `AgentClient` exposes `start_run`, `step`, and `end_run`; each serializes the params, sends the JSON-RPC request, and deserializes the result. Tests use the existing mock-server pattern.

- [ ] **Step 1: Add a new test file for the session-method mock-server tests**

Wave 1's mock server lives in `crates/xvision-agent-client/tests/transport_mock.rs` and exercises `UdsTransport::call` directly (no `AgentClient` wrapper) for `runtime.health`. For Wave 2 we add `AgentClient`-level method tests that need the full client (handshake + transport). Create a sibling file `crates/xvision-agent-client/tests/session_methods_mock.rs` that follows the same single-connection mock-server pattern but dispatches `session.*` methods.

- [ ] **Step 2: Write failing tests for `start_run`, `step`, `end_run`**

Create `crates/xvision-agent-client/tests/session_methods_mock.rs`:

```rust
//! AgentClient session-method tests against a single-connection mock UDS
//! server. The server handles `runtime.health` (for AgentClient's
//! handshake) plus the three new session methods. Pattern mirrors
//! `transport_mock.rs`.

use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use xvision_agent_client::{
    BudgetLimits, EndRunParams, StartRunParams, StepParams, UdsTransport,
};

async fn start_session_mock(socket_path: PathBuf) -> tokio::task::JoinHandle<()> {
    let listener = UnixListener::bind(&socket_path).expect("bind");
    tokio::spawn(async move {
        if let Ok((conn, _)) = listener.accept().await {
            let (r, mut w) = conn.into_split();
            let mut br = BufReader::new(r);
            let mut line = String::new();
            while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = req["id"].clone();
                let method = req["method"].as_str().unwrap_or("");
                let resp = match method {
                    "runtime.health" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocol_version": "0.1.0",
                            "sidecar_version": "0.2.0",
                            "cline_sdk_version": "1.2.3",
                            "status": "ok"
                        }
                    }),
                    "session.start_run" => {
                        let p = &req["params"];
                        assert_eq!(p["run_id"], "r1");
                        assert_eq!(p["provider_id"], "xvision-mock");
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": { "run_id": "r1", "started_at_ms": 42 }
                        })
                    }
                    "session.step" => {
                        let p = &req["params"];
                        assert_eq!(p["run_id"], "r1");
                        assert_eq!(p["prompt"], "hi");
                        json!({
                            "jsonrpc": "2.0",
                            "id": id,
                            "result": {
                                "status": "completed",
                                "output_text": "hello",
                                "iterations": 1,
                                "usage": {
                                    "input_tokens": 10,
                                    "output_tokens": 5,
                                    "cache_read_tokens": 0,
                                    "cache_write_tokens": 0
                                }
                            }
                        })
                    }
                    "session.end_run" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "ended": true }
                    }),
                    _ => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "unknown method" }
                    }),
                };
                let mut out = serde_json::to_vec(&resp).unwrap();
                out.push(b'\n');
                w.write_all(&out).await.unwrap();
                w.flush().await.unwrap();
                line.clear();
            }
        }
    })
}

#[tokio::test]
async fn start_run_round_trip() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_session_mock(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let res: xvision_agent_client::StartRunResult = t
        .call::<StartRunParams, _>(
            "session.start_run",
            Some(StartRunParams {
                run_id: "r1".into(),
                provider_id: "xvision-mock".into(),
                model_id: "mock-model".into(),
                api_key: Some("test".into()),
                base_url: None,
                system_prompt: "test".into(),
                allowed_tools: vec!["echo".into()],
                budget_limits: BudgetLimits {
                    max_input_tokens: 1000,
                    max_output_tokens: 1000,
                    max_wall_ms: 30_000,
                },
            }),
        )
        .await
        .expect("rpc");
    assert_eq!(res.run_id, "r1");
    assert_eq!(res.started_at_ms, 42);
}

#[tokio::test]
async fn step_round_trip() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_session_mock(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let res: xvision_agent_client::StepResult = t
        .call::<StepParams, _>(
            "session.step",
            Some(StepParams { run_id: "r1".into(), prompt: "hi".into() }),
        )
        .await
        .expect("rpc");
    assert_eq!(res.status, "completed");
    assert_eq!(res.output_text, "hello");
    assert_eq!(res.iterations, 1);
    assert_eq!(res.usage.input_tokens, 10);
}

#[tokio::test]
async fn end_run_round_trip() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_session_mock(sock.clone()).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let res: xvision_agent_client::EndRunResult = t
        .call::<EndRunParams, _>(
            "session.end_run",
            Some(EndRunParams { run_id: "r1".into() }),
        )
        .await
        .expect("rpc");
    assert!(res.ended);
}
```

- [ ] **Step 3: Run, expect compile error**

```bash
cargo test -p xvision-agent-client --no-run 2>&1 | tail -20
```

Expected: compile errors on missing types `StartRunParams`, `StepParams`, `EndRunParams`, `BudgetLimits`, and missing methods on `AgentClient`.

- [ ] **Step 4: Add the protocol types**

Append to `crates/xvision-agent-client/src/protocol.rs`:

```rust
#[derive(Debug, Clone, Serialize)]
pub struct BudgetLimits {
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub max_wall_ms: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct StartRunParams {
    pub run_id: String,
    pub provider_id: String,
    pub model_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<String>,
    pub system_prompt: String,
    pub allowed_tools: Vec<String>,
    pub budget_limits: BudgetLimits,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StartRunResult {
    pub run_id: String,
    pub started_at_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepParams {
    pub run_id: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RunUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    #[serde(default)]
    pub total_cost: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StepResult {
    pub status: String,
    pub output_text: String,
    pub iterations: u32,
    pub usage: RunUsage,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EndRunParams {
    pub run_id: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EndRunResult {
    pub ended: bool,
}
```

- [ ] **Step 5: Add the methods to `AgentClient`**

Edit `crates/xvision-agent-client/src/client.rs`. `AgentClient` already wraps `self.transport: UdsTransport` and uses `self.transport.call::<P, R>(method, Some(params))` for `runtime.health`, `tool.registry.set`, and `tool.registry.get`. The session methods follow the same shape. Add inside `impl AgentClient`, alongside the existing `register_tools` / `list_tools` methods:

```rust
use crate::protocol::{
    EndRunParams, EndRunResult, StartRunParams, StartRunResult, StepParams, StepResult,
};

impl AgentClient {
    pub async fn start_run(&self, params: StartRunParams) -> Result<StartRunResult> {
        self.transport
            .call::<StartRunParams, StartRunResult>("session.start_run", Some(params))
            .await
    }

    pub async fn step(&self, params: StepParams) -> Result<StepResult> {
        self.transport
            .call::<StepParams, StepResult>("session.step", Some(params))
            .await
    }

    pub async fn end_run(&self, params: EndRunParams) -> Result<EndRunResult> {
        self.transport
            .call::<EndRunParams, EndRunResult>("session.end_run", Some(params))
            .await
    }
}
```

Note: the existing `impl AgentClient` block in `client.rs` already imports its protocol types at the top of the file (`use crate::protocol::{...}`). Merge the new imports there rather than adding a duplicate `use` statement inside the impl.

- [ ] **Step 6: Re-export the new types**

Edit `crates/xvision-agent-client/src/lib.rs`. Extend the `pub use protocol::{...}` block:

```rust
pub use protocol::{
    BudgetLimits, EndRunParams, EndRunResult, RunUsage, RuntimeHealthResult, SideEffectLevel,
    StartRunParams, StartRunResult, StepParams, StepResult, ToolDescriptor,
    ToolRegistryGetResult, ToolRegistrySetResult, SUPPORTED_PROTOCOL_VERSION,
};
```

- [ ] **Step 7: Run the tests, expect pass**

```bash
cargo test -p xvision-agent-client
```

Expected: all three new tests pass plus the previously passing tests still pass.

- [ ] **Step 8: Commit**

```bash
git add crates/xvision-agent-client/src/protocol.rs \
        crates/xvision-agent-client/src/client.rs \
        crates/xvision-agent-client/src/lib.rs \
        crates/xvision-agent-client/tests/
git commit -m "$(cat <<'EOF'
feat(agent-client): AgentClient::start_run/step/end_run + protocol types

Wave 2 Task 7. Mirrors the sidecar's session lifecycle API in Rust with
strongly-typed params/results. Tested against the existing mock-server
fixture.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 8: End-to-end integration test (Rust spawns real sidecar, runs one session)

**Files:**
- Create: `crates/xvision-agent-client/tests/session_lifecycle.rs`

**Goal:** Prove the full loop: Rust spawns the real sidecar, registers a tool registry, starts a run, calls step (which causes the sidecar's `Agent` to call the registered tool via the callback socket, round-tripping through Rust's `ToolDispatch` and back), gets the final output, ends the run. Gated by `XVISION_RUN_SIDECAR_TESTS=1` matching the existing supervisor smoke test convention.

- [ ] **Step 1: Verify the existing smoke-test fixture conventions**

```bash
grep -rn "XVISION_RUN_SIDECAR_TESTS\|spawn.*sidecar" crates/xvision-agent-client/tests/ | head
```

Note the env-var gate name and the existing helper for spawning the sidecar (probably via `AgentClient::spawn` from `supervisor.rs`). The new test reuses the same fixture and the same env-var gate.

- [ ] **Step 2: Add a sidecar-env override so the integration test installs the mock provider**

The sidecar test needs the mock provider registered before any session starts. Two options:

1. Extend `xvision-agentd/src/index.ts` to check `XVISION_TEST_MOCK_PROVIDER=1` and dynamically import + install the mock provider before starting the UDS server.
2. Add a CLI flag `--test-mock-provider`.

Use option 1 — env var is simpler from the Rust side. Edit `xvision-agentd/src/index.ts`:

```ts
import { startUdsServer } from "./transport/uds-server.js"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "./version.js"
import { setCallbackSocketPath } from "./transport/callback-client.js"

async function main(): Promise<void> {
  const args = process.argv.slice(2)

  if (args[0] === "--version") {
    // ... unchanged ...
  }

  // Test-only: install a deterministic mock provider before sessions
  // can start. Gated by env var so production builds never load it.
  if (process.env.XVISION_TEST_MOCK_PROVIDER === "1") {
    const { installMockProvider, setMockScript } = await import(
      "../test/helpers/mock-provider.js"
    )
    installMockProvider()
    // Default script: a single tool call to "echo" with msg="from-sidecar",
    // followed by a final text "done". The integration test can pre-set the
    // mock script by writing to a JSON file path passed via
    // XVISION_TEST_MOCK_SCRIPT, but the default makes the simplest test work
    // without any extra wiring.
    setMockScript([
      { toolCall: { name: "echo", input: { msg: "from-sidecar" } } },
      { text: "done" },
    ])
  }

  // ... rest unchanged ...
}
```

Note: pull the mock-provider import out of `test/` for production safety — see Step 3.

- [ ] **Step 3: Move the mock provider helper into a non-test path**

The dynamic import in Step 2 references `test/helpers/mock-provider.js`. `tsc` only compiles files included by `tsconfig.json` `include`. If `test/` is excluded from the production build, the import will fail at runtime even with the env var set.

Decision: keep `mock-provider.ts` under `xvision-agentd/src/testing/mock-provider.ts` (a `src/` path), and re-export from the test file:

```bash
mkdir -p xvision-agentd/src/testing
git mv xvision-agentd/test/helpers/mock-provider.ts xvision-agentd/src/testing/mock-provider.ts
```

Update the test file `xvision-agentd/test/helpers/mock-provider.test.ts` and any other import sites to import from `../../src/testing/mock-provider.js` instead of `./mock-provider.js`.

Update Step 2's dynamic import path:

```ts
const { installMockProvider, setMockScript } = await import(
  "./testing/mock-provider.js"
)
```

Run `pnpm --dir xvision-agentd test` to verify nothing broke from the move.

- [ ] **Step 4: Write the failing Rust integration test**

Create `crates/xvision-agent-client/tests/session_lifecycle.rs`:

```rust
//! End-to-end Wave 2 integration test.
//!
//! Spawns the real sidecar with the test mock provider installed, registers
//! a tool, starts a run, calls step (which causes the sidecar's Agent to
//! call the registered tool via the callback socket), verifies the round
//! trip, and ends the run.
//!
//! Gated by `XVISION_RUN_SIDECAR_TESTS=1` to keep CI from spawning Node by
//! default. Build the sidecar first:
//!     pnpm --dir xvision-agentd build

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use serde_json::json;
use tempfile::TempDir;
use tokio::time::timeout;

use xvision_agent_client::{
    AgentClient, BudgetLimits, EndRunParams, SideEffectLevel, StartRunParams, StepParams,
    ToolDescriptor, ToolDispatch, ToolDispatchError,
};

struct EchoDispatch;

#[async_trait]
impl ToolDispatch for EchoDispatch {
    async fn invoke(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> std::result::Result<serde_json::Value, ToolDispatchError> {
        if name != "echo" {
            return Err(ToolDispatchError::UnknownTool(name.into()));
        }
        let msg = input.get("msg").and_then(|v| v.as_str()).unwrap_or("");
        Ok(json!({ "echoed": msg }))
    }
}

#[tokio::test]
async fn full_session_round_trip() {
    if std::env::var("XVISION_RUN_SIDECAR_TESTS").ok().as_deref() != Some("1") {
        eprintln!("skipping: XVISION_RUN_SIDECAR_TESTS != 1");
        return;
    }

    let sidecar_path: PathBuf = std::env::var("XVISION_AGENTD_PATH")
        .unwrap_or_else(|_| "xvision-agentd/dist/index.js".to_string())
        .into();

    // Mock provider lives in the sidecar; gate via env var before spawn.
    // Supervisor::spawn currently inherits the parent process env, so this
    // env_var set propagates into the spawned node process. If Wave 2
    // changes Supervisor to clear the env, update Supervisor to accept an
    // explicit env-vars vec and plumb XVISION_TEST_MOCK_PROVIDER through.
    std::env::set_var("XVISION_TEST_MOCK_PROVIDER", "1");

    let dir = TempDir::new().expect("tempdir");
    let socket_path = dir.path().join("xvision-agentd.sock");
    let callback_path = dir.path().join("xvision-callbacks.sock");

    let client = AgentClient::spawn_with_callbacks(
        &sidecar_path,
        &socket_path,
        &callback_path,
        Arc::new(EchoDispatch),
    )
    .await
    .expect("spawn sidecar");

    // Step 1: register the echo tool via the Wave-1 register_tools path.
    client
        .register_tools(vec![ToolDescriptor {
            name: "echo".into(),
            version: "1.0.0".into(),
            description: "echoes its input back".into(),
            input_schema: json!({
                "type": "object",
                "properties": { "msg": { "type": "string" } },
                "required": ["msg"]
            }),
            output_schema: json!({ "type": "object" }),
            timeout_ms: 5000,
            side_effect_level: SideEffectLevel::Pure,
            requires_approval: false,
        }])
        .await
        .expect("register_tools");

    // Step 2: start_run.
    let started = client
        .start_run(StartRunParams {
            run_id: "wave2-it-1".into(),
            provider_id: "xvision-mock".into(),
            model_id: "mock-model".into(),
            api_key: Some("test".into()),
            base_url: None,
            system_prompt: "you are a test agent".into(),
            allowed_tools: vec!["echo".into()],
            budget_limits: BudgetLimits {
                max_input_tokens: 1000,
                max_output_tokens: 1000,
                max_wall_ms: 30_000,
            },
        })
        .await
        .expect("start_run");
    assert_eq!(started.run_id, "wave2-it-1");

    // Step 3: step. Mock script (set in xvision-agentd/src/index.ts when
    // XVISION_TEST_MOCK_PROVIDER=1): echo tool call then "done".
    let stepped = timeout(
        Duration::from_secs(20),
        client.step(StepParams {
            run_id: "wave2-it-1".into(),
            prompt: "go".into(),
        }),
    )
    .await
    .expect("step timed out")
    .expect("step");

    assert_eq!(stepped.status, "completed");
    assert!(stepped.output_text.contains("done"));

    // Step 4: end_run.
    let ended = client
        .end_run(EndRunParams { run_id: "wave2-it-1".into() })
        .await
        .expect("end_run");
    assert!(ended.ended);

    client.shutdown().await.expect("shutdown");
}
```

Notes on the API surface used (verified against Wave 1's `crates/xvision-agent-client/src/client.rs`):

- `AgentClient::spawn_with_callbacks(bin, socket_path, callback_socket_path, dispatch)` is the spawn entry point that wires the callback listener for sidecar→Rust tool calls.
- `register_tools(Vec<ToolDescriptor>) -> Result<ToolRegistrySetResult>` is the existing `tool.registry.set` wrapper.
- `shutdown(self) -> Result<()>` aborts the callback listener, removes the callback socket file, and kills the sidecar process.

- [ ] **Step 5: Build the sidecar so the integration test can find it**

```bash
pnpm --dir xvision-agentd build
```

Expected: `xvision-agentd/dist/index.js` exists.

- [ ] **Step 6: Run the integration test**

```bash
XVISION_RUN_SIDECAR_TESTS=1 \
XVISION_AGENTD_PATH="$(pwd)/xvision-agentd/dist/index.js" \
cargo test -p xvision-agent-client --test session_lifecycle -- --nocapture
```

Expected: PASS — the assistant text contains `"done"`, `ended: true`, no errors.

If the test fails, diagnose in this order before changing assertions:
1. Sidecar didn't start — check that `dist/index.js` exists and is readable.
2. Mock provider didn't register — check that `XVISION_TEST_MOCK_PROVIDER=1` was set before `AgentClient::spawn` and that the dynamic import path resolves.
3. Tool callback didn't reach Rust — instrument `EchoDispatch::invoke` with `eprintln!` and re-run; verify the callback socket path was passed to the sidecar.

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-agent-client/tests/session_lifecycle.rs \
        xvision-agentd/src/index.ts xvision-agentd/src/testing/mock-provider.ts \
        xvision-agentd/test/helpers/mock-provider.test.ts
git commit -m "$(cat <<'EOF'
test(agent-client,agentd): wave 2 end-to-end session lifecycle integration

Wave 2 Task 8. Real sidecar + real @cline/sdk Agent + scripted mock
provider + Rust ToolDispatch round-trip. Gated by
XVISION_RUN_SIDECAR_TESTS=1 to keep CI predictable.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 9: Bundle the @cline/sdk dependency into the deploy image

**Files:**
- Modify: `Dockerfile.deploy`
- Verify: `scripts/deploy-image.sh`

**Goal:** The deploy image already bundles `xvision-agentd` (Wave 1 Task 10). With `@cline/sdk` now a runtime dep, the `pnpm install --prod` step inside the multistage build must pull it in. Verify the image still boots and `runtime.health` reports the resolved SDK version.

- [ ] **Step 1: Inspect the existing Dockerfile**

```bash
grep -n "xvision-agentd\|pnpm\|npm" Dockerfile.deploy | head -30
```

Find the stage where the sidecar's `node_modules` is installed. If it uses `pnpm install --prod --frozen-lockfile` against `xvision-agentd/`, the new `@cline/sdk` dep is picked up automatically — confirm by re-building.

- [ ] **Step 2: Build the deploy image locally**

```bash
scripts/deploy-image.sh
```

Expected: image builds without errors. Note the tagged image name from the script output.

- [ ] **Step 3: Run the version check inside the image**

```bash
docker run --rm <image-tag> node /opt/xvision-agentd/dist/index.js --version
```

Expected: JSON line with `cline_sdk_version: "<the real semver>"`. Not `"unbound"`, not `"unknown"`.

- [ ] **Step 4: If the version is "unknown", fix the package.json path resolution**

The `resolveClineSdkVersion()` function in `xvision-agentd/src/version.ts` resolves `node_modules/@cline/sdk/package.json` relative to `dist/version.js`. In the Docker image, the layout may differ:

```bash
docker run --rm <image-tag> ls /opt/xvision-agentd
docker run --rm <image-tag> ls /opt/xvision-agentd/node_modules/@cline 2>/dev/null
```

If `node_modules` is in a different location, update the resolution path. Typical fix: use `require.resolve` (via `createRequire(import.meta.url)`) which respects the actual Node module resolution algorithm rather than a hard-coded relative path.

```ts
import { createRequire } from "node:module"

function resolveClineSdkVersion(): string {
  try {
    const require = createRequire(import.meta.url)
    const pkg = require("@cline/sdk/package.json") as { version?: unknown }
    if (typeof pkg.version === "string" && /^\d+\.\d+\.\d+/.test(pkg.version)) {
      return pkg.version
    }
  } catch {
    // fall through
  }
  return "unknown"
}
```

Re-run `pnpm --dir xvision-agentd test` and `pnpm --dir xvision-agentd build` after this change.

- [ ] **Step 5: Commit if any Dockerfile or version.ts changes were needed**

```bash
git add Dockerfile.deploy xvision-agentd/src/version.ts xvision-agentd/test/version.test.ts
git commit -m "$(cat <<'EOF'
build(deploy): ensure @cline/sdk is available in deploy image

Wave 2 Task 9. Uses createRequire so the SDK package.json resolves
correctly inside the deploy image's node_modules layout.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

---

## Task 10: Wave 2 acceptance gate

**Files:** none. This is a verification checklist.

After Tasks 1–9, the branch must:

- [ ] **Step 1: Sidecar tests pass**

```bash
pnpm --dir xvision-agentd test
```

Expected: all green, including the new `session-store`, `tool-shim`, `mock-provider`, `session-start-run`, `session-end-run`, and `session-step` files.

- [ ] **Step 2: Sidecar typecheck passes**

```bash
pnpm --dir xvision-agentd typecheck
pnpm --dir xvision-agentd typecheck:all
```

Expected: no errors.

- [ ] **Step 3: Rust workspace tests pass (sans gated integration)**

```bash
cargo test --workspace
```

Expected: all green. The new `session_lifecycle.rs` test is skipped without `XVISION_RUN_SIDECAR_TESTS=1` — that's expected.

- [ ] **Step 4: End-to-end integration test passes**

```bash
pnpm --dir xvision-agentd build
XVISION_RUN_SIDECAR_TESTS=1 \
XVISION_AGENTD_PATH="$(pwd)/xvision-agentd/dist/index.js" \
cargo test -p xvision-agent-client --test session_lifecycle -- --nocapture
```

Expected: PASS.

- [ ] **Step 5: Deploy image boots and reports the resolved SDK version**

```bash
scripts/deploy-image.sh
docker run --rm <image-tag> node /opt/xvision-agentd/dist/index.js --version
```

Expected: JSON line with a concrete `cline_sdk_version` semver — not `"unbound"`, not `"unknown"`.

- [ ] **Step 6: Add Wave 2 status note to the license-audit research memo**

Edit `docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md`. Add a new section at the end:

```markdown
## 2026-05-17 status — wave 2 lands with licensing still deferred

Wave 2 ships `@cline/sdk` import + session lifecycle + tool round-trip
through the Cline Agent. Per direction, the licensing baseline (LICENSE,
NOTICE, CONTRIBUTING, SECURITY, CODE_OF_CONDUCT, THIRD_PARTY_LICENSES,
cargo-deny, license-checker, license workflow) remains a deferred
follow-up. F1–F4 above are still open.
```

- [ ] **Step 7: Final commit of the recap**

```bash
git add docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md
git commit -m "$(cat <<'EOF'
docs(research): record wave 2 landing with licensing still deferred

Wave 2 Task 10. Captures the post-wave-2 status alongside the existing
F1–F4 open follow-ups.

Co-Authored-By: Claude Opus 4.7 (1M context) <noreply@anthropic.com>
EOF
)"
```

If every step above passed, Wave 2 is complete and the branch is ready for review.

---

## What Wave 3+ will plug into this scaffold

(For context only — not Wave 2 work.)

- Add `submit_decision` as a Cline custom tool with `lifecycle: { completesRun: true }`, replacing the schema-injection-in-system-prompt path in today's `crates/xvision-engine/src/agent/llm.rs:149-194` (Wave 3).
- Switch one real eval call site (live paper mode) over end-to-end. Manual validation against a known asset (Wave 3).
- Move the provider capability matrix into the `session.start_run` protocol (Wave 3).
- Backtest executor switch + perf gate. Measure first; warm sessions and batched callbacks before any architectural split (Wave 3 → 4).
- Route sidecar `agent.subscribe()` events into the Rust event bus → SQLite spans + OTel export. Backpressure tested (Wave 4).
- MCP server config flow + skills bundle per run (Wave 4).
- Streaming text deltas surfaced in the dashboard (Wave 4).
- Delete `crates/xvision-engine/src/agent/` (Wave 5).
