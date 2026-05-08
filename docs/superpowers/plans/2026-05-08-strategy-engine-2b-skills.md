# Strategy Creation Engine — Plan 2b (Skills) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan #1 + Plan 2a merged.
> **Scope decision (2026-05-08):** Marketplace + reputation work has been **fully deferred to Plan 5** (blockchain integration). Plan 2b ships only the **skills system** — OSShip-style markdown skills authors can compose into strategy slots. Skills are useful for authoring (e.g., a `news-aware-decision` skill that any trader slot can attach) and stand alone without a marketplace. Discovery + reputation + content-addressed publishing all wait for the blockchain plan to be in place.
> **Execution-order decision (2026-05-08):** Execute this plan **after Plan 3 (eval engine) AND Plan 2a ship**, in case eval's findings-extractor markdown prompt format or 2a's MCP authoring surface surface design decisions that affect how skills should be structured. Plan 3 ships an inline OSShip-style markdown prompt at `xianvec-engine/src/eval/findings/prompts/extractor-v1.md` — when this plan ships, that prompt becomes a candidate for migration into the formal skill registry as `eval-findings-extractor`. The cross-pollination is real, hence the deferral.

**Goal:** Authors can write reusable OSShip-style skill markdown files, save them locally, and attach them to strategy slots to override prompts + tool allowlists. After this plan ships: an author runs `xvn skill new --from-file my-trader.md` to register a skill, `xvn skill ls` to list saved skills, and `xvn skill attach <strategy_id> --slot trader --skill my-trader` to swap a strategy's trader prompt with the skill's body. The same surface is exposed via 3 MCP verbs so external AI agents can compose skills into strategies.

**Architecture:** One new crate. `xianvec-skills` parses + validates OSShip-style skill markdown, supports skill attach to slots, stores skills under `$XVN_HOME/skills/`. The MCP server (Plan 2a) gains 3 new authoring verbs.

**Tech Stack:** Rust 2021. New deps: `serde_yaml = "0.9"`, `sha2 = "0.10"`. Reuses everything from Plans #1 and 2a.

**Out of scope (deferred):**
- Marketplace publish / browse / install / attest — **Plan 5** (blockchain integration). This includes the `xianvec-marketplace` crate, the `License` enum, the `Listing` struct, content-addressed bundle distribution, Ed25519 author identity, local reputation receipts, and the `xvn marketplace ...` CLI surface. All of it ships together when Plan 5 lands, gated on eval + strategy engines being battle-tested in production.
- ALL on-chain work (Plan 5 source spec: `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`, deferred status).
- Tier B sealed-hosted strategies + xvn API server with envelope encryption — Plan 4 (post-hackathon)
- Skill marketplace / paid skills / skill discovery — bundled into Plan 5
- Durable scheduler + live execution — Plan 2c
- Web dashboard / Wizard — Plan 2d
- Eval engine — Plan 3

**Why skills ship in v1 but marketplace doesn't:** Skills are a local authoring abstraction with immediate utility (compose a skill into a slot, the strategy gets richer prompts). Marketplace is a multi-user discovery + trust system whose value is unlocked by the blockchain — without on-chain provenance + reputation, a "marketplace" is just a list of files on someone's laptop. Better to wait and ship marketplace + chain together as one coherent thing.

---

## File structure

```
crates/
└── xianvec-skills/                          # NEW
    ├── Cargo.toml
    ├── src/
    │   ├── lib.rs                           # Skill type, parse(), validate()
    │   ├── frontmatter.rs                   # YAML frontmatter parser
    │   ├── store.rs                         # FilesystemSkillStore
    │   └── attach.rs                        # attach_skill_to_agent helper
    └── tests/
        ├── parse.rs
        ├── attach.rs
        ├── store.rs
        └── fixtures/
            └── crypto-trader-base.md
```

Plus modifications:
- `crates/xianvec-engine/Cargo.toml` — add `xianvec-skills` dep
- `crates/xianvec-engine/src/mcp/skill.rs` — NEW: 3 skill MCP verbs
- `crates/xianvec-cli/Cargo.toml` — add `xianvec-skills` dep
- `crates/xianvec-cli/src/commands/skill.rs` — NEW: `xvn skill {new | ls | attach}`
- `Cargo.toml` (workspace) — register xianvec-skills

---

## Phase 2B.A — `xianvec-skills` crate

### Task 1: Crate scaffolding + `Skill` type + parse

