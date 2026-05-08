# Strategy Creation Engine — Plan 2a (MCP + Tool-Call Dispatch + 7 Templates) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan #1 (`docs/superpowers/plans/2026-05-08-strategy-creation-engine-mvp.md`) merged. PR: https://github.com/latentwill/xianvec/pull/3.
> **Execution-order decision (2026-05-08):** Execute this plan **after Plan 3 (eval engine) ships**, in case eval surfaces design decisions affecting MCP authoring verbs, the 7 templates, or the tool-call dispatch shape. Eval's findings extractor (Plan 3 Task 8) uses inline OSShip-style markdown prompts; that prompt-loading pattern may inform how skills (Plan 2b) and tool-use loops (this plan) should be structured. The plan's *technical* deps remain Plan #1 only — only the execution timing is deferred.

**Goal:** Make xvn fully driveable by an external AI agent (Claude Code, Hermes, Cursor) over MCP — and make the agent loop actually use tools during decisions instead of operating on stub inputs. After this plan ships, an external agent can connect to xvn via `xvn agent serve --mcp`, list templates, create + customize a strategy bundle, and run it inline against a fixture where the LLM *itself* requests OHLCV/indicator data via tool-use mid-decision. All 7 remaining v1 templates are registered (trend_follower, breakout, momentum, range_trade, scalping, news_trader, custom).

**Architecture:** Three pieces sit on top of Plan #1's foundation. (1) An MCP server in `xianvec-engine/src/mcp/` exposes the authoring verb group as MCP tools over stdio JSON-RPC. (2) `LlmDispatch` and `execute_slot` extend to support Anthropic-style tool-use loops — request includes tool definitions; LLM emits tool_use blocks; runtime routes to `ToolRegistry.invoke`; result loops back into the conversation until the LLM emits a final content-only response. (3) Seven new template files register through the existing `Template` trait + registry pattern.

**Tech Stack:** Rust 2021. New deps: `rmcp` (model context protocol Rust impl) for MCP server. Reuses everything from Plan #1: anyhow, serde, serde_json, tokio, async-trait, reqwest. Tests use the existing `MockDispatch` extended with tool-use-aware variants.

**Out of scope (deferred to Plans 2b / 2c / 2d / 3):**
- MCP verbs for skill management, eval lifecycle, marketplace + live — those land in their own plans
- Tier B sealing + xvn API server (Plan 2b)
- Durable scheduler port from SwarmClaw (Plan 2c)
- Live execution daemon (Plan 2c)
- Web dashboard / Agent Wizard (Plan 2d)
- Eval engine (Plan 3)
- News/sentiment API integration — `news_trader` template ships with stub prompts; real news-tool dispatch lands in Plan 2c alongside other external-signal tools

---

## File structure

```
crates/xianvec-engine/
├── Cargo.toml                              # add rmcp + jsonschema deps
├── src/
│   ├── lib.rs                              # add `pub mod mcp;`
│   ├── agent/
│   │   ├── llm.rs                          # extend LlmRequest/Response with tools + tool_use
│   │   ├── execute.rs                      # extend execute_slot with tool-use loop
│   │   └── tool_call.rs                    # NEW: tool-call routing helpers
│   ├── mcp/                                # NEW
│   │   ├── mod.rs                          # MCP server entry, tool registration
│   │   ├── authoring.rs                    # the 7 authoring verbs as MCP tools
│   │   └── schema.rs                       # JSON schemas for verb args
│   └── templates/
│       ├── mod.rs                          # add 7 module declarations
│       ├── trend_follower.rs               # NEW
│       ├── breakout.rs                     # NEW
│       ├── momentum.rs                     # NEW
│       ├── range_trade.rs                  # NEW
│       ├── scalping.rs                     # NEW
│       ├── news_trader.rs                  # NEW
│       ├── custom.rs                       # NEW
│       └── registry.rs                     # register all 7 + ma_crossover_baseline
├── tests/
│   ├── mcp_authoring.rs                    # NEW: MCP smoke + authoring round-trip
│   ├── tool_call_loop.rs                   # NEW: tool-use loop tests with mock + real tools
│   └── seven_templates.rs                  # NEW: each template validates + roundtrips
```

Plus modifications:
- `crates/xianvec-cli/src/lib.rs` — add `Agent { #[command(subcommand)] action: AgentAction }` top-level command
- `crates/xianvec-cli/src/commands/agent.rs` — new module: `xvn agent serve --mcp` subcommand
- `crates/xianvec-cli/src/commands/strategy.rs` — update `run_inline` to populate seed_inputs with real OHLCV + indicator data fetched via the tool registry, so the trader slot sees actual data

---

## Phase 2A.A — MCP server skeleton

### Task 1: Add `rmcp` dep + `mcp` module skeleton

**Files:**
- Modify: `crates/xianvec-engine/Cargo.toml`
- Create: `crates/xianvec-engine/src/mcp/mod.rs`
- Modify: `crates/xianvec-engine/src/lib.rs`

- [ ] **Step 1: Add deps**

In `crates/xianvec-engine/Cargo.toml` `[dependencies]`, add:

```toml
rmcp        = { version = "0.2", features = ["server", "transport-io"] }
jsonschema  = "0.18"
```

Run: `cargo search rmcp --limit 3` to confirm the latest stable. If `rmcp` isn't on crates.io under that name in your snapshot, the alternatives (in order of preference) are: `mcp-server`, or a thin hand-rolled stdio JSON-RPC implementation. Document the choice in a one-line comment.

- [ ] **Step 2: Create `mcp/mod.rs` skeleton**

```rust
//! MCP server surface — exposes xianvec-engine authoring verbs as MCP tools.
//!
//! Wire format: stdio JSON-RPC per the Model Context Protocol spec.
//! Verbs implemented in this plan: list_templates, create_strategy,
//! get_strategy, update_slot, set_mechanical_param, set_risk_config,
//! validate_draft.
//!
//! Skill, eval, and marketplace verbs land in subsequent plans (2b/2c/3).

pub mod authoring;
pub mod schema;

use std::path::PathBuf;

use crate::bundle::store::FilesystemStore;

pub struct McpServer {
    store: FilesystemStore,
}

impl McpServer {
    pub fn new(strategies_dir: PathBuf) -> Self {
        Self { store: FilesystemStore::new(strategies_dir) }
    }
}
```

- [ ] **Step 3: Wire into lib.rs**

Add `pub mod mcp;` to `crates/xianvec-engine/src/lib.rs` (preserve existing modules + re-exports).

- [ ] **Step 4: Stub authoring/schema modules**

Create `crates/xianvec-engine/src/mcp/authoring.rs`:

```rust
//! MCP authoring verbs (T2-T8 will fill these in).
```

Create `crates/xianvec-engine/src/mcp/schema.rs`:

```rust
//! JSON schemas for MCP authoring verb arguments.
```

- [ ] **Step 5: Build**

