# Strategy Creation Engine — Plan 2d (Web Dashboard + Agent Wizard) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan #1 + Plan 2a + Plan 2b + Plan 2c merged. Visual design system locked in `docs/design/gptprompts.md`. UX archetypes defined in `docs/design/ux-field.md`.

**Goal:** The product's face. After this plan ships: `xvn` (no args) opens the dashboard at `http://localhost:7878/`. The default landing is the **Agent Wizard** — chat on the left, live visual strategy progress on the right. The wizard is itself an LLM agent that drives xvn's MCP server (Plan 2a) on the user's behalf. Users without an external AI agent (Claude Code / Hermes) can still author strategies end-to-end.

**Architecture:** New crate `xianvec-dashboard` ships an axum HTTP server + a single-page app. The SPA is hand-written HTML/JS (NO Node build step — keep one binary, one install). TradingView Lightweight Charts is embedded as a CDN-loaded library (single `<script src=...>`). The wizard's LLM loop runs *server-side* in the dashboard — the SPA just streams chat over SSE; the dashboard holds the user's API key in memory and calls Anthropic directly (the same `LlmDispatch` from Plan 2a). Multi-archetype routing: `/` is Wizard (L1 default), `/authoring/<id>` is Inspector form (L3), `/marketplace` is the listings spreadsheet, `/live/<deployment_id>` is the live cockpit (Flight Deck archetype).

**Tech Stack:** Rust 2021. New deps: `axum 0.7` (server), `tower-http` (static file serving + tracing), `axum-extra` (SSE), `askama` or `minijinja` (server-side HTML templating). No JS bundler; the SPA is plain HTML + ES modules + Tailwind via CDN. Chart library: TradingView Lightweight Charts via CDN (`https://unpkg.com/lightweight-charts@4.x/dist/lightweight-charts.standalone.production.js`).

**Out of scope (deferred to Plan 4 / never):**
- Server-side rendering for SEO (this is a localhost tool, not a public site)
- Mobile-responsive layouts (desktop-only at the resolutions the design system targets)
- Real-time collaboration / multi-user (single-user localhost only)
- Notebook / Spreadsheet / Lab Bench archetypes (Wizard + Inspector + Marketplace + Live cockpit are the v1 four; the others are post-hackathon)
- TradingView Advanced Charts upgrade — Lightweight only in v1

---

## File structure

```
crates/
├── xianvec-dashboard/                       # NEW
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                           # public API: serve(addr, xvn_home)
│   │   ├── routes/
│   │   │   ├── mod.rs                       # router builder
│   │   │   ├── wizard.rs                    # POST /wizard/chat (SSE), GET /
│   │   │   ├── authoring.rs                 # GET /authoring/<id>, PUT /api/strategy/<id>/slot
│   │   │   ├── marketplace.rs               # GET /marketplace, /api/listings
│   │   │   ├── live.rs                      # GET /live/<deployment_id>, /api/live/<id>/events (SSE)
│   │   │   └── api.rs                       # JSON CRUD endpoints used by the SPA
│   │   ├── wizard_loop.rs                   # the agent loop: LLM + MCP-tool dispatch (server-side)
│   │   ├── static_assets.rs                 # embed static files via include_str! / include_bytes!
│   │   └── templates.rs                     # askama templates wiring
│   ├── templates/                           # HTML templates (askama)
│   │   ├── base.html
│   │   ├── wizard.html
│   │   ├── authoring.html
│   │   ├── marketplace.html
│   │   └── live.html
│   ├── static/
│   │   ├── css/
│   │   │   └── theme.css                    # design system tokens (palette, type)
│   │   ├── js/
│   │   │   ├── wizard.js                    # SSE client + chat UI
│   │   │   ├── inspector.js                 # form bindings
│   │   │   ├── marketplace.js               # listings grid
│   │   │   ├── live.js                      # live cockpit (charts + ticker)
│   │   │   └── chart.js                     # Lightweight Charts wrappers
│   │   └── favicon.svg
│   └── tests/
│       ├── routes_smoke.rs                  # axum-test harness, GET / returns 200
│       └── wizard_chat.rs                   # SSE round-trip with mock LLM
└── xianvec-cli/
    └── src/commands/
        └── dashboard.rs                     # NEW: `xvn` (no args) starts dashboard;
                                             # `xvn dashboard --port 7878` explicit
```

The default `xvn` invocation (no subcommand) launches the dashboard. Existing CLI subcommands (`strategy`, `marketplace`, `agent`, `live`, `deploy`, `skill`) remain untouched.

---

## Phase 2D.A — Dashboard crate scaffolding

### Task 1: New crate + axum hello-world

