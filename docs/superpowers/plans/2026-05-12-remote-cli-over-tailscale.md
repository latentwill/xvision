# Remote CLI Over Tailscale Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an async, shell-free remote CLI job surface under `xvision-dashboard` so external agents can submit `xvn` argv over the existing Tailscale-served HTTPS endpoint and stream or poll results.

**Architecture:** Extend `xvision-dashboard` with a new `/api/cli/jobs*` route family backed by a SQLite job store and a background runner that spawns the local `xvn` binary directly. Persist job metadata and output chunks so clients can reconnect, and expose lifecycle/output over both JSON and SSE.

**Tech Stack:** Rust, axum, tokio, sqlx/sqlite, axum-test, Server-Sent Events

---

## File map

- Create: `crates/xvision-engine/migrations/013_cli_jobs.sql`
- Create: `crates/xvision-dashboard/src/cli_jobs/mod.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/model.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/store.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/runner.rs`
- Create: `crates/xvision-dashboard/src/routes/cli.rs`
- Create: `crates/xvision-dashboard/tests/cli_jobs_routes.rs`
- Modify: `crates/xvision-dashboard/src/lib.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Modify: `crates/xvision-dashboard/src/state.rs`
- Modify: `crates/xvision-dashboard/src/error.rs`
- Modify: `crates/xvision-engine/src/api/mod.rs`

### Task 1: Persist CLI jobs and chunks

**Files:**
- Create: `crates/xvision-engine/migrations/013_cli_jobs.sql`
- Modify: `crates/xvision-engine/src/api/mod.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/model.rs`
- Create: `crates/xvision-dashboard/src/cli_jobs/store.rs`
- Test: `crates/xvision-dashboard/tests/cli_jobs_routes.rs`

- [ ] **Step 1: Write the failing persistence test**

Add an integration test that boots the dashboard and asserts `POST /api/cli/jobs` persists a queued job row and `GET /api/cli/jobs/:id` returns it:

```rust
#[tokio::test]
async fn create_job_persists_queued_row() {
    let (server, _tmp) = boot().await;

    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["help"],
            "timeout_secs": 30
        }))
        .await;

    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    let job_id = body["job_id"].as_str().expect("job_id");

    let get = server.get(&format!("/api/cli/jobs/{job_id}")).await;
    get.assert_status_ok();
    let meta: serde_json::Value = get.json();
    assert_eq!(meta["status"], "queued");
    assert_eq!(meta["argv"], serde_json::json!(["help"]));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xvision-dashboard create_job_persists_queued_row -- --nocapture`

Expected: FAIL with route not found or missing `cli_jobs` persistence code.

- [ ] **Step 3: Add the migration and store skeleton**

Create the migration tables and enough Rust types/store code to represent queued jobs:

```sql
CREATE TABLE IF NOT EXISTS cli_jobs (
    job_id TEXT PRIMARY KEY,
    argv_json TEXT NOT NULL,
    status TEXT NOT NULL,
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
    stream TEXT NOT NULL,
    chunk_index INTEGER NOT NULL,
    byte_offset INTEGER NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (job_id, stream, chunk_index),
    FOREIGN KEY (job_id) REFERENCES cli_jobs(job_id) ON DELETE CASCADE
);
```

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CliJobStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Cancelled,
}
```

- [ ] **Step 4: Wire the new migration into startup**

Add the new migration constant in `crates/xvision-engine/src/api/mod.rs` and execute it in `ApiContext::open`, so both CLI and dashboard startup apply it.

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p xvision-dashboard create_job_persists_queued_row -- --nocapture`

Expected: PASS

### Task 2: Add route validation and metadata/output endpoints

**Files:**
- Create: `crates/xvision-dashboard/src/routes/cli.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Modify: `crates/xvision-dashboard/src/error.rs`
- Test: `crates/xvision-dashboard/tests/cli_jobs_routes.rs`

- [ ] **Step 1: Write failing validation tests**

Add tests for empty argv and denied subcommands:

```rust
#[tokio::test]
async fn create_job_rejects_empty_argv() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": [] }))
        .await;
    response.assert_status_bad_request();
    let body: serde_json::Value = response.json();
    assert_eq!(body["code"], "validation");
}

#[tokio::test]
async fn create_job_rejects_dashboard_subcommand() {
    let (server, _tmp) = boot().await;
    let response = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["dashboard", "serve"] }))
        .await;
    response.assert_status_bad_request();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p xvision-dashboard create_job_rejects_ -- --nocapture`

Expected: FAIL until `/api/cli/jobs` validation exists.

- [ ] **Step 3: Implement request/response types and route handlers**

Add request validation and these handlers:

- `POST /api/cli/jobs`
- `GET /api/cli/jobs/:id`
- `GET /api/cli/jobs/:id/output`
- `POST /api/cli/jobs/:id/cancel`

Validation rules:

```rust
if body.argv.is_empty() {
    return Err(DashboardError::Validation {
        field: "argv".into(),
        msg: "must contain at least one argument".into(),
    });
}
if matches!(body.argv.first().map(String::as_str), Some("dashboard" | "mcp")) {
    return Err(DashboardError::Validation {
        field: "argv".into(),
        msg: "subcommand is not allowed over remote cli".into(),
    });
}
```