Run: `cargo build -p xianvec-engine 2>&1 | tail -3`
Expected: clean.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-engine/Cargo.toml crates/xianvec-engine/src/mcp crates/xianvec-engine/src/lib.rs
git commit -m "feat(engine): scaffold MCP server module"
```

---

### Task 2: `xvn agent serve --mcp` CLI subcommand

**Files:**
- Create: `crates/xianvec-cli/src/commands/agent.rs`
- Modify: `crates/xianvec-cli/src/commands/mod.rs`
- Modify: `crates/xianvec-cli/src/lib.rs`

- [ ] **Step 1: Create the agent subcommand module**

```rust
//! `xvn agent ...` — serve the MCP server (and other agent surfaces in later plans).

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct AgentCmd {
    #[command(subcommand)]
    action: AgentAction,
}

#[derive(Subcommand, Debug)]
enum AgentAction {
    /// Run the MCP server on stdio. External AI agents (Claude Code,
    /// Hermes, Cursor) connect here.
    Serve {
        /// Run the MCP server (stdio JSON-RPC).
        #[arg(long, default_value_t = false)]
        mcp: bool,
    },
}

pub async fn run(cmd: AgentCmd) -> anyhow::Result<()> {
    match cmd.action {
        AgentAction::Serve { mcp } => {
            if !mcp {
                anyhow::bail!("only --mcp transport is supported in this plan");
            }
            serve_mcp().await
        }
    }
}

async fn serve_mcp() -> anyhow::Result<()> {
    // Implementation lands in Task 3.
    anyhow::bail!("not implemented yet — Task 3")
}
```

- [ ] **Step 2: Register module**

Append `pub mod agent;` to `crates/xianvec-cli/src/commands/mod.rs`.

- [ ] **Step 3: Wire into top-level Command enum**

In `crates/xianvec-cli/src/lib.rs`:

In the `Command` enum, add (alphabetically near `Strategy` is fine):

```rust
    /// Agent surfaces (MCP server etc.).
    Agent(commands::agent::AgentCmd),
```

In `Cli::run()` match:

```rust
            Command::Agent(cmd) => commands::agent::run(cmd).await,
```

- [ ] **Step 4: Smoke build**

```bash
export PATH=$HOME/.cargo/bin:$PATH
cargo build -p xianvec-cli 2>&1 | tail -3
cargo run -q -p xianvec-cli -- agent serve --mcp 2>&1 | head -3
```

Expected: build clean. `xvn agent serve --mcp` exits with "not implemented yet — Task 3" — that's the placeholder.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-cli/src/commands/agent.rs crates/xianvec-cli/src/commands/mod.rs crates/xianvec-cli/src/lib.rs
git commit -m "feat(cli): xvn agent serve --mcp subcommand skeleton"
```

---

### Task 3: Implement minimal MCP server (list_templates only)

**Goal:** Get the round-trip working end-to-end with one verb. Subsequent tasks add verbs incrementally.

**Files:**
- Modify: `crates/xianvec-engine/src/mcp/mod.rs`
- Modify: `crates/xianvec-engine/src/mcp/authoring.rs`
- Modify: `crates/xianvec-cli/src/commands/agent.rs`
- Create: `crates/xianvec-engine/tests/mcp_authoring.rs`

- [ ] **Step 1: Write failing integration test**

Create `crates/xianvec-engine/tests/mcp_authoring.rs`:

```rust
//! End-to-end test: spin up the MCP server in a child process, send a
//! `tools/list` request, expect `list_templates` (and the other 6 verbs
//! once T4-T8 land) in the response.

use std::process::{Command, Stdio};
use std::io::{Read, Write};

#[test]
fn mcp_server_advertises_list_templates_tool() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(["agent", "serve", "--mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn xvn");

    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = child.stdout.take().unwrap();

    // Send MCP initialize request, then tools/list.
    let init = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"test","version":"0.0"}}}
"#;
    stdin.write_all(init.as_bytes()).unwrap();

    let list = r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}
"#;
    stdin.write_all(list.as_bytes()).unwrap();
    drop(stdin);

    let mut buf = String::new();
    stdout.read_to_string(&mut buf).unwrap();
    child.wait().unwrap();

    assert!(buf.contains("list_templates"), "stdout: {buf}");
}
```

- [ ] **Step 2: Verify failure**

`cargo test -p xianvec-engine mcp_server_advertises 2>&1 | tail -10` → FAIL (server bails with "not implemented yet — Task 3").

- [ ] **Step 3: Implement `list_templates` MCP tool**

In `crates/xianvec-engine/src/mcp/authoring.rs`, define a single tool:

```rust
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::templates::registry;

#[derive(Debug, Serialize, Deserialize)]
pub struct ListTemplatesArgs {}

#[derive(Debug, Serialize, Deserialize)]
pub struct ListTemplatesItem {
    pub name: String,
    pub display_name: String,
    pub plain_summary: String,
}

pub fn list_templates(_args: ListTemplatesArgs) -> anyhow::Result<Vec<ListTemplatesItem>> {
    Ok(registry::list_template_names()
        .into_iter()
        .filter_map(|name| {
            registry::get(&name).map(|tpl| ListTemplatesItem {
                name,
                display_name: tpl.display_name().to_string(),
                plain_summary: tpl.plain_summary().to_string(),
            })
        })
        .collect())
}

/// JSON Schema describing the tool's input. Returned in `tools/list`.
pub fn list_templates_schema() -> Value {
    serde_json::json!({
        "type": "object",
        "properties": {},
        "required": [],
    })
}
```

- [ ] **Step 4: Implement the MCP server runtime**

This step depends on the chosen MCP library. With `rmcp` 0.2:

In `crates/xianvec-engine/src/mcp/mod.rs`:

```rust
use rmcp::{ServerHandler, model::{Tool, CallToolResult, Content}};
use serde_json::Value;

use crate::mcp::authoring;

#[derive(Debug, Default, Clone)]
pub struct XvnMcp;

impl ServerHandler for XvnMcp {
    async fn list_tools(&self) -> Result<Vec<Tool>, rmcp::Error> {
        Ok(vec![Tool {
            name: "list_templates".into(),
            description: Some("List all registered xvn strategy templates with display name and plain summary.".into()),
            input_schema: authoring::list_templates_schema(),
        }])
    }

    async fn call_tool(&self, name: &str, args: Value) -> Result<CallToolResult, rmcp::Error> {
        let result = match name {
            "list_templates" => {
                let parsed = serde_json::from_value(args)
                    .map_err(|e| rmcp::Error::invalid_params(e.to_string()))?;
                let items = authoring::list_templates(parsed)
                    .map_err(|e| rmcp::Error::internal(e.to_string()))?;
                serde_json::to_value(items).unwrap()
            }
            _ => return Err(rmcp::Error::method_not_found()),
        };
        Ok(CallToolResult { content: vec![Content::text(result.to_string())] })
    }
}

pub async fn run_stdio() -> anyhow::Result<()> {
    use rmcp::transport::io::stdio;
    rmcp::server::serve(XvnMcp::default(), stdio()).await?;
    Ok(())
}
```

> **Note:** the exact `rmcp` API may differ by version. Read the actual rmcp docs (`cargo doc -p rmcp --open`) and adapt the calls. The shape above is illustrative — the goal is: stdio transport, advertise one tool, dispatch by name. If `rmcp` is too unstable, the fallback is a hand-rolled JSON-RPC reader/writer over `tokio::io::stdin/stdout` — about 80 lines. Document the chosen path.

