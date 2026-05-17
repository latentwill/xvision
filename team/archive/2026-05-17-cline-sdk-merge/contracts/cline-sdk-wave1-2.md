---
track: cline-sdk-wave1-2
lane: foundation
wave: cline-sdk-agent-replacement
worktree: (removed post-merge)
branch: task/cline-sdk-wave1-impl (deleted post-merge)
base: origin/main
status: merged
pr: https://github.com/latentwill/xvision/pull/208
merge_commit: 7365cc57f592f121283e9ced60220234a8bea980
depends_on: []
blocks: [cline-sdk-wave3]
stacking: none
allowed_paths:
  - xvision-agentd/**
  - crates/xvision-agent-client/**
  - docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md
  - docs/superpowers/plans/2026-05-17-cline-sdk-agent-replacement-wave1.md
  - docs/superpowers/plans/2026-05-17-cline-sdk-agent-replacement-wave2.md
  - docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md
  - Dockerfile.deploy
  - .dockerignore
forbidden_paths:
  - crates/xvision-engine/src/agent/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - "@cline/sdk Agent / AgentTool / AgentModel / AgentRuntimeConfigWithModel"
  - xvision-agent-client::AgentClient {start_run, step, end_run}
  - tool callback UDS socket protocol (Wave 1)
parallel_safe: true
parallel_conflicts: []
verification:
  - pnpm --dir xvision-agentd test
  - pnpm --dir xvision-agentd typecheck
  - pnpm --dir xvision-agentd build
  - cargo test -p xvision-agent-client
  - XVISION_RUN_SIDECAR_TESTS=1 cargo test -p xvision-agent-client --test session_lifecycle
acceptance:
  - Sidecar JSON-RPC server up, version handshake passes, tool registry handshake passes
  - End-to-end start_run / step / end_run round-trips through real Cline Agent
  - Tool callback round-trip exercised by integration test (real sidecar, real Cline Agent, real tool)
  - Mock provider available for deterministic CI via AgentRuntimeConfigWithModel
  - Wave 3 follow-ups (submit_decision lifecycle tool, real eval call site swap, event-bus routing, MCP, skills) explicitly deferred
---

# Scope

Replaces the in-Rust agent loop (`crates/xvision-engine/src/agent/`) with a
Node sidecar that hosts `@cline/sdk`'s `Agent` runtime. Rust remains the source
of truth (run ledger, cycle ids, tool implementations, credentials, scenario
replay); the sidecar is a thin adapter that translates between Rust's
canonical protocol and Cline's API.

Wave 1 = sidecar scaffold + JSON-RPC server + Rust client crate + supervisor +
version handshake + tool registry handshake + tool callback round-trip via
callback socket + deploy image bundling. Wave 2 = real `@cline/sdk`
integration, `session.start_run` / `session.step` / `session.end_run` methods,
tool shim translating Wave-1 descriptors into Cline `AgentTool[]`, mock
`AgentModel` for deterministic CI, Rust-side `AgentClient` methods,
end-to-end integration test.

Implements:

- Spec: `docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md`
- Plan (wave 1): `docs/superpowers/plans/2026-05-17-cline-sdk-agent-replacement-wave1.md`
- Plan (wave 2): `docs/superpowers/plans/2026-05-17-cline-sdk-agent-replacement-wave2.md`

PR: <https://github.com/latentwill/xvision/pull/208> (55 files, +11.9k).

# Out of scope

- Deleting `crates/xvision-engine/src/agent/**` — the in-Rust agent loop
  stays live until a real eval call site is moved over in Wave 3.
- New migrations.
- Licensing baseline (LICENSE / NOTICE / CONTRIBUTING / SECURITY /
  CODE_OF_CONDUCT / THIRD_PARTY_LICENSES / cargo-deny / license-checker / CI
  workflow). Tracked in
  `docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md` as F1–F4
  and explicitly deferred by direction.
- `submit_decision` as a Cline custom tool with `lifecycle.completesRun =
  true`. Wave 3.
- Routing `agent.subscribe()` events into the Rust event bus → SQLite spans
  / OTel export. Wave 3 (integrates with the agent-run-observability wave).
- MCP server config flow, skills runtime, streaming text deltas in the
  dashboard. Wave 3+.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/cline-sdk-wave1 status
git -C .worktrees/cline-sdk-wave1 log --oneline -5 origin/main..HEAD
```

# Notes

- PR #199 (DRAFT spec) is superseded by this PR; close after #208 merges.
- Deploy-image runtime check is deferred to next push from a Docker host:
  `scripts/deploy-image.sh && docker run <tag> node /opt/xvision-agentd/dist/index.js --version`
  should report `cline_sdk_version: "0.0.41"`.
- `Agent` creates an isolated provider gateway per construction. Mock-provider
  path branches on `provider_id === MOCK_PROVIDER_ID` and injects a `model:
  AgentModel` via `AgentRuntimeConfigWithModel`. Production providers still
  use the normal `providerId/modelId/apiKey/baseUrl` path. See
  `xvision-agentd/src/session/build-agent.ts`.
- `AgentModel`/`AgentModelEvent`/`AgentModelRequest` types are not reachable
  via `@cline/sdk` or `@cline/shared` public exports under
  `moduleResolution: NodeNext` — the mock model defines structural
  local interfaces.
