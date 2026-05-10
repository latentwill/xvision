# xvn Scheduling Plan — Part 3 (Tasks 10–14: tool registry + agent runner)

> Continues `2026-05-10-xvn-scheduling-and-agent-cli-part2.md`. Same goals/architecture/tech stack apply.

---

### Task 10: xianvec-intern — LlmToolDispatch trait

**Files:**
- Create: `crates/xianvec-intern/src/tool_dispatch.rs`
- Modify: `crates/xianvec-intern/src/lib.rs`
- Create: `crates/xianvec-intern/tests/tool_dispatch_mock.rs`

> **Context.** Existing intern backend (`AnthropicIntern`, `OpenAICompatIntern`) is hard-wired to one shape — the briefing prompt — and produces a single typed JSON response. The agent runner needs a more general loop: send messages, receive assistant turns that may include tool calls, send tool results back, repeat. This task adds a small `LlmToolDispatch` trait + a mock impl for tests. Anthropic + OpenAI-compat real impls land as follow-up — for v1 we ship the trait + mock so the AgentRunner is testable.

- [ ] **Step 1: Failing test against a mock**

Create `crates/xianvec-intern/tests/tool_dispatch_mock.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use xianvec_intern::tool_dispatch::{
    AssistantTurn, LlmToolDispatch, Message, ToolCall, ToolDispatchRequest, ToolSchema,
};

struct MockDispatch { canned: Vec<AssistantTurn> }

#[async_trait]
impl LlmToolDispatch for MockDispatch {
    async fn run_turn(&self, _req: ToolDispatchRequest) -> anyhow::Result<AssistantTurn> {
        Ok(self.canned[0].clone())
    }
}

#[tokio::test]
async fn mock_returns_canned_assistant_turn() {
    let dispatch = MockDispatch {
        canned: vec![AssistantTurn {
            text: Some("hello".into()),
            tool_calls: vec![ToolCall {
                tool_call_id: "call_1".into(),
                name: "report.strategy_review".into(),
                arguments: serde_json::json!({}),
            }],
            stop_reason: "tool_use".into(),
            tokens_in: 10, tokens_out: 5, cache_read_tokens: 0, cache_write_tokens: 0,
        }],
    };
    let res = dispatch.run_turn(ToolDispatchRequest {
        model: "claude-opus-4-7".into(),
        system: "you are an agent".into(),
        messages: vec![Message::user("plan tonight")],
        tools: vec![ToolSchema {
            name: "report.strategy_review".into(),
            description: "review".into(),
            input_schema: serde_json::json!({"type":"object"}),
        }],
        max_tokens: 1024,
        temperature: 0.0,
    }).await.unwrap();
    assert_eq!(res.tool_calls.len(), 1);
    assert_eq!(res.tool_calls[0].name, "report.strategy_review");
}
```

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement `tool_dispatch.rs`**

Create `crates/xianvec-intern/src/tool_dispatch.rs`:

```rust
//! Generic LLM tool-use dispatch trait. Used by the AgentRunner to send a
//! turn (system + history + tools + max_tokens) and receive an assistant
//! turn that may include text and/or tool calls.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    System { content: String },
    User { content: String },
    Assistant {
        content: Option<String>,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        tool_call_id: String,
        is_error: bool,
        content: serde_json::Value,
    },
}

impl Message {
    pub fn user(s: impl Into<String>) -> Self { Message::User { content: s.into() } }
    pub fn system(s: impl Into<String>) -> Self { Message::System { content: s.into() } }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub tool_call_id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSchema {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,    // JSON Schema for arguments
}

#[derive(Debug, Clone)]
pub struct ToolDispatchRequest {
    pub model: String,
    pub system: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolSchema>,
    pub max_tokens: u32,
    pub temperature: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssistantTurn {
    pub text: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub stop_reason: String,            // "end_turn" | "tool_use" | "max_tokens" | other
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cache_read_tokens: u32,
    pub cache_write_tokens: u32,
}

#[async_trait]
pub trait LlmToolDispatch: Send + Sync {
    async fn run_turn(&self, req: ToolDispatchRequest) -> anyhow::Result<AssistantTurn>;
}
```

- [ ] **Step 4: Re-export from `lib.rs`**

In `crates/xianvec-intern/src/lib.rs`, add:

```rust
pub mod tool_dispatch;
pub use tool_dispatch::{
    AssistantTurn, LlmToolDispatch, Message, ToolCall, ToolDispatchRequest, ToolSchema,
};
```

- [ ] **Step 5: Run — expect pass**

```bash
cargo test -p xianvec-intern --test tool_dispatch_mock
```

Expected: 1 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-intern/src/tool_dispatch.rs \
        crates/xianvec-intern/src/lib.rs \
        crates/xianvec-intern/tests/tool_dispatch_mock.rs
git commit -m "feat(intern): LlmToolDispatch trait + Message/ToolCall/AssistantTurn shapes"
```

---

### Task 11: ToolRegistry + ToolHandler + glob matching

**Files:**
- Create: `crates/xianvec-engine/src/agent_runner/mod.rs`
- Create: `crates/xianvec-engine/src/agent_runner/registry.rs`
- Create: `crates/xianvec-engine/src/agent_runner/builtins.rs`
- Modify: `crates/xianvec-engine/src/lib.rs`
- Create: `crates/xianvec-engine/tests/tool_registry.rs`

- [ ] **Step 1: Failing tests**

Create `crates/xianvec-engine/tests/tool_registry.rs`:

```rust
use std::sync::Arc;

use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xianvec_engine::agent_runner::registry::{filter_tools, ToolRegistry};
use xianvec_engine::api::{Actor, ApiContext};

async fn fixture() -> (Arc<ApiContext>, ToolRegistry, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    std::fs::create_dir_all(dir.path().join("strategies/sh_t")).unwrap();
    std::fs::write(dir.path().join("strategies/sh_t/manifest.toml"), b"x").unwrap();
    let ctx = Arc::new(ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap()
    })));
    let registry = ToolRegistry::with_builtins();
    (ctx, registry, dir)
}

#[tokio::test]
async fn registry_includes_all_seven_domains() {
    let (_ctx, reg, _dir) = fixture().await;
    let names: Vec<&str> = reg.tools().iter().map(|t| t.schema().name.as_str()).collect();
    assert!(names.iter().any(|n| n.starts_with("strategy.")));
    assert!(names.iter().any(|n| n.starts_with("risk.")));
    assert!(names.iter().any(|n| n.starts_with("deploy.")));
    assert!(names.iter().any(|n| n.starts_with("report.")));
    assert!(names.iter().any(|n| n.starts_with("maintenance.")));
    assert!(names.iter().any(|n| n.starts_with("schedule.")));
    assert!(names.iter().any(|n| n.starts_with("autoresearch.")));
    assert!(names.iter().any(|n| n == &"record_outcome"));
}