**Files:**
- Create: `crates/xianvec-dashboard/Cargo.toml`
- Create: `crates/xianvec-dashboard/src/lib.rs`
- Modify: `Cargo.toml` (workspace) — add `crates/xianvec-dashboard` to members + default-members

- [ ] **Step 1: Cargo.toml**

```toml
[package]
name        = "xianvec-dashboard"
description = "Web dashboard + agent wizard for xvn (axum + lightweight charts)"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
repository.workspace   = true

[lib]
name = "xianvec_dashboard"
path = "src/lib.rs"

[dependencies]
xianvec-engine      = { path = "../xianvec-engine" }
xianvec-marketplace = { path = "../xianvec-marketplace" }
xianvec-skills      = { path = "../xianvec-skills" }

axum         = { version = "0.7", features = ["macros"] }
axum-extra   = { version = "0.9", features = ["typed-routing"] }
tower-http   = { version = "0.5", features = ["fs", "trace", "cors"] }
askama       = { version = "0.12", features = ["with-axum"] }
askama_axum  = "0.4"
serde        = { workspace = true }
serde_json   = { workspace = true }
chrono       = { workspace = true }
ulid         = "1"
tokio        = { workspace = true }
tokio-stream = "0.1"
tracing      = { workspace = true }
anyhow       = { workspace = true }
thiserror    = { workspace = true }
async-trait  = { workspace = true }
mime_guess   = "2"
rust-embed   = "8"

[dev-dependencies]
tempfile     = "3"
reqwest      = { workspace = true }
tokio        = { workspace = true, features = ["rt", "macros"] }
```

- [ ] **Step 2: lib.rs hello-world**

```rust
//! xianvec-dashboard — axum web dashboard for xvn.
//!
//! Surfaces:
//! - `/` Agent Wizard (default landing)
//! - `/authoring/<id>` L3 Inspector form
//! - `/marketplace` listings spreadsheet
//! - `/live/<deployment_id>` Flight Deck cockpit

use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{Router, response::Html, routing::get};

pub mod routes;
pub mod static_assets;
pub mod templates;
pub mod wizard_loop;

#[derive(Clone)]
pub struct AppState {
    pub xvn_home: PathBuf,
}

pub async fn serve(addr: SocketAddr, xvn_home: PathBuf) -> anyhow::Result<()> {
    let state = AppState { xvn_home };
    let app: Router = routes::build_router(state);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("xvn dashboard listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
```

- [ ] **Step 3: Stub modules**

```rust
// routes/mod.rs
use axum::{Router, routing::get};
use crate::AppState;

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { "xvn dashboard scaffold — Task 2 wires real routes" }))
        .with_state(state)
}
```

```rust
// static_assets.rs, templates.rs, wizard_loop.rs — empty placeholders for now
```

- [ ] **Step 4: Smoke test**

```rust
// tests/routes_smoke.rs
use axum::body::Body;
use axum::http::Request;
use tower::ServiceExt;
use xianvec_dashboard::{routes::build_router, AppState};
use tempfile::tempdir;

#[tokio::test]
async fn root_returns_scaffold_text() {
    let dir = tempdir().unwrap();
    let state = AppState { xvn_home: dir.path().to_path_buf() };
    let app = build_router(state);
    let resp = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await.unwrap();
    assert_eq!(resp.status(), 200);
}
```

- [ ] **Step 5: Build + test + commit**

```bash
cargo test -p xianvec-dashboard 2>&1 | grep "test result"  # 1 passed
git add crates/xianvec-dashboard Cargo.toml
git commit -m "feat(dashboard): scaffold xianvec-dashboard crate with axum hello"
```

---

### Task 2: `xvn` (no args) launches dashboard

**Files:**
- Create: `crates/xianvec-cli/src/commands/dashboard.rs`
- Modify: `crates/xianvec-cli/src/lib.rs` (default behavior + Dashboard subcommand)

- [ ] **Step 1: Add deps**

In `crates/xianvec-cli/Cargo.toml`: `xianvec-dashboard = { path = "../xianvec-dashboard" }`.

- [ ] **Step 2: Add `Dashboard` subcommand**

```rust
// commands/dashboard.rs
use clap::Args;
use std::net::SocketAddr;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct DashboardCmd {
    #[arg(long, default_value = "127.0.0.1:7878")]
    addr: SocketAddr,
    #[arg(long)]
    no_open: bool,    // skip opening browser
}

pub async fn run(cmd: DashboardCmd) -> anyhow::Result<()> {
    let xvn_home = std::env::var("XVN_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
    if !cmd.no_open {
        let url = format!("http://{}/", cmd.addr);
        let _ = open::that(&url);   // crate `open` for cross-platform browser launch
    }
    xianvec_dashboard::serve(cmd.addr, xvn_home).await
}
```