- [ ] **Step 5: Wire CLI to the runtime**

In `crates/xianvec-cli/src/commands/agent.rs`, replace the `serve_mcp` body:

```rust
async fn serve_mcp() -> anyhow::Result<()> {
    xianvec_engine::mcp::run_stdio().await
}
```

Add `xianvec-engine` to xianvec-cli deps (already there from Plan #1).

- [ ] **Step 6: Test passes**

`cargo test -p xianvec-engine mcp_server_advertises 2>&1 | tail -5` → PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/xianvec-engine/src/mcp crates/xianvec-cli/src/commands/agent.rs crates/xianvec-engine/tests/mcp_authoring.rs
git commit -m "feat(engine): MCP server with list_templates verb"
```

---

## Phase 2A.B — Authoring MCP verbs (six more)

Tasks 4-9 add one verb each. Each follows the same pattern as Task 3:
1. Append a test asserting the verb works (typically via the test harness in `mcp_authoring.rs`)
2. Add the verb function in `authoring.rs`
3. Add to the `tools/list` advertisement in `mcp/mod.rs`
4. Add to the `call_tool` dispatch in `mcp/mod.rs`
5. Test passes, commit

The full task spec is in [appendix A — MCP verbs](#appendix-a-mcp-verbs) at the bottom of this plan. Each verb's signature and JSON schema are spelled out there.

### Task 4: `create_strategy` verb

Args: `{ template: String, name: String, creator: Option<String> }`. Returns: `{ id: String }`. Persists draft via `FilesystemStore` rooted at `$XVN_HOME/strategies/`.

### Task 5: `get_strategy` verb

Args: `{ id: String }`. Returns: full `StrategyBundle` JSON.

### Task 6: `update_slot` verb

Args: `{ id: String, slot: "regime" | "intern" | "trader", prompt: Option<String>, model_requirement: Option<String>, allowed_tools: Option<Vec<String>> }`. Returns: `{ id: String, updated: ["prompt", ...] }`. Mutates the named slot in-place; only fields with `Some` are changed.

### Task 7: `set_mechanical_param` verb

Args: `{ id: String, key: String, value: serde_json::Value }`. Returns: `{ id: String, key: String }`. Updates `bundle.mechanical_params[key] = value`.

### Task 8: `set_risk_config` verb

Args: `{ id: String, preset: Option<String>, explicit: Option<RiskConfig> }`. Mutually exclusive — exactly one of `preset` / `explicit`. Returns: `{ id: String, applied: "preset"|"explicit" }`.

### Task 9: `validate_draft` verb

Args: `{ id: String }`. Returns: `{ id: String, ok: bool, errors: Vec<String> }`. Wraps `validate_bundle`; flat error list.

Each task is ~10 minutes (the pattern is repetitive once Task 3 lands). All tests collected in `tests/mcp_authoring.rs`. After Task 9 commits, the MCP authoring surface is complete.

---

## Phase 2A.C — Tool-call dispatch in agent loop

### Task 10: Extend `LlmRequest` / `LlmResponse` with tool-use shape

**Files:**
- Modify: `crates/xianvec-engine/src/agent/llm.rs`
- Modify: `crates/xianvec-engine/tests/llm_dispatch.rs` (extend tests)

- [ ] **Step 1: Add types to `llm.rs`**

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentBlock {
    Text { text: String },
    ToolUse { id: String, name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StopReason {
    EndTurn,
    ToolUse,
    MaxTokens,
}
```

Extend `LlmRequest`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub system_prompt: String,
    /// Conversation messages — each is a Vec of ContentBlock for tool-use loops.
    /// Initial request: one user message with one Text block.
    /// Tool-use loop: append assistant message (with tool_use), then user
    /// message with tool_result, then re-call.
    pub messages: Vec<Message>,
    pub max_tokens: u32,
    pub tools: Vec<ToolDefinition>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: String, // "user" | "assistant"
    pub content: Vec<ContentBlock>,
}
```

Extend `LlmResponse`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub content: Vec<ContentBlock>,
    pub stop_reason: StopReason,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

impl LlmResponse {
    /// Convenience: concatenate text blocks. Empty if response was tool_use only.
    pub fn text(&self) -> String {
        self.content.iter().filter_map(|c| match c {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        }).collect::<Vec<_>>().join("")
    }
    /// Convenience: extract tool_use blocks for routing.
    pub fn tool_uses(&self) -> Vec<(&str, &str, &serde_json::Value)> {
        self.content.iter().filter_map(|c| match c {
            ContentBlock::ToolUse { id, name, input } => Some((id.as_str(), name.as_str(), input)),
            _ => None,
        }).collect()
    }
}
```

- [ ] **Step 2: Update `MockDispatch` for tool-use loops**

```rust
pub struct MockDispatch {
    /// Sequenced canned responses — pop one per `complete()` call. When
    /// empty, `complete()` returns the last canned response forever.
    canned: std::sync::Mutex<Vec<LlmResponse>>,
}

impl MockDispatch {
    pub fn echo(text: impl Into<String>) -> Self {
        Self::sequence(vec![LlmResponse {
            content: vec![ContentBlock::Text { text: text.into() }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 1, output_tokens: 1,
        }])
    }

    pub fn sequence(responses: Vec<LlmResponse>) -> Self {
        Self { canned: std::sync::Mutex::new(responses) }
    }

    /// Helper: build a tool_use response.
    pub fn tool_use(tool_id: &str, name: &str, input: serde_json::Value) -> LlmResponse {
        LlmResponse {
            content: vec![ContentBlock::ToolUse {
                id: tool_id.into(), name: name.into(), input,
            }],
            stop_reason: StopReason::ToolUse,
            input_tokens: 10, output_tokens: 20,
        }
    }
}

#[async_trait]
impl LlmDispatch for MockDispatch {
    async fn complete(&self, _req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let mut q = self.canned.lock().unwrap();
        if q.len() > 1 {
            Ok(q.remove(0))
        } else {
            Ok(q.first().cloned().unwrap_or_else(|| LlmResponse {
                content: vec![ContentBlock::Text { text: "ok".into() }],
                stop_reason: StopReason::EndTurn,
                input_tokens: 1, output_tokens: 1,
            }))
        }
    }
}
```

- [ ] **Step 3: Update `AnthropicDispatch` to send tools + parse tool_use**

Anthropic API supports `tools` field in request and emits `content` array with `type: "text"` or `type: "tool_use"`:

```rust
#[async_trait]
impl LlmDispatch for AnthropicDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let body = serde_json::json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": req.system_prompt,
            "messages": req.messages,
            "tools": req.tools,
        });
        let resp = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await?
            .error_for_status()?
            .json::<serde_json::Value>()
            .await?;

        let raw_content = resp["content"].as_array().cloned().unwrap_or_default();
        let mut content = Vec::with_capacity(raw_content.len());
        for block in raw_content {
            match block["type"].as_str() {
                Some("text") => content.push(ContentBlock::Text {
                    text: block["text"].as_str().unwrap_or("").to_string(),
                }),
                Some("tool_use") => content.push(ContentBlock::ToolUse {
                    id: block["id"].as_str().unwrap_or("").to_string(),
                    name: block["name"].as_str().unwrap_or("").to_string(),
                    input: block["input"].clone(),
                }),
                _ => {}
            }
        }
        let stop_reason = match resp["stop_reason"].as_str() {
            Some("end_turn") => StopReason::EndTurn,
            Some("tool_use") => StopReason::ToolUse,
            Some("max_tokens") => StopReason::MaxTokens,
            _ => StopReason::EndTurn,
        };
        let input_tokens = resp["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
        Ok(LlmResponse { content, stop_reason, input_tokens, output_tokens })
    }
}
```

- [ ] **Step 4: Update existing tests for the new shape**

In `tests/llm_dispatch.rs`, change the existing `mock_dispatch_returns_expected_output`:

```rust
#[tokio::test]
async fn mock_dispatch_returns_text_block() {
    let mock = MockDispatch::echo(r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#);
    let resp = mock.complete(LlmRequest {
        model: "anthropic.claude-sonnet-4.6".into(),
        system_prompt: "you are a trader".into(),
        messages: vec![Message {
            role: "user".into(),
            content: vec![ContentBlock::Text { text: "decide".into() }],
        }],
        max_tokens: 200,
        tools: vec![],
    }).await.unwrap();
    assert!(resp.text().contains("hold"));
    assert!(matches!(resp.stop_reason, StopReason::EndTurn));
}
```

- [ ] **Step 5: Run tests, fix call sites in execute_slot/pipeline**