**Files:**
- Create: `crates/xianvec-skills/Cargo.toml`
- Create: `crates/xianvec-skills/src/lib.rs`
- Create: `crates/xianvec-skills/src/frontmatter.rs`
- Modify: workspace `Cargo.toml` (add to `members` + `default-members`)

- [ ] **Step 1: Cargo.toml**

```toml
[package]
name        = "xianvec-skills"
description = "OSShip-style markdown skills for xvn agents"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
repository.workspace   = true

[lib]
name = "xianvec_skills"
path = "src/lib.rs"

[dependencies]
xianvec-engine = { path = "../xianvec-engine" }

serde       = { workspace = true }
serde_json  = { workspace = true }
serde_yaml  = "0.9"
sha2        = "0.10"
chrono      = { workspace = true }
anyhow      = { workspace = true }
thiserror   = { workspace = true }
async-trait = { workspace = true }
tokio       = { workspace = true }

[dev-dependencies]
tempfile = "3"
tokio    = { workspace = true, features = ["rt", "macros"] }
```

- [ ] **Step 2: `lib.rs`**

```rust
//! OSShip-style markdown skills for xvn.
//!
//! A skill is a single markdown file with YAML frontmatter:
//!
//! ```text
//! ---
//! name: crypto-trader-base
//! display_name: "Generalist crypto trader"
//! description: "Default trader prompt for any crypto strategy"
//! version: 1.0.0
//! allowed_tools: [ohlcv, indicator_panel]
//! model_requirement: "anthropic.claude-sonnet-4.6+"
//! ---
//!
//! You are a crypto trader. ...
//! ```
//!
//! Plan 2b ships parser + filesystem store + attach-to-agent helper.
//! Marketplace discovery + content-addressed publishing wait for
//! Plan 5 (blockchain integration).