Add `open = "5"` to xianvec-cli Cargo.toml.

- [ ] **Step 3: Make `xvn` (no args) default to `xvn dashboard`**

In `crates/xianvec-cli/src/lib.rs`'s `Cli` struct, change the subcommand to optional:

```rust
#[derive(Parser, Debug)]
#[command(name = "xvn", version, about = "XIANVEC: AI trading agent platform")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    pub async fn run(self) -> anyhow::Result<()> {
        match self.command {
            Some(Command::Dashboard(cmd)) => commands::dashboard::run(cmd).await,
            // ... other subcommands ...
            None => commands::dashboard::run(commands::dashboard::DashboardCmd {
                addr: "127.0.0.1:7878".parse().unwrap(),
                no_open: false,
            }).await,
        }
    }
}
```

- [ ] **Step 4: Smoke**

```bash
cargo run -q -p xianvec-cli &
sleep 1
curl -s http://127.0.0.1:7878/ | head -1
kill %1
```

Expected: scaffold message reachable.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-cli
git commit -m "feat(cli): xvn (no args) launches dashboard at localhost:7878"
```

---

## Phase 2D.B — Design system + Wizard archetype

### Task 3: Tailwind via CDN + design tokens CSS

**File:** `crates/xianvec-dashboard/static/css/theme.css`

Per `docs/design/gptprompts.md`'s shared design system: deep navy-charcoal palette, mint accent (#5BE0A2), Inter sans + JetBrains Mono. Define CSS custom properties mapping to those values, plus utility classes shared across all archetypes.

```css
:root {
  --bg-primary: #0B0F14;
  --bg-elevated: #11161D;
  --bg-panel: #1A2029;
  --border: #1F2630;
  --text-primary: #E8ECF1;
  --text-secondary: #8B95A4;
  --text-tertiary: #5A6573;
  --accent-mint: #5BE0A2;
  --status-warn: #F4B23A;
  --status-danger: #F26A6A;
  --status-info: #6EB4F2;
}

body {
  background: var(--bg-primary);
  color: var(--text-primary);
  font-family: 'Inter', system-ui, sans-serif;
  margin: 0;
}

.mono { font-family: 'JetBrains Mono', ui-monospace, monospace; }

.card {
  background: var(--bg-elevated);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 16px;
}

.btn-primary {
  background: var(--accent-mint);
  color: var(--bg-primary);
  border: 0;
  padding: 8px 14px;
  border-radius: 6px;
  font-weight: 500;
  cursor: pointer;
}

.btn-ghost {
  background: transparent;
  color: var(--text-primary);
  border: 1px solid var(--border);
  padding: 8px 14px;
  border-radius: 6px;
  cursor: pointer;
}

.pill {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 99px;
  font-size: 11px;
  background: var(--bg-panel);
  border: 1px solid var(--border);
}

/* Dark-mode borders rule (CLAUDE.md): never use 100% white. */
.border-soft { border: 1px solid var(--border); }
```

Embed via `rust-embed`:

```rust
// static_assets.rs
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "static/"]
pub struct Static;
```

Mount as static file route in router:

```rust
// routes/mod.rs
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

async fn static_handler(Path(path): Path<String>) -> impl IntoResponse {
    match crate::static_assets::Static::get(&path) {
        Some(file) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref().to_string())], file.data).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

