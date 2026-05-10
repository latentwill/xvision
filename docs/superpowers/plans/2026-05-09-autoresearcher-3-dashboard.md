# Autoresearcher AR-3 — Dashboard (5 views) + SSE + Mutator-Skill Ladder UI

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Spec:** `docs/superpowers/specs/2026-05-09-karpathy-autoresearcher-design.md` — full design context. This plan implements **§9 (dashboard surfaces)** end-to-end.
> **Companion plans:** AR-1 (mutator + lineage + gate + seal — must ship), AR-2 (cycle orchestrator + judge + canary + inversion + diversity — must ship), MP-1 (marketplace plugin; adds a sixth tab post-AR-3).
> **Hard upstream dependencies:**
>   1. **AR-1 + AR-2 must be on `main`.** AR-3 consumes `xvision_engine::autoresearch::progress::{ProgressChannel, AutoresearchEvent}` and reads from the SQLite tables introduced by migrations 003/004. Verify before starting: `git log autoresearch-ar2..HEAD --oneline` shows the AR-2 tag is reachable.
>   2. **A live `evening-cycle` invocation drives the live view.** The dashboard tails the `ProgressChannel` shared with whichever process is running the cycle — typically the same `xvn autoresearch evening-cycle` daemon, but for development we run them in two terminals against the same SQLite + blob root.
> **Hackathon role:** Wk 4 milestone (autoresearch spec §10): "Dashboard: all 5 core views. SSE event flow. Mutator-skill ladder. Dashboard renders live cycle in real time." This plan is exactly that scope.

**Goal:** After this plan ships, `xvn dashboard serve` boots a local axum server at `http://localhost:7777` that renders the five core autoresearch views (live cycle viewer, genealogy tree, mutation diff inspector, mutator-skill ladder, ladder-with-provenance) with real-time SSE updates from `ProgressChannel`. Clicking a node in the genealogy tree opens the diff inspector. The page reads on a projector at five meters (autoresearch spec §9). The marketplace plugin's tab is *not* added here — MP-1 adds it as the sixth tab.

**Architecture:** New crate `crates/xvision-dashboard/` with `axum` for the server + `tower-http` for static asset serving. Frontend is a single static SPA (vanilla HTML + JS + D3 v7 + minimal CSS); no React, no Next.js, no build step beyond `cargo build`. SSE events arrive on `/api/events` and are dispatched into client-side handlers per event type. REST endpoints (`/api/lineage`, `/api/lineage/<hash>`, `/api/seals/<cycle_id>`, `/api/ladder/snapshots`, `/api/diversity/samples`, `/api/findings/<bundle_hash>`) read from SQLite. The dashboard process opens the same SQLite database file and the same `ProgressChannel` as the orchestrator — for in-process operation we provide a "combined" mode (`xvn dashboard serve --with-cycle`) that runs both in the same Tokio runtime; for two-process operation we add a Unix domain socket bridge that proxies events into the dashboard's channel.

**Tech Stack:** Rust 2021 + axum 0.7 + tower-http 0.5 + tokio (already workspace-pinned). Frontend: vanilla HTML, vanilla JS (no bundler), D3 v7 (loaded from a CDN with SRI hash, plus a vendored fallback in `static/vendor/d3.v7.min.js` so demos work offline), simple CSS (custom design tokens; no Tailwind to keep zero build deps).

**Out of scope (explicitly deferred):**
- Marketplace tab — MP-1 (sixth tab; this plan ships only the five core tabs)
- Authentication / multi-user / persistence of UI state — single-user local
- Mobile responsive layouts — designed for projector + desktop
- Unit tests for the JS code — we lint with `deno lint` against the static files but don't run a test runner; correctness comes from the Rust-side endpoint tests + manual cross-browser smoke
- Real-time chart smoothing / WebSocket fallback — SSE is sufficient
- Per-node sparklines from realized PnL (we render the field but pull from `metrics_snapshot`; full sparkline computation is in MP-1's lineage manifest renderer)

---

## File structure

```
crates/
└── xvision-dashboard/                              # NEW CRATE
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                                  # public: serve(opts) -> Result
    │   ├── server.rs                               # axum router + state
    │   ├── state.rs                                # AppState (pool, blob store, channel handle)
    │   ├── sse.rs                                  # /api/events handler (bridges ProgressChannel)
    │   ├── api/
    │   │   ├── mod.rs
    │   │   ├── lineage.rs                          # GET /api/lineage, /api/lineage/:hash
    │   │   ├── seals.rs                            # GET /api/seals, /api/seals/:cycle_id
    │   │   ├── ladder.rs                           # GET /api/ladder/snapshots
    │   │   ├── diversity.rs                        # GET /api/diversity/samples
    │   │   ├── canary.rs                           # GET /api/canary/runs
    │   │   └── findings.rs                         # GET /api/findings/:bundle_hash
    │   └── ipc.rs                                  # Unix-socket event bridge (orchestrator <-> dashboard)
    ├── static/
    │   ├── index.html                              # five-tab SPA shell
    │   ├── css/
    │   │   └── tokens.css                          # design tokens (projector-friendly)
    │   ├── js/
    │   │   ├── bus.js                              # SSE subscriber + event router
    │   │   ├── views/
    │   │   │   ├── live_cycle.js                   # view #1
    │   │   │   ├── genealogy.js                    # view #2 (D3 force / radial)
    │   │   │   ├── diff_inspector.js               # view #3
    │   │   │   ├── mutator_ladder.js               # view #4
    │   │   │   └── ladder_provenance.js            # view #5
    │   │   └── shared/
    │   │       ├── api.js                          # fetch wrappers
    │   │       └── format.js                       # hash truncation, time formatting
    │   └── vendor/
    │       └── d3.v7.min.js                        # checked-in offline fallback
    └── tests/
        ├── api_lineage.rs
        ├── api_seals.rs
        ├── api_ladder.rs
        ├── api_diversity.rs
        ├── api_findings.rs
        ├── sse_smoke.rs
        └── ipc_bridge.rs
```

Plus modifications:
- `Cargo.toml` workspace — add `crates/xvision-dashboard` to `members`
- `crates/xvision-cli/src/lib.rs` — add `Command::Dashboard(commands::dashboard::DashboardCmd)`
- `crates/xvision-cli/src/commands/dashboard.rs` — NEW: thin CLI wrapper around `xvision_dashboard::serve`
- `crates/xvision-cli/src/commands/autoresearch.rs` — extend `EveningCycle` to optionally bind a Unix socket so the dashboard can subscribe (`--ipc-socket /tmp/xvn-events.sock`)

---

## Phase A — Scaffold the `xvision-dashboard` crate

### Task 1: Create the crate, register in workspace

**Files:**
- Create: `crates/xvision-dashboard/Cargo.toml`
- Create: `crates/xvision-dashboard/src/lib.rs`
- Create: `crates/xvision-dashboard/src/server.rs`
- Create: `crates/xvision-dashboard/src/state.rs`
- Modify: `Cargo.toml` (workspace root) — add member

- [ ] **Step 1: Workspace member**

Open `Cargo.toml` at the repo root. Find the `members = [...]` array under `[workspace]` and append:

```toml
"crates/xvision-dashboard",
```

- [ ] **Step 2: Crate manifest**

Create `crates/xvision-dashboard/Cargo.toml`:

```toml
[package]
name        = "xvision-dashboard"
description = "Autoresearch dashboard — axum server + static SPA"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
repository.workspace   = true

[lib]
name = "xvision_dashboard"
path = "src/lib.rs"

[dependencies]
xvision-engine = { path = "../xvision-engine" }

axum         = { version = "0.7", features = ["macros"] }
tower-http   = { version = "0.5", features = ["fs", "cors", "trace"] }
tokio        = { workspace = true, features = ["rt-multi-thread", "macros", "net", "sync", "signal"] }
sqlx         = { workspace = true, features = ["sqlite", "runtime-tokio-rustls", "chrono"] }

serde        = { workspace = true }
serde_json   = { workspace = true }
chrono       = { workspace = true }
anyhow       = { workspace = true }
thiserror    = { workspace = true }
tracing      = { workspace = true }

futures      = "0.3"
tokio-stream = "0.1"

[dev-dependencies]
reqwest = { workspace = true, features = ["json", "stream"] }
tempfile = "3"
ulid     = "1"
```

(`reqwest` is in workspace per the existing engine `Cargo.toml`. If it's not exposed under `[workspace.dependencies]`, list it as `reqwest = { version = "0.12", features = [...] }` directly.)

- [ ] **Step 3: lib.rs**

Create `crates/xvision-dashboard/src/lib.rs`:

```rust
//! xvision-dashboard — axum server + static SPA for the autoresearch
//! evening cycle.
//!
//! Public entry point: [`serve`]. Loads SQLite + blob store, attaches an
//! optional Unix-socket IPC subscriber, mounts the static SPA + REST API
//! + SSE channel, and runs until SIGINT/SIGTERM.

pub mod api;
pub mod ipc;
pub mod server;
pub mod sse;
pub mod state;

pub use server::{serve, ServeOpts};
```

- [ ] **Step 4: Skeleton server.rs + state.rs**

Create `crates/xvision-dashboard/src/state.rs`:

```rust
use std::path::PathBuf;
use std::sync::Arc;

use sqlx::SqlitePool;

use xvision_engine::autoresearch::lineage::LineageStore;
use xvision_engine::autoresearch::progress::ProgressChannel;

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
    pub store: Arc<LineageStore>,
    pub channel: ProgressChannel,
    pub blob_root: PathBuf,
}
```

Create `crates/xvision-dashboard/src/server.rs`:

```rust
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use sqlx::SqlitePool;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use xvision_engine::autoresearch::lineage::LineageStore;
use xvision_engine::autoresearch::progress::ProgressChannel;

use crate::api;
use crate::ipc;
use crate::sse;
use crate::state::AppState;

#[derive(Debug, Clone)]
pub struct ServeOpts {
    pub bind: SocketAddr,
    pub db_path: PathBuf,
    pub blob_root: PathBuf,
    pub static_dir: PathBuf,
    pub ipc_socket: Option<PathBuf>,
}

pub async fn serve(opts: ServeOpts) -> anyhow::Result<()> {
    let pool = SqlitePool::connect(&format!("sqlite://{}", opts.db_path.display())).await?;
    let store = Arc::new(LineageStore::new(pool.clone(), opts.blob_root.clone()).await?);
    let channel = ProgressChannel::default();

    if let Some(sock) = &opts.ipc_socket {
        ipc::spawn_subscriber(sock.clone(), channel.clone()).await?;
    }

    let state = AppState {
        pool,
        store,
        channel: channel.clone(),
        blob_root: opts.blob_root.clone(),
    };

    let api_router = Router::new()
        .nest("/api", api::router())
        .route("/api/events", axum::routing::get(sse::events_handler))
        .with_state(state);

    let app = api_router
        .nest_service("/static", ServeDir::new(&opts.static_dir))
        .nest_service("/", ServeDir::new(&opts.static_dir).append_index_html_on_directories(true))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any));

    tracing::info!("xvision-dashboard listening on http://{}", opts.bind);
    let listener = tokio::net::TcpListener::bind(opts.bind).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c().await.ok();
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutting down");
}
```

- [ ] **Step 5: Build verifies the scaffold**

Run: `cargo build -p xvision-dashboard` — fails because `api`, `ipc`, `sse` modules are missing. That's expected; subsequent tasks introduce them. For this commit we add empty stubs:

```bash
mkdir -p crates/xvision-dashboard/src/api crates/xvision-dashboard/static/{css,js/views,js/shared,vendor} crates/xvision-dashboard/tests
touch crates/xvision-dashboard/src/{ipc.rs,sse.rs}
touch crates/xvision-dashboard/src/api/mod.rs
```

Add to `crates/xvision-dashboard/src/api/mod.rs`:

```rust
use axum::Router;

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
}
```

Add to `crates/xvision-dashboard/src/sse.rs`:

```rust
use axum::extract::State;
use axum::response::sse::{Event, Sse};
use std::convert::Infallible;
use futures::stream::Stream;

use crate::state::AppState;

pub async fn events_handler(
    State(_state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    use futures::stream;
    Sse::new(stream::empty()).keep_alive(axum::response::sse::KeepAlive::default())
}
```

Add to `crates/xvision-dashboard/src/ipc.rs`:

```rust
use std::path::PathBuf;

use xvision_engine::autoresearch::progress::ProgressChannel;

pub async fn spawn_subscriber(_path: PathBuf, _channel: ProgressChannel) -> anyhow::Result<()> {
    // Real implementation in Task 4.
    Ok(())
}
```

Now run: `cargo build -p xvision-dashboard` → success.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml crates/xvision-dashboard/
git commit -m "feat(dashboard): scaffold xvision-dashboard crate (axum + static SPA shell)"
```

---

### Task 2: `xvn dashboard serve` CLI wrapper

**Files:**
- Create: `crates/xvision-cli/src/commands/dashboard.rs`
- Modify: `crates/xvision-cli/src/lib.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs`
- Modify: `crates/xvision-cli/Cargo.toml` — add `xvision-dashboard = { path = "../xvision-dashboard" }`

- [ ] **Step 1: Subcommand**

Create `crates/xvision-cli/src/commands/dashboard.rs`:

```rust
use std::net::SocketAddr;
use std::path::PathBuf;

use clap::Args;

use xvision_dashboard::{serve, ServeOpts};

#[derive(Debug, Args)]
pub struct DashboardCmd {
    #[arg(long, default_value = "127.0.0.1:7777")]
    pub bind: SocketAddr,

    #[arg(long)]
    pub db: PathBuf,

    #[arg(long)]
    pub blob_root: Option<PathBuf>,

    #[arg(long, default_value = "crates/xvision-dashboard/static")]
    pub static_dir: PathBuf,

    #[arg(long)]
    pub ipc_socket: Option<PathBuf>,
}

pub async fn run(cmd: DashboardCmd) -> anyhow::Result<()> {
    let blob_root = cmd
        .blob_root
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".xvn/lineage/blobs"));
    let opts = ServeOpts {
        bind: cmd.bind,
        db_path: cmd.db,
        blob_root,
        static_dir: cmd.static_dir,
        ipc_socket: cmd.ipc_socket,
    };
    serve(opts).await
}
```

- [ ] **Step 2: Wire into top-level CLI**

In `crates/xvision-cli/src/commands/mod.rs`, add: `pub mod dashboard;`.

In `crates/xvision-cli/src/lib.rs`, add to `Command` enum: `Dashboard(commands::dashboard::DashboardCmd),`. Add to `Cli::run()` match: `Command::Dashboard(cmd) => commands::dashboard::run(cmd).await,`.

- [ ] **Step 3: Smoke**

```bash
TMPDIR=$(mktemp -d)
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/001_init.sql 2>/dev/null
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/002_eval.sql
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/003_autoresearch.sql
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/004_autoresearch_evals.sql
cargo run -p xvision-cli -- dashboard serve --db $TMPDIR/test.db --blob-root $TMPDIR/blobs &
DASH_PID=$!
sleep 1
curl -s http://127.0.0.1:7777/api/events --max-time 1 | head -5
kill $DASH_PID
```

Expected: server boots, `/api/events` opens an SSE stream (immediately empty since the stub in Task 1 returns an empty stream).

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-cli/
git commit -m "feat(cli): xvn dashboard serve — boots axum + static SPA"
```