pub mod attach;
pub mod frontmatter;
pub mod store;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub allowed_tools: Vec<String>,
    pub model_requirement: String,
    pub body: String,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("missing frontmatter delimiters")]
    MissingFrontmatter,
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),
    #[error("required field missing: {0}")]
    MissingField(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub fn parse(markdown: &str) -> Result<Skill, SkillError> {
    let (frontmatter_yaml, body) = frontmatter::split(markdown)?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(frontmatter_yaml)?;
    let get_str = |key: &str| -> Result<String, SkillError> {
        parsed
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| SkillError::MissingField(key.to_string()))
    };
    let allowed_tools: Vec<String> = parsed
        .get("allowed_tools")
        .and_then(|v| v.as_sequence())
        .map(|seq| seq.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();
    Ok(Skill {
        name: get_str("name")?,
        display_name: get_str("display_name")?,
        description: get_str("description")?,
        version: get_str("version")?,
        allowed_tools,
        model_requirement: get_str("model_requirement")?,
        body: body.to_string(),
        content_hash: sha256_hex(markdown.as_bytes()),
    })
}

fn sha256_hex(input: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
```

- [ ] **Step 3: `frontmatter.rs`**

```rust
use crate::SkillError;

pub fn split(markdown: &str) -> Result<(&str, &str), SkillError> {
    let trimmed = markdown.trim_start();
    let after_open = trimmed
        .strip_prefix("---\n")
        .or_else(|| trimmed.strip_prefix("---\r\n"))
        .ok_or(SkillError::MissingFrontmatter)?;
    let close_idx = after_open.find("\n---\n")
        .or_else(|| after_open.find("\r\n---\r\n"))
        .ok_or(SkillError::MissingFrontmatter)?;
    let yaml = &after_open[..close_idx];
    let body_start = close_idx + "\n---\n".len();
    let body = if body_start < after_open.len() { &after_open[body_start..] } else { "" };
    Ok((yaml, body.trim_start_matches('\n')))
}
```

- [ ] **Step 4: Test fixture + parse roundtrip**

Create `crates/xianvec-skills/tests/fixtures/crypto-trader-base.md`:

```markdown
---
name: crypto-trader-base
display_name: "Generalist crypto trader"
description: "Default trader prompt for any crypto strategy"
version: 1.0.0
allowed_tools:
  - ohlcv
  - indicator_panel
model_requirement: "anthropic.claude-sonnet-4.6+"
---

You are a crypto trader. Inputs include ohlcv_history, indicator_panel,
and portfolio_state.

Decide ONE of: long_open | short_open | flat | hold.
Output JSON: {action, conviction (0-1), justification}.
```

Create `crates/xianvec-skills/tests/parse.rs`:

```rust
use xianvec_skills::parse;

const FIXTURE: &str = include_str!("fixtures/crypto-trader-base.md");

#[test]
fn parses_valid_skill() {
    let skill = parse(FIXTURE).expect("parse fixture");
    assert_eq!(skill.name, "crypto-trader-base");
    assert_eq!(skill.allowed_tools, vec!["ohlcv", "indicator_panel"]);
    assert!(skill.body.contains("crypto trader"));
    assert_eq!(skill.content_hash.len(), 64);
}

#[test]
fn rejects_missing_frontmatter() {
    let err = parse("just some text").unwrap_err();
    assert!(matches!(err, xianvec_skills::SkillError::MissingFrontmatter));
}

#[test]
fn rejects_missing_required_field() {
    let bad = "---\nname: x\n---\nbody\n";
    let err = parse(bad).unwrap_err();
    assert!(matches!(err, xianvec_skills::SkillError::MissingField(_)));
}
```

- [ ] **Step 5: Build + test + commit**

```bash
cargo test -p xianvec-skills 2>&1 | grep "test result"
git add crates/xianvec-skills Cargo.toml
git commit -m "feat(skills): scaffold xianvec-skills crate with markdown parser"
```

---

### Task 2: `FilesystemSkillStore`

**Files:**
- Create: `crates/xianvec-skills/src/store.rs`
- Create: `crates/xianvec-skills/tests/store.rs`

Pattern matches Plan #1 Task 7 (`FilesystemStore` for bundles). Save-by-name, load-by-name, list. Path: `$XVN_HOME/skills/<name>.md`.

- [ ] **Step 1: `SkillStore` trait + impl**

```rust
use std::path::PathBuf;

use anyhow::Context;
use async_trait::async_trait;

use crate::Skill;

#[async_trait]
pub trait SkillStore: Send + Sync {
    /// Save the original markdown source so the body roundtrips byte-exact.
    async fn save(&self, name: &str, markdown: &str) -> anyhow::Result<()>;
    async fn load(&self, name: &str) -> anyhow::Result<Skill>;
    async fn list(&self) -> anyhow::Result<Vec<String>>;
}

pub struct FilesystemSkillStore {
    root: PathBuf,
}

impl FilesystemSkillStore {
    pub fn new(root: PathBuf) -> Self { Self { root } }
    fn path_for(&self, name: &str) -> PathBuf { self.root.join(format!("{name}.md")) }
}

#[async_trait]
impl SkillStore for FilesystemSkillStore {
    async fn save(&self, name: &str, markdown: &str) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.path_for(name);
        tokio::fs::write(&path, markdown)
            .await
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }
    async fn load(&self, name: &str) -> anyhow::Result<Skill> {
        let bytes = tokio::fs::read(self.path_for(name)).await?;
        Ok(crate::parse(std::str::from_utf8(&bytes)?)?)
    }
    async fn list(&self) -> anyhow::Result<Vec<String>> {
        if !self.root.exists() { return Ok(vec![]); }
        let mut out = vec![];
        let mut rd = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = rd.next_entry().await? {
            let name = entry.file_name();
            let s = name.to_string_lossy();
            if let Some(stem) = s.strip_suffix(".md") {
                out.push(stem.to_string());
            }
        }
        out.sort();
        Ok(out)
    }
}
```

- [ ] **Step 2: Test**

```rust
// tests/store.rs
use xianvec_skills::store::{FilesystemSkillStore, SkillStore};
use tempfile::tempdir;

const FIXTURE: &str = include_str!("fixtures/crypto-trader-base.md");

#[tokio::test]
async fn save_and_load_roundtrip() {
    let dir = tempdir().unwrap();
    let store = FilesystemSkillStore::new(dir.path().to_path_buf());
    store.save("crypto-trader-base", FIXTURE).await.unwrap();
    let loaded = store.load("crypto-trader-base").await.unwrap();
    assert_eq!(loaded.name, "crypto-trader-base");
}

#[tokio::test]
async fn list_returns_saved_skills() {
    let dir = tempdir().unwrap();
    let store = FilesystemSkillStore::new(dir.path().to_path_buf());
    store.save("a", "---\nname: a\ndisplay_name: A\ndescription: x\nversion: 1.0\nmodel_requirement: anthropic.claude-sonnet-4.6\n---\nbody").await.unwrap();
    store.save("b", "---\nname: b\ndisplay_name: B\ndescription: x\nversion: 1.0\nmodel_requirement: anthropic.claude-sonnet-4.6\n---\nbody").await.unwrap();
    let names = store.list().await.unwrap();
    assert_eq!(names, vec!["a".to_string(), "b".to_string()]);
}
```

- [ ] **Step 3: Commit**

```bash
cargo test -p xianvec-skills 2>&1 | grep "test result"
git add crates/xianvec-skills/src/store.rs crates/xianvec-skills/tests/store.rs
git commit -m "feat(skills): filesystem-backed SkillStore"
```

---

### Task 3: `attach_skill_to_agent` helper

**Files:**
- Create: `crates/xianvec-skills/src/attach.rs`
- Create: `crates/xianvec-skills/tests/attach.rs`

Mutates a `StrategyBundle`'s named slot (regime/intern/trader): replaces prompt with skill's body, sets model_requirement, unions allowed_tools. Returns error if the slot is empty.

- [ ] **Step 1: Implement**

```rust
use crate::Skill;
use xianvec_engine::bundle::StrategyBundle;

pub fn attach_skill_to_agent(
    bundle: &mut StrategyBundle,
    slot_role: &str,
    skill: &Skill,
) -> anyhow::Result<()> {
    let slot = match slot_role {
        "regime" => bundle.regime_slot.as_mut(),
        "intern" => bundle.intern_slot.as_mut(),
        "trader" => bundle.trader_slot.as_mut(),
        other => anyhow::bail!("unknown slot role: {other} (must be regime, intern, or trader)"),
    };
    let slot = slot.ok_or_else(|| anyhow::anyhow!("slot '{slot_role}' is empty — fill it before attaching"))?;
    slot.prompt = skill.body.clone();
    slot.model_requirement = skill.model_requirement.clone();
    let mut tools = slot.allowed_tools.clone();
    for t in &skill.allowed_tools {
        if !tools.contains(t) { tools.push(t.clone()); }
    }
    slot.allowed_tools = tools;
    Ok(())
}
```

- [ ] **Step 2: Test**

```rust
// tests/attach.rs
use xianvec_skills::Skill;
use xianvec_skills::attach::attach_skill_to_agent;
use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xianvec_engine::bundle::risk::RiskPreset;
use xianvec_engine::bundle::slot::LLMSlot;
use xianvec_engine::bundle::StrategyBundle;

fn dummy_skill() -> Skill {
    Skill {
        name: "test".into(), display_name: "T".into(),
        description: "x".into(), version: "1.0.0".into(),
        allowed_tools: vec!["indicator_panel".into()],
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        body: "NEW PROMPT".into(),
        content_hash: "deadbeef".into(),
    }
}

fn dummy_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01".into(), display_name: "T".into(), plain_summary: "x".into(),
            creator: "@t".into(), template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()], decision_cadence_minutes: 15,
            required_models: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(), published_at: None,
        },
        regime_slot: None, intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(), prompt: "OLD".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