#[tokio::test]
async fn filter_strategy_glob() {
    let (_ctx, reg, _dir) = fixture().await;
    let filtered = filter_tools(&reg, &["strategy.*".into(), "record_outcome".into()]);
    assert!(filtered.iter().all(|t| {
        let n = &t.schema().name;
        n.starts_with("strategy.") || n == "record_outcome"
    }));
    assert!(!filtered.iter().any(|t| t.schema().name.starts_with("risk.")));
}

#[tokio::test]
async fn invoke_strategy_show_via_registry() {
    let (ctx, reg, _dir) = fixture().await;
    xianvec_engine::api::strategy::record_created(&ctx, "sh_t", Actor::Cli).await.unwrap();
    let handler = reg.tools().iter().find(|t| t.schema().name == "strategy.show").unwrap();
    let result = handler.invoke(&ctx, serde_json::json!({"id":"sh_t"}), Actor::Cli).await.unwrap();
    assert_eq!(result["status"], "active");
}

#[tokio::test]
async fn record_outcome_returns_arguments_unchanged() {
    let (ctx, reg, _dir) = fixture().await;
    let handler = reg.tools().iter().find(|t| t.schema().name == "record_outcome").unwrap();
    let args = serde_json::json!({"summary":"done","actions_taken":[],"anomalies":[]});
    let res = handler.invoke(&ctx, args.clone(), Actor::Cli).await.unwrap();
    assert_eq!(res, args);
}
```

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement registry types**

Create `crates/xianvec-engine/src/agent_runner/mod.rs`:

```rust
//! Generic tool-use agent runner. The runner is invoked by the scheduler at
//! schedule fire-time, and is also reusable for ad-hoc agent invocations
//! (e.g., `xvn agent ask`).

pub mod builtins;
pub mod registry;
```

Create `crates/xianvec-engine/src/agent_runner/registry.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::Value;

use crate::agent_runner::builtins;
use crate::api::{ApiContext, ApiError, ApiResult, Actor};

pub use xianvec_intern::tool_dispatch::ToolSchema;

#[async_trait]
pub trait ToolHandler: Send + Sync {
    fn schema(&self) -> &ToolSchema;
    async fn invoke(&self, ctx: &ApiContext, args: Value, actor: Actor) -> ApiResult<Value>;
}

pub struct ToolRegistry {
    tools: Vec<Arc<dyn ToolHandler>>,
}

impl ToolRegistry {
    pub fn empty() -> Self { Self { tools: Vec::new() } }

    pub fn with_builtins() -> Self {
        let mut r = Self::empty();
        builtins::register_all(&mut r);
        r
    }

    pub fn add(&mut self, t: Arc<dyn ToolHandler>) { self.tools.push(t); }
    pub fn tools(&self) -> &[Arc<dyn ToolHandler>] { &self.tools }

    pub fn find(&self, name: &str) -> Option<Arc<dyn ToolHandler>> {
        self.tools.iter().find(|t| t.schema().name == name).cloned()
    }
}

/// Filter the registry by glob patterns on tool name.
/// Pattern syntax matches the `glob` crate's Pattern (e.g., "strategy.*").
pub fn filter_tools(reg: &ToolRegistry, patterns: &[String]) -> Vec<Arc<dyn ToolHandler>> {
    if patterns.iter().any(|p| p == "*") {
        return reg.tools().to_vec();
    }
    let compiled: Vec<glob::Pattern> = patterns.iter()
        .filter_map(|p| glob::Pattern::new(p).ok())
        .collect();
    reg.tools().iter()
        .filter(|t| compiled.iter().any(|c| c.matches(&t.schema().name)))
        .cloned()
        .collect()
}

/// Helper for builtins: a closure-backed ToolHandler.
pub struct FnHandler<F> {
    schema: ToolSchema,
    f: F,
}

impl<F> FnHandler<F> {
    pub fn new(schema: ToolSchema, f: F) -> Self { Self { schema, f } }
}

#[async_trait]
impl<F> ToolHandler for FnHandler<F>
where
    F: for<'a> Fn(&'a ApiContext, Value, Actor) -> futures::future::BoxFuture<'a, ApiResult<Value>>
        + Send + Sync,
{
    fn schema(&self) -> &ToolSchema { &self.schema }
    async fn invoke(&self, ctx: &ApiContext, args: Value, actor: Actor) -> ApiResult<Value> {
        (self.f)(ctx, args, actor).await
    }
}

pub fn require<'a, T: serde::de::DeserializeOwned>(args: &'a Value, key: &str) -> ApiResult<T> {
    let v = args.get(key).ok_or_else(|| ApiError::InvalidArgument(format!("missing key `{key}`")))?;
    serde_json::from_value(v.clone()).map_err(|e| ApiError::InvalidArgument(format!("bad `{key}`: {e}")))
}

pub fn optional<T: serde::de::DeserializeOwned>(args: &Value, key: &str) -> ApiResult<Option<T>> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => Ok(Some(serde_json::from_value(v.clone())
            .map_err(|e| ApiError::InvalidArgument(format!("bad `{key}`: {e}")))?)),
    }
}
```

- [ ] **Step 4: Implement builtins**

Create `crates/xianvec-engine/src/agent_runner/builtins.rs`:

```rust
//! Wire every engine API function as a ToolHandler. Each handler is a thin
//! shim around one API function: parse args, call the function, return JSON.

use std::sync::Arc;

use futures::FutureExt;
use serde_json::{json, Value};

use crate::agent_runner::registry::{require, optional, FnHandler, ToolHandler, ToolRegistry, ToolSchema};
use crate::api::{autoresearch, deploy, maintenance, report, risk, schedule, strategy, Actor, ApiResult};

fn schema(name: &str, desc: &str, input_schema: Value) -> ToolSchema {
    ToolSchema { name: name.to_string(), description: desc.to_string(), input_schema }
}

pub fn register_all(reg: &mut ToolRegistry) {
    register_strategy(reg);
    register_risk(reg);
    register_deploy(reg);
    register_report(reg);
    register_maintenance(reg);
    register_schedule(reg);
    register_autoresearch(reg);
    register_record_outcome(reg);
}

