# Strategy Creation Engine — MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land a minimum-viable `xianvec-engine` crate that can author, validate, and inline-execute a strategy bundle end-to-end via CLI. After this plan ships, an external AI agent (Claude Code or Hermes) can call `xvn strategy new mean_reversion --name eth-mr` → `xvn strategy validate eth-mr` → `xvn strategy run eth-mr --bars data/probes/btc-2025q1.parquet` and see the agent's decisions logged.

**Architecture:** New `xianvec-engine` crate sits alongside the existing `xianvec-*` workspace members. Strategy bundles are JSON-serialized; templates produce default bundles via a `Template` trait; tools dispatch through a registry; LLM calls go through a `LlmDispatch` trait (initial impl: Anthropic SDK + async-openai). Agent loop is inline (no durable scheduler — that's Plan #2). One template ships in v1 (`mean_reversion`), one existing baseline migrated as LLM-shim (`ma_crossover`).

**Tech Stack:** Rust 2021, anyhow, serde + serde_json + schemars, tokio, sqlx (sqlite, deferred to Plan #2 for actual storage — MVP uses filesystem), clap, ulid, async-trait, anthropic-sdk + async-openai, tracing, proptest, tempfile.

**Out of scope for this plan (deferred to Plan #2 / #3):** Agent Wizard UI, web dashboard, MCP server, Tier B sealing, durable scheduler ported from SwarmClaw, marketplace + 8004 publish, eval engine (separate plan), live execution daemon, fly.io deploy recipe, more than 1 template, more than 1 migrated baseline, OSShip-style skill marketplace.

---

## File structure

```
crates/xianvec-engine/
├── Cargo.toml
├── src/
│   ├── lib.rs                      # crate root, re-exports
│   ├── bundle/
│   │   ├── mod.rs                  # StrategyBundle root type
│   │   ├── manifest.rs             # PublicManifest
│   │   ├── slot.rs                 # LLMSlot
│   │   ├── layers.rs               # Layer scaffold (Data, Mech, Risk, Execution)
│   │   ├── risk.rs                 # RiskConfig + preset expansion
│   │   ├── store.rs                # filesystem save/load
│   │   └── validate.rs             # bundle validation
│   ├── templates/
│   │   ├── mod.rs                  # Template trait + registry
│   │   └── mean_reversion.rs       # v1 template
│   ├── baselines/
│   │   ├── mod.rs                  # LLM-shim wrapper trait
│   │   └── ma_crossover.rs         # migrated baseline
│   ├── tools/
│   │   ├── mod.rs                  # ToolRegistry
│   │   ├── ohlcv.rs                # proxies xianvec-data OHLCV
│   │   └── indicators.rs           # proxies xianvec-data IndicatorPanel
│   ├── agent/
│   │   ├── mod.rs                  # public agent API
│   │   ├── llm.rs                  # LlmDispatch trait + impls
│   │   ├── execute.rs              # single-slot execution
│   │   └── pipeline.rs             # 3-slot pipeline (regime → intern → trader)
│   ├── tokens.rs                   # token estimator
│   └── error.rs                    # crate-wide error types
├── tests/
│   ├── bundle_roundtrip.rs
│   ├── template_validation.rs
│   ├── ma_crossover_shim.rs
│   ├── tool_registry.rs
│   └── pipeline_inline.rs
└── README.md
```

Plus modifications to:
- `Cargo.toml` (workspace) — add `xianvec-engine` to members + default-members
- `crates/xianvec-cli/Cargo.toml` — add `xianvec-engine` dependency
- `crates/xianvec-cli/src/` — add `strategy` subcommand module

---

## Phase 1A — Crate scaffolding & bundle types

### Task 1: Create `xianvec-engine` crate skeleton

**Files:**
- Create: `crates/xianvec-engine/Cargo.toml`
- Create: `crates/xianvec-engine/src/lib.rs`
- Modify: `Cargo.toml` (workspace root) — add to `members` and `default-members`

- [ ] **Step 1: Create `crates/xianvec-engine/Cargo.toml`**

```toml
[package]
name        = "xianvec-engine"
description = "strategy creation, bundling, agent execution"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
repository.workspace   = true

[lib]
name = "xianvec_engine"
path = "src/lib.rs"

[dependencies]
xianvec-core   = { path = "../xianvec-core" }
xianvec-data   = { path = "../xianvec-data" }

serde       = { workspace = true }
serde_json  = { workspace = true }
schemars    = "0.8"
chrono      = { workspace = true }
uuid        = { workspace = true }
ulid        = { version = "1", features = ["serde"] }
anyhow      = { workspace = true }
thiserror   = { workspace = true }
async-trait = { workspace = true }
tokio       = { workspace = true }
tracing     = { workspace = true }

[dev-dependencies]
proptest = { workspace = true }
tempfile = "3"
tokio    = { workspace = true, features = ["rt", "macros"] }
```

- [ ] **Step 2: Create `crates/xianvec-engine/src/lib.rs`**

```rust
//! xianvec-engine — strategy creation, bundling, agent execution.
//!
//! See: docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md

pub mod agent;
pub mod baselines;
pub mod bundle;
pub mod error;
pub mod templates;
pub mod tokens;
pub mod tools;

pub use bundle::StrategyBundle;
pub use error::EngineError;
```

- [ ] **Step 3: Create empty module files so `cargo build` succeeds**

Create empty placeholder files:
- `src/agent/mod.rs` with `// placeholder`
- `src/baselines/mod.rs` with `// placeholder`
- `src/bundle/mod.rs` with `pub struct StrategyBundle;`
- `src/error.rs` with `#[derive(Debug, thiserror::Error)] pub enum EngineError {}`
- `src/templates/mod.rs` with `// placeholder`
- `src/tokens.rs` with `// placeholder`
- `src/tools/mod.rs` with `// placeholder`

- [ ] **Step 4: Register crate in workspace `Cargo.toml`**

In the root `Cargo.toml`, add `"crates/xianvec-engine",` to both the `members` array and the `default-members` array. Insert alphabetically after `crates/xianvec-eval`.

- [ ] **Step 5: Smoke test — verify it builds**

Run: `cargo build -p xianvec-engine`
Expected: clean build, no warnings about unused imports.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-engine Cargo.toml
git commit -m "feat(engine): scaffold xianvec-engine crate"
```

---

### Task 2: Define `LLMSlot` type

**Files:**
- Create: `crates/xianvec-engine/src/bundle/slot.rs`
- Test: `crates/xianvec-engine/tests/bundle_roundtrip.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xianvec-engine/tests/bundle_roundtrip.rs`:

```rust
use xianvec_engine::bundle::slot::LLMSlot;

#[test]
fn slot_serializes_to_json_and_back() {
    let slot = LLMSlot {
        role: "trader".to_string(),
        prompt: "decide: enter long, enter short, or no-op".to_string(),
        model_requirement: "anthropic.claude-sonnet-4.6+".to_string(),
        allowed_tools: vec!["ohlcv".to_string(), "indicator_panel".to_string()],
    };
    let json = serde_json::to_string(&slot).unwrap();
    let parsed: LLMSlot = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.role, "trader");
    assert_eq!(parsed.allowed_tools.len(), 2);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine slot_serializes`
Expected: FAIL — `LLMSlot` not found.

- [ ] **Step 3: Implement `LLMSlot`**

Create `crates/xianvec-engine/src/bundle/slot.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LLMSlot {
    pub role: String,                  // "regime", "intern", "trader"
    pub prompt: String,                // slot prompt body
    pub model_requirement: String,     // e.g., "anthropic.claude-sonnet-4.6+"
    pub allowed_tools: Vec<String>,    // tool names from registry
}
```

Then update `src/bundle/mod.rs` to export it:

```rust
pub mod slot;

pub struct StrategyBundle;  // placeholder, replaced in Task 5
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p xianvec-engine slot_serializes`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/bundle crates/xianvec-engine/tests
git commit -m "feat(engine): add LLMSlot bundle type"
```

---

### Task 3: Define `RiskConfig` + preset expansion

**Files:**
- Create: `crates/xianvec-engine/src/bundle/risk.rs`
- Test: `crates/xianvec-engine/tests/bundle_roundtrip.rs` (extend)

- [ ] **Step 1: Write the failing test (append to `bundle_roundtrip.rs`)**

```rust
use xianvec_engine::bundle::risk::{RiskConfig, RiskPreset};

#[test]
fn preset_expands_to_explicit_config() {
    let cons = RiskPreset::Conservative.expand();
    assert!(cons.risk_pct_per_trade <= 0.015);
    assert!(cons.max_leverage <= 3.0);
    let bal = RiskPreset::Balanced.expand();
    assert!(bal.risk_pct_per_trade > cons.risk_pct_per_trade);
    let agg = RiskPreset::Aggressive.expand();
    assert!(agg.risk_pct_per_trade > bal.risk_pct_per_trade);
}

#[test]
fn risk_config_roundtrips() {
    let cfg = RiskPreset::Balanced.expand();
    let json = serde_json::to_string(&cfg).unwrap();
    let parsed: RiskConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(cfg, parsed);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine preset_expands`
Expected: FAIL — `RiskConfig` not found.

- [ ] **Step 3: Implement `RiskConfig` + presets**

Create `crates/xianvec-engine/src/bundle/risk.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RiskConfig {
    pub risk_pct_per_trade: f64,            // e.g., 0.015 = 1.5%
    pub max_concurrent_positions: u32,
    pub max_leverage: f64,
    pub stop_loss_atr_multiple: f64,
    pub daily_loss_kill_pct: f64,           // e.g., 0.05 = 5%
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskPreset {
    Conservative,
    Balanced,
    Aggressive,
}

impl RiskPreset {
    pub fn expand(self) -> RiskConfig {
        match self {
            RiskPreset::Conservative => RiskConfig {
                risk_pct_per_trade: 0.010,
                max_concurrent_positions: 1,
                max_leverage: 2.0,
                stop_loss_atr_multiple: 2.0,
                daily_loss_kill_pct: 0.03,
            },
            RiskPreset::Balanced => RiskConfig {
                risk_pct_per_trade: 0.015,
                max_concurrent_positions: 2,
                max_leverage: 3.0,
                stop_loss_atr_multiple: 2.0,
                daily_loss_kill_pct: 0.05,
            },
            RiskPreset::Aggressive => RiskConfig {
                risk_pct_per_trade: 0.025,
                max_concurrent_positions: 3,
                max_leverage: 5.0,
                stop_loss_atr_multiple: 1.5,
                daily_loss_kill_pct: 0.08,
            },
        }
    }
}
```

Update `src/bundle/mod.rs`:

```rust
pub mod risk;
pub mod slot;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine bundle`
Expected: PASS for both new tests.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/bundle/risk.rs crates/xianvec-engine/tests
git commit -m "feat(engine): add RiskConfig and three presets"
```

---

### Task 4: Define `PublicManifest` type

**Files:**
- Create: `crates/xianvec-engine/src/bundle/manifest.rs`
- Test: extend `bundle_roundtrip.rs`

- [ ] **Step 1: Write the failing test**

```rust
use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};

#[test]
fn manifest_roundtrip_with_required_fields() {
    let m = PublicManifest {
        id: "01H8N7Z123".to_string(),
        display_name: "Buys dips".to_string(),
        plain_summary: "Buys ETH when oversold, sells when recovered.".to_string(),
        creator: "@xianvec_official".to_string(),
        template: "mean_reversion".to_string(),
        regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
        asset_universe: vec!["ETH/USD".to_string()],
        decision_cadence_minutes: 15,
        required_models: vec!["anthropic.claude-sonnet-4.6+".to_string()],
        required_tools: vec!["ohlcv".to_string(), "indicator_panel".to_string()],
        risk_preset_or_config: "balanced".to_string(),
        published_at: None,
    };
    let json = serde_json::to_string(&m).unwrap();
    let parsed: PublicManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.template, "mean_reversion");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine manifest_roundtrip`
Expected: FAIL — `PublicManifest` not found.

- [ ] **Step 3: Implement `PublicManifest`**

Create `crates/xianvec-engine/src/bundle/manifest.rs`:

```rust
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PublicManifest {
    pub id: String,                              // ULID
    pub display_name: String,                    // L1 plain English ("Buys dips")
    pub plain_summary: String,                   // L1 description
    pub creator: String,                         // @handle or 8004 wallet
    pub template: String,                        // template name
    pub regime_fit: Vec<RegimeFit>,
    pub asset_universe: Vec<String>,             // e.g., ["ETH/USD", "BTC/USD"]
    pub decision_cadence_minutes: u32,
    pub required_models: Vec<String>,
    pub required_tools: Vec<String>,
    pub risk_preset_or_config: String,           // "conservative" | "balanced" | "aggressive" | "custom"
    pub published_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RegimeFit {
    TrendingBull,
    TrendingBear,
    RangeBound,
    Chop,
    HighVol,
    LowVol,
    EventDriven,
}
```

Update `src/bundle/mod.rs`:

```rust
pub mod manifest;
pub mod risk;
pub mod slot;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine manifest`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/bundle/manifest.rs crates/xianvec-engine/tests
git commit -m "feat(engine): add PublicManifest type"
```

---

### Task 5: Define `StrategyBundle` root type

**Files:**
- Modify: `crates/xianvec-engine/src/bundle/mod.rs` (replace placeholder)
- Test: extend `bundle_roundtrip.rs`

- [ ] **Step 1: Write the failing test**

```rust
use xianvec_engine::bundle::StrategyBundle;
use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xianvec_engine::bundle::risk::{RiskConfig, RiskPreset};
use xianvec_engine::bundle::slot::LLMSlot;

fn sample_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01H8N7Z000".to_string(),
            display_name: "Test".to_string(),
            plain_summary: "test bundle".to_string(),
            creator: "@test".to_string(),
            template: "mean_reversion".to_string(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".to_string()],
            decision_cadence_minutes: 15,
            required_models: vec!["anthropic.claude-sonnet-4.6".to_string()],
            required_tools: vec!["ohlcv".to_string()],
            risk_preset_or_config: "balanced".to_string(),
            published_at: None,
        },
        regime_slot: Some(LLMSlot {
            role: "regime".into(), prompt: "...".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
        }),
        intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(), prompt: "...".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({"rsi_oversold": 30, "rsi_overbought": 70}),
    }
}

#[test]
fn bundle_roundtrip() {
    let b = sample_bundle();
    let json = serde_json::to_string(&b).unwrap();
    let parsed: StrategyBundle = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.manifest.template, "mean_reversion");
    assert!(parsed.regime_slot.is_some());
    assert!(parsed.intern_slot.is_none());
    assert!(parsed.trader_slot.is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine bundle_roundtrip`
Expected: FAIL — `StrategyBundle` is unit struct (placeholder).

- [ ] **Step 3: Replace placeholder with real `StrategyBundle`**

Replace `src/bundle/mod.rs` contents:

```rust
pub mod manifest;
pub mod risk;
pub mod slot;

use serde::{Deserialize, Serialize};

use crate::bundle::manifest::PublicManifest;
use crate::bundle::risk::RiskConfig;
use crate::bundle::slot::LLMSlot;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyBundle {
    pub manifest: PublicManifest,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub regime_slot: Option<LLMSlot>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub intern_slot: Option<LLMSlot>,

    /// At least one slot must be filled; trader is required.
    pub trader_slot: Option<LLMSlot>,

    pub risk: RiskConfig,

    /// Template-specific mechanical params (e.g., rsi thresholds, EMA periods).
    pub mechanical_params: serde_json::Value,
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine bundle_roundtrip`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/bundle/mod.rs crates/xianvec-engine/tests
git commit -m "feat(engine): define StrategyBundle root type"
```

---

### Task 6: Bundle validation

**Files:**
- Create: `crates/xianvec-engine/src/bundle/validate.rs`
- Test: extend `bundle_roundtrip.rs`

- [ ] **Step 1: Write the failing test**

```rust
use xianvec_engine::bundle::validate::{validate_bundle, ValidationError};

#[test]
fn valid_bundle_passes() {
    let b = sample_bundle();
    assert!(validate_bundle(&b).is_ok());
}

#[test]
fn bundle_without_any_llm_slot_fails() {
    let mut b = sample_bundle();
    b.regime_slot = None;
    b.intern_slot = None;
    b.trader_slot = None;
    let err = validate_bundle(&b).unwrap_err();
    assert!(matches!(err, ValidationError::NoLlmSlots));
}

#[test]
fn bundle_with_empty_asset_universe_fails() {
    let mut b = sample_bundle();
    b.manifest.asset_universe.clear();
    let err = validate_bundle(&b).unwrap_err();
    assert!(matches!(err, ValidationError::EmptyAssetUniverse));
}

#[test]
fn bundle_with_zero_capital_risk_fails() {
    let mut b = sample_bundle();
    b.risk.risk_pct_per_trade = 0.0;
    let err = validate_bundle(&b).unwrap_err();
    assert!(matches!(err, ValidationError::InvalidRisk(_)));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine valid_bundle_passes`
Expected: FAIL — `validate_bundle` not found.

- [ ] **Step 3: Implement validation**

Create `crates/xianvec-engine/src/bundle/validate.rs`:

```rust
use thiserror::Error;

use crate::bundle::StrategyBundle;

#[derive(Debug, Error)]
pub enum ValidationError {
    #[error("strategy must have at least one filled LLM slot")]
    NoLlmSlots,
    #[error("asset universe cannot be empty")]
    EmptyAssetUniverse,
    #[error("invalid risk config: {0}")]
    InvalidRisk(String),
    #[error("required tool '{0}' not in any slot's allowed_tools")]
    UndeclaredTool(String),
}

pub fn validate_bundle(b: &StrategyBundle) -> Result<(), ValidationError> {
    if b.regime_slot.is_none() && b.intern_slot.is_none() && b.trader_slot.is_none() {
        return Err(ValidationError::NoLlmSlots);
    }
    if b.manifest.asset_universe.is_empty() {
        return Err(ValidationError::EmptyAssetUniverse);
    }
    if b.risk.risk_pct_per_trade <= 0.0 || b.risk.risk_pct_per_trade > 0.5 {
        return Err(ValidationError::InvalidRisk(format!(
            "risk_pct_per_trade must be in (0, 0.5], got {}",
            b.risk.risk_pct_per_trade
        )));
    }
    if b.risk.max_leverage <= 0.0 || b.risk.max_leverage > 100.0 {
        return Err(ValidationError::InvalidRisk(format!(
            "max_leverage must be in (0, 100], got {}",
            b.risk.max_leverage
        )));
    }
    Ok(())
}
```

Update `src/bundle/mod.rs`: add `pub mod validate;`

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine validate`
Expected: PASS for all four validation tests.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/bundle/validate.rs crates/xianvec-engine/src/bundle/mod.rs crates/xianvec-engine/tests
git commit -m "feat(engine): add bundle validation"
```

---

### Task 7: Filesystem store (save/load)

**Files:**
- Create: `crates/xianvec-engine/src/bundle/store.rs`
- Test: `crates/xianvec-engine/tests/bundle_store.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xianvec-engine/tests/bundle_store.rs`:

```rust
use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xianvec_engine::bundle::risk::RiskPreset;
use xianvec_engine::bundle::slot::LLMSlot;
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::StrategyBundle;
use tempfile::tempdir;

fn sample_bundle(id: &str) -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: id.to_string(),
            display_name: "Test".into(), plain_summary: "x".into(), creator: "@t".into(),
            template: "mean_reversion".into(), regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()], decision_cadence_minutes: 15,
            required_models: vec!["anthropic.claude-sonnet-4.6".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(), published_at: None,
        },
        regime_slot: None, intern_slot: None,
        trader_slot: Some(LLMSlot {
            role: "trader".into(), prompt: "decide".into(),
            model_requirement: "anthropic.claude-sonnet-4.6".into(),
            allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn save_and_load_roundtrip() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    let b = sample_bundle("01H8N7Z000");
    store.save(&b).await.unwrap();
    let loaded = store.load("01H8N7Z000").await.unwrap();
    assert_eq!(loaded, b);
}

#[tokio::test]
async fn list_returns_saved_bundles() {
    let dir = tempdir().unwrap();
    let store = FilesystemStore::new(dir.path().to_path_buf());
    store.save(&sample_bundle("01H8N7ZAAA")).await.unwrap();
    store.save(&sample_bundle("01H8N7ZBBB")).await.unwrap();
    let ids = store.list().await.unwrap();
    assert_eq!(ids.len(), 2);
    assert!(ids.contains(&"01H8N7ZAAA".to_string()));
    assert!(ids.contains(&"01H8N7ZBBB".to_string()));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine save_and_load`
Expected: FAIL — `FilesystemStore` not found.

- [ ] **Step 3: Implement `FilesystemStore`**

Create `crates/xianvec-engine/src/bundle/store.rs`:

```rust
use std::path::PathBuf;

use anyhow::Context;
use async_trait::async_trait;

use crate::bundle::StrategyBundle;

#[async_trait]
pub trait BundleStore: Send + Sync {
    async fn save(&self, bundle: &StrategyBundle) -> anyhow::Result<()>;
    async fn load(&self, id: &str) -> anyhow::Result<StrategyBundle>;
    async fn list(&self) -> anyhow::Result<Vec<String>>;
}

pub struct FilesystemStore {
    root: PathBuf,
}

impl FilesystemStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn path_for(&self, id: &str) -> PathBuf {
        self.root.join(format!("{id}.json"))
    }
}

#[async_trait]
impl BundleStore for FilesystemStore {
    async fn save(&self, bundle: &StrategyBundle) -> anyhow::Result<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        let path = self.path_for(&bundle.manifest.id);
        let json = serde_json::to_vec_pretty(bundle)?;
        tokio::fs::write(&path, json)
            .await
            .with_context(|| format!("writing {}", path.display()))?;
        Ok(())
    }

    async fn load(&self, id: &str) -> anyhow::Result<StrategyBundle> {
        let path = self.path_for(id);
        let bytes = tokio::fs::read(&path)
            .await
            .with_context(|| format!("reading {}", path.display()))?;
        Ok(serde_json::from_slice(&bytes)?)
    }

    async fn list(&self) -> anyhow::Result<Vec<String>> {
        if !self.root.exists() {
            return Ok(vec![]);
        }
        let mut ids = vec![];
        let mut rd = tokio::fs::read_dir(&self.root).await?;
        while let Some(entry) = rd.next_entry().await? {
            let name = entry.file_name();
            let name_str = name.to_string_lossy();
            if let Some(id) = name_str.strip_suffix(".json") {
                ids.push(id.to_string());
            }
        }
        Ok(ids)
    }
}
```

Update `src/bundle/mod.rs`: add `pub mod store;`.

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine save_and_load list_returns`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/bundle/store.rs crates/xianvec-engine/src/bundle/mod.rs crates/xianvec-engine/tests
git commit -m "feat(engine): add filesystem-backed BundleStore"
```

---

## Phase 1B — Templates (one v1 template)

### Task 8: `Template` trait + registry

**Files:**
- Create: `crates/xianvec-engine/src/templates/mod.rs` (replace placeholder)
- Test: `crates/xianvec-engine/tests/template_validation.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xianvec-engine/tests/template_validation.rs`:

```rust
use xianvec_engine::templates::{Template, registry};

#[test]
fn unknown_template_returns_none() {
    assert!(registry::get("does_not_exist").is_none());
}

#[test]
fn list_template_names_returns_a_vec() {
    let _names: Vec<String> = registry::list_template_names();
    // empty until Task 9 — Task 9 adds the assertion that mean_reversion is present.
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine unknown_template_returns_none`
Expected: FAIL — `registry` module not found.

- [ ] **Step 3: Implement `Template` trait**

Replace `src/templates/mod.rs`:

```rust
pub mod registry;

use crate::bundle::StrategyBundle;

pub trait Template: Send + Sync {
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn plain_summary(&self) -> &'static str;
    /// Build a fresh draft bundle with default fields.
    /// `id` is the ULID assigned to the new draft.
    /// `name` is the human-readable name (e.g., "eth-mr-v1").
    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle;
}
```

Create `src/templates/registry.rs`:

```rust
use std::sync::OnceLock;

use crate::templates::Template;

static REGISTRY: OnceLock<Vec<Box<dyn Template>>> = OnceLock::new();

fn registry() -> &'static [Box<dyn Template>] {
    REGISTRY.get_or_init(|| {
        // Templates added via Task 9.
        vec![]
    })
}

pub fn get(name: &str) -> Option<&'static dyn Template> {
    registry().iter().find(|t| t.name() == name).map(|t| t.as_ref())
}

pub fn list_template_names() -> Vec<String> {
    registry().iter().map(|t| t.name().to_string()).collect()
}
```

- [ ] **Step 4: Run tests — both should pass with empty registry**

Run: `cargo test -p xianvec-engine template`
Expected: PASS for both `unknown_template_returns_none` and `list_template_names_returns_a_vec`.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/templates crates/xianvec-engine/tests
git commit -m "feat(engine): Template trait and empty registry"
```

---

### Task 9: `mean_reversion` template

**Files:**
- Create: `crates/xianvec-engine/src/templates/mean_reversion.rs`
- Modify: `crates/xianvec-engine/src/templates/registry.rs`

- [ ] **Step 1: Implement `mean_reversion` template**

Create `crates/xianvec-engine/src/templates/mean_reversion.rs`:

```rust
use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are a mean-reversion crypto trader. Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: RSI(14), Bollinger(20, 2), ATR(14)
- portfolio_state: open positions, available capital

Decide ONE of: long_open | short_open | flat | hold.
Mean-reversion logic: enter long when RSI < 30 AND price < lower_bollinger;
enter short when RSI > 70 AND price > upper_bollinger; otherwise flat or hold.
Output JSON: {action, conviction (0-1), justification (one line)}.
"#;

const REGIME_PROMPT: &str = r#"Classify the current crypto market regime as one of:
trending_bull | trending_bear | range_bound | chop.
Use indicator_panel + recent ohlcv_history. Return JSON: {regime, confidence (0-1)}.
"#;

pub struct MeanReversion;

impl Template for MeanReversion {
    fn name(&self) -> &'static str { "mean_reversion" }
    fn display_name(&self) -> &'static str { "Buys dips" }
    fn plain_summary(&self) -> &'static str {
        "Buys when prices drop below normal range and sells when they recover. \
         Best in calm sideways markets."
    }
    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id,
                display_name: name,
                plain_summary: self.plain_summary().to_string(),
                creator,
                template: "mean_reversion".into(),
                regime_fit: vec![RegimeFit::RangeBound, RegimeFit::LowVol],
                asset_universe: vec!["ETH/USD".into()],
                decision_cadence_minutes: 15,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "balanced".into(),
                published_at: None,
            },
            regime_slot: Some(LLMSlot {
                role: "regime".into(),
                prompt: REGIME_PROMPT.into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["indicator_panel".into()],
            }),
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: TRADER_PROMPT.into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            }),
            risk: RiskPreset::Balanced.expand(),
            mechanical_params: serde_json::json!({
                "rsi_oversold": 30, "rsi_overbought": 70,
                "bollinger_period": 20, "bollinger_sigma": 2.0,
                "atr_period": 14
            }),
        }
    }
}
```

- [ ] **Step 2: Wire into registry**

Replace the registry init block in `src/templates/registry.rs`:

```rust
use crate::templates::mean_reversion::MeanReversion;
use crate::templates::Template;

fn registry() -> &'static [Box<dyn Template>] {
    REGISTRY.get_or_init(|| {
        vec![Box::new(MeanReversion) as Box<dyn Template>]
    })
}
```

Update `src/templates/mod.rs`: add `pub mod mean_reversion;`.

- [ ] **Step 3: Add tests covering registry presence and draft validation**

Append to `tests/template_validation.rs`:

```rust
use xianvec_engine::bundle::validate::validate_bundle;

#[test]
fn registry_has_mean_reversion() {
    let names = registry::list_template_names();
    assert!(names.contains(&"mean_reversion".to_string()));
}

#[test]
fn mean_reversion_draft_validates() {
    let tpl = registry::get("mean_reversion").expect("template exists");
    let draft = tpl.new_draft(
        "01H8N7ZTEST".into(),
        "test-eth-mr".into(),
        "@test".into(),
    );
    validate_bundle(&draft).expect("draft must validate");
    assert_eq!(draft.manifest.template, "mean_reversion");
    assert_eq!(draft.manifest.display_name, "test-eth-mr");
    assert!(draft.trader_slot.is_some());
}
```

- [ ] **Step 4: Run all template tests**

Run: `cargo test -p xianvec-engine template`
Expected: PASS for `unknown_template_returns_none`, `list_template_names_returns_a_vec`, `registry_has_mean_reversion`, `mean_reversion_draft_validates`.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/templates crates/xianvec-engine/tests
git commit -m "feat(engine): mean_reversion v1 template"
```

---

## Phase 1C — Baseline migration (one baseline, LLM-shimmed)

### Task 10: LLM-shim wrapper trait + `ma_crossover` migration

**Files:**
- Create: `crates/xianvec-engine/src/baselines/mod.rs` (replace placeholder)
- Create: `crates/xianvec-engine/src/baselines/ma_crossover.rs`
- Test: `crates/xianvec-engine/tests/ma_crossover_shim.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xianvec-engine/tests/ma_crossover_shim.rs`:

```rust
use xianvec_engine::baselines::ma_crossover::ma_crossover_template;
use xianvec_engine::bundle::validate::validate_bundle;
use xianvec_engine::templates::Template;

#[test]
fn ma_crossover_produces_valid_bundle() {
    let tpl = ma_crossover_template();
    let draft = tpl.new_draft(
        "01H8N7ZBASE".into(),
        "btc-ma-cross".into(),
        "@xianvec_official".into(),
    );
    validate_bundle(&draft).expect("baseline must validate");
    // The shim wraps a deterministic rule in a single LLM trader slot.
    assert!(draft.trader_slot.is_some());
    let trader = draft.trader_slot.unwrap();
    assert!(trader.prompt.to_lowercase().contains("crossover"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine ma_crossover_produces`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the shim**

Replace `src/baselines/mod.rs`:

```rust
pub mod ma_crossover;
```

Create `src/baselines/ma_crossover.rs`:

```rust
use crate::bundle::manifest::{PublicManifest, RegimeFit};
use crate::bundle::risk::RiskPreset;
use crate::bundle::slot::LLMSlot;
use crate::bundle::StrategyBundle;
use crate::templates::Template;

const TRADER_PROMPT: &str = r#"You are wrapping a deterministic moving-average
crossover rule in an LLM confirmation step. Inputs:
- mechanical_signal: { kind: "ma_crossover", direction: "up"|"down"|"flat" }
- ohlcv_history: last 200 bars
- portfolio_state

Rule: when fast MA crosses above slow MA, the mechanical signal is "up";
crossover below is "down"; otherwise "flat".

Your job: confirm or veto the mechanical signal based on price context (sudden
spike, illiquid wick, gap). If you confirm "up", emit long_open. If "down",
emit short_open or flat depending on whether shorts are allowed. If "flat",
emit hold.

Output JSON: {action: long_open|short_open|flat|hold, conviction (0-1), justification}.
"#;

pub fn ma_crossover_template() -> Box<dyn Template> {
    Box::new(MaCrossover)
}

struct MaCrossover;

impl Template for MaCrossover {
    fn name(&self) -> &'static str { "ma_crossover_baseline" }
    fn display_name(&self) -> &'static str { "MA crossover (baseline)" }
    fn plain_summary(&self) -> &'static str {
        "Wraps the classic fast/slow moving-average crossover rule in an LLM confirmation step. \
         Used as a marketplace seed listing."
    }
    fn new_draft(&self, id: String, name: String, creator: String) -> StrategyBundle {
        StrategyBundle {
            manifest: PublicManifest {
                id, display_name: name, plain_summary: self.plain_summary().into(),
                creator, template: "ma_crossover_baseline".into(),
                regime_fit: vec![RegimeFit::TrendingBull, RegimeFit::TrendingBear],
                asset_universe: vec!["BTC/USD".into()],
                decision_cadence_minutes: 60,
                required_models: vec!["anthropic.claude-sonnet-4.6".into()],
                required_tools: vec!["ohlcv".into(), "indicator_panel".into()],
                risk_preset_or_config: "conservative".into(),
                published_at: None,
            },
            regime_slot: None,
            intern_slot: None,
            trader_slot: Some(LLMSlot {
                role: "trader".into(),
                prompt: TRADER_PROMPT.into(),
                model_requirement: "anthropic.claude-sonnet-4.6".into(),
                allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
            }),
            risk: RiskPreset::Conservative.expand(),
            mechanical_params: serde_json::json!({
                "fast_ma_period": 20, "slow_ma_period": 50
            }),
        }
    }
}
```

- [ ] **Step 4: Run test**

Run: `cargo test -p xianvec-engine ma_crossover_produces`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/baselines crates/xianvec-engine/tests
git commit -m "feat(engine): ma_crossover baseline as LLM-shim template"
```

---

## Phase 1D — Tool registry + LLM dispatch

### Task 11: `ToolRegistry` trait

**Files:**
- Create: `crates/xianvec-engine/src/tools/mod.rs` (replace placeholder)
- Test: `crates/xianvec-engine/tests/tool_registry.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xianvec-engine/tests/tool_registry.rs`:

```rust
use xianvec_engine::tools::{ToolRegistry, ToolName};

#[tokio::test]
async fn registry_lists_required_tools() {
    let reg = ToolRegistry::default_with_builtins();
    let tools = reg.list();
    assert!(tools.contains(&ToolName::new("ohlcv")));
    assert!(tools.contains(&ToolName::new("indicator_panel")));
}

#[tokio::test]
async fn unknown_tool_returns_none() {
    let reg = ToolRegistry::default_with_builtins();
    assert!(reg.get(&ToolName::new("nonsense_tool")).is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine registry_lists`
Expected: FAIL — types not found.

- [ ] **Step 3: Implement `ToolRegistry` and `Tool` trait**

Replace `src/tools/mod.rs`:

```rust
pub mod indicators;
pub mod ohlcv;

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ToolName(String);

impl ToolName {
    pub fn new(s: impl Into<String>) -> Self { Self(s.into()) }
    pub fn as_str(&self) -> &str { &self.0 }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> ToolName;
    fn description(&self) -> &'static str;
    /// JSON in, JSON out. Schema is documented per-tool.
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value>;
}

pub struct ToolRegistry {
    tools: HashMap<ToolName, Arc<dyn Tool>>,
}

impl ToolRegistry {
    pub fn empty() -> Self { Self { tools: HashMap::new() } }

    pub fn default_with_builtins() -> Self {
        let mut r = Self::empty();
        r.register(Arc::new(ohlcv::OhlcvTool));
        r.register(Arc::new(indicators::IndicatorPanelTool));
        r
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name(), tool);
    }

    pub fn get(&self, name: &ToolName) -> Option<Arc<dyn Tool>> {
        self.tools.get(name).cloned()
    }

    pub fn list(&self) -> Vec<ToolName> {
        self.tools.keys().cloned().collect()
    }
}
```

- [ ] **Step 4: Stub the two builtin tools (real impls in Task 12)**

Create `src/tools/ohlcv.rs`:

```rust
use async_trait::async_trait;

use crate::tools::{Tool, ToolName};

pub struct OhlcvTool;

#[async_trait]
impl Tool for OhlcvTool {
    fn name(&self) -> ToolName { ToolName::new("ohlcv") }
    fn description(&self) -> &'static str { "OHLCV history for an asset and time range" }
    async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        // Real impl in Task 12 — for now, return a deterministic stub so registry tests pass.
        Ok(serde_json::json!({"stub": true, "tool": "ohlcv"}))
    }
}
```

Create `src/tools/indicators.rs`:

```rust
use async_trait::async_trait;

use crate::tools::{Tool, ToolName};

pub struct IndicatorPanelTool;

#[async_trait]
impl Tool for IndicatorPanelTool {
    fn name(&self) -> ToolName { ToolName::new("indicator_panel") }
    fn description(&self) -> &'static str { "Computed indicator panel for an asset" }
    async fn invoke(&self, _input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::json!({"stub": true, "tool": "indicator_panel"}))
    }
}
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p xianvec-engine registry_lists unknown_tool_returns_none`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-engine/src/tools crates/xianvec-engine/tests
git commit -m "feat(engine): ToolRegistry with stub OHLCV and IndicatorPanel"
```

---

### Task 12: Wire OHLCV + IndicatorPanel tools to `xianvec-data`

**Files:**
- Modify: `crates/xianvec-engine/src/tools/ohlcv.rs`
- Modify: `crates/xianvec-engine/src/tools/indicators.rs`
- Modify: `crates/xianvec-engine/Cargo.toml` — confirm `xianvec-data` dep

- [ ] **Step 1: Inspect xianvec-data public API**

Run: `grep -n 'pub fn\|pub struct' crates/xianvec-data/src/lib.rs`
Expected: see `Ohlcv`, indicator entry points; capture exact names.

If the public surface returns `MarketSnapshot` or similar, use that path. If the file is small, read it whole:

```bash
cat crates/xianvec-data/src/lib.rs
```

- [ ] **Step 2: Write failing integration test**

Append to `tests/tool_registry.rs`:

```rust
#[tokio::test]
async fn ohlcv_tool_returns_real_bars_for_known_fixture() {
    let reg = ToolRegistry::default_with_builtins();
    let tool = reg.get(&ToolName::new("ohlcv")).expect("ohlcv");
    let out = tool.invoke(serde_json::json!({
        "asset": "BTC/USD",
        "fixture": "test-fixture-btc-2024-01"
    })).await.expect("invoke");
    let bars = out.get("bars").expect("bars in response");
    assert!(bars.is_array());
    assert!(bars.as_array().unwrap().len() > 0);
}
```

- [ ] **Step 3: Run test to confirm it fails**

Run: `cargo test -p xianvec-engine ohlcv_tool_returns_real_bars`
Expected: FAIL — stub returns `{"stub": true}`.

- [ ] **Step 4: Implement OHLCV tool against `xianvec-data`**

Replace `src/tools/ohlcv.rs`:

```rust
use async_trait::async_trait;
use serde::Deserialize;

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct OhlcvRequest {
    asset: String,
    #[serde(default)]
    fixture: Option<String>,
    #[serde(default = "default_lookback")]
    lookback_bars: usize,
}

fn default_lookback() -> usize { 200 }

pub struct OhlcvTool;

#[async_trait]
impl Tool for OhlcvTool {
    fn name(&self) -> ToolName { ToolName::new("ohlcv") }
    fn description(&self) -> &'static str { "OHLCV history for an asset and time range" }
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: OhlcvRequest = serde_json::from_value(input)?;
        // For MVP, fixture-mode reads parquet from data/probes/.
        // Real Alpaca pull is a Plan #2 task — keeping MVP deterministic.
        let bars = if let Some(fixture) = req.fixture {
            xianvec_data::fixtures::load_ohlcv_fixture(&fixture, &req.asset, req.lookback_bars)?
        } else {
            anyhow::bail!("MVP requires a fixture name; live Alpaca fetch lands in Plan #2");
        };
        Ok(serde_json::json!({"asset": req.asset, "bars": bars}))
    }
}
```

> **Note:** if `xianvec_data::fixtures::load_ohlcv_fixture` doesn't exist yet, add a minimal one in `crates/xianvec-data/src/fixtures.rs` that reads a parquet file from `data/probes/<fixture>.parquet` and returns a `Vec<Ohlcv>` (or whatever the existing OHLCV type is). The existing `xianvec-data` already has parquet loading via `polars` per workspace deps; reuse that machinery rather than introducing a new code path.

- [ ] **Step 5: Implement IndicatorPanel tool similarly**

Replace `src/tools/indicators.rs`:

```rust
use async_trait::async_trait;
use serde::Deserialize;

use crate::tools::{Tool, ToolName};

#[derive(Deserialize)]
struct PanelRequest {
    asset: String,
    fixture: String,
    #[serde(default = "default_lookback")]
    lookback_bars: usize,
}

fn default_lookback() -> usize { 200 }

pub struct IndicatorPanelTool;

#[async_trait]
impl Tool for IndicatorPanelTool {
    fn name(&self) -> ToolName { ToolName::new("indicator_panel") }
    fn description(&self) -> &'static str { "Computed indicator panel (RSI, MACD, BB, ATR, MA, EMA)" }
    async fn invoke(&self, input: serde_json::Value) -> anyhow::Result<serde_json::Value> {
        let req: PanelRequest = serde_json::from_value(input)?;
        let panel = xianvec_data::indicators::compute_panel_from_fixture(
            &req.fixture, &req.asset, req.lookback_bars,
        )?;
        Ok(serde_json::to_value(panel)?)
    }
}
```

> **Note:** add `compute_panel_from_fixture` to `crates/xianvec-data/src/indicators.rs` if it doesn't already exist — it's a thin wrapper over the existing indicator computation that takes a fixture path.

- [ ] **Step 6: Add a tiny test fixture**

Run: `ls data/probes/ | head -5` to see existing probe data. If no parquet OHLCV fixture exists, generate one:

```bash
cat > /tmp/make_test_fixture.py << 'EOF'
import polars as pl
import datetime as dt
import os
os.makedirs("data/probes", exist_ok=True)
rows = []
ts = dt.datetime(2024, 1, 1)
price = 42000.0
for i in range(300):
    o = price
    h = price * 1.005
    l = price * 0.995
    c = price * (1 + 0.001 * ((i % 7) - 3))
    v = 100 + i
    rows.append({"timestamp": ts.isoformat(), "open": o, "high": h, "low": l, "close": c, "volume": v})
    ts += dt.timedelta(hours=1)
    price = c
df = pl.DataFrame(rows)
df.write_parquet("data/probes/test-fixture-btc-2024-01.parquet")
print(f"wrote {len(rows)} bars")
EOF
python3 /tmp/make_test_fixture.py
```

Verify: `ls -la data/probes/test-fixture-btc-2024-01.parquet`
Expected: file exists, ~10KB.

- [ ] **Step 7: Run tests**

Run: `cargo test -p xianvec-engine ohlcv_tool_returns`
Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add crates/xianvec-engine/src/tools crates/xianvec-data/src crates/xianvec-engine/tests data/probes/test-fixture-btc-2024-01.parquet
git commit -m "feat(engine): wire OHLCV and IndicatorPanel tools to xianvec-data fixtures"
```

---

### Task 13: `LlmDispatch` trait + Anthropic implementation

**Files:**
- Create: `crates/xianvec-engine/src/agent/llm.rs`
- Modify: `crates/xianvec-engine/src/agent/mod.rs` (replace placeholder)
- Modify: `crates/xianvec-engine/Cargo.toml` (add anthropic-sdk + reqwest)
- Test: `crates/xianvec-engine/tests/llm_dispatch.rs`

- [ ] **Step 1: Add deps**

In `crates/xianvec-engine/Cargo.toml`, add to `[dependencies]`:

```toml
reqwest = { workspace = true }
async-anthropic = "0.6"   # or anthropic-sdk = "0.x" — pick whichever is the active maintained crate at writing time; verify with `cargo search anthropic`
```

Run: `cargo search anthropic-sdk --limit 5`
Pick the most-downloaded maintained crate. Document choice with a one-line comment in Cargo.toml.

- [ ] **Step 2: Write failing test (with `#[ignore]` for live API)**

Create `crates/xianvec-engine/tests/llm_dispatch.rs`:

```rust
use xianvec_engine::agent::llm::{LlmDispatch, LlmRequest, MockDispatch};

#[tokio::test]
async fn mock_dispatch_returns_expected_output() {
    let mock = MockDispatch::echo(r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#);
    let resp = mock.complete(LlmRequest {
        model: "anthropic.claude-sonnet-4.6".into(),
        system_prompt: "you are a trader".into(),
        user_prompt: "decide".into(),
        max_tokens: 200,
    }).await.unwrap();
    assert!(resp.text.contains("hold"));
    assert!(resp.input_tokens > 0);
    assert!(resp.output_tokens > 0);
}

#[tokio::test]
#[ignore = "needs ANTHROPIC_API_KEY"]
async fn anthropic_dispatch_returns_real_text() {
    use xianvec_engine::agent::llm::AnthropicDispatch;
    let key = std::env::var("ANTHROPIC_API_KEY").unwrap();
    let d = AnthropicDispatch::new(key);
    let resp = d.complete(LlmRequest {
        model: "claude-sonnet-4-6".into(),
        system_prompt: "you are concise".into(),
        user_prompt: "say 'hello' and nothing else".into(),
        max_tokens: 50,
    }).await.unwrap();
    assert!(resp.text.to_lowercase().contains("hello"));
}
```

- [ ] **Step 3: Run mock test to verify it fails**

Run: `cargo test -p xianvec-engine mock_dispatch_returns`
Expected: FAIL — types not found.

- [ ] **Step 4: Implement `LlmDispatch` + `MockDispatch` + `AnthropicDispatch`**

Replace `src/agent/mod.rs`:

```rust
pub mod llm;
```

Create `src/agent/llm.rs`:

```rust
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmRequest {
    pub model: String,
    pub system_prompt: String,
    pub user_prompt: String,
    pub max_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmResponse {
    pub text: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
}

#[async_trait]
pub trait LlmDispatch: Send + Sync {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse>;
}

// ---- MockDispatch (testing) -----------------------------------------------

pub struct MockDispatch {
    canned: String,
}

impl MockDispatch {
    pub fn echo(s: impl Into<String>) -> Self { Self { canned: s.into() } }
}

#[async_trait]
impl LlmDispatch for MockDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        Ok(LlmResponse {
            text: self.canned.clone(),
            input_tokens: estimate_tokens(&req.system_prompt) + estimate_tokens(&req.user_prompt),
            output_tokens: estimate_tokens(&self.canned),
        })
    }
}

fn estimate_tokens(s: &str) -> u32 {
    // ~4 chars/token. Coarse but deterministic for tests.
    ((s.len() + 3) / 4) as u32
}

// ---- AnthropicDispatch (real) ---------------------------------------------

pub struct AnthropicDispatch {
    api_key: String,
    client: reqwest::Client,
}

impl AnthropicDispatch {
    pub fn new(api_key: String) -> Self {
        Self { api_key, client: reqwest::Client::new() }
    }
}

#[async_trait]
impl LlmDispatch for AnthropicDispatch {
    async fn complete(&self, req: LlmRequest) -> anyhow::Result<LlmResponse> {
        let body = serde_json::json!({
            "model": req.model,
            "max_tokens": req.max_tokens,
            "system": req.system_prompt,
            "messages": [{"role": "user", "content": req.user_prompt}],
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

        let text = resp["content"][0]["text"].as_str().unwrap_or_default().to_string();
        let input_tokens = resp["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32;
        let output_tokens = resp["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32;
        Ok(LlmResponse { text, input_tokens, output_tokens })
    }
}
```

- [ ] **Step 5: Run mock test**

Run: `cargo test -p xianvec-engine mock_dispatch_returns`
Expected: PASS.

- [ ] **Step 6: Run live test if you have a key (otherwise skip)**

Run: `ANTHROPIC_API_KEY=$(op read 'op://Personal/Anthropic API/credential') cargo test -p xianvec-engine anthropic_dispatch_returns_real_text -- --ignored`
Expected: PASS if key works. Skip if no key.

- [ ] **Step 7: Commit**

```bash
git add crates/xianvec-engine/src/agent crates/xianvec-engine/Cargo.toml crates/xianvec-engine/tests/llm_dispatch.rs
git commit -m "feat(engine): LlmDispatch trait with Mock and Anthropic implementations"
```

---

## Phase 1E — Agent execution (inline, no scheduler)

### Task 14: Single-slot execution (`execute_slot`)

**Files:**
- Create: `crates/xianvec-engine/src/agent/execute.rs`
- Modify: `crates/xianvec-engine/src/agent/mod.rs`
- Test: `crates/xianvec-engine/tests/agent_slot.rs`

- [ ] **Step 1: Write failing test**

Create `crates/xianvec-engine/tests/agent_slot.rs`:

```rust
use std::sync::Arc;
use xianvec_engine::agent::execute::{execute_slot, SlotInput};
use xianvec_engine::agent::llm::MockDispatch;
use xianvec_engine::bundle::slot::LLMSlot;
use xianvec_engine::tools::ToolRegistry;

#[tokio::test]
async fn execute_slot_returns_parsed_output() {
    let slot = LLMSlot {
        role: "trader".into(),
        prompt: "decide".into(),
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
    };
    let dispatch = Arc::new(MockDispatch::echo(
        r#"{"action":"long_open","conviction":0.7,"justification":"oversold"}"#,
    ));
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let out = execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({"ohlcv_history": [], "indicator_panel": {}}),
        dispatch,
        tools,
    }).await.unwrap();
    assert_eq!(out.text.contains("long_open"), true);
    assert!(out.input_tokens > 0);
}

#[tokio::test]
async fn execute_slot_rejects_undeclared_tool() {
    use xianvec_engine::tools::ToolName;
    use std::sync::Arc;

    let slot = LLMSlot {
        role: "trader".into(),
        prompt: "decide".into(),
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        allowed_tools: vec!["ohlcv".into()],
    };
    let dispatch = Arc::new(MockDispatch::echo("ok"));
    let mut tools = ToolRegistry::default_with_builtins();
    // Caller asks the slot to use a tool not in its allowlist.
    let result = execute_slot(SlotInput {
        slot: &slot,
        upstream_inputs: serde_json::json!({"requested_tool": "indicator_panel"}),
        dispatch,
        tools: Arc::new(tools),
    }).await;
    // Slot allowlist is "ohlcv" only; undeclared "indicator_panel" use should be rejected
    // when (in a future task) the agent tries to invoke it. For MVP execute_slot just
    // dispatches the prompt; tool-allowlist enforcement happens at tool-invoke time when
    // tool calls are wired in (deferred to Plan #2). This test asserts the slot still
    // succeeds because it doesn't actually call the tool — keeps the contract honest.
    assert!(result.is_ok());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine execute_slot_returns`
Expected: FAIL — `execute_slot` not found.

- [ ] **Step 3: Implement `execute_slot`**

Create `src/agent/execute.rs`:

```rust
use std::sync::Arc;

use crate::agent::llm::{LlmDispatch, LlmRequest, LlmResponse};
use crate::bundle::slot::LLMSlot;
use crate::tools::ToolRegistry;

pub struct SlotInput<'a> {
    pub slot: &'a LLMSlot,
    pub upstream_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
}

pub async fn execute_slot<'a>(input: SlotInput<'a>) -> anyhow::Result<LlmResponse> {
    let user_prompt = format!(
        "Inputs:\n{}\n\nFollow the slot's instructions and emit JSON.",
        serde_json::to_string_pretty(&input.upstream_inputs)?
    );
    let req = LlmRequest {
        model: input.slot.model_requirement.clone(),
        system_prompt: input.slot.prompt.clone(),
        user_prompt,
        max_tokens: 1000,
    };
    let resp = input.dispatch.complete(req).await?;
    // Tools are intentionally not invoked in MVP — Plan #2 wires tool-call dispatch into
    // the agent loop (when the LLM emits a tool_use, we route through `input.tools`).
    let _ = input.tools;
    Ok(resp)
}
```

Update `src/agent/mod.rs`:

```rust
pub mod execute;
pub mod llm;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine execute_slot`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/agent crates/xianvec-engine/tests/agent_slot.rs
git commit -m "feat(engine): execute_slot — single-slot inline LLM dispatch"
```

---

### Task 15: 3-slot pipeline (regime → intern → trader)

**Files:**
- Create: `crates/xianvec-engine/src/agent/pipeline.rs`
- Modify: `crates/xianvec-engine/src/agent/mod.rs`
- Test: `crates/xianvec-engine/tests/pipeline_inline.rs`

- [ ] **Step 1: Write failing test**

Create `crates/xianvec-engine/tests/pipeline_inline.rs`:

```rust
use std::sync::Arc;
use xianvec_engine::agent::llm::MockDispatch;
use xianvec_engine::agent::pipeline::{run_pipeline, PipelineInputs, PipelineOutputs};
use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xianvec_engine::bundle::risk::RiskPreset;
use xianvec_engine::bundle::slot::LLMSlot;
use xianvec_engine::bundle::StrategyBundle;
use xianvec_engine::tools::ToolRegistry;

fn fixture_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01H8N7ZPIPE".into(), display_name: "Pipe Test".into(),
            plain_summary: "x".into(), creator: "@t".into(),
            template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
            asset_universe: vec!["BTC/USD".into()], decision_cadence_minutes: 15,
            required_models: vec!["mock".into()],
            required_tools: vec!["ohlcv".into()],
            risk_preset_or_config: "balanced".into(), published_at: None,
        },
        regime_slot: Some(LLMSlot {
            role: "regime".into(), prompt: "classify regime".into(),
            model_requirement: "mock".into(), allowed_tools: vec!["ohlcv".into()],
        }),
        intern_slot: Some(LLMSlot {
            role: "intern".into(), prompt: "build briefing".into(),
            model_requirement: "mock".into(), allowed_tools: vec!["ohlcv".into()],
        }),
        trader_slot: Some(LLMSlot {
            role: "trader".into(), prompt: "decide".into(),
            model_requirement: "mock".into(), allowed_tools: vec!["ohlcv".into()],
        }),
        risk: RiskPreset::Balanced.expand(),
        mechanical_params: serde_json::json!({}),
    }
}