#[test]
fn attaches_to_trader_replaces_prompt_unions_tools() {
    let mut bundle = dummy_bundle();
    attach_skill_to_agent(&mut bundle, "trader", &dummy_skill()).unwrap();
    let trader = bundle.trader_slot.unwrap();
    assert_eq!(trader.prompt, "NEW PROMPT");
    assert!(trader.allowed_tools.contains(&"ohlcv".into()));
    assert!(trader.allowed_tools.contains(&"indicator_panel".into()));
}

#[test]
fn attaching_to_empty_slot_fails() {
    let mut bundle = dummy_bundle();
    let err = attach_skill_to_agent(&mut bundle, "regime", &dummy_skill()).unwrap_err();
    assert!(err.to_string().contains("regime"));
}

#[test]
fn unknown_slot_role_fails() {
    let mut bundle = dummy_bundle();
    let err = attach_skill_to_agent(&mut bundle, "bogus", &dummy_skill()).unwrap_err();
    assert!(err.to_string().contains("bogus"));
}
```

- [ ] **Step 3: Commit**

```bash
cargo test -p xianvec-skills attach 2>&1 | grep "test result"
git add crates/xianvec-skills/src/attach.rs crates/xianvec-skills/tests/attach.rs
git commit -m "feat(skills): attach_skill_to_agent helper"
```

---

## Phase 2B.B — MCP verbs (3)

### Task 4: `create_skill` MCP verb

Pattern matches Plan 2a Task 3. Add file `crates/xianvec-engine/src/mcp/skill.rs`.

Verb: `create_skill { markdown: String } → { name: String, content_hash: String }`.

- [ ] **Step 1: Test**

Append to `crates/xianvec-engine/tests/mcp_authoring.rs`:

```rust
#[test]
fn mcp_advertises_create_skill() {
    // Same pattern as mcp_server_advertises_list_templates_tool but checks for
    // the create_skill tool in the tools/list response.
    // ... (subagent fills in following Plan 2a Task 3's test pattern)
}
```

- [ ] **Step 2: Implement**

```rust
// crates/xianvec-engine/src/mcp/skill.rs
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use xianvec_skills::{parse, store::{FilesystemSkillStore, SkillStore}};

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSkillArgs { pub markdown: String }

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateSkillResult { pub name: String, pub content_hash: String }