`cargo build --workspace` will surface call sites that broke. The main breakage will be in `execute_slot` (Plan #1, Task 14) which constructed `LlmRequest` with the old `user_prompt` field. **DO NOT FIX execute_slot here** — Task 11 redesigns it for the tool-use loop. Instead, update execute_slot's existing call site to use the new `messages` shape minimally:

```rust
// in execute.rs
let req = LlmRequest {
    model: input.slot.model_requirement.clone(),
    system_prompt: input.slot.prompt.clone(),
    messages: vec![Message {
        role: "user".into(),
        content: vec![ContentBlock::Text { text: user_prompt }],
    }],
    max_tokens: 1000,
    tools: vec![],  // Task 11 will populate this from slot.allowed_tools
};
```

Same minimal patch in `tests/agent_slot.rs` and `tests/pipeline_inline.rs` — the seed_inputs helper logic is unchanged; only the LlmRequest shape needs translation.

- [ ] **Step 6: All workspace tests pass**

`cargo test --workspace 2>&1 | grep -E "test result.*FAIL"` → no output.
`cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -2` → clean.

- [ ] **Step 7: Commit**

```bash
git add crates/xianvec-engine
git commit -m "feat(engine): extend LlmRequest/Response with tool-use content blocks"
```

---

### Task 11: Tool-call loop in `execute_slot`

**Files:**
- Modify: `crates/xianvec-engine/src/agent/execute.rs`
- Create: `crates/xianvec-engine/src/agent/tool_call.rs`
- Modify: `crates/xianvec-engine/src/agent/mod.rs`
- Modify: `crates/xianvec-engine/tests/agent_slot.rs`

- [ ] **Step 1: Write failing test for the loop**

Append to `tests/agent_slot.rs`:

```rust
use xianvec_engine::agent::llm::{ContentBlock, LlmResponse, MockDispatch, StopReason};

#[tokio::test]
async fn execute_slot_loops_through_tool_use_to_final_text() {
    use xianvec_engine::agent::execute::{execute_slot, SlotInput};
    use xianvec_engine::bundle::slot::LLMSlot;
    use xianvec_engine::tools::ToolRegistry;
    use std::sync::Arc;

    let slot = LLMSlot {
        role: "trader".into(),
        prompt: "decide".into(),
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
    };

    // Sequence: turn 1 emits tool_use(ohlcv); turn 2 emits final text.
    let dispatch = Arc::new(MockDispatch::sequence(vec![
        MockDispatch::tool_use(
            "tu_001", "ohlcv",
            serde_json::json!({"asset": "BTC/USD", "fixture": "test-fixture-btc-2024-01"}),
        ),
        LlmResponse {
            content: vec![ContentBlock::Text {
                text: r#"{"action":"long_open","conviction":0.7,"justification":"oversold"}"#.into(),
            }],
            stop_reason: StopReason::EndTurn,
            input_tokens: 50, output_tokens: 30,
        },
    ]));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let out = execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({"asset": "BTC/USD", "fixture": "test-fixture-btc-2024-01"}),
        dispatch,
        tools,
    }).await.unwrap();
    assert!(out.text().contains("long_open"));
    // Two LLM calls: tool_use then final text.
    assert!(out.input_tokens >= 50);
}
```

- [ ] **Step 2: Verify failure**

`cargo test -p xianvec-engine execute_slot_loops_through 2>&1 | tail -10` → FAIL — current execute_slot doesn't loop.

- [ ] **Step 3: Implement the loop**

Create `crates/xianvec-engine/src/agent/tool_call.rs`:

```rust
use std::sync::Arc;

use crate::agent::llm::{ContentBlock, ToolDefinition};
use crate::tools::{ToolName, ToolRegistry};

/// Build tool definitions from a slot's allowed_tools. The runtime asks the
/// registry for each tool's name + description; if the slot lists a tool the
/// registry doesn't know, it's silently dropped (validation should catch this
/// at bundle time).
pub fn definitions_for_slot(
    allowed_tools: &[String],
    registry: &ToolRegistry,
) -> Vec<ToolDefinition> {
    allowed_tools
        .iter()
        .filter_map(|name| {
            registry.get(&ToolName::new(name.clone())).map(|tool| ToolDefinition {
                name: name.clone(),
                description: tool.description().to_string(),
                // For now: empty schema. Tools will declare richer schemas in 2c.
                input_schema: serde_json::json!({"type": "object"}),
            })
        })
        .collect()
}

/// Invoke a tool by name and return a stringified result for inclusion in
/// the next message as a tool_result content block.
pub async fn invoke(
    name: &str,
    input: serde_json::Value,
    registry: Arc<ToolRegistry>,
) -> anyhow::Result<String> {
    let tool = registry
        .get(&ToolName::new(name.to_string()))
        .ok_or_else(|| anyhow::anyhow!("tool '{name}' not in registry"))?;
    let out = tool.invoke(input).await?;
    Ok(out.to_string())
}

pub(crate) fn tool_uses(content: &[ContentBlock]) -> Vec<(String, String, serde_json::Value)> {
    content
        .iter()
        .filter_map(|c| match c {
            ContentBlock::ToolUse { id, name, input } => {
                Some((id.clone(), name.clone(), input.clone()))
            }
            _ => None,
        })
        .collect()
}
```

Update `crates/xianvec-engine/src/agent/mod.rs`:

```rust
pub mod execute;
pub mod llm;
pub mod pipeline;
pub mod tool_call;
```

REPLACE `crates/xianvec-engine/src/agent/execute.rs`:

```rust
use std::sync::Arc;

use crate::agent::llm::{
    ContentBlock, LlmDispatch, LlmRequest, LlmResponse, Message, StopReason,
};
use crate::agent::tool_call;
use crate::bundle::slot::LLMSlot;
use crate::tools::ToolRegistry;

const MAX_TOOL_USE_ITERATIONS: u32 = 8;

pub struct SlotInput<'a> {
    pub slot: &'a LLMSlot,
    pub upstream_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
}

pub async fn execute_slot<'a>(input: SlotInput<'a>) -> anyhow::Result<LlmResponse> {
    let initial_user = format!(
        "Inputs:\n{}\n\nFollow the slot's instructions. You may call tools \
         to fetch additional data; emit your final decision as JSON.",
        serde_json::to_string_pretty(&input.upstream_inputs)?
    );

    let tool_defs = tool_call::definitions_for_slot(&input.slot.allowed_tools, &input.tools);

    let mut messages = vec![Message {
        role: "user".into(),
        content: vec![ContentBlock::Text { text: initial_user }],
    }];

    let mut total_input_tokens = 0u32;
    let mut total_output_tokens = 0u32;
    let mut last_response: Option<LlmResponse> = None;

    for _iter in 0..MAX_TOOL_USE_ITERATIONS {
        let req = LlmRequest {
            model: input.slot.model_requirement.clone(),
            system_prompt: input.slot.prompt.clone(),
            messages: messages.clone(),
            max_tokens: 1000,
            tools: tool_defs.clone(),
        };
        let resp = input.dispatch.complete(req).await?;
        total_input_tokens += resp.input_tokens;
        total_output_tokens += resp.output_tokens;

        // If the model wants tools, run them and loop.
        let uses = tool_call::tool_uses(&resp.content);
        if uses.is_empty() || matches!(resp.stop_reason, StopReason::EndTurn | StopReason::MaxTokens) {
            // Final response — return.
            last_response = Some(LlmResponse {
                content: resp.content,
                stop_reason: resp.stop_reason,
                input_tokens: total_input_tokens,
                output_tokens: total_output_tokens,
            });
            break;
        }

        // Append assistant message with the tool_use blocks.
        messages.push(Message { role: "assistant".into(), content: resp.content.clone() });

        // Run each tool and collect tool_result blocks.
        let mut results = Vec::with_capacity(uses.len());
        for (tu_id, tu_name, tu_input) in uses {
            let result = tool_call::invoke(&tu_name, tu_input, input.tools.clone()).await
                .unwrap_or_else(|e| format!("tool error: {e}"));
            results.push(ContentBlock::ToolResult {
                tool_use_id: tu_id,
                content: result,
            });
        }
        messages.push(Message { role: "user".into(), content: results });
    }

    last_response.ok_or_else(|| anyhow::anyhow!(
        "execute_slot exceeded {MAX_TOOL_USE_ITERATIONS} tool-use iterations"
    ))
}
```

- [ ] **Step 4: Tests pass**

`cargo test -p xianvec-engine execute_slot 2>&1 | tail -10` → all execute_slot tests pass.
`cargo test -p xianvec-engine pipeline 2>&1 | tail -5` → pipeline tests still pass (they use MockDispatch::echo which now returns end_turn immediately, so no tool loop runs).

- [ ] **Step 5: Update execute_slot test that asserts undeclared-tool behavior**

The existing `execute_slot_succeeds_even_when_caller_passes_extra_inputs` test (Plan #1) still asserts that tools-not-in-allowlist don't crash. With tool_use now wired, undeclared tools are simply absent from `tool_defs` — the LLM can't request them. The existing test's assertion (`result.is_ok()`) stays valid because MockDispatch::echo emits Text-only content; no tool routing happens. Verify no breakage.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-engine/src/agent
git commit -m "feat(engine): tool-use loop in execute_slot — LLM can call tools mid-decision"
```

---

### Task 12: Real OHLCV + IndicatorPanel in `xvn strategy run`

**Files:**
- Modify: `crates/xianvec-cli/src/commands/strategy.rs`

- [ ] **Step 1: Write failing test**

Append to `crates/xianvec-cli/tests/strategy_cli.rs`:

```rust
#[test]
fn run_inline_seeds_with_real_ohlcv_and_indicators() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "new", "--template", "mean_reversion", "--name", "real-data"], dir.path());
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let out = xvn(
        &["strategy", "run", &id, "--fixture", "test-fixture-btc-2024-01",
          "--decisions", "1", "--mock"],
        dir.path(),
    );
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    // Per-decision output should include something derived from real bars.
    // We use a marker that proves real data populated the seed: the trader
    // mock prompt receives the full seed JSON, so the seed's first ohlcv close
    // value should be reflected somewhere if we log it.
    assert!(stdout.contains("decision[0]:"));
    assert!(stdout.contains("seed_summary:"), "stdout: {stdout}");
}
```

- [ ] **Step 2: Verify fail**

`cargo test -p xianvec-cli run_inline_seeds_with_real_ohlcv` → FAIL.

- [ ] **Step 3: Update `run_inline` to fetch via tools**

In `crates/xianvec-cli/src/commands/strategy.rs`, replace the `for n in 0..decisions` block:

```rust
for n in 0..decisions {
    let ohlcv_tool = tools
        .get(&xianvec_engine::tools::ToolName::new("ohlcv".to_string()))
        .ok_or_else(|| anyhow::anyhow!("ohlcv tool not registered"))?;
    let panel_tool = tools
        .get(&xianvec_engine::tools::ToolName::new("indicator_panel".to_string()))
        .ok_or_else(|| anyhow::anyhow!("indicator_panel tool not registered"))?;

    let ohlcv = ohlcv_tool.invoke(serde_json::json!({
        "asset": asset,
        "fixture": fixture,
        "lookback_bars": 200,
    })).await?;
    let panel = panel_tool.invoke(serde_json::json!({
        "asset": asset,
        "fixture": fixture,
        "lookback_bars": 200,
    })).await?;

    let bar_count = ohlcv.get("bars")
        .and_then(|b| b.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    println!("seed_summary: bars={bar_count} asset={asset} fixture={fixture}");

    let seed = serde_json::json!({
        "decision_index": n,
        "asset": asset,
        "fixture": fixture,
        "ohlcv_history": ohlcv,
        "indicator_panel": panel,
    });
    let outs = run_pipeline(PipelineInputs {
        bundle: &bundle, seed_inputs: seed,
        dispatch: dispatch.clone(), tools: tools.clone(),
    }).await?;
    total_in += outs.total_input_tokens;
    total_out += outs.total_output_tokens;
    if let Some(t) = &outs.trader {
        println!("decision[{n}]: {}", t.text().trim());
    }
}
```

> **Note:** because Plan #1's `LlmResponse.text` was a field, but Plan 2a Task 10 changed it to a method `text()`, the call site here uses `t.text()`. If a different breakage shows up, follow the compiler.

- [ ] **Step 4: Test passes**

`cargo test -p xianvec-cli run_inline_seeds_with_real_ohlcv 2>&1 | tail -5` → PASS.
`cargo test --workspace 2>&1 | grep -E "test result.*FAIL"` → no output.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-cli/src/commands/strategy.rs crates/xianvec-cli/tests/strategy_cli.rs
git commit -m "feat(cli): xvn strategy run seeds pipeline with real OHLCV + indicators"
```

---

## Phase 2A.D — Seven remaining templates

Each template task follows the SAME pattern as Plan #1 Task 9 (`mean_reversion`). For brevity, only the prompts and key parameters are spelled out per template. The structure of each task is:

1. Create `src/templates/<name>.rs` with a struct + Template impl
2. Add `pub mod <name>;` to `src/templates/mod.rs`
3. Wire into `templates/registry.rs`'s `get_or_init` vec
4. Append a validation test to `tests/template_validation.rs` (or batch into `tests/seven_templates.rs`)
5. Build clean, commit

Use Plan #1 Task 9 as the implementation reference.

### Task 13: `trend_follower` template

**display_name:** "Catches uptrends"
**plain_summary:** "Buys when crypto starts trending up, sells when momentum fades. Best when markets are moving."
**regime_fit:** [TrendingBull, TrendingBear]
**asset_universe:** ["BTC/USD", "ETH/USD"]
**decision_cadence_minutes:** 60
**risk_preset:** balanced
**required_tools:** [ohlcv, indicator_panel]

**Trader prompt summary:** Given EMA(12), EMA(26), and EMA(50) on the indicator panel, enter long if EMA(12) > EMA(26) > EMA(50) AND price > EMA(12). Reverse for shorts. Output `{action, conviction, justification}`.

**Mechanical params:** `{ "ema_fast": 12, "ema_mid": 26, "ema_slow": 50 }`

### Task 14: `breakout` template

**display_name:** "Buys breakouts"
**plain_summary:** "Buys when price breaks above its recent range, exits on stall."
**regime_fit:** [TrendingBull, HighVol]
**asset_universe:** ["BTC/USD"]
**decision_cadence_minutes:** 30
**risk_preset:** balanced
**required_tools:** [ohlcv, indicator_panel]

**Trader prompt summary:** Use Donchian(20) (or fall back to highest-high/lowest-low of last 20 bars from ohlcv_history if Donchian isn't in the panel). Enter long when close > donchian_high(20) AND volume > 1.5× SMA(volume, 20). Output `{action, conviction, justification}`.

**Mechanical params:** `{ "donchian_period": 20, "volume_confirm_multiple": 1.5 }`

### Task 15: `momentum` template

**display_name:** "Rides momentum"
**plain_summary:** "Holds positions while momentum is strong; cuts when it fades."
**regime_fit:** [TrendingBull, TrendingBear]
**asset_universe:** ["BTC/USD", "ETH/USD"]
**decision_cadence_minutes:** 60
**risk_preset:** balanced
**required_tools:** [ohlcv, indicator_panel]

**Trader prompt summary:** Use MACD signal-line crossovers + ADX(14) for strength. Enter long on MACD bullish cross when ADX > 25. Output `{action, conviction, justification}`.

**Mechanical params:** `{ "macd_fast": 12, "macd_slow": 26, "macd_signal": 9, "adx_period": 14, "adx_threshold": 25 }`

### Task 16: `range_trade` template

**display_name:** "Trades the range"
**plain_summary:** "Buys near support, sells near resistance — only during sideways markets."
**regime_fit:** [RangeBound, LowVol]
**asset_universe:** ["ETH/USD"]
**decision_cadence_minutes:** 30
**risk_preset:** conservative
**required_tools:** [ohlcv, indicator_panel]

**Trader prompt summary:** Use Bollinger(20, 2) %B oscillator. Enter long when %B < 0.1 AND close > prior close. Enter short when %B > 0.9 AND close < prior close. Output `{action, conviction, justification}`.

**Mechanical params:** `{ "bb_period": 20, "bb_sigma": 2.0, "lower_threshold": 0.1, "upper_threshold": 0.9 }`

### Task 17: `scalping` template

**display_name:** "Quick small trades"
**plain_summary:** "Many small trades, very short hold times. Sensitive to fees and latency — use only on liquid pairs."
**regime_fit:** [HighVol]
**asset_universe:** ["BTC/USD"]
**decision_cadence_minutes:** 5
**risk_preset:** conservative
**required_tools:** [ohlcv, indicator_panel]

**Trader prompt summary:** Use 1-min/5-min EMA crossovers with very tight stops (0.3% from entry). Output `{action, conviction, justification}`. Conviction must reflect spread + fee awareness — return `flat` if estimated fees exceed expected move.

**Mechanical params:** `{ "ema_fast": 5, "ema_slow": 13, "stop_pct": 0.003, "take_profit_pct": 0.006 }`

### Task 18: `news_trader` template

**display_name:** "Trades news events"
**plain_summary:** "Reacts to news and sentiment changes. Requires a news API key (configured separately)."
**regime_fit:** [EventDriven, HighVol]
**asset_universe:** ["ETH/USD"]
**decision_cadence_minutes:** 15
**risk_preset:** conservative
**required_tools:** [ohlcv, indicator_panel]

> Note: news/sentiment as a tool isn't wired in 2a. The trader prompt acknowledges this and operates on price + indicators only as a fallback. Plan 2c adds the real news tool, at which point this template's `required_tools` adds `news_sentiment`.

**Trader prompt summary:** "You would normally have access to a news_sentiment tool. In this MVP it is not yet wired — operate on price action only and emit a flat decision unless extreme volatility appears in ohlcv_history (>3 ATR move in last 4 bars)."

**Mechanical params:** `{ "extreme_move_atr_multiple": 3.0, "lookback_bars": 4 }`

### Task 19: `custom` template (single-LLM-agent freeform)

**display_name:** "Single-agent freeform"
**plain_summary:** "A blank canvas. One LLM trader agent with no scaffold — for sophisticated authors who want full discretion."
**regime_fit:** [TrendingBull, TrendingBear, RangeBound, Chop]  // any
**asset_universe:** ["BTC/USD"]
**decision_cadence_minutes:** 60
**risk_preset:** conservative  // err on the safe side for free-form authors
**required_tools:** [ohlcv, indicator_panel]
**slots:** trader_slot only (no regime, no intern). Slot's prompt is intentionally minimal.

**Trader prompt summary:** "You are a trading agent. Decide based on the inputs provided. Output JSON: {action, conviction, justification}."

**Mechanical params:** `{}` (empty — author fills in their own structure if any)

### Task 20: Register all 7 templates + ma_crossover_baseline; integration test

**Files:**
- Modify: `crates/xianvec-engine/src/templates/mod.rs` (add 7 mod declarations)
- Modify: `crates/xianvec-engine/src/templates/registry.rs` (add 8 entries — 7 new + ma_crossover_baseline now registered)
- Create: `crates/xianvec-engine/tests/seven_templates.rs`

- [ ] **Step 1: Module declarations**

In `crates/xianvec-engine/src/templates/mod.rs`:

```rust
pub mod breakout;
pub mod custom;
pub mod mean_reversion;
pub mod momentum;
pub mod news_trader;
pub mod range_trade;
pub mod registry;
pub mod scalping;
pub mod trend_follower;
// (Template trait stays here)
```

- [ ] **Step 2: Registry**

REPLACE `templates/registry.rs`'s `registry()` body:

```rust
fn registry() -> &'static [Box<dyn Template>] {
    REGISTRY.get_or_init(|| vec![
        Box::new(crate::templates::trend_follower::TrendFollower) as Box<dyn Template>,
        Box::new(crate::templates::breakout::Breakout),
        Box::new(crate::templates::mean_reversion::MeanReversion),
        Box::new(crate::templates::momentum::Momentum),
        Box::new(crate::templates::range_trade::RangeTrade),
        Box::new(crate::templates::scalping::Scalping),
        Box::new(crate::templates::news_trader::NewsTrader),
        Box::new(crate::templates::custom::Custom),
        // Baseline as a marketplace seed listing
        crate::baselines::ma_crossover::ma_crossover_template(),
    ])
}
```

- [ ] **Step 3: Integration test**

Create `crates/xianvec-engine/tests/seven_templates.rs`:

```rust
use xianvec_engine::bundle::validate::validate_bundle;
use xianvec_engine::templates::{registry, Template};

const EXPECTED_TEMPLATES: &[&str] = &[
    "trend_follower", "breakout", "mean_reversion", "momentum",
    "range_trade", "scalping", "news_trader", "custom",
    "ma_crossover_baseline",
];

#[test]
fn all_v1_templates_are_registered() {
    let names = registry::list_template_names();
    for name in EXPECTED_TEMPLATES {
        assert!(names.contains(&name.to_string()), "missing template: {name}");
    }
}

#[test]
fn each_template_produces_a_valid_draft() {
    for name in EXPECTED_TEMPLATES {
        let tpl = registry::get(name).unwrap_or_else(|| panic!("template {name} missing"));
        let draft = tpl.new_draft(
            format!("01H8N7Z{}", name).chars().take(26).collect(),
            format!("test-{name}"),
            "@test".into(),
        );
        validate_bundle(&draft).unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}

#[test]
fn templates_have_unique_names() {
    let names = registry::list_template_names();
    let mut sorted = names.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(names.len(), sorted.len(), "duplicate template names: {names:?}");
}
```

- [ ] **Step 4: Tests pass**

`cargo test -p xianvec-engine seven_templates 2>&1 | tail -10` → PASS for all 3 tests.
`cargo test --workspace` → no failures.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/templates crates/xianvec-engine/tests/seven_templates.rs
git commit -m "feat(engine): register 7 v1 templates + ma_crossover baseline in marketplace"
```

---

## Phase 2A.E — Polish + smoke

### Task 21: Update README + smoke recipe

**Files:**
- Modify: `crates/xianvec-engine/README.md`
- Modify: `MANUAL.md`

- [ ] **Step 1: Update `crates/xianvec-engine/README.md`**

Replace the "What ships in MVP" section with:

```markdown
## What ships in v0.2 (Plan 2a)

- Strategy bundle types (manifest + slots + risk + mechanical params) **— v0.1 (Plan #1)**
- 8 templates: `trend_follower`, `breakout`, `mean_reversion`, `momentum`, `range_trade`, `scalping`, `news_trader`, `custom`, plus `ma_crossover_baseline`
- `ToolRegistry` with `ohlcv` and `indicator_panel` tools (fixture-mode) **— v0.1**
- 3-slot agent pipeline (regime → intern → trader), inline execution **— v0.1**
- **NEW:** Tool-use loops — LLM agents can request OHLCV/indicators mid-decision; runtime routes through ToolRegistry and feeds tool_result back. Up to 8 iterations per slot.
- `LlmDispatch` trait + Anthropic + Mock implementations **— v0.1, extended for tool-use**
- Token estimator **— v0.1**
- CLI: `xvn strategy {new | validate | ls | show | templates | run}` **— v0.1, run now seeds with real OHLCV+indicators**
- **NEW:** `xvn agent serve --mcp` — full MCP authoring surface (7 verbs): `list_templates`, `create_strategy`, `get_strategy`, `update_slot`, `set_mechanical_param`, `set_risk_config`, `validate_draft`.

## What does NOT ship in v0.2

- Skill management MCP verbs (Plan 2b)
- Tier B sealing + xvn API server (Plan 2b)
- Durable scheduler (Plan 2c)
- Live execution daemon (Plan 2c)
- Marketplace + 8004 publish (Plan 2b)
- Web dashboard / Agent Wizard (Plan 2d)
- Eval engine (Plan 3)
- Real news/sentiment tool — `news_trader` template ships with a fallback prompt
```

- [ ] **Step 2: Update `MANUAL.md`**

Append to the "Strategy authoring" section a new "AI agent driving xvn" subsection:

```markdown
### AI agent drives xvn (Plan 2a)

External AI agents (Claude Code, Hermes, Cursor) connect to xvn over MCP:

```bash
xvn agent serve --mcp
```

The server speaks JSON-RPC over stdio. Authoring verbs available:
`list_templates`, `create_strategy`, `get_strategy`, `update_slot`,
`set_mechanical_param`, `set_risk_config`, `validate_draft`.

In Claude Code, register the MCP server in `~/.claude/settings.json`:

```json
{
  "mcpServers": {
    "xvn": { "command": "xvn", "args": ["agent", "serve", "--mcp"] }
  }
}
```
```

- [ ] **Step 3: Run a full smoke test**

```bash
export PATH=$HOME/.cargo/bin:$PATH
export XVN_HOME=/tmp/xvn-2a-smoke
rm -rf $XVN_HOME

# Templates round-trip
cargo run -q -p xianvec-cli -- strategy templates

# CLI authoring with the new templates
cargo run -q -p xianvec-cli -- strategy new --template trend_follower --name tf-smoke
cargo run -q -p xianvec-cli -- strategy new --template breakout --name br-smoke
cargo run -q -p xianvec-cli -- strategy ls

# Run with real OHLCV via tools
ID=$(cargo run -q -p xianvec-cli -- strategy ls | head -1)
cargo run -q -p xianvec-cli -- strategy run "$ID" --fixture test-fixture-btc-2024-01 --decisions 2 --mock

# MCP server smoke (send tools/list, expect 7 authoring verbs)
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"smoke","version":"0"}}}' | cargo run -q -p xianvec-cli -- agent serve --mcp &
sleep 1
# Manual: send tools/list and inspect output, or use mcp-inspector
```

- [ ] **Step 4: Commit**

```bash
git add crates/xianvec-engine/README.md MANUAL.md
git commit -m "docs(engine): Plan 2a README + manual update"
```

---

### Task 22: Final workspace check + clippy + fmt

**Files:** None (verification only).

- [ ] **Step 1: Tests workspace-wide**

`cargo test --workspace 2>&1 | grep -E "test result.*FAIL"` → no output.
`cargo test --workspace 2>&1 | grep "test result" | awk -F'[. ]' 'BEGIN{p=0} {p+=$5} END{print "passed:", p}'` → at least the Plan 1 baseline + new tests.

- [ ] **Step 2: Clippy**

`cargo clippy --workspace --all-targets -- -D warnings 2>&1 | tail -3` → clean.

- [ ] **Step 3: Fmt scoped to plan-touched crates**

`cargo fmt -p xianvec-engine -p xianvec-cli -- --check 2>&1 | head -20` → no diff. If diff: run `cargo fmt -p xianvec-engine -p xianvec-cli` and commit as `chore(engine): cargo fmt cleanup for Plan 2a files`.

- [ ] **Step 4: Verify scope discipline**

`git diff main..HEAD -- crates/xianvec-eval/ | wc -l` → 0 (xianvec-eval untouched, per Plan #1's deferral).
`git log --oneline main..HEAD | wc -l` → roughly 22 plan commits (+ any docs commits).

- [ ] **Step 5: Final commit (if cleanup landed)**

If anything to commit, follow the same `chore(...)` pattern as Plan #1's final fmt commit. Otherwise: done.

---

## Self-review checklist

After all 22 tasks ship, walk through:

**Spec coverage from `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md` for Plan 2a's scope:**
- [x] §10 MCP server surface — authoring verb group (skills/eval/marketplace deferred)
- [x] §4 Templates — all 8 v1 templates registered
- [x] §3 Strategy artifact — extended with tool-use dispatch in slot execution
- [ ] §2 KISS / Agent Wizard — deferred to Plan 2d
- [ ] §5 Permission tiers — deferred to Plan 2b (Tier B sealing)
- [ ] §6 Skill bundle format — deferred to Plan 2b
- [ ] §11 Live execution — deferred to Plan 2c
- [ ] §12 Durable scheduler — deferred to Plan 2c
- [ ] §13 Marketplace + 8004 — deferred to Plan 2b

**Type consistency check:** `LlmRequest`, `LlmResponse`, `ContentBlock`, `Message`, `StopReason`, `ToolDefinition`, `MockDispatch::sequence/echo/tool_use`, `tool_call::definitions_for_slot/invoke/tool_uses`, `XvnMcp`, `McpServer`, all template structs (`TrendFollower`, `Breakout`, `Momentum`, `RangeTrade`, `Scalping`, `NewsTrader`, `Custom`) — names used consistently across all 22 tasks.

**No placeholders:** every code block contains real Rust. The trader-prompt summaries in §2A.D are written-out instructions but each task implementer must compose the actual `const TRADER_PROMPT: &str = r#"..."#;` body using Plan #1's `mean_reversion` template as a structural reference. If a template is implemented by following only the summary and not the full prompt-text expansion, flag it as DONE_WITH_CONCERNS.

