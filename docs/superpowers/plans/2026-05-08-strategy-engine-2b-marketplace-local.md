# Strategy Creation Engine — Plan 2b (Marketplace + Skills + Local Reputation) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Depends on:** Plan #1 + Plan 2a merged.
> **Blockchain decision (2026-05-08):** This plan ships a **fully off-chain marketplace + local reputation system**. NO ERC-8004 calls, NO Mantle interaction, NO `xianvec-identity` dependency. Listings + attestations are Ed25519-signed locally and stored in SQLite. The 8004 on-chain integration is a separate future plan (**Plan 5**), gated on eval + strategy engines being battle-tested in production. The smart-contract-surface design spec (`docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`, marked deferred) will inform that future plan.

**Goal:** Strategies are publishable, listable, and buyable artifacts with verifiable provenance — entirely off-chain. After this plan ships: an author runs `xvn marketplace publish <strategy_id>` and a signed listing is written to local SQLite (signed with the author's Ed25519 keypair stored at `$XVN_HOME/keys/`). A buyer runs `xvn marketplace browse` to see all locally-known listings, `xvn marketplace install <listing>` to fetch + verify a bundle's content hash, and after execution `xvn marketplace attest-run` writes an Ed25519-signed reputation receipt. The receipts are export-ready: when Plan 5 ships, a future `xvn marketplace push-to-chain` command will batch-publish them to on-chain registries.

**Architecture:** Two new crates. (1) `xianvec-skills` parses + validates OSShip-style skill markdown, supports skill attach to slots, stores skills under `$XVN_HOME/skills/`. (2) `xianvec-marketplace` owns publish/browse/buy/install/attest logic, manages the author's local Ed25519 keypair, encodes license terms in the bundle's public manifest, persists everything to SQLite. The MCP server (Plan 2a) gains 8 new verbs across skill management + marketplace lifecycle.

**Tech Stack:** Rust 2021. New deps: `ed25519-dalek = "2"`, `hex = "0.4"`, `rand = "0.8"`, `sha2 = "0.10"`, `serde_yaml = "0.9"`. NO `xianvec-identity` dep, NO `alloy` dep, NO Mantle RPC calls. Reuses everything from Plans #1 and 2a.

**Out of scope (deferred to Plans 4 / 5):**
- ALL on-chain integration — 8004 IdentityRegistry, ReputationRegistry, ListingRegistry, Marketplace, LicenseToken contracts on Mantle. Future **Plan 5**, gated on eval + strategy engines being battle-tested.
- Tier B sealed-hosted strategies + xvn API server with envelope encryption — Plan 4 (post-hackathon)
- Real IPFS gateway hosting — Plan 5 (paired with on-chain bundle CIDs)
- x402 buy-rail / EIP-3009 buyWithAuthorization — Plan 5 (commerce contracts)
- Durable scheduler + live execution — Plan 2c
- Web dashboard / Marketplace UI — Plan 2d
- Eval engine signed attestations — produced by Plan 3; this plan only signs marketplace metadata

---

## File structure

```
crates/
├── xianvec-skills/                          # NEW
│   ├── Cargo.toml
│   ├── src/{lib,frontmatter,store,attach}.rs
│   └── tests/{parse,attach,fixtures/crypto-trader-base.md}
├── xianvec-marketplace/                     # NEW (no xianvec-identity dep)
│   ├── Cargo.toml
│   ├── migrations/010_marketplace.sql       # listings + reputation tables
│   ├── src/
│   │   ├── lib.rs                           # Listing, License, ReputationReceipt
│   │   ├── identity.rs                      # author's local Ed25519 keypair (load/create)
│   │   ├── content_hash.rs                  # canonical hashing of StrategyBundle JSON
│   │   ├── publish.rs                       # publish_strategy() — signs + saves locally
│   │   ├── browse.rs                        # browse_listings(), get_listing()
│   │   ├── install.rs                       # install_strategy() — verifies hash + signature
│   │   ├── receipt.rs                       # attest_run() — signs reputation receipt
│   │   └── store.rs                         # SQLite ListingStore + ReputationStore
│   └── tests/{publish_local, content_hash_stable, receipt}.rs
├── xianvec-engine/
│   └── src/mcp/{skill,marketplace}.rs       # NEW: 8 MCP verbs total
└── xianvec-cli/
    └── src/commands/{skill,marketplace}.rs  # NEW CLI subcommands
```

Workspace root `Cargo.toml` adds `xianvec-skills` and `xianvec-marketplace` to both `members` and `default-members`.

---

## Phase 2B.A — `xianvec-skills` crate

### Task 1: Crate scaffolding + `Skill` type + parse

**Files:**
- Create: `crates/xianvec-skills/Cargo.toml`
- Create: `crates/xianvec-skills/src/lib.rs`
- Create: `crates/xianvec-skills/src/frontmatter.rs`
- Modify: workspace `Cargo.toml`

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
//! You are a crypto trader. ...
//! ```

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

- [ ] **Step 3: Frontmatter splitter**

```rust
// frontmatter.rs
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

Create fixture at `crates/xianvec-skills/tests/fixtures/crypto-trader-base.md` with the example shown above. Tests:

```rust
// tests/parse.rs
use xianvec_skills::parse;
const FIXTURE: &str = include_str!("fixtures/crypto-trader-base.md");

#[test]
fn parses_valid_skill() {
    let skill = parse(FIXTURE).expect("parse");
    assert_eq!(skill.name, "crypto-trader-base");
    assert_eq!(skill.allowed_tools, vec!["ohlcv", "indicator_panel"]);
    assert_eq!(skill.content_hash.len(), 64);
}

#[test]
fn rejects_missing_frontmatter() {
    let err = parse("plain text").unwrap_err();
    assert!(matches!(err, xianvec_skills::SkillError::MissingFrontmatter));
}

#[test]
fn rejects_missing_required_field() {
    let bad = "---\nname: x\n---\nbody\n";
    let err = parse(bad).unwrap_err();
    assert!(matches!(err, xianvec_skills::SkillError::MissingField(_)));
}
```

- [ ] **Step 5: Commit**

```bash
cargo test -p xianvec-skills 2>&1 | grep "test result"
git add crates/xianvec-skills Cargo.toml
git commit -m "feat(skills): scaffold xianvec-skills crate with markdown parser"
```

---

### Task 2: `FilesystemSkillStore`

Pattern matches Plan #1 Task 7. `SkillStore` trait + `FilesystemSkillStore` impl. Save-by-name, load-by-name, list. Path: `$XVN_HOME/skills/<name>.md`.

Test: save/load/list roundtrip with tempdir. Commit `feat(skills): filesystem-backed SkillStore`.

---

### Task 3: `attach_skill_to_agent` helper

Mutates a `StrategyBundle`'s named slot (regime/intern/trader): replaces prompt with skill's body, sets model_requirement, unions allowed_tools. Returns error if the slot is empty.

Test: attach to trader, assert prompt + tools updated. Attach to regime when None → error. Commit `feat(skills): attach_skill_to_agent helper`.

---

## Phase 2B.B — `xianvec-marketplace` crate (off-chain)

### Task 4: Crate scaffolding + types

**Files:**
- Create: `crates/xianvec-marketplace/Cargo.toml`
- Create: `crates/xianvec-marketplace/src/lib.rs`
- Modify: workspace `Cargo.toml`

```toml
[package]
name        = "xianvec-marketplace"
description = "Off-chain marketplace + local reputation for xvn (8004 push deferred to Plan 5)"
version.workspace      = true
edition.workspace      = true
rust-version.workspace = true
license.workspace      = true
repository.workspace   = true

[lib]
name = "xianvec_marketplace"
path = "src/lib.rs"

[dependencies]
xianvec-engine = { path = "../xianvec-engine" }

serde         = { workspace = true }
serde_json    = { workspace = true }
sqlx          = { workspace = true }
chrono        = { workspace = true }
ulid          = { version = "1", features = ["serde"] }
ed25519-dalek = "2"
hex           = "0.4"
rand          = "0.8"
sha2          = "0.10"
anyhow        = { workspace = true }
thiserror     = { workspace = true }
async-trait   = { workspace = true }
tokio         = { workspace = true }
tracing       = { workspace = true }

[dev-dependencies]
tempfile = "3"
tokio    = { workspace = true, features = ["rt", "macros"] }
```

```rust
// src/lib.rs
//! Off-chain marketplace + local reputation for xvn strategies.
//!
//! NOT BLOCKCHAIN: this crate intentionally has no Mantle / 8004 / alloy
//! dependencies. Listings and reputation receipts are Ed25519-signed by
//! the author's local keypair and persisted to SQLite. When Plan 5 ships,
//! a `push-to-chain` command will batch-publish these signed artifacts to
//! on-chain registries (per the smart-contract-surface design spec).

pub mod browse;
pub mod content_hash;
pub mod identity;
pub mod install;
pub mod publish;
pub mod receipt;
pub mod store;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Listing {
    pub id: String,                          // ULID
    pub bundle_content_hash: String,         // SHA-256 of canonical bundle JSON
    pub creator_pubkey_hex: String,          // author's Ed25519 pubkey (their identity for v1)
    pub creator_handle: String,              // @handle from bundle.manifest.creator
    pub display_name: String,
    pub plain_summary: String,
    pub license: License,
    pub published_at: chrono::DateTime<chrono::Utc>,
    pub signature_hex: String,               // Ed25519 sig over canonical(JSON of all above except this)
    /// Stays None until Plan 5 publishes to chain. After that, the on-chain
    /// agent_id minted by ListingRegistry is recorded here.
    pub on_chain_agent_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum License {
    /// Tier A: open. Buyer downloads the full bundle, can fork and republish.
    Open,
    /// Tier A paid: open but with a price tag. No payment rails in v1
    /// (Plan 5 commerce contracts add EIP-3009 buyWithAuthorization).
    PaidOpen { price_usdc: f64 },
    /// Tier B placeholder — sealed-hosted via xvn API server.
    /// Plan 4 (post-hackathon) ships this.
    Sealed,
}

pub use browse::{browse_listings, get_listing};
pub use install::install_strategy;
pub use publish::publish_strategy;
pub use receipt::{attest_run, ReputationReceipt};
```

Add `xianvec-marketplace` to workspace `members` + `default-members`. Commit `feat(marketplace): scaffold xianvec-marketplace crate (off-chain)`.

---

### Task 5: Author identity (local Ed25519 keypair)

**File:** `crates/xianvec-marketplace/src/identity.rs`

Manages the author's local signing key. Stored at `$XVN_HOME/keys/author.ed25519`. Generated on first use. Permissions chmod 600 on Unix.

```rust
use std::path::Path;

use ed25519_dalek::{SigningKey, VerifyingKey};
use rand::rngs::OsRng;

pub struct AuthorIdentity {
    pub signing_key: SigningKey,
}

impl AuthorIdentity {
    pub async fn load_or_create(xvn_home: &Path) -> anyhow::Result<Self> {
        let path = xvn_home.join("keys/author.ed25519");
        if path.exists() {
            let bytes = tokio::fs::read(&path).await?;
            let key_bytes: [u8; 32] = bytes.as_slice().try_into()
                .map_err(|_| anyhow::anyhow!("malformed key file at {}", path.display()))?;
            return Ok(Self { signing_key: SigningKey::from_bytes(&key_bytes) });
        }
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let mut csprng = OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        tokio::fs::write(&path, signing_key.to_bytes()).await?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = tokio::fs::metadata(&path).await?.permissions();
            perms.set_mode(0o600);
            tokio::fs::set_permissions(&path, perms).await?;
        }
        Ok(Self { signing_key })
    }

    pub fn pubkey_hex(&self) -> String {
        let pubkey: VerifyingKey = self.signing_key.verifying_key();
        hex::encode(pubkey.as_bytes())
    }
}
```

Tests: load_or_create creates new keypair on first call; second call returns the same one. Commit `feat(marketplace): local Ed25519 author identity`.

---

### Task 6: Content hash for canonical bundle JSON

**File:** `crates/xianvec-marketplace/src/content_hash.rs`

Same canonicalization scheme that Plan 3's eval attestation uses. Expose `canonicalize_value` as `pub` so `publish.rs`, `install.rs`, and `receipt.rs` reuse it.

```rust
use sha2::{Digest, Sha256};
use xianvec_engine::bundle::StrategyBundle;

pub fn content_hash(bundle: &StrategyBundle) -> anyhow::Result<String> {
    let value = serde_json::to_value(bundle)?;
    let canonical = canonicalize_value(&value);
    let bytes = serde_json::to_vec(&canonical)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

pub fn canonicalize_value(v: &serde_json::Value) -> serde_json::Value {
    match v {
        serde_json::Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let mut out = serde_json::Map::new();
            for k in keys { out.insert(k.clone(), canonicalize_value(&map[k])); }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(arr) => serde_json::Value::Array(arr.iter().map(canonicalize_value).collect()),
        other => other.clone(),
    }
}
```

Test: same bundle with field reordering produces identical hash. Commit `feat(marketplace): canonical content hash for bundles`.

---

### Task 7: SQLite schema + ListingStore + ReputationStore

**Files:**
- Create: `crates/xianvec-marketplace/migrations/010_marketplace.sql`
- Create: `crates/xianvec-marketplace/src/store.rs`

```sql
-- migrations/010_marketplace.sql

CREATE TABLE IF NOT EXISTS listings (
    id                       TEXT PRIMARY KEY,
    bundle_content_hash      TEXT NOT NULL,
    creator_pubkey_hex       TEXT NOT NULL,
    creator_handle           TEXT NOT NULL,
    display_name             TEXT NOT NULL,
    plain_summary            TEXT NOT NULL,
    license_kind             TEXT NOT NULL,        -- 'open' | 'paid_open' | 'sealed'
    license_price_usdc       REAL,                  -- NULL except for paid_open
    published_at             TEXT NOT NULL,
    signature_hex            TEXT NOT NULL,
    on_chain_agent_id        INTEGER                -- populated only after Plan 5 push-to-chain
);

CREATE INDEX IF NOT EXISTS idx_listings_creator ON listings(creator_pubkey_hex);
CREATE INDEX IF NOT EXISTS idx_listings_hash ON listings(bundle_content_hash);

CREATE TABLE IF NOT EXISTS reputation_receipts (
    id                        TEXT PRIMARY KEY,
    listing_id                TEXT NOT NULL,
    bundle_content_hash       TEXT NOT NULL,
    scenario_id               TEXT,
    sharpe                    REAL,
    max_drawdown_pct          REAL,
    win_rate                  REAL,
    n_trades                  INTEGER,
    regime_tag                TEXT,
    asset_tag                 TEXT,
    operator_pubkey_hex       TEXT NOT NULL,
    signed_at                 TEXT NOT NULL,
    signature_hex             TEXT NOT NULL,
    -- Plan 5: when this receipt is pushed to ReputationRegistry, the on-chain tx_hash lands here.
    on_chain_tx_hash          TEXT
);

CREATE INDEX IF NOT EXISTS idx_receipts_listing ON reputation_receipts(listing_id);
CREATE INDEX IF NOT EXISTS idx_receipts_hash ON reputation_receipts(bundle_content_hash);
```

`store.rs`: `ListingStore` and `ReputationStore` traits + SQLite impls. Methods:
- `ListingStore`: insert, get_by_id, list_all, list_by_creator, list_by_hash
- `ReputationStore`: insert_receipt, get_by_id, list_for_listing, list_for_hash

Tests with in-memory SQLite. Commit `feat(marketplace): SQLite store for listings + reputation receipts`.

---

### Task 8: `publish_strategy` — sign listing locally

**File:** `crates/xianvec-marketplace/src/publish.rs`

```rust
use std::path::PathBuf;

use anyhow::Context;
use chrono::Utc;
use ed25519_dalek::Signer;
use ulid::Ulid;
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};

use crate::content_hash::{canonicalize_value, content_hash};
use crate::identity::AuthorIdentity;
use crate::store::ListingStore;
use crate::{License, Listing};

pub struct PublishConfig {
    pub strategies_dir: PathBuf,
    pub xvn_home: PathBuf,
}

pub async fn publish_strategy(
    strategy_id: &str,
    license: License,
    cfg: &PublishConfig,
    listing_store: &dyn ListingStore,
) -> anyhow::Result<Listing> {
    let bundle_store = FilesystemStore::new(cfg.strategies_dir.clone());
    let bundle = bundle_store.load(strategy_id).await
        .with_context(|| format!("loading bundle {strategy_id}"))?;
    let hash = content_hash(&bundle)?;
    let identity = AuthorIdentity::load_or_create(&cfg.xvn_home).await?;

    let mut listing = Listing {
        id: Ulid::new().to_string(),
        bundle_content_hash: hash,
        creator_pubkey_hex: identity.pubkey_hex(),
        creator_handle: bundle.manifest.creator.clone(),
        display_name: bundle.manifest.display_name.clone(),
        plain_summary: bundle.manifest.plain_summary.clone(),
        license,
        published_at: Utc::now(),
        signature_hex: String::new(),
        on_chain_agent_id: None,
    };
    let bytes = canonical_listing_bytes(&listing)?;
    let signature = identity.signing_key.sign(&bytes);
    listing.signature_hex = hex::encode(signature.to_bytes());

    listing_store.insert(&listing).await?;
    Ok(listing)
}

pub(crate) fn canonical_listing_bytes(listing: &Listing) -> anyhow::Result<Vec<u8>> {
    let value = serde_json::json!({
        "id": listing.id,
        "bundle_content_hash": listing.bundle_content_hash,
        "creator_pubkey_hex": listing.creator_pubkey_hex,
        "creator_handle": listing.creator_handle,
        "display_name": listing.display_name,
        "plain_summary": listing.plain_summary,
        "license": listing.license,
        "published_at": listing.published_at,
    });
    Ok(serde_json::to_vec(&canonicalize_value(&value))?)
}
```

Tests: publish a draft, assert listing has non-empty signature_hex, content_hash matches the bundle, and verify the signature with the creator's pubkey. Confirm no network calls happen.

Commit `feat(marketplace): publish_strategy signs listing locally with Ed25519`.

---

### Task 9: `browse_listings` and `get_listing`

**File:** `crates/xianvec-marketplace/src/browse.rs`

Thin wrappers over `ListingStore::list_all` and `ListingStore::get_by_id`. Sort `browse_listings` by `published_at DESC`.

Tests: insert 3 listings, browse returns them in reverse-chronological order. Commit `feat(marketplace): browse + get listing helpers`.

---

### Task 10: `install_strategy` — verify hash + signature

**File:** `crates/xianvec-marketplace/src/install.rs`

Given a Listing + the raw bundle JSON bytes (fetched from local file, gist URL, etc.):
1. Verify listing signature with `creator_pubkey_hex`
2. Verify bundle content hash matches `bundle_content_hash`
3. Validate bundle (`validate_bundle`)
4. Reject `License::Sealed` (Plan 4); warn on `License::PaidOpen`
5. Save bundle locally

```rust
use std::path::PathBuf;

use anyhow::Context;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use xianvec_engine::bundle::store::{BundleStore, FilesystemStore};
use xianvec_engine::bundle::validate::validate_bundle;
use xianvec_engine::bundle::StrategyBundle;

use crate::content_hash::content_hash;
use crate::publish::canonical_listing_bytes;
use crate::{License, Listing};

pub async fn install_strategy(
    listing: &Listing,
    bundle_json: &[u8],
    strategies_dir: PathBuf,
) -> anyhow::Result<StrategyBundle> {
    verify_listing_signature(listing)?;

    let bundle: StrategyBundle = serde_json::from_slice(bundle_json)
        .with_context(|| "parsing bundle JSON")?;
    let hash = content_hash(&bundle)?;
    if hash != listing.bundle_content_hash {
        anyhow::bail!(
            "content hash mismatch: listing {}, bundle {}",
            listing.bundle_content_hash, hash
        );
    }
    validate_bundle(&bundle)?;
    match &listing.license {
        License::Open => {}
        License::PaidOpen { .. } => {
            tracing::warn!("PaidOpen listing — payment rails land in Plan 5 (commerce contracts)");
        }
        License::Sealed => anyhow::bail!("Sealed licenses require Plan 4 (xvn API server)"),
    }
    let store = FilesystemStore::new(strategies_dir);
    store.save(&bundle).await?;
    Ok(bundle)
}

fn verify_listing_signature(listing: &Listing) -> anyhow::Result<()> {
    let pubkey_bytes = hex::decode(&listing.creator_pubkey_hex)?;
    let pubkey = VerifyingKey::from_bytes(pubkey_bytes.as_slice().try_into()?)?;
    let sig_bytes = hex::decode(&listing.signature_hex)?;
    let signature = Signature::from_bytes(&sig_bytes.try_into()
        .map_err(|_| anyhow::anyhow!("bad sig length"))?);
    let bytes = canonical_listing_bytes(listing)?;
    pubkey.verify(&bytes, &signature)?;
    Ok(())
}
```

Tests: install with valid signature + matching hash succeeds. Tampered bundle (any field) fails verification. Tampered signature fails verification.

Commit `feat(marketplace): install_strategy with hash + Ed25519 signature verification`.

---

### Task 11: `attest_run` — sign reputation receipt to local SQLite

**File:** `crates/xianvec-marketplace/src/receipt.rs`

```rust
use chrono::Utc;
use ed25519_dalek::Signer;
use serde::{Deserialize, Serialize};
use ulid::Ulid;

use crate::content_hash::canonicalize_value;
use crate::identity::AuthorIdentity;
use crate::store::ReputationStore;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationReceipt {
    pub id: String,
    pub listing_id: String,
    pub bundle_content_hash: String,
    pub scenario_id: Option<String>,
    pub sharpe: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub win_rate: Option<f64>,
    pub n_trades: Option<u32>,
    pub regime_tag: Option<String>,
    pub asset_tag: Option<String>,
    pub operator_pubkey_hex: String,
    pub signed_at: chrono::DateTime<chrono::Utc>,
    pub signature_hex: String,
    /// Populated only by Plan 5's push-to-chain command.
    pub on_chain_tx_hash: Option<String>,
}

pub struct AttestRunArgs<'a> {
    pub listing_id: &'a str,
    pub bundle_content_hash: &'a str,
    pub scenario_id: Option<&'a str>,
    pub sharpe: Option<f64>,
    pub max_drawdown_pct: Option<f64>,
    pub win_rate: Option<f64>,
    pub n_trades: Option<u32>,
    pub regime_tag: Option<&'a str>,
    pub asset_tag: Option<&'a str>,
}