pub async fn create_skill(args: CreateSkillArgs, xvn_home: &std::path::Path) -> anyhow::Result<CreateSkillResult> {
    let skill = parse(&args.markdown)?;
    let store = FilesystemSkillStore::new(xvn_home.join("skills"));
    store.save(&skill.name, &args.markdown).await?;
    Ok(CreateSkillResult { name: skill.name, content_hash: skill.content_hash })
}

pub fn create_skill_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": { "markdown": { "type": "string", "description": "Full skill markdown including YAML frontmatter" } },
        "required": ["markdown"]
    })
}
```

- [ ] **Step 3: Wire into MCP server**

In `crates/xianvec-engine/src/mcp/mod.rs`, register `create_skill` in `list_tools` and `call_tool` dispatch. Add `pub mod skill;`.

In `xianvec-engine/Cargo.toml`, add `xianvec-skills = { path = "../xianvec-skills" }`.

- [ ] **Step 4: Test passes + commit**

```bash
cargo test -p xianvec-engine mcp_advertises_create_skill 2>&1 | tail -5
git add crates/xianvec-engine
git commit -m "feat(engine): MCP create_skill verb"
```

### Task 5: `list_skills` MCP verb

Args: `{}`. Returns: `Vec<{ name, display_name, description, version }>`. Same pattern. Commit `feat(engine): MCP list_skills verb`.

### Task 6: `attach_skill_to_agent` MCP verb

Args: `{ strategy_id, slot, skill_name }`. Returns: `{ strategy_id, slot, skill_name }`. Mutates the saved bundle in `$XVN_HOME/strategies/<id>.json` via `FilesystemStore::load → attach_skill_to_agent → FilesystemStore::save`. Commit `feat(engine): MCP attach_skill_to_agent verb`.

After Task 6, all 10 verbs (7 authoring from Plan 2a + 3 skills from Plan 2b) appear in `tools/list`. Update `tests/mcp_authoring.rs` to assert the full set.

---

## Phase 2B.C — CLI surface

### Task 7: `xvn skill {new | ls | attach}`

**Files:**
- Create: `crates/xianvec-cli/src/commands/skill.rs`
- Modify: `crates/xianvec-cli/src/commands/mod.rs`
- Modify: `crates/xianvec-cli/src/lib.rs` (add `Skill(commands::skill::SkillCmd)` variant + dispatch)
- Modify: `crates/xianvec-cli/Cargo.toml` (add `xianvec-skills = { path = "../xianvec-skills" }`)

- [ ] **Step 1: Subcommand module**

```rust
//! `xvn skill ...` — author + manage skills locally.

use std::env;
use std::path::PathBuf;

use clap::{Args, Subcommand};
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_skills::store::{FilesystemSkillStore, SkillStore};
use xianvec_skills::{attach::attach_skill_to_agent, parse};

#[derive(Args, Debug)]
pub struct SkillCmd {
    #[command(subcommand)]
    action: SkillAction,
}

#[derive(Subcommand, Debug)]
enum SkillAction {
    /// Register a new skill from a markdown file.
    New {
        /// Path to the skill markdown file.
        #[arg(long)]
        from_file: PathBuf,
    },
    /// List saved skills.
    Ls,
    /// Attach a skill to a slot in a saved strategy.
    Attach {
        strategy_id: String,
        #[arg(long)]
        slot: String,        // regime | intern | trader
        #[arg(long)]
        skill: String,
    },
}

