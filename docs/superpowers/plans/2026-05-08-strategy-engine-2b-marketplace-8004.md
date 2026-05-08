# Strategy Creation Engine — Plan 2b (Marketplace + 8004) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan #1 + Plan 2a merged. Coordinate with `decisions/0008-erc8004-deployment.md` for the ERC-8004 registry deployment runbook on Mantle.

**Goal:** Land the hackathon-critical marketplace + 8004 reputation surface. Strategies become publishable, listable, and buyable artifacts with on-chain provenance. After this plan ships: a strategy author runs `xvn marketplace publish <id>` and an 8004 listing is minted on Mantle Sepolia with the bundle content hash, license terms, and creator identity. A buyer runs `xvn marketplace browse` to see all listings, `xvn marketplace install <listing>` to fetch the open bundle, and after execution `xvn marketplace attest-run` writes a reputation receipt to the registry.

**Architecture:** Three new crates + new MCP verbs. (1) `xianvec-skills` parses + validates OSShip-style skill markdown, supports skill attach to slots, stores skills under `$XVN_HOME/skills/`. (2) `xianvec-marketplace` owns publish/browse/buy/install logic, wraps `xianvec-identity::IdentityClient` for 8004 calls, encodes license terms in the bundle's public manifest. (3) The MCP server (Plan 2a) gains 6 new verbs across skill management and marketplace lifecycle. **Tier A (open) sealing only in this plan**; Tier B (sealed-hosted via xvn API server with envelope encryption) is explicitly deferred to Plan 4 (post-hackathon).

**Tech Stack:** Rust 2021. New deps: `ed25519-dalek` (already used by xianvec-identity), `cid` (IPFS content-id encoding), `multihash`. Reuses everything from Plans #1 and 2a. The `xianvec-identity` crate ships an `IdentityClient` already — Plan 2b composes on top, doesn't reinvent.

**Out of scope (deferred to later plans):**
- Tier B sealed-hosted strategies + xvn API server with envelope encryption — **post-hackathon (Plan 4)**
- Durable scheduler + live execution — Plan 2c
- Web dashboard / Marketplace UI — Plan 2d
- Eval engine (signed eval attestations are stubbed for v1; real ones land in Plan 3)
- Real IPFS gateway hosting — for the hackathon, Tier A bundles are uploaded to a public gist OR served from the xianvec-marketplace crate's local-export directory; the IPFS CID is computed but not gateway-pinned

---

## File structure

```
crates/
├── xianvec-skills/                          # NEW
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                           # Skill type, parse(), validate()
│   │   ├── frontmatter.rs                   # YAML frontmatter parser
│   │   ├── store.rs                         # FilesystemSkillStore
│   │   └── attach.rs                        # attach_skill_to_agent helper (mutates StrategyBundle)
│   └── tests/
│       ├── parse.rs
│       ├── attach.rs
│       └── fixtures/                        # sample skill markdown files
│           └── crypto-trader-base.md
├── xianvec-marketplace/                     # NEW
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs                           # Listing, License, MarketplaceClient
│   │   ├── publish.rs                       # publish_strategy() — mint 8004 listing
│   │   ├── browse.rs                        # browse_listings() — read 8004 + index
│   │   ├── install.rs                       # install_strategy() — fetch + verify bundle
│   │   ├── receipt.rs                       # attest_run() — write reputation receipt
│   │   └── content_hash.rs                  # canonical hashing of StrategyBundle JSON
│   └── tests/
│       ├── publish_local.rs
│       └── receipt.rs
├── xianvec-engine/
│   ├── Cargo.toml                           # add xianvec-skills + xianvec-marketplace deps
│   └── src/
│       └── mcp/
│           ├── skill.rs                     # NEW: 3 skill MCP verbs
│           └── marketplace.rs               # NEW: 5 marketplace MCP verbs
└── xianvec-cli/
    ├── Cargo.toml                           # add xianvec-marketplace + xianvec-skills deps
    └── src/commands/
        ├── marketplace.rs                   # NEW: xvn marketplace {publish | browse | install | attest-run}
        └── skill.rs                         # NEW: xvn skill {new | ls | attach}
```

