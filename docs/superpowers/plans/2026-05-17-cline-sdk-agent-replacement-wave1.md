# Cline SDK Agent Replacement — Wave 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Spec:** `docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md`

**Goal:** Stand up the Rust↔Node boundary the rest of the migration plugs into: license-verified baseline, a Node sidecar process reachable from Rust over JSON-RPC, version-handshake protocol, tool-registry handshake, and one real Rust-side tool (`OhlcvTool`) round-tripping end-to-end. No Cline SDK code is integrated yet — Wave 1 is the adapter scaffold.

**Architecture:** A new top-level `xvision-agentd/` Node 22 TypeScript project listens on a Unix domain socket, framed as NDJSON JSON-RPC 2.0. A new Rust crate `xvision-agent-client` (in `crates/`) spawns and supervises the sidecar, sends RPC, and dispatches tool callbacks back into the existing `crates/xvision-engine/src/tools/` registry. The protocol is the anti-churn shield — neither side imports each other's internals. Cline SDK does not appear in Wave 1.

**Tech Stack:** Rust 1.x (existing workspace), Node 22, TypeScript 5, vitest, pnpm 9 (workspace already uses it), JSON-RPC 2.0 over Unix domain sockets with NDJSON framing, tokio, serde/serde_json, async-trait. License-hygiene tools: cargo-deny, cargo-license, license-checker.

**Scope:** Steps 0–4 of the spec's migration plan. Waves 2+ (Cline SDK integration, `submit_decision` tool, executor switch-over, observability, MCP, deletion of the old agent loop) get separate plans authored from the same spec once Wave 1 is in.

**Worktree:** Per project convention, execute in a `.worktrees/cline-sdk-wave1/` worktree to keep the main checkout's `target/` cache hot. Set `CARGO_TARGET_DIR=$HOME/.cargo-target/xvision` from the worktree to avoid duplicating Rust build output (per repo CLAUDE.md).

---

## File Structure

**New files:**

```
LICENSE                                  — Apache-2.0 text
NOTICE                                   — copyright/attribution
CONTRIBUTING.md                          — contributor guide
SECURITY.md                              — vulnerability reporting
CODE_OF_CONDUCT.md                       — Contributor Covenant v2.1
THIRD_PARTY_LICENSES.md                  — generated transitive licenses

deny.toml                                — cargo-deny config
.github/workflows/license.yml            — CI license gate

docs/superpowers/research/
  2026-05-17-cline-sdk-license-audit.md  — Step 0 verification memo

xvision-agentd/                          — Node sidecar (new top-level dir)
  package.json
  tsconfig.json
  vitest.config.ts
  src/
    index.ts                             — CLI entrypoint
    version.ts                           — protocol/sidecar version constants
    transport/
      jsonrpc.ts                         — request/response/error/notif types
      ndjson.ts                          — NDJSON framing read/write
      uds-server.ts                      — Unix-socket JSON-RPC server
    methods/
      runtime-health.ts                  — runtime.health RPC handler
      tool-registry.ts                   — tool registry state + registry.set
    tools/
      registry-types.ts                  — ToolDescriptor type
  test/
    ndjson.test.ts
    uds-server.test.ts
    runtime-health.test.ts
    tool-registry.test.ts

crates/xvision-agent-client/             — Rust client (new workspace member)
  Cargo.toml
  src/
    lib.rs
    protocol.rs                          — JSON-RPC envelopes (mirror sidecar shapes)
    transport.rs                         — UDS + NDJSON framing client
    supervisor.rs                        — spawn/monitor/shutdown sidecar process
    client.rs                            — AgentClient: high-level methods
    tool_dispatch.rs                     — bridges sidecar tool callbacks → Rust ToolRegistry
    errors.rs
  tests/
    transport_mock.rs                    — JSON-RPC client against an in-test mock server
    e2e_health.rs                        — real sidecar + real client
    e2e_ohlcv_callback.rs                — real sidecar + real OHLCV tool callback
```

**Modified files:**

```
Cargo.toml                               — add xvision-agent-client to [workspace] members + default-members
Dockerfile.deploy                        — multistage Node 22 layer; bundle sidecar
```

**Untouched in Wave 1:**

- `crates/xvision-engine/src/agent/*` — still in place, still drives all runs. Deleted in a later wave.
- `crates/xvision-engine/src/tools/*` — preserved; `xvision-agent-client` reuses the existing `ToolRegistry`.
- `crates/xvision-engine/src/eval/executor/*` — untouched.

---

## Task 1: License verification (Step 0, blocking)

**Files:**
- Create: `docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md`

**Goal:** Confirm Cline SDK and its transitive Node dependencies can be redistributed under Apache-2.0 alongside xvision's existing Rust dependency graph. No code changes; produces a go/no-go memo.

- [ ] **Step 1: Verify Cline SDK package license**

Fetch the npm registry metadata for `@cline/sdk`:

```bash
curl -sSL https://registry.npmjs.org/@cline/sdk/latest | jq '.license, .repository, .version'
curl -sSL https://registry.npmjs.org/@cline/llms/latest | jq '.license, .repository, .version'
curl -sSL https://registry.npmjs.org/@cline/shared/latest | jq '.license, .repository, .version'
curl -sSL https://registry.npmjs.org/@cline/core/latest | jq '.license, .repository, .version'
```

Capture each result. Expected: each is `Apache-2.0`. If any reports a non-permissive license (GPL, AGPL, SSPL, BUSL), STOP and surface to the user — Wave 1 cannot proceed.

- [ ] **Step 2: Verify the parent repo license**

```bash
curl -sSL https://api.github.com/repos/cline/cline/license | jq '.license.spdx_id, .license.name, .html_url'
```

Expected: `Apache-2.0`. Cross-check against the package licenses from Step 1. Discrepancies are blockers.

- [ ] **Step 3: Audit transitive dependency licenses**

In a scratch directory (NOT the repo):

```bash
mkdir -p /tmp/cline-license-audit && cd /tmp/cline-license-audit
npm init -y >/dev/null
npm install --no-fund --no-audit @cline/sdk
npx license-checker --production --json > licenses.json
jq -r '.[] | .licenses' licenses.json | sort -u
```

Expected license set: `MIT`, `ISC`, `Apache-2.0`, `BSD-2-Clause`, `BSD-3-Clause`, `0BSD`, `CC0-1.0`, `Python-2.0` (occasionally), `Unlicense` (occasionally). Any of `GPL-*`, `AGPL-*`, `LGPL-*`, `SSPL`, `BUSL`, or `CC-BY-NC*` is a blocker.

- [ ] **Step 4: Write the audit memo**

Create `docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md` with the structure:

```markdown
# Cline SDK License Audit

**Date:** 2026-05-17
**Auditor:** <author>
**Outcome:** PASS | BLOCK

## Cline package licenses
| Package | Version | License |
| --- | --- | --- |
| @cline/sdk | <ver> | Apache-2.0 |
| @cline/llms | <ver> | Apache-2.0 |
| @cline/shared | <ver> | Apache-2.0 |
| @cline/core | <ver> | Apache-2.0 |

## Parent repo
- Repo: https://github.com/cline/cline
- License: Apache-2.0
- Verified at: <gh api url>

## Transitive dependency license set
<paste sorted unique license list from license-checker>

## Notable items
<any unusual licenses, dual-license clarifications, NOTICE-bearing packages>

## Verdict
<PASS | BLOCK with reason>
```

- [ ] **Step 5: Commit the memo**

```bash
git add docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md
git commit -m "docs(research): cline sdk license audit (wave 1 step 0)"
```

---

## Task 2: Repo licensing baseline + CI license gate (Step 1)

**Files:**
- Create: `LICENSE`, `NOTICE`, `CONTRIBUTING.md`, `SECURITY.md`, `CODE_OF_CONDUCT.md`, `THIRD_PARTY_LICENSES.md`, `deny.toml`, `.github/workflows/license.yml`

**Goal:** Lock in Apache-2.0 baseline and add CI checks that fail PRs introducing license-incompatible Rust or Node deps.

- [ ] **Step 1: Add `LICENSE`**

Create `LICENSE` with the verbatim Apache-2.0 text from https://www.apache.org/licenses/LICENSE-2.0.txt. Do not modify the license body. Append:

```
Copyright 2026 xvision contributors
```

just above the license header (not inside it).

- [ ] **Step 2: Add `NOTICE`**

```
xvision
Copyright 2026 xvision contributors

This product includes software developed by the xvision contributors
(https://github.com/latentwill/xvision).

Bundled third-party software is listed in THIRD_PARTY_LICENSES.md.
```

- [ ] **Step 3: Add `SECURITY.md`**

```markdown
# Security Policy

## Reporting a vulnerability

Please report security issues privately to **security@xvision.dev**
(or open a GitHub private security advisory if email is unavailable).

Do not file a public issue or PR for security-sensitive reports.
We aim to acknowledge reports within 3 business days.

## Supported versions

The `main` branch is the only supported version. Backports to release
tags are handled case-by-case.
```

- [ ] **Step 4: Add `CODE_OF_CONDUCT.md`**