pub fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/static/*path", get(static_handler))
        // ... other routes added in Task 4+
        .with_state(state)
}
```

Test: smoke `curl http://localhost:7878/static/css/theme.css` returns 200 + the CSS body.

Commit `feat(dashboard): static asset mount + design system CSS`.

---

### Task 4: Wizard route + base template

**Files:**
- Create: `crates/xianvec-dashboard/templates/base.html`
- Create: `crates/xianvec-dashboard/templates/wizard.html`
- Create: `crates/xianvec-dashboard/src/routes/wizard.rs`
- Modify: `crates/xianvec-dashboard/src/routes/mod.rs`

- [ ] **Step 1: base.html**

```html
{# templates/base.html #}
<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>{% block title %}xvn{% endblock %}</title>
<link rel="stylesheet" href="/static/css/theme.css">
<link rel="icon" href="/static/favicon.svg">
<script src="https://cdn.tailwindcss.com"></script>
<script>
  tailwind.config = {
    theme: { extend: { colors: {
      mint: '#5BE0A2', warn: '#F4B23A', danger: '#F26A6A', info: '#6EB4F2',
    } } }
  };
</script>
</head>
<body>
<header class="flex items-center justify-between px-6 py-3 border-b border-soft">
  <div class="font-semibold mono lowercase">xvn</div>
  <nav class="flex gap-4 text-sm">
    <a href="/" class="hover:text-mint">Wizard</a>
    <a href="/marketplace" class="hover:text-mint">Marketplace</a>
    <a href="/authoring" class="hover:text-mint">Authoring</a>
    <a href="/live" class="hover:text-mint">Live</a>
  </nav>
</header>
<main class="p-6">{% block main %}{% endblock %}</main>
{% block scripts %}{% endblock %}
</body>
</html>
```

- [ ] **Step 2: wizard.html**

Per `docs/design/gptprompts.md` archetype 1a (Strategy Wizard) — two columns: chat on the left (~58%), visual progress sidecar on the right (~42%). Include: chat thread, composer with quick-reply chips, the "Building draft" sidecar with seven layer rows + ready progress bar.

```html
{# templates/wizard.html #}
{% extends "base.html" %}
{% block title %}xvn — Wizard{% endblock %}
{% block main %}
<div class="grid grid-cols-[58%_42%] gap-6 max-w-6xl mx-auto">
  <section class="card flex flex-col h-[80vh]" id="chat">
    <div class="text-xs uppercase text-secondary mb-3">Wizard</div>
    <div id="thread" class="flex-1 overflow-y-auto space-y-3"></div>
    <form id="composer" class="flex gap-2 mt-4">
      <input id="msg" class="flex-1 bg-panel border border-soft rounded-md px-3 py-2 text-sm"
             placeholder="type your reply…" />
      <button class="btn-primary" type="submit">Send</button>
    </form>
  </section>
  <aside class="card h-[80vh] overflow-y-auto" id="progress">
    <div class="text-xs uppercase text-secondary mb-3">Building <span class="mono" id="draft-name">—</span></div>
    <ul class="space-y-2 text-sm" id="layers">
      <li>① Data layer <span class="float-right text-secondary">—</span></li>
      <li>② Regime classifier <span class="float-right text-secondary">—</span></li>
      <li>③ Signal interpreter <span class="float-right text-secondary">—</span></li>
      <li>④ Decision arbiter <span class="float-right text-secondary">—</span></li>
      <li>⑤ Mechanical rules <span class="float-right text-secondary">—</span></li>
      <li>⑥ Risk preset <span class="float-right text-secondary">—</span></li>
      <li>⑦ Execution <span class="float-right text-secondary">—</span></li>
    </ul>
    <div class="mt-6">
      <div class="text-xs uppercase text-secondary mb-1">Ready</div>
      <div class="h-2 bg-panel rounded">
        <div id="ready-bar" class="h-2 bg-mint rounded" style="width: 0%"></div>
      </div>
    </div>
    <button class="btn-primary mt-4 w-full" id="run-eval-btn" disabled>Run preview eval</button>
  </aside>
</div>
{% endblock %}
{% block scripts %}
<script type="module" src="/static/js/wizard.js"></script>
{% endblock %}
```

- [ ] **Step 3: routes/wizard.rs — render template + LLM key gate**

```rust
use askama::Template;
use askama_axum::IntoResponse;
use axum::{extract::State, response::Response};

#[derive(Template)]
#[template(path = "wizard.html")]
struct WizardPage;

pub async fn root(State(_state): State<crate::AppState>) -> Response {
    WizardPage.into_response()
}
```

Wire `/` to `wizard::root` in `routes/mod.rs`.

- [ ] **Step 4: Test**

```rust
// extend tests/routes_smoke.rs
#[tokio::test]
async fn root_renders_wizard_html() {
    let dir = tempdir().unwrap();
    let state = AppState { xvn_home: dir.path().to_path_buf() };
    let app = build_router(state);
    let resp = app
        .oneshot(Request::get("/").body(Body::empty()).unwrap())
        .await.unwrap();
    let body = String::from_utf8(axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap().to_vec()).unwrap();
    assert!(body.contains("Wizard"));
    assert!(body.contains("Decision arbiter"));
}
```

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-dashboard
git commit -m "feat(dashboard): Wizard route + base template + visual progress sidecar"
```

---

### Task 5: Wizard SSE chat endpoint

**File:** `crates/xianvec-dashboard/src/routes/wizard.rs`

The SPA POSTs the user's message to `/api/wizard/chat` (with the LLM key in headers or session); server runs the LLM loop, streams chunks back as SSE.

```rust
use std::convert::Infallible;
use std::time::Duration;

use axum::{extract::{Json, State}, response::sse::{Event, KeepAlive, Sse}};
use futures::Stream;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub struct ChatRequest {
    pub session_id: String,        // ULID, generated client-side per session
    pub message: String,
    pub api_key: String,           // user provides on first message; server holds in memory keyed by session_id
    pub provider: String,          // "anthropic" | "openai"
    pub model: String,             // e.g., "claude-sonnet-4-6"
}

#[derive(Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WizardEvent {
    Token { text: String },
    ToolCall { tool: String, args: serde_json::Value },
    ToolResult { tool: String, result: serde_json::Value },
    Layer { which: String, status: String },  // updates the visual progress sidecar
    Ready { progress: f32 },
    Done { draft_id: Option<String> },
    Error { message: String },
}

pub async fn chat(
    State(state): State<crate::AppState>,
    Json(req): Json<ChatRequest>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut loop_ctx = crate::wizard_loop::WizardLoop::new(state.xvn_home.clone(), req).await;
        while let Some(event) = loop_ctx.next_event().await {
            let payload = serde_json::to_string(&event).unwrap();
            yield Ok::<_, Infallible>(Event::default().data(payload));
            if matches!(event, WizardEvent::Done { .. } | WizardEvent::Error { .. }) {
                break;
            }
        }
    };
    Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
}
```

> Subagent should add `async-stream = "0.3"` and `futures = "0.3"` to xianvec-dashboard Cargo.toml.

- [ ] Wire `POST /api/wizard/chat` → `chat` in `routes/mod.rs`.
- [ ] The `WizardLoop` in `wizard_loop.rs` is the next task. For now, stub it to emit a single Token event + Done.

Commit `feat(dashboard): wizard SSE chat endpoint with stub loop`.

---

### Task 6: WizardLoop — server-side LLM agent that drives MCP

**File:** `crates/xianvec-dashboard/src/wizard_loop.rs`

The wizard is itself an LLM agent. Server-side, this struct:
1. Holds conversation state for a session_id
2. Maintains an `LlmDispatch` instance built from the user's provider+key+model
3. On `next_event()`:
   - Calls `dispatch.complete(...)` with the wizard's system prompt + conversation
   - If response has `ContentBlock::ToolUse`, routes to MCP authoring functions (`xianvec_engine::mcp::authoring::*`) and emits ToolCall + ToolResult events
   - Streams the assistant's text back as Token events
   - Emits Layer + Ready events when the wizard touches a strategy slot
4. On Done, emits the final draft_id

```rust
use std::path::PathBuf;

use xianvec_engine::agent::llm::{
    AnthropicDispatch, ContentBlock, LlmDispatch, LlmRequest, Message, ToolDefinition, StopReason,
};
use xianvec_engine::mcp::authoring;

use crate::routes::wizard::{ChatRequest, WizardEvent};

const WIZARD_SYSTEM_PROMPT: &str = include_str!("../prompts/wizard.md");

pub struct WizardLoop {
    xvn_home: PathBuf,
    dispatch: Box<dyn LlmDispatch>,
    messages: Vec<Message>,
    pending_events: Vec<WizardEvent>,
    is_done: bool,
}

impl WizardLoop {
    pub async fn new(xvn_home: PathBuf, req: ChatRequest) -> Self {
        let dispatch: Box<dyn LlmDispatch> = match req.provider.as_str() {
            "anthropic" => Box::new(AnthropicDispatch::new(req.api_key.clone())),
            other => panic!("unsupported provider: {other}"),  // TODO: error event instead
        };
        let mut messages = vec![Message {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: req.message.clone() }],
        }];
        Self {
            xvn_home,
            dispatch,
            messages,
            pending_events: vec![],
            is_done: false,
        }
    }

    pub async fn next_event(&mut self) -> Option<WizardEvent> {
        if let Some(ev) = self.pending_events.pop() {
            return Some(ev);
        }
        if self.is_done {
            return None;
        }
        // Call LLM with wizard system prompt + accumulated messages.
        let req = LlmRequest {
            model: "claude-sonnet-4-6".into(),  // TODO take from session
            system_prompt: WIZARD_SYSTEM_PROMPT.into(),
            messages: self.messages.clone(),
            max_tokens: 1500,
            tools: wizard_tool_defs(),
        };
        let resp = match self.dispatch.complete(req).await {
            Ok(r) => r,
            Err(e) => {
                self.is_done = true;
                return Some(WizardEvent::Error { message: e.to_string() });
            }
        };
        // Emit Token events for text blocks.
        for block in &resp.content {
            if let ContentBlock::Text { text } = block {
                self.pending_events.push(WizardEvent::Token { text: text.clone() });
            }
        }
        // Run any tool_use blocks against MCP authoring.
        let tool_uses: Vec<_> = resp.content.iter().filter_map(|b| match b {
            ContentBlock::ToolUse { id, name, input } => Some((id.clone(), name.clone(), input.clone())),
            _ => None,
        }).collect();
        if !tool_uses.is_empty() {
            self.messages.push(Message { role: "assistant".into(), content: resp.content.clone() });
            let mut tool_results = vec![];
            for (id, name, input) in tool_uses {
                self.pending_events.push(WizardEvent::ToolCall { tool: name.clone(), args: input.clone() });
                let result = run_authoring_tool(&self.xvn_home, &name, input).await
                    .unwrap_or_else(|e| serde_json::json!({"error": e.to_string()}));
                self.pending_events.push(WizardEvent::ToolResult { tool: name, result: result.clone() });
                tool_results.push(ContentBlock::ToolResult {
                    tool_use_id: id, content: result.to_string(),
                });
            }
            self.messages.push(Message { role: "user".into(), content: tool_results });
        } else {
            self.is_done = true;
            self.pending_events.push(WizardEvent::Done { draft_id: None });
        }
        // Reverse pending so we pop in the right order.
        self.pending_events.reverse();
        self.pending_events.pop()
    }
}

async fn run_authoring_tool(
    xvn_home: &std::path::Path,
    name: &str,
    input: serde_json::Value,
) -> anyhow::Result<serde_json::Value> {
    // Mirror xianvec_engine::mcp::authoring's dispatch logic, scoped to xvn_home.
    // This is the same code path as the MCP server's call_tool; consider
    // extracting a shared dispatcher in xianvec-engine to avoid duplication.
    // For Plan 2d, inline-call the authoring fns directly:
    match name {
        "list_templates" => {
            let _: authoring::ListTemplatesArgs = serde_json::from_value(input)?;
            let items = authoring::list_templates(authoring::ListTemplatesArgs {})?;
            Ok(serde_json::to_value(items)?)
        }
        "create_strategy" | "get_strategy" | "update_slot" | "set_mechanical_param"
        | "set_risk_config" | "validate_draft" => {
            // Each verb wraps the FilesystemStore at xvn_home.join("strategies").
            // Subagent: implement these by lifting authoring helpers into a `Dispatcher`
            // that takes a store. This same Dispatcher will back the MCP server too —
            // shared code path between MCP and Wizard.
            anyhow::bail!("authoring verb '{name}' not yet wired in WizardLoop — Task 6 follow-up")
        }
        other => anyhow::bail!("unknown authoring verb: {other}"),
    }
}

fn wizard_tool_defs() -> Vec<ToolDefinition> {
    // The 7 authoring verbs from Plan 2a.
    vec![
        ToolDefinition {
            name: "list_templates".into(),
            description: "List xvn strategy templates with display name and plain summary".into(),
            input_schema: serde_json::json!({"type": "object", "properties": {}, "required": []}),
        },
        // ...the other 6 verbs from Plan 2a §A schemas...
    ]
}
```

> Note: there's near-duplication between MCP server's `call_tool` and Wizard's `run_authoring_tool`. Strongly consider extracting a `xianvec-engine/src/authoring_dispatcher.rs` that both call into. Subagent should make this refactor and have both surfaces (MCP + Wizard) call it. The plan as written tolerates inline duplication for v1 if the refactor adds too much risk.

- [ ] **Step 1: Wizard system prompt**

Create `crates/xianvec-dashboard/prompts/wizard.md`:

```markdown
You are the xvn setup agent. The user is building or selecting an AI trading
strategy. Walk them through it.

Your tools:
- list_templates: see the 8 v1 templates
- create_strategy: instantiate a new draft
- update_slot: customize a slot's prompt
- set_mechanical_param: set a parameter (e.g., RSI threshold)
- set_risk_config: apply a preset (conservative/balanced/aggressive)
- validate_draft: verify before recommending the user run an eval

Style:
- Plain English at first ("Buys dips", not "Mean reversion")
- Ask one or two questions at a time
- Confirm before mutating (e.g., "I'll set the RSI oversold threshold to 25 — sound good?")
- When the strategy is ready to evaluate, say so explicitly and stop.

Never invent tools that aren't in the list. Never propose actions that
require an MCP verb you weren't given.
```

- [ ] **Step 2: Test with mock dispatch**

Pattern: build a `WizardLoop` with a mock dispatch that emits a fixed tool_use → tool_result loop, await `next_event` repeatedly, assert the right WizardEvents come through.

> The test setup is non-trivial because `WizardLoop::new` builds a real `AnthropicDispatch`. Refactor: take `Box<dyn LlmDispatch>` directly so the test can inject `MockDispatch`.

Commit `feat(dashboard): WizardLoop drives MCP authoring tools server-side`.

---

### Task 7: Wizard front-end JS (`wizard.js`)

**File:** `crates/xianvec-dashboard/static/js/wizard.js`

```javascript
const sessionId = crypto.randomUUID();
const thread = document.getElementById('thread');
const composer = document.getElementById('composer');
const msgInput = document.getElementById('msg');
const draftName = document.getElementById('draft-name');
const readyBar = document.getElementById('ready-bar');
const layers = document.getElementById('layers');

let apiKey = localStorage.getItem('xvn_anthropic_key');
if (!apiKey) {
  apiKey = prompt('Paste your Anthropic API key (stored locally only):');
  if (apiKey) localStorage.setItem('xvn_anthropic_key', apiKey);
}

function appendMessage(role, text) {
  const div = document.createElement('div');
  div.className = role === 'user'
    ? 'self-end bg-panel rounded p-3 max-w-[75%]'
    : 'self-start border border-soft rounded p-3 max-w-[75%]';
  div.textContent = text;
  thread.appendChild(div);
  thread.scrollTop = thread.scrollHeight;
}

let activeAssistantBubble = null;

async function send(message) {
  appendMessage('user', message);
  const resp = await fetch('/api/wizard/chat', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      session_id: sessionId, message, api_key: apiKey,
      provider: 'anthropic', model: 'claude-sonnet-4-6',
    }),
  });
  const reader = resp.body.getReader();
  const decoder = new TextDecoder();
  let buf = '';
  while (true) {
    const { value, done } = await reader.read();
    if (done) break;
    buf += decoder.decode(value, { stream: true });
    const lines = buf.split('\n');
    buf = lines.pop();
    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const evt = JSON.parse(line.slice(6));
        handleEvent(evt);
      }
    }
  }
}

function handleEvent(evt) {
  switch (evt.type) {
    case 'token':
      if (!activeAssistantBubble) {
        activeAssistantBubble = document.createElement('div');
        activeAssistantBubble.className = 'self-start border border-soft rounded p-3 max-w-[75%]';
        thread.appendChild(activeAssistantBubble);
      }
      activeAssistantBubble.textContent += evt.text;
      thread.scrollTop = thread.scrollHeight;
      break;
    case 'tool_call':
      // optional: visual indicator that wizard is calling a tool
      break;
    case 'tool_result':
      // update sidecar based on tool output
      if (evt.tool === 'create_strategy') draftName.textContent = evt.result.id || '—';
      if (evt.tool === 'update_slot') updateLayerStatus(evt.result.slot, '✓');
      break;
    case 'layer':
      updateLayerStatus(evt.which, evt.status);
      break;
    case 'ready':
      readyBar.style.width = `${(evt.progress * 100).toFixed(0)}%`;
      break;
    case 'done':
      activeAssistantBubble = null;
      break;
    case 'error':
      appendMessage('assistant', `[error] ${evt.message}`);
      break;
  }
}

function updateLayerStatus(which, status) {
  const map = { regime: 1, intern: 2, trader: 3, risk: 5, execution: 6 };
  const idx = map[which];
  if (!idx) return;
  const li = layers.children[idx];
  const span = li.querySelector('span');
  span.textContent = status;
  span.style.color = status === '✓' ? 'var(--accent-mint)' : 'var(--text-secondary)';
}

composer.addEventListener('submit', e => {
  e.preventDefault();
  const m = msgInput.value.trim();
  if (!m) return;
  msgInput.value = '';
  send(m);
});

// Kickoff: greet on page load.
appendMessage('assistant', "Hi — I'm xvn's setup agent. What kind of strategy would you like to build?");
```

End-to-end test: spin up dashboard, simulate a chat that creates + validates a draft. Use a real Anthropic key in CI (`#[ignore]`) or mock the LLM in test mode.

Commit `feat(dashboard): wizard.js front-end with SSE + visual progress`.

---

## Phase 2D.C — Inspector form (L3 archetype)

### Task 8: Inspector route + template

Per `gptprompts.md` archetype 2a (Strategy Inspector). Single dense form with collapsible layer rows, validation summary right rail.

Routes:
- `GET /authoring` → list of drafts, link to each
- `GET /authoring/<id>` → render Inspector form
- `PUT /api/strategy/<id>/slot/<role>` → update slot (calls authoring dispatcher)
- `PUT /api/strategy/<id>/risk` → update risk preset/explicit

Template `templates/authoring.html`: per archetype 2a — left nav rail of drafts, center panel of 7 collapsible layer cards, right rail "Validation" card + ghost/primary/dropdown buttons.

JS `static/js/inspector.js`: wire form changes to PUT calls; show validation result live.

Test: render Inspector for an existing draft, assert all 7 layer rows present + validation status visible.

Commit `feat(dashboard): Inspector form (L3 archetype) for direct authoring`.

---

### Task 9: Marketplace listings grid

Per archetype 7a — sortable spreadsheet of templates / listings. Route `GET /marketplace`. JS sorts client-side.

Listings come from `xianvec-marketplace::browse_listings` (Plan 2b). Templates come from `xianvec-engine::templates::registry`.

Test: render with a few seeded listings, assert sortable columns + row click navigates to a detail page.

Commit `feat(dashboard): marketplace grid (Spreadsheet archetype)`.

---

### Task 10: Live cockpit (Flight Deck archetype)

Per archetype 6b — full-bleed cockpit with gauge tiles + progress bar + 4 large action buttons. Streams from `/api/live/<deployment_id>/events` (SSE proxy of the scheduler_events table).

Embed Lightweight Charts:

```html
<script src="https://unpkg.com/lightweight-charts@4.1.3/dist/lightweight-charts.standalone.production.js"></script>
```

`static/js/chart.js` wraps `LightweightCharts.createChart(...)` with the design system theme (mint up, coral down, dark grid).

Routes:
- `GET /live/<deployment_id>` → render Flight Deck
- `GET /api/live/<deployment_id>/events` → SSE stream from `scheduler_events`

Test: smoke that an active deployment's events render in the ticker + chart updates.

Commit `feat(dashboard): live cockpit (Flight Deck) with Lightweight Charts`.

---

## Phase 2D.D — Integration + polish

### Task 11: End-to-end smoke (browser-driven)

Use `playwright` or a similar browser-driver test harness. For Rust workspace, `chromedriver` + `thirtyfour` is one option; or use a shell script + curl + manual visual inspection for hackathon scope.

Hackathon-acceptable smoke:

```bash
export XVN_HOME=/tmp/xvn-2d-smoke
xvn &
DASHBOARD_PID=$!
sleep 2
# Open browser manually, click through wizard, verify:
# - Wizard greets you
# - You can paste an Anthropic key + chat
# - Wizard calls list_templates + create_strategy
# - The visual progress sidecar updates as the draft is built
# - You can click "Run preview eval" and see decisions stream
# - Marketplace tab shows the new draft (after publish)
# - Live tab shows a deployment's gauges + chart

kill $DASHBOARD_PID
```

Document the smoke procedure in `crates/xianvec-dashboard/README.md`.

Commit `chore: Plan 2d end-to-end smoke verified`.

### Task 12: README + manual

Update top-level `MANUAL.md` with the dashboard section. Add `crates/xianvec-dashboard/README.md` describing the architecture (Wizard server-side LLM loop, SSE streaming, Lightweight Charts CDN, design system tokens).

Commit `docs: Plan 2d dashboard README + manual update`.

### Task 13: Final workspace check

`cargo test --workspace` clean. clippy clean. fmt scoped to plan-touched crates. xianvec-eval still untouched. ~13 commits since Plan 2c's tip.

---

## Self-review checklist

**Spec coverage:**
- [x] §2 KISS / Agent Wizard — Wizard at `/`, chat + visual progress sidecar, server-side LLM loop driving MCP
- [x] §8 Authoring entry points — Web UI form (Inspector) + Wizard (built-in CLI wizard from Plan #1 + external MCP from Plan 2a all share the same authoring dispatcher)
- [x] §13 Marketplace browsing — listings grid (Spreadsheet archetype)
- [x] §11 Live execution monitoring — Flight Deck cockpit, SSE-streamed events
- [x] Visual design system locked — palette / typography / components per docs/design/gptprompts.md

**Out of scope as planned:**
- [ ] Notebook / Lab Bench / Canvas / Slot Machine archetypes — post-hackathon
- [ ] Tier B sealing UI — Plan 4
- [ ] Eval comparison view — Plan 3 (the eval engine plan ships its own dashboard surface for run comparisons)

**Type consistency:** `AppState`, `WizardLoop`, `ChatRequest`, `WizardEvent`, all axum routers + handlers, JS event types — consistent.

**Frequent commits:** 13 tasks → ~13 commits.

---

## What's next

Plan 3 — **Eval Engine** — already specified in `docs/superpowers/specs/2026-05-08-eval-engine-design.md`; this plan defines its implementation.
Plan 4 (post-hackathon) — Tier B sealing + xvn API server + remaining UX archetypes (Notebook, Lab Bench, Canvas).