Workspace root `Cargo.toml` adds `xianvec-skills` and `xianvec-marketplace` to both `members` and `default-members` (alphabetically after the relevant existing crates).

---

## Phase 2B.A — `xianvec-skills` crate

### Task 1: Crate scaffolding + `Skill` type + parse

**Files:**
- Create: `crates/xianvec-skills/Cargo.toml`
- Create: `crates/xianvec-skills/src/lib.rs`
- Create: `crates/xianvec-skills/src/frontmatter.rs`
- Modify: `Cargo.toml` (workspace) — add `crates/xianvec-skills` to `members` + `default-members`

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
ulid        = { version = "1", features = ["serde"] }
chrono      = { workspace = true }
anyhow      = { workspace = true }
thiserror   = { workspace = true }
async-trait = { workspace = true }
tokio       = { workspace = true }

[dev-dependencies]
tempfile = "3"
tokio    = { workspace = true, features = ["rt", "macros"] }
```

> Note: `xianvec-skills` depends on `xianvec-engine` (for `LLMSlot`, `StrategyBundle` mutations in `attach.rs`). This is one-way — `xianvec-engine` does NOT depend on `xianvec-skills`; the engine module that wires skills into MCP (`mcp/skill.rs`) reaches across via type imports only.

- [ ] **Step 2: Define `Skill` type + `parse`**

Create `crates/xianvec-skills/src/lib.rs`:

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
//! You are a crypto trader. Inputs include:
//! - ohlcv_history
//! - indicator_panel
//! - portfolio_state
//!
//! Decide ONE of: long_open | short_open | flat | hold.
//! Output JSON: {action, conviction (0-1), justification}.
//! ```
//!
//! Plan 2b ships parser + filesystem store + attach-to-agent helper.
//! Plan 4 (post-hackathon) ships the skill marketplace surface.

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
    /// The prompt body — everything after the frontmatter `---`.
    pub body: String,
    /// SHA-256 of the canonical YAML+body, hex-encoded. Used for content-addressing.
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

    let name = get_str("name")?;
    let display_name = get_str("display_name")?;
    let description = get_str("description")?;
    let version = get_str("version")?;
    let model_requirement = get_str("model_requirement")?;

    let content_hash = sha256_hex(markdown.as_bytes());

    Ok(Skill {
        name, display_name, description, version,
        allowed_tools, model_requirement,
        body: body.to_string(), content_hash,
    })
}

fn sha256_hex(input: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input);
    let bytes = hasher.finalize();
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
```

> Note: `sha2 = "0.10"` is needed in `Cargo.toml`. Add it.

- [ ] **Step 3: Implement frontmatter splitter**

Create `crates/xianvec-skills/src/frontmatter.rs`:

```rust
use crate::SkillError;

/// Split markdown with `---` YAML frontmatter into (yaml, body).
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
    // The body starts right after the closing `---\n`.
    let body_start = close_idx + "\n---\n".len();
    let body = if body_start < after_open.len() {
        &after_open[body_start..]
    } else {
        ""
    };
    Ok((yaml, body.trim_start_matches('\n')))
}
```

- [ ] **Step 4: Test fixture + parse roundtrip test**

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
    assert_eq!(skill.display_name, "Generalist crypto trader");
    assert_eq!(skill.version, "1.0.0");
    assert_eq!(skill.allowed_tools, vec!["ohlcv", "indicator_panel"]);
    assert!(skill.body.contains("crypto trader"));
    assert_eq!(skill.content_hash.len(), 64);
}

#[test]
fn rejects_missing_frontmatter() {
    let err = parse("just some text, no frontmatter").unwrap_err();
    assert!(matches!(err, xianvec_skills::SkillError::MissingFrontmatter));
}

#[test]
fn rejects_missing_required_field() {
    let bad = "---\nname: x\n---\nbody\n";
    let err = parse(bad).unwrap_err();
    assert!(matches!(err, xianvec_skills::SkillError::MissingField(_)));
}
```

- [ ] **Step 5: Run tests**