Use the verbatim Contributor Covenant v2.1 (https://www.contributor-covenant.org/version/2/1/code_of_conduct.txt). Set the contact line to `conduct@xvision.dev`.

- [ ] **Step 5: Add `CONTRIBUTING.md`**

```markdown
# Contributing

Thanks for considering a contribution. Before opening a PR:

1. Discuss substantial changes in an issue first.
2. Run `cargo build --workspace` and `cargo test --workspace` locally.
3. Run `bash scripts/board-lint.sh` if you touched `team/contracts/*`.
4. Follow the terminology table in `CLAUDE.md`.
5. License: by submitting a contribution you agree to license it under
   the project's Apache-2.0 license (see `LICENSE`).

## Code of Conduct

See `CODE_OF_CONDUCT.md`. Reports to `conduct@xvision.dev`.

## Security

See `SECURITY.md`. Do not file public issues for security reports.
```

- [ ] **Step 6: Generate `THIRD_PARTY_LICENSES.md` (Rust side)**

Install the tooling and generate the initial Rust report:

```bash
cargo install --locked cargo-license cargo-deny
cargo license --json > /tmp/rust-licenses.json
```

Create `THIRD_PARTY_LICENSES.md`:

```markdown
# Third-Party Licenses

This file lists every direct and transitive dependency of xvision and its
companion `xvision-agentd` sidecar, along with the license under which it
is redistributed.

This document is regenerated by CI. To regenerate locally:

```bash
cargo license --json > docs/_generated/rust-licenses.json
(cd xvision-agentd && pnpm licenses list --json > ../docs/_generated/node-licenses.json)
node scripts/render-third-party-licenses.mjs > THIRD_PARTY_LICENSES.md
```

## Rust dependencies

<paste a flat list from /tmp/rust-licenses.json: name @ version — license>

## Node dependencies (xvision-agentd)

<populated once xvision-agentd has a lockfile (see Task 3)>
```

Render the Rust block by hand for v1 (the `scripts/render-third-party-licenses.mjs` helper is out of scope for Wave 1; the file is correct-by-hand for now).

- [ ] **Step 7: Add `deny.toml`**

```toml
[licenses]
version = 2
allow = [
    "Apache-2.0",
    "MIT",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "ISC",
    "0BSD",
    "CC0-1.0",
    "Unicode-DFS-2016",
    "MPL-2.0",
]
confidence-threshold = 0.93

[advisories]
version = 2
yanked = "deny"

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
allow-registry = ["https://github.com/rust-lang/crates.io-index"]
```

- [ ] **Step 8: Run `cargo deny check` to capture baseline state**

```bash
cargo deny check
```

If it fails on existing dependencies, **do not silence them** — record the failure in the audit memo and pause for human review. Per the alpha rule in repo memory, do not add allow-list exceptions to suppress real findings without explicit sign-off. If it passes, proceed.

- [ ] **Step 9: Add the CI license workflow**

Create `.github/workflows/license.yml`:

```yaml
name: license

on:
  pull_request:
  push:
    branches: [main]

jobs:
  cargo-deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check

  cargo-license:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo install --locked cargo-license
      - run: cargo license --json > /tmp/rust-licenses.json
      - run: |
          jq -r '.[] | .license' /tmp/rust-licenses.json \
            | sort -u \
            | tee /tmp/rust-license-set.txt
          # Fail on any license outside the allow-list.
          grep -Ev '^(Apache-2\.0|MIT|BSD-2-Clause|BSD-3-Clause|ISC|0BSD|CC0-1\.0|Unicode-DFS-2016|MPL-2\.0|Apache-2\.0 OR MIT|MIT OR Apache-2\.0|Apache-2\.0/MIT|.*OR Apache-2\.0.*|Apache-2\.0 WITH LLVM-exception)$' \
            /tmp/rust-license-set.txt && exit 1 || exit 0

  node-license:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v4
        with: { version: 9 }
      - uses: actions/setup-node@v4
        with: { node-version: 22 }
      - run: pnpm install --frozen-lockfile --dir xvision-agentd
        # The `if-no-files-found` guard lets this job no-op until Task 3 lands.
        continue-on-error: true
      - run: |
          if [ -d xvision-agentd/node_modules ]; then
            pnpm --dir xvision-agentd dlx license-checker \
              --production --onlyAllow \
              "Apache-2.0;MIT;BSD-2-Clause;BSD-3-Clause;ISC;0BSD;CC0-1.0;Python-2.0;Unlicense"
          fi
```

- [ ] **Step 10: Commit licensing baseline**

```bash
git add LICENSE NOTICE CONTRIBUTING.md SECURITY.md CODE_OF_CONDUCT.md \
        THIRD_PARTY_LICENSES.md deny.toml .github/workflows/license.yml
git commit -m "chore(license): adopt apache-2.0 baseline and ci license gates"
```

---

## Task 3: Scaffold `xvision-agentd` Node project

**Files:**
- Create: `xvision-agentd/package.json`, `xvision-agentd/tsconfig.json`, `xvision-agentd/vitest.config.ts`, `xvision-agentd/.gitignore`, `xvision-agentd/src/version.ts`, `xvision-agentd/src/index.ts`, `xvision-agentd/test/version.test.ts`

**Goal:** A Node 22/TypeScript project that builds, type-checks, and runs vitest. No JSON-RPC yet — that's Task 4.

- [ ] **Step 1: Write the failing version test**

Create `xvision-agentd/test/version.test.ts`:

```ts
import { describe, expect, it } from "vitest"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "../src/version.js"

describe("version constants", () => {
  it("exposes a protocol version", () => {
    expect(PROTOCOL_VERSION).toMatch(/^\d+\.\d+\.\d+$/)
  })
  it("exposes a sidecar version", () => {
    expect(SIDECAR_VERSION).toMatch(/^\d+\.\d+\.\d+$/)
  })
})
```

- [ ] **Step 2: Add `package.json`**

```json
{
  "name": "xvision-agentd",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "packageManager": "pnpm@9.0.0",
  "engines": { "node": ">=22.0.0" },
  "scripts": {
    "build": "tsc -p tsconfig.json",
    "typecheck": "tsc -p tsconfig.json --noEmit",
    "test": "vitest run",
    "test:watch": "vitest",
    "start": "node dist/index.js"
  },
  "dependencies": {},
  "devDependencies": {
    "typescript": "^5.6.0",
    "vitest": "^2.1.0",
    "@types/node": "^22.0.0"
  }
}
```

- [ ] **Step 3: Add `tsconfig.json`**

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "NodeNext",
    "lib": ["ES2022"],
    "outDir": "dist",
    "rootDir": "src",
    "strict": true,
    "noUncheckedIndexedAccess": true,
    "exactOptionalPropertyTypes": true,
    "noFallthroughCasesInSwitch": true,
    "noImplicitOverride": true,
    "isolatedModules": true,
    "skipLibCheck": true,
    "resolveJsonModule": true,
    "esModuleInterop": true,
    "sourceMap": true,
    "declaration": true
  },
  "include": ["src/**/*.ts", "test/**/*.ts"],
  "exclude": ["dist", "node_modules"]
}
```

- [ ] **Step 4: Add `vitest.config.ts`**

```ts
import { defineConfig } from "vitest/config"

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    environment: "node",
    pool: "threads",
  },
})
```

- [ ] **Step 5: Add `.gitignore`**

```
node_modules/
dist/
*.log
.pnpm-store/
```

- [ ] **Step 6: Add `src/version.ts`**

```ts
// Bumped manually. Wave-1 baseline.
//
// PROTOCOL_VERSION semver:
//   MAJOR — backwards-incompatible RPC shape change
//   MINOR — additive method or field
//   PATCH — bug-fix only, no protocol surface change
//
// SIDECAR_VERSION is the @cline/sdk-binding shim version.
export const PROTOCOL_VERSION = "0.1.0"
export const SIDECAR_VERSION = "0.1.0"
```

- [ ] **Step 7: Add `src/index.ts` stub**

```ts
// CLI entrypoint. Wave 1: prints version and exits.
// Wave 1 Task 4 wires up the JSON-RPC server.
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "./version.js"

const args = process.argv.slice(2)
if (args[0] === "--version") {
  console.log(JSON.stringify({ protocol_version: PROTOCOL_VERSION, sidecar_version: SIDECAR_VERSION }))
  process.exit(0)
}

console.error("xvision-agentd: no socket path provided")
process.exit(2)
```

- [ ] **Step 8: Install + run tests**

```bash
cd xvision-agentd
pnpm install
pnpm test
```

Expected: 2 tests pass.

- [ ] **Step 9: Verify build + version CLI**

```bash
cd xvision-agentd
pnpm build
node dist/index.js --version
```

Expected output: `{"protocol_version":"0.1.0","sidecar_version":"0.1.0"}`

- [ ] **Step 10: Update THIRD_PARTY_LICENSES.md with Node deps**

```bash
cd xvision-agentd
pnpm licenses list --prod
```

Paste the report into `THIRD_PARTY_LICENSES.md` under the "Node dependencies" section. Confirm everything is on the deny.toml-equivalent allow-list (Apache-2.0, MIT, BSD-2-Clause, BSD-3-Clause, ISC, 0BSD).

- [ ] **Step 11: Commit**

```bash
git add xvision-agentd/ THIRD_PARTY_LICENSES.md
git commit -m "feat(agentd): scaffold node sidecar project skeleton"
```

---

## Task 4: Sidecar JSON-RPC server with `runtime.health`

**Files:**
- Create: `xvision-agentd/src/transport/jsonrpc.ts`, `xvision-agentd/src/transport/ndjson.ts`, `xvision-agentd/src/transport/uds-server.ts`, `xvision-agentd/src/methods/runtime-health.ts`, `xvision-agentd/test/ndjson.test.ts`, `xvision-agentd/test/uds-server.test.ts`, `xvision-agentd/test/runtime-health.test.ts`
- Modify: `xvision-agentd/src/index.ts`

**Goal:** Sidecar listens on a Unix socket, parses NDJSON-framed JSON-RPC 2.0, dispatches `runtime.health`, returns `{protocol_version, sidecar_version, cline_sdk_version, status}`. `cline_sdk_version` is `"unbound"` in Wave 1 (no SDK yet).

- [ ] **Step 1: Failing test — NDJSON framing**

Create `xvision-agentd/test/ndjson.test.ts`:

```ts
import { describe, expect, it } from "vitest"
import { encodeNdjson, NdjsonDecoder } from "../src/transport/ndjson.js"

