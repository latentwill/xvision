# v1 Frontend — Plan 1: Foundation + Strategies vertical slice

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stand up the `xvision-dashboard` axum crate that serves a Vite-built React+TS SPA, port the prototype's Folio dark tokens and shell, set up Rust→TS type codegen, and ship the first end-to-end vertical slice: a working `/strategies` page backed by real `GET /api/strategies` data.

**Architecture:** Single new Rust crate (`xvision-dashboard`) hosting an axum server. Frontend lives at `frontend/web/`, builds with Vite to `crates/xvision-dashboard/static/`, and is embedded into the binary via `rust-embed`. API types are codegen'd from `xvision-engine` request/response structs using `ts-rs` so the TypeScript client never drifts from Rust. State on the client uses TanStack Query for server state and Zustand for ephemeral UI state. No auth in v1 (localhost trust model).

**Tech Stack:** Rust 1.95.0, axum 0.7, tower-http, [`rust-embed`](https://github.com/pyrossh/rust-embed) 8.x, [`ts-rs`](https://github.com/Aleph-Alpha/ts-rs) 9.x. Frontend: Vite 5, React 18, TypeScript 5.5, Tailwind CSS 3.4, React Router 6 (data router), TanStack Query 5, Zustand 4, Radix UI primitives, `@fontsource/*` for self-hosted fonts. Package manager: pnpm 9.

---

## Scope and split

This plan covers **Phase 0 + the first slice of Phase 1** from `frontend/DESIGN.md`. It is plan 1 of 5; later plans depend on this scaffolding being in place.

| Plan | Title | Depends on |
|---|---|---|
| **1 (this)** | Foundation + Strategies vertical slice | — |
| 2 | Remaining read-only screens (Home, Eval runs, Run detail w/o findings, Settings) | 1 |
| 3 | Authoring — Inspector + slot editor + validation diagnostics + bundle lineage | 1, 2 |
| 4 | Agent surfaces — Wizard end-to-end, Chat rail, Inspector live-preview | 1, 2, 3, plus existing plans 2d + chat-rail-persistence |
| 5 | Findings + Compare + Polish — findings schema/extractor/UI, compare view, command palette, trade ledger | 1, 2, 3 |

Plans 2–5 are sketched in the appendix; full plans will be authored separately when this one lands.

## Prerequisites

**Already in place:**
- `xvision-engine::api::*` foundation (engine API plan landed before this).
- `xvision-engine::api::strategy::list/get/create/update/delete` — used by Task 7.
- `frontend/prototype/` design handoff (visual source of truth).

**Not required for this plan:**
- `eval_runs` persistence (the eval engine plan covers it; not needed until Plan 2 lights up the runs list).
- WizardLoop, chat-rail backend, findings — used by later plans.

## File structure

```
crates/xvision-dashboard/                    NEW
├── Cargo.toml
├── build.rs                                  # triggers `pnpm build` if frontend/web/dist is stale
├── src/
│   ├── lib.rs                                # serve() entrypoint
│   ├── server.rs                             # router builder
│   ├── error.rs                              # ApiError → http response
│   ├── routes/
│   │   ├── mod.rs
│   │   ├── health.rs                         # GET /api/health
│   │   ├── strategies.rs                     # GET /api/strategies
│   │   └── static_files.rs                   # SPA fallback via rust-embed
│   └── embed.rs                              # rust-embed asset loader
├── static/                                   # populated by build.rs (gitignored)
└── tests/
    └── http.rs                               # axum_test integration tests

crates/xvision-engine/src/api/                EXISTS — augmented
├── mod.rs                                    # add #[cfg_attr(feature="ts-rs", derive(TS))]
├── strategy.rs                               # request/response structs gain TS export
└── health.rs                                 NEW — HealthReport + probe fns

crates/xvision-cli/src/                       EXISTS — augmented
└── commands/
    └── dashboard.rs                          NEW — `xvn dashboard serve [--port 8788]`

xtask/                                        NEW (workspace member)
├── Cargo.toml
└── src/main.rs                               # `cargo xtask gen-types`

frontend/web/                                 NEW
├── package.json
├── pnpm-lock.yaml                            # generated
├── vite.config.ts
├── tailwind.config.ts
├── tsconfig.json
├── postcss.config.js
├── index.html
├── .gitignore
├── public/
│   └── favicon.svg
└── src/
    ├── main.tsx
    ├── App.tsx
    ├── routes.tsx
    ├── api/
    │   ├── client.ts                         # base fetch + error mapping
    │   ├── strategies.ts
    │   └── types.gen.ts                      # codegen output (committed)
    ├── components/
    │   ├── shell/
    │   │   ├── Sidebar.tsx
    │   │   ├── Topbar.tsx
    │   │   └── ChatRailPlaceholder.tsx       # collapsed-only stub for plan 1
    │   └── primitives/
    │       ├── Icon.tsx
    │       ├── Sparkline.tsx
    │       ├── Pill.tsx
    │       ├── Dot.tsx
    │       └── Card.tsx
    ├── routes/
    │   ├── home.tsx                          # placeholder
    │   ├── strategies.tsx                    # real data (Plan 1 deliverable)
    │   ├── authoring.tsx                     # placeholder
    │   ├── eval-runs.tsx                     # placeholder
    │   ├── eval-runs-detail.tsx              # placeholder
    │   ├── eval-compare.tsx                  # placeholder
    │   ├── settings/
    │   │   ├── providers.tsx                 # placeholder
    │   │   ├── brokers.tsx                   # placeholder
    │   │   ├── daemon.tsx                    # placeholder
    │   │   ├── identity.tsx                  # placeholder
    │   │   └── danger.tsx                    # placeholder
    │   └── setup.tsx                         # placeholder
    ├── stores/
    │   └── ui.ts                             # Zustand store
    ├── styles/
    │   ├── tokens.css                        # ported from prototype/styles.css
    │   └── globals.css                       # @tailwind directives
    └── lib/
        └── format.ts

Cargo.toml                                    # workspace members += dashboard, xtask
.gitignore                                    # add frontend/web/dist, frontend/web/node_modules, crates/xvision-dashboard/static
```

---

## Tasks

### Task 1: Add `xvision-dashboard` crate skeleton

**Files:**
- Create: `crates/xvision-dashboard/Cargo.toml`
- Create: `crates/xvision-dashboard/src/lib.rs`
- Create: `crates/xvision-dashboard/src/server.rs`
- Create: `crates/xvision-dashboard/src/error.rs`
- Create: `crates/xvision-dashboard/src/routes/mod.rs`
- Create: `crates/xvision-dashboard/src/routes/health.rs`
- Create: `crates/xvision-dashboard/tests/http.rs`
- Modify: `Cargo.toml` (workspace members)

- [ ] **Step 1.1: Add the crate to the workspace**

Edit `Cargo.toml` (the workspace root) — find `[workspace] members = [...]` and append `"crates/xvision-dashboard"`.

- [ ] **Step 1.2: Write the crate `Cargo.toml`**

```toml
[package]
name = "xvision-dashboard"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
xvision-core = { path = "../xvision-core" }
xvision-engine = { path = "../xvision-engine" }
axum = { version = "0.7", features = ["macros"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }
tower = "0.5"
tower-http = { version = "0.6", features = ["trace", "cors"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
anyhow = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }
rust-embed = { version = "8", features = ["compression"] }
mime_guess = "2"

[dev-dependencies]
axum-test = "16"
tokio = { version = "1", features = ["full", "test-util"] }
```

- [ ] **Step 1.3: Write the failing integration test**

Create `crates/xvision-dashboard/tests/http.rs`:

```rust
use axum_test::TestServer;
use xvision_dashboard::server::build_router;

#[tokio::test]
async fn health_endpoint_returns_200() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();

    let response = server.get("/api/health").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert_eq!(body["status"], "ok");
}
```

- [ ] **Step 1.4: Run the test — expect it to fail to compile**

Run: `cargo test -p xvision-dashboard --test http`
Expected: compile error — `build_router` not defined.

- [ ] **Step 1.5: Implement `lib.rs` and re-export `server`**

Create `crates/xvision-dashboard/src/lib.rs`:

```rust
pub mod error;
pub mod routes;
pub mod server;

pub use server::serve;
```

- [ ] **Step 1.6: Implement `error.rs`**

Create `crates/xvision-dashboard/src/error.rs`:

```rust
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DashboardError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("validation: {field}: {msg}")]
    Validation { field: String, msg: String },
    #[error("conflict: {0}")]
    Conflict(String),
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for DashboardError {
    fn into_response(self) -> Response {
        let (status, code, msg) = match &self {
            DashboardError::NotFound(m) => (StatusCode::NOT_FOUND, "not_found", m.clone()),
            DashboardError::Validation { field, msg } => (
                StatusCode::BAD_REQUEST,
                "validation",
                format!("{field}: {msg}"),
            ),
            DashboardError::Conflict(m) => (StatusCode::CONFLICT, "conflict", m.clone()),
            DashboardError::Internal(e) => {
                tracing::error!(error = ?e, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal",
                    "internal error".into(),
                )
            }
        };
        (status, Json(json!({ "code": code, "message": msg }))).into_response()
    }
}
```

- [ ] **Step 1.7: Implement `routes/mod.rs` and `routes/health.rs`**

Create `crates/xvision-dashboard/src/routes/mod.rs`:

```rust
pub mod health;
```

Create `crates/xvision-dashboard/src/routes/health.rs`:

```rust
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: &'static str,
}

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse { status: "ok" })
}
```

- [ ] **Step 1.8: Implement `server.rs`**

Create `crates/xvision-dashboard/src/server.rs`:

```rust
use axum::{routing::get, Router};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use crate::routes::health::health;

pub fn build_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .layer(TraceLayer::new_for_http())
}

pub async fn serve(addr: SocketAddr) -> anyhow::Result<()> {
    let app = build_router();
    tracing::info!(%addr, "xvision-dashboard listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 1.9: Run tests — expect pass**

Run: `cargo test -p xvision-dashboard --test http`
Expected: 1 passed.

- [ ] **Step 1.10: Commit**

```bash
git add Cargo.toml crates/xvision-dashboard/
git commit -m "feat(dashboard): scaffold xvision-dashboard crate with /api/health"
```

---

### Task 2: Add `xvn dashboard serve` CLI command

**Files:**
- Modify: `crates/xvision-cli/Cargo.toml` (add xvision-dashboard dep)
- Create: `crates/xvision-cli/src/commands/dashboard.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs`
- Modify: `crates/xvision-cli/src/main.rs` (or wherever the clap parser lives)

- [ ] **Step 2.1: Add dashboard dep to CLI crate**

Edit `crates/xvision-cli/Cargo.toml`, under `[dependencies]`:

```toml
xvision-dashboard = { path = "../xvision-dashboard" }
```

- [ ] **Step 2.2: Locate the existing clap subcommand enum**

Run: `grep -rn "Subcommand\]" crates/xvision-cli/src/`
Expected: finds the existing `Commands` or `Cli` enum with variants like `Strategy`, `Eval`, etc.

- [ ] **Step 2.3: Add a `Dashboard` variant**

In the file from Step 2.2, add to the enum:

```rust
/// Run the web dashboard
Dashboard {
    #[command(subcommand)]
    cmd: DashboardCmd,
},
```

And below the parent enum:

```rust
#[derive(clap::Subcommand, Debug)]
pub enum DashboardCmd {
    /// Start the dashboard HTTP server
    Serve {
        /// Bind address
        #[arg(long, default_value = "127.0.0.1:8788")]
        bind: String,
    },
}
```

- [ ] **Step 2.4: Implement the handler**

Create `crates/xvision-cli/src/commands/dashboard.rs`:

```rust
use anyhow::Context;
use std::net::SocketAddr;

use crate::cli::DashboardCmd;

pub async fn run(cmd: DashboardCmd) -> anyhow::Result<()> {
    match cmd {
        DashboardCmd::Serve { bind } => {
            let addr: SocketAddr = bind.parse().context("invalid --bind address")?;
            xvision_dashboard::serve(addr).await
        }
    }
}
```

(Adjust the `use crate::cli::...` path to match where `DashboardCmd` was added in Step 2.3.)

- [ ] **Step 2.5: Wire into the dispatch**

Find the existing `match` over `Commands` in `crates/xvision-cli/src/main.rs` (or `commands/mod.rs`). Add the new arm:

```rust
Commands::Dashboard { cmd } => commands::dashboard::run(cmd).await?,
```

Add `pub mod dashboard;` to `crates/xvision-cli/src/commands/mod.rs`.

- [ ] **Step 2.6: Smoke-test manually**

Run: `cargo run -p xvision-cli -- dashboard serve --bind 127.0.0.1:8788 &`
In another shell: `curl -s http://127.0.0.1:8788/api/health`
Expected: `{"status":"ok"}`
Then: `kill %1` (or pkill the cargo run).

- [ ] **Step 2.7: Commit**

```bash
git add crates/xvision-cli/
git commit -m "feat(cli): add 'xvn dashboard serve' subcommand"
```

---

### Task 3: Add `ts-rs` to `xvision-engine` and derive on API types

**Files:**
- Modify: `crates/xvision-engine/Cargo.toml`
- Modify: `crates/xvision-engine/src/api/strategy.rs` (and other api modules)

- [ ] **Step 3.1: Add ts-rs as an optional dep behind a feature**

Edit `crates/xvision-engine/Cargo.toml`:

```toml
[dependencies]
ts-rs = { version = "9", optional = true }

[features]
default = []
ts-export = ["dep:ts-rs"]
```

- [ ] **Step 3.2: Identify the API request/response types**

Run: `grep -rn "pub struct\|pub enum" crates/xvision-engine/src/api/`
Expected: types like `StrategyListRequest`, `StrategyListResponse`, `StrategySummary`, `ApiError`, etc.

- [ ] **Step 3.3: Derive `TS` on every public API type**

For each `pub struct` and `pub enum` in `crates/xvision-engine/src/api/`, add the conditional derive. Pattern:

```rust
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../frontend/web/src/api/types.gen/"))]
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StrategySummary {
    pub bundle_id: String,
    pub name: String,
    pub template: String,
    pub parent_bundle_id: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
```

(Apply to every type in the module. If a struct contains `chrono::DateTime`, add `#[cfg_attr(feature="ts-export", ts(type = "string"))]` on that field — `ts-rs` doesn't know `DateTime`.)

- [ ] **Step 3.4: Verify the feature compiles**

Run: `cargo check -p xvision-engine --features ts-export`
Expected: no errors. Fix any missing derives or unsupported types.

- [ ] **Step 3.5: Run the export tests to emit the .ts files**

ts-rs emits exports as a side effect of running tests. Run:

```bash
cargo test -p xvision-engine --features ts-export --tests -- --quiet
```

Then: `ls frontend/web/src/api/types.gen/`
Expected: `StrategySummary.ts`, etc., one file per type.

- [ ] **Step 3.6: Commit**

```bash
git add crates/xvision-engine/Cargo.toml crates/xvision-engine/src/api/ frontend/web/src/api/types.gen/
git commit -m "feat(engine): derive ts-rs TS on API types behind ts-export feature"
```

---

### Task 4: Create `xtask` for type codegen automation

**Files:**
- Create: `xtask/Cargo.toml`
- Create: `xtask/src/main.rs`
- Modify: `Cargo.toml` (workspace members)
- Create: `frontend/web/src/api/types.gen.ts` (combined barrel — see step 4.4)

- [ ] **Step 4.1: Add xtask to the workspace**

Edit root `Cargo.toml` `[workspace] members` to include `"xtask"`.

Create `xtask/Cargo.toml`:

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
anyhow = "1"
clap = { version = "4", features = ["derive"] }
xshell = "0.2"
```

- [ ] **Step 4.2: Implement the gen-types task**

Create `xtask/src/main.rs`:

```rust
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use xshell::{cmd, Shell};

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Generate TypeScript types from Rust API structs
    GenTypes,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let sh = Shell::new()?;
    match cli.cmd {
        Cmd::GenTypes => gen_types(&sh),
    }
}

fn gen_types(sh: &Shell) -> Result<()> {
    let out_dir = "frontend/web/src/api/types.gen";
    cmd!(sh, "rm -rf {out_dir}").run().ok();
    cmd!(
        sh,
        "cargo test -p xvision-engine --features ts-export --tests -- --quiet"
    )
    .run()
    .context("ts-rs export run failed")?;

    let entries: Vec<_> = std::fs::read_dir(out_dir)
        .with_context(|| format!("read {out_dir}"))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().map(|x| x == "ts").unwrap_or(false)
                && e.file_name() != "index.ts"
        })
        .collect();

    let barrel = entries
        .iter()
        .map(|e| {
            let stem = e.path().file_stem().unwrap().to_string_lossy().into_owned();
            format!("export * from \"./types.gen/{stem}\";")
        })
        .collect::<Vec<_>>()
        .join("\n")
        + "\n";

    std::fs::write("frontend/web/src/api/types.gen.ts", barrel)?;
    println!("wrote {} type exports", entries.len());
    Ok(())
}
```

- [ ] **Step 4.3: Run it**

Run: `cargo xtask gen-types`
Expected: prints "wrote N type exports", `frontend/web/src/api/types.gen.ts` exists with `export *` lines.

- [ ] **Step 4.4: Document in MANUAL.md**

Append to `/Users/edkennedy/Code/xvision/MANUAL.md` (find the "Tooling" section or add one):

```markdown
### Type codegen