- [ ] **Step 4: Run the validation tests**

Run: `cargo test -p xvision-dashboard create_job_rejects_ -- --nocapture`

Expected: PASS

### Task 3: Add the background runner and terminal job completion

**Files:**
- Create: `crates/xvision-dashboard/src/cli_jobs/runner.rs`
- Modify: `crates/xvision-dashboard/src/cli_jobs/mod.rs`
- Modify: `crates/xvision-dashboard/src/state.rs`
- Modify: `crates/xvision-dashboard/src/routes/cli.rs`
- Test: `crates/xvision-dashboard/tests/cli_jobs_routes.rs`

- [ ] **Step 1: Write the failing execution test**

Add a test that submits `["help"]` and eventually sees terminal output:

```rust
#[tokio::test]
async fn create_job_runs_xvn_and_captures_output() {
    let (server, _tmp) = boot().await;
    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({ "argv": ["help"], "timeout_secs": 30 }))
        .await;
    create.assert_status_ok();
    let body: serde_json::Value = create.json();
    let job_id = body["job_id"].as_str().unwrap();

    for _ in 0..100 {
        let meta = server.get(&format!("/api/cli/jobs/{job_id}")).await;
        meta.assert_status_ok();
        let json: serde_json::Value = meta.json();
        if json["status"] == "succeeded" || json["status"] == "failed" {
            let out = server.get(&format!("/api/cli/jobs/{job_id}/output")).await;
            out.assert_status_ok();
            let payload: serde_json::Value = out.json();
            assert!(payload["stdout"].as_str().unwrap_or("").contains("Usage: xvn"));
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }

    panic!("job did not reach terminal status");
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p xvision-dashboard create_job_runs_xvn_and_captures_output -- --nocapture`

Expected: FAIL because jobs never leave `queued`.

- [ ] **Step 3: Implement the runner**

Runner responsibilities:

- spawn `xvn` directly with `tokio::process::Command`
- set stdin to null, stdout/stderr to piped
- update `started_at` / `status`
- read stdout/stderr concurrently
- append output chunks and byte counters
- set terminal status + exit code

Spawn shape:

```rust
let mut child = tokio::process::Command::new("xvn")
    .args(&job.argv)
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped())
    .spawn()?;
```

- [ ] **Step 4: Run the execution test**

Run: `cargo test -p xvision-dashboard create_job_runs_xvn_and_captures_output -- --nocapture`

Expected: PASS

### Task 4: Add timeout, cancellation, and SSE events

**Files:**
- Modify: `crates/xvision-dashboard/src/cli_jobs/runner.rs`
- Modify: `crates/xvision-dashboard/src/routes/cli.rs`
- Modify: `crates/xvision-dashboard/src/state.rs`
- Test: `crates/xvision-dashboard/tests/cli_jobs_routes.rs`

- [ ] **Step 1: Write the failing timeout and SSE tests**

Add one test that uses a deliberately tiny timeout and one test that connects to `/api/cli/jobs/:id/events`:

```rust
#[tokio::test]
async fn job_timeout_marks_timed_out_status() {
    let (server, _tmp) = boot().await;
    let create = server
        .post("/api/cli/jobs")
        .json(&serde_json::json!({
            "argv": ["ab-compare", "--help"],
            "timeout_secs": 0
        }))
        .await;
    create.assert_status_bad_request();
}
```

```rust
#[tokio::test]
async fn sse_stream_emits_job_finished_event() {
    // Boot real TCP server, submit a help job, then assert the SSE body
    // contains `job_started` and `job_finished`.
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p xvision-dashboard job_timeout_marks_timed_out_status sse_stream_emits_job_finished_event -- --nocapture`

Expected: FAIL until timeout/SSE support exists.

- [ ] **Step 3: Implement timeout, cancel, and event publishing**

Add:

- timeout cap validation
- runner timeout enforcement with terminate then kill-after-grace
- `cancel_requested` handling
- an in-process event bus keyed by `job_id`
- SSE event serialization for `job_started`, `stdout_chunk`, `stderr_chunk`, `job_finished`, `job_timed_out`, `job_cancelled`

- [ ] **Step 4: Run the timeout and SSE tests**

Run: `cargo test -p xvision-dashboard job_timeout_marks_timed_out_status sse_stream_emits_job_finished_event -- --nocapture`

Expected: PASS

### Task 5: Full dashboard verification

**Files:**
- Test: `crates/xvision-dashboard/tests/cli_jobs_routes.rs`
- Verify: `crates/xvision-dashboard/src/routes/cli.rs`
- Verify: `crates/xvision-dashboard/src/cli_jobs/*`

- [ ] **Step 1: Run the focused dashboard integration suite**

Run: `cargo test -p xvision-dashboard cli_jobs_ -- --nocapture`

Expected: all new remote-cli tests PASS

- [ ] **Step 2: Run the broader dashboard suite**

Run: `cargo test -p xvision-dashboard -- --nocapture`

Expected: existing dashboard tests remain green

- [ ] **Step 3: Run a compile-level verification on the workspace slice**

Run: `cargo test -p xvision-engine --test api_context -- --nocapture`

Expected: PASS, confirming the new migration wiring did not break engine startup