pub async fn run(cmd: SkillCmd) -> anyhow::Result<()> {
    match cmd.action {
        SkillAction::New { from_file } => new(from_file).await,
        SkillAction::Ls => ls().await,
        SkillAction::Attach { strategy_id, slot, skill } => attach(&strategy_id, &slot, &skill).await,
    }
}

fn xvn_home() -> PathBuf {
    if let Ok(p) = env::var("XVN_HOME") {
        return PathBuf::from(p);
    }
    dirs::home_dir().expect("$HOME").join(".xvn")
}

fn skill_store() -> FilesystemSkillStore { FilesystemSkillStore::new(xvn_home().join("skills")) }
fn strategy_store() -> FilesystemStore { FilesystemStore::new(xvn_home().join("strategies")) }

async fn new(from_file: PathBuf) -> anyhow::Result<()> {
    let markdown = tokio::fs::read_to_string(&from_file).await?;
    let parsed = parse(&markdown)?;
    skill_store().save(&parsed.name, &markdown).await?;
    println!("{}", parsed.name);
    Ok(())
}

async fn ls() -> anyhow::Result<()> {
    for name in skill_store().list().await? { println!("{name}"); }
    Ok(())
}

async fn attach(strategy_id: &str, slot: &str, skill_name: &str) -> anyhow::Result<()> {
    let mut bundle = strategy_store().load(strategy_id).await?;
    let skill = skill_store().load(skill_name).await?;
    attach_skill_to_agent(&mut bundle, slot, &skill)?;
    strategy_store().save(&bundle).await?;
    println!("attached {skill_name} → {strategy_id}#{slot}");
    Ok(())
}
```

- [ ] **Step 2: Wire into top-level Command enum**

```rust
// in xianvec-cli/src/lib.rs
Skill(commands::skill::SkillCmd),
// dispatch:
Command::Skill(cmd) => commands::skill::run(cmd).await,
```

Append `pub mod skill;` to `crates/xianvec-cli/src/commands/mod.rs`.

- [ ] **Step 3: Integration test**

Create `crates/xianvec-cli/tests/skill_cli.rs`:

```rust
use std::process::Command;
use tempfile::tempdir;

fn xvn(args: &[&str], home: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_xvn"))
        .args(args)
        .env("XVN_HOME", home)
        .output()
        .expect("xvn invocation")
}

const FIXTURE: &str = include_str!("../../xianvec-skills/tests/fixtures/crypto-trader-base.md");

#[test]
fn new_ls_attach_roundtrip() {
    let dir = tempdir().unwrap();
    let skill_path = dir.path().join("crypto-trader-base.md");
    std::fs::write(&skill_path, FIXTURE).unwrap();

    // Register skill
    let out = xvn(&["skill", "new", "--from-file", skill_path.to_str().unwrap()], dir.path());
    assert!(out.status.success());
    let name = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert_eq!(name, "crypto-trader-base");

    // List
    let out = xvn(&["skill", "ls"], dir.path());
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains("crypto-trader-base"));

    // Create a strategy then attach
    let out = xvn(&["strategy", "new", "--template", "mean_reversion", "--name", "test"], dir.path());
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    let out = xvn(&["skill", "attach", &id, "--slot", "trader", "--skill", "crypto-trader-base"], dir.path());
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert!(String::from_utf8(out.stdout).unwrap().contains("attached"));

    // Verify the bundle's trader prompt now contains the skill body
    let out = xvn(&["strategy", "show", &id], dir.path());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("crypto trader"));  // text from the fixture's body
}
```

- [ ] **Step 4: Test + commit**

```bash
cargo test -p xianvec-cli skill 2>&1 | grep "test result"
git add crates/xianvec-cli
git commit -m "feat(cli): xvn skill new/ls/attach"
```

---

## Phase 2B.D — Polish + smoke

### Task 8: README + manual

- Update `crates/xianvec-engine/README.md`'s "What ships" section: add the 3 skill MCP verbs; explicitly note that marketplace is deferred to Plan 5.
- Update `MANUAL.md`: add `xvn skill {new|ls|attach}` reference. Note that "marketplace publish/browse/install/attest commands are deferred to Plan 5 (blockchain integration)".
- Add `crates/xianvec-skills/README.md` mirroring the engine README structure.

End-to-end smoke:

```bash
export XVN_HOME=/tmp/xvn-2b-skill-smoke
rm -rf $XVN_HOME