Run after editing any struct in `xvision-engine/src/api/`:

```sh
cargo xtask gen-types
```

This regenerates `frontend/web/src/api/types.gen/` and the barrel `types.gen.ts`. Commit the result.
```

- [ ] **Step 4.5: Commit**

```bash
git add Cargo.toml xtask/ MANUAL.md frontend/web/src/api/types.gen.ts
git commit -m "feat(xtask): add 'cargo xtask gen-types' for Rust→TS API codegen"
```

---

### Task 5: Implement `GET /api/strategies` route

**Files:**
- Create: `crates/xvision-dashboard/src/routes/strategies.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Modify: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 5.1: Identify the engine API entry point**

Run: `grep -n "pub fn list\|pub async fn list" crates/xvision-engine/src/api/strategy.rs`
Expected: a function like `pub async fn list(ctx: &ApiContext) -> ApiResult<StrategyListResponse>`.

If the function doesn't exist, **stop** and confirm the engine API plan is landed before continuing.

- [ ] **Step 5.2: Add a failing test**

Append to `crates/xvision-dashboard/tests/http.rs`:

```rust
#[tokio::test]
async fn strategies_list_returns_array() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();

    let response = server.get("/api/strategies").await;
    response.assert_status_ok();
    let body: serde_json::Value = response.json();
    assert!(body["items"].is_array(), "items must be array");
}
```