describe("ndjson framing", () => {
  it("encodes an object as a single line", () => {
    const out = encodeNdjson({ a: 1 })
    expect(out).toBe('{"a":1}\n')
  })

  it("decodes a stream of two messages across chunk boundaries", () => {
    const dec = new NdjsonDecoder()
    const events: unknown[] = []
    dec.on("message", (m) => events.push(m))
    dec.push(Buffer.from('{"a":1}\n{"b":'))
    dec.push(Buffer.from('2}\n'))
    expect(events).toEqual([{ a: 1 }, { b: 2 }])
  })

  it("emits a parse error on invalid json", () => {
    const dec = new NdjsonDecoder()
    const errors: Error[] = []
    dec.on("error", (e) => errors.push(e))
    dec.push(Buffer.from("not json\n"))
    expect(errors).toHaveLength(1)
  })
})
```

- [ ] **Step 2: Run, expect fail**

```bash
cd xvision-agentd && pnpm test -- ndjson
```

Expected: FAIL — module not found.

- [ ] **Step 3: Implement `transport/ndjson.ts`**

```ts
import { EventEmitter } from "node:events"

export function encodeNdjson(value: unknown): string {
  return JSON.stringify(value) + "\n"
}

export class NdjsonDecoder extends EventEmitter {
  private buffer = ""

  push(chunk: Buffer | string): void {
    this.buffer += typeof chunk === "string" ? chunk : chunk.toString("utf8")
    let idx: number
    while ((idx = this.buffer.indexOf("\n")) !== -1) {
      const line = this.buffer.slice(0, idx)
      this.buffer = this.buffer.slice(idx + 1)
      if (line.length === 0) continue
      try {
        this.emit("message", JSON.parse(line))
      } catch (err) {
        this.emit("error", err instanceof Error ? err : new Error(String(err)))
      }
    }
  }
}
```

- [ ] **Step 4: Run, expect pass**

```bash
cd xvision-agentd && pnpm test -- ndjson
```

Expected: 3 pass.

- [ ] **Step 5: Failing test — JSON-RPC envelope types**

Create `xvision-agentd/src/transport/jsonrpc.ts` first as a type-only module so tests compile. Append type definitions:

```ts
export interface JsonRpcRequest {
  jsonrpc: "2.0"
  id: number | string
  method: string
  params?: unknown
}

export interface JsonRpcSuccess<T = unknown> {
  jsonrpc: "2.0"
  id: number | string
  result: T
}

export interface JsonRpcError {
  jsonrpc: "2.0"
  id: number | string | null
  error: { code: number; message: string; data?: unknown }
}

export interface JsonRpcNotification {
  jsonrpc: "2.0"
  method: string
  params?: unknown
}

export type JsonRpcResponse<T = unknown> = JsonRpcSuccess<T> | JsonRpcError

export const RPC_ERROR_CODES = {
  ParseError: -32700,
  InvalidRequest: -32600,
  MethodNotFound: -32601,
  InvalidParams: -32602,
  InternalError: -32603,
  // xvision-agentd custom range -32000 to -32099
  IncompatibleVersion: -32000,
  ToolError: -32001,
  Cancelled: -32002,
} as const
```

- [ ] **Step 6: Failing test — `runtime.health` method**

Create `xvision-agentd/test/runtime-health.test.ts`:

```ts
import { describe, expect, it } from "vitest"
import { handleRuntimeHealth } from "../src/methods/runtime-health.js"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "../src/version.js"

describe("runtime.health", () => {
  it("returns protocol + sidecar + cline_sdk versions and status:ok", () => {
    const result = handleRuntimeHealth()
    expect(result).toEqual({
      protocol_version: PROTOCOL_VERSION,
      sidecar_version: SIDECAR_VERSION,
      cline_sdk_version: "unbound",
      status: "ok",
    })
  })
})
```

- [ ] **Step 7: Run, expect fail**

```bash
cd xvision-agentd && pnpm test -- runtime-health
```

Expected: FAIL — module not found.

- [ ] **Step 8: Implement `methods/runtime-health.ts`**

```ts
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "../version.js"

export interface RuntimeHealthResult {
  protocol_version: string
  sidecar_version: string
  cline_sdk_version: string
  status: "ok"
}

export function handleRuntimeHealth(): RuntimeHealthResult {
  return {
    protocol_version: PROTOCOL_VERSION,
    sidecar_version: SIDECAR_VERSION,
    cline_sdk_version: "unbound",
    status: "ok",
  }
}
```

- [ ] **Step 9: Run, expect pass**

```bash
cd xvision-agentd && pnpm test -- runtime-health
```

Expected: 1 pass.

- [ ] **Step 10: Failing test — UDS server end-to-end**

Create `xvision-agentd/test/uds-server.test.ts`:

```ts
import { describe, expect, it, beforeEach, afterEach } from "vitest"
import * as net from "node:net"
import * as os from "node:os"
import * as path from "node:path"
import * as fs from "node:fs/promises"
import { startUdsServer } from "../src/transport/uds-server.js"
import { encodeNdjson, NdjsonDecoder } from "../src/transport/ndjson.js"
import type { JsonRpcResponse } from "../src/transport/jsonrpc.js"

let socketPath: string
let server: { close: () => Promise<void> }

beforeEach(async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "xvision-agentd-"))
  socketPath = path.join(dir, "sock")
  server = await startUdsServer(socketPath)
})

afterEach(async () => {
  await server.close()
})

async function rpc<T>(method: string, params?: unknown): Promise<JsonRpcResponse<T>> {
  return new Promise((resolve, reject) => {
    const sock = net.createConnection(socketPath)
    const decoder = new NdjsonDecoder()
    decoder.on("message", (msg) => {
      sock.end()
      resolve(msg as JsonRpcResponse<T>)
    })
    decoder.on("error", reject)
    sock.on("data", (chunk) => decoder.push(chunk))
    sock.on("error", reject)
    sock.on("connect", () => {
      sock.write(encodeNdjson({ jsonrpc: "2.0", id: 1, method, params }))
    })
  })
}

describe("uds-server", () => {
  it("returns runtime.health result", async () => {
    const resp = await rpc<{ status: string }>("runtime.health")
    expect("result" in resp).toBe(true)
    if ("result" in resp) {
      expect(resp.result.status).toBe("ok")
    }
  })

  it("returns MethodNotFound for unknown methods", async () => {
    const resp = await rpc("does.not.exist")
    expect("error" in resp).toBe(true)
    if ("error" in resp) {
      expect(resp.error.code).toBe(-32601)
    }
  })

  it("returns ParseError on malformed input", async () => {
    const sock = net.createConnection(socketPath)
    const decoder = new NdjsonDecoder()
    const result: unknown = await new Promise((resolve, reject) => {
      decoder.on("message", (m) => {
        sock.end()
        resolve(m)
      })
      decoder.on("error", reject)
      sock.on("data", (c) => decoder.push(c))
      sock.on("connect", () => sock.write("not json\n"))
      sock.on("error", reject)
    })
    expect(result).toMatchObject({ error: { code: -32700 } })
  })
})
```

- [ ] **Step 11: Run, expect fail**

```bash
cd xvision-agentd && pnpm test -- uds-server
```

Expected: FAIL — module not found.

- [ ] **Step 12: Implement `transport/uds-server.ts`**

```ts
import * as net from "node:net"
import { NdjsonDecoder, encodeNdjson } from "./ndjson.js"
import {
  JsonRpcRequest,
  JsonRpcResponse,
  RPC_ERROR_CODES,
} from "./jsonrpc.js"
import { handleRuntimeHealth } from "../methods/runtime-health.js"

export interface UdsServerHandle {
  close(): Promise<void>
}

type MethodHandler = (params: unknown) => Promise<unknown> | unknown

const methods: Record<string, MethodHandler> = {
  "runtime.health": () => handleRuntimeHealth(),
}

export async function startUdsServer(socketPath: string): Promise<UdsServerHandle> {
  const server = net.createServer((conn) => {
    const decoder = new NdjsonDecoder()
    decoder.on("message", async (raw) => {
      const resp = await dispatch(raw)
      if (resp) conn.write(encodeNdjson(resp))
    })
    decoder.on("error", (_err) => {
      conn.write(encodeNdjson({
        jsonrpc: "2.0",
        id: null,
        error: { code: RPC_ERROR_CODES.ParseError, message: "parse error" },
      }))
    })
    conn.on("data", (chunk) => decoder.push(chunk))
    conn.on("error", () => { /* swallow; client may close abruptly */ })
  })

  await new Promise<void>((resolve, reject) => {
    server.once("error", reject)
    server.listen(socketPath, () => {
      server.off("error", reject)
      resolve()
    })
  })

  return {
    async close() {
      await new Promise<void>((resolve) => server.close(() => resolve()))
    },
  }
}

async function dispatch(raw: unknown): Promise<JsonRpcResponse | null> {
  if (
    typeof raw !== "object" ||
    raw === null ||
    (raw as { jsonrpc?: unknown }).jsonrpc !== "2.0"
  ) {
    return {
      jsonrpc: "2.0",
      id: null,
      error: { code: RPC_ERROR_CODES.InvalidRequest, message: "invalid request" },
    }
  }
  const req = raw as JsonRpcRequest
  const handler = methods[req.method]
  if (!handler) {
    return {
      jsonrpc: "2.0",
      id: req.id,
      error: { code: RPC_ERROR_CODES.MethodNotFound, message: `unknown method: ${req.method}` },
    }
  }
  try {
    const result = await handler(req.params)
    return { jsonrpc: "2.0", id: req.id, result }
  } catch (err) {
    return {
      jsonrpc: "2.0",
      id: req.id,
      error: {
        code: RPC_ERROR_CODES.InternalError,
        message: err instanceof Error ? err.message : String(err),
      },
    }
  }
}
```

- [ ] **Step 13: Run, expect pass**

```bash
cd xvision-agentd && pnpm test
```

Expected: all tests pass.

- [ ] **Step 14: Wire `src/index.ts` to start the server**

Replace the contents of `xvision-agentd/src/index.ts`:

```ts
import { startUdsServer } from "./transport/uds-server.js"
import { PROTOCOL_VERSION, SIDECAR_VERSION } from "./version.js"