pub async fn attest_run(
    args: AttestRunArgs<'_>,
    identity: &AuthorIdentity,
    store: &dyn ReputationStore,
) -> anyhow::Result<ReputationReceipt> {
    let mut receipt = ReputationReceipt {
        id: Ulid::new().to_string(),
        listing_id: args.listing_id.to_string(),
        bundle_content_hash: args.bundle_content_hash.to_string(),
        scenario_id: args.scenario_id.map(String::from),
        sharpe: args.sharpe,
        max_drawdown_pct: args.max_drawdown_pct,
        win_rate: args.win_rate,
        n_trades: args.n_trades,
        regime_tag: args.regime_tag.map(String::from),
        asset_tag: args.asset_tag.map(String::from),
        operator_pubkey_hex: identity.pubkey_hex(),
        signed_at: Utc::now(),
        signature_hex: String::new(),
        on_chain_tx_hash: None,
    };
    let bytes = canonical_receipt_bytes(&receipt)?;
    let signature = identity.signing_key.sign(&bytes);
    receipt.signature_hex = hex::encode(signature.to_bytes());
    store.insert_receipt(&receipt).await?;
    Ok(receipt)
}

fn canonical_receipt_bytes(r: &ReputationReceipt) -> anyhow::Result<Vec<u8>> {
    let value = serde_json::json!({
        "id": r.id,
        "listing_id": r.listing_id,
        "bundle_content_hash": r.bundle_content_hash,
        "scenario_id": r.scenario_id,
        "sharpe": r.sharpe,
        "max_drawdown_pct": r.max_drawdown_pct,
        "win_rate": r.win_rate,
        "n_trades": r.n_trades,
        "regime_tag": r.regime_tag,
        "asset_tag": r.asset_tag,
        "operator_pubkey_hex": r.operator_pubkey_hex,
        "signed_at": r.signed_at,
    });
    Ok(serde_json::to_vec(&canonicalize_value(&value))?)
}
```

Tests: attest a fake run, verify signature against operator pubkey, assert receipt persists. Plan 5 will read these signed records and push to ReputationRegistry on-chain.

Commit `feat(marketplace): attest_run signs receipt locally to SQLite`.

---

## Phase 2B.C — MCP verbs (skill + marketplace)

### Tasks 12-14: Skill MCP verbs

Pattern matches Plan 2a Task 3. Add file `crates/xianvec-engine/src/mcp/skill.rs`. Three verbs:

| Task | Verb | Args | Returns |
|---|---|---|---|
| 12 | `create_skill` | `{ markdown }` | `{ name, content_hash }` |
| 13 | `list_skills` | `{}` | `Vec<{ name, display_name, description, version }>` |
| 14 | `attach_skill_to_agent` | `{ strategy_id, slot, skill_name }` | `{ strategy_id, slot, skill_name }` |

### Tasks 15-19: Marketplace MCP verbs

File: `crates/xianvec-engine/src/mcp/marketplace.rs`. Five verbs:

| Task | Verb | Args | Returns |
|---|---|---|---|
| 15 | `publish_strategy` | `{ strategy_id, license: { kind, ... } }` | `Listing` |
| 16 | `browse_listings` | `{ filter: Option<{ creator_pubkey_hex, bundle_content_hash }> }` | `Vec<Listing>` |
| 17 | `get_listing` | `{ id }` | `Listing` |
| 18 | `install_strategy` | `{ listing_id, bundle_json: String }` | `{ strategy_id }` |
| 19 | `attest_run` | `{ listing_id, bundle_content_hash, sharpe?, ... }` | `ReputationReceipt` |

After Task 19, all 8 verbs advertised + dispatched. `tests/mcp_authoring.rs` extends to assert all 15 verbs (7 authoring + 3 skill + 5 marketplace). One commit per task using Plan 2a's template.

---

## Phase 2B.D — CLI surface

### Task 20: `xvn skill {new | ls | attach}`

Module: `crates/xianvec-cli/src/commands/skill.rs`. Mirrors Plan #1 Task 17/18's pattern. Subcommands:
- `new --from-file <path>` — parse markdown, save, print name
- `ls` — list saved skills
- `attach <strategy_id> --slot <regime|intern|trader> --skill <name>` — calls attach_skill_to_agent, re-saves bundle

Add `Skill(commands::skill::SkillCmd)` to top-level `Command` enum.

Integration test: full round-trip in tempdir. Commit `feat(cli): xvn skill new/ls/attach`.

### Task 21: `xvn marketplace {publish | browse | get | install | attest-run}`

Module: `crates/xianvec-cli/src/commands/marketplace.rs`. Subcommands:
- `publish <strategy_id> --license open` — runs publish_strategy, prints Listing JSON
- `browse [--creator <pubkey>] [--hash <content_hash>]` — list local listings
- `get <listing_id>` — show one listing
- `install <listing_id> --bundle-file <path>` — install bundle from local file (verify hash + signature)
- `attest-run <listing_id> <bundle_hash> --sharpe <f64> [--scenario <id>] [--regime <tag>] [--asset <tag>]` — sign reputation receipt

Add `Marketplace(commands::marketplace::MarketplaceCmd)` to `Command` enum.

Integration test: full publish → browse → install → attest round-trip in a tempdir. **Assert no network calls happen** (test should succeed on an air-gapped machine — no `IdentityClient`, no Mantle RPC, no IPFS gateway).

Commit `feat(cli): xvn marketplace publish/browse/get/install/attest-run (off-chain)`.

---

## Phase 2B.E — Polish + smoke

### Task 22: README + manual + smoke

- Update `crates/xianvec-engine/README.md`'s "What ships" with the new MCP verbs.
- Update `MANUAL.md` with the marketplace + skill flow, **explicitly noting "off-chain only — Plan 5 adds on-chain push when eval + strategy engines are battle-tested"**.
- Add `crates/xianvec-skills/README.md` and `crates/xianvec-marketplace/README.md`.
- End-to-end smoke (no network):

```bash
export XVN_HOME=/tmp/xvn-2b-smoke
rm -rf $XVN_HOME

