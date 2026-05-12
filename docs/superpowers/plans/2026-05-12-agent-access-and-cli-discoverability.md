# Agent Access and CLI Discoverability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make xvision operable by external and embedded agents through clear repo documentation, discoverable skill guidance, accurate CLI docs, and a first remote-control surface over the existing Tailscale-served dashboard node.

**Architecture:** Split the work into two layers. First, repair the documentation/discoverability surface so GitHub readers, Claude Code users, and CLI users all get the same story about skills and commands. Then add the first executable remote-control slice inside `xvision-dashboard`: a shell-free job API that runs typed `xvn` argv arrays on the local Tailscale-served node and persists job/output state in the existing SQLite database.

**Tech Stack:** Markdown docs (`README.md`, `MANUAL.md`, `.claude/skills/*`, crate READMEs), Rust workspace (`xvision-cli`, `xvision-dashboard`, `xvision-engine`), Axum, SQLx/SQLite migrations, async process management with Tokio, dashboard integration tests, `cargo`.

---

## File Structure

- `README.md` — new GitHub-facing agent/operator overview.
- `.claude/skills/README.md` and `.claude/skills/xvision/SKILL.md` — repo-local skill discoverability and examples for Claude Code users.
- `.claude/skills/xvision/references/cli.md` — canonical agent-facing CLI examples; must match the real surface.
- `crates/xvision-skills/README.md` and `crates/xvision-engine/README.md` — clarify xvision-internal runtime skills vs Claude Code repo skills.
- `crates/xvision-cli/src/lib.rs` and `crates/xvision-cli/src/commands/eval.rs` — fix stale help/comments that still describe `eval run` as deferred.
- `crates/xvision-dashboard/src/routes/cli.rs` — new remote CLI HTTP surface.
- `crates/xvision-dashboard/src/cli_jobs/*` — job runner, persistence helpers, and SSE fanout for remote CLI jobs.
- `crates/xvision-dashboard/src/server.rs` — mount the new `/api/cli/jobs*` route family.
- `crates/xvision-engine/migrations/013_cli_jobs.sql` — add persistent job/output tables for reconnectable remote execution.
- `crates/xvision-dashboard/tests/http.rs` — cover create/status/output/cancel paths against a tempdir-backed dashboard.

## Task 1: Add the Root README Agent Overview and Skill Entry Points

**Files:**
- Modify: `README.md`
- Modify: `.claude/skills/README.md`
- Modify: `.claude/skills/xvision/SKILL.md`
- Modify: `crates/xvision-skills/README.md`

- [ ] **Step 1: Write the failing documentation smoke test**

Create `scripts/check_agent_docs.sh` so the repo can mechanically verify that the root README points agents at the expected entry points:

```bash
#!/usr/bin/env bash
set -euo pipefail

README=README.md

grep -q "## For Agents" "$README"
grep -q "MANUAL.md" "$README"
grep -q "FOLLOWUPS.md" "$README"
grep -q ".claude/skills/xvision/SKILL.md" "$README"
grep -q "xvn --help" "$README"
grep -q "xvn.tail2bb69.ts.net" "$README"
```

- [ ] **Step 2: Run the smoke test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
bash scripts/check_agent_docs.sh
```

Expected: FAIL because the root README does not yet have a dedicated agent overview section.

- [ ] **Step 3: Add a GitHub-facing `For Agents` section to the root README**

In `README.md`, add a section near the top after `What it does NOT do`:

```md
## For Agents

If you are an external or embedded agent using this repo, start here:

1. Read `MANUAL.md` for operator commands and environment assumptions.
2. Read `FOLLOWUPS.md` for active engineering tracks and deferred work.
3. If you are running inside Claude Code rooted in this repo, load `.claude/skills/xvision/SKILL.md`.
4. For exact CLI usage, run `xvn --help` and read `.claude/skills/xvision/references/cli.md`.
5. For live-node remote control, use the Tailscale-served dashboard node (`xvn.tail2bb69.ts.net` or `xvnej.tail2bb69.ts.net`) rather than assuming arbitrary SSH access.
```

Also update the route description:

```md
V1 routes: `/` Dashboard, `/setup` Wizard, `/strategies`, `/authoring/:id`, ...
```

- [ ] **Step 4: Make the skill docs explicit about the two skill systems**

In `.claude/skills/README.md`, keep the existing distinction but add one direct “how to use this repo as an agent” sentence:

```md
If your session is rooted in this repo, Claude Code auto-discovers `.claude/skills/xvision/`; that is the first xvision-specific skill an external coding agent should load.
```

In `crates/xvision-skills/README.md`, add one explicit warning near the top:

```md
These are xvision runtime skills attached to strategy slots via `xvn skill ...`. They are not the same thing as repo-local Claude Code skills under `.claude/skills/`.
```

- [ ] **Step 5: Expand the xvision Claude skill with concrete examples**

In `.claude/skills/xvision/SKILL.md`, add a short examples block:

```md
## High-value examples

- `xvn strategy ls`
- `xvn strategy show <id>`
- `xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest`
- `xvn provider ls`
- `xvn dashboard serve --bind 127.0.0.1:8788`
```

Also add one note that live-node control currently means the Tailscale-served dashboard node, not generic SSH orchestration.

- [ ] **Step 6: Re-run the documentation smoke test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
bash scripts/check_agent_docs.sh
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add README.md .claude/skills/README.md .claude/skills/xvision/SKILL.md crates/xvision-skills/README.md scripts/check_agent_docs.sh
git commit -m "docs(agent): add repo entry points and skill guidance"
```

## Task 2: Repair CLI Discoverability Drift

**Files:**
- Modify: `crates/xvision-cli/src/lib.rs`
- Modify: `crates/xvision-cli/src/commands/eval.rs`
- Modify: `.claude/skills/xvision/references/cli.md`
- Modify: `MANUAL.md`
- Test: `crates/xvision-cli/tests/help_cli.rs`

- [ ] **Step 1: Write the failing CLI/help test**

Create `crates/xvision-cli/tests/help_cli.rs`:

```rust
use std::process::Command;

#[test]
fn top_level_help_and_eval_help_describe_eval_run_as_available() {
    let top = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .arg("--help")
        .output()
        .expect("xvn --help");
    assert!(top.status.success());
    let top_stdout = String::from_utf8(top.stdout).unwrap();
    assert!(top_stdout.contains("Eval"), "top-level help should list eval");

    let eval = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["eval", "--help"])
        .output()
        .expect("xvn eval --help");
    assert!(eval.status.success());
    let eval_stdout = String::from_utf8(eval.stdout).unwrap();
    assert!(eval_stdout.contains("Run an eval"), "eval help should expose run");
    assert!(
        !eval_stdout.contains("deferred to a follow-up"),
        "stale deferred wording must be removed"
    );
}
```

- [ ] **Step 2: Run the focused CLI test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-cli top_level_help_and_eval_help_describe_eval_run_as_available -- --nocapture
```

Expected: FAIL because `crates/xvision-cli/src/commands/eval.rs` still contains stale “deferred to a follow-up” wording.

- [ ] **Step 3: Fix the stale CLI help/comments**

In `crates/xvision-cli/src/commands/eval.rs`, replace the file-level header:

```rust
//! `xvn eval` — launch, browse, inspect, compare, and attest eval runs.
//! `run` is part of the shipped surface and uses the same engine API as
//! the dashboard-backed eval routes.
```

In `crates/xvision-cli/src/lib.rs`, replace:

```rust
/// Browse eval runs and canonical scenarios. (`run` lands in a follow-up.)
```

with:

```rust
/// Launch, browse, compare, and inspect eval runs plus canonical scenarios.
```

- [ ] **Step 4: Update the canonical CLI reference docs**

In `.claude/skills/xvision/references/cli.md`, expand the `eval` section:

```md
## Eval

```bash
xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval list
xvn eval show <run_id>
xvn eval compare <run_id_a> <run_id_b>
```
```

In `MANUAL.md`, add or update the matching examples under the eval section so the operator docs and the skill reference stop diverging.

- [ ] **Step 5: Re-run the CLI test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-cli top_level_help_and_eval_help_describe_eval_run_as_available -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-cli/src/lib.rs crates/xvision-cli/src/commands/eval.rs .claude/skills/xvision/references/cli.md MANUAL.md crates/xvision-cli/tests/help_cli.rs
git commit -m "docs(cli): align help and references with shipped eval surface"
```