// ---- strategy ------------------------------------------------------------
fn register_strategy(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("strategy.list", "List strategies",
            json!({"type":"object","properties":{"include_archived":{"type":"boolean"}}})),
        |ctx, args, _actor| async move {
            let inc = optional::<bool>(&args, "include_archived")?.unwrap_or(false);
            let r = strategy::list(ctx, strategy::ListFilter { include_archived: inc, include_deleted: false, only: None }).await?;
            Ok(serde_json::to_value(r)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("strategy.show", "Show a strategy detail",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, _actor| async move {
            let id: String = require(&args, "id")?;
            let r = strategy::show(ctx, &id).await?;
            Ok(serde_json::to_value(r)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("strategy.deactivate", "Deactivate a strategy",
            json!({"type":"object","required":["id","reason"],
                   "properties":{"id":{"type":"string"},"reason":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            let reason: String = require(&args, "reason")?;
            strategy::deactivate(ctx, &id, &reason, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("strategy.reactivate", "Reactivate a strategy",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            strategy::reactivate(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("strategy.archive", "Archive a strategy",
            json!({"type":"object","required":["id","reason"],
                   "properties":{"id":{"type":"string"},"reason":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            let reason: String = require(&args, "reason")?;
            strategy::archive(ctx, &id, &reason, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("strategy.unarchive", "Unarchive a strategy",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            strategy::unarchive(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("strategy.delete", "Permanently delete a strategy (tombstone audit, remove bundle dir)",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            strategy::delete(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
}

// ---- risk ----------------------------------------------------------------
fn register_risk(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("risk.get", "Get risk state for a deployment",
            json!({"type":"object","required":["deployment_id"],"properties":{"deployment_id":{"type":"string"}}})),
        |ctx, args, _actor| async move {
            let id: String = require(&args, "deployment_id")?;
            Ok(serde_json::to_value(risk::get(ctx, &id).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("risk.set_capital", "Set capital_usd for a deployment",
            json!({"type":"object","required":["deployment_id","capital_usd","reason"],
                   "properties":{"deployment_id":{"type":"string"},"capital_usd":{"type":"number"},"reason":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            let usd: f64 = require(&args, "capital_usd")?;
            let reason: String = require(&args, "reason")?;
            risk::set_capital(ctx, &id, usd, &reason, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("risk.scale_capital", "Multiply capital_usd by `factor`",
            json!({"type":"object","required":["deployment_id","factor","reason"],
                   "properties":{"deployment_id":{"type":"string"},"factor":{"type":"number"},"reason":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            let factor: f64 = require(&args, "factor")?;
            let reason: String = require(&args, "reason")?;
            risk::scale_capital(ctx, &id, factor, &reason, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("risk.set_stop_loss", "Set ATR multiple for stop loss",
            json!({"type":"object","required":["deployment_id","atr_multiple","reason"],
                   "properties":{"deployment_id":{"type":"string"},"atr_multiple":{"type":"number"},"reason":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            let atr: f32 = require(&args, "atr_multiple")?;
            let reason: String = require(&args, "reason")?;
            risk::set_stop_loss(ctx, &id, atr, &reason, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("risk.set_position_size_pct", "Set position size as fraction of capital",
            json!({"type":"object","required":["deployment_id","pct","reason"],
                   "properties":{"deployment_id":{"type":"string"},"pct":{"type":"number"},"reason":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            let pct: f32 = require(&args, "pct")?;
            let reason: String = require(&args, "reason")?;
            risk::set_position_size_pct(ctx, &id, pct, &reason, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("risk.trip_circuit_breaker", "Trip circuit breaker (halts new orders)",
            json!({"type":"object","required":["deployment_id","reason"],
                   "properties":{"deployment_id":{"type":"string"},"reason":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            let reason: String = require(&args, "reason")?;
            risk::trip_circuit_breaker(ctx, &id, &reason, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("risk.reset_circuit_breaker", "Reset circuit breaker",
            json!({"type":"object","required":["deployment_id"],"properties":{"deployment_id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            risk::reset_circuit_breaker(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
}

// ---- deploy --------------------------------------------------------------
fn register_deploy(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("deploy.list", "List deployments",
            json!({"type":"object","properties":{"only_running":{"type":"boolean"}}})),
        |ctx, args, _actor| async move {
            let only = optional::<bool>(&args, "only_running")?.unwrap_or(false);
            Ok(serde_json::to_value(deploy::list(ctx, deploy::DepListFilter { only_running: only }).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("deploy.show", "Show a deployment detail",
            json!({"type":"object","required":["deployment_id"],"properties":{"deployment_id":{"type":"string"}}})),
        |ctx, args, _actor| async move {
            let id: String = require(&args, "deployment_id")?;
            Ok(serde_json::to_value(deploy::show(ctx, &id).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("deploy.start", "Start a deployment",
            json!({"type":"object","required":["deployment_id"],"properties":{"deployment_id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            deploy::start(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("deploy.stop", "Stop a deployment",
            json!({"type":"object","required":["deployment_id"],
                   "properties":{"deployment_id":{"type":"string"},
                                 "mode":{"type":"string","enum":["graceful","flatten","hard"]}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            let mode_s: String = optional(&args, "mode")?.unwrap_or_else(|| "graceful".into());
            let mode = match mode_s.as_str() {
                "flatten" => deploy::StopMode::Flatten,
                "hard" => deploy::StopMode::Hard,
                _ => deploy::StopMode::Graceful,
            };
            deploy::stop(ctx, &id, mode, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("deploy.flatten", "Flatten all positions for a deployment",
            json!({"type":"object","required":["deployment_id"],"properties":{"deployment_id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            Ok(serde_json::to_value(deploy::flatten(ctx, &id, actor).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("deploy.restart", "Restart a deployment",
            json!({"type":"object","required":["deployment_id"],"properties":{"deployment_id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            deploy::restart(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("deploy.switch_mode", "Switch broker mode (paper/live)",
            json!({"type":"object","required":["deployment_id","broker"],
                   "properties":{"deployment_id":{"type":"string"},"broker":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "deployment_id")?;
            let broker: String = require(&args, "broker")?;
            deploy::switch_mode(ctx, &id, &broker, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
}

// ---- report --------------------------------------------------------------
fn register_report(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("report.strategy_review", "Review all Active strategies",
            json!({"type":"object","properties":{"window_days":{"type":"integer"}}})),
        |ctx, args, _actor| async move {
            let window = optional::<u32>(&args, "window_days")?;
            Ok(serde_json::to_value(report::strategy_review(ctx,
                report::ReviewOpts { window_days: window, include_deactivated: false }).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("report.deployment_health", "Health summary for one or all deployments",
            json!({"type":"object","properties":{"deployment_id":{"type":"string"}}})),
        |ctx, args, _actor| async move {
            let id: Option<String> = optional(&args, "deployment_id")?;
            Ok(serde_json::to_value(report::deployment_health(ctx, id.as_deref()).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("report.anomaly_scan", "Run canned anomaly heuristics",
            json!({"type":"object","properties":{}})),
        |ctx, _args, _actor| async move {
            Ok(serde_json::to_value(report::anomaly_scan(ctx).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("report.eod", "End-of-day report; renders Markdown by default",
            json!({"type":"object","properties":{
                "deployments":{"type":"array","items":{"type":"string"}},
                "render_markdown":{"type":"boolean"}
            }})),
        |ctx, args, _actor| async move {
            let deps: Option<Vec<String>> = optional(&args, "deployments")?;
            let render: bool = optional::<bool>(&args, "render_markdown")?.unwrap_or(true);
            Ok(serde_json::to_value(report::eod(ctx,
                report::EodOpts { deployments: deps, baseline_arm: None, render_markdown: render }
            ).await?)?)
        }.boxed(),
    )));
}

// ---- maintenance ---------------------------------------------------------
fn register_maintenance(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("maintenance.rotate_logs", "Rotate logs older than N days",
            json!({"type":"object","required":["retain_days"],"properties":{"retain_days":{"type":"integer"}}})),
        |ctx, args, actor| async move {
            let n: u32 = require(&args, "retain_days")?;
            Ok(serde_json::to_value(maintenance::rotate_logs(ctx, n, actor).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("maintenance.compact_strategy_audit", "Drop strategy audit rows older than N days",
            json!({"type":"object","required":["retain_days"],"properties":{"retain_days":{"type":"integer"}}})),
        |ctx, args, actor| async move {
            let n: u32 = require(&args, "retain_days")?;
            Ok(serde_json::to_value(maintenance::compact_strategy_audit(ctx, n, actor).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("maintenance.vacuum_db", "Run SQLite VACUUM",
            json!({"type":"object","properties":{}})),
        |ctx, _args, actor| async move {
            maintenance::vacuum_db(ctx, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("maintenance.integrity_check", "Find orphaned audit rows / orphaned bundle dirs",
            json!({"type":"object","properties":{}})),
        |ctx, _args, actor| async move {
            Ok(serde_json::to_value(maintenance::integrity_check(ctx, actor).await?)?)
        }.boxed(),
    )));
}

// ---- schedule ------------------------------------------------------------
fn register_schedule(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("schedule.list", "List schedules",
            json!({"type":"object","properties":{}})),
        |ctx, _args, _actor| async move {
            Ok(serde_json::to_value(schedule::list(ctx, schedule::ScheduleFilter::default()).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("schedule.show", "Show schedule detail",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, _actor| async move {
            let id: String = require(&args, "id")?;
            Ok(serde_json::to_value(schedule::show(ctx, &id).await?)?)
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("schedule.pause", "Pause a schedule",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            schedule::pause(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("schedule.resume", "Resume a schedule",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            schedule::resume(ctx, &id, actor).await?;
            Ok(json!({"ok":true}))
        }.boxed(),
    )));
    reg.add(Arc::new(FnHandler::new(
        schema("schedule.run_now", "Trigger a manual fire",
            json!({"type":"object","required":["id"],"properties":{"id":{"type":"string"}}})),
        |ctx, args, actor| async move {
            let id: String = require(&args, "id")?;
            let fire = schedule::run_now(ctx, &id, actor).await?;
            Ok(json!({"fire_id": fire}))
        }.boxed(),
    )));
}

// ---- autoresearch --------------------------------------------------------
fn register_autoresearch(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("autoresearch.run_evening_cycle", "Run AR-2 evening cycle",
            json!({"type":"object","properties":{
                "strategy_id":{"type":"string"},"dry_run":{"type":"boolean"}
            }})),
        |ctx, args, _actor| async move {
            let sid: Option<String> = optional(&args, "strategy_id")?;
            let dry: bool = optional::<bool>(&args, "dry_run")?.unwrap_or(false);
            Ok(serde_json::to_value(autoresearch::run_evening_cycle(ctx,
                autoresearch::EveningCycleOpts { strategy_id: sid, dry_run: dry }).await?)?)
        }.boxed(),
    )));
}

// ---- record_outcome (mandatory final tool) ------------------------------
fn register_record_outcome(reg: &mut ToolRegistry) {
    reg.add(Arc::new(FnHandler::new(
        schema("record_outcome",
            "Required final tool call. Records the run's outcome. Must be called once at the end.",
            json!({"type":"object","required":["summary"],"properties":{
                "summary":{"type":"string"},
                "actions_taken":{"type":"array"},
                "anomalies":{"type":"array"}
            }})),
        |_ctx, args, _actor| async move {
            // Echo args back; the runner observes the call via the assistant turn,
            // not via the result. The handler only needs to succeed.
            Ok(args)
        }.boxed(),
    )));
}
```

- [ ] **Step 5: Wire into engine `lib.rs`**

Add to `crates/xianvec-engine/src/lib.rs`:

```rust
pub mod agent_runner;
```

- [ ] **Step 6: Add `futures` to engine deps**

In `crates/xianvec-engine/Cargo.toml`:

```toml
futures = "0.3"
glob    = "0.3"
xianvec-intern = { path = "../xianvec-intern" }
```

- [ ] **Step 7: Run — expect pass**

```bash
cargo test -p xianvec-engine --test tool_registry
```

Expected: 4 passed.

- [ ] **Step 8: Commit**

```bash
git add crates/xianvec-engine/src/agent_runner \
        crates/xianvec-engine/src/lib.rs \
        crates/xianvec-engine/Cargo.toml \
        crates/xianvec-engine/tests/tool_registry.rs
git commit -m "feat(engine/agent_runner): ToolRegistry + glob filtering + 30+ builtin handlers"
```

---

### Task 12: AgentRunner core loop + transcript

**Files:**
- Create: `crates/xianvec-engine/src/agent_runner/loop_.rs`
- Create: `crates/xianvec-engine/src/agent_runner/transcript.rs`
- Create: `crates/xianvec-engine/src/agent_runner/pricing.rs`
- Create: `crates/xianvec-engine/src/agent_runner/budget.rs`
- Modify: `crates/xianvec-engine/src/agent_runner/mod.rs`
- Create: `crates/xianvec-engine/tests/agent_runner.rs`

- [ ] **Step 1: Failing test against mock dispatch**

Create `crates/xianvec-engine/tests/agent_runner.rs`:

```rust
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use sqlx::SqlitePool;
use tempfile::TempDir;
use xianvec_engine::agent_runner::{registry::ToolRegistry, AgentRunner, FireStatus, RunRequest};
use xianvec_engine::api::{strategy, Actor, ApiContext};
use xianvec_intern::tool_dispatch::{
    AssistantTurn, LlmToolDispatch, Message, ToolCall, ToolDispatchRequest,
};

struct ScriptedDispatch { turns: Mutex<Vec<AssistantTurn>> }

#[async_trait]
impl LlmToolDispatch for ScriptedDispatch {
    async fn run_turn(&self, _req: ToolDispatchRequest) -> anyhow::Result<AssistantTurn> {
        let mut g = self.turns.lock().unwrap();
        if g.is_empty() { anyhow::bail!("no more turns"); }
        Ok(g.remove(0))
    }
}

async fn fixture() -> (Arc<ApiContext>, TempDir) {
    let dir = TempDir::new().unwrap();
    let db = SqlitePool::connect("sqlite::memory:").await.unwrap();
    sqlx::migrate!("./migrations").run(&db).await.unwrap();
    std::fs::create_dir_all(dir.path().join("strategies/sh_t")).unwrap();
    std::fs::write(dir.path().join("strategies/sh_t/manifest.toml"), b"x").unwrap();
    let ctx = Arc::new(ApiContext::new(dir.path().to_path_buf(), db).with_clock(Arc::new(|| {
        Utc.with_ymd_and_hms(2026, 5, 10, 12, 0, 0).unwrap()
    })));
    strategy::record_created(&ctx, "sh_t", Actor::Cli).await.unwrap();
    (ctx, dir)
}

#[tokio::test]
async fn agent_calls_strategy_show_then_record_outcome() {
    let (ctx, _dir) = fixture().await;
    let dispatch = Arc::new(ScriptedDispatch { turns: Mutex::new(vec![
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall {
                tool_call_id: "call_1".into(),
                name: "strategy.show".into(),
                arguments: serde_json::json!({"id":"sh_t"}),
            }],
            stop_reason: "tool_use".into(),
            tokens_in: 100, tokens_out: 50, cache_read_tokens: 0, cache_write_tokens: 0,
        },
        AssistantTurn {
            text: Some("done".into()),
            tool_calls: vec![ToolCall {
                tool_call_id: "call_2".into(),
                name: "record_outcome".into(),
                arguments: serde_json::json!({
                    "summary":"reviewed sh_t", "actions_taken":[], "anomalies":[]
                }),
            }],
            stop_reason: "tool_use".into(),
            tokens_in: 50, tokens_out: 30, cache_read_tokens: 0, cache_write_tokens: 0,
        },
    ]) });
    let runner = AgentRunner {
        dispatch,
        registry: Arc::new(ToolRegistry::with_builtins()),
        ctx: ctx.clone(),
    };
    let outcome = runner.run(RunRequest {
        fire_id: "fire_test".into(),
        prompt: "review sh_t and end".into(),
        allowed_tools: vec!["strategy.show".into(), "record_outcome".into()],
        model: "claude-opus-4-7".into(),
        max_tokens: 10_000,
        max_cost_usd: 1.00,
        timeout_seconds: 60,
        context_seed: None,
        actor: Actor::Schedule { schedule_id: "sch_test".into(), fire_id: "fire_test".into() },
    }).await;
    assert_eq!(outcome.status, FireStatus::Ok);
    assert_eq!(outcome.summary.as_deref(), Some("reviewed sh_t"));
    assert!(outcome.tokens_in > 0);
    assert!(outcome.transcript_path.is_some());
}

#[tokio::test]
async fn budget_enforced_when_max_tokens_exceeded() {
    let (ctx, _dir) = fixture().await;
    let dispatch = Arc::new(ScriptedDispatch { turns: Mutex::new(vec![
        AssistantTurn {
            text: None,
            tool_calls: vec![ToolCall { tool_call_id: "x".into(), name: "strategy.show".into(),
                arguments: serde_json::json!({"id":"sh_t"}) }],
            stop_reason: "tool_use".into(),
            tokens_in: 100, tokens_out: 100, cache_read_tokens: 0, cache_write_tokens: 0,
        },
    ]) });
    let runner = AgentRunner {
        dispatch,
        registry: Arc::new(ToolRegistry::with_builtins()),
        ctx: ctx.clone(),
    };
    let outcome = runner.run(RunRequest {
        fire_id: "fire_b".into(),
        prompt: "x".into(),
        allowed_tools: vec!["*".into()],
        model: "claude-opus-4-7".into(),
        max_tokens: 50,                  // tiny — exceeded after first turn
        max_cost_usd: 100.0,
        timeout_seconds: 60,
        context_seed: None,
        actor: Actor::Cli,
    }).await;
    assert_eq!(outcome.status, FireStatus::BudgetExceeded);
}
```

- [ ] **Step 2: Run — expect failure**

- [ ] **Step 3: Implement pricing**

Create `crates/xianvec-engine/src/agent_runner/pricing.rs`:

```rust
//! Per-model token pricing in USD per 1M tokens. Cache reads charged at the
//! lower rate; cache writes charged at write rate.

#[derive(Debug, Clone, Copy)]
pub struct ModelPricing {
    pub in_per_mtok:           f64,
    pub out_per_mtok:          f64,
    pub cache_read_per_mtok:   f64,
    pub cache_write_per_mtok:  f64,
}

pub fn lookup(model: &str) -> Option<ModelPricing> {
    match model {
        "claude-opus-4-7"   => Some(ModelPricing { in_per_mtok: 15.0,  out_per_mtok: 75.0,  cache_read_per_mtok: 1.5,   cache_write_per_mtok: 18.75 }),
        "claude-sonnet-4-6" => Some(ModelPricing { in_per_mtok:  3.0,  out_per_mtok: 15.0,  cache_read_per_mtok: 0.30,  cache_write_per_mtok:  3.75 }),
        "claude-haiku-4-5-20251001" => Some(ModelPricing { in_per_mtok: 1.0, out_per_mtok: 5.0, cache_read_per_mtok: 0.10, cache_write_per_mtok: 1.25 }),
        _ => None,
    }
}

pub fn turn_cost(p: &ModelPricing, tokens_in: u32, tokens_out: u32, cache_read: u32, cache_write: u32) -> f64 {
    let m = 1_000_000.0;
    (tokens_in as f64) * p.in_per_mtok / m
        + (tokens_out as f64) * p.out_per_mtok / m
        + (cache_read as f64) * p.cache_read_per_mtok / m
        + (cache_write as f64) * p.cache_write_per_mtok / m
}
```

- [ ] **Step 4: Implement budget**

Create `crates/xianvec-engine/src/agent_runner/budget.rs`:

```rust
use crate::agent_runner::pricing::{lookup, turn_cost, ModelPricing};

#[derive(Debug, Clone)]
pub struct BudgetTracker {
    pub model: String,
    pub max_tokens: u32,
    pub max_cost_usd: f64,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f64,
    pub warned_80: bool,
    pricing: Option<ModelPricing>,
}

impl BudgetTracker {
    pub fn new(model: String, max_tokens: u32, max_cost_usd: f64) -> Self {
        let pricing = lookup(&model);
        if pricing.is_none() {
            tracing::warn!(model = %model, "no pricing info; cost-cap disabled, token-cap only");
        }
        Self { model, max_tokens, max_cost_usd, tokens_in: 0, tokens_out: 0, cost_usd: 0.0,
               warned_80: false, pricing }
    }

    pub fn record(&mut self, tin: u32, tout: u32, cread: u32, cwrite: u32) {
        self.tokens_in += tin;
        self.tokens_out += tout;
        if let Some(p) = self.pricing {
            self.cost_usd += turn_cost(&p, tin, tout, cread, cwrite);
        }
    }

    pub fn at_or_over_warning(&mut self) -> bool {
        if self.warned_80 { return false; }
        let tok_used = self.tokens_in + self.tokens_out;
        let tok_pct = tok_used as f64 / self.max_tokens.max(1) as f64;
        let cost_pct = if self.pricing.is_some() { self.cost_usd / self.max_cost_usd.max(0.001) } else { 0.0 };
        if tok_pct >= 0.8 || cost_pct >= 0.8 {
            self.warned_80 = true;
            return true;
        }
        false
    }

    pub fn exceeded(&self) -> bool {
        let tok_used = self.tokens_in + self.tokens_out;
        if tok_used >= self.max_tokens { return true; }
        if self.pricing.is_some() && self.cost_usd >= self.max_cost_usd { return true; }
        false
    }
}
```

- [ ] **Step 5: Implement transcript**

Create `crates/xianvec-engine/src/agent_runner/transcript.rs`:

```rust
use std::io::Write;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::api::{ApiContext, ApiResult};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranscriptEntry {
    pub at: chrono::DateTime<chrono::Utc>,
    pub kind: String,            // "system" | "user" | "assistant" | "tool_call" | "tool_result"
    pub payload: serde_json::Value,
}

pub struct TranscriptWriter {
    path: PathBuf,
    file: std::fs::File,
}

impl TranscriptWriter {
    pub fn open(ctx: &ApiContext, fire_id: &str) -> ApiResult<Self> {
        let dir = ctx.xvn_home.join("schedule_transcripts");
        std::fs::create_dir_all(&dir)?;
        let path = dir.join(format!("{fire_id}.jsonl"));
        let file = std::fs::OpenOptions::new()
            .create(true).append(true).open(&path)?;
        Ok(Self { path, file })
    }

    pub fn append(&mut self, kind: &str, payload: serde_json::Value) -> ApiResult<()> {
        let entry = TranscriptEntry { at: chrono::Utc::now(), kind: kind.to_string(), payload };
        let line = serde_json::to_string(&entry)?;
        writeln!(self.file, "{line}")?;
        self.file.flush()?;
        Ok(())
    }

    pub fn path(&self) -> &std::path::Path { &self.path }
}
```

- [ ] **Step 6: Implement loop_ + AgentRunner**

Create `crates/xianvec-engine/src/agent_runner/loop_.rs`:

```rust
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::time::Duration;

use xianvec_intern::tool_dispatch::{
    AssistantTurn, LlmToolDispatch, Message, ToolCall, ToolDispatchRequest,
};

use crate::agent_runner::budget::BudgetTracker;
use crate::agent_runner::registry::{filter_tools, ToolHandler, ToolRegistry};
use crate::agent_runner::transcript::TranscriptWriter;
use crate::api::{Actor, ApiContext};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FireStatus {
    Ok,
    Failed,
    Timeout,
    BudgetExceeded,
    Crashed,
    Cancelled,
}

#[derive(Debug, Clone)]
pub struct RunRequest {
    pub fire_id: String,
    pub prompt: String,
    pub allowed_tools: Vec<String>,
    pub model: String,
    pub max_tokens: u32,
    pub max_cost_usd: f64,
    pub timeout_seconds: u32,
    pub context_seed: Option<serde_json::Value>,
    pub actor: Actor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionRecord { pub tool: String, pub args: serde_json::Value, pub result: serde_json::Value }

#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub fire_id: String,
    pub status: FireStatus,
    pub summary: Option<String>,
    pub actions_taken: Vec<ActionRecord>,
    pub anomalies: Vec<String>,
    pub tokens_in: u32,
    pub tokens_out: u32,
    pub cost_usd: f64,
    pub elapsed_ms: u64,
    pub transcript_path: Option<String>,
}

pub struct AgentRunner {
    pub dispatch: Arc<dyn LlmToolDispatch>,
    pub registry: Arc<ToolRegistry>,
    pub ctx: Arc<ApiContext>,
}

impl AgentRunner {
    pub async fn run(&self, req: RunRequest) -> RunOutcome {
        let started = Instant::now();
        let timeout = Duration::from_secs(req.timeout_seconds as u64);
        let mut transcript = match TranscriptWriter::open(&self.ctx, &req.fire_id) {
            Ok(t) => t,
            Err(e) => return failure_outcome(req.fire_id.clone(), format!("transcript open: {e}"), started),
        };

        // Filter tools by allowlist.
        let allowed = filter_tools(&self.registry, &req.allowed_tools);
        let tool_schemas: Vec<_> = allowed.iter().map(|t| t.schema().clone()).collect();
        let by_name: std::collections::HashMap<String, Arc<dyn ToolHandler>> =
            allowed.iter().map(|t| (t.schema().name.clone(), t.clone())).collect();

        let system = build_system_prompt(&tool_schemas);
        let _ = transcript.append("system", json!({"text": &system}));

        let mut messages: Vec<Message> = Vec::new();
        let mut user = req.prompt.clone();
        if let Some(ctx_seed) = &req.context_seed {
            user.push_str("\n\nContext (JSON): ");
            user.push_str(&ctx_seed.to_string());
        }
        messages.push(Message::user(user.clone()));
        let _ = transcript.append("user", json!({"text": user}));

        let mut budget = BudgetTracker::new(req.model.clone(), req.max_tokens, req.max_cost_usd);
        let mut actions: Vec<ActionRecord> = Vec::new();
        let mut summary: Option<String> = None;
        let mut anomalies: Vec<String> = Vec::new();
        let mut got_record_outcome = false;
        let mut prompted_for_outcome = false;

        loop {
            if started.elapsed() >= timeout {
                return finish(req.fire_id, FireStatus::Timeout, summary, actions, anomalies, budget, started, Some(transcript.path().to_path_buf()));
            }
            if budget.exceeded() {
                return finish(req.fire_id, FireStatus::BudgetExceeded, summary, actions, anomalies, budget, started, Some(transcript.path().to_path_buf()));
            }
            let turn_req = ToolDispatchRequest {
                model: req.model.clone(),
                system: system.clone(),
                messages: messages.clone(),
                tools: tool_schemas.clone(),
                max_tokens: 4096.min(req.max_tokens.saturating_sub(budget.tokens_in + budget.tokens_out)),
                temperature: 0.0,
            };
            let turn = match self.dispatch.run_turn(turn_req).await {
                Ok(t) => t,
                Err(e) => {
                    let _ = transcript.append("assistant_error", json!({"error": e.to_string()}));
                    return finish(req.fire_id, FireStatus::Failed, summary, actions, anomalies, budget, started, Some(transcript.path().to_path_buf()));
                }
            };
            budget.record(turn.tokens_in, turn.tokens_out, turn.cache_read_tokens, turn.cache_write_tokens);
            let _ = transcript.append("assistant", serde_json::to_value(&turn).unwrap_or(json!({})));

            messages.push(Message::Assistant {
                content: turn.text.clone(),
                tool_calls: turn.tool_calls.clone(),
            });

            if turn.tool_calls.is_empty() {
                if !got_record_outcome && !prompted_for_outcome {
                    prompted_for_outcome = true;
                    messages.push(Message::user("Please call record_outcome before finishing."));
                    let _ = transcript.append("user", json!({"text":"Please call record_outcome before finishing."}));
                    continue;
                }
                return finish(req.fire_id, FireStatus::Ok, summary, actions, anomalies, budget, started, Some(transcript.path().to_path_buf()));
            }

            for call in turn.tool_calls {
                if call.name == "record_outcome" {
                    summary = call.arguments.get("summary").and_then(|v| v.as_str()).map(|s| s.to_string());
                    if let Some(arr) = call.arguments.get("anomalies").and_then(|v| v.as_array()) {
                        anomalies = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                    }
                    got_record_outcome = true;
                    let _ = transcript.append("tool_call", json!({"name": "record_outcome", "args": call.arguments}));
                    messages.push(Message::ToolResult {
                        tool_call_id: call.tool_call_id, is_error: false,
                        content: call.arguments,
                    });
                    continue;
                }
                let handler = match by_name.get(&call.name) {
                    Some(h) => h.clone(),
                    None => {
                        let err_payload = json!({"error":"not_allowed","tool":&call.name});
                        let _ = transcript.append("tool_result", json!({"tool_call_id": &call.tool_call_id, "is_error": true, "content": &err_payload}));
                        messages.push(Message::ToolResult { tool_call_id: call.tool_call_id, is_error: true, content: err_payload });
                        continue;
                    }
                };
                let _ = transcript.append("tool_call", json!({"name": &call.name, "args": &call.arguments}));
                let result = handler.invoke(&self.ctx, call.arguments.clone(), req.actor.clone()).await;
                match result {
                    Ok(v) => {
                        actions.push(ActionRecord { tool: call.name.clone(), args: call.arguments, result: v.clone() });
                        let _ = transcript.append("tool_result", json!({"tool_call_id": &call.tool_call_id, "is_error": false, "content": &v}));
                        messages.push(Message::ToolResult { tool_call_id: call.tool_call_id, is_error: false, content: v });
                    }
                    Err(e) => {
                        let err_payload = json!({"error": e.to_string()});
                        let _ = transcript.append("tool_result", json!({"tool_call_id": &call.tool_call_id, "is_error": true, "content": &err_payload}));
                        messages.push(Message::ToolResult { tool_call_id: call.tool_call_id, is_error: true, content: err_payload });
                    }
                }
            }

            if got_record_outcome {
                return finish(req.fire_id, FireStatus::Ok, summary, actions, anomalies, budget, started, Some(transcript.path().to_path_buf()));
            }
            if budget.at_or_over_warning() {
                messages.push(Message::system("budget_warning: 80% used"));
                let _ = transcript.append("system", json!({"text":"budget_warning: 80% used"}));
            }
        }
    }
}

fn build_system_prompt(tools: &[xianvec_intern::tool_dispatch::ToolSchema]) -> String {
    let mut s = String::new();
    s.push_str("You are an xvn agent invoked by a scheduled job. Follow the user's prompt.\n");
    s.push_str("Use tools to inspect and act on xvn state. Never invent data — if a tool fails, report the failure in record_outcome.\n");
    s.push_str("You MUST end every run with `record_outcome({summary, actions_taken, anomalies})` before finishing. ");
    s.push_str("If you cannot complete the task, still call record_outcome with an explanatory summary.\n\n");
    s.push_str("Tools available:\n");
    for t in tools {
        s.push_str(&format!("- {}: {}\n", t.name, t.description));
    }
    s
}

fn failure_outcome(fire_id: String, msg: String, started: Instant) -> RunOutcome {
    RunOutcome {
        fire_id, status: FireStatus::Failed,
        summary: Some(msg), actions_taken: vec![], anomalies: vec![],
        tokens_in: 0, tokens_out: 0, cost_usd: 0.0,
        elapsed_ms: started.elapsed().as_millis() as u64,
        transcript_path: None,
    }
}

fn finish(
    fire_id: String, status: FireStatus, summary: Option<String>,
    actions: Vec<ActionRecord>, anomalies: Vec<String>,
    budget: BudgetTracker, started: Instant, transcript_path: Option<std::path::PathBuf>,
) -> RunOutcome {
    RunOutcome {
        fire_id, status, summary, actions_taken: actions, anomalies,
        tokens_in: budget.tokens_in, tokens_out: budget.tokens_out, cost_usd: budget.cost_usd,
        elapsed_ms: started.elapsed().as_millis() as u64,
        transcript_path: transcript_path.map(|p| p.to_string_lossy().to_string()),
    }
}
```

- [ ] **Step 7: Update `agent_runner/mod.rs`**

Replace contents:

```rust
pub mod budget;
pub mod builtins;
pub mod loop_;
pub mod pricing;
pub mod registry;
pub mod transcript;

pub use loop_::{ActionRecord, AgentRunner, FireStatus, RunOutcome, RunRequest};
```

- [ ] **Step 8: Run — expect pass**

```bash
cargo test -p xianvec-engine --test agent_runner
```

Expected: 2 passed.

- [ ] **Step 9: Commit**

```bash
git add crates/xianvec-engine/src/agent_runner \
        crates/xianvec-engine/tests/agent_runner.rs
git commit -m "feat(engine/agent_runner): tool-use loop, transcript JSONL, budget enforcement"
```

---

### Task 13: `xvn agent ask` CLI — interactive single-shot agent

**Files:**
- Create: `crates/xianvec-cli/src/commands/agent.rs`
- Modify: `crates/xianvec-cli/src/commands/mod.rs`
- Modify: `crates/xianvec-cli/src/lib.rs`

> **Context.** `xvn agent run` (the long-lived daemon) lands in Task 19. `xvn agent ask "<prompt>"` ships now as a one-shot agent invocation against the engine — useful for testing the runner without the scheduler. Until a real `LlmToolDispatch` is wired, `xvn agent ask` requires a `--mock` flag that uses an env-var-driven scripted dispatch (one assistant turn per `XVN_MOCK_TURN_<N>` env var, or a single turn from `XVN_MOCK_TURN`).

- [ ] **Step 1: Implement command**

Create `crates/xianvec-cli/src/commands/agent.rs`:

```rust
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use clap::{Args, Subcommand};
use sqlx::SqlitePool;
use xianvec_engine::agent_runner::{registry::ToolRegistry, AgentRunner, RunRequest};
use xianvec_engine::api::{Actor, ApiContext};
use xianvec_intern::tool_dispatch::{AssistantTurn, LlmToolDispatch, ToolDispatchRequest};

#[derive(Args, Debug)]
pub struct AgentCmd {
    #[command(subcommand)]
    pub action: AgentAction,
}

#[derive(Subcommand, Debug)]
pub enum AgentAction {
    /// Run a single ad-hoc agent invocation against the engine (for testing).
    Ask {
        prompt: String,
        #[arg(long, default_value = "*")]
        allow: String,
        #[arg(long, default_value = "claude-opus-4-7")]
        model: String,
        #[arg(long, default_value_t = 50_000)]
        max_tokens: u32,
        #[arg(long, default_value_t = 1.0)]
        max_cost_usd: f64,
        #[arg(long, default_value_t = 600)]
        timeout_seconds: u32,
        /// Use the mock dispatch (reads XVN_MOCK_TURN env var). Required until a real LLM dispatch is wired.
        #[arg(long)]
        mock: bool,
    },
}

struct EnvMockDispatch { turns: Mutex<Vec<AssistantTurn>> }

#[async_trait]
impl LlmToolDispatch for EnvMockDispatch {
    async fn run_turn(&self, _req: ToolDispatchRequest) -> anyhow::Result<AssistantTurn> {
        let mut g = self.turns.lock().unwrap();
        if g.is_empty() { anyhow::bail!("no more mock turns"); }
        Ok(g.remove(0))
    }
}

fn load_mock_turns_from_env() -> anyhow::Result<Vec<AssistantTurn>> {
    if let Ok(s) = std::env::var("XVN_MOCK_TURN") {
        let t: AssistantTurn = serde_json::from_str(&s)?;
        return Ok(vec![t]);
    }
    let mut out = Vec::new();
    for i in 0..32 {
        if let Ok(s) = std::env::var(format!("XVN_MOCK_TURN_{i}")) {
            out.push(serde_json::from_str::<AssistantTurn>(&s)?);
        }
    }
    if out.is_empty() {
        anyhow::bail!("set XVN_MOCK_TURN or XVN_MOCK_TURN_<N> env vars (JSON-encoded AssistantTurn)");
    }
    Ok(out)
}

pub async fn run(cmd: AgentCmd) -> anyhow::Result<()> {
    match cmd.action {
        AgentAction::Ask { prompt, allow, model, max_tokens, max_cost_usd, timeout_seconds, mock } => {
            let xvn_home = std::env::var("XVN_HOME").map(std::path::PathBuf::from)
                .unwrap_or_else(|_| dirs::home_dir().unwrap().join(".xvn"));
            let db_path = xvn_home.join("xvn.db");
            std::fs::create_dir_all(&xvn_home)?;
            let url = format!("sqlite://{}?mode=rwc", db_path.display());
            let db = SqlitePool::connect(&url).await?;
            sqlx::migrate!("../xianvec-engine/migrations").run(&db).await?;
            let ctx = Arc::new(ApiContext::new(xvn_home, db));

            if !mock {
                anyhow::bail!("`--mock` is required until a real LLM dispatch lands. Pass --mock and set XVN_MOCK_TURN env var.");
            }
            let turns = load_mock_turns_from_env()?;
            let dispatch = Arc::new(EnvMockDispatch { turns: Mutex::new(turns) });
            let runner = AgentRunner {
                dispatch,
                registry: Arc::new(ToolRegistry::with_builtins()),
                ctx: ctx.clone(),
            };
            let allowed: Vec<String> = allow.split(',').map(|s| s.trim().to_string()).collect();
            let outcome = runner.run(RunRequest {
                fire_id: format!("ask_{}", ulid::Ulid::new()),
                prompt, allowed_tools: allowed,
                model, max_tokens, max_cost_usd, timeout_seconds,
                context_seed: None,
                actor: Actor::Cli,
            }).await;
            println!("{}", serde_json::to_string_pretty(&serde_json::json!({
                "status": outcome.status,
                "summary": outcome.summary,
                "actions_taken": outcome.actions_taken.iter().map(|a| &a.tool).collect::<Vec<_>>(),
                "tokens_in": outcome.tokens_in,
                "tokens_out": outcome.tokens_out,
                "cost_usd": outcome.cost_usd,
                "elapsed_ms": outcome.elapsed_ms,
                "transcript_path": outcome.transcript_path,
            }))?);
            Ok(())
        }
    }
}
```

- [ ] **Step 2: Wire into top-level CLI**

In `crates/xianvec-cli/src/commands/mod.rs`:

```rust
pub mod agent;
```

In `crates/xianvec-cli/src/lib.rs` (the `Command` enum and `Cli::run` dispatch — the exact location depends on existing layout; locate the enum that lists `Strategy`, `Live`, etc., and add):

```rust
Agent(commands::agent::AgentCmd),
```

And in the run dispatch:

```rust
Command::Agent(cmd) => commands::agent::run(cmd).await?,
```

- [ ] **Step 3: Smoke test manually**

```bash
export XVN_HOME=/tmp/xvn-agent-smoke
export XVN_MOCK_TURN='{"text":null,"tool_calls":[{"tool_call_id":"x","name":"record_outcome","arguments":{"summary":"hi","actions_taken":[],"anomalies":[]}}],"stop_reason":"tool_use","tokens_in":10,"tokens_out":5,"cache_read_tokens":0,"cache_write_tokens":0}'
cargo run -p xianvec-cli -- agent ask "hi" --mock
```

Expected: JSON output with `"status":"ok"` and `"summary":"hi"`.

- [ ] **Step 4: Commit**

```bash
git add crates/xianvec-cli/src/commands/agent.rs \
        crates/xianvec-cli/src/commands/mod.rs \
        crates/xianvec-cli/src/lib.rs
git commit -m "feat(cli): xvn agent ask — single-shot agent invocation (mock dispatch)"
```

---

> **End of Part 3.** Phase B (tool registry + agent runner) complete. Part 4 covers Phase C: durable scheduler (Tasks 14–19).