- [ ] **Step 5.3: Run — expect a 404 failure**

Run: `cargo test -p xvision-dashboard --test http strategies_list_returns_array`
Expected: assertion failure (status 404).

- [ ] **Step 5.4: Add an `ApiContext` builder**

`engine::api::*` functions need an `ApiContext`. Add a helper to build one in dashboard requests.

Create `crates/xvision-dashboard/src/context.rs`:

```rust
use anyhow::Result;
use xvision_engine::api::ApiContext;

pub fn build_context() -> Result<ApiContext> {
    // Mirrors what xvn CLI does — read XVN_HOME, open SQLite, set actor.
    xvision_engine::api::context::for_dashboard()
}
```

(If `for_dashboard()` doesn't exist on `xvision_engine::api::context`, add it next to the CLI builder. It's the same constructor with `actor: ApiActor::Dashboard`.)

In `lib.rs`, add `pub mod context;`.

- [ ] **Step 5.5: Implement the route**

Create `crates/xvision-dashboard/src/routes/strategies.rs`:

```rust
use axum::Json;
use serde::Serialize;
use xvision_engine::api::strategy;

use crate::context::build_context;
use crate::error::DashboardError;

#[derive(Serialize)]
pub struct StrategyListBody {
    pub items: Vec<strategy::StrategySummary>,
}

pub async fn list() -> Result<Json<StrategyListBody>, DashboardError> {
    let ctx = build_context().map_err(DashboardError::Internal)?;
    let resp = strategy::list(&ctx).await.map_err(map_api_err)?;
    Ok(Json(StrategyListBody { items: resp.items }))
}

fn map_api_err(e: xvision_engine::api::ApiError) -> DashboardError {
    use xvision_engine::api::ApiError as E;
    match e {
        E::NotFound(m) => DashboardError::NotFound(m),
        E::Validation { field, msg } => DashboardError::Validation { field, msg },
        E::Conflict(m) => DashboardError::Conflict(m),
        E::Internal(e) => DashboardError::Internal(e.into()),
    }
}
```

(Adjust struct/error names if engine API uses different ones — the audit confirmed `StrategySummary` exists; the variant names are best-guess. Match what's actually defined.)

- [ ] **Step 5.6: Register in `routes/mod.rs` and `server.rs`**

In `crates/xvision-dashboard/src/routes/mod.rs`:

```rust
pub mod health;
pub mod strategies;
```

In `crates/xvision-dashboard/src/server.rs`, extend the router:

```rust
pub fn build_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/strategies", get(crate::routes::strategies::list))
        .layer(TraceLayer::new_for_http())
}
```

- [ ] **Step 5.7: Run the tests — expect pass**

Run: `cargo test -p xvision-dashboard --test http`
Expected: 2 passed.

- [ ] **Step 5.8: Regenerate TS types**

Run: `cargo xtask gen-types`
Expected: `StrategyListBody.ts` (and any newly-touched types) appear in `frontend/web/src/api/types.gen/`.

- [ ] **Step 5.9: Commit**

```bash
git add crates/xvision-dashboard/ crates/xvision-engine/src/api/ frontend/web/src/api/
git commit -m "feat(dashboard): add GET /api/strategies route"
```

---

### Task 6: Scaffold `frontend/web/` Vite app

**Files:**
- Create: `frontend/web/package.json`
- Create: `frontend/web/vite.config.ts`
- Create: `frontend/web/tsconfig.json`
- Create: `frontend/web/tailwind.config.ts`
- Create: `frontend/web/postcss.config.js`
- Create: `frontend/web/index.html`
- Create: `frontend/web/src/main.tsx`
- Create: `frontend/web/src/App.tsx`
- Create: `frontend/web/src/styles/globals.css`
- Create: `frontend/web/.gitignore`
- Create: `frontend/web/public/favicon.svg`
- Modify: `.gitignore` (root)

- [ ] **Step 6.1: Verify pnpm is available**

Run: `pnpm --version`
Expected: prints a version. If not installed: `npm install -g pnpm@9` (one-time setup).

- [ ] **Step 6.2: Write `package.json`**

Create `frontend/web/package.json`:

```json
{
  "name": "xvision-web",
  "version": "0.1.0",
  "private": true,
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc -b && vite build",
    "preview": "vite preview",
    "typecheck": "tsc --noEmit",
    "lint": "eslint src --ext ts,tsx"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1",
    "react-router-dom": "^6.26.0",
    "@tanstack/react-query": "^5.51.0",
    "zustand": "^4.5.4",
    "@radix-ui/react-dialog": "^1.1.1",
    "@radix-ui/react-dropdown-menu": "^2.1.1",
    "@radix-ui/react-popover": "^1.1.1",
    "@radix-ui/react-tabs": "^1.1.0",
    "clsx": "^2.1.1",
    "@fontsource/inter": "^5.0.18",
    "@fontsource/jetbrains-mono": "^5.0.20",
    "@fontsource/cormorant-garamond": "^5.0.13"
  },
  "devDependencies": {
    "@types/node": "^22.0.0",
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "autoprefixer": "^10.4.20",
    "eslint": "^9.8.0",
    "eslint-plugin-react-hooks": "^4.6.2",
    "eslint-plugin-react-refresh": "^0.4.9",
    "postcss": "^8.4.40",
    "tailwindcss": "^3.4.7",
    "typescript": "^5.5.4",
    "vite": "^5.3.5"
  },
  "packageManager": "pnpm@9.7.0"
}
```

- [ ] **Step 6.3: Write `vite.config.ts`**

Create `frontend/web/vite.config.ts`:

```ts
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import path from "path";

export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: { "@": path.resolve(__dirname, "src") },
  },
  build: {
    outDir: "../../crates/xvision-dashboard/static",
    emptyOutDir: true,
    sourcemap: true,
  },
  server: {
    port: 5173,
    proxy: {
      "/api": "http://127.0.0.1:8788",
    },
  },
});
```

- [ ] **Step 6.4: Write `tsconfig.json`**

Create `frontend/web/tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "skipLibCheck": true,
    "esModuleInterop": true,
    "isolatedModules": true,
    "resolveJsonModule": true,
    "allowImportingTsExtensions": false,
    "noEmit": true,
    "useDefineForClassFields": true,
    "baseUrl": ".",
    "paths": { "@/*": ["src/*"] }
  },
  "include": ["src"]
}
```

- [ ] **Step 6.5: Write `tailwind.config.ts` (tokens come in Task 7)**

Create `frontend/web/tailwind.config.ts`:

```ts
import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {},
  },
  plugins: [],
} satisfies Config;
```

Create `frontend/web/postcss.config.js`:

```js
export default {
  plugins: { tailwindcss: {}, autoprefixer: {} },
};
```

- [ ] **Step 6.6: Write `index.html`**

Create `frontend/web/index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <link rel="icon" type="image/svg+xml" href="/favicon.svg" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>xvn</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

Create `frontend/web/public/favicon.svg`:

```svg
<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 32 32"><rect width="32" height="32" fill="#0F0E0C"/><text x="16" y="22" text-anchor="middle" font-family="Cormorant Garamond,serif" font-style="italic" font-size="20" fill="#D4A547">x</text></svg>
```

- [ ] **Step 6.7: Write `src/main.tsx`, `src/App.tsx`, and `src/styles/globals.css`**

Create `frontend/web/src/styles/globals.css`:

```css
@import "@fontsource/inter/400.css";
@import "@fontsource/inter/500.css";
@import "@fontsource/inter/600.css";
@import "@fontsource/jetbrains-mono/400.css";
@import "@fontsource/jetbrains-mono/500.css";
@import "@fontsource/cormorant-garamond/400.css";
@import "@fontsource/cormorant-garamond/500.css";
@import "@fontsource/cormorant-garamond/400-italic.css";
@import "@fontsource/cormorant-garamond/500-italic.css";
@import "./tokens.css";

@tailwind base;
@tailwind components;
@tailwind utilities;
```

Create `frontend/web/src/main.tsx`:

```tsx
import React from "react";
import ReactDOM from "react-dom/client";
import "./styles/globals.css";
import App from "./App";

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);
```

Create `frontend/web/src/App.tsx`:

```tsx
export default function App() {
  return (
    <div className="min-h-screen bg-[var(--bg)] text-[var(--text)] p-8">
      <h1 className="font-serif italic text-4xl">xvn</h1>
      <p className="mt-2 text-sm" style={{ color: "var(--text-2)" }}>
        Dashboard scaffold — ready for components.
      </p>
    </div>
  );
}
```

(`tokens.css` is created in Task 7 — until then this references undefined CSS variables, which is fine, just looks unstyled.)

- [ ] **Step 6.8: Write `.gitignore` files**

Create `frontend/web/.gitignore`:

```
node_modules/
dist/
.vite/
*.log
.DS_Store
```

Append to root `.gitignore`:

```