## Task 3: Add Persistent Remote CLI Job Tables

**Files:**
- Create: `crates/xvision-engine/migrations/013_cli_jobs.sql`
- Modify: `crates/xvision-engine/src/api/mod.rs`
- Test: `crates/xvision-engine/tests/api_context.rs`

- [ ] **Step 1: Write the failing migration test**

Add to `crates/xvision-engine/tests/api_context.rs`:

```rust
#[tokio::test]
async fn api_context_open_creates_cli_job_tables() {
    let td = tempfile::tempdir().unwrap();
    let ctx = xvision_engine::api::ApiContext::open(
        td.path(),
        xvision_engine::api::Actor::Cli { user: "test".into() },
    )
    .await
    .unwrap();

    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name"
    )
    .fetch_all(&ctx.db)
    .await
    .unwrap();

    assert!(tables.contains(&"cli_jobs".to_string()));
    assert!(tables.contains(&"cli_job_output_chunks".to_string()));
}
```

- [ ] **Step 2: Run the focused migration test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-engine api_context_open_creates_cli_job_tables -- --nocapture
```

Expected: FAIL because migration `013_cli_jobs.sql` does not exist yet.

- [ ] **Step 3: Create the migration**

Create `crates/xvision-engine/migrations/013_cli_jobs.sql`:

```sql
CREATE TABLE IF NOT EXISTS cli_jobs (
  job_id TEXT PRIMARY KEY,
  argv_json TEXT NOT NULL,
  status TEXT NOT NULL CHECK (status IN ('queued','running','succeeded','failed','timed_out','cancelled')),
  created_at TEXT NOT NULL,
  started_at TEXT,
  finished_at TEXT,
  exit_code INTEGER,
  timeout_secs INTEGER NOT NULL,
  timed_out INTEGER NOT NULL DEFAULT 0,
  cancel_requested INTEGER NOT NULL DEFAULT 0,
  stdout_bytes INTEGER NOT NULL DEFAULT 0,
  stderr_bytes INTEGER NOT NULL DEFAULT 0,
  stdout_truncated INTEGER NOT NULL DEFAULT 0,
  stderr_truncated INTEGER NOT NULL DEFAULT 0,
  error_message TEXT
);

CREATE TABLE IF NOT EXISTS cli_job_output_chunks (
  job_id TEXT NOT NULL,
  stream TEXT NOT NULL CHECK (stream IN ('stdout','stderr')),
  chunk_index INTEGER NOT NULL,
  byte_offset INTEGER NOT NULL,
  payload TEXT NOT NULL,
  created_at TEXT NOT NULL,
  PRIMARY KEY (job_id, stream, chunk_index)
);

CREATE INDEX IF NOT EXISTS idx_cli_jobs_status_created_at
  ON cli_jobs(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_cli_job_output_chunks_job_stream
  ON cli_job_output_chunks(job_id, stream, chunk_index);
```

- [ ] **Step 4: Register the migration in the engine API bootstrap**

In `crates/xvision-engine/src/api/mod.rs`, add:

```rust
const MIGRATION_013_CLI_JOBS: &str = include_str!("../../migrations/013_cli_jobs.sql");
```

and include it in the migration list in order after the existing `MIGRATION_012_RUNS_FK`.

- [ ] **Step 5: Re-run the migration test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-engine api_context_open_creates_cli_job_tables -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-engine/migrations/013_cli_jobs.sql crates/xvision-engine/src/api/mod.rs crates/xvision-engine/tests/api_context.rs
git commit -m "feat(remote-cli): add persistent cli job tables"
```

## Task 4: Implement the Remote CLI Job API in the Dashboard

**Files:**
- Create: `crates/xvision-dashboard/src/routes/cli.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/mod.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/store.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/runner.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Test: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 1: Write the failing dashboard HTTP test**

Add to `crates/xvision-dashboard/tests/http.rs`:

```rust
#[tokio::test]
async fn remote_cli_job_can_be_created_and_polled() {
    let (server, _tmp) = boot().await;

    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["eval", "--help"],
            "timeout_secs": 30
        }))
        .await;

    response.assert_status(StatusCode::ACCEPTED);
    let body: serde_json::Value = response.json();
    let job_id = body["job_id"].as_str().unwrap();

    let meta = server.get(&format!("/api/cli/jobs/{job_id}")).await;
    meta.assert_status_ok();
    let meta_body: serde_json::Value = meta.json();
    assert_eq!(meta_body["job_id"], job_id);
    assert!(meta_body["status"].is_string());
}
```

- [ ] **Step 2: Run the focused dashboard test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-dashboard remote_cli_job_can_be_created_and_polled -- --nocapture
```