---

## Phase B — REST API endpoints

### Task 3: `/api/lineage` + `/api/lineage/:bundle_hash`

**Files:**
- Create: `crates/xvision-dashboard/src/api/lineage.rs`
- Modify: `crates/xvision-dashboard/src/api/mod.rs`
- Create: `crates/xvision-dashboard/tests/api_lineage.rs`

- [ ] **Step 1: Failing test**

```rust
// tests/api_lineage.rs
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use chrono::Utc;
use sqlx::SqlitePool;
use tempfile::tempdir;

use xvision_dashboard::server::{serve, ServeOpts};
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::lineage::{LineageNode, LineageStatus, LineageStore, MetricsSnapshot};

async fn boot_with_seed() -> (SocketAddr, tempfile::TempDir) {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display())).await.unwrap();
    sqlx::migrate!("../xvision-engine/migrations").run(&pool).await.unwrap();
    let store = Arc::new(LineageStore::new(pool, dir.path().join("blobs")).await.unwrap());
    for i in 0..3u32 {
        let h = ContentHash::of_bytes(format!("b{i}").as_bytes());
        store.insert_node(&LineageNode {
            bundle_hash: h,
            parent_hash: None,
            diff_blob_hash: None,
            finding_blob_hash: None,
            status: LineageStatus::Active,
            born_at: Utc::now(),
            metrics: Some(MetricsSnapshot {
                days_alive: i,
                trades_attributed: i,
                realized_pnl_attributed: i as f64,
            }),
            cycle_id: None,
            session_id: None,
        }).await.unwrap();
    }
    let bind: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(bind).await.unwrap();
    let bind_actual = listener.local_addr().unwrap();
    drop(listener);
    let opts = ServeOpts {
        bind: bind_actual,
        db_path: db_path.clone(),
        blob_root: dir.path().join("blobs"),
        static_dir: PathBuf::from("static"),
        ipc_socket: None,
    };
    tokio::spawn(async move { serve(opts).await.ok(); });
    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
    (bind_actual, dir)
}

#[tokio::test]
async fn list_lineage_returns_three_nodes() {
    let (addr, _dir) = boot_with_seed().await;
    let resp: serde_json::Value = reqwest::get(format!("http://{addr}/api/lineage"))
        .await.unwrap()
        .json().await.unwrap();
    let arr = resp.as_array().unwrap();
    assert_eq!(arr.len(), 3);
    for item in arr {
        assert!(item["bundle_hash"].is_string());
        assert!(item["status"].as_str().unwrap() == "active");
    }
}

#[tokio::test]
async fn get_lineage_by_hash_returns_node() {
    let (addr, _dir) = boot_with_seed().await;
    let h = ContentHash::of_bytes(b"b0").to_hex();
    let resp: serde_json::Value = reqwest::get(format!("http://{addr}/api/lineage/{h}"))
        .await.unwrap()
        .json().await.unwrap();
    assert_eq!(resp["bundle_hash"].as_str().unwrap(), h);
    assert_eq!(resp["status"].as_str().unwrap(), "active");
}
```

- [ ] **Step 2: Implement lineage.rs**

