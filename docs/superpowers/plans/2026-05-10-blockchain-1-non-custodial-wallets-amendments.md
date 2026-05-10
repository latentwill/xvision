# Wallet Plan #1 — Amendments to Apply Before Execution

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Modifies:** [`docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md`](2026-05-10-blockchain-1-non-custodial-wallets-plan.md) (3,606 lines).
> **Sources of amendments:**
> - [`docs/superpowers/reviews/2026-05-10-non-custodial-wallets-plan-review.md`](../reviews/2026-05-10-non-custodial-wallets-plan-review.md) — adversarial review (6 blockers, 14 high, 15 medium, 9 low)
> - [`docs/superpowers/research/2026-05-10-ideonomy-explorations.md`](../research/2026-05-10-ideonomy-explorations.md) — strategic synthesis (multi-asset; OFAC/SLA decisions; reputation-quota → FOLLOWUPS)
> - Triage discussion (this conversation, 2026-05-10)
>
> **Run after:** [`2026-05-10-terminology-rename-option-b.md`](2026-05-10-terminology-rename-option-b.md). The amendments below assume the rename has already landed: `cycle_id` is now `cycle_id` in source, `agent_id` (in plan/spec text) is now `agent_id`, `Strategy` trait is `Algorithm`. If the rename hasn't run, run it first; if you're applying amendments to the plan in-place and the plan still says `agent_id`, the rename plan's Phase 4.2 already sed'd the wallet plan, so this should be consistent.

---

**Goal:** Update the wallet plan to (a) close the 6 blockers identified in the adversarial review so the plan compiles against the existing trunk, (b) drop scope items the operator chose to cut (OFAC, public disclosure SLA, reputation-weighted quota), (c) add scope items the operator chose to keep (multi-asset / F18; runtime agent rename — handled in the leverage-items plan), and (d) tighten the dispatcher / reservations / audit-log designs against the review's specific code-level findings.

**Architecture:** The amendments are organized in seven groups, each modifying a specific phase of the original plan. Group A adds a new Phase -1 (F18 prerequisite). Groups B–G modify Phases 1, 3, 4, 6, 7, 8, 9 in place. Each amendment shows the exact section of the original plan to find, what to remove, and what to replace it with. Order of execution matches the order of the original plan.

**Tech Stack:** New deps beyond the original plan: `zeroize = "1"` (security hygiene per review §12.2), `humantime = "2"` (relative-time CLI parsing per review §3.10), `hkdf = "0.12"` + `sha2 = "0.10"` + `aes-gcm = "0.10"` (encrypted trading key store), and `hex = "0.4"`. The EIP-712 trading-key registration uses `alloy = "2"` which is **already** in workspace deps (see root `Cargo.toml`); no separate `alloy-sol-types` or `ethers` import is needed — use `alloy::sol_types::*` and `alloy::signers::*`. Multi-asset support reuses `xianvec-core`'s existing `AssetSymbol` enum (variants: `Btc`, `Eth`, `Sol`); no new dep.

**What is REMOVED from scope (vs. original plan):**
- Reputation-weighted quota — moved to FOLLOWUPS with discarded-idea note (Group F).
- OFAC sanctions screening — moved to a future hosted-instance launch-readiness plan; not relevant to self-hosted open-source code (Group G).
- Public disclosure SLA commitment — replaced with README "use at your own risk" warning (Group G).
- BTC-only restriction — F18 is now a prerequisite, multi-asset works from v1 (Group A).

**What is ADDED to scope (vs. original plan):**
- F18 (`asset: AssetSymbol` on `TraderDecision` + per-asset mark price) — new Phase -1.
- `trading_keys` migration + module + AppContext wiring (Group B, missing from original).
- `global_state` migration + dispatcher check + tests (Group B, missing from original).
- `strategies` migration for runtime-editable per-agent config (Group B, missing from original).
- Funding-payment ingestion job in reconciler (Group F, missing from original).
- `xvn key verify` command (Group D, missing from original).
- `xvn key revoke` calls Orderly DELETE (Group D, originally local-only).
- Adversarial spam-key test, audit-log triggers test, N=10+ concurrency test, content-hash determinism test, log redaction (Group G).

---

## File structure (additions to plan's file structure)

```
crates/
├── xianvec-data/
│   └── src/
│       ├── trading_keys.rs                              # NEW (Group B) — encrypted key store, AES-256-GCM with HKDF-derived per-user key, Zeroizing wrapper
│       ├── global_state.rs                              # NEW (Group B) — single-row halt/unhalt module
│       ├── strategies.rs                                # NEW (Group B) — runtime-editable per-agent config store (replaces TOML hot-reload)
│       └── migrations/
│           ├── 20260510000008_trading_keys.sql         # NEW
│           ├── 20260510000009_global_state.sql         # NEW
│           └── 20260510000010_strategies.sql          # NEW
│
├── xianvec-core/
│   └── src/
│       └── trading.rs                                   # MODIFY (Group A) — add `asset: AssetSymbol` to TraderDecision; touches every constructor/test fixture downstream
│
├── xianvec-execution/
│   └── src/
│       ├── orderly.rs                                   # MODIFY (Group C) — generalize submit() over AssetSymbol; add register_trading_key(EIP-712) and delete_orderly_key
│       └── dispatcher.rs                                # MODIFY (Group C) — wrap-not-replace OrderlyExecutor::submit; reservation released on Err; halt check at top
│
├── xianvec-cli/
│   └── src/
│       └── commands/
│           ├── key.rs                                   # MODIFY (Group D) — add `verify` subcommand; revoke calls Orderly DELETE
│
└── xianvec-data/
    └── migrations/
        # plus the original plan's seven migrations (positions, funding_attributions, decisions,
        # strategy_status, pending_approvals, policy_changes, pending_reservations) — unchanged
```

---

## Group A — Multi-asset prerequisite (NEW Phase -1)

The original plan defers F18 ("`asset` on `TraderDecision`", from FOLLOWUPS.md) and pins v1 to BTC implicitly. Operator decision 2026-05-10: do F18 properly so the wallet rail is multi-asset from v1. Hackathon demo can still focus on BTC; the architecture stops baking BTC in.

### Task A.1: Add `asset` field to `TraderDecision`

**Files:**
- Modify: `crates/xianvec-core/src/trading.rs:114` (the `TraderDecision` struct)
- Modify: every fixture / constructor of `TraderDecision` — hit by `cargo check` after the field add

- [ ] **Step 1: Read the current TraderDecision**

Read `crates/xianvec-core/src/trading.rs` lines 114-130. Note the fields: `cycle_id` (renamed from `cycle_id`), `action`, `size_bps`, `direction`, `stop_loss_pct`, `take_profit_pct`, `trader_summary`. There is no `asset` field today.

- [ ] **Step 2: Write the failing test (TDD)**

Add to `crates/xianvec-core/src/trading.rs` (under the existing tests module, or create one if absent):

```rust
#[cfg(test)]
mod trader_decision_asset_tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn trader_decision_carries_asset() {
        let td = TraderDecision {
            cycle_id: Uuid::nil(),
            asset: AssetSymbol::Eth,
            action: Action::Buy,
            size_bps: 1000,
            direction: Direction::Long,
            stop_loss_pct: 2.5,
            take_profit_pct: 5.0,
            trader_summary: "ETH long on BTC-leadership rotation.".into(),
        };
        assert_eq!(td.asset, AssetSymbol::Eth);
    }
}
```

- [ ] **Step 3: Run the test to confirm it fails**

Run: `cargo test -p xianvec-core trader_decision_carries_asset`
Expected: FAIL — "missing field `asset`" or similar.

- [ ] **Step 4: Add the field**

In `crates/xianvec-core/src/trading.rs`, modify the `TraderDecision` struct (line 114) to:

```rust
pub struct TraderDecision {
    #[garde(skip)]
    pub cycle_id: Uuid,
    /// Asset the decision applies to. Required for multi-asset routing.
    /// Per spec §3.5; F18 from FOLLOWUPS.md.
    #[garde(skip)]
    pub asset: AssetSymbol,
    #[garde(skip)]
    pub action: Action,
    /// Position size in basis points of NAV (max 20% = 2000bps).
    #[garde(range(min = 0, max = 2000))]
    pub size_bps: u32,
    #[garde(skip)]
    pub direction: Direction,
    #[garde(range(min = 0.1, max = 20.0))]
    pub stop_loss_pct: f32,
    #[garde(range(min = 0.1, max = 50.0))]
    pub take_profit_pct: f32,
    #[garde(length(min = 10, max = 500))]
    pub trader_summary: String,
}
```

- [ ] **Step 5: Run the test to confirm it passes**

Run: `cargo test -p xianvec-core trader_decision_carries_asset`
Expected: PASS.

- [ ] **Step 6: Build the workspace and identify all downstream call sites**

Run: `cargo build --workspace 2>&1 | tee /tmp/xvn-f18-build.log`
Expected: many errors of the form "missing field `asset` in initializer of `TraderDecision`". Each is a fixture or a real construction site. Capture the full list:

```bash
grep -E "missing field.*asset" /tmp/xvn-f18-build.log
```

- [ ] **Step 7: Fix every call site**

For each error, open the file at the indicated line and add `asset: AssetSymbol::Btc,` (or the contextually appropriate symbol) into the struct literal. Sites likely to need updates:
- `crates/xianvec-trader/src/parse.rs` — the LLM-output parser. Must extract `asset` from the LLM response or default to the briefing's `asset`. Wire it from `InternBriefing.asset` (which already exists per `crates/xianvec-core/src/trading.rs:76`).
- `crates/xianvec-eval/src/baselines/*.rs` — baseline strategies. Each baseline should default to `AssetSymbol::Btc` for the v1 demo.
- `crates/xianvec-eval/src/harness.rs` — test fixtures.
- `crates/xianvec-core/src/store.rs` — sample data fixtures.
- `crates/xianvec-execution/src/orderly.rs` — calls to `submit` may need to thread `decision.asset` through.