Expected: FAIL because `/api/cli/jobs` does not exist yet.

- [ ] **Step 3: Implement the persistent store helpers**

Create `crates/xvision-dashboard/src/cli_jobs/store.rs` with a narrow typed store over the new tables:

```rust
pub struct CliJobStore {
    pool: sqlx::SqlitePool,
}

impl CliJobStore {
    pub fn new(pool: sqlx::SqlitePool) -> Self { Self { pool } }

    pub async fn create_job(&self, job_id: &str, argv_json: &str, timeout_secs: i64) -> anyhow::Result<()> { /* insert queued row */ }
    pub async fn mark_running(&self, job_id: &str) -> anyhow::Result<()> { /* update status + started_at */ }
    pub async fn append_chunk(&self, job_id: &str, stream: &str, chunk_index: i64, byte_offset: i64, payload: &str) -> anyhow::Result<()> { /* insert chunk + update byte counters */ }
    pub async fn finish_job(&self, job_id: &str, status: &str, exit_code: Option<i64>, error_message: Option<&str>) -> anyhow::Result<()> { /* terminal update */ }
    pub async fn request_cancel(&self, job_id: &str) -> anyhow::Result<()> { /* set cancel_requested = 1 */ }
}
```

- [ ] **Step 4: Implement the runner**

Create `crates/xvision-dashboard/src/cli_jobs/runner.rs` that spawns the local `xvn` binary directly:

```rust
let mut child = tokio::process::Command::new(std::env::current_exe()?.with_file_name("xvn"))
    .args(argv)
    .current_dir(&xvn_home)
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()?;
```

Guardrails for v1:

```rust
fn validate_argv(argv: &[String]) -> anyhow::Result<()> {
    if argv.is_empty() {
        anyhow::bail!("argv must be non-empty");
    }
    match argv[0].as_str() {
        "dashboard" | "mcp" => anyhow::bail!("subcommand is not allowed through remote cli"),
        _ => Ok(()),
    }
}
```

On completion, persist `succeeded`, `failed`, or `timed_out`. Keep output chunking simple: read UTF-8 lossy text from stdout/stderr and append fixed-size chunks to `cli_job_output_chunks`.

- [ ] **Step 5: Expose the HTTP routes**

Create `crates/xvision-dashboard/src/routes/cli.rs` with these handlers:

```rust
pub async fn create_job(...) -> Result<(StatusCode, Json<CreateJobOut>), DashboardError> { /* validate + persist + spawn */ }
pub async fn get_job(...) -> Result<Json<JobMetaOut>, DashboardError> { /* read cli_jobs row */ }
pub async fn get_output(...) -> Result<Json<JobOutputOut>, DashboardError> { /* assemble stdout/stderr */ }
pub async fn cancel_job(...) -> Result<Json<JobMetaOut>, DashboardError> { /* set cancel_requested */ }
```

Wire them in `crates/xvision-dashboard/src/server.rs`:

```rust
.route("/api/cli/jobs", post(cli::create_job))
.route("/api/cli/jobs/:id", get(cli::get_job))
.route("/api/cli/jobs/:id/output", get(cli::get_output))
.route("/api/cli/jobs/:id/cancel", post(cli::cancel_job))
```

Use `StatusCode::ACCEPTED` for create and cancel.