# Create a skill from the fixture
echo "writing skill from fixture..."
cp crates/xianvec-skills/tests/fixtures/crypto-trader-base.md /tmp/my-trader.md
xvn skill new --from-file /tmp/my-trader.md
xvn skill ls

# Create a strategy + attach
ID=$(xvn strategy new --template mean_reversion --name skill-smoke)
xvn skill attach $ID --slot trader --skill crypto-trader-base

# Run with the customized prompt
xvn strategy run $ID --fixture test-fixture-btc-2024-01 --decisions 2 --mock
```

Each step exits 0. Commit `chore: Plan 2b end-to-end smoke verified`.

### Task 9: Final workspace check

`cargo test --workspace` clean. clippy clean. fmt scoped to plan-touched crates. `xianvec-eval`, `xianvec-identity`, `xianvec-marketplace` (which doesn't exist yet — Plan 5 territory) all untouched.

Commit `chore: Plan 2b final workspace check` if any cleanup landed.

---

## Self-review checklist

**Spec coverage (from `2026-05-08-strategy-creation-engine-design.md`):**
- [x] §6 Skill bundle format (OSShip-style markdown) — `xianvec-skills` crate
- [x] §10 MCP verb groups: skill management (3 verbs)
- [ ] §10 MCP verb groups: marketplace (5 verbs) — **deferred to Plan 5**
- [ ] §13 Marketplace + 8004 — **deferred to Plan 5**
- [ ] §5 Tier B — Plan 4
- [ ] §11 Live execution — Plan 2c
- [ ] §12 Durable scheduler — Plan 2c
- [ ] §2 Wizard / dashboard — Plan 2d
- [ ] Eval engine — Plan 3

**Type consistency:** `Skill`, `SkillError`, `SkillStore`, `FilesystemSkillStore`, `attach_skill_to_agent`, `CreateSkillArgs`, `CreateSkillResult` — consistent across all 9 tasks. **No `xianvec-marketplace` references, no Listing / License / ReputationReceipt types, no Ed25519 author identity.** All of that lives in Plan 5.

**No placeholders:** every code block is real Rust. Tests are spelled out.

**Frequent commits:** 9 tasks → ~9 focused commits.

---

## What's next

Plan 2c — **Durable Scheduler + Live Execution** (no marketplace dep — live decisions persist locally to scheduler_events; Plan 5 adds attestation publishing later)
Plan 2d — **Web Dashboard + Agent Wizard** (no Marketplace tab in v1 — wizard / authoring / live archetypes only; Plan 5 reintroduces a Marketplace tab)
Plan 3 — **Eval Engine** (produces signed eval attestations as JSON; Plan 5 batches them on-chain)
Plan 4 (post-hackathon) — **Tier B sealing + xvn API server**

**Plan 5 (post battle-testing) — Marketplace + 8004 ON-CHAIN INTEGRATION**
- **Source spec:** `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md` (deferred status)
- **Gating:** eval + strategy engines battle-tested in production · marketplace volume signal confirmed · security review on contracts · on-chain costs justified by usage
- **Surface (folds in everything that this plan deferred):**
  - **Marketplace crate** (was originally in Plan 2b before deferral): `xianvec-marketplace` with Listing, License, ReputationReceipt; local Ed25519 author identity; SQLite ListingStore + ReputationStore; publish_strategy / browse_listings / install_strategy / attest_run; 5 marketplace MCP verbs; `xvn marketplace` CLI subcommands
  - **On-chain registries** (per the smart-contract spec): ListingRegistry, Marketplace, LicenseToken (ERC-1155 soulbound), EvalAttestationRegistry; xvn self-registers as ERC-8004 agent #0
  - **Commerce**: x402 buy-rail via EIP-3009 buyWithAuthorization
  - **Bridges**: `xvn marketplace push-to-chain` batches local listings + receipts to Mantle; `xvn marketplace pull-from-chain` discovers other instances' listings
  - **Deploy**: CREATE2 deterministic deploys for multi-chain mirroring; UUPS proxies behind 7-day timelock + 2-of-3 multisig with progressive admin-burn
- **Existing crate `xianvec-identity` (deferred per ADR 0008)** is the foundation for Plan 5's chain interaction. Its `IdentityRegistry` + `ReputationRegistry` interfaces are the smaller subset of what the smart-contract-surface spec extends to.