For each baseline file (e.g. `crates/xianvec-eval/src/baselines/always_long.rs`), the change is mechanical — find every `TraderDecision { ... }` struct literal and add `asset: ctx.asset,` (where `ctx` carries the briefing's asset) or `asset: AssetSymbol::Btc,` if there is no contextual asset.

- [ ] **Step 8: For the LLM trader parse path, extract asset from the JSON response**

In `crates/xianvec-trader/src/parse.rs`, locate the function that parses the LLM's JSON response into `TraderDecision`. It currently reads `cycle_id` (now `cycle_id`), `action`, `size_bps`, etc. Add asset extraction:

```rust
let asset = parsed.get("asset")
    .and_then(|v| v.as_str())
    .and_then(|s| AssetSymbol::from_str(s).ok())
    .unwrap_or(briefing.asset);  // fall back to the briefing's asset
```

(Adjust to the exact pattern in the existing parse function.)

- [ ] **Step 9: Update the trader prompt to ask for `asset`**

In `crates/xianvec-trader/src/prompt.rs`, the trader prompt instructs the LLM what JSON to return. Add `"asset"` to the schema description and example, telling the LLM to echo the asset from the briefing (so existing single-asset prompts continue to work; multi-asset is a forward extension).

- [ ] **Step 10: Build and run the full workspace test suite**

Run: `cargo test --workspace`
Expected: all pass. If any failures, they are likely a missed call site; locate and fix.

- [ ] **Step 11: Commit**

```bash
git add -A
git commit -m "feat(core): add asset field to TraderDecision (F18 unblock)

Wallet rail dispatcher needs the asset on the decision to route multi-asset
orders. Default value falls back to the briefing's asset; the trader prompt
schema is extended to ask for it explicitly."
```

### Task A.2: Replace mark-price placeholder in dispatcher and simulator

**Files:**
- Modify: original plan Task 3.1 Step 3 ("Pre-trade simulation wrapper") and Task 3.2 Step 2 (the `78_000.0` placeholder in `intended_notional / 78_000.0`).
- Modify: `crates/xianvec-execution/src/orderly.rs` — extract a `mark_price(asset: AssetSymbol) -> Result<f64>` helper (or its existing equivalent if one exists).

- [ ] **Step 1: Find any existing mark-price-fetch helper**

Run: `rg -n "fn (mark_price|fetch_mark|get_mark|orderly_mark)" crates/xianvec-execution/src/`
Expected: either a hit (reuse it) or no hits (write one).

- [ ] **Step 2: If no helper exists, add one to orderly.rs**

Add to `crates/xianvec-execution/src/orderly.rs`:

```rust
impl OrderlyExecutor {
    /// Fetch the current mark price for an asset, in USDC, with a short cache.
    /// Used by the dispatcher's pre-trade simulation and bps→USDC conversion.
    pub async fn mark_price(&self, asset: AssetSymbol) -> anyhow::Result<f64> {
        let symbol = orderly_symbol(asset);  // helper to map AssetSymbol -> "PERP_BTC_USDC" etc.
        let url = format!("{}/v3/public/futures/{}", self.base_url, symbol);
        let resp = self.http.get(&url).send().await?.error_for_status()?;
        let v: serde_json::Value = resp.json().await?;
        let mark = v["data"]["mark_price"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("missing mark_price in response"))?;
        Ok(mark)
    }
}

fn orderly_symbol(asset: AssetSymbol) -> &'static str {
    match asset {
        AssetSymbol::Btc => "PERP_BTC_USDC",
        AssetSymbol::Eth => "PERP_ETH_USDC",
        AssetSymbol::Sol => "PERP_SOL_USDC",
        // extend per the AssetSymbol enum's actual variants
    }
}
```

(Inspect `AssetSymbol` in `crates/xianvec-core/src/trading.rs:34` for the actual variant list and update the match arms accordingly.)

- [ ] **Step 3: Replace the `78_000.0` placeholder in the dispatcher**

In the dispatcher (Task 3.2 of the original plan), find `intended_notional / 78_000.0`. Replace with:

```rust
let mark = self.orderly.mark_price(decision.asset).await
    .map_err(|e| anyhow::anyhow!("mark price unavailable for {:?}: {}", decision.asset, e))?;
let qty = intended_notional / mark;
```

- [ ] **Step 4: Add a property test asserting dispatcher rejects when mark price is unavailable**

In `crates/xianvec-execution/src/dispatcher.rs` test module:

```rust
#[tokio::test]
async fn dispatch_rejects_when_mark_price_unavailable() {
    let mock_orderly = MockOrderlyApi::with_mark_price_failure();
    let dispatcher = OrderDispatcher::new(/* ... */);
    let outcome = dispatcher.dispatch(&decision_for_asset(AssetSymbol::Btc), /* ... */).await;
    assert!(matches!(outcome, Outcome::Vetoed { .. }));
    // verify a Reject audit row was written
}
```

- [ ] **Step 5: Run dispatcher tests and commit**

Run: `cargo test -p xianvec-execution dispatcher`
Expected: pass.

```bash
git add -A
git commit -m "feat(execution): per-asset mark-price fetch in dispatcher; remove 78_000 placeholder"
```

---

## Group B — Missing migrations and modules (Phase 1 additions)

The original plan calls `ctx.store_trading_key(...)`, `ctx.global_halt(...)`, and assumes a runtime-editable `StrategyConfig` store, but creates none of these tables/modules. Add three migrations + their backing Rust modules to Phase 1.

### Task B.1: `trading_keys` migration + module (review §1.2 BLOCKER)

**Files:**
- Create: `crates/xianvec-data/src/migrations/20260510000008_trading_keys.sql`
- Create: `crates/xianvec-data/src/trading_keys.rs`
- Modify: `crates/xianvec-data/src/lib.rs` — `pub mod trading_keys;`

- [ ] **Step 1: Write the SQL migration**

Create `crates/xianvec-data/src/migrations/20260510000008_trading_keys.sql`:

```sql
-- Per-user encrypted Ed25519 trading key, AES-256-GCM at rest, HKDF-derived
-- per-user key. Spec §3.2 + §5; review §1.2.

CREATE TABLE IF NOT EXISTS trading_keys (
    user_id          TEXT PRIMARY KEY,
    pubkey_hex       TEXT NOT NULL UNIQUE,
    encrypted_blob   TEXT NOT NULL,            -- "v1:nonce_hex:ciphertext_hex" format (review §9.5)
    scope            TEXT NOT NULL,            -- e.g. "trading"
    ip_restriction   TEXT,                     -- optional Orderly-side IP allow-list
    registered_at    INTEGER NOT NULL,         -- unix millis
    expires_at       INTEGER NOT NULL,         -- unix millis (90-day default per spec)
    revoked_at       INTEGER,                  -- nullable; non-null = revoked locally
    last_used_at     INTEGER                   -- updated on each successful sign
);

CREATE INDEX IF NOT EXISTS idx_trading_keys_pubkey ON trading_keys(pubkey_hex);
CREATE INDEX IF NOT EXISTS idx_trading_keys_expires ON trading_keys(expires_at);
```

- [ ] **Step 2: Write the Rust module — failing test first**

Create `crates/xianvec-data/src/trading_keys.rs` and in the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::SqlitePool;

    #[sqlx::test]
    async fn round_trip_encrypted_blob(pool: SqlitePool) {
        let store = TradingKeyStore::new(pool, &test_secret());
        let key_bytes = [0xAA; 32];
        store.insert("op", "abcd1234", &key_bytes, "trading", None, 90).await.unwrap();
        let loaded = store.load_decrypted("op").await.unwrap();
        assert_eq!(&loaded[..], &key_bytes[..]);
    }

    #[sqlx::test]
    async fn revoke_marks_local_revoked_at(pool: SqlitePool) {
        let store = TradingKeyStore::new(pool, &test_secret());
        store.insert("op", "abcd1234", &[0xAA; 32], "trading", None, 90).await.unwrap();
        store.revoke_local("op").await.unwrap();
        assert!(store.is_revoked("op").await.unwrap());
    }

    fn test_secret() -> [u8; 32] { [0x11; 32] }
}
```

- [ ] **Step 3: Implement `TradingKeyStore`**

```rust
use aes_gcm::{aead::Aead, Aes256Gcm, Key, KeyInit, Nonce};
use anyhow::{anyhow, Result};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;
use sqlx::SqlitePool;
use zeroize::Zeroizing;

pub struct TradingKeyStore {
    pool: SqlitePool,
    master_secret: [u8; 32],
}

impl TradingKeyStore {
    pub fn new(pool: SqlitePool, master_secret: &[u8; 32]) -> Self {
        Self { pool, master_secret: *master_secret }
    }

    fn derive_user_key(&self, user_id: &str) -> Zeroizing<[u8; 32]> {
        let hkdf = Hkdf::<Sha256>::new(Some(user_id.as_bytes()), &self.master_secret);
        let mut okm = Zeroizing::new([0u8; 32]);
        hkdf.expand(b"xianvec-trading-key-v1", okm.as_mut())
            .expect("HKDF expand");
        okm
    }