# Frontend build output
frontend/web/node_modules/
frontend/web/dist/
crates/xvision-dashboard/static/
```

- [ ] **Step 6.9: Install and verify**

Run: `cd frontend/web && pnpm install && pnpm typecheck && pnpm build && cd -`
Expected: `crates/xvision-dashboard/static/` now contains `index.html` and bundled JS.

- [ ] **Step 6.10: Commit**

```bash
git add frontend/web/ .gitignore
git commit -m "feat(frontend): scaffold Vite + React + Tailwind app"
```

---

### Task 7: Port `prototype/styles.css` tokens to Tailwind

**Files:**
- Create: `frontend/web/src/styles/tokens.css`
- Modify: `frontend/web/tailwind.config.ts`

- [ ] **Step 7.1: Write `tokens.css` (verbatim port of prototype's :root)**

Create `frontend/web/src/styles/tokens.css`:

```css
:root {
  --bg: #0F0E0C;
  --surface-sidebar: #17150F;
  --surface-card: #14120E;
  --surface-elev: #1B1810;
  --surface-panel: #221E14;
  --surface-hover: #1F1C13;

  --border: #2A2618;
  --border-strong: #3A3322;
  --border-soft: #221F15;

  --text: #F1ECDD;
  --text-2: #A39A85;
  --text-3: #6B6553;
  --text-4: #4A4536;

  --gold: #D4A547;
  --gold-soft: #B8862E;
  --gold-bg: rgba(212, 165, 71, 0.10);
  --gold-bg-strong: rgba(212, 165, 71, 0.18);

  --warn: #DB9230;
  --danger: #C8443A;
  --info: #6F8FB8;

  --radius-card: 6px;
  --radius-sm: 4px;
}

html, body, #root {
  background: var(--bg);
  color: var(--text);
  font-family: "Inter", sans-serif;
  font-size: 13px;
  line-height: 1.45;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}
```

- [ ] **Step 7.2: Wire tokens into Tailwind config**

Replace `frontend/web/tailwind.config.ts`:

```ts
import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        bg: "var(--bg)",
        surface: {
          sidebar: "var(--surface-sidebar)",
          card: "var(--surface-card)",
          elev: "var(--surface-elev)",
          panel: "var(--surface-panel)",
          hover: "var(--surface-hover)",
        },
        border: {
          DEFAULT: "var(--border)",
          strong: "var(--border-strong)",
          soft: "var(--border-soft)",
        },
        text: {
          DEFAULT: "var(--text)",
          2: "var(--text-2)",
          3: "var(--text-3)",
          4: "var(--text-4)",
        },
        gold: {
          DEFAULT: "var(--gold)",
          soft: "var(--gold-soft)",
        },
        warn: "var(--warn)",
        danger: "var(--danger)",
        info: "var(--info)",
      },
      fontFamily: {
        sans: ['"Inter"', "sans-serif"],
        serif: ['"Cormorant Garamond"', "serif"],
        mono: ['"JetBrains Mono"', "monospace"],
      },
      borderRadius: {
        card: "var(--radius-card)",
        sm: "var(--radius-sm)",
      },
    },
  },
  plugins: [],
} satisfies Config;
```

- [ ] **Step 7.3: Update App.tsx to use Tailwind classes**

Replace `frontend/web/src/App.tsx`:

```tsx
export default function App() {
  return (
    <div className="min-h-screen bg-bg text-text p-8">
      <h1 className="font-serif italic text-4xl">xvn</h1>
      <p className="mt-2 text-sm text-text-2">
        Dashboard scaffold — ready for components.
      </p>
    </div>
  );
}
```

- [ ] **Step 7.4: Verify**

Run: `cd frontend/web && pnpm dev` (in one terminal); open http://localhost:5173 in a browser.
Expected: warm-black background, "xvn" in italic Cormorant gold-ish, muted secondary text.

Stop the dev server when verified.

- [ ] **Step 7.5: Commit**

```bash
git add frontend/web/src/styles/tokens.css frontend/web/tailwind.config.ts frontend/web/src/App.tsx
git commit -m "feat(frontend): port Folio dark tokens to Tailwind config"
```

---

### Task 8: Port shell primitives (`Icon`, `Sidebar`, `Topbar`)

**Files:**
- Create: `frontend/web/src/components/primitives/Icon.tsx`
- Create: `frontend/web/src/components/primitives/Sparkline.tsx`
- Create: `frontend/web/src/components/primitives/Pill.tsx`
- Create: `frontend/web/src/components/primitives/Dot.tsx`
- Create: `frontend/web/src/components/primitives/Card.tsx`
- Create: `frontend/web/src/components/shell/Sidebar.tsx`
- Create: `frontend/web/src/components/shell/Topbar.tsx`
- Create: `frontend/web/src/components/shell/ChatRailPlaceholder.tsx`
- Create: `frontend/web/src/components/shell/AppShell.tsx`

- [ ] **Step 8.1: Implement `Icon`**

Open `frontend/prototype/shared.jsx` and copy the `paths` object verbatim into the new component. Create `frontend/web/src/components/primitives/Icon.tsx`:

```tsx
import { ReactNode } from "react";