```rust
// src/api/lineage.rs
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::state::AppState;
use xvision_engine::autoresearch::content_hash::ContentHash;

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct LineageNodeRow {
    pub bundle_hash: String,
    pub parent_hash: Option<String>,
    pub diff_blob_hash: Option<String>,
    pub finding_blob_hash: Option<String>,
    pub status: String,
    pub born_at: String,
    pub metrics: Option<serde_json::Value>,
    pub cycle_id: Option<String>,
    pub session_id: Option<String>,
}

pub async fn list(
    Query(q): Query<ListQuery>,
    State(state): State<AppState>,
) -> Result<Json<Vec<LineageNodeRow>>, (StatusCode, String)> {
    let limit = q.limit.unwrap_or(500).min(2000) as i64;
    let rows = match q.status {
        Some(st) => sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_blob_hash, finding_blob_hash, status,
                    born_at, metrics_json, cycle_id, session_id
             FROM autoresearch_lineage_nodes WHERE status = ? ORDER BY born_at DESC LIMIT ?",
        )
        .bind(st)
        .bind(limit),
        None => sqlx::query(
            "SELECT bundle_hash, parent_hash, diff_blob_hash, finding_blob_hash, status,
                    born_at, metrics_json, cycle_id, session_id
             FROM autoresearch_lineage_nodes ORDER BY born_at DESC LIMIT ?",
        )
        .bind(limit),
    }
    .fetch_all(&state.pool)
    .await
    .map_err(internal)?;

    let mut out = Vec::with_capacity(rows.len());
    for row in rows {
        out.push(row_to_node(&row).map_err(internal)?);
    }
    Ok(Json(out))
}

pub async fn get(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<LineageNodeRow>, (StatusCode, String)> {
    let row = sqlx::query(
        "SELECT bundle_hash, parent_hash, diff_blob_hash, finding_blob_hash, status,
                born_at, metrics_json, cycle_id, session_id
         FROM autoresearch_lineage_nodes WHERE bundle_hash = ?",
    )
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal)?
    .ok_or((StatusCode::NOT_FOUND, format!("no lineage node {hash}")))?;
    Ok(Json(row_to_node(&row).map_err(internal)?))
}

fn row_to_node(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<LineageNodeRow> {
    let metrics_json: Option<&str> = row.try_get("metrics_json")?;
    Ok(LineageNodeRow {
        bundle_hash: row.try_get::<&str, _>("bundle_hash")?.to_string(),
        parent_hash: row.try_get::<Option<&str>, _>("parent_hash")?.map(str::to_string),
        diff_blob_hash: row.try_get::<Option<&str>, _>("diff_blob_hash")?.map(str::to_string),
        finding_blob_hash: row.try_get::<Option<&str>, _>("finding_blob_hash")?.map(str::to_string),
        status: row.try_get::<&str, _>("status")?.to_string(),
        born_at: row.try_get::<&str, _>("born_at")?.to_string(),
        metrics: metrics_json
            .map(serde_json::from_str::<serde_json::Value>)
            .transpose()?,
        cycle_id: row.try_get::<Option<String>, _>("cycle_id")?,
        session_id: row.try_get::<Option<String>, _>("session_id")?,
    })
}

fn internal<E: std::fmt::Display>(e: E) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, format!("{e}"))
}
```

Update `src/api/mod.rs`:

```rust
pub mod lineage;

use axum::routing::get;
use axum::Router;

pub fn router() -> Router<crate::state::AppState> {
    Router::new()
        .route("/lineage", get(lineage::list))
        .route("/lineage/:hash", get(lineage::get))
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-dashboard --test api_lineage
git add crates/xvision-dashboard/
git commit -m "feat(dashboard): GET /api/lineage + /api/lineage/:hash"
```

---

### Task 4: IPC bridge (orchestrator → dashboard)

The orchestrator writes events to its own `ProgressChannel`. The dashboard runs in a separate process. Without an IPC bridge, the dashboard's channel has no events. We add a Unix-socket subscriber: the orchestrator binds the socket and writes JSON-serialized events line by line; the dashboard subscribes, parses each line, and re-emits into its own channel.

**Files:**
- Replace stub: `crates/xvision-dashboard/src/ipc.rs`
- Modify: `crates/xvision-cli/src/commands/autoresearch.rs` — add `--ipc-socket` to `EveningCycle` and start a UDS sender alongside the stdout subscriber
- Create: `crates/xvision-dashboard/tests/ipc_bridge.rs`

- [ ] **Step 1: Failing test**

```rust
// tests/ipc_bridge.rs
use std::path::PathBuf;
use std::time::Duration;

use tempfile::tempdir;
use tokio::io::AsyncWriteExt;
use tokio::net::UnixStream;

use xvision_dashboard::ipc::spawn_subscriber;
use xvision_engine::autoresearch::progress::{AutoresearchEvent, ProgressChannel};

#[tokio::test]
async fn ipc_bridge_re_emits_events_into_dashboard_channel() {
    let dir = tempdir().unwrap();
    let socket: PathBuf = dir.path().join("events.sock");
    let channel = ProgressChannel::default();
    let mut rx = channel.subscribe();
    spawn_subscriber(socket.clone(), channel).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let mut client = UnixStream::connect(&socket).await.unwrap();
    let event = AutoresearchEvent::CycleStarted {
        cycle_id: "test-cycle".into(),
        session_id: "test-session".into(),
        parent_count: 2,
    };
    let line = serde_json::to_string(&event).unwrap() + "\n";
    client.write_all(line.as_bytes()).await.unwrap();
    client.shutdown().await.unwrap();

    let received = tokio::time::timeout(Duration::from_secs(1), rx.recv()).await.unwrap().unwrap();
    match received {
        AutoresearchEvent::CycleStarted { cycle_id, .. } => assert_eq!(cycle_id, "test-cycle"),
        other => panic!("unexpected: {other:?}"),
    }
}
```

- [ ] **Step 2: Implement ipc.rs**

```rust
//! Unix-socket subscriber. The orchestrator (`xvn autoresearch
//! evening-cycle`) binds to this socket and writes JSON-serialized
//! AutoresearchEvent lines. The dashboard subscribes here and re-emits
//! each event into its in-process ProgressChannel so SSE clients see
//! live updates.

use std::path::PathBuf;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::net::UnixListener;

use xvision_engine::autoresearch::progress::{AutoresearchEvent, ProgressChannel};

pub async fn spawn_subscriber(path: PathBuf, channel: ProgressChannel) -> anyhow::Result<()> {
    if path.exists() {
        let _ = tokio::fs::remove_file(&path).await;
    }
    let listener = UnixListener::bind(&path)?;
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    let ch = channel.clone();
                    tokio::spawn(async move {
                        let reader = BufReader::new(stream);
                        let mut lines = reader.lines();
                        while let Ok(Some(line)) = lines.next_line().await {
                            match serde_json::from_str::<AutoresearchEvent>(&line) {
                                Ok(ev) => ch.emit(ev),
                                Err(e) => tracing::warn!("ipc parse error: {e}; line: {line}"),
                            }
                        }
                    });
                }
                Err(e) => {
                    tracing::error!("uds accept failed: {e}");
                    break;
                }
            }
        }
    });
    Ok(())
}
```

- [ ] **Step 3: Orchestrator-side sender**

Modify `crates/xvision-cli/src/commands/autoresearch.rs`. In the `EveningCycle` action, add `--ipc-socket: Option<PathBuf>`, and in the handler (right after `let progress = ProgressChannel::default();`), spawn:

```rust
if let Some(sock) = &ipc_socket {
    spawn_uds_sender(sock.clone(), progress.subscribe()).await?;
}

async fn spawn_uds_sender(
    path: std::path::PathBuf,
    mut rx: tokio::sync::broadcast::Receiver<xvision_engine::autoresearch::progress::AutoresearchEvent>,
) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;
    tokio::spawn(async move {
        let mut backoff = std::time::Duration::from_millis(100);
        loop {
            match UnixStream::connect(&path).await {
                Ok(mut stream) => {
                    while let Ok(ev) = rx.recv().await {
                        let mut line = serde_json::to_string(&ev).unwrap();
                        line.push('\n');
                        if stream.write_all(line.as_bytes()).await.is_err() {
                            break;
                        }
                    }
                }
                Err(_) => {
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(std::time::Duration::from_secs(5));
                }
            }
        }
    });
    Ok(())
}
```

- [ ] **Step 4: Run + commit**

```bash
cargo test -p xvision-dashboard --test ipc_bridge
git add crates/xvision-dashboard/src/ipc.rs crates/xvision-dashboard/tests/ipc_bridge.rs crates/xvision-cli/src/commands/autoresearch.rs
git commit -m "feat(dashboard): UDS ipc bridge — orchestrator events flow into dashboard channel"
```

---

### Task 5: SSE endpoint `/api/events`

**Files:**
- Replace `crates/xvision-dashboard/src/sse.rs`
- Create: `crates/xvision-dashboard/tests/sse_smoke.rs`

- [ ] **Step 1: Failing test**

```rust
// tests/sse_smoke.rs
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use futures::StreamExt;
use sqlx::SqlitePool;
use tempfile::tempdir;
use tokio::time::timeout;

use xvision_dashboard::server::{serve, ServeOpts};
use xvision_engine::autoresearch::progress::AutoresearchEvent;

#[tokio::test]
async fn sse_endpoint_streams_events_emitted_into_progress_channel() {
    // For this smoke we exercise the dashboard's own ProgressChannel via
    // the IPC bridge.
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display())).await.unwrap();
    sqlx::migrate!("../xvision-engine/migrations").run(&pool).await.unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let bind = listener.local_addr().unwrap();
    drop(listener);
    let socket = dir.path().join("events.sock");
    let opts = ServeOpts {
        bind,
        db_path,
        blob_root: dir.path().join("blobs"),
        static_dir: PathBuf::from("static"),
        ipc_socket: Some(socket.clone()),
    };
    tokio::spawn(async move { serve(opts).await.ok(); });
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Client connects to UDS and emits one event.
    let mut uds = tokio::net::UnixStream::connect(&socket).await.unwrap();
    use tokio::io::AsyncWriteExt;
    let event = AutoresearchEvent::CycleSealed {
        cycle_id: "test-cycle".into(),
        seal_blob_hash: "abc".repeat(20),
        merkle_root: "def".repeat(20),
    };
    let line = serde_json::to_string(&event).unwrap() + "\n";
    uds.write_all(line.as_bytes()).await.unwrap();
    uds.shutdown().await.unwrap();

    // Subscribe to SSE.
    let resp = reqwest::get(format!("http://{bind}/api/events")).await.unwrap();
    let mut stream = resp.bytes_stream();
    let chunk = timeout(Duration::from_secs(2), stream.next()).await.unwrap().unwrap().unwrap();
    let text = std::str::from_utf8(&chunk).unwrap();
    assert!(text.contains("cycle_sealed") || text.contains("event:"), "got: {text}");
}
```

- [ ] **Step 2: Implement sse.rs**

```rust
use std::convert::Infallible;