async function main(): Promise<void> {
  const args = process.argv.slice(2)

  if (args[0] === "--version") {
    console.log(JSON.stringify({ protocol_version: PROTOCOL_VERSION, sidecar_version: SIDECAR_VERSION }))
    process.exit(0)
  }

  const socketIdx = args.indexOf("--socket")
  if (socketIdx === -1 || !args[socketIdx + 1]) {
    console.error("xvision-agentd: missing --socket <path>")
    process.exit(2)
  }
  const socketPath = args[socketIdx + 1]!

  const server = await startUdsServer(socketPath)
  const shutdown = async (): Promise<void> => {
    await server.close()
    process.exit(0)
  }
  process.on("SIGTERM", shutdown)
  process.on("SIGINT", shutdown)

  // Parent-PID liveness monitor. Exit if our parent goes away.
  // .unref() lets the interval not keep the event loop alive on its own —
  // graceful shutdown via SIGTERM still works.
  const parentPid = process.ppid
  setInterval(() => {
    try {
      process.kill(parentPid, 0)
    } catch {
      void shutdown()
    }
  }, 1000).unref()

  // Structured "ready" log on stderr so the Rust supervisor can sync.
  process.stderr.write(JSON.stringify({ event: "ready", socket: socketPath }) + "\n")
}

void main()
```

- [ ] **Step 15: Smoke-test the binary manually**

```bash
cd xvision-agentd && pnpm build
SOCKET=$(mktemp -u -t xvn-agentd.XXXXX)
node dist/index.js --socket "$SOCKET" &
PID=$!
sleep 0.2
printf '{"jsonrpc":"2.0","id":1,"method":"runtime.health"}\n' | nc -U "$SOCKET" -q 1
kill "$PID"
```

Expected: a single JSON-RPC success response with `status:"ok"`.

- [ ] **Step 16: Commit**

```bash
git add xvision-agentd/
git commit -m "feat(agentd): json-rpc server over uds with runtime.health"
```

---

## Task 5: Rust client crate scaffold + JSON-RPC client + mock-server tests

**Files:**
- Create: `crates/xvision-agent-client/Cargo.toml`, `crates/xvision-agent-client/src/lib.rs`, `crates/xvision-agent-client/src/protocol.rs`, `crates/xvision-agent-client/src/transport.rs`, `crates/xvision-agent-client/src/errors.rs`, `crates/xvision-agent-client/tests/transport_mock.rs`
- Modify: `Cargo.toml` (workspace root)

**Goal:** A new Rust crate that speaks JSON-RPC over a Unix socket. Tested against a tokio mock server in-process; no real sidecar yet.

- [ ] **Step 1: Add crate to workspace `Cargo.toml`**

Modify `/Users/edkennedy/Code/xvision/Cargo.toml`:

Add `"crates/xvision-agent-client",` to the `members` array (alphabetical position is fine; keep adjacent to other `xvision-*` entries).

Add `"crates/xvision-agent-client",` to `default-members` as well (the client is part of every build).

- [ ] **Step 2: Create `crates/xvision-agent-client/Cargo.toml`**

```toml
[package]
name = "xvision-agent-client"
version = "0.1.0"
edition = "2021"
license = "Apache-2.0"
publish = false

[dependencies]
tokio = { version = "1", features = ["net", "io-util", "macros", "process", "rt-multi-thread", "time", "sync"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"

[dev-dependencies]
tempfile = "3"
tokio = { version = "1", features = ["test-util", "macros"] }
```

If the workspace already pins these versions in a `[workspace.dependencies]` block, prefer `tokio = { workspace = true, features = [...] }` style. Check `/Users/edkennedy/Code/xvision/Cargo.toml` and adapt.

- [ ] **Step 3: Failing test — JSON-RPC client against a mock server**

Create `crates/xvision-agent-client/tests/transport_mock.rs`:

```rust
use std::path::PathBuf;

use serde_json::json;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use xvision_agent_client::{RuntimeHealthResult, UdsTransport};

async fn start_mock_server(socket_path: PathBuf) -> tokio::task::JoinHandle<()> {
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
                let resp = if method == "runtime.health" {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "protocol_version": "0.1.0",
                            "sidecar_version": "0.1.0",
                            "cline_sdk_version": "unbound",
                            "status": "ok"
                        }
                    })
                } else {
                    json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": { "code": -32601, "message": "unknown method" }
                    })
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
async fn calls_runtime_health_against_mock() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_mock_server(sock.clone()).await;

    // Tiny wait to let the listener be ready.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let t = UdsTransport::connect(&sock).await.expect("connect");
    let h: RuntimeHealthResult = t
        .call::<(), _>("runtime.health", None)
        .await
        .expect("rpc");
    assert_eq!(h.protocol_version, "0.1.0");
    assert_eq!(h.cline_sdk_version, "unbound");
    assert_eq!(h.status, "ok");
}

#[tokio::test]
async fn surfaces_method_not_found_as_rpc_error() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let _server = start_mock_server(sock.clone()).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let t = UdsTransport::connect(&sock).await.unwrap();
    let err = t
        .call::<(), serde_json::Value>("does.not.exist", None)
        .await
        .expect_err("should fail");
    match err {
        xvision_agent_client::AgentClientError::Rpc { code, .. } => assert_eq!(code, -32601),
        other => panic!("wrong error variant: {other:?}"),
    }
}
```

- [ ] **Step 4: Run, expect compile error**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client
```

Expected: FAIL — `xvision_agent_client::{RuntimeHealthResult, UdsTransport, AgentClientError}` not found. That's the right kind of failure for "write the test, then make it pass."

- [ ] **Step 5: Create `crates/xvision-agent-client/src/errors.rs`**

```rust
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentClientError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("rpc error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("malformed response: missing both result and error")]
    MalformedResponse,
    #[error("incompatible version: {0}")]
    IncompatibleVersion(String),
    #[error("sidecar transport closed")]
    TransportClosed,
}

pub type Result<T> = std::result::Result<T, AgentClientError>;
```

- [ ] **Step 6: Create `crates/xvision-agent-client/src/protocol.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct JsonRpcRequest<'a, P: Serialize> {
    pub jsonrpc: &'a str,
    pub id: u64,
    pub method: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<P>,
}

// `Option<T>` already deserializes as `None` when the field is absent;
// `#[serde(default)]` would force serde's derive to add an `R: Default`
// bound to JsonRpcResponse<R>, breaking calls with non-Default `R`.
#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcResponse<R> {
    pub jsonrpc: String,
    pub id: Option<serde_json::Value>,
    pub result: Option<R>,
    pub error: Option<JsonRpcErrorBody>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct JsonRpcErrorBody {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeHealthResult {
    pub protocol_version: String,
    pub sidecar_version: String,
    pub cline_sdk_version: String,
    pub status: String,
}

pub const SUPPORTED_PROTOCOL_VERSION: &str = "0.1.0";
```

- [ ] **Step 7: Create `crates/xvision-agent-client/src/transport.rs`**

```rust
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Mutex;

use crate::errors::{AgentClientError, Result};
use crate::protocol::{JsonRpcErrorBody, JsonRpcRequest, JsonRpcResponse};

/// Mutex-guarded UDS transport. Wave 1: synchronous request-response only.
pub struct UdsTransport {
    inner: Mutex<TransportInner>,
    next_id: AtomicU64,
}

struct TransportInner {
    reader: BufReader<tokio::net::unix::OwnedReadHalf>,
    writer: tokio::net::unix::OwnedWriteHalf,
}

impl UdsTransport {
    pub async fn connect(socket_path: impl AsRef<Path>) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        let (r, w) = stream.into_split();
        Ok(Self {
            inner: Mutex::new(TransportInner {
                reader: BufReader::new(r),
                writer: w,
            }),
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn call<P: Serialize, R: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        params: Option<P>,
    ) -> Result<R> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let req = JsonRpcRequest { jsonrpc: "2.0", id, method, params };

        let mut guard = self.inner.lock().await;
        let mut line = serde_json::to_vec(&req)?;
        line.push(b'\n');
        guard.writer.write_all(&line).await?;
        guard.writer.flush().await?;

        let mut buf = String::new();
        let n = guard.reader.read_line(&mut buf).await?;
        if n == 0 {
            return Err(AgentClientError::TransportClosed);
        }
        let resp: JsonRpcResponse<R> = serde_json::from_str(&buf)?;
        if let Some(err) = resp.error {
            let JsonRpcErrorBody { code, message, .. } = err;
            return Err(AgentClientError::Rpc { code, message });
        }
        resp.result.ok_or(AgentClientError::MalformedResponse)
    }
}
```

- [ ] **Step 8: Create `crates/xvision-agent-client/src/lib.rs`**

```rust
pub mod errors;
pub mod protocol;
pub mod transport;

pub use errors::{AgentClientError, Result};
pub use protocol::{RuntimeHealthResult, SUPPORTED_PROTOCOL_VERSION};
pub use transport::UdsTransport;
```

- [ ] **Step 9: Run, expect pass**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client -- --nocapture
```

Expected: 2 pass.

- [ ] **Step 10: Commit**

```bash
git add Cargo.toml crates/xvision-agent-client/
git commit -m "feat(agent-client): rust crate, json-rpc client over uds with mock-server tests"
```

---

## Task 6: Sidecar supervisor — spawn, monitor, shutdown

**Files:**
- Create: `crates/xvision-agent-client/src/supervisor.rs`, `crates/xvision-agent-client/src/client.rs`
- Modify: `crates/xvision-agent-client/src/lib.rs`

**Goal:** Rust spawns `node dist/index.js --socket <path>`, waits for the structured `ready` log, opens the UDS transport, and exposes an `AgentClient::health()` method. On drop, the supervisor sends SIGTERM and reaps.

- [ ] **Step 1: Failing test — spawn a real sidecar and call `health()`**

Create `crates/xvision-agent-client/tests/supervisor_smoke.rs`:

```rust
use std::path::PathBuf;
use tempfile::TempDir;
use xvision_agent_client::AgentClient;

fn agentd_bin() -> PathBuf {
    // Repo-root-relative path computed from CARGO_MANIFEST_DIR.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("xvision-agentd/dist/index.js")
}

#[tokio::test]
async fn spawns_and_calls_health() {
    let bin = agentd_bin();
    if !bin.exists() {
        eprintln!("skipping: xvision-agentd not built. Run `pnpm --dir xvision-agentd build` first.");
        return;
    }

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");

    let client = AgentClient::spawn(&bin, &sock)
        .await
        .expect("spawn sidecar");

    let h = client.health().await.expect("health");
    assert_eq!(h.status, "ok");
    assert_eq!(h.protocol_version, "0.1.0");
    assert_eq!(h.cline_sdk_version, "unbound");

    client.shutdown().await.expect("shutdown");
}
```

- [ ] **Step 2: Run, expect fail**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client supervisor_smoke
```

Expected: compile error — `AgentClient::spawn`, `health`, `shutdown` not defined.

- [ ] **Step 3: Implement `supervisor.rs`**

```rust
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

use crate::errors::{AgentClientError, Result};

pub struct Supervisor {
    child: Option<Child>,
    pub socket_path: PathBuf,
}

impl Supervisor {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let mut cmd = Command::new("node");
        cmd.arg(bin)
            .arg("--socket")
            .arg(socket_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        let mut child = cmd.spawn()?;

        // Wait for the structured `ready` event on stderr.
        let stderr = child
            .stderr
            .take()
            .ok_or(AgentClientError::TransportClosed)?;
        let mut lines = BufReader::new(stderr).lines();

        let ready = tokio::time::timeout(Duration::from_secs(5), async {
            while let Some(line) = lines.next_line().await? {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) {
                    if v.get("event").and_then(|x| x.as_str()) == Some("ready") {
                        return Ok::<(), AgentClientError>(());
                    }
                }
            }
            Err(AgentClientError::TransportClosed)
        })
        .await;

        match ready {
            Ok(Ok(())) => Ok(Self {
                child: Some(child),
                socket_path: socket_path.to_path_buf(),
            }),
            _ => {
                let _ = child.kill().await;
                Err(AgentClientError::TransportClosed)
            }
        }
    }

    pub async fn shutdown(mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
            let _ = tokio::time::timeout(Duration::from_secs(2), child.wait()).await;
        }
        Ok(())
    }
}