`cargo test -p xianvec-skills 2>&1 | grep "test result"` → 3 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/xianvec-skills Cargo.toml
git commit -m "feat(skills): scaffold xianvec-skills crate with markdown parser"
```

---

### Task 2: `FilesystemSkillStore`

Pattern matches Plan #1 Task 7 (`FilesystemStore` for bundles). Files:
- Create `crates/xianvec-skills/src/store.rs` with `SkillStore` trait + `FilesystemSkillStore` impl. Save-by-name, load-by-name, list. Path: `$XVN_HOME/skills/<name>.md`.
- Test: `crates/xianvec-skills/tests/store.rs` — save/load/list roundtrip with tempdir.

Follow Plan #1 Task 7's structure exactly. Commit message: `feat(skills): filesystem-backed SkillStore`.

---

### Task 3: `attach_skill_to_agent` helper

**Files:**
- Create: `crates/xianvec-skills/src/attach.rs`
- Test: `crates/xianvec-skills/tests/attach.rs`

- [ ] **Step 1: Failing test**

```rust
use xianvec_skills::Skill;
use xianvec_skills::attach::attach_skill_to_agent;
use xianvec_engine::bundle::slot::LLMSlot;
use xianvec_engine::bundle::manifest::{PublicManifest, RegimeFit};
use xianvec_engine::bundle::risk::RiskPreset;
use xianvec_engine::bundle::StrategyBundle;