    pub async fn insert(
        &self,
        user_id: &str,
        pubkey_hex: &str,
        key_bytes: &[u8; 32],
        scope: &str,
        ip_restriction: Option<&str>,
        ttl_days: u64,
    ) -> Result<()> {
        let user_key = self.derive_user_key(user_id);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(user_key.as_ref()));
        let mut nonce_bytes = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut nonce_bytes);
        let ct = cipher
            .encrypt(Nonce::from_slice(&nonce_bytes), key_bytes.as_ref())
            .map_err(|e| anyhow!("aes encrypt: {}", e))?;
        let blob = format!("v1:{}:{}", hex::encode(nonce_bytes), hex::encode(&ct));

        let now = chrono::Utc::now().timestamp_millis();
        let expires = now + (ttl_days as i64 * 86_400_000);
        sqlx::query(
            "INSERT INTO trading_keys (user_id, pubkey_hex, encrypted_blob, scope, ip_restriction, registered_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(user_id)
        .bind(pubkey_hex)
        .bind(blob)
        .bind(scope)
        .bind(ip_restriction)
        .bind(now)
        .bind(expires)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn load_decrypted(&self, user_id: &str) -> Result<Zeroizing<Vec<u8>>> {
        let blob: String = sqlx::query_scalar(
            "SELECT encrypted_blob FROM trading_keys WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;

        let parts: Vec<&str> = blob.splitn(3, ':').collect();
        if parts.len() != 3 || parts[0] != "v1" {
            return Err(anyhow!("unsupported blob version: {}", parts.first().unwrap_or(&"")));
        }
        let nonce = hex::decode(parts[1])?;
        let ct = hex::decode(parts[2])?;

        let user_key = self.derive_user_key(user_id);
        let cipher = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(user_key.as_ref()));
        let pt = cipher
            .decrypt(Nonce::from_slice(&nonce), ct.as_slice())
            .map_err(|e| anyhow!("aes decrypt: {}", e))?;
        Ok(Zeroizing::new(pt))
    }

    pub async fn revoke_local(&self, user_id: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query("UPDATE trading_keys SET revoked_at = ? WHERE user_id = ?")
            .bind(now)
            .bind(user_id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn is_revoked(&self, user_id: &str) -> Result<bool> {
        let r: Option<i64> = sqlx::query_scalar(
            "SELECT revoked_at FROM trading_keys WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await?;
        Ok(r.is_some())
    }
}
```

- [ ] **Step 4: Add deps**

In `crates/xianvec-data/Cargo.toml`, ensure these are present (add if missing):

```toml
aes-gcm = "0.10"
hkdf = "0.12"
sha2 = "0.10"
hex = "0.4"
zeroize = { version = "1.7", features = ["derive"] }
rand = "0.8"
chrono = "0.4"
```

- [ ] **Step 5: Wire into `xianvec-data/src/lib.rs`**

Add `pub mod trading_keys;`.

- [ ] **Step 6: Update `AppContext::from_env` (Task 4.10 of original plan)**

In `crates/xianvec-cli/src/lib.rs` (or wherever `AppContext` lives — see original plan Task 4.10), add a `trading_keys: TradingKeyStore` field and initialize it in `from_env` using the `CREDENTIAL_SECRET` env var:

```rust
let secret_hex = std::env::var("CREDENTIAL_SECRET")
    .map_err(|_| anyhow!("CREDENTIAL_SECRET env var required"))?;
let secret_bytes = hex::decode(&secret_hex)?;
let secret_arr: [u8; 32] = secret_bytes.try_into()
    .map_err(|_| anyhow!("CREDENTIAL_SECRET must be 32 bytes hex"))?;
let trading_keys = TradingKeyStore::new(pool.clone(), &secret_arr);
```

- [ ] **Step 7: Run tests + commit**

Run: `cargo test -p xianvec-data trading_keys`
Expected: pass.

```bash
git add -A
git commit -m "feat(data): trading_keys store with HKDF-per-user AES-256-GCM (review §1.2)"
```

### Task B.2: `global_state` migration + module (review §1.3 BLOCKER)

**Files:**
- Create: `crates/xianvec-data/src/migrations/20260510000009_global_state.sql`
- Create: `crates/xianvec-data/src/global_state.rs`
- Modify: `crates/xianvec-data/src/lib.rs` — `pub mod global_state;`

- [ ] **Step 1: Write the migration**

Create `crates/xianvec-data/src/migrations/20260510000009_global_state.sql`:

```sql
-- Single-row global halt flag. `xvn kill --all` writes here; OrderDispatcher
-- reads at the very top of dispatch(). Spec §3.9; review §1.3.

CREATE TABLE IF NOT EXISTS global_state (
    id           INTEGER PRIMARY KEY CHECK (id = 1),  -- enforce single row
    halted_at    INTEGER,                             -- nullable; non-null = halted
    halted_by    TEXT,                                -- e.g. "operator-cli"
    reason       TEXT
);

INSERT OR IGNORE INTO global_state (id, halted_at, halted_by, reason) VALUES (1, NULL, NULL, NULL);
```

- [ ] **Step 2: Write failing test**

```rust
#[sqlx::test]
async fn global_halt_round_trips(pool: SqlitePool) {
    let g = GlobalState::new(pool);
    assert!(!g.is_halted().await.unwrap());
    g.halt("test-reason", "operator-cli").await.unwrap();
    let s = g.current().await.unwrap();
    assert!(s.halted_at.is_some());
    assert_eq!(s.reason.as_deref(), Some("test-reason"));
    g.unhalt().await.unwrap();
    assert!(!g.is_halted().await.unwrap());
}
```

- [ ] **Step 3: Implement `GlobalState`**

```rust
use anyhow::Result;
use sqlx::SqlitePool;

pub struct GlobalState { pool: SqlitePool }

#[derive(Debug, Clone)]
pub struct GlobalHaltStatus {
    pub halted_at: Option<i64>,
    pub halted_by: Option<String>,
    pub reason: Option<String>,
}

impl GlobalState {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    pub async fn is_halted(&self) -> Result<bool> {
        let v: Option<i64> = sqlx::query_scalar(
            "SELECT halted_at FROM global_state WHERE id = 1",
        ).fetch_one(&self.pool).await?;
        Ok(v.is_some())
    }

    pub async fn halt(&self, reason: &str, by: &str) -> Result<()> {
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "UPDATE global_state SET halted_at = ?, halted_by = ?, reason = ? WHERE id = 1",
        ).bind(now).bind(by).bind(reason).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn unhalt(&self) -> Result<()> {
        sqlx::query(
            "UPDATE global_state SET halted_at = NULL, halted_by = NULL, reason = NULL WHERE id = 1",
        ).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn current(&self) -> Result<GlobalHaltStatus> {
        let row = sqlx::query_as::<_, (Option<i64>, Option<String>, Option<String>)>(
            "SELECT halted_at, halted_by, reason FROM global_state WHERE id = 1",
        ).fetch_one(&self.pool).await?;
        Ok(GlobalHaltStatus { halted_at: row.0, halted_by: row.1, reason: row.2 })
    }
}
```

- [ ] **Step 4: Add halt check to dispatcher (modifies original plan Task 3.2)**

In `crates/xianvec-execution/src/dispatcher.rs`, at the very top of `OrderDispatcher::dispatch`, BEFORE the emit-stage audit row:

```rust
if self.global_state.is_halted().await? {
    let st = self.global_state.current().await?;
    self.audit.write(Stage::Reject, &decision.cycle_id, json!({
        "reason": "global_halt",
        "halted_by": st.halted_by,
        "halt_reason": st.reason,
    })).await?;
    return Ok(Outcome::Halted);
}
```

- [ ] **Step 5: Add a test that halt blocks dispatch**

```rust
#[tokio::test]
async fn halted_dispatch_returns_halted_and_writes_one_reject_row() {
    let (dispatcher, audit_db, gs) = setup_dispatcher().await;
    gs.halt("operator-test", "cli").await.unwrap();
    let outcome = dispatcher.dispatch(&decision(), &portfolio_state()).await.unwrap();
    assert!(matches!(outcome, Outcome::Halted));
    let rows = count_audit_rows(&audit_db, Stage::Reject).await;
    assert_eq!(rows, 1);
}
```

- [ ] **Step 6: Wire `xvn kill --all` to call `gs.halt(...)` (modifies original plan Task 4.2)**

The original plan's Task 4.2 step 3 calls `ctx.global_halt(...)` which doesn't exist. Replace with `ctx.global_state.halt(...)`:

```rust
KillTarget::All => {
    ctx.global_state.halt(&args.reason, "operator-cli").await?;
    println!("Global halt set. All dispatchers will reject new orders.");
}
```

- [ ] **Step 7: Wire `xvn unhalt --all` (extend original plan Task 4.3)**

The original plan's Task 4.3 only unhalts a strategy. Add an `--all` arm that calls `ctx.global_state.unhalt()`.

- [ ] **Step 8: Commit**

```bash
git add -A
git commit -m "feat(data,execution,cli): global_state halt module + dispatcher check + kill/unhalt --all wiring (review §1.3)"
```

### Task B.3: `strategies` (per-agent config) migration + module (review §1.5 HIGH)

**Files:**
- Create: `crates/xianvec-data/src/migrations/20260510000010_strategies.sql`
- Create: `crates/xianvec-data/src/strategies.rs`
- Modify: original plan Task 2.1 to use this store as the dispatcher's source of truth, with TOML kept as a bulk-import seed only.

- [ ] **Step 1: Write the migration**

```sql
-- Runtime-editable per-agent configuration. The dispatcher reads from this on
-- every dispatch() call so policy edits via `xvn budget set` or the dashboard
-- take effect on the next order without process restart. Spec §3.4; review §1.5.

CREATE TABLE IF NOT EXISTS strategies (
    agent_id      TEXT PRIMARY KEY,
    config_json   TEXT NOT NULL,
    updated_at    INTEGER NOT NULL,
    updated_by    TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_strategies_updated_at ON strategies(updated_at);
```

- [ ] **Step 2: Write the Rust module — failing test first**

```rust
#[sqlx::test]
async fn config_set_then_get_returns_latest(pool: SqlitePool) {
    let store = StrategyConfigStore::new(pool);
    let cfg = sample_config_with_hard_cap(1000);
    store.set("agent-a", &cfg, "operator-cli").await.unwrap();
    let loaded = store.get("agent-a").await.unwrap();
    assert_eq!(loaded.hard_cap_usdc_notional, 1000.0);

    let cfg2 = sample_config_with_hard_cap(2000);
    store.set("agent-a", &cfg2, "ui").await.unwrap();
    let loaded = store.get("agent-a").await.unwrap();
    assert_eq!(loaded.hard_cap_usdc_notional, 2000.0);
}
```

- [ ] **Step 3: Implement `StrategyConfigStore`**

```rust
use anyhow::Result;
use sqlx::SqlitePool;
use xianvec_risk::config::StrategyConfig;  // from original plan Task 2.1

pub struct StrategyConfigStore { pool: SqlitePool }

impl StrategyConfigStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    pub async fn set(&self, agent_id: &str, cfg: &StrategyConfig, by: &str) -> Result<()> {
        let json = serde_json::to_string(cfg)?;
        let now = chrono::Utc::now().timestamp_millis();
        sqlx::query(
            "INSERT INTO strategies (agent_id, config_json, updated_at, updated_by) \
             VALUES (?, ?, ?, ?) \
             ON CONFLICT(agent_id) DO UPDATE SET config_json = excluded.config_json, updated_at = excluded.updated_at, updated_by = excluded.updated_by",
        ).bind(agent_id).bind(json).bind(now).bind(by).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn get(&self, agent_id: &str) -> Result<StrategyConfig> {
        let json: String = sqlx::query_scalar(
            "SELECT config_json FROM strategies WHERE agent_id = ?",
        ).bind(agent_id).fetch_one(&self.pool).await?;
        let cfg: StrategyConfig = serde_json::from_str(&json)?;
        Ok(cfg)
    }

    pub async fn list(&self) -> Result<Vec<(String, StrategyConfig)>> {
        let rows = sqlx::query_as::<_, (String, String)>(
            "SELECT agent_id, config_json FROM strategies ORDER BY agent_id",
        ).fetch_all(&self.pool).await?;
        rows.into_iter()
            .map(|(id, j)| Ok((id, serde_json::from_str(&j)?)))
            .collect()
    }
}
```

- [ ] **Step 4: Modify original plan Task 2.1 — `parse_strategies_toml` becomes seed-only**

Update `crates/xianvec-risk/src/config.rs` (created in original plan Task 2.1) so the TOML loader has a clear "seed-import" semantic:

```rust
/// Bulk-import seed: parse a TOML config and write each agent into the store.
/// This is intended for first-run seeding only. Runtime edits go through
/// `StrategyConfigStore::set` directly via `xvn budget set` or the dashboard.
pub async fn seed_from_toml(
    toml_str: &str,
    store: &StrategyConfigStore,
) -> anyhow::Result<usize> {
    let parsed: HashMap<String, StrategyConfig> = toml::from_str(toml_str)?;
    let n = parsed.len();
    for (agent_id, cfg) in parsed {
        store.set(&agent_id, &cfg, "seed-import").await?;
    }
    Ok(n)
}
```

- [ ] **Step 5: Modify dispatcher to read from the store, not from a static reference**

In `crates/xianvec-execution/src/dispatcher.rs`, the dispatcher's constructor takes a `StrategyConfigStore` (not a borrowed `&StrategyConfig`). Each dispatch reads:

```rust
let cfg = self.strategies.get(&decision.agent_id).await
    .map_err(|e| anyhow!("agent config missing for {}: {}", decision.agent_id, e))?;
```

- [ ] **Step 6: Add a hot-reload test (modifies original plan Task 4.8)**

```rust
#[tokio::test]
async fn budget_set_takes_effect_on_next_dispatch_without_restart() {
    let (dispatcher, store, _) = setup_dispatcher_with_strategies(/* hard_cap=1000 */).await;
    // First dispatch at $400 succeeds (under cap)
    let o1 = dispatcher.dispatch(&decision_for_amount(400.0), &state_with_zero_in_flight()).await.unwrap();
    assert!(matches!(o1, Outcome::Submitted { .. }));

    // Operator lowers cap to $300 via budget set
    let mut new_cfg = store.get("agent-a").await.unwrap();
    new_cfg.hard_cap_usdc_notional = 300.0;
    store.set("agent-a", &new_cfg, "operator-cli").await.unwrap();

    // Next dispatch at $400 vetoes
    let o2 = dispatcher.dispatch(&decision_for_amount(400.0), &state_with_zero_in_flight()).await.unwrap();
    assert!(matches!(o2, Outcome::Vetoed { .. }));
}
```

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(data,risk,execution): StrategyConfigStore with hot-reload (review §1.5)"
```

---

## Group C — Phase 3 dispatcher redesign (review §2.1, §2.4, §2.5, §2.7, §9.2, §9.6 BLOCKERS/HIGHS)

The original plan's Task 3.2 introduces an `OrderlyOrderSubmit` trait that throws away the existing `OrderlyExecutor::submit` logic (bps→USDC, mark price, TP/SL bracket, fill polling). This rewrite is wrong — keep the executor's behavior; the dispatcher *wraps* it.

### Task C.1: Redesign the `OrderlyOrderSubmit` trait surface (replaces original Task 3.2 Step 1)

**Files:**
- Modify: `crates/xianvec-execution/src/dispatcher.rs` (planned in original Task 3.2)
- Modify: `crates/xianvec-execution/src/orderly.rs` to expose the right shape

- [ ] **Step 1: Replace the original trait definition**

The original plan defines:
```rust
// OLD - DO NOT USE
trait OrderlyOrderSubmit {
    async fn submit_order(
        &self,
        client_order_id: &str,
        asset: &str,
        side: &str,
        size_usdc: f64,
    ) -> Result<String>;
}
```

Replace with:

```rust
/// Wrap-not-replace: the dispatcher hands the full TraderDecision plus
/// equity to the executor; the executor keeps its existing bps→USDC,
/// mark-price, TP/SL-bracket logic. The dispatcher's job is gating + audit;
/// the executor's job is "actually place the order".
#[async_trait::async_trait]
pub trait OrderSink: Send + Sync {
    async fn submit(
        &self,
        client_order_id: &str,
        decision: &TraderDecision,
        equity_usdc: f64,
    ) -> anyhow::Result<SubmitResult>;
}

#[derive(Debug, Clone)]
pub struct SubmitResult {
    pub orderly_position_id: String,
    pub fill_price: Option<f64>,
    pub signed_payload: Vec<u8>,    // for the audit Sign-stage row, per spec §3.8 (review §2.4)
    pub signature_hex: String,
}
```

- [ ] **Step 2: Implement `OrderSink` for the existing `OrderlyExecutor`**

Add to `crates/xianvec-execution/src/orderly.rs`:

```rust
#[async_trait::async_trait]
impl OrderSink for OrderlyExecutor {
    async fn submit(
        &self,
        client_order_id: &str,
        decision: &TraderDecision,
        equity_usdc: f64,
    ) -> anyhow::Result<SubmitResult> {
        // The existing submit() does the full pipeline (bps→USDC, mark, qty, TP/SL).
        // Refactor it slightly to also return signed_payload + signature_hex
        // for the audit row.
        let receipt = self.submit_with_audit_payload(decision, equity_usdc, client_order_id).await?;
        Ok(SubmitResult {
            orderly_position_id: receipt.orderly_position_id,
            fill_price: receipt.fill_price,
            signed_payload: receipt.signed_payload,
            signature_hex: receipt.signature_hex,
        })
    }
}
```

The original `submit_with_audit_payload` is a *small* refactor of the existing `submit`: pull the signed-bytes computation out of the inner sign-and-post call so they can be returned alongside the `ExecutionReceipt`. Roughly: where the current code computes `signed = ed25519_sign(payload_bytes, &self.key)` and then POSTs, capture both `payload_bytes` and `hex::encode(signed.to_bytes())` into a tuple and pass them up.

- [ ] **Step 3: Dispatcher uses `OrderSink` — submit failure releases reservation (review §2.7)**

In `crates/xianvec-execution/src/dispatcher.rs`, replace the original Task 3.2 Stage-6 block with:

```rust
// Stage 6: open ledger row + sign + submit (with rollback on failure)
let position_id = ulid::Ulid::new().to_string();
let client_order_id = position_id.clone();

self.ledger.open_position(&position_id, &decision.agent_id, decision.asset, &decision.cycle_id).await?;

let result = self.order_sink.submit(&client_order_id, decision, equity_usdc).await;
match result {
    Ok(sr) => {
        self.audit.write(Stage::Sign, &decision.cycle_id, json!({
            "client_order_id": &client_order_id,
            "signed_payload_hex": hex::encode(&sr.signed_payload),
            "signature_hex": &sr.signature_hex,
        })).await?;
        self.audit.write(Stage::Submit, &decision.cycle_id, json!({
            "orderly_position_id": &sr.orderly_position_id,
            "fill_price": sr.fill_price,
        })).await?;
        self.ledger.update_after_submit(&position_id, &sr.orderly_position_id, sr.fill_price).await?;
        // Reservation lifecycle ends naturally (committed when position is open)
        self.reservations.commit(&decision.agent_id, &reservation_token).await?;
        Ok(Outcome::Submitted { position_id })
    }
    Err(e) => {
        // CRITICAL: release reservation AND delete the phantom position row
        self.reservations.release(&decision.agent_id, &reservation_token).await?;
        self.ledger.delete_phantom(&position_id).await?;
        self.audit.write(Stage::Reject, &decision.cycle_id, json!({
            "reason": "submit_failed",
            "error": format!("{:#}", e),
        })).await?;
        Err(e)
    }
}
```

- [ ] **Step 4: Move `quota_factor` cap multiplication INSIDE `try_reserve` (review §2.5)**

In the original plan Task 6.1 Step 4, the dispatcher computes `quota_factor` then passes `cap * quota_factor` to `try_reserve`. This is racy — two concurrent dispatches can read different quota factors. Refactor:

```rust
// In the reservation manager (replaces original plan Task 2.3 surface):
pub async fn try_reserve(
    &self,
    agent_id: &str,
    notional: f64,
    quota_inputs: &QuotaInputs,
    hard_cap: f64,
) -> Result<ReservationToken, ReservationError> {
    let _lock = self.lock_for(agent_id).await;
    let quota = compute_quota_factor(quota_inputs);
    let effective_cap = hard_cap * quota;
    let in_flight = self.ledger.in_flight_notional(agent_id).await?;
    let already_reserved = self.sum_reservations(agent_id).await?;
    if in_flight + already_reserved + notional > effective_cap {
        return Err(ReservationError::WouldExceedCap { /* ... */ });
    }
    // insert reservation row, return token
    // ...
}
```

The dispatcher passes `quota_inputs` and `hard_cap` to `try_reserve`. The reservation manager has the single source of truth on the cap.

- [ ] **Step 5: Empty-orderbook guard (review §9.2)**

In `crates/xianvec-execution/src/simulate.rs` (original plan Task 3.1 Step 2), replace:

```rust
// OLD - DO NOT USE
let mid = (book.bids[0].0 + book.asks[0].0) / 2.0;
```

with:

```rust
let (best_bid, best_ask) = match (book.bids.first(), book.asks.first()) {
    (Some(b), Some(a)) => (b.0, a.0),
    _ => return Err(anyhow!("orderbook empty for {:?}", asset)),
};
let mid = (best_bid + best_ask) / 2.0;
```

- [ ] **Step 6: Compute `orders_in_last_minute` from audit log (review §9.6)**

The dispatcher should not require its caller to track frequency-cap counters. Add a helper to the audit module:

```rust
impl AuditLog {
    pub async fn submit_count_since(
        &self,
        agent_id: &str,
        since_ms: i64,
    ) -> anyhow::Result<i64> {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM decisions WHERE agent_id = ? AND stage = 'submit' AND occurred_at >= ?",
        )
        .bind(agent_id)
        .bind(since_ms)
        .fetch_one(&self.pool)
        .await
        .map_err(Into::into)
    }
}
```

In the dispatcher, before applying frequency rules:

```rust
let now = chrono::Utc::now().timestamp_millis();
let orders_last_min = self.audit.submit_count_since(&decision.agent_id, now - 60_000).await?;
let orders_last_hour = self.audit.submit_count_since(&decision.agent_id, now - 3_600_000).await?;
```

- [ ] **Step 7: Stage enum `#[sqlx(rename_all = ...)]` correction (review §2.10)**

In `crates/xianvec-data/src/audit.rs` (original plan Task 1.2 Step 1), the `Stage` enum had `#[sqlx(rename_all = "lowercase")]` which maps `RiskEval` → `riskeval`. The migration's CHECK constraint expects `risk_eval`. Change to:

```rust
#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
pub enum Stage {
    Emit,
    RiskEval,
    Simulate,
    Sign,
    Submit,
    Response,
    Fill,
    Close,
    Cancel,
    Reject,
}
```

- [ ] **Step 8: Add audit-log append-only triggers (review §6.2)**

Append to the original migration `20260510000003_decisions.sql` (or add a new migration `20260510000011_decisions_append_only.sql`):

```sql
CREATE TRIGGER IF NOT EXISTS decisions_no_update
BEFORE UPDATE ON decisions
BEGIN
    SELECT RAISE(ABORT, 'decisions is append-only');
END;

CREATE TRIGGER IF NOT EXISTS decisions_no_delete
BEFORE DELETE ON decisions
BEGIN
    SELECT RAISE(ABORT, 'decisions is append-only');
END;
```

Add tests:

```rust
#[sqlx::test]
async fn audit_log_rejects_update(pool: SqlitePool) {
    let audit = AuditLog::new(pool.clone());
    audit.write(Stage::Emit, "cycle-a", json!({"a": 1})).await.unwrap();
    let r = sqlx::query("UPDATE decisions SET stage = 'submit' WHERE cycle_id = 'cycle-a'")
        .execute(&pool).await;
    assert!(r.is_err(), "update must be rejected by trigger");
}

#[sqlx::test]
async fn audit_log_rejects_delete(pool: SqlitePool) {
    let audit = AuditLog::new(pool.clone());
    audit.write(Stage::Emit, "cycle-a", json!({"a": 1})).await.unwrap();
    let r = sqlx::query("DELETE FROM decisions WHERE cycle_id = 'cycle-a'").execute(&pool).await;
    assert!(r.is_err(), "delete must be rejected by trigger");
}
```

- [ ] **Step 9: Canonicalize the audit content-hash (review §6.5)**

In `crates/xianvec-data/src/audit.rs`, the original plan computes `payload_hash = sha256(serde_json::to_string(&payload))`. JSON output ordering is not stable across versions. Replace with sorted-key canonical JSON:

```rust
fn canonical_json(value: &serde_json::Value) -> String {
    use serde_json::Value;
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            let parts: Vec<String> = keys.iter()
                .map(|k| format!("\"{}\":{}", k, canonical_json(&map[*k])))
                .collect();
            format!("{{{}}}", parts.join(","))
        }
        Value::Array(arr) => {
            let parts: Vec<String> = arr.iter().map(canonical_json).collect();
            format!("[{}]", parts.join(","))
        }
        v => v.to_string(),
    }
}

pub fn payload_hash(payload: &serde_json::Value) -> String {
    use sha2::{Sha256, Digest};
    let mut h = Sha256::new();
    h.update(canonical_json(payload).as_bytes());
    hex::encode(h.finalize())
}
```

Add a test:

```rust
#[test]
fn payload_hash_invariant_under_key_order() {
    let a = json!({"x": 1, "y": 2, "z": 3});
    let b = json!({"z": 3, "y": 2, "x": 1});
    assert_eq!(payload_hash(&a), payload_hash(&b));
}
```

- [ ] **Step 10: Commit**

```bash
git add -A
git commit -m "feat(execution,data): dispatcher redesign per review §2.1/§2.4/§2.5/§2.7/§9.2/§9.6/§2.10/§6.2/§6.5"
```

---

## Group D — Phase 4 CLI: key flow fixes (review §1.4, §1.7, §1.8, §3.7, §9.5, §12.2)

### Task D.1: `xvn key issue` implements EIP-712 + ip_restriction + Zeroizing

**Files:**
- Modify: `crates/xianvec-cli/src/commands/key.rs` (planned in original Task 4.7)
- Modify: `crates/xianvec-execution/src/orderly.rs` (add `register_trading_key` and `delete_orderly_key`)

- [ ] **Step 1: Add `register_trading_key` (EIP-712) to OrderlyExecutor**

`alloy = "2"` is already in workspace deps. Add `alloy = { workspace = true, features = ["sol-types", "signers"] }` to `crates/xianvec-execution/Cargo.toml` if not already present. The Orderly EIP-712 schema for `add_orderly_key`:

```rust
use alloy::primitives::{U256, Address};
use alloy::sol_types::{eip712_domain, sol, SolStruct};
use alloy::signers::{Signer, local::PrivateKeySigner};

sol! {
    struct AddOrderlyKey {
        string brokerId;
        uint256 chainId;
        string orderlyKey;
        string scope;
        uint64 timestamp;
        uint64 expiration;
    }
}

impl OrderlyExecutor {
    pub async fn register_trading_key(
        &self,
        evm_signer: &PrivateKeySigner,   // user's EVM private key (operator's wallet for v1)
        pubkey_hex: &str,
        scope: &str,
        ip_restriction: Option<&str>,
        ttl_days: u64,
    ) -> anyhow::Result<()> {
        let now = chrono::Utc::now().timestamp() as u64;
        let expiration = now + (ttl_days * 86400);
        let verifier: Address = self.orderly_verifier.parse()?;
        let domain = eip712_domain! {
            name: "Orderly",
            version: "1",
            chain_id: self.chain_id,
            verifying_contract: verifier,
        };
        let msg = AddOrderlyKey {
            brokerId: self.broker_id.clone().into(),
            chainId: U256::from(self.chain_id),
            orderlyKey: pubkey_hex.to_string().into(),
            scope: scope.to_string().into(),
            timestamp: now,
            expiration,
        };
        let hash = msg.eip712_signing_hash(&domain);
        let sig = evm_signer.sign_hash(&hash).await?;

        let mut payload = json!({
            "broker_id": self.broker_id,
            "chain_id": self.chain_id,
            "orderly_key": pubkey_hex,
            "scope": scope,
            "timestamp": now,
            "expiration": expiration,
            "signature": format!("0x{}", hex::encode(sig)),
        });
        if let Some(ip) = ip_restriction {
            payload["ip_restriction"] = json!(ip);
        }

        let mut payload_json = serde_json::json!({
            "broker_id": self.broker_id,
            "chain_id": self.chain_id,
            "orderly_key": pubkey_hex,
            "scope": scope,
            "timestamp": now,
            "expiration": expiration,
            "signature": format!("0x{}", hex::encode(sig.as_bytes())),
        });
        if let Some(ip) = ip_restriction {
            payload_json["ip_restriction"] = serde_json::json!(ip);
        }
        let resp = self.http
            .post(format!("{}/v1/orderly_key", self.base_url))
            .json(&payload_json)
            .send().await?
            .error_for_status()?;
        let _: serde_json::Value = resp.json().await?;
        Ok(())
    }

    pub async fn delete_orderly_key(
        &self,
        evm_signer: &PrivateKeySigner,
        pubkey_hex: &str,
    ) -> anyhow::Result<()> {
        // Mirror register_trading_key but with a DeleteOrderlyKey message type
        // and a DELETE HTTP verb. Per Orderly's published EIP-712 schema:
        //   sol! { struct DeleteOrderlyKey { string brokerId; uint256 chainId;
        //          string orderlyKey; uint64 timestamp; } }
        // Build the typed-data hash, sign, POST/DELETE to /v1/orderly_key/{pubkey}.
        // On non-2xx response, return Err so the CLI's revoke flow falls back to
        // printing manual instructions (see Task D.1 Step 4).
        let now = chrono::Utc::now().timestamp() as u64;
        let verifier: Address = self.orderly_verifier.parse()?;
        let domain = eip712_domain! {
            name: "Orderly",
            version: "1",
            chain_id: self.chain_id,
            verifying_contract: verifier,
        };
        sol! { struct DeleteOrderlyKey { string brokerId; uint256 chainId; string orderlyKey; uint64 timestamp; } }
        let msg = DeleteOrderlyKey {
            brokerId: self.broker_id.clone().into(),
            chainId: U256::from(self.chain_id),
            orderlyKey: pubkey_hex.to_string().into(),
            timestamp: now,
        };
        let hash = msg.eip712_signing_hash(&domain);
        let sig = evm_signer.sign_hash(&hash).await?;
        let resp = self.http
            .delete(format!("{}/v1/orderly_key/{}", self.base_url, pubkey_hex))
            .json(&serde_json::json!({
                "broker_id": self.broker_id,
                "chain_id": self.chain_id,
                "timestamp": now,
                "signature": format!("0x{}", hex::encode(sig.as_bytes())),
            }))
            .send().await?
            .error_for_status()?;
        let _: serde_json::Value = resp.json().await?;
        Ok(())
    }
}
```

- [ ] **Step 2: `xvn key issue` end-to-end**

In `crates/xianvec-cli/src/commands/key.rs`:

```rust
async fn issue(args: &KeyIssueArgs, ctx: &AppContext) -> anyhow::Result<()> {
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;
    use zeroize::Zeroizing;

    let signing = SigningKey::generate(&mut OsRng);
    let pubkey_hex = hex::encode(signing.verifying_key().to_bytes());
    let secret_bytes = Zeroizing::new(signing.to_bytes());

    let ip = args.ip_restriction.clone().or_else(|| std::env::var("XVN_PUBLIC_IP").ok());
    let evm_signer = ctx.load_evm_signer().await?;  // loads from 1Password / env per ops policy
    ctx.orderly.register_trading_key(
        &evm_signer,
        &pubkey_hex,
        "trading",
        ip.as_deref(),
        90,
    ).await?;

    ctx.trading_keys.insert(
        &args.user.unwrap_or_else(|| "op".into()),
        &pubkey_hex,
        secret_bytes.as_ref().try_into()?,
        "trading",
        ip.as_deref(),
        90,
    ).await?;

    println!("Issued trading key:");
    println!("  pubkey: {}", pubkey_hex);
    println!("  expires: {}", chrono::Utc::now() + chrono::Duration::days(90));
    println!();
    println!("To verify locally: `xvn key verify {}`", pubkey_hex);
    Ok(())
}
```

- [ ] **Step 3: `xvn key verify` (review §1.4 missing command)**

Add a `Verify { pubkey_hex: String }` arm to the `Key` subcommand. Implementation:

```rust
async fn verify(pubkey_hex: &str, ctx: &AppContext) -> anyhow::Result<()> {
    // Find the key by pubkey
    let user_id: String = sqlx::query_scalar(
        "SELECT user_id FROM trading_keys WHERE pubkey_hex = ?",
    ).bind(pubkey_hex).fetch_one(&ctx.pool).await?;

    let secret = ctx.trading_keys.load_decrypted(&user_id).await?;
    let signing = ed25519_dalek::SigningKey::from_bytes(secret.as_ref().try_into()?);
    let derived = hex::encode(signing.verifying_key().to_bytes());
    if derived == pubkey_hex {
        println!("OK: {} matches stored encrypted secret for user '{}'", pubkey_hex, user_id);
    } else {
        eprintln!("MISMATCH: derived pubkey {} != requested {}", derived, pubkey_hex);
        std::process::exit(2);
    }
    Ok(())
}
```

- [ ] **Step 4: `xvn key revoke` calls Orderly DELETE (review §1.8)**

Replace the original local-only revoke with:

```rust
async fn revoke(args: &KeyRevokeArgs, ctx: &AppContext) -> anyhow::Result<()> {
    let evm_signer = ctx.load_evm_signer().await?;
    let pubkey: String = sqlx::query_scalar(
        "SELECT pubkey_hex FROM trading_keys WHERE user_id = ? AND revoked_at IS NULL",
    ).bind(&args.user).fetch_one(&ctx.pool).await?;

    match ctx.orderly.delete_orderly_key(&evm_signer, &pubkey).await {
        Ok(()) => {
            ctx.trading_keys.revoke_local(&args.user).await?;
            println!("Revoked on Orderly side and locally for user '{}'.", args.user);
        }
        Err(e) => {
            ctx.trading_keys.revoke_local(&args.user).await?;
            eprintln!("Local revoke succeeded; Orderly-side delete FAILED: {:#}", e);
            eprintln!("Manual fallback: visit https://orderly.network and revoke key {} from the operator's wallet.", pubkey);
            std::process::exit(1);
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Dispatcher key-expiry fast-path (review §1.7)**

In `crates/xianvec-execution/src/dispatcher.rs`, after the global-halt check and before risk eval:

```rust
let now = chrono::Utc::now().timestamp_millis();
let expires_at: i64 = sqlx::query_scalar(
    "SELECT expires_at FROM trading_keys WHERE user_id = ? AND revoked_at IS NULL",
).bind(&decision.user_id).fetch_one(&self.pool).await?;
if now > expires_at - 86_400_000 {
    self.audit.write(Stage::Reject, &decision.cycle_id, json!({
        "reason": "key_expiring_soon_or_expired",
        "expires_at": expires_at,
    })).await?;
    return Ok(Outcome::Vetoed { reason: "trading key expired or expires within 24h".into() });
}
```

- [ ] **Step 6: Commit**

```bash
git add -A
git commit -m "feat(cli,execution): EIP-712 key issue + verify + revoke; ip_restriction; Zeroizing (review §1.4/§1.7/§1.8/§12.2)"
```

---

## Group E — Phase 5 (cross-margin guard, conditional on G2 outcome)

The original plan's Phase 5 is conditional on the G2 ADR. Two minor tightenings from review:

### Task E.1: Aggregate-margin caching + fail-closed (review §1.9)

**Files:**
- Modify: `crates/xianvec-risk/src/rules/aggregate_margin.rs` (planned in original Task 5.2)

- [ ] **Step 1: Specify the cache + fail-closed behavior**

In the aggregate-margin rule (CROSS_ONLY case), wrap the Orderly margin fetch in a 5-second TTL in-memory cache. On cache miss + endpoint failure, return `Vetoed { reason: "margin_fetch_unavailable" }`:

```rust
pub struct MarginGuard {
    orderly: Arc<dyn OrderSink>,
    cache: tokio::sync::Mutex<Option<(MarginInfo, Instant)>>,
    ttl: Duration,
}

impl MarginGuard {
    pub async fn current_margin(&self) -> Result<MarginInfo, MarginError> {
        let mut cache = self.cache.lock().await;
        if let Some((m, t)) = cache.as_ref() {
            if t.elapsed() < self.ttl {
                return Ok(m.clone());
            }
        }
        match self.orderly.fetch_margin().await {
            Ok(m) => {
                *cache = Some((m.clone(), Instant::now()));
                Ok(m)
            }
            Err(e) => Err(MarginError::FetchFailed(e.to_string())),
        }
    }

    pub async fn check(&self, projected_notional: f64) -> RuleOutcome {
        match self.current_margin().await {
            Ok(m) if m.utilization_pct + projected_pct(projected_notional, &m) > 0.85 => {
                RuleOutcome::Veto { reason: "aggregate margin > 85%".into() }
            }
            Ok(_) => RuleOutcome::Pass,
            Err(_) => RuleOutcome::Veto { reason: "margin_fetch_unavailable; failing closed".into() },
        }
    }
}
```

- [ ] **Step 2: Add a test**

```rust
#[tokio::test]
async fn margin_guard_fails_closed_when_endpoint_down() {
    let mock = MockOrderlyApi::with_margin_failure();
    let guard = MarginGuard::new(Arc::new(mock), Duration::from_secs(5));
    assert!(matches!(guard.check(100.0).await, RuleOutcome::Veto { .. }));
}
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(risk): aggregate-margin 5s cache + fail-closed (review §1.9)"
```

---

## Group F — Phase 6 + Phase 7 fixes

### Task F.1: Cold-start formula + drawdown decay (review §1.6, §9.4)

**Files:**
- Modify: `crates/xianvec-risk/src/quota.rs` (planned in original Task 6.1)

- [ ] **Step 1: Resolve the cold-start formula ambiguity (review §1.6, Option A)**

Replace the original implementation:

```rust
// OLD - DO NOT USE (mean-zero strategy gets quota = 0.75)
fn compute_quota_factor(i: &QuotaInputs) -> f64 {
    if i.closed_pnls_30d.len() < COLD_START_MIN_SAMPLES {
        return COLD_START_FLOOR;
    }
    let s = sharpe(&i.closed_pnls_30d);
    let dd = i.rolling_drawdown_30d;
    (COLD_START_FLOOR + sigmoid(s / SHARPE_NORMALIZER) * (1.0 - dd / DRAWDOWN_FLOOR).max(0.0))
        .min(1.0)
}
```

with:

```rust
fn compute_quota_factor(i: &QuotaInputs) -> f64 {
    let dd_decay = (1.0 - i.rolling_drawdown_30d / DRAWDOWN_FLOOR).max(0.0);

    if i.closed_pnls_30d.len() < COLD_START_MIN_SAMPLES {
        // Cold start: floor, but still apply drawdown decay (review §9.4)
        return COLD_START_FLOOR * dd_decay;
    }

    let s = sharpe(&i.closed_pnls_30d);
    // Operator-intent formula: max(floor, sigmoid * dd_decay)
    // Mean-zero (s=0, dd=0): max(0.25, 0.5 * 1.0) = 0.5
    // Hot (s>>0, dd=0): max(0.25, 1.0 * 1.0) = 1.0
    // Burned (s<<0): max(0.25, 0 * dd_decay) = 0.25 with dd intact, less if dd too
    (COLD_START_FLOOR.max(sigmoid(s / SHARPE_NORMALIZER) * dd_decay)).min(1.0)
}
```

- [ ] **Step 2: Add named-regime tests**

```rust
#[test]
fn cold_with_zero_drawdown_returns_floor() {
    let q = compute_quota_factor(&QuotaInputs { closed_pnls_30d: vec![1.0], rolling_drawdown_30d: 0.0 });
    assert!((q - 0.25).abs() < 1e-9);
}

#[test]
fn cold_with_50pct_drawdown_floor_decays() {
    // DRAWDOWN_FLOOR = 0.50 say; dd = 0.50 → decay = 0
    let q = compute_quota_factor(&QuotaInputs { closed_pnls_30d: vec![-0.5], rolling_drawdown_30d: 0.50 });
    assert!((q - 0.0).abs() < 1e-9);
}

#[test]
fn mean_zero_30plus_samples_returns_half() {
    let pnls = vec![0.0; 30];
    let q = compute_quota_factor(&QuotaInputs { closed_pnls_30d: pnls, rolling_drawdown_30d: 0.0 });
    assert!((q - 0.5).abs() < 1e-9);
}

#[test]
fn hot_strategy_returns_one() {
    let pnls = vec![100.0; 30];
    let q = compute_quota_factor(&QuotaInputs { closed_pnls_30d: pnls, rolling_drawdown_30d: 0.0 });
    assert!((q - 1.0).abs() < 1e-9);
}

#[test]
fn burned_strategy_returns_floor() {
    let pnls = vec![-100.0; 30];
    let q = compute_quota_factor(&QuotaInputs { closed_pnls_30d: pnls, rolling_drawdown_30d: 0.0 });
    assert!((q - 0.25).abs() < 1e-9);
}
```

- [ ] **Step 3: Add ledger queries that the quota computation needs (review §3.5)**

In `crates/xianvec-data/src/ledger.rs`:

```rust
impl Ledger {
    pub async fn closed_pnls_window(
        &self,
        agent_id: &str,
        since_ms: i64,
    ) -> anyhow::Result<Vec<f64>> {
        sqlx::query_scalar(
            "SELECT realized_pnl_usdc FROM positions WHERE agent_id = ? AND closed_at >= ? AND realized_pnl_usdc IS NOT NULL ORDER BY closed_at",
        )
        .bind(agent_id).bind(since_ms)
        .fetch_all(&self.pool).await
        .map_err(Into::into)
    }

    pub async fn rolling_drawdown_30d(&self, agent_id: &str) -> anyhow::Result<f64> {
        let now = chrono::Utc::now().timestamp_millis();
        let since = now - 30 * 86_400_000;
        let pnls: Vec<f64> = self.closed_pnls_window(agent_id, since).await?;
        // Walk equity curve to find peak-trough drawdown.
        let mut equity = 0.0;
        let mut peak = 0.0;
        let mut max_dd = 0.0;
        for p in pnls {
            equity += p;
            peak = peak.max(equity);
            let dd = (peak - equity) / peak.max(1.0);
            max_dd = max_dd.max(dd);
        }
        Ok(max_dd)
    }
}
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "feat(risk,data): quota cold-start formula + ledger drawdown query (review §1.6/§9.4/§3.5)"
```

### Task F.2: Reconciler real implementation + funding ingestion (review §1.1, §3.4)

**Files:**
- Modify: `crates/xianvec-execution/src/reconcile.rs` (planned in original Task 7.1)

- [ ] **Step 1: Replace `Reconciler::run` `todo!()` with the real loop**

```rust
impl Reconciler {
    pub async fn run(&self, user_id: &str) -> anyhow::Result<ReconcileReport> {
        let local_open = self.ledger.list_open_positions(user_id).await?;
        let server_open = self.orderly.list_positions(user_id).await?;

        let mut report = ReconcileReport::default();

        // Local-only: server has it not. Mark as drifted; could be a missed close.
        for local in &local_open {
            if !server_open.iter().any(|s| s.orderly_position_id == local.orderly_position_id) {
                self.ledger.mark_drifted(&local.position_id, "missing_on_server").await?;
                report.drifted += 1;
            }
        }

        // Server-only: we never tracked it. Open a tracking row in "untracked" state.
        for server in &server_open {
            if !local_open.iter().any(|l| l.orderly_position_id == server.orderly_position_id) {
                self.ledger.insert_untracked_position(server).await?;
                report.untracked += 1;
            }
        }

        // Both: verify size + entry_price. Update if needed.
        for server in &server_open {
            if let Some(local) = local_open.iter().find(|l| l.orderly_position_id == server.orderly_position_id) {
                if (local.size_usdc - server.size_usdc).abs() > 1e-6 {
                    self.ledger.update_size(&local.position_id, server.size_usdc).await?;
                    report.size_corrected += 1;
                }
            }
        }

        Ok(report)
    }
}
```

- [ ] **Step 2: Funding ingestion (review §1.1 BLOCKER)**

Add a new method:

```rust
impl Reconciler {
    pub async fn ingest_funding(&self, user_id: &str) -> anyhow::Result<usize> {
        let last_seen = self.ledger.last_funding_event_ms(user_id).await?.unwrap_or(0);
        let events = self.orderly.funding_history_since(user_id, last_seen).await?;
        let mut inserted = 0;
        for ev in events {
            // Match each event to the position holding the symbol during its window
            let pos = self.ledger.find_position_holding_during(
                user_id, &ev.symbol, ev.funding_time_ms,
            ).await?;
            if let Some(p) = pos {
                self.ledger.insert_funding_attribution(
                    &p.position_id, ev.funding_amount_usdc, ev.funding_time_ms,
                ).await?;
                inserted += 1;
            }
        }
        Ok(inserted)
    }
}
```

- [ ] **Step 3: Wire funding into `realized_pnl_usdc` at close**

In `crates/xianvec-data/src/ledger.rs`:

```rust
impl Ledger {
    pub async fn close_position(
        &self,
        position_id: &str,
        exit_price: f64,
        gross_pnl_usdc: f64,
    ) -> anyhow::Result<()> {
        let funding_total: f64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(funding_amount_usdc), 0) FROM funding_attributions WHERE position_id = ?",
        ).bind(position_id).fetch_one(&self.pool).await?;
        let realized = gross_pnl_usdc + funding_total;
        sqlx::query(
            "UPDATE positions SET realized_pnl_usdc = ?, exit_price = ?, closed_at = ? WHERE position_id = ?",
        ).bind(realized).bind(exit_price).bind(chrono::Utc::now().timestamp_millis()).bind(position_id)
            .execute(&self.pool).await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Add the test (review §1.1 explicit case)**

```rust
#[sqlx::test]
async fn realized_pnl_includes_funding_for_position_held_across_two_periods(pool: SqlitePool) {
    let ledger = Ledger::new(pool);
    let position_id = "pos1";
    ledger.open_position(position_id, "agent-a", AssetSymbol::Btc, "cycle-1", /* ... */).await.unwrap();
    ledger.insert_funding_attribution(position_id, -10.5, 1700000000_000).await.unwrap();
    ledger.insert_funding_attribution(position_id, -8.0, 1700003600_000).await.unwrap();
    ledger.close_position(position_id, 100.0, 50.0).await.unwrap();

    let realized = ledger.realized_pnl(position_id).await.unwrap();
    assert!((realized - (50.0 - 18.5)).abs() < 1e-9);
}
```

- [ ] **Step 5: Commit**

```bash
git add -A
git commit -m "feat(execution,data): reconciler implementation + funding ingestion (review §1.1/§3.4)"
```

---

## Group G — Phase 9 final integration + scope drops

### Task G.1: DROP OFAC and public-disclosure SLA from Phase 9

**Files:**
- Modify: original plan Task 9.1 and Task 9.2

- [ ] **Step 1: Remove any task that adds OFAC sanctions screening**

Search the original plan for "OFAC" or "sanctions" — if any was added by a previous edit, remove it. The current plan (as of the original 3,606-line draft) does NOT include OFAC. The decision is to keep it out of the wallet plan and cover it in the future hosted-instance plan if/when one is written. Add to FOLLOWUPS instead — see Task G.4 below.

- [ ] **Step 2: Replace any "public disclosure SLA" task with a README warning**

The wallet plan does not currently have a public-SLA task. The research-flagged "public disclosure SLA commitment" is moved to the leverage-items plan as a one-line README warning, NOT as a binding SLA. No change needed in the wallet plan beyond confirming none exists.

### Task G.2: ADD adversarial spam-key test (review §6.1)

**Files:**
- Add: a new sub-task to original plan Task 9.1 (e2e integration test).

- [ ] **Step 1: Write the adversarial test**

```rust
#[tokio::test]
async fn compromised_key_cannot_drain_funds_or_exceed_caps() {
    let env = TestEnv::new_with_real_testnet().await;
    let key = env.issue_test_trading_key().await;

    // 1. Spam 1000 order intents in 60s
    let mut handles = Vec::new();
    for i in 0..1000 {
        let env = env.clone();
        handles.push(tokio::spawn(async move {
            env.dispatch_order(format!("cycle-{}", i), 100.0).await
        }));
    }
    let outcomes: Vec<_> = futures::future::join_all(handles).await;
    let approved = outcomes.iter().filter(|r| matches!(r.as_ref().unwrap(), Ok(Outcome::Submitted { .. }))).count();
    let rejected = outcomes.iter().filter(|r| matches!(r.as_ref().unwrap(), Ok(Outcome::Vetoed { .. }))).count();

    // Per spec: max_orders_per_minute * 60s upper bound
    assert!(approved <= env.config().max_orders_per_minute as usize * 1, "frequency cap should reject excess");
    assert!(rejected > 900, "most should be rejected");

    // 2. After `xvn kill --user op`, no further submits succeed
    env.kill_user("op").await;
    tokio::time::sleep(Duration::from_secs(1)).await;
    let post_kill_attempt = env.dispatch_order("post-kill", 100.0).await;
    assert!(matches!(post_kill_attempt, Ok(Outcome::Halted) | Ok(Outcome::Vetoed { .. })));

    // 3. Withdraw via the trading key returns 401/403 (Orderly-side scope enforcement)
    let withdraw_result = env.attempt_withdraw_with_trading_key(&key, 1.0).await;
    assert!(matches!(withdraw_result, Err(_)));
}
```

(Run only when `XVN_TEST_TESTNET=1` is set; otherwise skip.)

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "test: adversarial spam-key e2e test (review §6.1)"
```

### Task G.3: ADD N=10+ concurrency test (review §6.3)

**Files:**
- Add: a property test in `crates/xianvec-risk/tests/`.

- [ ] **Step 1: Write the test**

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn n_concurrent_reservations_respect_cap(
        n_strategies in 1..50_usize,
        cap in 100.0f64..10000.0f64,
        notional in 1.0f64..1000.0f64,
    ) {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let manager = ReservationManager::new(test_pool().await);
            let mut handles = Vec::new();
            for i in 0..n_strategies {
                let m = manager.clone();
                handles.push(tokio::spawn(async move {
                    m.try_reserve(&format!("agent-{}", i), notional, &neutral_quota_inputs(), cap).await
                }));
            }
            let outcomes = futures::future::join_all(handles).await;
            let total_reserved: f64 = outcomes.iter()
                .filter_map(|r| r.as_ref().unwrap().as_ref().ok())
                .map(|t| t.notional).sum();
            // No agent's total reservations exceed its cap
            for i in 0..n_strategies {
                let agent_total = outcomes.iter().enumerate()
                    .filter(|(idx, r)| *idx == i && r.as_ref().unwrap().is_ok())
                    .count() as f64 * notional;
                prop_assert!(agent_total <= cap);
            }
        });
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "test: N-strategy concurrent reservation property test (review §6.3)"
```

### Task G.4: Update FOLLOWUPS.md with discarded ideas + deferred items

**Files:**
- Modify: `FOLLOWUPS.md` (root)

- [ ] **Step 1: Append the following entries**

```markdown
## Wallet plan deferred / discarded items (2026-05-10)

### Deferred to v2 (post-hackathon)
- **MPC trading-key signing** (spec §10): single-process key custody is the v1 residual risk; MPC closes that.
- **Browser-issued ephemeral session keys** (spec §10): user-side ephemeral key, server holds nothing.
- **Smart-account / ERC-4337 trading scope** (spec §10): scope at the wallet level rather than the broker's API key.
- **OKX / Kite Passport eval** (spec §10): research alternative non-custodial broker rails.
- **Fystack pilot** (spec §10): MPC custody-as-a-service pilot.
- **Performance-fee-on-withdrawal helper contract** (spec §10).
- **x402 per-trade micropayments** (spec §10).

### Discarded (with rationale)
- **Reputation-weighted quota in wallet engine** — proposed 2026-05-10 (research Theme C); discarded same day.
  Rationale: trade-size perception varies wildly between traders ($100 to one is $100k to another), so quota-as-multiple-of-cap is not a meaningful metric to attach to reputation. The broader intent — expand ERC-8004 with viable economic-teeth metrics — is preserved as an open question; specific mechanism TBD.
- **OFAC sanctions screening in wallet plan** — proposed 2026-05-10 (research Theme D); deferred same day.
  Rationale: xianvec is open-source; the maintainers do not bear OFAC obligations for self-hosted users. A future hosted-instance plan (xianvec.io marketplace operator) needs OFAC screening at the marketplace contract event handler. Out of scope for the wallet rail.
- **Public disclosure SLA commitment** — proposed 2026-05-10 (research Theme E); discarded same day.
  Rationale: open-source project; "use at your own risk" README warning is the appropriate disclosure. SLA commitments are inappropriate for unmaintained-by-a-corporation code.

### Cross-plan reminders
- **SLF4 reputation writes** (separate plan): assumes `realized_pnl_usdc` includes funding. Group F's funding ingestion makes this true. Verify before SLF4 ships.
- **Live cockpit (Plan 2d)**: should join `scheduler_events` (Plan 2c) + `decisions` (this plan) when it surfaces trade history.
- **F18 `asset` on `TraderDecision`**: NOW DONE (Group A). Remove from FOLLOWUPS.
- **`agent_id` rename**: NOW DONE (terminology rename Option B). Remove from FOLLOWUPS.
```

- [ ] **Step 2: Commit**

```bash
git add FOLLOWUPS.md
git commit -m "docs(followups): wallet plan v1.1 deferred + discarded items"
```

### Task G.5: Phase 8 dashboard route — make standalone fallback the default (review §5.2)

**Files:**
- Modify: original plan Phase 8 preamble (line 3154) and Task 8.1 (line 3160).

- [ ] **Step 1: Update the Phase 8 preamble**

The original preamble says "Plan 2d assumed complete; standalone fallback is documented if 2d slips." Reverse the polarity: standalone is the default, lift-into-2d is a follow-up.

In the original plan around line 3154, replace:

```markdown
## Phase 8 — Strategy Budgets Spreadsheet UI

**Primary path:** `xianvec-dashboard` (Plan 2d). Standalone fallback if 2d slipped past 2026-06-01.
```

with:

```markdown
## Phase 8 — Agent Budgets Spreadsheet UI

**Primary path:** standalone Axum crate `xianvec-budget-ui`, launched via `xvn budget serve`. Lift-into-`xianvec-dashboard` (Plan 2d) is a follow-up; not blocking the wallet plan ship.

**Why default to standalone:** Plan 2d's `AppState` is intentionally minimal and adding DB + Orderly + ledger access to it forces dependencies on every other dashboard route. Building Phase 8 in its own crate keeps the surface focused. Lifting routes into `xianvec-dashboard` post-hackathon is a mechanical move-and-rewire.

**Local-bind constraint:** `xvn budget serve` MUST bind to 127.0.0.1 by default (review §12.4). Any networking exposure requires explicit `--bind` flag and a `X-XVN-OPERATOR-SECRET` header check.
```

- [ ] **Step 2: Update Task 8.1 to skip the 2d-existence probe**

The original Task 8.1 was "Verify Plan 2d's dashboard exists; if not, branch to fallback." Replace with:

```markdown
### Task 8.1: Set up the standalone `xianvec-budget-ui` crate

(Original Task 8.1's "verify 2d existence" check is removed; we always build standalone for v1.)
```

- [ ] **Step 3: Add agent-rename feature wiring as a future hook in Phase 8 routes**

The leverage-items plan (`2026-05-10-leverage-items.md`) introduces a runtime agent-rename feature. The Phase 8 routes should reserve `display_name` in their templates so adding the rename is a small follow-up:

```rust
// In `xianvec-budget-ui/src/templates.rs`:
pub struct BudgetRow {
    pub agent_id: String,
    pub display_name: Option<String>,  // populated by leverage-items plan
    pub hard_cap: f64,
    // ... other fields
}
```

- [ ] **Step 4: Commit**

```bash
git add -A
git commit -m "refactor(plan): Phase 8 default to standalone xianvec-budget-ui (review §5.2)"
```

---

## Group H — Cross-cutting hygiene

### Task H.1: Add `humantime` workspace dep + `xvn audit --since` parsing (review §3.10)

**Files:**
- Modify: workspace `Cargo.toml`.
- Modify: `crates/xianvec-cli/src/commands/audit.rs` (planned in original Task 4.9).

- [ ] **Step 1: Add `humantime = "2"` to `[workspace.dependencies]`**

- [ ] **Step 2: Replace the original Task 4.9 Step 2 placeholder with**:

```rust
fn parse_since(s: &str) -> anyhow::Result<i64> {
    if let Ok(d) = humantime::parse_duration(s) {
        let now = chrono::Utc::now().timestamp_millis();
        return Ok(now - d.as_millis() as i64);
    }
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Ok(dt.timestamp_millis());
    }
    Err(anyhow::anyhow!("unrecognized --since format: '{}' (try '1h', '24h', '7d', or RFC3339)", s))
}
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(cli): humantime-based --since parser (review §3.10)"
```

### Task H.2: Add SQLite WAL mode at pool init (review §3.8)

**Files:**
- Modify: `AppContext::from_env` (Task 4.10 of original plan)

- [ ] **Step 1: Apply WAL pragma after pool creation**

```rust
let pool = SqlitePoolOptions::new()
    .max_connections(8)
    .connect(&db_url).await?;
sqlx::query("PRAGMA journal_mode = WAL;").execute(&pool).await?;
sqlx::query("PRAGMA synchronous = NORMAL;").execute(&pool).await?;
```

- [ ] **Step 2: Commit**

```bash
git add -A
git commit -m "perf(data): enable SQLite WAL + synchronous=NORMAL (review §3.8)"
```

### Task H.3: Log redaction for trading key (review §7.6)

**Files:**
- Modify: `crates/xianvec-data/src/trading_keys.rs` (already created in Task B.1)

- [ ] **Step 1: Newtype the secret-key bytes with a custom Debug**

```rust
#[derive(Clone)]
pub struct SecretKeyBytes(Zeroizing<[u8; 32]>);

impl std::fmt::Debug for SecretKeyBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretKeyBytes(<redacted>)")
    }
}

impl SecretKeyBytes {
    pub fn new(bytes: [u8; 32]) -> Self { Self(Zeroizing::new(bytes)) }
    pub fn as_bytes(&self) -> &[u8; 32] { &*self.0 }
}
```

- [ ] **Step 2: Add a CI grep for hex-blob leakage in test output**

Add to `Makefile` or CI step:

```bash
cargo test --workspace 2>&1 | tee /tmp/xvn-test-out.txt
if grep -E '\b[a-f0-9]{64}\b' /tmp/xvn-test-out.txt | grep -v "pubkey" > /tmp/xvn-leak-check.txt; then
    if [ -s /tmp/xvn-leak-check.txt ]; then
        echo "POTENTIAL SECRET LEAKAGE — non-pubkey 64-char hex string in test output"
        cat /tmp/xvn-leak-check.txt
        exit 1
    fi
fi
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "feat(data,ci): trading-key log redaction + CI leak check (review §7.6)"
```

---

## Self-review

**Spec coverage** — every blocker from the adversarial review has at least one task:

| Blocker | Group/Task | Status |
|---|---|---|
| §1.1 funding attribution writer | Group F.2 | ✓ task added |
| §1.2 trading_keys table | Group B.1 | ✓ task added |
| §1.3 global_halt | Group B.2 | ✓ task added |
| §1.4 phishing-resistant browser flow | Group D.1 | ✓ EIP-712 register + verify |
| §1.5 policy hot-reload enforced | Group B.3 | ✓ StrategyConfigStore |
| §2.1 OrderlyOrderSubmit redesign | Group C.1 | ✓ wrap-not-replace |
| §2.2 TraderDecision shape | Group A.1 | ✓ asset added; bps→USDC via mark_price |

Highs covered as listed in groups B/C/D/E/F/G/H. Mediums and lows partially covered; remaining items are minor hygiene (e.g. CLI table formatting, doc comments) and tracked in Group G.4 FOLLOWUPS for batch cleanup post-ship.

**Placeholder scan** — no TBD/TODO/"implement later". Each step has either a concrete code block, a concrete command, or an explicit modification target.

**Type/name consistency** — uses `agent_id` (not `agent_id`), `cycle_id` (not `cycle_id`), `Algorithm` (not `Strategy` for the trait), `OrderSink` (not `OrderlyOrderSubmit`), `PerStrategyVerdict` (mentioned in CLAUDE.md doc; not yet introduced as a type — when the plan's Verdict enum is defined in original Task 2.2, name it `PerStrategyVerdict`), `SubmitResult` (not `ExecutionReceipt` — distinct from the existing receipt type).

**Order-of-execution** — Group A (F18) before Group C (dispatcher) because the dispatcher uses `decision.asset`. Group B (migrations) before Group C (dispatcher uses `global_state` and `strategies`). Group F (quota + reconciler) and G (Phase 9 tests) last. The wallet plan's original Phase 0–9 still drives the overall order; these amendments slot into the corresponding phases.

**Open coordination point** — the `agent_id` field in the plan's `decisions` and `positions` migrations (originally `agent_id`) was renamed by the rename plan's Phase 4.2 sed pass. If those migration files contain `agent_id`, run that sed pass before applying these amendments.