impl Drop for Supervisor {
    fn drop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.start_kill();
        }
    }
}
```

- [ ] **Step 4: Implement `client.rs`**

```rust
use std::path::Path;

use crate::errors::Result;
use crate::protocol::RuntimeHealthResult;
use crate::supervisor::Supervisor;
use crate::transport::UdsTransport;

pub struct AgentClient {
    transport: UdsTransport,
    supervisor: Supervisor,
}

impl AgentClient {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let supervisor = Supervisor::spawn(bin, socket_path).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        Ok(Self { transport, supervisor })
    }

    pub async fn health(&self) -> Result<RuntimeHealthResult> {
        self.transport.call::<(), _>("runtime.health", None).await
    }

    pub async fn shutdown(self) -> Result<()> {
        self.supervisor.shutdown().await
    }
}
```

- [ ] **Step 5: Re-export in `lib.rs`**

Replace the existing `crates/xvision-agent-client/src/lib.rs`:

```rust
pub mod client;
pub mod errors;
pub mod protocol;
pub mod supervisor;
pub mod transport;

pub use client::AgentClient;
pub use errors::{AgentClientError, Result};
pub use protocol::{RuntimeHealthResult, SUPPORTED_PROTOCOL_VERSION};
pub use transport::UdsTransport;
```

- [ ] **Step 6: Build the sidecar so the smoke test can find it**

```bash
cd /Users/edkennedy/Code/xvision/xvision-agentd
pnpm install
pnpm build
```

- [ ] **Step 7: Run smoke test, expect pass**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client supervisor_smoke -- --nocapture
```

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add crates/xvision-agent-client/
git commit -m "feat(agent-client): supervisor spawns sidecar; AgentClient::health round-trip"
```

---

## Task 7: Version handshake enforcement

**Files:**
- Modify: `crates/xvision-agent-client/src/client.rs`, `crates/xvision-agent-client/src/protocol.rs`
- Create: `crates/xvision-agent-client/tests/handshake.rs`

**Goal:** First thing `AgentClient::spawn` does after the transport is open is call `runtime.health` and compare `protocol_version` against `SUPPORTED_PROTOCOL_VERSION`. Mismatch returns `AgentClientError::IncompatibleVersion`.

- [ ] **Step 1: Failing test — handshake rejects mismatched protocol**

Create `crates/xvision-agent-client/tests/handshake.rs`:

```rust
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

use xvision_agent_client::{AgentClient, AgentClientError, UdsTransport};

async fn start_fake_sidecar(sock: PathBuf, protocol_version: &'static str) {
    let listener = UnixListener::bind(&sock).unwrap();
    tokio::spawn(async move {
        if let Ok((conn, _)) = listener.accept().await {
            let (r, mut w) = conn.into_split();
            let mut br = BufReader::new(r);
            let mut line = String::new();
            while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                let req: serde_json::Value = serde_json::from_str(&line).unwrap();
                let id = req["id"].clone();
                let resp = serde_json::json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "result": {
                        "protocol_version": protocol_version,
                        "sidecar_version": "0.1.0",
                        "cline_sdk_version": "unbound",
                        "status": "ok"
                    }
                });
                let mut out = serde_json::to_vec(&resp).unwrap();
                out.push(b'\n');
                w.write_all(&out).await.unwrap();
                w.flush().await.unwrap();
                line.clear();
            }
        }
    });
}

#[tokio::test]
async fn handshake_accepts_matching_protocol() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    start_fake_sidecar(sock.clone(), "0.1.0").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let t = UdsTransport::connect(&sock).await.unwrap();
    AgentClient::handshake(&t).await.expect("handshake ok");
}

#[tokio::test]
async fn handshake_rejects_incompatible_protocol() {
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    start_fake_sidecar(sock.clone(), "9.9.9").await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    let t = UdsTransport::connect(&sock).await.unwrap();
    let err = AgentClient::handshake(&t).await.expect_err("should fail");
    assert!(matches!(err, AgentClientError::IncompatibleVersion(_)));
}
```

- [ ] **Step 2: Run, expect fail**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client handshake
```

Expected: compile error — `AgentClient::handshake` not defined.

- [ ] **Step 3: Implement `AgentClient::handshake` and call it from `spawn`**

Replace `crates/xvision-agent-client/src/client.rs`:

```rust
use std::path::Path;

use crate::errors::{AgentClientError, Result};
use crate::protocol::{RuntimeHealthResult, SUPPORTED_PROTOCOL_VERSION};
use crate::supervisor::Supervisor;
use crate::transport::UdsTransport;

pub struct AgentClient {
    transport: UdsTransport,
    supervisor: Supervisor,
    versions: RuntimeHealthResult,
}

impl AgentClient {
    pub async fn spawn(bin: &Path, socket_path: &Path) -> Result<Self> {
        let supervisor = Supervisor::spawn(bin, socket_path).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        let versions = Self::handshake(&transport).await?;
        Ok(Self { transport, supervisor, versions })
    }

    pub async fn handshake(transport: &UdsTransport) -> Result<RuntimeHealthResult> {
        let h: RuntimeHealthResult = transport.call::<(), _>("runtime.health", None).await?;
        if h.protocol_version != SUPPORTED_PROTOCOL_VERSION {
            return Err(AgentClientError::IncompatibleVersion(format!(
                "sidecar speaks protocol {}; client supports {}",
                h.protocol_version, SUPPORTED_PROTOCOL_VERSION
            )));
        }
        Ok(h)
    }

    pub fn versions(&self) -> &RuntimeHealthResult {
        &self.versions
    }

    pub async fn health(&self) -> Result<RuntimeHealthResult> {
        self.transport.call::<(), _>("runtime.health", None).await
    }

    pub async fn shutdown(self) -> Result<()> {
        self.supervisor.shutdown().await
    }
}
```

- [ ] **Step 4: Run, expect pass**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client handshake
```

Expected: 2 pass.

- [ ] **Step 5: Re-run all client tests**

```bash
cargo test -p xvision-agent-client
```

Expected: all green.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-agent-client/
git commit -m "feat(agent-client): version handshake on spawn"
```

---

## Task 8: Tool registry handshake protocol

**Files:**
- Create: `xvision-agentd/src/methods/tool-registry.ts`, `xvision-agentd/test/tool-registry.test.ts`
- Modify: `xvision-agentd/src/transport/uds-server.ts` (register the new method), `crates/xvision-agent-client/src/protocol.rs`, `crates/xvision-agent-client/src/client.rs`
- Create: `crates/xvision-agent-client/tests/tool_registry.rs`

**Goal:** Rust pushes a tool descriptor table to the sidecar with `tool.registry.set`. The sidecar stores it (in-memory, no Cline integration yet) and returns the count + a stable hash of the registry. Rust can read it back with `tool.registry.get`.

- [ ] **Step 1: Define `ToolDescriptor` shape**