- [ ] **Step 6: Re-run the focused dashboard test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
cargo test -p xvision-dashboard remote_cli_job_can_be_created_and_polled -- --nocapture
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-dashboard/src/routes/cli.rs crates/xvision-dashboard/src/cli_jobs/mod.rs crates/xvision-dashboard/src/cli_jobs/store.rs crates/xvision-dashboard/src/cli_jobs/runner.rs crates/xvision-dashboard/src/routes/mod.rs crates/xvision-dashboard/src/server.rs crates/xvision-dashboard/tests/http.rs
git commit -m "feat(remote-cli): add dashboard job api for typed xvn execution"
```

## Task 5: Document the Tailscale-Only Remote Path and the SSH Follow-up Boundary

**Files:**
- Modify: `README.md`
- Modify: `.claude/skills/xvision/SKILL.md`
- Modify: `.claude/skills/xvision/references/deploy.md`
- Modify: `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md`

- [ ] **Step 1: Write the failing documentation smoke test**

Extend `scripts/check_agent_docs.sh`:

```bash
grep -q "tailscale-only" README.md
grep -q "not arbitrary SSH" README.md
grep -q "future connection/security" docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md
```

- [ ] **Step 2: Run the smoke test to verify failure**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
bash scripts/check_agent_docs.sh
```

Expected: FAIL because the root README and remote-cli spec do not yet explicitly capture the “SSH-originated requirement narrowed to tailscale-only” boundary.

- [ ] **Step 3: Add the boundary note to the root README**

In `README.md`, add one short note in the new agent section:

```md
Remote agent control is tailscale-only in the current design. Use the Tailscale-served dashboard node and its typed `xvn` job surface; do not assume arbitrary SSH access is the supported long-term control path.
```

- [ ] **Step 4: Update the xvision skill and deploy reference**

In `.claude/skills/xvision/SKILL.md`, add:

```md
If a task targets the live node, prefer the Tailscale-served control surface. Earlier operator habits may mention SSH, but that is not the primary path this repo is standardizing on.
```

In `.claude/skills/xvision/references/deploy.md`, add one small section that names the current node endpoints and points to the remote CLI design/spec.

- [ ] **Step 5: Add the explicit follow-up section to the remote CLI design**

In `docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md`, add a short section near the follow-ups:

```md
## SSH-origin follow-up

This work item started from the need to tell an external agent how to reach the server, which often meant “use SSH”. For v1 we intentionally narrow that to the Tailscale-served node and a typed remote CLI wrapper. Broader host-to-host access, stronger auth, capability tokens, rate limiting, and any future generic remote-host story remain follow-up connection/security work.
```

- [ ] **Step 6: Re-run the documentation smoke test**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
bash scripts/check_agent_docs.sh
```

Expected: PASS.

- [ ] **Step 7: Run final verification**

Run:

```bash
cd /root/deploy/xvision/.worktrees/codex-qa-pass-4
bash scripts/check_agent_docs.sh
cargo test -p xvision-cli top_level_help_and_eval_help_describe_eval_run_as_available -- --nocapture
cargo test -p xvision-engine api_context_open_creates_cli_job_tables -- --nocapture
cargo test -p xvision-dashboard remote_cli_job_can_be_created_and_polled -- --nocapture
```

Expected: all focused checks pass.

- [ ] **Step 8: Commit**

```bash
git add README.md .claude/skills/xvision/SKILL.md .claude/skills/xvision/references/deploy.md docs/superpowers/specs/2026-05-12-remote-cli-over-tailscale-design.md scripts/check_agent_docs.sh
git commit -m "docs(remote-cli): clarify tailscale-only path and ssh follow-up boundary"
```

## Self-Review

- [ ] **Spec coverage:** Task 1 covers the GitHub README and skill entry points, Task 2 covers CLI discoverability drift, Task 3 adds the persistent remote-cli data model, Task 4 implements the executable dashboard job API, and Task 5 captures the tailscale-only/SSH boundary note. No approved spec requirement is left without a task.
- [ ] **Placeholder scan:** Search this file for unfinished markers, vague future-tense instructions, or undefined route/module names before execution.
- [ ] **Type consistency:** Keep route names `/api/cli/jobs`, `/api/cli/jobs/:id`, `/api/cli/jobs/:id/output`, and `/api/cli/jobs/:id/cancel` identical across the migration, route module, tests, and README copy.