**Frequent commits:** 22 tasks, ~22 commits, each green and focused.

---

## What's next after this plan ships

Plan 2b — **Sealing + Marketplace + 8004**
- xvn API server (OSShip-style centralized hosting with Ed25519 signing)
- Tier B sealed strategies (server-hosted, per-execution fetch)
- 8004 marketplace publish flow
- License token issuance + buyer auth
- Skill management MCP verbs

Plan 2c — **Durable Scheduler + Live Execution**
- Port SwarmClaw scheduler pattern (Rust)
- Cron / heartbeat / retry / agent-handoff
- Live execution daemon (Alpaca paper, Orderly live)
- News/sentiment tool integration (unblocks `news_trader` template)
- fly.io deploy recipe

Plan 2d — **Web Dashboard + Agent Wizard**
- axum SPA shell, multi-archetype router
- L1 Agent Wizard at `/` (chat + visual progress sidecar)
- L3 Inspector form
- TradingView Lightweight Charts integration

Plan 3 — **Eval Engine**
- Already specified at `docs/superpowers/specs/2026-05-08-eval-engine-design.md`
- Independent of Plans 2a–2d; can run in parallel

---

## Appendix A — MCP verbs

Detailed signatures for Tasks 4-9. Each follows the same pattern as Task 3's `list_templates`. Schema fields use JSON Schema draft 2020-12.