ID=$(xvn strategy new --template trend_follower --name btc-trend-demo)

LISTING_JSON=$(xvn marketplace publish $ID --license open)
LISTING_ID=$(echo $LISTING_JSON | jq -r .id)
HASH=$(echo $LISTING_JSON | jq -r .bundle_content_hash)
echo "Published listing: $LISTING_ID"

xvn marketplace browse | head

# Install elsewhere — different XVN_HOME = different "buyer"
SELLER_HOME=$XVN_HOME
export XVN_HOME=/tmp/xvn-2b-buyer
rm -rf $XVN_HOME
xvn marketplace install $LISTING_ID --bundle-file $SELLER_HOME/strategies/$ID.json

INSTALLED=$(xvn strategy ls | head -1)
xvn strategy run $INSTALLED --fixture test-fixture-btc-2024-01 --decisions 2 --mock

xvn marketplace attest-run $LISTING_ID $HASH --sharpe 1.62 --regime trending_bull --asset BTC
```

Each step exits 0. **Confirm with `tcpdump`/`nettop` that no outbound packets are emitted during the smoke run.**

Commit `chore: Plan 2b end-to-end smoke verified (off-chain, no network)`.

### Task 23: Final workspace check

`cargo test --workspace` clean. clippy clean. fmt scoped to plan-touched crates. `xianvec-eval` still untouched. `xianvec-identity` still untouched (this plan deliberately doesn't depend on it). ~22 commits since Plan 2a's tip.

Commit `chore: Plan 2b final workspace check` if any cleanup landed.

---

## Self-review checklist

**Spec coverage:**
- [x] §6 Skill bundle format (OSShip-style markdown) — `xianvec-skills` crate
- [x] §10 MCP verb groups: skill management (3 verbs) + marketplace (5 verbs)
- [x] §13 Marketplace + reputation — entirely off-chain, Ed25519 signed
- [x] §5 Permission tiers: Tier A (open) — fully implemented
- [ ] §5 Tier B (sealed-hosted) — explicitly deferred to Plan 4 (post-hackathon)
- [ ] §13 On-chain marketplace surface — explicitly deferred to **Plan 5**, gated on eval + strategy engines being battle-tested. Smart contract spec already exists at `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md` (marked deferred).
- [ ] §11 Live execution — Plan 2c
- [ ] §12 Durable scheduler — Plan 2c
- [ ] §2 Wizard / dashboard — Plan 2d
- [ ] Eval engine — Plan 3

**Type consistency:** `Skill`, `SkillError`, `attach_skill_to_agent`, `Listing`, `License`, `AuthorIdentity`, `PublishConfig`, `ListingStore`, `ReputationStore`, `ReputationReceipt`, `AttestRunArgs`, `content_hash`, `canonicalize_value`, `publish_strategy`, `browse_listings`, `get_listing`, `install_strategy`, `attest_run` — consistent across all 23 tasks. **No `xianvec_identity::IdentityClient` usage anywhere.**

**No placeholders:** every code block is real Rust the engineer can paste. Tests are spelled out.

**Frequent commits:** 23 tasks → ~23 focused commits.

---

## What's next

Plan 2c — **Durable Scheduler + Live Execution**
Plan 2d — **Web Dashboard + Agent Wizard**
Plan 3 — **Eval Engine**
Plan 4 (post-hackathon) — **Tier B sealing + xvn API server**

**Plan 5 (post battle-testing) — 8004 ON-CHAIN INTEGRATION**
- **Source spec:** `docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md` (deferred status)
- **Gating:** eval + strategy engines battle-tested in production · marketplace volume signal confirmed · security review on contracts · on-chain costs justified by usage
- **Surface:** ListingRegistry, Marketplace, LicenseToken (ERC-1155 soulbound), EvalAttestationRegistry; xvn self-registers as ERC-8004 agent #0; x402 buy-rail via EIP-3009 buyWithAuthorization; CREATE2 deterministic deploys for multi-chain mirroring; UUPS proxies behind 7-day timelock + 2-of-3 multisig with progressive admin-burn ladder
- **New surfaces this plan adds:**
  - `xvn marketplace push-to-chain` — batches all locally-signed listings + receipts and publishes to Mantle. Reuses the existing canonical bytes that the local store already signed; on-chain integrity is verifiable from those signatures with no re-signing.
  - `xvn marketplace pull-from-chain` — discovers new listings from other xvn instances by indexing Mantle events.
  - License purchase flow with EIP-3009 (USDC buyWithAuthorization).
  - Reads back on-chain `agent_id` and `tx_hash` and populates this plan's `Listing.on_chain_agent_id` / `ReputationReceipt.on_chain_tx_hash` fields. **The forward-compat is already wired.**
- **Existing crate `xianvec-identity` (deferred per ADR 0008)** is the foundation for Plan 5's chain interaction. ADR 0008's `IdentityRegistry` + `ReputationRegistry` interfaces are the smaller subset of what the smart-contract-surface spec extends to (adding ListingRegistry, Marketplace, LicenseToken).