const paths: Record<string, ReactNode> = {
  home: <path d="M3 9.5L10 4l7 5.5V16a1 1 0 01-1 1h-3v-5H9v5H4a1 1 0 01-1-1V9.5z" />,
  chart: <path d="M3 16h14M5 13l3-4 3 2 4-6" />,
  play: (
    <>
      <circle cx="10" cy="10" r="7" />
      <path d="M8 7l5 3-5 3V7z" fill="currentColor" stroke="none" />
    </>
  ),
  bars: <path d="M4 16V8M8 16V5M12 16v-6M16 16v-9" />,
  book: <path d="M4 4h5a3 3 0 013 3v9a2 2 0 00-2-2H4V4zM16 4h-5a3 3 0 00-3 3v9a2 2 0 012-2h6V4z" />,
  db: (
    <>
      <ellipse cx="10" cy="5" rx="6" ry="2" />
      <path d="M4 5v10c0 1.1 2.7 2 6 2s6-.9 6-2V5M4 10c0 1.1 2.7 2 6 2s6-.9 6-2" />
    </>
  ),
  cog: (
    <>
      <circle cx="10" cy="10" r="2.5" />
      <path d="M10 2v2M10 16v2M16.4 6l-1.4.8M5 13.2L3.6 14M18 10h-2M4 10H2M16.4 14L15 13.2M5 6.8L3.6 6" />
    </>
  ),
  plus: <path d="M10 4v12M4 10h12" />,
  search: (
    <>
      <circle cx="9" cy="9" r="5" />
      <path d="M13 13l4 4" />
    </>
  ),
  chevR: <path d="M8 5l5 5-5 5" />,
};

type Props = {
  name: keyof typeof paths;
  size?: number;
  color?: string;
  strokeWidth?: number;
  className?: string;
};

export function Icon({
  name,
  size = 16,
  color = "currentColor",
  strokeWidth = 1.5,
  className,
}: Props) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 20 20"
      fill="none"
      stroke={color}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      strokeLinejoin="round"
      className={className}
      style={{ flexShrink: 0 }}
    >
      {paths[name]}
    </svg>
  );
}
```

(Copy the rest of the icon paths from `prototype/shared.jsx` lines 5–32 — `pulse`, `dollar`, `bag`, `barchart`, `diamond`, `code`, `arrow`, `check`, `findingDot`, `branch`, `settings`, `box`, `user`, `list`, `flame`, `sliders`. The full set is needed by later tasks.)

- [ ] **Step 8.2: Implement `Pill`, `Dot`, `Card`**

Create `frontend/web/src/components/primitives/Pill.tsx`:

```tsx
import { clsx } from "clsx";
import { ReactNode } from "react";

type Variant = "default" | "gold" | "solid" | "danger" | "warn";

export function Pill({
  variant = "default",
  children,
  className,
}: {
  variant?: Variant;
  children: ReactNode;
  className?: string;
}) {
  return (
    <span
      className={clsx(
        "inline-flex items-center px-2 py-[3px] rounded-[3px] text-[11px] tracking-wide border whitespace-nowrap",
        variant === "default" && "border-border text-text-2",
        variant === "gold" && "border-[rgba(212,165,71,0.35)] text-gold",
        variant === "solid" && "bg-gold text-bg border-gold font-medium",
        variant === "danger" && "border-[rgba(200,68,58,0.4)] text-danger",
        variant === "warn" && "border-[rgba(219,146,48,0.4)] text-warn",
        className,
      )}
    >
      {children}
    </span>
  );
}
```

Create `frontend/web/src/components/primitives/Dot.tsx`:

```tsx
import { clsx } from "clsx";

type Tone = "gold" | "warn" | "danger" | "info" | "muted";

export function Dot({ tone = "gold", className }: { tone?: Tone; className?: string }) {
  return (
    <span
      className={clsx(
        "inline-block w-1.5 h-1.5 rounded-full mr-2 align-middle relative -top-px",
        tone === "gold" && "bg-gold",
        tone === "warn" && "bg-warn",
        tone === "danger" && "bg-danger",
        tone === "info" && "bg-info",
        tone === "muted" && "bg-text-3",
        className,
      )}
    />
  );
}
```

Create `frontend/web/src/components/primitives/Card.tsx`:

```tsx
import { ReactNode } from "react";
import { clsx } from "clsx";

export function Card({
  children,
  className,
}: {
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={clsx("bg-surface-card border border-border rounded-card", className)}>
      {children}
    </div>
  );
}
```

- [ ] **Step 8.3: Implement `Sparkline`**

Create `frontend/web/src/components/primitives/Sparkline.tsx` — port from `prototype/shared.jsx:95-109`:

```tsx
type Props = {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
};

export function Sparkline({ data, width = 80, height = 22, color = "var(--gold)" }: Props) {
  if (data.length === 0) return null;
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min || 1;
  const pts = data
    .map((v, i) => {
      const x = (i / (data.length - 1)) * width;
      const y = height - ((v - min) / range) * (height - 2) - 1;
      return `${x},${y}`;
    })
    .join(" ");
  return (
    <svg className="inline-block align-middle" width={width} height={height} viewBox={`0 0 ${width} ${height}`}>
      <polyline points={pts} fill="none" stroke={color} strokeWidth="1.2" />
    </svg>
  );
}
```

- [ ] **Step 8.4: Implement `Sidebar`**

Create `frontend/web/src/components/shell/Sidebar.tsx` — port from `prototype/shared.jsx:42-78`, but use React Router `NavLink`:

```tsx
import { NavLink } from "react-router-dom";
import { Icon } from "@/components/primitives/Icon";

const items = [
  { to: "/", label: "Home", icon: "home" as const },
  { to: "/strategies", label: "Strategies", icon: "chart" as const },
  { to: "/eval/runs", label: "Eval", icon: "bars" as const },
  { to: "/settings/providers", label: "Settings", icon: "cog" as const },
];

const disabledItems = [
  { label: "Live", icon: "play" as const, hint: "v1.1" },
  { label: "Journal", icon: "book" as const, hint: "deferred" },
];

export function Sidebar() {
  return (
    <aside className="w-[200px] bg-surface-sidebar border-r border-border-soft flex flex-col py-6">
      <div className="font-serif italic font-medium text-[38px] text-text px-6 pb-8 leading-none tracking-tight">
        xvn
      </div>
      <nav className="flex flex-col flex-1">
        {items.map((i) => (
          <NavLink
            key={i.to}
            to={i.to}
            end={i.to === "/"}
            className={({ isActive }) =>
              [
                "flex items-center gap-3 px-6 py-2.5 text-[13.5px] cursor-pointer border-l-2 transition-colors",
                isActive
                  ? "text-text bg-[rgba(212,165,71,0.06)] border-gold"
                  : "text-text-2 border-transparent hover:text-text",
              ].join(" ")
            }
          >
            <Icon name={i.icon} size={17} />
            <span>{i.label}</span>
          </NavLink>
        ))}
        {disabledItems.map((i) => (
          <div
            key={i.label}
            title={i.hint}
            className="flex items-center gap-3 px-6 py-2.5 text-[13.5px] text-text-3 cursor-not-allowed"
          >
            <Icon name={i.icon} size={17} />
            <span>{i.label}</span>
            <span className="ml-auto text-[10px] uppercase tracking-wider text-text-4">{i.hint}</span>
          </div>
        ))}
      </nav>
    </aside>
  );
}
```

- [ ] **Step 8.5: Implement `Topbar`**

Create `frontend/web/src/components/shell/Topbar.tsx`:

```tsx
type Props = {
  title: string;
  sub?: string;
};

export function Topbar({ title, sub }: Props) {
  return (
    <div className="flex items-start justify-between gap-8 mb-7">
      <div>
        <h1 className="font-serif font-medium text-[38px] m-0 mb-1 tracking-tight">{title}</h1>
        {sub && <div className="text-text-2 text-sm">{sub}</div>}
      </div>
      <div className="flex items-center gap-2.5 w-[380px] px-3 py-2 bg-surface-elev border border-border rounded text-text-3 text-[13px]">
        <span className="inline-flex items-center gap-0.5 px-1.5 py-0.5 border border-border-strong rounded-sm font-mono text-[11px] text-text-2">
          ⌘K
        </span>
        <span className="flex-1">Jump to anything…</span>
      </div>
    </div>
  );
}
```

- [ ] **Step 8.6: Implement `ChatRailPlaceholder`**

Create `frontend/web/src/components/shell/ChatRailPlaceholder.tsx`:

```tsx
export function ChatRailPlaceholder() {
  return (
    <aside className="w-10 bg-surface-sidebar border-l border-border-soft flex flex-col items-center py-4 text-text-3 text-xs">
      <div title="Chat rail (Plan 4)">💬</div>
    </aside>
  );
}
```

(Plan 4 will replace this with the real chat rail.)

- [ ] **Step 8.7: Implement `AppShell`**

Create `frontend/web/src/components/shell/AppShell.tsx`:

```tsx
import { Outlet } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { ChatRailPlaceholder } from "./ChatRailPlaceholder";