### create_strategy

```json
{
  "name": "create_strategy",
  "description": "Create a new strategy draft from a template. Returns the new draft's ULID.",
  "input_schema": {
    "type": "object",
    "properties": {
      "template": { "type": "string", "description": "Template name (e.g., 'mean_reversion')" },
      "name":     { "type": "string", "description": "Human-readable strategy name" },
      "creator":  { "type": "string", "description": "@handle or wallet (default: @anonymous)" }
    },
    "required": ["template", "name"]
  }
}
```

Returns: `{ "id": "<ULID>" }`.

### get_strategy

```json
{
  "name": "get_strategy",
  "description": "Fetch a saved strategy bundle by id.",
  "input_schema": {
    "type": "object",
    "properties": { "id": { "type": "string" } },
    "required": ["id"]
  }
}
```

Returns: full `StrategyBundle` JSON.

### update_slot

```json
{
  "name": "update_slot",
  "description": "Mutate a single slot (regime|intern|trader). Only fields with values are changed.",
  "input_schema": {
    "type": "object",
    "properties": {
      "id":   { "type": "string" },
      "slot": { "type": "string", "enum": ["regime", "intern", "trader"] },
      "prompt":             { "type": "string" },
      "model_requirement":  { "type": "string" },
      "allowed_tools":      { "type": "array", "items": { "type": "string" } }
    },
    "required": ["id", "slot"]
  }
}
```