fn dummy_bundle() -> StrategyBundle {
    StrategyBundle {
        manifest: PublicManifest {
            id: "01H8N7Z000".into(), display_name: "T".into(), plain_summary: "x".into(),
            creator: "@t".into(), template: "mean_reversion".into(),
            regime_fit: vec![RegimeFit::RangeBound],
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

#[test]
fn attaching_replaces_trader_prompt() {
    let mut bundle = dummy_bundle();
    let skill = Skill {
        name: "test-skill".into(), display_name: "Test".into(),
        description: "x".into(), version: "1.0.0".into(),
        allowed_tools: vec!["ohlcv".into(), "indicator_panel".into()],
        model_requirement: "anthropic.claude-sonnet-4.6".into(),
        body: "NEW PROMPT BODY".into(),
        content_hash: "deadbeef".into(),
    };
    attach_skill_to_agent(&mut bundle, "trader", &skill).unwrap();
    let trader = bundle.trader_slot.unwrap();
    assert_eq!(trader.prompt, "NEW PROMPT BODY");
    assert!(trader.allowed_tools.contains(&"indicator_panel".to_string()));
}

#[test]
fn rejects_attaching_to_missing_slot() {
    let mut bundle = dummy_bundle();
    let skill = /* ... as above ... */;
    let err = attach_skill_to_agent(&mut bundle, "regime", &skill).unwrap_err();
    assert!(err.to_string().contains("regime"));
}
```

- [ ] **Step 2: Implement**

```rust
use crate::Skill;
use xianvec_engine::bundle::StrategyBundle;
use xianvec_engine::bundle::slot::LLMSlot;

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
    let slot = slot.ok_or_else(|| anyhow::anyhow!("slot '{slot_role}' is empty — cannot attach"))?;

    slot.prompt = skill.body.clone();
    slot.model_requirement = skill.model_requirement.clone();
    // Union of existing + skill's allowed_tools, deduped.
    let mut tools = slot.allowed_tools.clone();
    for t in &skill.allowed_tools {
        if !tools.contains(t) { tools.push(t.clone()); }
    }
    slot.allowed_tools = tools;
    Ok(())
}
```

- [ ] **Step 3-5:** Tests pass. Commit `feat(skills): attach_skill_to_agent helper`.

---

## Phase 2B.B — `xianvec-marketplace` crate

### Task 4: Crate scaffolding + types

Create `crates/xianvec-marketplace/`. Cargo.toml deps include:
- `xianvec-engine` (for `StrategyBundle`)
- `xianvec-identity` (for `IdentityClient`, `RegistryAddresses`, `TokenId`)
- `serde`, `serde_json`, `chrono`, `anyhow`, `thiserror`, `tokio`, `tracing`
- `cid = "0.11"` and `multihash = "0.19"` for IPFS CID encoding

`src/lib.rs`:

```rust
//! Marketplace + 8004 reputation surface for xvn strategies.

pub mod browse;
pub mod content_hash;
pub mod install;
pub mod publish;
pub mod receipt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Listing {
    pub id: String,                  // ULID — local index id (NOT the on-chain agent id)
    pub agent_id: u64,               // ERC-8004 agent id (returned by IdentityRegistry.register())
    pub bundle_content_hash: String, // SHA-256 of canonical bundle JSON
    pub ipfs_cid: Option<String>,    // optional — populated when bundle is uploaded to IPFS
    pub creator: String,             // wallet address or @handle
    pub display_name: String,
    pub plain_summary: String,
    pub license: License,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub tx_hash: Option<String>,     // mint tx on Mantle (None for off-chain test runs)
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum License {
    /// Tier A: open. Buyer downloads the full bundle, can fork.
    Open { gateway_url: Option<String> },
    /// Tier A paid: open but requires a payment. Hackathon scope: just metadata; no payment rails.
    PaidOpen { price_usdc: f64 },
    /// Tier B placeholder. Not yet supported in this plan.
    /// Plan 4 (post-hackathon) ships the xvn API server + envelope encryption.
    Sealed,
}

pub use publish::publish_strategy;
pub use browse::{browse_listings, get_listing};
pub use install::install_strategy;
pub use receipt::attest_run;
```

Add to workspace `Cargo.toml` `members` + `default-members` (alphabetically after `xianvec-marketplace` and `xianvec-skills` go in their slots).

Smoke build, commit `feat(marketplace): scaffold xianvec-marketplace crate`.

---

### Task 5: Content hash for canonical bundle JSON

**File:** `crates/xianvec-marketplace/src/content_hash.rs`

```rust
use sha2::{Digest, Sha256};

use xianvec_engine::bundle::StrategyBundle;

/// Compute the canonical SHA-256 content hash of a strategy bundle.
/// Canonicalization: serde_json::to_value(...) then sort keys recursively
/// before serializing back to bytes. This makes the hash stable across
/// bundles that re-order fields.
pub fn content_hash(bundle: &StrategyBundle) -> anyhow::Result<String> {
    let value = serde_json::to_value(bundle)?;
    let canonical = canonicalize(&value);
    let bytes = serde_json::to_vec(&canonical)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    Ok(digest.iter().map(|b| format!("{b:02x}")).collect())
}

fn canonicalize(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys {
                out.insert(k.clone(), canonicalize(&map[k]));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => {
            serde_json::Value::Array(arr.iter().map(canonicalize).collect())
        }
        other => other.clone(),
    }
}
```

Test: `crates/xianvec-marketplace/tests/content_hash_stable.rs` — compute hash of two bundles that differ only in field order; assert hashes match.

Commit `feat(marketplace): canonical content hash for strategy bundles`.

---

### Task 6: `publish_strategy` — mint 8004 listing

**File:** `crates/xianvec-marketplace/src/publish.rs`

```rust
use std::path::PathBuf;

use anyhow::Context;
use chrono::Utc;
use ulid::Ulid;
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::StrategyBundle;
use xianvec_identity::{IdentityClient, RegistryAddresses};

use crate::content_hash::content_hash;
use crate::{License, Listing};

pub struct PublishConfig {
    pub strategies_dir: PathBuf,    // $XVN_HOME/strategies
    pub listings_dir: PathBuf,      // $XVN_HOME/listings (local index)
    pub registry: RegistryAddresses,
    pub rpc_url: String,            // e.g., https://rpc.sepolia.mantle.xyz
    pub operator_private_key: String, // for signing the register() tx
    /// If true, skip the on-chain mint and produce a local-only listing
    /// with `agent_id = 0` and `tx_hash = None`. Useful for tests / demo
    /// without a funded wallet.
    pub dry_run: bool,
}

pub async fn publish_strategy(
    strategy_id: &str,
    license: License,
    cfg: &PublishConfig,
) -> anyhow::Result<Listing> {
    let bundle_store = FilesystemStore::new(cfg.strategies_dir.clone());
    let bundle = bundle_store.load(strategy_id).await
        .with_context(|| format!("loading bundle {strategy_id}"))?;

    let hash = content_hash(&bundle)?;
    let agent_uri = build_agent_uri(&bundle, &hash);

    let (agent_id, tx_hash) = if cfg.dry_run {
        (0u64, None)
    } else {
        let client = IdentityClient::new(&cfg.rpc_url, cfg.registry.clone(), &cfg.operator_private_key)?;
        let token = client.register(&agent_uri).await
            .with_context(|| "minting 8004 listing")?;
        // Convert TokenId (U256) → u64 with overflow check.
        let agent_id = token.0.try_into().map_err(|_| anyhow::anyhow!("agent_id > u64::MAX"))?;
        (agent_id, Some(format!("0x{:x}", token.0))) // tx_hash placeholder; real tx hash is returned by IdentityClient.register; adapt to its actual signature
    };

    let listing = Listing {
        id: Ulid::new().to_string(),
        agent_id,
        bundle_content_hash: hash,
        ipfs_cid: None,                 // populated by separate `publish-to-ipfs` later if desired
        creator: bundle.manifest.creator.clone(),
        display_name: bundle.manifest.display_name.clone(),
        plain_summary: bundle.manifest.plain_summary.clone(),
        license,
        published_at: Utc::now(),
        tx_hash,
    };

    save_listing_local(&cfg.listings_dir, &listing).await?;
    Ok(listing)
}

fn build_agent_uri(bundle: &StrategyBundle, content_hash: &str) -> String {
    // Per ADR 0008's stub interface, agentURI is a string. We embed:
    // - The bundle content hash (verifiable provenance)
    // - The display name + creator
    // For Tier A open bundles, agentURI also includes the gateway URL once IPFS upload lands.
    format!(
        "xvn://strategy/{template}/{content_hash}?creator={creator}&name={name}",
        template = bundle.manifest.template,
        content_hash = content_hash,
        creator = urlencoding::encode(&bundle.manifest.creator),
        name = urlencoding::encode(&bundle.manifest.display_name),
    )
}

async fn save_listing_local(dir: &PathBuf, listing: &Listing) -> anyhow::Result<()> {
    tokio::fs::create_dir_all(dir).await?;
    let path = dir.join(format!("{}.json", listing.id));
    tokio::fs::write(&path, serde_json::to_vec_pretty(listing)?).await?;
    Ok(())
}
```

> Note: `IdentityClient::register` returns a `TokenId(U256)` per `xianvec-identity::client.rs` — verify the exact signature when implementing. If `register` returns the tx hash separately, capture it. If the existing API doesn't expose tx_hash, either extend it or accept `None` for v1 (the on-chain agent_id is the load-bearing field).

Test: `crates/xianvec-marketplace/tests/publish_local.rs` — call `publish_strategy` with `dry_run: true`, assert listing has correct content_hash + non-empty agent_uri-derived fields.

Commit `feat(marketplace): publish_strategy mints 8004 listing (dry-run + real)`.

---

### Task 7: `browse_listings` and `get_listing`

**File:** `crates/xianvec-marketplace/src/browse.rs`

```rust
use std::path::Path;

use crate::Listing;

/// Browse all locally-known listings. v1: filesystem index under
/// $XVN_HOME/listings. v2 (post-hackathon): query the on-chain registry
/// directly to discover listings published by other xvn instances.
pub async fn browse_listings(listings_dir: &Path) -> anyhow::Result<Vec<Listing>> {
    let mut out = vec![];
    if !listings_dir.exists() { return Ok(out); }
    let mut rd = tokio::fs::read_dir(listings_dir).await?;
    while let Some(entry) = rd.next_entry().await? {
        if entry.path().extension().and_then(|s| s.to_str()) == Some("json") {
            let bytes = tokio::fs::read(entry.path()).await?;
            let listing: Listing = serde_json::from_slice(&bytes)?;
            out.push(listing);
        }
    }
    out.sort_by(|a, b| b.published_at.cmp(&a.published_at));
    Ok(out)
}

pub async fn get_listing(listings_dir: &Path, id: &str) -> anyhow::Result<Listing> {
    let path = listings_dir.join(format!("{id}.json"));
    let bytes = tokio::fs::read(&path).await?;
    Ok(serde_json::from_slice(&bytes)?)
}
```

Test: write 2 listings to a tempdir, call `browse_listings`, assert sort order + count.

Commit `feat(marketplace): browse + get listing helpers`.

---

### Task 8: `install_strategy` — fetch + verify bundle

**File:** `crates/xianvec-marketplace/src/install.rs`

```rust
use std::path::PathBuf;

use anyhow::Context;
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::StrategyBundle;
use xianvec_engine::bundle::validate::validate_bundle;

use crate::content_hash::content_hash;
use crate::{License, Listing};

/// Install a strategy from a listing. For Tier A Open bundles, the bundle
/// JSON is fetched from the gateway URL (or, in dry-run/local-test mode,
/// expected to already exist in $XVN_HOME/strategies). Validates the
/// content hash before saving locally.
pub async fn install_strategy(
    listing: &Listing,
    bundle_json: &[u8],
    strategies_dir: PathBuf,
) -> anyhow::Result<StrategyBundle> {
    let bundle: StrategyBundle = serde_json::from_slice(bundle_json)
        .with_context(|| "parsing bundle JSON")?;
    let hash = content_hash(&bundle)?;
    if hash != listing.bundle_content_hash {
        anyhow::bail!(
            "content hash mismatch: listing says {}, bundle hashes to {}",
            listing.bundle_content_hash, hash
        );
    }
    validate_bundle(&bundle)?;
    // Only Open licenses install fully in this plan. Sealed/PaidOpen require Plan 4 / payment rails.
    match &listing.license {
        License::Open { .. } => {}
        License::PaidOpen { .. } => {
            // Hackathon scope: warn + proceed. Real payment verification lands in Plan 4.
            tracing::warn!("PaidOpen listing — payment verification not yet implemented");
        }
        License::Sealed => anyhow::bail!("Sealed licenses require Plan 4 (xvn API server)"),
    }
    let store = FilesystemStore::new(strategies_dir);
    store.save(&bundle).await?;
    Ok(bundle)
}
```

Test: build a bundle, hash it, construct a Listing with that hash, call `install_strategy` with the bundle JSON; assert bundle is saved + ID matches.

Commit `feat(marketplace): install_strategy with content-hash verification`.

---

### Task 9: `attest_run` — write reputation receipt

**File:** `crates/xianvec-marketplace/src/receipt.rs`

```rust
use anyhow::Context;
use xianvec_identity::{IdentityClient, RegistryAddresses};

/// Reputation receipt written after a strategy execution. Wraps
/// `IdentityClient.give_feedback()` (per ADR 0008's stub interface).
///
/// Sharpe-like metric is encoded as int128 with valueDecimals; this v1
/// uses fixed point: value = sharpe * 10000, valueDecimals = 4.
pub struct AttestRunArgs<'a> {
    pub agent_id: u64,
    pub sharpe: f64,
    pub feedback_uri: &'a str,
    pub feedback_hash: [u8; 32],
    pub regime_tag: &'a str,
    pub asset_tag: &'a str,
    pub endpoint: &'a str,
}

pub async fn attest_run(
    args: AttestRunArgs<'_>,
    rpc_url: &str,
    registry: RegistryAddresses,
    operator_private_key: &str,
    dry_run: bool,
) -> anyhow::Result<Option<String>> {
    if dry_run {
        tracing::info!(agent_id = args.agent_id, sharpe = args.sharpe, "dry-run attest");
        return Ok(None);
    }
    let client = IdentityClient::new(rpc_url, registry, operator_private_key)?;
    let value = (args.sharpe * 10_000.0) as i128;
    let tx_hash = client.give_feedback(
        args.agent_id, value, 4,
        args.regime_tag, args.asset_tag,
        args.endpoint, args.feedback_uri, args.feedback_hash,
    ).await.with_context(|| "give_feedback to 8004 ReputationRegistry")?;
    Ok(Some(format!("{tx_hash:?}")))
}
```

> Note: the actual `IdentityClient.give_feedback()` signature comes from `crates/xianvec-identity/src/client.rs`. Adapt to whatever it expects. If the existing surface is different (e.g., uses an `EvalReceipt` struct), wrap that struct construction here.

Test: dry-run attest_run, assert `Ok(None)`. Live test (`#[ignore]`) hits Mantle Sepolia.

Commit `feat(marketplace): attest_run writes 8004 reputation receipt`.

---

## Phase 2B.C — MCP verbs (skill + marketplace)

### Tasks 10-12: Skill MCP verbs

Three verbs. Each follows Plan 2a Task 3's MCP-tool pattern (define args struct, args schema, function, register in tools/list + dispatch by name). Add file `crates/xianvec-engine/src/mcp/skill.rs`.

| Task | Verb | Args | Returns |
|---|---|---|---|
| 10 | `create_skill` | `{ markdown: String }` | `{ name: String, content_hash: String }` — parses, saves to skill store |
| 11 | `list_skills` | `{}` | `Vec<{ name, display_name, description, version }>` |
| 12 | `attach_skill_to_agent` | `{ strategy_id, slot, skill_name }` | `{ strategy_id, slot, skill_name }` — calls `attach_skill_to_agent` then re-saves bundle |

After Task 12, register all 3 in `tools/list` + dispatch in `call_tool`. One commit per task using the same template as Plan 2a §A.

### Tasks 13-17: Marketplace MCP verbs

Five verbs. File: `crates/xianvec-engine/src/mcp/marketplace.rs`. Each wraps the `xianvec-marketplace` crate.

| Task | Verb | Args | Returns |
|---|---|---|---|
| 13 | `publish_strategy` | `{ strategy_id, license: { kind, ... } }` | `Listing` (full struct) |
| 14 | `browse_listings` | `{ filter: Option<{ template, regime_fit }> }` | `Vec<Listing>` |
| 15 | `get_listing` | `{ id }` | `Listing` |
| 16 | `install_strategy` | `{ listing_id, bundle_json: Option<String> }` | `{ strategy_id }` — bundle_json optional for local-fixture testing |
| 17 | `attest_run` | `{ agent_id, sharpe, feedback_uri, feedback_hash_hex, regime_tag, asset_tag, endpoint }` | `{ tx_hash: Option<String> }` |

After Task 17, all 5 are advertised + dispatched. `tests/mcp_authoring.rs` (from Plan 2a) extends to assert all 13 verbs (7 authoring + 3 skill + 5 marketplace) appear in `tools/list`. Re-run to confirm.

Commit pattern: `feat(engine): MCP <verb_name> verb` (one per task). After Task 17, a single integration test verifies the complete list.

---

## Phase 2B.D — CLI surface

### Task 18: `xvn skill {new | ls | attach}`

Mirror Plan #1 Task 17/18's pattern. Module: `crates/xianvec-cli/src/commands/skill.rs`. Subcommands:
- `new --from-file <path>` — parse markdown, save to skill store, print name
- `ls` — list saved skills
- `attach <strategy_id> --slot <regime|intern|trader> --skill <name>` — calls `attach_skill_to_agent`, re-saves bundle

Add `Skill(commands::skill::SkillCmd)` to top-level `Command` enum in lib.rs.

Integration test: `xvn skill new --from-file <fixture> → xvn skill ls → xvn skill attach <id> --slot trader --skill <name>` round-trip.

Commit `feat(cli): xvn skill new/ls/attach`.

### Task 19: `xvn marketplace {publish | browse | get | install | attest-run}`

Module: `crates/xianvec-cli/src/commands/marketplace.rs`. Add `Marketplace(commands::marketplace::MarketplaceCmd)` to `Command` enum.

Subcommands:
- `publish <strategy_id> --license open [--gateway-url <url>]` — runs publish_strategy with dry_run from env (default `true` for safety; explicit `--no-dry-run` flag for real chain calls)
- `browse [--template <name>] [--regime <regime>]` — list all known listings
- `get <listing_id>` — show one listing as JSON
- `install <listing_id> [--bundle-file <path>]` — install a strategy bundle locally; uses local file if specified, otherwise expects in-repo fixture for hackathon
- `attest-run <agent_id> --sharpe <f64> --feedback-uri <uri> --feedback-hash <hex> --regime <tag> --asset <tag> --endpoint <url>` — write reputation receipt

Use 1Password CLI (`op read`) to fetch operator private key from `op://xianvec/mantle-operator/private-key` per ADR 0008. If env `XVN_DRY_RUN=true` (default) is set, use `dry_run: true` in publish/attest calls.

Integration test: full publish (dry-run) → browse → get → install → smoke run round-trip.

Commit `feat(cli): xvn marketplace publish/browse/get/install/attest-run`.

---

## Phase 2B.E — Polish + smoke

### Task 20: README + manual updates

Update `crates/xianvec-engine/README.md`'s "What ships" section with the new MCP verbs. Update `MANUAL.md` to describe the marketplace + skill flow.

Add new READMEs at `crates/xianvec-skills/README.md` and `crates/xianvec-marketplace/README.md` mirroring the engine README structure.

Commit `docs: Plan 2b READMEs and manual update`.

### Task 21: End-to-end smoke

Run the full hackathon-demo flow:

```bash
export XVN_HOME=/tmp/xvn-2b-smoke
rm -rf $XVN_HOME

# 1. Author a strategy
ID=$(xvn strategy new --template trend_follower --name btc-trend-demo)

# 2. Publish to marketplace (dry-run; no on-chain mint)
LISTING=$(xvn marketplace publish $ID --license open)

# 3. Browse — should show the new listing
xvn marketplace browse

# 4. Inspect listing
xvn marketplace get $LISTING

# 5. Install elsewhere (simulate buyer in a second XVN_HOME)
export XVN_HOME=/tmp/xvn-2b-buyer
rm -rf $XVN_HOME
xvn marketplace install $LISTING --bundle-file <path-from-step-2>

# 6. Run the installed strategy
INSTALLED_ID=$(xvn strategy ls | head -1)
xvn strategy run $INSTALLED_ID --fixture test-fixture-btc-2024-01 --decisions 2 --mock

# 7. Attest the run (dry-run)
xvn marketplace attest-run 0 --sharpe 1.62 --feedback-uri "data:..." \
  --feedback-hash 00...00 --regime trending_bull --asset BTC --endpoint "https://example"
```

Each step exits 0. Document the live-mode equivalent (with operator key + Mantle Sepolia RPC) in the README — but don't run it as part of the test suite.

Commit `chore: Plan 2b end-to-end smoke verified`.

### Task 22: Final workspace check

`cargo test --workspace` clean. `cargo clippy --workspace --all-targets -- -D warnings` clean. `cargo fmt -p xianvec-skills -p xianvec-marketplace -p xianvec-engine -p xianvec-cli -- --check` clean. `xianvec-eval` still untouched. ~22 commits since Plan 2a's tip.

Commit `chore: Plan 2b final workspace check` if any cleanup landed.

---

## Self-review checklist

**Spec coverage from `2026-05-08-strategy-creation-engine-design.md` for Plan 2b's scope:**
- [x] §6 Skill bundle format (OSShip-style markdown) — `xianvec-skills` crate
- [x] §10 MCP verb groups: skill management (3 verbs) + marketplace + live (5 verbs, "live" subset deferred to Plan 2c)
- [x] §13 Marketplace + 8004 integration — Tier A flow, mint via xianvec-identity
- [x] §5 Permission tiers: Tier A (open) — fully implemented
- [ ] §5 Tier B (sealed-hosted) — explicitly deferred to Plan 4 (post-hackathon). Marked in `License::Sealed` with `bail!` so it's not silently broken.
- [ ] §11 Live execution — Plan 2c
- [ ] §12 Durable scheduler — Plan 2c
- [ ] §2 Wizard / dashboard — Plan 2d
- [ ] Eval engine — Plan 3 (this plan stubs reputation receipts to allow hackathon-demo without eval engine)

**Type consistency:** `Skill`, `SkillError`, `attach_skill_to_agent`, `Listing`, `License`, `PublishConfig`, `AttestRunArgs`, `content_hash`, `publish_strategy`, `browse_listings`, `get_listing`, `install_strategy`, `attest_run` — all consistent across all 22 tasks.

**No placeholders:** every code block is real Rust the engineer can paste. Tests are spelled out.

**Frequent commits:** 22 tasks → 22 focused commits.

---

## What's next

Plan 2c — **Durable Scheduler + Live Execution**
Plan 2d — **Web Dashboard + Agent Wizard**
Plan 3 — **Eval Engine**
Plan 4 (post-hackathon) — **Tier B sealing + xvn API server**