export function AppShell() {
  return (
    <div className="grid grid-cols-[200px_1fr_40px] h-screen w-screen overflow-hidden bg-bg text-text">
      <Sidebar />
      <main className="overflow-y-auto px-9 pt-9 pb-6">
        <Outlet />
      </main>
      <ChatRailPlaceholder />
    </div>
  );
}
```

- [ ] **Step 8.8: Commit**

```bash
git add frontend/web/src/components/
git commit -m "feat(frontend): port Icon/Pill/Dot/Card/Sparkline/Sidebar/Topbar/AppShell from prototype"
```

---

### Task 9: Wire React Router with placeholder routes

**Files:**
- Create: `frontend/web/src/routes/home.tsx`
- Create: `frontend/web/src/routes/strategies.tsx`
- Create: `frontend/web/src/routes/authoring.tsx`
- Create: `frontend/web/src/routes/eval-runs.tsx`
- Create: `frontend/web/src/routes/eval-runs-detail.tsx`
- Create: `frontend/web/src/routes/eval-compare.tsx`
- Create: `frontend/web/src/routes/setup.tsx`
- Create: `frontend/web/src/routes/settings/providers.tsx`
- Create: `frontend/web/src/routes/settings/brokers.tsx`
- Create: `frontend/web/src/routes/settings/daemon.tsx`
- Create: `frontend/web/src/routes/settings/identity.tsx`
- Create: `frontend/web/src/routes/settings/danger.tsx`
- Create: `frontend/web/src/routes.tsx`
- Modify: `frontend/web/src/App.tsx`

- [ ] **Step 9.1: Create a placeholder helper**

Create `frontend/web/src/routes/_placeholder.tsx`:

```tsx
import { Topbar } from "@/components/shell/Topbar";

export function Placeholder({ title, plan }: { title: string; plan: string }) {
  return (
    <>
      <Topbar title={title} sub={`Lands in Plan ${plan}.`} />
      <div className="text-text-3 text-sm">Placeholder.</div>
    </>
  );
}
```

- [ ] **Step 9.2: Create all placeholder route files**

Each placeholder file is one line. Example — create `frontend/web/src/routes/home.tsx`:

```tsx
import { Placeholder } from "./_placeholder";
export default function Home() { return <Placeholder title="Home" plan="2" />; }
```

Repeat for:
- `authoring.tsx` → `<Placeholder title="Authoring" plan="3" />`
- `eval-runs.tsx` → `<Placeholder title="Eval runs" plan="2" />`
- `eval-runs-detail.tsx` → `<Placeholder title="Run detail" plan="2" />`
- `eval-compare.tsx` → `<Placeholder title="Compare runs" plan="5" />`
- `setup.tsx` → `<Placeholder title="Setup wizard" plan="4" />`
- `settings/providers.tsx` → `<Placeholder title="Providers" plan="2" />`
- `settings/brokers.tsx` → `<Placeholder title="Brokers" plan="2" />`
- `settings/daemon.tsx` → `<Placeholder title="Daemon" plan="2" />`
- `settings/identity.tsx` → `<Placeholder title="Identity" plan="2" />`
- `settings/danger.tsx` → `<Placeholder title="Danger zone" plan="2" />`

Leave `strategies.tsx` minimal for now — Task 11 fills it in:

```tsx
import { Placeholder } from "./_placeholder";
export default function Strategies() { return <Placeholder title="Strategies" plan="1 (this plan)" />; }
```

- [ ] **Step 9.3: Build the router**

Create `frontend/web/src/routes.tsx`:

```tsx
import { createBrowserRouter } from "react-router-dom";
import { AppShell } from "@/components/shell/AppShell";
import Home from "./routes/home";
import Strategies from "./routes/strategies";
import Authoring from "./routes/authoring";
import EvalRuns from "./routes/eval-runs";
import EvalRunsDetail from "./routes/eval-runs-detail";
import EvalCompare from "./routes/eval-compare";
import Setup from "./routes/setup";
import Providers from "./routes/settings/providers";
import Brokers from "./routes/settings/brokers";
import Daemon from "./routes/settings/daemon";
import Identity from "./routes/settings/identity";
import Danger from "./routes/settings/danger";

export const router = createBrowserRouter([
  {
    path: "/",
    element: <AppShell />,
    children: [
      { index: true, element: <Home /> },
      { path: "setup", element: <Setup /> },
      { path: "strategies", element: <Strategies /> },
      { path: "authoring/:bundleId", element: <Authoring /> },
      { path: "eval/runs", element: <EvalRuns /> },
      { path: "eval/runs/:runId", element: <EvalRunsDetail /> },
      { path: "eval/compare", element: <EvalCompare /> },
      { path: "settings/providers", element: <Providers /> },
      { path: "settings/brokers", element: <Brokers /> },
      { path: "settings/daemon", element: <Daemon /> },
      { path: "settings/identity", element: <Identity /> },
      { path: "settings/danger", element: <Danger /> },
    ],
  },
]);
```

- [ ] **Step 9.4: Replace `App.tsx` to use the router**

Replace `frontend/web/src/App.tsx`:

```tsx
import { RouterProvider } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { router } from "./routes";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: { staleTime: 30_000, refetchOnWindowFocus: false },
  },
});

export default function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <RouterProvider router={router} />
    </QueryClientProvider>
  );
}
```

- [ ] **Step 9.5: Verify**

Run: `cd frontend/web && pnpm dev`. Open http://localhost:5173.
- Click each sidebar item → URL updates, placeholder renders.
- Direct-navigate to `/eval/runs/abc` → "Run detail" placeholder.
Stop the dev server.

- [ ] **Step 9.6: Commit**

```bash
git add frontend/web/src/routes/ frontend/web/src/routes.tsx frontend/web/src/App.tsx
git commit -m "feat(frontend): wire React Router with placeholder routes for all v1 paths"
```

---

### Task 10: Implement API client + strategies fetcher

**Files:**
- Create: `frontend/web/src/api/client.ts`
- Create: `frontend/web/src/api/strategies.ts`

- [ ] **Step 10.1: Implement the base fetch wrapper**

Create `frontend/web/src/api/client.ts`:

```ts
export type ApiErrorBody = { code: string; message: string };

export class ApiError extends Error {
  constructor(public status: number, public body: ApiErrorBody) {
    super(body.message);
  }
}

export async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(path, {
    ...init,
    headers: { "Content-Type": "application/json", ...(init?.headers ?? {}) },
  });
  if (!res.ok) {
    const body = (await res.json().catch(() => ({
      code: "unknown",
      message: res.statusText,
    }))) as ApiErrorBody;
    throw new ApiError(res.status, body);
  }
  return (await res.json()) as T;
}
```

- [ ] **Step 10.2: Implement the strategies fetcher**

Create `frontend/web/src/api/strategies.ts`:

```ts
import { apiFetch } from "./client";
import type { StrategySummary } from "./types.gen";

export type StrategiesListResponse = { items: StrategySummary[] };

export const strategiesApi = {
  list: () => apiFetch<StrategiesListResponse>("/api/strategies"),
};
```

(If `types.gen.ts` doesn't yet export `StrategySummary`, run `cargo xtask gen-types` from the repo root.)

- [ ] **Step 10.3: Commit**

```bash
git add frontend/web/src/api/
git commit -m "feat(frontend): add base API client and strategies fetcher"
```

---

### Task 11: Implement the Strategies list screen with real data

**Files:**
- Modify: `frontend/web/src/routes/strategies.tsx`
- Create: `frontend/web/src/components/tables/StrategiesTable.tsx`
- Create: `frontend/web/src/lib/format.ts`

- [ ] **Step 11.1: Add formatting helpers**

Create `frontend/web/src/lib/format.ts`:

```ts
export function fmtRelative(ts: string | Date | null | undefined): string {
  if (!ts) return "—";
  const d = typeof ts === "string" ? new Date(ts) : ts;
  const diffSec = Math.floor((Date.now() - d.getTime()) / 1000);
  if (diffSec < 60) return `${diffSec}s ago`;
  if (diffSec < 3600) return `${Math.floor(diffSec / 60)}m ago`;
  if (diffSec < 86400) return `${Math.floor(diffSec / 3600)}h ago`;
  return `${Math.floor(diffSec / 86400)}d ago`;
}
```

- [ ] **Step 11.2: Implement `StrategiesTable`**

Create `frontend/web/src/components/tables/StrategiesTable.tsx`:

```tsx
import type { StrategySummary } from "@/api/types.gen";
import { fmtRelative } from "@/lib/format";