Returns: `{ "id": "...", "updated": ["prompt", "allowed_tools"] }` (lists fields that were changed).

### set_mechanical_param

```json
{
  "name": "set_mechanical_param",
  "description": "Set a single mechanical_params key on a draft.",
  "input_schema": {
    "type": "object",
    "properties": {
      "id":    { "type": "string" },
      "key":   { "type": "string" },
      "value": {} 
    },
    "required": ["id", "key", "value"]
  }
}
```

Returns: `{ "id": "...", "key": "rsi_oversold" }`.

### set_risk_config

```json
{
  "name": "set_risk_config",
  "description": "Apply a preset OR explicit RiskConfig to a draft. Mutually exclusive.",
  "input_schema": {
    "type": "object",
    "properties": {
      "id":       { "type": "string" },
      "preset":   { "type": "string", "enum": ["conservative", "balanced", "aggressive"] },
      "explicit": {
        "type": "object",
        "properties": {
          "risk_pct_per_trade":       { "type": "number" },
          "max_concurrent_positions": { "type": "integer" },
          "max_leverage":             { "type": "number" },
          "stop_loss_atr_multiple":   { "type": "number" },
          "daily_loss_kill_pct":      { "type": "number" }
        }
      }
    },
    "required": ["id"]
  }
}
```

Returns: `{ "id": "...", "applied": "preset" | "explicit" }`. Server-side validation: exactly one of `preset` / `explicit` must be present.

### validate_draft

```json
{
  "name": "validate_draft",
  "description": "Run validate_bundle on a saved draft. Returns ok=true or a list of error strings.",
  "input_schema": {
    "type": "object",
    "properties": { "id": { "type": "string" } },
    "required": ["id"]
  }
}
```

Returns: `{ "id": "...", "ok": true }` OR `{ "id": "...", "ok": false, "errors": ["strategy must have a trader slot (slot ④ Decision Arbiter)"] }`.
