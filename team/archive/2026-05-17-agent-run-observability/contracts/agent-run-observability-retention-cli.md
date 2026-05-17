---
track: agent-run-observability-retention-cli
lane: leaf
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-retention-cli
branch: task/agent-run-observability-retention-cli
base: origin/main
status: ready
depends_on:
  - agent-run-observability-schema
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/obs/**
  - crates/xvision-cli/src/commands/mod.rs    # subcommand registration only
  - crates/xvision-observability/src/retention.rs
  - crates/xvision-observability/src/janitor.rs
  - crates/xvision-observability/tests/retention_*.rs
forbidden_paths:
  - crates/xvision-engine/src/agent/**
  - crates/xvision-agent-client/**
  - xvision-agentd/**
  - frontend/web/src/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - Config loader from `agent-run-observability-schema`
  - Blob store from `agent-run-observability-schema`
  - clap (workspace dep)
parallel_safe: true
parallel_conflicts:
  - multi-owner row in OWNERSHIP for `crates/xvision-cli/src/commands/mod.rs` (registration line only)
verification:
  - cargo build -p xvision-cli
  - cargo test -p xvision-observability --test retention_precedence
  - cargo test -p xvision-observability --test janitor
  - xvn obs retention show --help
acceptance:
  - `xvn obs retention show` prints the resolved retention mode + each toggle, plus where each value came from (CLI flag / env / config / default).
  - `xvn obs retention set --mode hash_only|redacted|full_debug` writes the value to `$XVN_HOME/config/observability.toml`. `full_debug` also writes the dashboard-banner sentinel file (consumed by the UI later in Phase B; for now just ensures the file exists).
  - `xvn obs retention clear` removes the file (returns to defaults).
  - **Startup WARN line** emitted by the config loader (in `xvision-observability`) when `mode == "full_debug"`, exactly as specified in the plan. Test asserts the WARN appears in captured tracing output.
  - **Precedence chain** test: `CLI flag > env var > config file > default`. Each layer covered.
  - Janitor task deletes blob refs older than `payload_ttl_days` and truncates rows beyond `max_payload_bytes`. Janitor is wired to a periodic tokio task that can be started independently or via a new `xvn obs janitor run --once` invocation for ops use.
  - Janitor leaves the row hashes intact when it deletes a blob — only the `*_payload_ref` is nulled.
---

# Scope

Phase A leaf #3 of the agent-run-observability wave. CLI + janitor for the
retention policy. Independent of the event bus (only depends on schema crate
+ config loader). Useful before any rows exist, because it stabilises the
config surface that Phase B leaves will read.

# Out of scope

- The `--retention` flag on `xvn eval run` — that wires up in Phase B once
  emission lands and there are actual runs to apply retention to.
- Anything UI-side. The dashboard `full_debug` banner is consumed in the
  Phase B UI leaf; this leaf only writes the sentinel file.
- OTel toggle CLI surface — covered separately in the `otel-bridge` leaf.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-retention-cli \
  -b task/agent-run-observability-retention-cli origin/main
```

# Notes

- Subcommand registration in `crates/xvision-cli/src/commands/mod.rs` is the
  only line this leaf touches in that file. Treat it like the V2A
  multi-owner exemption: one-line addition, no refactor.
- The startup WARN line is the contract; capture it via a tracing
  test-subscriber and grep for `full_debug retention enabled`.
- The janitor's "delete oldest blob first" tie-breaker is by SHA hex sort
  for determinism (the blob ref's mtime can be unreliable on copied
  workspaces).