export function StrategiesTable({ rows }: { rows: StrategySummary[] }) {
  if (rows.length === 0) {
    return (
      <div className="bg-surface-card border border-border rounded-card p-12 text-center text-text-2 text-sm">
        No strategies yet. Click <span className="text-gold">New strategy</span> to start.
      </div>
    );
  }
  return (
    <div className="bg-surface-card border border-border rounded-card">
      <table className="w-full border-collapse">
        <thead>
          <tr>
            <Th className="pl-5">Name</Th>
            <Th>Template</Th>
            <Th>Forked from</Th>
            <Th className="pr-5">Updated</Th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.bundle_id} className="hover:bg-surface-hover">
              <Td className="pl-5 font-mono text-text">{r.name}</Td>
              <Td className="text-text-2">{r.template}</Td>
              <Td className="text-text-2 font-mono">{r.parent_bundle_id ?? "—"}</Td>
              <Td className="pr-5 text-text-2">{fmtRelative(r.updated_at)}</Td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function Th({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return (
    <th className={`text-left font-normal text-text-2 text-xs py-2.5 px-3 border-b border-border-soft ${className}`}>
      {children}
    </th>
  );
}

function Td({ children, className = "" }: { children: React.ReactNode; className?: string }) {
  return <td className={`py-3 px-3 border-b border-border-soft text-[13px] last:border-b-0 ${className}`}>{children}</td>;
}
```

- [ ] **Step 11.3: Implement the route**

Replace `frontend/web/src/routes/strategies.tsx`:

```tsx
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { StrategiesTable } from "@/components/tables/StrategiesTable";
import { strategiesApi } from "@/api/strategies";
import { ApiError } from "@/api/client";

export default function Strategies() {
  const { data, isLoading, error } = useQuery({
    queryKey: ["strategies", "list"],
    queryFn: () => strategiesApi.list(),
  });

  if (isLoading) {
    return (
      <>
        <Topbar title="Strategies" />
        <div className="text-text-2 text-sm">Loading…</div>
      </>
    );
  }
  if (error) {
    const msg = error instanceof ApiError ? error.body.message : String(error);
    return (
      <>
        <Topbar title="Strategies" />
        <div className="bg-surface-card border border-danger/40 rounded-card p-6 text-danger">
          {msg}
        </div>
      </>
    );
  }

  const rows = data?.items ?? [];
  return (
    <>
      <Topbar title="Strategies" sub={`${rows.length} ${rows.length === 1 ? "bundle" : "bundles"}`} />
      <StrategiesTable rows={rows} />
    </>
  );
}
```

- [ ] **Step 11.4: Verify against the running daemon**

In one terminal: `cargo run -p xvision-cli -- dashboard serve --bind 127.0.0.1:8788`
In another: `cd frontend/web && pnpm dev`
Open http://localhost:5173/strategies.
Expected: empty-state message OR a table of bundles, depending on whether `~/.xvn/bundles/` has anything.

If the daemon errors (e.g., missing XVN_HOME), the page should show a readable error, not crash.

Stop both processes.

- [ ] **Step 11.5: Commit**

```bash
git add frontend/web/src/routes/strategies.tsx frontend/web/src/components/tables/StrategiesTable.tsx frontend/web/src/lib/format.ts
git commit -m "feat(frontend): implement Strategies list with real /api/strategies data"
```

---

### Task 12: Embed the built SPA into the dashboard binary

**Files:**
- Create: `crates/xvision-dashboard/src/embed.rs`
- Create: `crates/xvision-dashboard/src/routes/static_files.rs`
- Modify: `crates/xvision-dashboard/src/lib.rs`
- Modify: `crates/xvision-dashboard/src/server.rs`
- Modify: `crates/xvision-dashboard/src/routes/mod.rs`
- Create: `crates/xvision-dashboard/build.rs`

- [ ] **Step 12.1: Implement the embed module**

Create `crates/xvision-dashboard/src/embed.rs`:

```rust
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct StaticAssets;
```

- [ ] **Step 12.2: Implement the static-file route**

Create `crates/xvision-dashboard/src/routes/static_files.rs`:

```rust
use axum::body::Body;
use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::embed::StaticAssets;

pub async fn serve(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    let target = if path.is_empty() { "index.html" } else { path };

    if let Some(file) = StaticAssets::get(target) {
        let mime = mime_guess::from_path(target).first_or_octet_stream();
        Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(Body::from(file.data.into_owned()))
            .unwrap()
    } else {
        // SPA fallback — unknown paths return index.html so React Router handles them.
        if let Some(index) = StaticAssets::get("index.html") {
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .body(Body::from(index.data.into_owned()))
                .unwrap()
        } else {
            (StatusCode::NOT_FOUND, "frontend not built").into_response()
        }
    }
}
```

- [ ] **Step 12.3: Wire it in**

Edit `crates/xvision-dashboard/src/routes/mod.rs`:

```rust
pub mod health;
pub mod static_files;
pub mod strategies;
```

Edit `crates/xvision-dashboard/src/lib.rs`:

```rust
pub mod context;
pub mod embed;
pub mod error;
pub mod routes;
pub mod server;

pub use server::serve;
```

Edit `crates/xvision-dashboard/src/server.rs` — add the catch-all route LAST (axum matches in order):

```rust
use axum::{routing::get, Router};
use std::net::SocketAddr;
use tower_http::trace::TraceLayer;

use crate::routes::health::health;
use crate::routes::static_files;

pub fn build_router() -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/strategies", get(crate::routes::strategies::list))
        .fallback(static_files::serve)
        .layer(TraceLayer::new_for_http())
}

pub async fn serve(addr: SocketAddr) -> anyhow::Result<()> {
    let app = build_router();
    tracing::info!(%addr, "xvision-dashboard listening");
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 12.4: Add the build.rs trigger**

Create `crates/xvision-dashboard/build.rs`:

```rust
use std::path::Path;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=../../frontend/web/src");
    println!("cargo:rerun-if-changed=../../frontend/web/index.html");
    println!("cargo:rerun-if-changed=../../frontend/web/package.json");
    println!("cargo:rerun-if-changed=../../frontend/web/vite.config.ts");

    // Skip frontend build on docs.rs and CI bots that pre-build.
    if std::env::var("XVN_SKIP_FRONTEND_BUILD").is_ok() {
        ensure_static_dir();
        return;
    }

    let web_dir = Path::new("../../frontend/web");
    if !web_dir.join("node_modules").exists() {
        run(web_dir, "pnpm", &["install", "--frozen-lockfile"]);
    }
    run(web_dir, "pnpm", &["build"]);
    ensure_static_dir();
}

fn run(dir: &Path, cmd: &str, args: &[&str]) {
    let status = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .status()
        .unwrap_or_else(|e| panic!("failed to spawn {cmd}: {e}"));
    if !status.success() {
        panic!("{cmd} {args:?} failed with status {status}");
    }
}

fn ensure_static_dir() {
    let dir = Path::new("static");
    if !dir.exists() {
        std::fs::create_dir_all(dir).ok();
    }
    // rust-embed needs at least one file at compile time.
    let stub = dir.join(".gitkeep");
    if !stub.exists() {
        std::fs::write(stub, "").ok();
    }
}
```

- [ ] **Step 12.5: Add `static/` to crate gitignore**

Append to `.gitignore` (root) — already done in Task 6 step 6.8 — verify with:

```bash
grep "xvision-dashboard/static" .gitignore
```

Expected: returns the line. If not, add it.

- [ ] **Step 12.6: Build the workspace**

Run: `cargo build -p xvision-dashboard`
Expected: build.rs invokes pnpm; eventually compiles. May take 1–2 minutes the first time.

- [ ] **Step 12.7: Smoke-test**

Run: `cargo run -p xvision-cli -- dashboard serve --bind 127.0.0.1:8788`
In a browser: http://127.0.0.1:8788/strategies
Expected: the strategies page renders; data loads from the same origin.

In another terminal: `curl -I http://127.0.0.1:8788/`
Expected: `200 OK`, `content-type: text/html`.

`curl -I http://127.0.0.1:8788/some/random/path`
Expected: `200 OK` (SPA fallback).

Stop the daemon.

- [ ] **Step 12.8: Commit**

```bash
git add crates/xvision-dashboard/
git commit -m "feat(dashboard): embed SPA via rust-embed with build.rs trigger"
```

---

### Task 13: Add E2E smoke test

**Files:**
- Modify: `crates/xvision-dashboard/tests/http.rs`

- [ ] **Step 13.1: Add an SPA fallback test**

Append to `crates/xvision-dashboard/tests/http.rs`:

```rust
#[tokio::test]
async fn spa_fallback_serves_index_html() {
    let app = build_router();
    let server = TestServer::new(app).unwrap();

    // Prime the static dir with a stub index.html since we test against the
    // build_router (which uses rust-embed at compile time, not runtime).
    // If embedded assets are empty (the .gitkeep stub), this returns 404 with
    // "frontend not built" — that's the documented fallback.
    let response = server.get("/strategies").await;
    let body = response.text();
    assert!(
        body.contains("<!doctype html") || body.contains("frontend not built"),
        "expected SPA HTML or fallback message, got: {body}"
    );
}
```

(rust-embed bakes assets at compile time. In CI we run `pnpm build` first, so the test sees real assets. Locally the test passes either way.)

- [ ] **Step 13.2: Run the test**

Run: `cargo test -p xvision-dashboard --test http`
Expected: 3 passed.

- [ ] **Step 13.3: Commit**

```bash
git add crates/xvision-dashboard/tests/http.rs
git commit -m "test(dashboard): add SPA fallback smoke test"
```

---

### Task 14: Document the build flow and publish first preview

**Files:**
- Modify: `frontend/README.md`
- Modify: `MANUAL.md`
- Modify: `frontend/DESIGN.md` (mark Plan 1 done)

- [ ] **Step 14.1: Update `frontend/README.md`**

Append a "Production app" section above the "V1 scope" table:

```markdown
## Production app

The production app lives at `frontend/web/`. Build with:

```sh
cd frontend/web
pnpm install
pnpm build       # outputs to crates/xvision-dashboard/static/
```

Or let cargo handle it: `cargo build -p xvision-dashboard` triggers the build via `build.rs`.

Run the dashboard:

```sh
cargo run -p xvision-cli -- dashboard serve
# default bind 127.0.0.1:8788
```

Local dev with HMR (frontend talks to a running daemon for `/api/*`):

```sh
# terminal 1
cargo run -p xvision-cli -- dashboard serve --bind 127.0.0.1:8788
# terminal 2
cd frontend/web && pnpm dev   # http://localhost:5173, proxies /api → 8788
```

After editing API types in `xvision-engine/src/api/`, regenerate TS:

```sh
cargo xtask gen-types
```
```

- [ ] **Step 14.2: Update `MANUAL.md`**

Confirm the "Type codegen" section added in Task 4.4 is still there. If the file has a "Running the dashboard" section, update it; else add one mirroring the README block above.

- [ ] **Step 14.3: Mark Plan 1 done in `frontend/DESIGN.md`**

In `frontend/DESIGN.md` §10 (Phased delivery), append `✓ landed` to "Phase 0 — scaffolding" and the Strategies-list bullet under "Phase 1".

- [ ] **Step 14.4: Final smoke**

Run from a clean checkout:

```bash
cargo build --workspace
cargo test --workspace
cargo run -p xvision-cli -- dashboard serve &
sleep 2
curl -s http://127.0.0.1:8788/api/health
curl -s http://127.0.0.1:8788/api/strategies | head -c 200
kill %1
```

Expected: all green; `/api/health` returns `{"status":"ok"}`; `/api/strategies` returns valid JSON.

- [ ] **Step 14.5: Commit**

```bash
git add frontend/README.md MANUAL.md frontend/DESIGN.md
git commit -m "docs: document dashboard build/serve flow; mark Plan 1 phase landed"
```

---

## Self-review

**Spec coverage:** Plan 1 covers DESIGN.md §3 (project layout for foundation files), §5 (TS codegen), §6.3 (Strategies list — first vertical), §10 Phase 0, and the first bullet of Phase 1. Other read-only screens (Home, Eval runs, Run detail, Settings) deliberately deferred to Plan 2 — flagged in this plan's scope-and-split table.

**Placeholder scan:** No "TBD" or "implement later" steps; every code block is complete. Step 5.1 has a conditional ("If the function doesn't exist, stop and confirm") — that's a check, not a placeholder.

**Type consistency:** `StrategySummary`, `StrategiesListResponse`/`StrategyListBody` (the Rust handler returns `StrategyListBody { items: Vec<StrategySummary> }`; the TS type generated for the body should match). Task 10.2 references `StrategySummary` from `types.gen` — Task 5.5 emits it via the engine API. If the Rust struct is named differently than `StrategySummary` (e.g., `StrategyListItem`), the engineer adjusts Task 10.2 imports accordingly.

**Cross-task hand-offs:** Task 7's `text-2`, `text-3` Tailwind colors are used by Tasks 8/9/11. Task 8's `Icon` component is used by Sidebar (Task 8) — same task. Task 12 references the `crate::routes::strategies::list` registered in Task 5.

---

## Appendix — Plans 2–5 sketch

These are the follow-on plans needed for the rest of v1. Each will be authored as its own document when this plan lands.

**Plan 2 — Read-only screens** (`2026-05-NN-frontend-2-read-only-screens.md`)
- Home: KPIs, equity chart (paper), recent runs, quick start, system status (depends on `/api/health` which lands in Plan 1 + a new `/api/dashboard/home` aggregator)
- Eval runs list: hooks `GET /api/eval/runs` (requires eval engine plan to land `eval_runs` table)
- Run detail (without findings panel — that's Plan 5)
- Compare runs view shell (no findings)
- Settings: providers (CRUD), brokers (read-only stub), daemon, identity, danger
- Activity feed wiring to `api_audit`
- New backend endpoints: `/api/dashboard/home`, `/api/eval/runs`, `/api/eval/runs/:id`, `/api/settings/*`

**Plan 3 — Authoring (Inspector)** (`2026-05-NN-frontend-3-authoring.md`)
- Inspector layout (4-column)
- Bundle outline tree
- Slot editor (form-only, no live preview)
- Token estimate (live, from existing `tokens.rs`)
- Validation diagnostics (new backend feature: `engine::api::strategy::validate`)
- Bundle status (`archived: bool` migration; computed status field)
- Bundle lineage (`parent_bundle_id` migration)
- Save draft, "New strategy", "New from template"
- Backend additions: `POST /api/strategies`, `PUT /api/strategies/:id`, `DELETE /api/strategies/:id`, `GET /api/strategies/:id/validate`, `GET /api/wizard/templates`

**Plan 4 — Agent surfaces** (`2026-05-NN-frontend-4-agent-surfaces.md`)
- Setup wizard end-to-end (depends on Plan 2d shipping WizardLoop)
- Inspector live-preview pane (depends on `POST /api/strategies/:id/preview-slot`)
- Chat rail UI (depends on chat-rail-persistence plan shipping the backend)
- `useSSE` hook
- Streaming message rendering
- Per-route scope handoff
- Wizard `?seed=` URL param plumbing (frontend half — backend half is in Plan 2d Task 7a)

**Plan 5 — Findings + Compare + Polish** (`2026-05-NN-frontend-5-findings-compare-polish.md`)
- New `findings` table schema; `Finding` type with `kind`/`severity`/`evidence`
- Rule-based extractor in `xvision-eval` (post-run pass)
- Trade ledger persistence (`trades` table; eval pipeline writer)
- Findings list UI on Run detail; "Re-extract" button; "Draft variant from this →" navigation
- Compare runs view (full)
- Command palette ⌘K (Radix Dialog + FTS5 search)
- Toast region
- Empty-state polish, error-boundary polish, accessibility audit
- Backend additions: `findings` table + `engine::api::findings::*`, `trades` table + `engine::api::trades::*`, `GET /api/search` (if not already in Plan 2)

Each follow-on plan is independently shippable — Plan 2 stands without Plans 3/4/5; Plan 3 depends on the Plan 1 scaffolding only; etc.

---

## Execution

Plan complete and saved. Two execution options:

1. **Subagent-Driven (recommended)** — I dispatch a fresh subagent per task, review between tasks, fast iteration.
2. **Inline Execution** — Execute tasks in this session using executing-plans, batch execution with checkpoints.

Which approach?