use axum::extract::State;
use axum::response::sse::{Event, KeepAlive, Sse};
use futures::stream::Stream;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt as _;

use crate::state::AppState;

pub async fn events_handler(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let rx = state.channel.subscribe();
    let stream = BroadcastStream::new(rx).filter_map(|result| match result {
        Ok(event) => {
            let json = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
            // Use event-type as SSE event name; payload as data.
            let kind = match &event {
                xvision_engine::autoresearch::progress::AutoresearchEvent::CycleStarted { .. } => "cycle_started",
                xvision_engine::autoresearch::progress::AutoresearchEvent::MutationProposed { .. } => "mutation_proposed",
                xvision_engine::autoresearch::progress::AutoresearchEvent::MutationEvaluating { .. } => "mutation_evaluating",
                xvision_engine::autoresearch::progress::AutoresearchEvent::MutationCommitted { .. } => "mutation_committed",
                xvision_engine::autoresearch::progress::AutoresearchEvent::MutationRejected { .. } => "mutation_rejected",
                xvision_engine::autoresearch::progress::AutoresearchEvent::MutationQuarantined { .. } => "mutation_quarantined",
                xvision_engine::autoresearch::progress::AutoresearchEvent::LineageForked { .. } => "lineage_forked",
                xvision_engine::autoresearch::progress::AutoresearchEvent::JudgeWroteFinding { .. } => "judge_wrote_finding",
                xvision_engine::autoresearch::progress::AutoresearchEvent::CanaryOutcome { .. } => "canary_outcome",
                xvision_engine::autoresearch::progress::AutoresearchEvent::DiversityUpdated { .. } => "diversity_updated",
                xvision_engine::autoresearch::progress::AutoresearchEvent::LadderSnapshot { .. } => "ladder_snapshot",
                xvision_engine::autoresearch::progress::AutoresearchEvent::CycleSealed { .. } => "cycle_sealed",
                xvision_engine::autoresearch::progress::AutoresearchEvent::CycleFailed { .. } => "cycle_failed",
            };
            Some(Ok::<Event, Infallible>(Event::default().event(kind).data(json)))
        }
        Err(_) => None, // lagged subscriber; drop
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-dashboard --test sse_smoke
git add crates/xvision-dashboard/src/sse.rs crates/xvision-dashboard/tests/sse_smoke.rs
git commit -m "feat(dashboard): SSE /api/events bridges ProgressChannel to clients"
```

---

### Task 6: `/api/seals` + `/api/seals/:cycle_id`

Returns a list of CycleSeal index rows; the detail endpoint fetches the full seal blob from the blob store.

**Files:**
- Create: `crates/xvision-dashboard/src/api/seals.rs`
- Modify: `crates/xvision-dashboard/src/api/mod.rs`
- Create: `crates/xvision-dashboard/tests/api_seals.rs`

- [ ] **Step 1: Failing test**

```rust
// tests/api_seals.rs
// Pattern: boot dashboard with a seeded SQLite + a real CycleSeal blob written
// to the store. GET /api/seals returns 1 row; GET /api/seals/:id verifies the
// seal signature.
// (Subagent fills in the boot helper using the same pattern as
// api_lineage.rs's boot_with_seed; seeds one CycleSeal via
// CycleSealWriter::seal_and_commit.)
```

(Subagent implements this as ~80 lines; the test seeds a seal and asserts both endpoints return the expected JSON.)

- [ ] **Step 2: Implement seals.rs**

```rust
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use sqlx::Row;

use crate::state::AppState;
use xvision_engine::autoresearch::content_hash::ContentHash;
use xvision_engine::autoresearch::seal::{CycleSeal, CycleSealWriter};

#[derive(Debug, Serialize)]
pub struct SealRow {
    pub cycle_id: String,
    pub session_id: String,
    pub sealed_at: String,
    pub merkle_root_hex: String,
    pub seal_blob_hash: String,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<SealRow>>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT cycle_id, session_id, sealed_at, merkle_root_hex, seal_blob_hash
         FROM autoresearch_cycle_seals ORDER BY sealed_at DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        out.push(SealRow {
            cycle_id: r.try_get::<&str, _>("cycle_id").unwrap_or_default().to_string(),
            session_id: r.try_get::<&str, _>("session_id").unwrap_or_default().to_string(),
            sealed_at: r.try_get::<&str, _>("sealed_at").unwrap_or_default().to_string(),
            merkle_root_hex: r.try_get::<&str, _>("merkle_root_hex").unwrap_or_default().to_string(),
            seal_blob_hash: r.try_get::<&str, _>("seal_blob_hash").unwrap_or_default().to_string(),
        });
    }
    Ok(Json(out))
}

#[derive(Debug, Serialize)]
pub struct SealDetail {
    pub seal: CycleSeal,
    pub signature_verified: bool,
}

pub async fn get(
    Path(cycle_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<SealDetail>, (StatusCode, String)> {
    let blob_hex: String = sqlx::query_scalar(
        "SELECT seal_blob_hash FROM autoresearch_cycle_seals WHERE cycle_id = ?",
    )
    .bind(&cycle_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .ok_or((StatusCode::NOT_FOUND, format!("no seal for cycle_id {cycle_id}")))?;
    let blob_hash = ContentHash::from_hex(&blob_hex)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let v = state.store.blobs().get_json(&blob_hash).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let seal: CycleSeal = serde_json::from_value(v)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let signature_verified = CycleSealWriter::verify(&seal).is_ok();
    Ok(Json(SealDetail { seal, signature_verified }))
}
```

In `src/api/mod.rs` add:

```rust
pub mod seals;
// inside router():
.route("/seals", get(seals::list))
.route("/seals/:cycle_id", get(seals::get))
```

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-dashboard --test api_seals
git add crates/xvision-dashboard/src/api/seals.rs crates/xvision-dashboard/src/api/mod.rs crates/xvision-dashboard/tests/api_seals.rs
git commit -m "feat(dashboard): GET /api/seals + /api/seals/:cycle_id (verifies signature)"
```

---

### Task 7: `/api/ladder/snapshots`, `/api/diversity/samples`, `/api/canary/runs`, `/api/findings/:bundle_hash`

Four small endpoints; one task. Each is a SELECT and a serde::Serialize struct.

**Files:**
- Create: `crates/xvision-dashboard/src/api/ladder.rs`
- Create: `crates/xvision-dashboard/src/api/diversity.rs`
- Create: `crates/xvision-dashboard/src/api/canary.rs`
- Create: `crates/xvision-dashboard/src/api/findings.rs`
- Modify: `crates/xvision-dashboard/src/api/mod.rs`
- Create: tests/api_ladder.rs, tests/api_diversity.rs, tests/api_findings.rs

- [ ] **Step 1: Implement four endpoints**

```rust
// src/api/ladder.rs
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use sqlx::Row;

use crate::state::AppState;
use xvision_engine::autoresearch::content_hash::ContentHash;

#[derive(Debug, Serialize)]
pub struct LadderRow {
    pub cycle_id: String,
    pub sampled_at: String,
    pub snapshot: serde_json::Value,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<LadderRow>>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT cycle_id, snapshot_blob_hash, sampled_at FROM autoresearch_mutator_ladder_snapshots ORDER BY sampled_at",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let mut out = Vec::with_capacity(rows.len());
    for r in rows {
        let blob_hex: &str = r.try_get("snapshot_blob_hash").unwrap_or("");
        let blob_hash = ContentHash::from_hex(blob_hex)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let snapshot = state.store.blobs().get_json(&blob_hash).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        out.push(LadderRow {
            cycle_id: r.try_get::<&str, _>("cycle_id").unwrap_or_default().to_string(),
            sampled_at: r.try_get::<&str, _>("sampled_at").unwrap_or_default().to_string(),
            snapshot,
        });
    }
    Ok(Json(out))
}
```

```rust
// src/api/diversity.rs
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use sqlx::Row;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct DiversitySampleRow {
    pub cycle_id: String,
    pub lineage_root: String,
    pub mean_pairwise_distance: f64,
    pub decay_ratio: Option<f64>,
    pub sampled_at: String,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<DiversitySampleRow>>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT cycle_id, lineage_root, mean_pairwise_distance, decay_ratio, sampled_at
         FROM autoresearch_diversity_samples ORDER BY sampled_at",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows.into_iter().map(|r| DiversitySampleRow {
        cycle_id: r.try_get::<&str, _>("cycle_id").unwrap_or_default().to_string(),
        lineage_root: r.try_get::<&str, _>("lineage_root").unwrap_or_default().to_string(),
        mean_pairwise_distance: r.try_get::<f64, _>("mean_pairwise_distance").unwrap_or(0.0),
        decay_ratio: r.try_get::<Option<f64>, _>("decay_ratio").unwrap_or(None),
        sampled_at: r.try_get::<&str, _>("sampled_at").unwrap_or_default().to_string(),
    }).collect()))
}
```

```rust
// src/api/canary.rs
use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use sqlx::Row;

use crate::state::AppState;

#[derive(Debug, Serialize)]
pub struct CanaryRunRow {
    pub cycle_id: String,
    pub canary_bundle_hash: String,
    pub accepted_count: i64,
    pub rejected_count: i64,
}

pub async fn list(State(state): State<AppState>) -> Result<Json<Vec<CanaryRunRow>>, (StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT cycle_id, canary_bundle_hash, accepted_count, rejected_count
         FROM autoresearch_canary_runs ORDER BY cycle_id DESC",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(rows.into_iter().map(|r| CanaryRunRow {
        cycle_id: r.try_get::<&str, _>("cycle_id").unwrap_or_default().to_string(),
        canary_bundle_hash: r.try_get::<&str, _>("canary_bundle_hash").unwrap_or_default().to_string(),
        accepted_count: r.try_get::<i64, _>("accepted_count").unwrap_or(0),
        rejected_count: r.try_get::<i64, _>("rejected_count").unwrap_or(0),
    }).collect()))
}
```

```rust
// src/api/findings.rs
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;

use crate::state::AppState;
use xvision_engine::autoresearch::content_hash::ContentHash;

#[derive(Debug, Serialize)]
pub struct FindingDetail {
    pub bundle_hash: String,
    pub finding: serde_json::Value,
}

pub async fn get(
    Path(bundle_hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<FindingDetail>, (StatusCode, String)> {
    let finding_hex: Option<String> = sqlx::query_scalar(
        "SELECT finding_blob_hash FROM autoresearch_lineage_nodes WHERE bundle_hash = ?",
    )
    .bind(&bundle_hash)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?
    .flatten();
    let finding_hex = finding_hex.ok_or((StatusCode::NOT_FOUND, format!("no finding for bundle_hash {bundle_hash}")))?;
    let h = ContentHash::from_hex(&finding_hex).map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let finding = state.store.blobs().get_json(&h).await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(FindingDetail { bundle_hash, finding }))
}
```

In `src/api/mod.rs`:

```rust
pub mod canary;
pub mod diversity;
pub mod findings;
pub mod ladder;
// inside router():
.route("/ladder/snapshots", get(ladder::list))
.route("/diversity/samples", get(diversity::list))
.route("/canary/runs", get(canary::list))
.route("/findings/:bundle_hash", get(findings::get))
```

- [ ] **Step 2: Failing tests**

(Subagent writes one happy-path test per endpoint following the same boot-with-seed pattern. Seed canary by inserting one row directly; seed ladder/diversity by inserting one row + one fixture blob.)

- [ ] **Step 3: Run + commit**

```bash
cargo test -p xvision-dashboard
git add crates/xvision-dashboard/
git commit -m "feat(dashboard): GET /api/ladder/snapshots, /api/diversity/samples, /api/canary/runs, /api/findings/:hash"
```

---

## Phase C — Frontend SPA shell + design tokens

### Task 8: index.html + tokens.css + JS bus

The SPA is one HTML page with five tab buttons and five view containers. Tab switching is client-side; the bus.js subscribes to SSE and dispatches events to the active view's handler.

**Files:**
- Create: `crates/xvision-dashboard/static/index.html`
- Create: `crates/xvision-dashboard/static/css/tokens.css`
- Create: `crates/xvision-dashboard/static/js/bus.js`
- Create: `crates/xvision-dashboard/static/js/shared/api.js`
- Create: `crates/xvision-dashboard/static/js/shared/format.js`
- Vendor: `crates/xvision-dashboard/static/vendor/d3.v7.min.js` (download once, commit)

- [ ] **Step 1: index.html**

```html
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>xvision autoresearch</title>
<link rel="stylesheet" href="/static/css/tokens.css">
</head>
<body>
<header class="app-header">
  <h1>xvision · autoresearch</h1>
  <div class="meta">
    <span id="meta-session">session: —</span>
    <span id="meta-cycle">cycle: —</span>
    <span id="meta-tokens">tokens: —</span>
  </div>
</header>
<nav class="tabs">
  <button data-tab="live"        class="tab active">Live cycle</button>
  <button data-tab="genealogy"   class="tab">Genealogy</button>
  <button data-tab="diff"        class="tab">Diff inspector</button>
  <button data-tab="ladder"      class="tab">Mutator ladder</button>
  <button data-tab="provenance"  class="tab">Ladder w/ provenance</button>
</nav>
<main>
  <section id="view-live"       class="view active"></section>
  <section id="view-genealogy"  class="view"></section>
  <section id="view-diff"       class="view"></section>
  <section id="view-ladder"     class="view"></section>
  <section id="view-provenance" class="view"></section>
</main>
<script src="/static/vendor/d3.v7.min.js"></script>
<script type="module" src="/static/js/bus.js"></script>
<script type="module" src="/static/js/views/live_cycle.js"></script>
<script type="module" src="/static/js/views/genealogy.js"></script>
<script type="module" src="/static/js/views/diff_inspector.js"></script>
<script type="module" src="/static/js/views/mutator_ladder.js"></script>
<script type="module" src="/static/js/views/ladder_provenance.js"></script>
<script type="module">
  // Tab switching.
  document.querySelectorAll(".tab").forEach((b) => {
    b.addEventListener("click", () => {
      document.querySelectorAll(".tab").forEach((x) => x.classList.remove("active"));
      document.querySelectorAll(".view").forEach((x) => x.classList.remove("active"));
      b.classList.add("active");
      const id = `view-${b.dataset.tab}`;
      document.getElementById(id).classList.add("active");
    });
  });
</script>
</body>
</html>
```

- [ ] **Step 2: tokens.css**

```css
:root {
  --bg: #0b0e14;
  --fg: #e6edf3;
  --fg-dim: #8b949e;
  --accent: #58a6ff;
  --accent-warm: #f0883e;
  --ok: #3fb950;
  --bad: #f85149;
  --ghost: #6e7681;
  --quarantine: #d29922;
  --border: rgba(99, 102, 241, 0.18);
  --mono: ui-monospace, "JetBrains Mono", "SFMono-Regular", Menlo, monospace;
  --sans: -apple-system, system-ui, "Segoe UI", Inter, sans-serif;
  --size-xl: 1.6rem;     /* projector-friendly */
  --size-lg: 1.2rem;
  --size-md: 1.0rem;
  --size-sm: 0.85rem;
}
* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; height: 100%; background: var(--bg); color: var(--fg); font-family: var(--sans); font-size: var(--size-md); }
.app-header { display: flex; justify-content: space-between; align-items: baseline; padding: 16px 24px; border-bottom: 1px solid var(--border); }
.app-header h1 { margin: 0; font-size: var(--size-xl); letter-spacing: -0.02em; }
.app-header .meta { font-family: var(--mono); color: var(--fg-dim); font-size: var(--size-sm); display: flex; gap: 16px; }
.tabs { display: flex; gap: 8px; padding: 8px 24px; border-bottom: 1px solid var(--border); }
.tab { background: transparent; color: var(--fg-dim); border: 1px solid transparent; padding: 8px 16px; font-size: var(--size-md); cursor: pointer; }
.tab.active { color: var(--fg); border-color: var(--border); background: rgba(255,255,255,0.02); }
main { padding: 16px 24px; }
.view { display: none; }
.view.active { display: block; }
.event { font-family: var(--mono); font-size: var(--size-sm); padding: 4px 8px; border-left: 2px solid var(--ghost); margin-bottom: 4px; }
.event.committed { border-left-color: var(--ok); }
.event.rejected  { border-left-color: var(--bad); }
.event.quarantined { border-left-color: var(--quarantine); }
.event.canary    { border-left-color: var(--accent-warm); }
.event.sealed    { border-left-color: var(--accent); }
.cell-hash { font-family: var(--mono); }
.lineage-svg { width: 100%; height: calc(100vh - 220px); border: 1px solid var(--border); }
.diff-pane { display: grid; grid-template-columns: 1fr 1fr 1fr; gap: 8px; }
.diff-pane > div { padding: 8px; background: rgba(255,255,255,0.02); border: 1px solid var(--border); font-family: var(--mono); font-size: var(--size-sm); white-space: pre-wrap; }
.diff-pane .add { color: var(--ok); }
.diff-pane .rem { color: var(--bad); }
.kpi { display: inline-block; padding: 8px 16px; border: 1px solid var(--border); margin-right: 8px; min-width: 140px; }
.kpi .label { color: var(--fg-dim); font-size: var(--size-sm); }
.kpi .value { font-size: var(--size-lg); font-family: var(--mono); }
```

- [ ] **Step 3: bus.js**

```js
// /static/js/bus.js
const handlers = new Map();

export function on(eventType, fn) {
  if (!handlers.has(eventType)) handlers.set(eventType, []);
  handlers.get(eventType).push(fn);
}

function dispatch(eventType, payload) {
  const list = handlers.get(eventType) || [];
  for (const fn of list) {
    try { fn(payload); } catch (e) { console.error(e); }
  }
}

const EVENT_KINDS = [
  "cycle_started",
  "mutation_proposed",
  "mutation_evaluating",
  "mutation_committed",
  "mutation_rejected",
  "mutation_quarantined",
  "lineage_forked",
  "judge_wrote_finding",
  "canary_outcome",
  "diversity_updated",
  "ladder_snapshot",
  "cycle_sealed",
  "cycle_failed",
];

const sse = new EventSource("/api/events");
for (const kind of EVENT_KINDS) {
  sse.addEventListener(kind, (ev) => {
    const data = JSON.parse(ev.data);
    dispatch(kind, data);
  });
}
sse.onerror = (e) => console.warn("sse error", e);
```

- [ ] **Step 4: api.js + format.js**

```js
// /static/js/shared/api.js
export async function get(path) {
  const res = await fetch(path);
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
  return res.json();
}
```

```js
// /static/js/shared/format.js
export function shortHash(h) { return h ? `${h.slice(0, 6)}…${h.slice(-4)}` : "—"; }
export function fmtPct(x)    { return (x * 100).toFixed(1) + "%"; }
export function fmtSharpe(x) { return x == null ? "—" : x.toFixed(3); }
export function fmtDateShort(s) { return s ? s.replace("T", " ").slice(0, 19) : "—"; }
```

- [ ] **Step 5: Vendor D3**

```bash
mkdir -p crates/xvision-dashboard/static/vendor
curl -sSL https://d3js.org/d3.v7.min.js -o crates/xvision-dashboard/static/vendor/d3.v7.min.js
echo "$(shasum -a 256 crates/xvision-dashboard/static/vendor/d3.v7.min.js) (committed offline copy)"
```

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-dashboard/static/
git commit -m "feat(dashboard): SPA shell + tokens + SSE bus + D3 v7 vendor"
```

---

## Phase D — View 1: Live evening cycle viewer

### Task 9: live_cycle.js

The headline view (autoresearch spec §9 #1). Vertical lineage column on the left listing parents being processed; mutation timeline scrolls right; ghost branches faded. Token meter at the top updates from `mutation_proposed` events.

**File:** `crates/xvision-dashboard/static/js/views/live_cycle.js`

- [ ] **Step 1: Implement live_cycle.js**

```js
// /static/js/views/live_cycle.js
import { on } from "/static/js/bus.js";
import { shortHash, fmtSharpe } from "/static/js/shared/format.js";

const root = document.getElementById("view-live");

let tokens = 0;
let cycleId = null;
let parents = new Map();   // parent_hash → { mutations: [...], element }

function render() {
  root.innerHTML = `
    <div class="kpi"><div class="label">cycle</div><div class="value" id="lc-cycle">—</div></div>
    <div class="kpi"><div class="label">tokens</div><div class="value" id="lc-tokens">${tokens}</div></div>
    <div class="kpi"><div class="label">parents</div><div class="value" id="lc-parents">${parents.size}</div></div>
    <div id="lc-stream" style="margin-top:16px;display:flex;flex-direction:column;gap:8px;font-family:var(--mono);font-size:var(--size-sm);"></div>
  `;
}

function append(klass, text) {
  const stream = document.getElementById("lc-stream");
  if (!stream) return;
  const el = document.createElement("div");
  el.className = `event ${klass}`;
  el.textContent = text;
  stream.prepend(el);
  if (stream.children.length > 200) stream.lastChild.remove();
}

render();

on("cycle_started", (data) => {
  cycleId = data.cycle_id;
  parents = new Map();
  tokens = 0;
  document.getElementById("lc-cycle").textContent = shortHash(cycleId);
  document.getElementById("lc-tokens").textContent = tokens;
  document.getElementById("lc-parents").textContent = data.parent_count;
  document.getElementById("meta-cycle").textContent = `cycle: ${shortHash(cycleId)}`;
  append("sealed", `cycle started · session ${shortHash(data.session_id)}`);
});

on("mutation_proposed", (d) =>
  append("", `mutation proposed for ${shortHash(d.parent_hash)} (retries=${d.retries})`),
);
on("mutation_evaluating", (d) =>
  append("", `evaluating ${shortHash(d.child_hash)} (${d.window})`),
);
on("mutation_committed", (d) => {
  append("committed", `✓ ${shortHash(d.child_hash)} ΔdaySharpe=${fmtSharpe(d.delta_day)} ΔholdoutSharpe=${fmtSharpe(d.delta_holdout)}`);
});
on("mutation_rejected", (d) =>
  append("rejected", `✗ ${shortHash(d.child_hash)}: ${d.reason}`),
);
on("mutation_quarantined", (d) =>
  append("quarantined", `⚠ ${shortHash(d.child_hash)}: ${d.reason}`),
);
on("judge_wrote_finding", (d) =>
  append("", `📝 finding for ${shortHash(d.child_hash)} (${d.confidence})`),
);
on("canary_outcome", (d) => {
  const cls = d.accepted === 0 ? "canary" : "rejected";
  append(cls, `canary: accepted=${d.accepted}, rejected=${d.rejected}${d.accepted ? " ⚠ ALARM" : " ✓"}`);
});
on("diversity_updated", (d) => {
  const decay = d.decay_ratio == null ? "—" : d.decay_ratio.toFixed(3);
  append("", `diversity ${shortHash(d.lineage_root)}: mean=${d.mean_distance.toFixed(3)} decay=${decay}`);
});
on("ladder_snapshot", (d) =>
  append("", `ladder · acceptance ${(d.acceptance_rate * 100).toFixed(1)}%`),
);
on("cycle_sealed", (d) =>
  append("sealed", `🔒 cycle sealed · merkle ${shortHash(d.merkle_root)}`),
);
on("cycle_failed", (d) =>
  append("rejected", `× cycle failed: ${d.error}`),
);
```

- [ ] **Step 2: Manual smoke**

```bash
TMPDIR=$(mktemp -d)
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/001_init.sql 2>/dev/null
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/002_eval.sql
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/003_autoresearch.sql
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/004_autoresearch_evals.sql
cargo run -p xvision-cli -- dashboard serve --db $TMPDIR/test.db --blob-root $TMPDIR/blobs --ipc-socket /tmp/xvn-events.sock &

# Separate terminal:
cargo run -p xvision-cli -- autoresearch session-init --config config/autoresearch.toml.example --db $TMPDIR/test.db
cargo run -p xvision-cli -- autoresearch evening-cycle --session-id <id-from-init> --config config/autoresearch.toml.example --db $TMPDIR/test.db --mock --ipc-socket /tmp/xvn-events.sock
```

Open `http://127.0.0.1:7777/`. Live tab should fill with mutation/eval/seal events as the cycle runs.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-dashboard/static/js/views/live_cycle.js
git commit -m "feat(dashboard): live evening-cycle viewer (event stream + KPIs)"
```

---

## Phase E — View 2: Genealogy tree

### Task 10: genealogy.js (D3 force-directed)

D3 force-directed graph (or radial when N > 20 lineages — for v1 we use force-directed for everything). Nodes sized by trade count; colored by lineage; edges encode mutation kind via stroke style. Click → selects the node and switches to the diff inspector tab pre-filled.

**File:** `crates/xvision-dashboard/static/js/views/genealogy.js`

- [ ] **Step 1: Implement genealogy.js**

```js
// /static/js/views/genealogy.js
import { get } from "/static/js/shared/api.js";
import { shortHash } from "/static/js/shared/format.js";
import { on } from "/static/js/bus.js";

const root = document.getElementById("view-genealogy");

let nodes = [];
let links = [];
let simulation = null;

function render() {
  root.innerHTML = `<svg class="lineage-svg" id="genealogy-svg"></svg>`;
  draw();
}

async function refresh() {
  const list = await get("/api/lineage?limit=2000");
  nodes = list.map((n) => ({
    id: n.bundle_hash,
    parent: n.parent_hash,
    status: n.status,
    metrics: n.metrics,
  }));
  links = list
    .filter((n) => n.parent_hash)
    .map((n) => ({ source: n.parent_hash, target: n.bundle_hash }));
  if (simulation) draw();
}

function draw() {
  if (typeof d3 === "undefined") {
    root.innerHTML = `<p>D3 failed to load — check /static/vendor/d3.v7.min.js.</p>`;
    return;
  }
  const svg = d3.select("#genealogy-svg");
  svg.selectAll("*").remove();
  const w = svg.node().clientWidth;
  const h = svg.node().clientHeight;
  const g = svg.append("g");

  svg.call(d3.zoom().on("zoom", (e) => g.attr("transform", e.transform)));

  const link = g.append("g").attr("stroke", "rgba(255,255,255,0.25)").attr("stroke-width", 1)
    .selectAll("line")
    .data(links)
    .enter().append("line");

  const node = g.append("g")
    .selectAll("circle")
    .data(nodes, (d) => d.id)
    .enter().append("circle")
    .attr("r", (d) => 4 + Math.min(12, (d.metrics?.trades_attributed || 0) / 5))
    .attr("fill", (d) => statusColor(d.status))
    .attr("stroke", "rgba(255,255,255,0.6)")
    .attr("stroke-width", 0.5)
    .style("cursor", "pointer")
    .on("click", (_, d) => {
      window.dispatchEvent(new CustomEvent("xvn:select-bundle", { detail: { bundle_hash: d.id } }));
      const diff = document.querySelector(".tab[data-tab='diff']");
      if (diff) diff.click();
    });

  node.append("title").text((d) => `${d.id}\nstatus: ${d.status}`);

  simulation = d3.forceSimulation(nodes)
    .force("link", d3.forceLink(links).id((d) => d.id).distance(40))
    .force("charge", d3.forceManyBody().strength(-30))
    .force("center", d3.forceCenter(w / 2, h / 2))
    .on("tick", () => {
      link
        .attr("x1", (d) => d.source.x).attr("y1", (d) => d.source.y)
        .attr("x2", (d) => d.target.x).attr("y2", (d) => d.target.y);
      node.attr("cx", (d) => d.x).attr("cy", (d) => d.y);
    });
}

function statusColor(s) {
  if (s === "active") return "#3fb950";
  if (s === "ghost") return "#6e7681";
  if (s === "quarantined") return "#d29922";
  return "#58a6ff";
}

render();
refresh().catch(console.error);

on("mutation_committed", () => refresh());
on("mutation_rejected", () => refresh());
on("mutation_quarantined", () => refresh());
```

- [ ] **Step 2: Smoke**

Same as Task 9; switch to the Genealogy tab. Refresh once a mutation commits — node count grows live.

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-dashboard/static/js/views/genealogy.js
git commit -m "feat(dashboard): genealogy view (D3 force-directed, status-coloured)"
```

---

## Phase F — View 3: Mutation diff inspector

### Task 11: diff_inspector.js

Three-pane layout: prose diff (markdown red/green via simple line-by-line rendering), param diff table, tool diff chips. Below: the LLM finding's summary + regime affinity + failure modes + confidence chip.

**File:** `crates/xvision-dashboard/static/js/views/diff_inspector.js`

- [ ] **Step 1: Implement diff_inspector.js**

```js
// /static/js/views/diff_inspector.js
import { get } from "/static/js/shared/api.js";
import { shortHash } from "/static/js/shared/format.js";

const root = document.getElementById("view-diff");

function render(state) {
  if (!state) {
    root.innerHTML = `<p>Select a node in the Genealogy view to inspect its mutation.</p>`;
    return;
  }
  const { node, diff, finding } = state;
  const proseHtml = renderProseDiff(diff?.prose_diff || "");
  const paramRows = (diff?.param_changes || []).map(
    (c) => `<tr><td class="cell-hash">${c.key}</td><td class="rem">${JSON.stringify(c.old)}</td><td class="add">${JSON.stringify(c.new)}</td></tr>`,
  ).join("");
  const toolAdds = (diff?.tool_changes?.added || []).map((t) => `<span class="add">+${t}</span>`).join(" ");
  const toolRems = (diff?.tool_changes?.removed || []).map((t) => `<span class="rem">−${t}</span>`).join(" ");

  root.innerHTML = `
    <div class="kpi"><div class="label">child</div><div class="value cell-hash">${shortHash(node.bundle_hash)}</div></div>
    <div class="kpi"><div class="label">parent</div><div class="value cell-hash">${shortHash(node.parent_hash || "—")}</div></div>
    <div class="kpi"><div class="label">status</div><div class="value">${node.status}</div></div>
    <div class="diff-pane" style="margin-top:16px;">
      <div><strong>prose</strong>\n${proseHtml || "<em>no prose change</em>"}</div>
      <div><strong>params</strong>
        <table style="width:100%; border-collapse:collapse; margin-top:8px;">
          <tr><th align="left">key</th><th align="left">old</th><th align="left">new</th></tr>
          ${paramRows || "<tr><td colspan=3><em>no param changes</em></td></tr>"}
        </table>
      </div>
      <div><strong>tools</strong>
        <p>${toolAdds || "<em>no additions</em>"}</p>
        <p>${toolRems || "<em>no removals</em>"}</p>
      </div>
    </div>
    <section style="margin-top:24px;">
      <h2>Finding</h2>
      ${finding ? `
        <p>${escapeHtml(finding.summary || "")}</p>
        <p><strong>regimes:</strong> ${(finding.regime_affinity || []).map(escapeHtml).join(", ") || "—"}</p>
        <p><strong>failure modes:</strong> ${(finding.failure_modes || []).map(escapeHtml).join("; ") || "—"}</p>
        <p><strong>confidence:</strong> ${finding.confidence || "—"} <em>(metrics-blind)</em></p>
      ` : "<p><em>No finding written for this node (gate rejected or finding pending).</em></p>"}
    </section>
  `;
}

function renderProseDiff(diff) {
  if (!diff) return "";
  return diff
    .split("\n")
    .map((line) => {
      if (line.startsWith("+")) return `<span class="add">${escapeHtml(line)}</span>`;
      if (line.startsWith("-")) return `<span class="rem">${escapeHtml(line)}</span>`;
      return escapeHtml(line);
    })
    .join("\n");
}

function escapeHtml(s) {
  return String(s).replace(/[&<>]/g, (c) => ({"&":"&amp;","<":"&lt;",">":"&gt;"}[c]));
}

render(null);

window.addEventListener("xvn:select-bundle", async (ev) => {
  const hash = ev.detail.bundle_hash;
  const node = await get(`/api/lineage/${hash}`);
  let diff = null;
  if (node.diff_blob_hash) {
    // Diff blobs aren't exposed via a dedicated endpoint; we fetch via the
    // generic blob route added in Task 12. For now, embed a placeholder.
    try {
      diff = await get(`/api/blobs/${node.diff_blob_hash}`);
    } catch (_) { /* ignore */ }
  }
  let finding = null;
  if (node.finding_blob_hash) {
    try {
      const detail = await get(`/api/findings/${hash}`);
      finding = detail.finding;
    } catch (_) {}
  }
  render({ node, diff, finding });
});
```

- [ ] **Step 2: Add `/api/blobs/:hash` for the diff inspector**

The diff inspector needs the raw diff blob. Add a GET handler:

```rust
// src/api/mod.rs (additions)
.route("/blobs/:hash", get(blobs::get))

// src/api/blobs.rs
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;

use crate::state::AppState;
use xvision_engine::autoresearch::content_hash::ContentHash;

pub async fn get(
    Path(hash): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let h = ContentHash::from_hex(&hash).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    let v = state.store.blobs().get_json(&h).await.map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    Ok(Json(v))
}
```

(Add `pub mod blobs;` in `src/api/mod.rs`.)

- [ ] **Step 3: Smoke**

In the dashboard, click a node in the Genealogy view; the Diff inspector tab should open with the mutation rendered.

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-dashboard/static/js/views/diff_inspector.js crates/xvision-dashboard/src/api/
git commit -m "feat(dashboard): mutation diff inspector + /api/blobs/:hash"
```

---

## Phase G — View 4: Mutator-skill ladder

### Task 12: mutator_ladder.js

Side-by-side with the strategy ladder (which lives in view 5). Shows acceptance rate over time, calibration scaffold, regime bias if present.

**File:** `crates/xvision-dashboard/static/js/views/mutator_ladder.js`

- [ ] **Step 1: Implement mutator_ladder.js**

```js
// /static/js/views/mutator_ladder.js
import { get } from "/static/js/shared/api.js";
import { fmtPct } from "/static/js/shared/format.js";
import { on } from "/static/js/bus.js";

const root = document.getElementById("view-ladder");

async function refresh() {
  const snaps = await get("/api/ladder/snapshots");
  if (!snaps.length) {
    root.innerHTML = `<p>No mutator-skill snapshots yet. Run a cycle.</p>`;
    return;
  }
  const last = snaps[snaps.length - 1].snapshot;
  const series = snaps.map((s) => ({ x: s.sampled_at, y: s.snapshot.acceptance_rate }));
  const proposed = snaps.map((s) => ({ x: s.sampled_at, y: s.snapshot.proposed_count }));

  root.innerHTML = `
    <div class="kpi"><div class="label">acceptance rate</div><div class="value">${fmtPct(last.acceptance_rate)}</div></div>
    <div class="kpi"><div class="label">accepted / proposed</div><div class="value">${last.accepted_count} / ${last.proposed_count}</div></div>
    <div class="kpi"><div class="label">snapshots</div><div class="value">${snaps.length}</div></div>
    <svg id="ladder-svg" style="width:100%; height:360px; margin-top:16px;"></svg>
  `;
  drawTrend("#ladder-svg", series, proposed);
}

function drawTrend(sel, accSeries, proposedSeries) {
  if (typeof d3 === "undefined") return;
  const svg = d3.select(sel);
  svg.selectAll("*").remove();
  const w = svg.node().clientWidth;
  const h = svg.node().clientHeight;
  const margin = { top: 20, right: 80, bottom: 40, left: 50 };
  const x = d3.scaleBand().domain(accSeries.map((d, i) => i)).range([margin.left, w - margin.right]).padding(0.15);
  const yLeft = d3.scaleLinear().domain([0, 1]).range([h - margin.bottom, margin.top]);
  const yRight = d3.scaleLinear().domain([0, d3.max(proposedSeries, (d) => d.y) || 1]).range([h - margin.bottom, margin.top]);

  svg.append("g").attr("transform", `translate(${margin.left},0)`).call(d3.axisLeft(yLeft).ticks(5).tickFormat(d3.format(".0%")))
    .selectAll("text").style("fill", "var(--fg-dim)");
  svg.append("g").attr("transform", `translate(${w - margin.right},0)`).call(d3.axisRight(yRight).ticks(5))
    .selectAll("text").style("fill", "var(--fg-dim)");

  svg.append("g").selectAll("rect")
    .data(proposedSeries).enter().append("rect")
    .attr("x", (_, i) => x(i)).attr("y", (d) => yRight(d.y)).attr("width", x.bandwidth())
    .attr("height", (d) => h - margin.bottom - yRight(d.y))
    .attr("fill", "rgba(88, 166, 255, 0.18)");

  svg.append("path").datum(accSeries)
    .attr("fill", "none").attr("stroke", "#3fb950").attr("stroke-width", 2)
    .attr("d", d3.line()
      .x((_, i) => x(i) + x.bandwidth() / 2)
      .y((d) => yLeft(d.y)));
}

refresh().catch(console.error);
on("ladder_snapshot", () => refresh());
```

- [ ] **Step 2: Commit**

```bash
git add crates/xvision-dashboard/static/js/views/mutator_ladder.js
git commit -m "feat(dashboard): mutator-skill ladder (acceptance trend + proposal volume)"
```

---

## Phase H — View 5: Ladder with provenance

### Task 13: ladder_provenance.js

Existing strategy ladder, augmented with lineage depth + parent hash + one-line mutation summary. Click row → switches to the Genealogy tab zoomed to that node.

**File:** `crates/xvision-dashboard/static/js/views/ladder_provenance.js`

- [ ] **Step 1: Implement ladder_provenance.js**

```js
// /static/js/views/ladder_provenance.js
import { get } from "/static/js/shared/api.js";
import { shortHash, fmtSharpe } from "/static/js/shared/format.js";

const root = document.getElementById("view-provenance");

async function refresh() {
  const list = await get("/api/lineage?status=active&limit=200");
  if (!list.length) {
    root.innerHTML = `<p>No active lineages yet.</p>`;
    return;
  }
  // Compute lineage depth by walking parent_hash chains.
  const byHash = new Map(list.map((n) => [n.bundle_hash, n]));
  function depth(h) {
    let d = 0;
    let cur = byHash.get(h);
    while (cur && cur.parent_hash) {
      d += 1;
      cur = byHash.get(cur.parent_hash);
      if (d > 200) break;
    }
    return d;
  }
  const rows = list.map((n) => ({
    id: n.bundle_hash,
    parent: n.parent_hash,
    pnl: (n.metrics && n.metrics.realized_pnl_attributed) || 0,
    trades: (n.metrics && n.metrics.trades_attributed) || 0,
    days: (n.metrics && n.metrics.days_alive) || 0,
    depth: depth(n.bundle_hash),
  }));
  rows.sort((a, b) => b.pnl - a.pnl);
  const tbody = rows.map((r) => `
    <tr data-bundle="${r.id}" style="cursor:pointer;">
      <td class="cell-hash">${shortHash(r.id)}</td>
      <td>${r.depth}</td>
      <td class="cell-hash">${shortHash(r.parent)}</td>
      <td>${r.pnl.toFixed(2)}</td>
      <td>${r.trades}</td>
      <td>${r.days}</td>
    </tr>
  `).join("");
  root.innerHTML = `
    <table style="width:100%; border-collapse:collapse;">
      <thead>
        <tr><th align="left">bundle</th><th align="left">depth</th><th align="left">parent</th><th align="left">pnl</th><th align="left">trades</th><th align="left">days</th></tr>
      </thead>
      <tbody>${tbody}</tbody>
    </table>
  `;
  root.querySelectorAll("tr[data-bundle]").forEach((row) => {
    row.addEventListener("click", () => {
      const h = row.dataset.bundle;
      window.dispatchEvent(new CustomEvent("xvn:select-bundle", { detail: { bundle_hash: h } }));
      const tab = document.querySelector(".tab[data-tab='diff']");
      if (tab) tab.click();
    });
  });
}

refresh().catch(console.error);
```

- [ ] **Step 2: Commit**

```bash
git add crates/xvision-dashboard/static/js/views/ladder_provenance.js
git commit -m "feat(dashboard): ladder-with-provenance (lineage depth + click-to-inspect)"
```

---

## Phase I — Polish + smoke

### Task 14: End-to-end smoke

- [ ] **Step 1: Multi-process smoke**

```bash
# Term 1: dashboard
TMPDIR=$(mktemp -d); export TMPDIR
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/001_init.sql 2>/dev/null
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/002_eval.sql
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/003_autoresearch.sql
sqlite3 $TMPDIR/test.db < crates/xvision-engine/migrations/004_autoresearch_evals.sql
cargo run -p xvision-cli -- dashboard serve --db $TMPDIR/test.db --blob-root $TMPDIR/blobs --ipc-socket /tmp/xvn-events.sock

# Term 2: orchestrator
cargo run -p xvision-cli -- autoresearch session-init --config config/autoresearch.toml.example --db $TMPDIR/test.db --key-path $TMPDIR/op.ed25519
SESSION=<from above>
cargo run -p xvision-cli -- autoresearch evening-cycle --session-id $SESSION --config config/autoresearch.toml.example --db $TMPDIR/test.db --mock --ipc-socket /tmp/xvn-events.sock
```

Open `http://127.0.0.1:7777/`. Verify:
- Live tab streams events live during the cycle.
- Genealogy tab populates with nodes once mutations commit.
- Click a node → diff inspector opens with the mutation rendered + finding (if any).
- Mutator-skill ladder tab shows acceptance trend.
- Ladder-with-provenance lists active nodes with lineage depth.

- [ ] **Step 2: Browser cross-check**

Open in Chrome + Firefox. Verify SSE connects, D3 renders, no console errors.

- [ ] **Step 3: Projector check**

Open at full screen, projected to a wall (or simulate by scaling browser zoom to 175%). Live cycle viewer's KPIs and event stream remain readable from 5 meters per design tokens.

- [ ] **Step 4: Commit**

```bash
git commit --allow-empty -m "chore(dashboard): AR-3 cross-browser + projector smoke verified"
```

---

### Task 15: Workspace check + AR-3 done

- [ ] **Step 1: Full workspace test**

```bash
cargo test --workspace 2>&1 | tail -40
```

Expected: all tests pass (eval engine + autoresearch AR-1 + AR-2 + dashboard AR-3 + everything else). New tests: 7 in `crates/xvision-dashboard/tests/` (api_lineage, api_seals, api_ladder, api_diversity, api_findings, sse_smoke, ipc_bridge).

- [ ] **Step 2: Fmt + clippy**

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
```

- [ ] **Step 3: Tag**

```bash
git commit --allow-empty -m "chore(autoresearch): AR-3 (dashboard 5 views + SSE) done — Wk 4 milestone"
git tag autoresearch-ar3
```

---

## Self-review checklist

**Spec coverage (autoresearch design §9):**
- [x] §9 #1 Live evening cycle viewer — live_cycle.js (Task 9)
- [x] §9 #1 Real-time SSE stream during evening run — sse.rs + bus.js (Tasks 5, 8)
- [x] §9 #1 Token / cost meter at top — KPI in live_cycle.js
- [x] §9 #1 Reads on a projector at 5m — design tokens in tokens.css (var(--size-xl) = 1.6rem; high-contrast palette; live event stream uses monospace)
- [x] §9 #2 Genealogy tree (D3 force-directed) — genealogy.js (Task 10)
- [x] §9 #2 Status-coloured nodes; ghost branches faded — statusColor() in genealogy.js
- [x] §9 #2 Click → drawer — implemented as cross-tab hand-off via xvn:select-bundle event into diff inspector (Task 11)
- [x] §9 #3 Mutation diff inspector (3-pane: prose / params / tools + finding) — diff_inspector.js (Task 11)
- [x] §9 #4 Mutator-skill ladder (acceptance + token efficiency) — mutator_ladder.js (Task 12). Calibration + regime bias panels reserved for future when AR-2's `regime_bias` field starts populating beyond `{}`.
- [x] §9 #5 Ladder with provenance (depth + parent hash + click-to-inspect) — ladder_provenance.js (Task 13)
- [x] §9 SSE event taxonomy (mutation_proposed / committed / rejected / quarantined / lineage_forked / canary_outcome / diversity_updated / cycle_sealed) — wired through sse.rs (Task 5) and bus.js (Task 8)
- [x] §9 Demo replay fallback `xvn autoresearch demo` — *delivered in AR-2 Task 11*; AR-3 doesn't duplicate

**Out of scope (cross-checked against companion plans):**
- Marketplace tab — MP-1 (sixth tab, not added here)
- Per-trade trace tape rendering inside diff inspector — uses placeholder text in v1; the trace fetcher is itself an AR-2 follow-up that AR-3 picks up cosmetically
- Authentication / multi-user — not needed for hackathon demo
- Mobile responsive — desktop + projector only

**Placeholder scan:**
- `xvn dashboard serve` smoke depends on having migrations applied; the smoke command sequence sets that up explicitly in Task 14.
- The `regime_bias` field on the mutator ladder snapshot is `{}` in v1 (AR-2 Task 8 leaves it unfilled); the dashboard renders an empty state. Replacing with regime-bias bars is a 30-line addition once findings start populating regime_affinity densely.
- D3 vendor copy: pinned to v7.x; tests don't exercise D3 (UI is verified manually). If `static/vendor/d3.v7.min.js` is missing, the page shows `D3 failed to load — check ...` (handled in genealogy.js's draw()). The Task 8 smoke commit downloads it.

**Type consistency:**
- SSE event names in `sse::events_handler` match the literal strings registered in `bus.js`'s `EVENT_KINDS` array (cycle_started, mutation_proposed, mutation_evaluating, mutation_committed, mutation_rejected, mutation_quarantined, lineage_forked, judge_wrote_finding, canary_outcome, diversity_updated, ladder_snapshot, cycle_sealed, cycle_failed). Cross-checked against `progress.rs::AutoresearchEvent` variants from AR-2 — all 13 covered.
- API response shapes (`LineageNodeRow`, `SealRow`, `LadderRow`, `DiversitySampleRow`, `CanaryRunRow`, `FindingDetail`) consistent between Rust handlers and the JS consumers (each consumer reads field names defined in the corresponding Rust struct).
- The cross-tab `xvn:select-bundle` custom event has identical payload shape (`{ bundle_hash: string }`) in genealogy.js (emitter), ladder_provenance.js (emitter), and diff_inspector.js (consumer).

**Frequent commits:** 15 tasks → ~15 commits.

---

## What ships after AR-3

`xvn dashboard serve` boots the live demo surface. Run alongside `xvn autoresearch evening-cycle` (or the scheduler-driven nightly job) and the five views render in real time:

1. **Live cycle viewer** — the headline. SSE event stream, KPIs, projector-friendly.
2. **Genealogy tree** — D3 force-directed; status-coloured; click-to-inspect.
3. **Mutation diff inspector** — three-pane prose/params/tools + LLM finding.
4. **Mutator-skill ladder** — acceptance trend + proposal volume; calibration/regime panels stubbed pending AR-2 follow-up data.
5. **Ladder with provenance** — active nodes with lineage depth, click → diff inspector.

The Wk 4 milestone (autoresearch spec §10): "Dashboard renders live cycle in real time" is satisfied.

**Next plan: MP-1** (marketplace plugin) lands the sixth tab — NFT mints, Merkle receipts, in-house attesters, anchor history, operator action panel — and the on-chain integration. AR-1/AR-2/AR-3 stay chain-free; MP-1 reads the CycleSeal artifacts produced by AR-2 and the API endpoints exposed by AR-3 (it adds new routes under `/api/marketplace/*` rather than modifying existing ones).