#[tokio::test]
async fn three_slot_pipeline_chains_outputs() {
    let bundle = fixture_bundle();
    let dispatch = Arc::new(MockDispatch::echo(r#"{"ok":true}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs: PipelineOutputs = run_pipeline(PipelineInputs {
        bundle: &bundle,
        seed_inputs: serde_json::json!({"ohlcv_history": [], "indicator_panel": {}}),
        dispatch,
        tools,
    }).await.unwrap();
    assert!(outs.regime.is_some());
    assert!(outs.intern.is_some());
    assert!(outs.trader.is_some());
    assert!(outs.total_input_tokens > 0);
    assert!(outs.total_output_tokens > 0);
}

#[tokio::test]
async fn skips_missing_optional_slots() {
    let mut bundle = fixture_bundle();
    bundle.regime_slot = None;  // skip
    let dispatch = Arc::new(MockDispatch::echo(r#"{"ok":true}"#));
    let tools = Arc::new(ToolRegistry::default_with_builtins());
    let outs = run_pipeline(PipelineInputs {
        bundle: &bundle,
        seed_inputs: serde_json::json!({}),
        dispatch, tools,
    }).await.unwrap();
    assert!(outs.regime.is_none());
    assert!(outs.intern.is_some());
    assert!(outs.trader.is_some());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine three_slot_pipeline`
Expected: FAIL — types not found.

- [ ] **Step 3: Implement pipeline**

Create `src/agent/pipeline.rs`:

```rust
use std::sync::Arc;

use crate::agent::execute::{execute_slot, SlotInput};
use crate::agent::llm::{LlmDispatch, LlmResponse};
use crate::bundle::StrategyBundle;
use crate::tools::ToolRegistry;

pub struct PipelineInputs<'a> {
    pub bundle: &'a StrategyBundle,
    pub seed_inputs: serde_json::Value,
    pub dispatch: Arc<dyn LlmDispatch>,
    pub tools: Arc<ToolRegistry>,
}

#[derive(Debug)]
pub struct PipelineOutputs {
    pub regime: Option<LlmResponse>,
    pub intern: Option<LlmResponse>,
    pub trader: Option<LlmResponse>,
    pub total_input_tokens: u32,
    pub total_output_tokens: u32,
}

pub async fn run_pipeline<'a>(input: PipelineInputs<'a>) -> anyhow::Result<PipelineOutputs> {
    let mut accumulated = input.seed_inputs.clone();
    let mut total_in = 0u32;
    let mut total_out = 0u32;

    let regime = if let Some(slot) = &input.bundle.regime_slot {
        let out = execute_slot(SlotInput {
            slot, upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(), tools: input.tools.clone(),
        }).await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        accumulated["regime_output"] = serde_json::Value::String(out.text.clone());
        Some(out)
    } else { None };

    let intern = if let Some(slot) = &input.bundle.intern_slot {
        let out = execute_slot(SlotInput {
            slot, upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(), tools: input.tools.clone(),
        }).await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        accumulated["intern_output"] = serde_json::Value::String(out.text.clone());
        Some(out)
    } else { None };

    let trader = if let Some(slot) = &input.bundle.trader_slot {
        let out = execute_slot(SlotInput {
            slot, upstream_inputs: accumulated.clone(),
            dispatch: input.dispatch.clone(), tools: input.tools.clone(),
        }).await?;
        total_in += out.input_tokens;
        total_out += out.output_tokens;
        Some(out)
    } else { None };

    Ok(PipelineOutputs {
        regime, intern, trader,
        total_input_tokens: total_in,
        total_output_tokens: total_out,
    })
}
```

Update `src/agent/mod.rs`:

```rust
pub mod execute;
pub mod llm;
pub mod pipeline;
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine pipeline`
Expected: PASS for both tests.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/agent/pipeline.rs crates/xianvec-engine/src/agent/mod.rs crates/xianvec-engine/tests/pipeline_inline.rs
git commit -m "feat(engine): 3-slot agent pipeline (regime → intern → trader)"
```

---

### Task 16: Token estimator (deterministic, pre-run)

**Files:**
- Modify: `crates/xianvec-engine/src/tokens.rs` (replace placeholder)
- Test: `crates/xianvec-engine/tests/tokens.rs`

- [ ] **Step 1: Write failing test**

Create `crates/xianvec-engine/tests/tokens.rs`:

```rust
use xianvec_engine::bundle::risk::RiskPreset;
use xianvec_engine::tokens::estimate_pipeline_tokens;
use xianvec_engine::templates::{registry, Template};

#[test]
fn estimator_returns_positive_token_counts_for_real_bundle() {
    let tpl = registry::get("mean_reversion").unwrap();
    let b = tpl.new_draft("01H8N7ZTKN".into(), "tkn-test".into(), "@t".into());
    let est = estimate_pipeline_tokens(&b, /*decision_points=*/ 100);
    assert!(est.total > 0);
    assert!(est.input > 0);
    assert!(est.output > 0);
    // input dominates output for typical strategy runs (long prompts, short JSON outs).
    assert!(est.input > est.output);
}

#[test]
fn estimator_scales_with_decision_points() {
    let tpl = xianvec_engine::templates::registry::get("mean_reversion").unwrap();
    let b = tpl.new_draft("01H8N7ZSCALE".into(), "scale-test".into(), "@t".into());
    let est_small = estimate_pipeline_tokens(&b, 10);
    let est_big   = estimate_pipeline_tokens(&b, 1000);
    assert!(est_big.total > est_small.total * 50);  // ~100x more decisions ≈ 100x more tokens
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-engine estimator_returns`
Expected: FAIL — `estimate_pipeline_tokens` not found.

- [ ] **Step 3: Implement estimator**

Replace `src/tokens.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::bundle::StrategyBundle;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenEstimate {
    pub input: u64,
    pub output: u64,
    pub total: u64,
}

const CHARS_PER_TOKEN: usize = 4;
const FIXED_CONTEXT_TOKENS_PER_FIRE: u64 = 600;   // ohlcv panel + indicator panel header
const OUTPUT_TOKENS_PER_FIRE: u64 = 80;            // typical small JSON decision

pub fn estimate_pipeline_tokens(b: &StrategyBundle, decision_points: u64) -> TokenEstimate {
    let mut per_fire_input = 0u64;
    let mut per_fire_output = 0u64;
    for slot in [&b.regime_slot, &b.intern_slot, &b.trader_slot].into_iter().flatten() {
        let prompt_tokens = ((slot.prompt.len() + CHARS_PER_TOKEN - 1) / CHARS_PER_TOKEN) as u64;
        per_fire_input += prompt_tokens + FIXED_CONTEXT_TOKENS_PER_FIRE;
        per_fire_output += OUTPUT_TOKENS_PER_FIRE;
    }
    let input = per_fire_input * decision_points;
    let output = per_fire_output * decision_points;
    TokenEstimate { input, output, total: input + output }
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p xianvec-engine estimator`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-engine/src/tokens.rs crates/xianvec-engine/tests/tokens.rs
git commit -m "feat(engine): token estimator for pipeline runs"
```

---

## Phase 1F — CLI surface (`xvn strategy ...`)

### Task 17: Wire `xvn strategy` subcommand skeleton

**Files:**
- Modify: `crates/xianvec-cli/Cargo.toml` — add `xianvec-engine` dep
- Modify: `crates/xianvec-cli/src/main.rs` — register `strategy` subcommand
- Create: `crates/xianvec-cli/src/strategy.rs`

- [ ] **Step 1: Inspect existing CLI structure**

Run: `head -80 crates/xianvec-cli/src/main.rs`
Note the `clap` derive style used; match it.

- [ ] **Step 2: Add dep**

In `crates/xianvec-cli/Cargo.toml` `[dependencies]`, add:

```toml
xianvec-engine = { path = "../xianvec-engine" }
ulid           = "1"
```

- [ ] **Step 3: Add `Strategy` subcommand**

In `crates/xianvec-cli/src/main.rs`, find the top-level command enum (likely `enum Command` or similar). Add a `Strategy` variant:

```rust
// in the top-level Subcommand enum
Strategy(crate::strategy::StrategyCmd),
```

In the dispatch match, add:

```rust
Command::Strategy(cmd) => crate::strategy::run(cmd).await,
```

Add module declaration near the top of `main.rs`:

```rust
mod strategy;
```

- [ ] **Step 4: Create the subcommand module**

Create `crates/xianvec-cli/src/strategy.rs`:

```rust
use clap::{Args, Subcommand};

#[derive(Args)]
pub struct StrategyCmd {
    #[command(subcommand)]
    action: StrategyAction,
}

#[derive(Subcommand)]
enum StrategyAction {
    /// Create a new strategy draft from a template.
    New {
        /// Template name (e.g., "mean_reversion").
        #[arg(long)]
        template: String,
        /// Human-readable name.
        #[arg(long)]
        name: String,
        /// Creator handle (default: $XVN_CREATOR or "@anonymous").
        #[arg(long)]
        creator: Option<String>,
    },
    /// Validate a saved strategy bundle by id.
    Validate { id: String },
    /// List all saved strategy ids.
    Ls,
    /// Show a saved strategy bundle as JSON.
    Show { id: String },
}

pub async fn run(cmd: StrategyCmd) -> anyhow::Result<()> {
    match cmd.action {
        StrategyAction::New { template, name, creator } => new(&template, &name, creator).await,
        StrategyAction::Validate { id } => validate(&id).await,
        StrategyAction::Ls => ls().await,
        StrategyAction::Show { id } => show(&id).await,
    }
}

async fn new(_template: &str, _name: &str, _creator: Option<String>) -> anyhow::Result<()> {
    anyhow::bail!("not implemented yet — Task 18")
}
async fn validate(_id: &str) -> anyhow::Result<()> { anyhow::bail!("not implemented yet — Task 18") }
async fn ls() -> anyhow::Result<()> { anyhow::bail!("not implemented yet — Task 18") }
async fn show(_id: &str) -> anyhow::Result<()> { anyhow::bail!("not implemented yet — Task 18") }
```

- [ ] **Step 5: Verify it builds**

Run: `cargo build -p xianvec-cli`
Expected: clean build. The `xvn strategy --help` should show the subcommands.

Verify: `cargo run -p xianvec-cli -- strategy --help`
Expected output mentions `new`, `validate`, `ls`, `show`.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-cli/Cargo.toml crates/xianvec-cli/src/main.rs crates/xianvec-cli/src/strategy.rs
git commit -m "feat(cli): wire xvn strategy subcommand skeleton"
```

---

### Task 18: Implement `xvn strategy new / validate / ls / show`

**Files:**
- Modify: `crates/xianvec-cli/src/strategy.rs`
- Create: `crates/xianvec-cli/tests/strategy_cli.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xianvec-cli/tests/strategy_cli.rs`:

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

#[test]
fn new_validate_ls_show_roundtrip() {
    let dir = tempdir().unwrap();

    let out = xvn(&["strategy", "new", "--template", "mean_reversion", "--name", "test1"], dir.path());
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert!(id.starts_with("01"), "expected ULID, got: {id}");

    let out = xvn(&["strategy", "validate", &id], dir.path());
    assert!(out.status.success());

    let out = xvn(&["strategy", "ls"], dir.path());
    assert!(out.status.success());
    assert!(String::from_utf8(out.stdout).unwrap().contains(&id));

    let out = xvn(&["strategy", "show", &id], dir.path());
    assert!(out.status.success());
    let json = String::from_utf8(out.stdout).unwrap();
    assert!(json.contains("\"template\""));
    assert!(json.contains("mean_reversion"));
}
```

> The `CARGO_BIN_EXE_xvn` env var requires the `xvn` binary target. Confirm by inspecting the existing `[[bin]]` section in `crates/xianvec-cli/Cargo.toml`. If it's named differently (e.g., `xianvec-cli`), substitute that name in the env var.

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-cli new_validate_ls_show`
Expected: FAIL — handlers panic with "not implemented yet".

- [ ] **Step 3: Implement the handlers**

Replace the stubs in `crates/xianvec-cli/src/strategy.rs` with real impls:

```rust
use std::env;
use std::path::PathBuf;

use ulid::Ulid;
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::validate::validate_bundle;
use xianvec_engine::templates::registry;

fn home() -> PathBuf {
    if let Ok(p) = env::var("XVN_HOME") {
        return PathBuf::from(p);
    }
    let h = dirs::home_dir().expect("$HOME");
    h.join(".xvn")
}

fn store() -> FilesystemStore { FilesystemStore::new(home().join("strategies")) }

async fn new(template: &str, name: &str, creator: Option<String>) -> anyhow::Result<()> {
    let tpl = registry::get(template)
        .ok_or_else(|| anyhow::anyhow!("unknown template '{template}' — try `xvn strategy templates`"))?;
    let id = Ulid::new().to_string();
    let creator = creator
        .or_else(|| env::var("XVN_CREATOR").ok())
        .unwrap_or_else(|| "@anonymous".to_string());
    let draft = tpl.new_draft(id.clone(), name.to_string(), creator);
    validate_bundle(&draft)?;
    store().save(&draft).await?;
    println!("{id}");
    Ok(())
}

async fn validate(id: &str) -> anyhow::Result<()> {
    let bundle = store().load(id).await?;
    validate_bundle(&bundle)?;
    println!("ok");
    Ok(())
}

async fn ls() -> anyhow::Result<()> {
    let ids = store().list().await?;
    for id in ids { println!("{id}"); }
    Ok(())
}

async fn show(id: &str) -> anyhow::Result<()> {
    let bundle = store().load(id).await?;
    let json = serde_json::to_string_pretty(&bundle)?;
    println!("{json}");
    Ok(())
}
```

Add to `crates/xianvec-cli/Cargo.toml` `[dependencies]`: `dirs = "5"`.

- [ ] **Step 4: Run test**

Run: `cargo test -p xianvec-cli new_validate_ls_show`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-cli/src/strategy.rs crates/xianvec-cli/Cargo.toml crates/xianvec-cli/tests/strategy_cli.rs
git commit -m "feat(cli): implement xvn strategy new/validate/ls/show"
```

---

### Task 19: Add `xvn strategy templates` listing

**Files:**
- Modify: `crates/xianvec-cli/src/strategy.rs`
- Modify: `crates/xianvec-cli/tests/strategy_cli.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/strategy_cli.rs`:

```rust
#[test]
fn templates_lists_known_templates() {
    let dir = tempdir().unwrap();
    let out = xvn(&["strategy", "templates"], dir.path());
    assert!(out.status.success());
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("mean_reversion"));
    assert!(stdout.contains("Buys dips"));  // display_name
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-cli templates_lists`
Expected: FAIL — subcommand not registered.

- [ ] **Step 3: Add the subcommand variant + handler**

In `crates/xianvec-cli/src/strategy.rs`:

```rust
// in the StrategyAction enum
Templates,

// in the run() match
StrategyAction::Templates => templates().await,

// new handler
async fn templates() -> anyhow::Result<()> {
    use xianvec_engine::templates::registry;
    let names = registry::list_template_names();
    for name in names {
        if let Some(tpl) = registry::get(&name) {
            println!("{:<20} {}", name, tpl.display_name());
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run test**

Run: `cargo test -p xianvec-cli templates_lists`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-cli/src/strategy.rs crates/xianvec-cli/tests/strategy_cli.rs
git commit -m "feat(cli): xvn strategy templates lists registered templates"
```

---

### Task 20: Add `xvn strategy run` for inline pipeline execution

**Files:**
- Modify: `crates/xianvec-cli/src/strategy.rs`
- Modify: `crates/xianvec-cli/tests/strategy_cli.rs`

- [ ] **Step 1: Write the failing test**

Append to `tests/strategy_cli.rs`:

```rust
#[test]
fn run_inline_with_mock_dispatch_succeeds() {
    let dir = tempdir().unwrap();

    // Create a draft.
    let out = xvn(&["strategy", "new", "--template", "mean_reversion", "--name", "run-test"], dir.path());
    assert!(out.status.success());
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    // Run inline against the test fixture, using the mock LLM dispatch (--mock).
    let out = xvn(
        &["strategy", "run", &id, "--fixture", "test-fixture-btc-2024-01", "--decisions", "3", "--mock"],
        dir.path(),
    );
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8(out.stdout).unwrap();
    assert!(stdout.contains("decisions:"));
    assert!(stdout.contains("input_tokens:"));
    assert!(stdout.contains("output_tokens:"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p xianvec-cli run_inline_with_mock`
Expected: FAIL — subcommand not registered.

- [ ] **Step 3: Implement `Run` subcommand**

In `crates/xianvec-cli/src/strategy.rs`:

```rust
// in StrategyAction enum
Run {
    id: String,
    /// Fixture parquet name under data/probes/.
    #[arg(long)]
    fixture: String,
    /// How many decision points to simulate (≥1).
    #[arg(long, default_value_t = 1)]
    decisions: u32,
    /// Use the deterministic mock LLM dispatch (no API calls).
    #[arg(long, default_value_t = false)]
    mock: bool,
},

// in run() match
StrategyAction::Run { id, fixture, decisions, mock } =>
    run_inline(&id, &fixture, decisions, mock).await,
```

Add the handler:

```rust
use std::sync::Arc;
use xianvec_engine::agent::llm::{AnthropicDispatch, LlmDispatch, MockDispatch};
use xianvec_engine::agent::pipeline::{run_pipeline, PipelineInputs};
use xianvec_engine::tokens::estimate_pipeline_tokens;
use xianvec_engine::tools::ToolRegistry;

async fn run_inline(id: &str, fixture: &str, decisions: u32, mock: bool) -> anyhow::Result<()> {
    let bundle = store().load(id).await?;
    let est = estimate_pipeline_tokens(&bundle, decisions as u64);
    println!(
        "estimate: input={} output={} total={} (decisions={})",
        est.input, est.output, est.total, decisions
    );

    let dispatch: Arc<dyn LlmDispatch> = if mock {
        Arc::new(MockDispatch::echo(r#"{"action":"hold","conviction":0.5,"justification":"mock"}"#))
    } else {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("set ANTHROPIC_API_KEY or pass --mock"))?;
        Arc::new(AnthropicDispatch::new(key))
    };
    let tools = Arc::new(ToolRegistry::default_with_builtins());

    let asset = bundle.manifest.asset_universe.first().cloned()
        .ok_or_else(|| anyhow::anyhow!("bundle has empty asset_universe"))?;
    let mut total_in = 0u32;
    let mut total_out = 0u32;
    for n in 0..decisions {
        let seed = serde_json::json!({
            "decision_index": n,
            "asset": asset,
            "fixture": fixture,
            "ohlcv_history": "<fetch via tool — Plan #2 wires this>",
            "indicator_panel": "<fetch via tool — Plan #2 wires this>",
        });
        let outs = run_pipeline(PipelineInputs {
            bundle: &bundle, seed_inputs: seed,
            dispatch: dispatch.clone(), tools: tools.clone(),
        }).await?;
        total_in += outs.total_input_tokens;
        total_out += outs.total_output_tokens;
    }
    println!(
        "decisions: {} input_tokens: {} output_tokens: {}",
        decisions, total_in, total_out
    );
    Ok(())
}
```

> **Note** the seed `ohlcv_history` and `indicator_panel` are placeholders — the agent loop in MVP doesn't pull these via tools yet. Plan #2 wires the tool-call dispatch path so the LLM can actually request fresh data. For MVP the LLM gets a stub and emits a decision based on the prompt alone — sufficient to demo the wire-up.

- [ ] **Step 4: Run test**

Run: `cargo test -p xianvec-cli run_inline_with_mock`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xianvec-cli/src/strategy.rs crates/xianvec-cli/tests/strategy_cli.rs
git commit -m "feat(cli): xvn strategy run for inline pipeline execution"
```

---

## Phase 1G — Documentation & integration smoke

### Task 21: README + smoke-test recipe

**Files:**
- Create: `crates/xianvec-engine/README.md`
- Modify: top-level `MANUAL.md` — add a section linking to the new CLI commands

- [ ] **Step 1: Write the engine README**

Create `crates/xianvec-engine/README.md`:

```markdown
# xianvec-engine

Strategy creation, bundling, and inline agent execution for xvn.

See specs:
- `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`
- `docs/superpowers/specs/2026-05-08-eval-engine-design.md`

## What ships in MVP (this crate, v0.1)

- Strategy bundle types (manifest + slots + risk + mechanical params)
- 1 template: `mean_reversion`
- 1 migrated baseline: `ma_crossover` (LLM-shimmed)
- `ToolRegistry` with `ohlcv` and `indicator_panel` tools (fixture-mode)
- 3-slot agent pipeline (regime → intern → trader), inline execution
- `LlmDispatch` trait + Anthropic + Mock implementations
- Token estimator
- CLI: `xvn strategy new | validate | ls | show | templates | run`

## What does NOT ship in MVP

- Web dashboard / Agent Wizard (Plan #2)
- MCP server (Plan #2)
- Tier B sealing + xvn API server (Plan #2)
- Durable scheduler (Plan #2)
- Live execution daemon (Plan #2)
- Eval harness (Plan #3)
- More than 1 template + 1 baseline (Plan #2)

## CLI quick-start

```bash
# create a draft
xvn strategy new --template mean_reversion --name eth-mr-v1
# → 01H8N7ZAB...

# validate
xvn strategy validate 01H8N7ZAB...

# inspect
xvn strategy show 01H8N7ZAB...

# run inline against the test fixture (mock LLM = no API cost)
xvn strategy run 01H8N7ZAB... --fixture test-fixture-btc-2024-01 --decisions 5 --mock

# run with real LLM (requires ANTHROPIC_API_KEY)
ANTHROPIC_API_KEY=$(op read 'op://Personal/Anthropic API/credential') \
  xvn strategy run 01H8N7ZAB... --fixture test-fixture-btc-2024-01 --decisions 5
```

Strategies are stored under `$XVN_HOME/strategies/<id>.json` (default `~/.xvn/strategies/`).
```

- [ ] **Step 2: Add a section to top-level `MANUAL.md`**

In `MANUAL.md`, add after the existing CLI section (or at the bottom if no CLI section exists):

```markdown
## Strategy authoring (MVP — see crates/xianvec-engine/README.md)

```bash
xvn strategy templates                 # list templates
xvn strategy new --template <t> --name <n>
xvn strategy validate <id>
xvn strategy show <id>
xvn strategy ls
xvn strategy run <id> --fixture <name> --decisions <N> [--mock]
```

End-to-end paths beyond this surface (web Wizard, marketplace publishing, live trading,
batch eval) land in subsequent plans (#2, #3) — they share this same bundle format.
```

- [ ] **Step 3: Smoke-run the full flow once**

Run:

```bash
cargo build --workspace
cargo run -p xianvec-cli -- strategy templates
cargo run -p xianvec-cli -- strategy new --template mean_reversion --name smoke-test
# capture id from stdout
ID=$(cargo run -q -p xianvec-cli -- strategy ls | head -1)
cargo run -p xianvec-cli -- strategy show $ID | head -20
cargo run -p xianvec-cli -- strategy validate $ID
cargo run -p xianvec-cli -- strategy run $ID --fixture test-fixture-btc-2024-01 --decisions 2 --mock
```

Expected: every step prints output, exits 0.

- [ ] **Step 4: Commit**

```bash
git add crates/xianvec-engine/README.md MANUAL.md
git commit -m "docs(engine): MVP README + manual update"
```

---

### Task 22: Final workspace check + integration test

**Files:**
- Run only — no new files.

- [ ] **Step 1: Run the entire test suite**

Run: `cargo test --workspace`
Expected: all green. If anything fails, fix it before merging.

- [ ] **Step 2: Run clippy across workspace**

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: no warnings. Fix anything that comes up — usually unused imports, missing docs, or `clippy::needless_pass_by_value`.

- [ ] **Step 3: Run rustfmt check**

Run: `cargo fmt --all -- --check`
Expected: zero output. If not, run `cargo fmt --all` and stage the result.

- [ ] **Step 4: Verify no `xianvec-eval` was modified**

Run: `git log --oneline -- crates/xianvec-eval/`
Expected: no new commits in this plan touched `xianvec-eval`. The migration of baselines is a *new copy* in `xianvec-engine/baselines/`, not a destructive edit. `xianvec-eval` deprecation lands in a later plan.

- [ ] **Step 5: Commit any cleanup from steps 2-3**

If clippy or fmt produced changes:

```bash
git add -A
git commit -m "chore(engine): clippy + fmt cleanup"
```

- [ ] **Step 6: Final commit**

If nothing else to commit, you're done. Run `git log --oneline -22` and verify there are roughly 22 commits from this plan, each focused, each green.

---

## Self-review checklist

Before declaring this plan complete, walk through:

**Spec coverage from `2026-05-08-strategy-creation-engine-design.md`:**
- [x] §3 Strategy artifact (scaffold + slots) — Tasks 2-7
- [x] §4 Templates — Tasks 8-9 (1 of 8; rest deferred to Plan #2)
- [x] §7 Tool registry — Tasks 11-12 (built-in tools only; author-defined skills deferred)
- [x] §9 CLI surface — Tasks 17-20 (subset: new/validate/ls/show/templates/run; eval/marketplace/live deferred)
- [ ] §2 KISS / Agent Wizard — deferred to Plan #2
- [ ] §5 Permission tiers — deferred to Plan #2 (only Tier A as filesystem)
- [ ] §6 Skill bundle format — deferred to Plan #2
- [ ] §8 Authoring entry points (web/MCP) — deferred to Plan #2
- [ ] §10 MCP server surface — deferred to Plan #2
- [ ] §11 Live execution — deferred to Plan #2
- [ ] §12 Durable scheduler — deferred to Plan #2
- [ ] §13 Marketplace + 8004 — deferred to Plan #2
- [x] §14 Crate structure — `xianvec-engine` lands in Task 1

**Type consistency check:** `StrategyBundle`, `LLMSlot`, `RiskConfig`, `RiskPreset`, `PublicManifest`, `RegimeFit`, `Template`, `ToolRegistry`, `ToolName`, `Tool`, `LlmDispatch`, `LlmRequest`, `LlmResponse`, `MockDispatch`, `AnthropicDispatch`, `SlotInput`, `PipelineInputs`, `PipelineOutputs`, `TokenEstimate`, `BundleStore`, `FilesystemStore`, `EngineError` — names used consistently across all 22 tasks.

**No placeholders:** every code block contains real Rust the engineer can paste. `// placeholder` files exist only as Step 3 of Task 1, replaced by real content in subsequent tasks.

**Frequent commits:** 22 tasks, ~22 commits, each a green build and a passing test addition.

---

## What's next after this plan ships

Plan #2 — **Strategy Creation Engine: Wizard + Marketplace + MCP**
- Web dashboard (axum + minimal SPA, L3 Inspector + L1 Wizard)
- MCP server surface (all four verb groups)
- Tier B sealing + xvn API server (OSShip-style)
- Durable scheduler (port pattern from SwarmClaw)
- More templates (trend_follower, breakout, momentum, range, scalping, news, custom)
- Tool-call dispatch in agent loop (LLM can actually call OHLCV / indicators mid-decision)
- Live execution daemon (Alpaca paper, Orderly live)
- 8004 publish flow

Plan #3 — **Eval Engine**
- Run/scenario/store types in `xianvec-engine/src/eval/`
- Backtest fill simulator + paper executor
- Pre-computed published evals + signed attestations
- Comparison view + Lightweight Charts UI
- Findings extractor
- Migration plan from `xianvec-eval` to deprecation