Modify `crates/xvision-agent-client/src/protocol.rs` to append:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDescriptor {
    pub name: String,
    pub version: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub output_schema: serde_json::Value,
    pub timeout_ms: u32,
    pub side_effect_level: SideEffectLevel,
    pub requires_approval: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectLevel {
    Pure,
    ReadOnly,
    ExternalRead,
    ExternalWrite,
}

#[derive(Debug, Clone, Serialize)]
pub struct ToolRegistrySetParams {
    pub tools: Vec<ToolDescriptor>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolRegistrySetResult {
    pub count: usize,
    pub registry_hash: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ToolRegistryGetResult {
    pub tools: Vec<ToolDescriptor>,
    pub registry_hash: String,
}
```

- [ ] **Step 2: Failing sidecar test — `tool.registry.set` / `.get`**

Create `xvision-agentd/test/tool-registry.test.ts`:

```ts
import { describe, expect, it } from "vitest"
import { resetRegistry, handleToolRegistrySet, handleToolRegistryGet } from "../src/methods/tool-registry.js"

const sample = {
  name: "ohlcv",
  version: "1.0.0",
  description: "OHLCV history",
  input_schema: { type: "object" },
  output_schema: { type: "object" },
  timeout_ms: 5000,
  side_effect_level: "external_read",
  requires_approval: false,
}

describe("tool.registry", () => {
  it("starts empty", () => {
    resetRegistry()
    const r = handleToolRegistryGet()
    expect(r.tools).toEqual([])
  })

  it("set returns count and a stable hash", () => {
    resetRegistry()
    const r1 = handleToolRegistrySet({ tools: [sample] })
    expect(r1.count).toBe(1)
    expect(r1.registry_hash).toMatch(/^[a-f0-9]{64}$/)
    const r2 = handleToolRegistrySet({ tools: [sample] })
    expect(r2.registry_hash).toBe(r1.registry_hash)
  })

  it("get returns the last-set tools", () => {
    resetRegistry()
    handleToolRegistrySet({ tools: [sample] })
    const got = handleToolRegistryGet()
    expect(got.tools).toHaveLength(1)
    expect(got.tools[0]?.name).toBe("ohlcv")
  })

  it("rejects malformed descriptors", () => {
    resetRegistry()
    expect(() => handleToolRegistrySet({ tools: [{ name: "x" } as never] })).toThrow()
  })
})
```

- [ ] **Step 3: Run, expect fail**

```bash
cd xvision-agentd && pnpm test -- tool-registry
```

Expected: module not found.

- [ ] **Step 4: Implement `methods/tool-registry.ts`**

```ts
import { createHash } from "node:crypto"

interface ToolDescriptor {
  name: string
  version: string
  description: string
  input_schema: unknown
  output_schema: unknown
  timeout_ms: number
  side_effect_level: "pure" | "read_only" | "external_read" | "external_write"
  requires_approval: boolean
}

interface ToolRegistrySetParams { tools: ToolDescriptor[] }
interface ToolRegistrySetResult { count: number; registry_hash: string }
interface ToolRegistryGetResult { tools: ToolDescriptor[]; registry_hash: string }

let current: ToolDescriptor[] = []
let currentHash = sha256("")

export function resetRegistry(): void {
  current = []
  currentHash = sha256("")
}

export function handleToolRegistrySet(params: unknown): ToolRegistrySetResult {
  const tools = validate(params)
  current = tools.slice().sort((a, b) => a.name.localeCompare(b.name))
  currentHash = sha256(JSON.stringify(current))
  return { count: current.length, registry_hash: currentHash }
}

export function handleToolRegistryGet(): ToolRegistryGetResult {
  return { tools: current, registry_hash: currentHash }
}

function validate(params: unknown): ToolDescriptor[] {
  if (typeof params !== "object" || params === null) throw new TypeError("params must be an object")
  const p = params as { tools?: unknown }
  if (!Array.isArray(p.tools)) throw new TypeError("tools must be an array")
  for (const t of p.tools) {
    if (typeof t !== "object" || t === null) throw new TypeError("tool must be an object")
    const x = t as Record<string, unknown>
    for (const k of ["name", "version", "description", "side_effect_level"]) {
      if (typeof x[k] !== "string") throw new TypeError(`tool.${k} must be string`)
    }
    if (typeof x.timeout_ms !== "number") throw new TypeError("tool.timeout_ms must be number")
    if (typeof x.requires_approval !== "boolean") throw new TypeError("tool.requires_approval must be bool")
    if (typeof x.input_schema !== "object" || x.input_schema === null) throw new TypeError("tool.input_schema required")
    if (typeof x.output_schema !== "object" || x.output_schema === null) throw new TypeError("tool.output_schema required")
  }
  return p.tools as ToolDescriptor[]
}

function sha256(s: string): string {
  return createHash("sha256").update(s).digest("hex")
}
```

- [ ] **Step 5: Run, expect pass**

```bash
cd xvision-agentd && pnpm test -- tool-registry
```

Expected: 4 pass.

- [ ] **Step 6: Wire methods into the UDS server**

Modify `xvision-agentd/src/transport/uds-server.ts`. Replace the `methods` record near the top:

```ts
import { handleToolRegistrySet, handleToolRegistryGet } from "../methods/tool-registry.js"

const methods: Record<string, MethodHandler> = {
  "runtime.health": () => handleRuntimeHealth(),
  "tool.registry.set": (p) => handleToolRegistrySet(p),
  "tool.registry.get": () => handleToolRegistryGet(),
}
```

The dispatcher already catches thrown errors and maps them to `InternalError`. For validation errors we want `InvalidParams` (-32602). Update `dispatch` to recognize `TypeError`:

```ts
  } catch (err) {
    const isTypeError = err instanceof TypeError
    return {
      jsonrpc: "2.0",
      id: req.id,
      error: {
        code: isTypeError ? RPC_ERROR_CODES.InvalidParams : RPC_ERROR_CODES.InternalError,
        message: err instanceof Error ? err.message : String(err),
      },
    }
  }
```

- [ ] **Step 7: Re-run sidecar tests**

```bash
cd xvision-agentd && pnpm test
```

Expected: all green. `uds-server` tests should still pass because they don't hit the new methods.

- [ ] **Step 8: Rebuild sidecar**

```bash
cd xvision-agentd && pnpm build
```

- [ ] **Step 9: Failing test — Rust side `register_tools` + `list_tools`**

Create `crates/xvision-agent-client/tests/tool_registry.rs`:

```rust
use std::path::PathBuf;
use tempfile::TempDir;

use xvision_agent_client::{AgentClient, SideEffectLevel, ToolDescriptor};

fn agentd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("xvision-agentd/dist/index.js")
}

fn sample_tool() -> ToolDescriptor {
    ToolDescriptor {
        name: "ohlcv".into(),
        version: "1.0.0".into(),
        description: "OHLCV history".into(),
        input_schema: serde_json::json!({"type": "object"}),
        output_schema: serde_json::json!({"type": "object"}),
        timeout_ms: 5_000,
        side_effect_level: SideEffectLevel::ExternalRead,
        requires_approval: false,
    }
}

#[tokio::test]
async fn registers_and_reads_back_tools() {
    let bin = agentd_bin();
    if !bin.exists() {
        eprintln!("skipping: build xvision-agentd first");
        return;
    }
    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let client = AgentClient::spawn(&bin, &sock).await.expect("spawn");

    let set = client
        .register_tools(vec![sample_tool()])
        .await
        .expect("register");
    assert_eq!(set.count, 1);
    assert_eq!(set.registry_hash.len(), 64);

    let got = client.list_tools().await.expect("list");
    assert_eq!(got.tools.len(), 1);
    assert_eq!(got.tools[0].name, "ohlcv");
    assert_eq!(got.registry_hash, set.registry_hash);

    client.shutdown().await.unwrap();
}
```

- [ ] **Step 10: Run, expect fail**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client tool_registry
```

Expected: compile error — methods don't exist.

- [ ] **Step 11: Add the methods on `AgentClient`**

Append to `crates/xvision-agent-client/src/client.rs`:

```rust
use crate::protocol::{
    ToolDescriptor, ToolRegistryGetResult, ToolRegistrySetParams, ToolRegistrySetResult,
};

impl AgentClient {
    pub async fn register_tools(&self, tools: Vec<ToolDescriptor>) -> Result<ToolRegistrySetResult> {
        self.transport
            .call::<ToolRegistrySetParams, ToolRegistrySetResult>(
                "tool.registry.set",
                Some(ToolRegistrySetParams { tools }),
            )
            .await
    }

    pub async fn list_tools(&self) -> Result<ToolRegistryGetResult> {
        self.transport
            .call::<(), ToolRegistryGetResult>("tool.registry.get", None)
            .await
    }
}
```

Re-export the new protocol types from `lib.rs`:

```rust
pub use protocol::{
    RuntimeHealthResult, SideEffectLevel, ToolDescriptor, ToolRegistryGetResult,
    ToolRegistrySetResult, SUPPORTED_PROTOCOL_VERSION,
};
```

- [ ] **Step 12: Run, expect pass**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client tool_registry -- --nocapture
```

Expected: pass.

- [ ] **Step 13: Commit**

```bash
git add xvision-agentd/ crates/xvision-agent-client/
git commit -m "feat(agent-client,agentd): tool registry handshake protocol"
```

---

## Task 9: OHLCV tool callback round-trip

**Files:**
- Create: `crates/xvision-agent-client/src/tool_dispatch.rs`, `xvision-agentd/src/methods/tool-invoke.ts`, `xvision-agentd/test/tool-invoke.test.ts`, `crates/xvision-agent-client/tests/e2e_ohlcv_callback.rs`
- Modify: `xvision-agentd/src/transport/uds-server.ts` (notify direction), `crates/xvision-agent-client/src/client.rs`, `crates/xvision-agent-client/src/transport.rs`, `crates/xvision-agent-client/src/lib.rs`

**Goal:** The sidecar can invoke a Rust-side tool. Wave-1 wire path: the Rust client opens the connection and sends `tool.invoke.from_sidecar` requests *from* the sidecar (via a method the client polls). To keep Wave 1's transport simple — strictly request/response, no concurrent server-push — we introduce a single test method `tool.invoke` that takes `{name, input}` and returns `{output}`. The sidecar's handler proxies to the Rust client via a callback channel set up at spawn time, using a second short-lived UDS socket for the reverse direction.

Rationale: full bidirectional streaming on one socket is more transport surface than Wave 1 needs. A second socket — opened by the sidecar, listened on by Rust — gives us clean request/response semantics in both directions without inventing JSON-RPC notification handling on day one. Wave 2 can collapse this to a single connection.

- [ ] **Step 1: Add callback-socket support to `Supervisor`**

Modify `crates/xvision-agent-client/src/supervisor.rs` to take an optional `callback_socket_path` and pass it as `--callback-socket` to node:

```rust
pub async fn spawn(bin: &Path, socket_path: &Path, callback_socket_path: Option<&Path>) -> Result<Self> {
    let mut cmd = Command::new("node");
    cmd.arg(bin).arg("--socket").arg(socket_path);
    if let Some(cb) = callback_socket_path {
        cmd.arg("--callback-socket").arg(cb);
    }
    cmd.stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // ... (rest unchanged)
}
```

Update existing call sites in `client.rs` to pass `None` for the legacy path. Then add the dual-socket constructor.

- [ ] **Step 2: Define a `ToolDispatch` trait in the client crate**

**Dependency direction matters.** `xvision-agent-client` must not depend on `xvision-engine` at runtime — Wave 5+ will reverse the direction (engine → client) and a runtime cycle would block it. So the client crate defines a small trait that the engine will later implement on its `ToolRegistry`. For Wave 1, tests provide their own implementation.

Create `crates/xvision-agent-client/src/tool_dispatch.rs`:

```rust
use std::path::Path;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

/// Implemented by anything that can resolve a tool name to a JSON-in/JSON-out
/// callable. The engine crate provides an impl over its existing
/// `ToolRegistry` in a later wave; Wave 1 tests provide their own.
#[async_trait]
pub trait ToolDispatch: Send + Sync + 'static {
    async fn invoke(&self, name: &str, input: serde_json::Value)
        -> std::result::Result<serde_json::Value, ToolDispatchError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolDispatchError {
    #[error("unknown tool: {0}")]
    UnknownTool(String),
    #[error("tool failed: {0}")]
    Failed(String),
}

#[derive(Debug, Deserialize)]
struct InvokeRequest {
    #[allow(dead_code)] jsonrpc: String,
    id: u64,
    method: String,
    params: InvokeParams,
}

#[derive(Debug, Deserialize)]
struct InvokeParams {
    name: String,
    input: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct RpcOk { jsonrpc: &'static str, id: u64, result: serde_json::Value }

#[derive(Debug, Serialize)]
struct RpcErr { jsonrpc: &'static str, id: u64, error: ErrorBody }

#[derive(Debug, Serialize)]
struct ErrorBody { code: i64, message: String }

pub async fn serve_callbacks(
    socket_path: &Path,
    dispatch: Arc<dyn ToolDispatch>,
) -> std::io::Result<()> {
    let listener = UnixListener::bind(socket_path)?;
    tokio::spawn(async move {
        loop {
            let Ok((conn, _)) = listener.accept().await else { continue };
            let dispatch = dispatch.clone();
            tokio::spawn(async move {
                let (r, mut w) = conn.into_split();
                let mut br = BufReader::new(r);
                let mut line = String::new();
                while br.read_line(&mut line).await.unwrap_or(0) > 0 {
                    let bytes = match serde_json::from_str::<InvokeRequest>(&line) {
                        Ok(req) if req.method == "tool.invoke" => {
                            match dispatch.invoke(&req.params.name, req.params.input).await {
                                Ok(out) => serde_json::to_vec(&RpcOk {
                                    jsonrpc: "2.0", id: req.id, result: out,
                                }).unwrap_or_default(),
                                Err(ToolDispatchError::UnknownTool(n)) => serde_json::to_vec(&RpcErr {
                                    jsonrpc: "2.0", id: req.id,
                                    error: ErrorBody { code: -32601, message: format!("unknown tool: {n}") },
                                }).unwrap_or_default(),
                                Err(ToolDispatchError::Failed(m)) => serde_json::to_vec(&RpcErr {
                                    jsonrpc: "2.0", id: req.id,
                                    error: ErrorBody { code: -32001, message: m },
                                }).unwrap_or_default(),
                            }
                        }
                        Ok(req) => serde_json::to_vec(&RpcErr {
                            jsonrpc: "2.0", id: req.id,
                            error: ErrorBody { code: -32601, message: format!("unknown method: {}", req.method) },
                        }).unwrap_or_default(),
                        Err(e) => {
                            let _ = w.write_all(format!(
                                "{{\"jsonrpc\":\"2.0\",\"id\":null,\"error\":{{\"code\":-32700,\"message\":\"{}\"}}}}\n",
                                e
                            ).as_bytes()).await;
                            line.clear();
                            continue;
                        }
                    };
                    let _ = w.write_all(&bytes).await;
                    let _ = w.write_all(b"\n").await;
                    let _ = w.flush().await;
                    line.clear();
                }
            });
        }
    });
    Ok(())
}
```

Add `async-trait` to runtime `[dependencies]` in `crates/xvision-agent-client/Cargo.toml`:

```toml
async-trait = "0.1"
```

Do **not** add `xvision-engine` as a runtime dependency. Add it as a **dev-dependency** so e2e tests can adapt the existing `ToolRegistry`:

```toml
[dev-dependencies]
xvision-engine = { path = "../xvision-engine" }
```

- [ ] **Step 3: Extend `AgentClient` with `spawn_with_callbacks` + `invoke_tool_via_sidecar`**

Add to `crates/xvision-agent-client/src/client.rs`:

```rust
use std::sync::Arc;

use crate::tool_dispatch::{serve_callbacks, ToolDispatch};

impl AgentClient {
    pub async fn spawn_with_callbacks(
        bin: &Path,
        socket_path: &Path,
        callback_socket_path: &Path,
        dispatch: Arc<dyn ToolDispatch>,
    ) -> Result<Self> {
        serve_callbacks(callback_socket_path, dispatch).await?;
        let supervisor = Supervisor::spawn(bin, socket_path, Some(callback_socket_path)).await?;
        let transport = UdsTransport::connect(&supervisor.socket_path).await?;
        let versions = Self::handshake(&transport).await?;
        Ok(Self { transport, supervisor, versions })
    }

    pub async fn invoke_tool_via_sidecar(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value> {
        #[derive(serde::Serialize)]
        struct P<'a> { name: &'a str, input: serde_json::Value }
        self.transport
            .call::<P, serde_json::Value>("tool.invoke", Some(P { name, input }))
            .await
    }
}
```

Re-export from `lib.rs`:

```rust
pub mod tool_dispatch;
pub use tool_dispatch::{ToolDispatch, ToolDispatchError};
```

- [ ] **Step 4: Sidecar — implement the callback-side caller**

Create `xvision-agentd/src/transport/callback-client.ts`:

```ts
import * as net from "node:net"
import { encodeNdjson, NdjsonDecoder } from "./ndjson.js"

let callbackSocketPath: string | undefined
let nextId = 1

export function setCallbackSocketPath(p: string | undefined): void {
  callbackSocketPath = p
}

export async function callRust(name: string, input: unknown): Promise<unknown> {
  if (!callbackSocketPath) throw new Error("callback socket not configured")
  return new Promise((resolve, reject) => {
    const sock = net.createConnection(callbackSocketPath!)
    const decoder = new NdjsonDecoder()
    decoder.on("message", (resp: unknown) => {
      sock.end()
      const r = resp as { result?: unknown; error?: { code: number; message: string } }
      if (r.error) reject(new Error(`${r.error.code}: ${r.error.message}`))
      else resolve(r.result)
    })
    decoder.on("error", reject)
    sock.on("data", (c) => decoder.push(c))
    sock.on("error", reject)
    sock.on("connect", () => {
      sock.write(
        encodeNdjson({ jsonrpc: "2.0", id: nextId++, method: "tool.invoke", params: { name, input } })
      )
    })
  })
}
```

- [ ] **Step 5: Sidecar — wire `tool.invoke` to call back into Rust**

Create `xvision-agentd/src/methods/tool-invoke.ts`:

```ts
import { callRust } from "../transport/callback-client.js"

interface InvokeParams { name?: unknown; input?: unknown }

export async function handleToolInvoke(params: unknown): Promise<unknown> {
  const p = (params ?? {}) as InvokeParams
  if (typeof p.name !== "string") throw new TypeError("params.name must be string")
  if (typeof p.input !== "object" || p.input === null) throw new TypeError("params.input must be object")
  return await callRust(p.name, p.input)
}
```

Register it in `uds-server.ts`:

```ts
import { handleToolInvoke } from "../methods/tool-invoke.js"
const methods: Record<string, MethodHandler> = {
  "runtime.health": () => handleRuntimeHealth(),
  "tool.registry.set": (p) => handleToolRegistrySet(p),
  "tool.registry.get": () => handleToolRegistryGet(),
  "tool.invoke": (p) => handleToolInvoke(p),
}
```

In `src/index.ts`, read `--callback-socket` and wire it:

```ts
import { setCallbackSocketPath } from "./transport/callback-client.js"
// ...
const cbIdx = args.indexOf("--callback-socket")
if (cbIdx !== -1 && args[cbIdx + 1]) {
  setCallbackSocketPath(args[cbIdx + 1])
}
```

- [ ] **Step 6: Sidecar test for `handleToolInvoke` parameter validation**

Create `xvision-agentd/test/tool-invoke.test.ts`:

```ts
import { describe, expect, it } from "vitest"
import { handleToolInvoke } from "../src/methods/tool-invoke.js"
import { setCallbackSocketPath } from "../src/transport/callback-client.js"

describe("tool.invoke params validation", () => {
  it("rejects missing name", async () => {
    setCallbackSocketPath(undefined)
    await expect(handleToolInvoke({ input: {} })).rejects.toThrow(/name/)
  })
  it("rejects missing input", async () => {
    setCallbackSocketPath(undefined)
    await expect(handleToolInvoke({ name: "x" })).rejects.toThrow(/input/)
  })
  it("rejects when callback socket unconfigured", async () => {
    setCallbackSocketPath(undefined)
    await expect(handleToolInvoke({ name: "x", input: {} })).rejects.toThrow(/callback socket/)
  })
})
```

Run + verify pass:

```bash
cd xvision-agentd && pnpm test -- tool-invoke
```

Expected: 3 pass.

- [ ] **Step 7: Rebuild sidecar**

```bash
cd xvision-agentd && pnpm build
```

- [ ] **Step 8: Failing e2e test — OHLCV round-trip with a fixture**

Create `crates/xvision-agent-client/tests/e2e_ohlcv_callback.rs`. Note the test-only adapter `EngineRegistryDispatch` that bridges the runtime `ToolDispatch` trait to the engine's `ToolRegistry`. Keeping the adapter in the test file (not in the client crate proper) preserves the one-way dependency direction (engine → client) that the rest of the migration relies on:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tempfile::TempDir;
use xvision_agent_client::{AgentClient, ToolDispatch, ToolDispatchError};
use xvision_engine::tools::{ToolName, ToolRegistry};

struct EngineRegistryDispatch(Arc<ToolRegistry>);

#[async_trait]
impl ToolDispatch for EngineRegistryDispatch {
    async fn invoke(
        &self,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, ToolDispatchError> {
        let tool = self
            .0
            .get(&ToolName::new(name))
            .ok_or_else(|| ToolDispatchError::UnknownTool(name.to_string()))?;
        tool.invoke(input)
            .await
            .map_err(|e| ToolDispatchError::Failed(e.to_string()))
    }
}

fn agentd_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("xvision-agentd/dist/index.js")
}

fn fixture_name() -> Option<String> {
    // Set XVN_OHLCV_FIXTURE to a fixture name known to
    // xvision_data::fixtures::load_ohlcv_fixture. Test is skipped if unset.
    std::env::var("XVN_OHLCV_FIXTURE").ok()
}

#[tokio::test]
async fn ohlcv_tool_round_trips_through_sidecar() {
    let bin = agentd_bin();
    if !bin.exists() {
        eprintln!("skipping: build xvision-agentd first");
        return;
    }
    let Some(fixture) = fixture_name() else {
        eprintln!("skipping: XVN_OHLCV_FIXTURE not set");
        return;
    };

    let dir = TempDir::new().unwrap();
    let sock = dir.path().join("sock");
    let cb_sock = dir.path().join("cb-sock");

    let registry = Arc::new(ToolRegistry::default_with_builtins());
    let dispatch: Arc<dyn ToolDispatch> = Arc::new(EngineRegistryDispatch(registry));
    let client = AgentClient::spawn_with_callbacks(&bin, &sock, &cb_sock, dispatch)
        .await
        .expect("spawn");

    let input = serde_json::json!({
        "asset": "BTC/USD",
        "fixture": fixture,
        "lookback_bars": 10,
    });
    let out = client
        .invoke_tool_via_sidecar("ohlcv", input)
        .await
        .expect("invoke ohlcv");

    assert_eq!(out["asset"], "BTC/USD");
    assert!(out["bars"].is_array());
    assert!(out["bars"].as_array().unwrap().len() <= 10);

    client.shutdown().await.unwrap();
}
```

Also ensure `async-trait` is in `[dev-dependencies]` of `crates/xvision-agent-client/Cargo.toml` (the runtime crate already has it).

- [ ] **Step 9: Run, expect fail (missing fixture or unbuilt sidecar)**

```bash
cd /Users/edkennedy/Code/xvision
cargo test -p xvision-agent-client e2e_ohlcv_callback
```

Expected: compiles. Test prints a "skipping" line if the sidecar isn't built or the fixture env var isn't set. The next step exercises it for real.

- [ ] **Step 10: Find a fixture and run the test**

```bash
ls /Users/edkennedy/Code/xvision/data/probes/ 2>/dev/null
ls /Users/edkennedy/Code/xvision/crates/xvision-data/fixtures/ 2>/dev/null
```

Pick a fixture name the existing `xvision_data::fixtures::load_ohlcv_fixture` knows about (inspect `xvision-data/src/fixtures.rs` to confirm). Then:

```bash
cd /Users/edkennedy/Code/xvision
XVN_OHLCV_FIXTURE=<fixture-name> cargo test -p xvision-agent-client e2e_ohlcv_callback -- --nocapture
```

Expected: pass.

- [ ] **Step 11: Run the whole client crate**

```bash
cargo test -p xvision-agent-client
```

Expected: all green (skipped tests log skip reasons).

- [ ] **Step 12: Commit**

```bash
git add crates/xvision-agent-client/ xvision-agentd/
git commit -m "feat(agent-client,agentd): tool callback round-trip via callback socket"
```

---

## Task 10: Dockerfile.deploy — bundle the sidecar

**Files:**
- Modify: `Dockerfile.deploy`

**Goal:** The deploy image carries Node 22 + the built sidecar. `xvn` can still run without it (Wave 1 doesn't switch any code paths yet), so the addition is purely additive.

- [ ] **Step 1: Inspect current Dockerfile.deploy**

```bash
cat /Users/edkennedy/Code/xvision/Dockerfile.deploy
```

Note the build stages, base image, and final `FROM` so the additions slot in cleanly.

- [ ] **Step 2: Add a sidecar build stage**

Add this stage near the top, before any other build stages (adjust positioning to fit the existing file):

```dockerfile
# ----- xvision-agentd build stage -----
FROM node:22-alpine AS agentd-build
WORKDIR /agentd
RUN corepack enable && corepack prepare pnpm@9.0.0 --activate
COPY xvision-agentd/package.json xvision-agentd/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY xvision-agentd/ ./
RUN pnpm build
RUN pnpm prune --prod
```

- [ ] **Step 3: Add Node 22 + sidecar to the runtime image**

In the final `FROM` stage of `Dockerfile.deploy`, add:

```dockerfile
RUN apk add --no-cache nodejs=~22 || \
    (apt-get update && apt-get install -y --no-install-recommends nodejs && rm -rf /var/lib/apt/lists/*)
# (Use the apk OR apt branch matching the runtime base image.)
COPY --from=agentd-build /agentd/dist /opt/xvision-agentd/dist
COPY --from=agentd-build /agentd/node_modules /opt/xvision-agentd/node_modules
COPY --from=agentd-build /agentd/package.json /opt/xvision-agentd/package.json
ENV XVN_AGENTD_BIN=/opt/xvision-agentd/dist/index.js
```

If the existing runtime base is `gcr.io/distroless/cc` or similar, use a small intermediate image that bundles a static Node binary instead — distroless can't `apk add`. Capture the choice in a one-line comment near the COPY block.

- [ ] **Step 4: Build the image to verify**

```bash
cd /Users/edkennedy/Code/xvision
scripts/deploy-image.sh
```

Expected: success. Per repo CLAUDE.md, **do not run cargo on deploy hosts** — this command is only valid on a local build host.

- [ ] **Step 5: Smoke-check inside the image**

```bash
docker run --rm -it xvision:deploy-$(git rev-parse --short HEAD) sh -c "node /opt/xvision-agentd/dist/index.js --version"
```

Expected: `{"protocol_version":"0.1.0","sidecar_version":"0.1.0"}`.

- [ ] **Step 6: Commit**

```bash
git add Dockerfile.deploy
git commit -m "build(deploy): bundle xvision-agentd in deploy image"
```

---

## Wave 1 acceptance gate

After Task 10, the branch should:

- [ ] Pass `cargo test --workspace`
- [ ] Pass `cd xvision-agentd && pnpm test`
- [ ] Pass `cargo deny check`
- [ ] Pass `cargo license` against the deny.toml allow-list
- [ ] Pass `pnpm --dir xvision-agentd dlx license-checker --production --onlyAllow "Apache-2.0;MIT;BSD-2-Clause;BSD-3-Clause;ISC;0BSD;CC0-1.0;Python-2.0;Unlicense"`
- [ ] `scripts/deploy-image.sh` produce an image whose `node /opt/xvision-agentd/dist/index.js --version` returns Wave-1 versions

If any of these fail, fix the root cause before opening the PR. Per project memory (`feedback_alpha_root_cause`), do not silence failures with try/catch or allow-list extensions.

## What Wave 2+ will plug into this scaffold

(For context only — not Wave 1 work.)

- Replace the in-memory tool registry with a real one and use it to register Cline custom tools (Wave 2).
- Add `session.start_run` / `session.step` / `session.end_run` methods, threading scoped provider credentials per run (Wave 2).
- Wire `@cline/sdk` Agent instantiation per session, with allow-listed tools (Wave 2).
- Add `submit_decision` as a Cline custom tool replacing schema-injected system prompts (Wave 3).
- Switch one eval call site over end-to-end (Wave 3).
- Move provider capability matrix into the protocol (Wave 3).
- Observability convergence: route sidecar events into the Rust event bus → SQLite spans + OTel (Wave 4).
- Delete `crates/xvision-engine/src/agent/` (Wave 5).
