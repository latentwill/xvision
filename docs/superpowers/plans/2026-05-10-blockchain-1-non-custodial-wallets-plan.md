# Blockchain Plan #1 — Non-Custodial Agent Wallets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> **Terminology:** Updated 2026-05-10 — `strategy_id` renamed to `agent_id` per Option B (see [`docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`](2026-05-10-terminology-rename-option-b.md)). The id is a local ULID pre-mint, resolves to the NFT token id post-SLF3. The amendments doc [`2026-05-10-blockchain-1-non-custodial-wallets-amendments.md`](2026-05-10-blockchain-1-non-custodial-wallets-amendments.md) supersedes specific sections of this plan; read it before executing.
> **Spec:** [`docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`](../specs/2026-05-09-non-custodial-agent-wallets-design.md)
> **Depends on:** Plan #1 (Strategy Creation Engine MVP — shipped). No hard dependency on Plans 2a / 2b / 2c, but Phase 4 (CLI) and Phase 8 (UI) integrate with their surfaces if shipped.
> **Related (parallel work):**
> - **SLF2 — ERC-8004 deployment on Mantle Sepolia** (`decisions/0008-erc8004-deployment.md`). Independent of this plan; wallet rail does not depend on registries existing.
> - **SLF3 — per-strategy NFT mint** (FOLLOWUPS.md). Once shipped, `agent_id` in this plan's ledger resolves to the NFT id; pre-mint we use a local ULID.
> - **Plan 5 — Marketplace contract surface** (`docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`, deferred). This plan assumes that contract spec verbatim; the only shared dependency is the `protocolFeeRecipient` settlement wallet address.
> - **Plan 2c — Durable scheduler** (`docs/superpowers/plans/2026-05-08-strategy-engine-2c-scheduler-live-exec.md`). The reconciliation job in Phase 7 ships as a scheduled job; if 2c is in flight, it lives in 2c's scheduler. Otherwise it ships as a standalone tokio task.
> - **Plan 2d — Web dashboard** (`docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md`). **Assumed complete by the time this plan executes** — Phase 8 ships the Strategy Budgets spreadsheet as a new route inside `crates/xvision-dashboard`, alongside the existing Wizard / Inspector / Live cockpit archetypes. A standalone Axum server is documented in Phase 8 as a fallback if 2d slips, but the primary path is the dashboard route.
> **Hackathon deadline:** 2026-06-15. Phases 0–9 ship pre-deadline. Step 7 of the spec's migration (multi-tenant per-user keys) is **post-hackathon** and intentionally not in this plan.

---

**Goal:** Make `xvision` a non-custodial trading orchestrator: each strategy variant gets enforced per-strategy budgets, all activity is audited, the operator has kill switches and emergency-close, and a spreadsheet UI lets them edit budgets across strategies. After this plan ships: the operator funds their own Orderly account, xvision signs orders with a trading-only Ed25519 key, every decision is logged, every cap is enforced race-free, and one CLI call (`xvn emergency-close --all`) returns the user's account to flat in seconds.

**Architecture:** Two rails (trading + marketplace) cleanly separated. The trading rail is fully non-custodial — xvision holds a scoped Orderly trading key that cannot withdraw, an off-chain SQLite ledger for attribution, and a Risk Engine that enforces per-strategy hard caps × dynamic quotas via a reservation pattern. The marketplace rail is the only smart-contract surface; this plan does not implement those contracts (Plan 5) but reserves a `protocolFeeRecipient` settlement wallet. v1 ships in single-user mode (operator = the only user), with multi-tenant key issuance deferred.

**Tech Stack:** Rust 2021 (workspace at `Cargo.toml`). New deps: `aes-gcm = "0.10"` (encrypt trading key at rest), `ulid = "1"` (idempotency keys), `dialoguer = "0.11"` (CLI confirm prompts). Phase 8 reuses `xvision-dashboard`'s existing `axum` + `askama` stack from Plan 2d — no new web deps in this plan. Existing: `sqlx` (sqlite+tokio), `clap`, `tokio`, `tracing`, `ed25519-dalek`, `reqwest`. SQLite event store for ledger + audit log + reservations + status tables; one connection pool per process.

**Out of scope (deferred):**
- Multi-tenant onboarding (Step 7 of spec migration) — single-user mode only in v1.
- Performance-fee-on-withdrawal helper contract (spec §10).
- x402 per-trade micropayments (spec §10).
- MPC trading-key signing (spec §10) — single-process key custody acknowledged residual risk.
- Smart-account / ERC-4337 trading scope (spec §10).
- Marketplace contract implementation (Plan 5, separate plan).
- ERC-8004 registry writes (SLF4/SLF5, separate work). The ledger will tag positions with `agent_id` regardless; once the NFT registry ships, the same id resolves to the on-chain NFT.

---

## File structure

```
crates/
├── xvision-data/
│   ├── Cargo.toml                                # add: sqlx workspace dep, ulid, sha2
│   └── src/
│       ├── lib.rs                                # MODIFY: pub mod ledger; pub mod audit; pub mod migrations;
│       ├── ledger.rs                             # NEW — positions + funding_attributions + helpers
│       ├── audit.rs                              # NEW — append-only decisions writer + reader
│       ├── status.rs                             # NEW — strategy_status table + transitions
│       ├── policy.rs                             # NEW — policy_changes journal
│       ├── pending.rs                            # NEW — pending_approvals + pending_reservations
│       └── migrations/                           # NEW — sqlx migrations dir
│           ├── 20260510000001_positions.sql
│           ├── 20260510000002_funding_attributions.sql
│           ├── 20260510000003_decisions.sql
│           ├── 20260510000004_strategy_status.sql
│           ├── 20260510000005_pending_approvals.sql
│           ├── 20260510000006_policy_changes.sql
│           └── 20260510000007_pending_reservations.sql
│
├── xvision-risk/
│   ├── Cargo.toml                                # add: workspace deps already present
│   └── src/
│       ├── lib.rs                                # MODIFY: pub mod reservations; pub mod quota; pub mod aggregate_margin;
│       ├── config.rs                             # MODIFY: extend with per-strategy schema (allowed_chains, allowed_assets, slippage, frequency, active_hours, approval_above)
│       ├── reservations.rs                       # NEW — write-locked reservation table
│       ├── quota.rs                              # NEW — pure quota_factor function
│       ├── rules/
│       │   ├── mod.rs                            # MODIFY: add per_strategy + aggregate_margin
│       │   ├── per_strategy.rs                   # NEW — per-strategy hard cap + scoped permissions
│       │   └── aggregate_margin.rs               # NEW — cross-margin contagion guard
│
├── xvision-execution/
│   ├── Cargo.toml                                # already has reqwest, ed25519-dalek; add ulid (workspace)
│   └── src/
│       ├── lib.rs                                # MODIFY: pub mod dispatcher; pub mod simulate; pub mod reconcile; pub mod approval;
│       ├── orderly.rs                            # MODIFY: extract sign_request to be reusable; expose order-info wrapper
│       ├── dispatcher.rs                         # NEW — OrderDispatcher: gate→reserve→simulate→sign→submit→audit
│       ├── simulate.rs                           # NEW — pre-trade simulation against Orderly order-info
│       ├── reconcile.rs                          # NEW — periodic reconciliation against live Orderly state
│       └── approval.rs                           # NEW — pending_approvals workflow + TTL handling
│
├── xvision-identity/
│   ├── Cargo.toml                                # add: aes-gcm
│   └── src/
│       ├── lib.rs                                # MODIFY: pub mod trading_key;
│       └── trading_key.rs                        # NEW — AES-256-GCM encrypted key storage; load/store/rotate
│
├── xvision-cli/
│   ├── Cargo.toml                                # add: dialoguer
│   └── src/
│       ├── main.rs                               # MODIFY: register new subcommands
│       └── commands/
│           ├── mod.rs                            # MODIFY: pub mod for each new command
│           ├── key.rs                            # NEW — issue/list/rotate/revoke
│           ├── budget.rs                         # NEW — show/set/bulk-import/serve
│           ├── kill.rs                           # NEW — kill --strategy/--user/--all
│           ├── unhalt.rs                         # NEW — unhalt --strategy
│           ├── emergency_close.rs                # NEW — emergency-close --strategy/--user/--all
│           ├── approve.rs                        # NEW — approve list/<id>/--reject
│           ├── audit.rs                          # NEW — audit position/strategy/pending
│           └── reconcile.rs                      # NEW — reconcile --user [--dry-run]
│
└── xvision-dashboard/                           # FROM PLAN 2d (assumed to exist)
    └── src/
        ├── lib.rs                               # MODIFY: register the new /budgets route
        ├── routes/
        │   └── budgets.rs                       # NEW — GET /budgets, POST /budgets/:strategy
        └── templates/
            ├── budgets.html                     # NEW — the strategy-budgets spreadsheet
            └── budgets_confirm_partial.html     # NEW — edit-confirm partial

# Fallback only if Plan 2d hasn't shipped on time:
#   crates/xvision-budget-ui/  — standalone Axum server with the same routes/templates,
#   launched via `xvn budget serve`. Lift the routes module into xvision-dashboard once 2d lands.

probes/
├── m1-orderly-key-scope/                         # NEW — G1 validation probe
│   ├── Cargo.toml
│   ├── README.md
│   └── src/main.rs
└── m2-orderly-margin-modes/                      # NEW — G2 validation probe
    ├── Cargo.toml
    ├── README.md
    └── src/main.rs

decisions/
├── 0012-orderly-key-scope-validation.md          # NEW — ADR from G1 probe outcome
└── 0013-orderly-margin-mode-validation.md        # NEW — ADR from G2 probe outcome

docs/
├── cli-reference.md                              # NEW — full CLI subcommand reference
└── superpowers/plans/
    └── 2026-05-10-blockchain-1-non-custodial-wallets-plan.md   # this plan

# also: Cargo.toml (workspace) — add aes-gcm, ulid, dialoguer, axum, askama to [workspace.dependencies]
#       Cargo.toml (workspace) — add xvision-budget-ui to members + default-members
```

---

## Phase 0 — Validation Gates (BLOCKS everything else)

The spec's two load-bearing assumptions must be confirmed against Orderly's live API before any other code is written. If G1 fails, the entire security model collapses and the plan must be reworked. If G2 falls back to cross-margin only, Phase 5's design changes.

### Task 0.1: Bootstrap workspace deps

**Files:**
- Modify: `Cargo.toml` (workspace root)

- [ ] **Step 1: Add workspace deps**

Edit `Cargo.toml` `[workspace.dependencies]`:

```toml
# crypto
aes-gcm = "0.10"
sha2    = "0.10"

# identifiers
ulid = "1"

# cli UX
dialoguer = "0.11"
```

`axum` / `askama` / `tower-http` are already declared in the workspace by Plan 2d. If Plan 2d has *not* yet landed when this plan starts execution, add them here too:

```toml
axum       = "0.7"
askama     = "0.12"
tower-http = { version = "0.5", features = ["fs", "trace"] }
```

- [ ] **Step 2: Verify workspace still builds**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml
git commit -m "build: add workspace deps for non-custodial wallets plan (aes-gcm, ulid, dialoguer)"
```

### Task 0.2: G1 probe — trading-only Orderly key scope

**Files:**
- Create: `probes/m1-orderly-key-scope/Cargo.toml`
- Create: `probes/m1-orderly-key-scope/README.md`
- Create: `probes/m1-orderly-key-scope/src/main.rs`

- [ ] **Step 1: Probe scaffold**

Create `probes/m1-orderly-key-scope/Cargo.toml`:

```toml
[package]
name = "m1-orderly-key-scope"
version = "0.1.0"
edition = "2021"
publish = false

[dependencies]
tokio    = { workspace = true }
reqwest  = { workspace = true }
serde    = { workspace = true }
serde_json = { workspace = true }
ed25519-dalek = { version = "2", features = ["pkcs8"] }
rand_core     = "0.6"
anyhow   = { workspace = true }
tracing  = { workspace = true }
tracing-subscriber = { workspace = true }
```

Create `probes/m1-orderly-key-scope/README.md`:

```markdown
# M1 — Orderly trading-only key scope (Validation Gate G1)

**Goal:** Confirm Orderly's `add_orderly_key` API permits a trading-only scope that excludes vault withdrawal and inter-account transfer.

**Why:** The non-custodial wallet design assumes a key with this exact property. If trading-only does not exist, the design must change before any code is written.

**Setup:**
- Run on Orderly mainnet with a test account holding ≤ $10 USDC.
- Set env: `ORDERLY_EVM_KEY` (the EVM private key for the test account), `ORDERLY_ACCOUNT_ID`.

**Run:** `cargo run -p m1-orderly-key-scope`

**Expected outcome:** Probe registers a trading-only Ed25519 key, then attempts a withdrawal call signed with that key. Withdrawal MUST be rejected with an authorization error. Order placement MUST succeed.

**Result:** record outcome in `decisions/0012-orderly-key-scope-validation.md`.
```

- [ ] **Step 2: Probe implementation**

Create `probes/m1-orderly-key-scope/src/main.rs`:

```rust
//! M1 — Validation Gate G1: trading-only Orderly key scope.
//!
//! Outcome:
//!   PASS = withdrawal rejected, order placed → spec proceeds as designed.
//!   FAIL = withdrawal succeeded → halt plan; redesign required (smart-account wrapper).

use anyhow::{anyhow, bail, Result};
use ed25519_dalek::{SigningKey, VerifyingKey};
use rand_core::OsRng;
use tracing::{error, info, warn};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let evm_key = std::env::var("ORDERLY_EVM_KEY")
        .map_err(|_| anyhow!("ORDERLY_EVM_KEY env var required"))?;
    let account_id = std::env::var("ORDERLY_ACCOUNT_ID")
        .map_err(|_| anyhow!("ORDERLY_ACCOUNT_ID env var required"))?;

    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let pubkey: VerifyingKey = signing_key.verifying_key();
    info!(pubkey = ?hex::encode(pubkey.to_bytes()), "generated test trading key");

    // Step 1: Register trading-only key via add_orderly_key.
    // (Use the same EIP-712 sign-and-POST pattern as crates/xvision-execution/src/orderly.rs:onboard_key.)
    let registered = register_trading_only_key(&evm_key, &account_id, &pubkey).await?;
    info!(registered_key_id = %registered, "key registered with trading-only scope");

    // Step 2: Attempt withdrawal signed by the trading key. MUST fail.
    let withdraw_result = attempt_withdrawal_with_trading_key(&signing_key, &account_id).await;
    match withdraw_result {
        Ok(_) => {
            error!("CRITICAL: withdrawal SUCCEEDED with trading-only key — security model broken");
            bail!("G1 FAILED: trading-only scope does not block withdraw");
        }
        Err(e) => {
            info!(?e, "withdrawal correctly rejected — G1 PASS step 1/2");
        }
    }

    // Step 3: Place a tiny order (e.g. limit at far-from-market price). MUST succeed.
    let order_result = place_test_order_with_trading_key(&signing_key, &account_id).await?;
    info!(order_id = %order_result, "order placed successfully — G1 PASS step 2/2");

    info!("G1 validation PASSED — proceed with spec as designed");
    Ok(())
}

async fn register_trading_only_key(
    evm_key: &str,
    account_id: &str,
    pubkey: &VerifyingKey,
) -> Result<String> {
    // TODO during execution: copy the EIP-712 + add_orderly_key flow from
    // crates/xvision-execution/src/orderly.rs (existing onboarding helper).
    // Set permissions to ["trading"] only — explicitly omit "withdraw" and "transfer".
    todo!("EIP-712 + POST /v3/orderly_key — see existing orderly.rs sign_request fn")
}

async fn attempt_withdrawal_with_trading_key(
    signing_key: &SigningKey,
    account_id: &str,
) -> Result<()> {
    // POST /v3/withdraw_request with Ed25519-signed body. Should 403/401.
    todo!("POST /v3/withdraw_request signed by trading key — expect rejection")
}

async fn place_test_order_with_trading_key(
    signing_key: &SigningKey,
    account_id: &str,
) -> Result<String> {
    // POST /v3/orders with a far-from-market limit (e.g. BTC-PERP buy at $1).
    // Cancel immediately after.
    todo!("POST /v3/orders signed by trading key — expect success; then cancel")
}
```

- [ ] **Step 3: Build the probe**

Run: `cargo build -p m1-orderly-key-scope`
Expected: PASS (with todo!() warnings)

- [ ] **Step 4: Commit scaffold**

```bash
git add probes/m1-orderly-key-scope/
git commit -m "probe(m1): scaffold G1 trading-only key scope validation"
```

- [ ] **Step 5: Implement the three TODO functions**

Open `crates/xvision-execution/src/orderly.rs` and locate the existing EIP-712 onboarding flow. Copy the request-signing helper into `m1`, then implement:

- `register_trading_only_key`: `POST /v3/orderly_key` with payload `{ "permissions": ["trading"], "expiration": <unix+90d>, "publicKey": <pubkey> }`. Sign EIP-712 with `evm_key`.
- `attempt_withdrawal_with_trading_key`: `POST /v3/withdraw_request` with payload `{ "amount": "1", "token": "USDC", "chain_id": 5000 }`. Sign Ed25519 with `signing_key`.
- `place_test_order_with_trading_key`: `POST /v3/orders` with payload `{ "symbol": "PERP_BTC_USDC", "order_type": "LIMIT", "side": "BUY", "price": "1", "order_quantity": "0.001" }`. Sign Ed25519. On success, immediately `DELETE /v3/order/{id}` to cancel.

- [ ] **Step 6: Run the probe**

```bash
export ORDERLY_EVM_KEY=$(op read 'op://xvision/orderly-test/private-key')
export ORDERLY_ACCOUNT_ID=$(op read 'op://xvision/orderly-test/account-id')
cargo run -p m1-orderly-key-scope
```

Expected: log line `G1 validation PASSED — proceed with spec as designed`. If `G1 FAILED`, **halt the plan and escalate to operator**.

- [ ] **Step 7: Commit probe implementation + capture output**

```bash
mkdir -p probes/m1-orderly-key-scope/results
cargo run -p m1-orderly-key-scope 2>&1 | tee probes/m1-orderly-key-scope/results/$(date -u +%Y%m%dT%H%M%SZ).log
git add probes/m1-orderly-key-scope/
git commit -m "probe(m1): implement G1 validation; record run output"
```

### Task 0.3: G2 probe — Orderly isolated-margin support

**Files:**
- Create: `probes/m2-orderly-margin-modes/Cargo.toml`
- Create: `probes/m2-orderly-margin-modes/README.md`
- Create: `probes/m2-orderly-margin-modes/src/main.rs`

- [ ] **Step 1: Probe scaffold (mirror m1 structure)**

Create `probes/m2-orderly-margin-modes/Cargo.toml` (identical deps to m1).

Create `probes/m2-orderly-margin-modes/README.md`:

```markdown
# M2 — Orderly margin-mode support (Validation Gate G2)

**Goal:** Determine whether Orderly's perp accounts support per-position isolated margin in addition to the default cross-margin.

**Why:** Cross-margin lets one strategy's blow-up liquidate positions in another strategy's "safe" range. Isolated mode would eliminate this contagion at a capital-efficiency cost. The plan's Phase 5 design depends on the answer.

**Setup:** same as m1.

**Run:** `cargo run -p m2-orderly-margin-modes`

**Expected outcomes:**
- ISOLATED_SUPPORTED: spec's per-strategy margin_mode config ships in Phase 5 Task 5.1.
- CROSS_ONLY: spec's aggregate margin utilization rule ships in Phase 5 Task 5.2.

**Result:** record outcome in `decisions/0013-orderly-margin-mode-validation.md`.
```

- [ ] **Step 2: Probe implementation**

Create `probes/m2-orderly-margin-modes/src/main.rs`:

```rust
//! M2 — Validation Gate G2: isolated-margin support on Orderly perps.

use anyhow::{anyhow, Result};
use tracing::{info, warn};

#[derive(Debug)]
enum MarginModeResult {
    IsolatedSupported,
    CrossOnly,
    Unknown(String),
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    // Strategy: query GET /v3/account_info and inspect for margin_mode field
    // and any per-position margin settings. If the API exposes a
    // POST /v3/positions/{symbol}/margin_mode endpoint or equivalent, isolated is supported.

    let result = probe_margin_modes().await?;
    match result {
        MarginModeResult::IsolatedSupported => {
            info!("G2 result: ISOLATED_SUPPORTED → Phase 5 Task 5.1 (per-strategy margin_mode)");
        }
        MarginModeResult::CrossOnly => {
            warn!("G2 result: CROSS_ONLY → Phase 5 Task 5.2 (aggregate margin utilization rule)");
        }
        MarginModeResult::Unknown(reason) => {
            warn!(?reason, "G2 result: UNKNOWN — manual investigation required");
        }
    }
    Ok(())
}

async fn probe_margin_modes() -> Result<MarginModeResult> {
    // 1. GET /v3/public/info to fetch supported features.
    // 2. GET /v3/account_info (auth required) to inspect current margin_mode.
    // 3. Attempt POST /v3/positions/PERP_BTC_USDC/margin_mode with {"mode": "ISOLATED"}.
    //    - 200/204 → ISOLATED_SUPPORTED
    //    - 404 (endpoint missing) or 400 ("not supported") → CROSS_ONLY
    //    - other → Unknown
    todo!("implement Orderly margin-mode probe via the three calls above")
}
```

- [ ] **Step 3: Build, implement TODO, run, capture output**

Same pattern as Task 0.2 steps 3–7. Final command:

```bash
cargo run -p m2-orderly-margin-modes 2>&1 | tee probes/m2-orderly-margin-modes/results/$(date -u +%Y%m%dT%H%M%SZ).log
```

- [ ] **Step 4: Commit**

```bash
git add probes/m2-orderly-margin-modes/
git commit -m "probe(m2): G2 isolated-margin support validation"
```

### Task 0.4: Record both ADRs

**Files:**
- Create: `decisions/0012-orderly-key-scope-validation.md`
- Create: `decisions/0013-orderly-margin-mode-validation.md`

- [ ] **Step 1: ADR 0012**

Create `decisions/0012-orderly-key-scope-validation.md`:

```markdown
# ADR 0012 — Orderly trading-only key scope validation (G1)

**Status:** [ACCEPTED | REJECTED] (fill in based on m1 probe result)
**Date:** 2026-05-10
**Probe:** `probes/m1-orderly-key-scope/`

## Outcome

[Paste the final log line from the m1 probe run + the result file path.]

## Decision

[If ACCEPTED:]
Orderly supports a trading-only key scope. The non-custodial wallet design proceeds as specified in `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`. The dispatcher will use Ed25519 keys with `permissions: ["trading"]`.

[If REJECTED:]
Trading-only scope is not enforceable on Orderly. The spec's security model collapses. Halt this plan and trigger redesign — likely path is a smart-account wrapper or deposit-only-working-capital mode.

## Consequences

- All Phase 1–9 tasks below assume G1 = ACCEPTED.
- If G1 = REJECTED, this ADR supersedes the spec; brainstorming reopens.
```

- [ ] **Step 2: ADR 0013**

Create `decisions/0013-orderly-margin-mode-validation.md`:

```markdown
# ADR 0013 — Orderly margin-mode support (G2)

**Status:** [ISOLATED_SUPPORTED | CROSS_ONLY] (fill in based on m2 probe result)
**Date:** 2026-05-10
**Probe:** `probes/m2-orderly-margin-modes/`

## Outcome

[Paste the final log line from the m2 probe run.]

## Decision

[If ISOLATED_SUPPORTED:]
Phase 5 ships **Task 5.1 only** — per-strategy `margin_mode` config; default to "isolated" for new strategies. Phase 5 Task 5.2 (aggregate margin rule) is not needed.

[If CROSS_ONLY:]
Phase 5 ships **Task 5.2 only** — global `max_aggregate_margin_utilization` rule. Per-strategy `margin_mode` config is not exposed (no choice exists). Documented in spec §3.4 as "cross-margin contagion mitigation."

## Consequences

- Phase 5 task selection branches on this ADR.
- Spec §3.4 cross-margin paragraph is canonical regardless of outcome — this ADR records which branch we took.
```

- [ ] **Step 3: Fill in actual outcomes from probe runs**

Open both ADRs, replace `[ACCEPTED | REJECTED]` and `[ISOLATED_SUPPORTED | CROSS_ONLY]` with the real results, and paste log line snippets.

- [ ] **Step 4: Commit**

```bash
git add decisions/0012-orderly-key-scope-validation.md decisions/0013-orderly-margin-mode-validation.md
git commit -m "adr: 0012 orderly key-scope validation; 0013 margin-mode support"
```

**Phase 0 gate:** if ADR 0012 = REJECTED, do not proceed. Escalate to operator.

---

## Phase 1 — Schema + Ledger + Audit Log

Build the persistence layer that the rest of the plan writes to. End of phase: SQLite migrations apply cleanly, every Orderly trade currently flowing through the system is tagged with `agent_id` and persisted in `positions` + `decisions`.

### Task 1.1: SQLite migrations

**Files:**
- Create: `crates/xvision-data/src/migrations/20260510000001_positions.sql`
- Create: `crates/xvision-data/src/migrations/20260510000002_funding_attributions.sql`
- Create: `crates/xvision-data/src/migrations/20260510000003_decisions.sql`
- Create: `crates/xvision-data/src/migrations/20260510000004_strategy_status.sql`
- Create: `crates/xvision-data/src/migrations/20260510000005_pending_approvals.sql`
- Create: `crates/xvision-data/src/migrations/20260510000006_policy_changes.sql`
- Create: `crates/xvision-data/src/migrations/20260510000007_pending_reservations.sql`

- [ ] **Step 1: Add sqlx + ulid + sha2 to xvision-data Cargo.toml**

```toml
[dependencies]
# ...existing...
sqlx       = { workspace = true }
ulid       = { workspace = true }
sha2       = { workspace = true }
serde      = { workspace = true }
serde_json = { workspace = true }
chrono     = { workspace = true }
tracing    = { workspace = true }
thiserror  = { workspace = true }
anyhow     = { workspace = true }
```

- [ ] **Step 2: Write 001 — positions**

```sql
CREATE TABLE positions (
    position_id           TEXT PRIMARY KEY,
    client_order_id       TEXT NOT NULL UNIQUE,
    user_id               TEXT NOT NULL,
    agent_id           TEXT NOT NULL,
    asset                 TEXT NOT NULL,
    side                  TEXT NOT NULL CHECK (side IN ('LONG','SHORT')),
    size_usdc             REAL NOT NULL,
    entry_price           REAL,
    exit_price            REAL,
    realized_pnl_usdc     REAL,
    opened_at             INTEGER NOT NULL,
    closed_at             INTEGER,
    orderly_position_id   TEXT
);
CREATE INDEX idx_positions_strategy        ON positions(agent_id, closed_at);
CREATE INDEX idx_positions_open            ON positions(agent_id) WHERE closed_at IS NULL;
CREATE INDEX idx_positions_user            ON positions(user_id, closed_at);
```

- [ ] **Step 3: Write 002 — funding_attributions**

```sql
CREATE TABLE funding_attributions (
    funding_id        TEXT PRIMARY KEY,
    position_id       TEXT NOT NULL REFERENCES positions(position_id),
    agent_id       TEXT NOT NULL,
    asset             TEXT NOT NULL,
    funding_rate_bps  REAL NOT NULL,
    notional_usdc     REAL NOT NULL,
    payment_usdc      REAL NOT NULL,
    funded_at         INTEGER NOT NULL
);
CREATE INDEX idx_funding_strategy ON funding_attributions(agent_id, funded_at);
CREATE INDEX idx_funding_position ON funding_attributions(position_id);
```

- [ ] **Step 4: Write 003 — decisions (audit log)**

```sql
CREATE TABLE decisions (
    decision_id          TEXT PRIMARY KEY,
    occurred_at          INTEGER NOT NULL,
    user_id              TEXT NOT NULL,
    agent_id          TEXT NOT NULL,
    stage                TEXT NOT NULL CHECK (stage IN
                            ('emit','risk_eval','simulate','sign','submit',
                             'response','fill','close','cancel','reject')),
    related_position_id  TEXT,
    related_decision_id  TEXT,
    payload_json         TEXT NOT NULL,
    payload_sha256       TEXT NOT NULL,
    notes                TEXT
);
CREATE INDEX idx_decisions_strategy_time ON decisions(agent_id, occurred_at);
CREATE INDEX idx_decisions_position      ON decisions(related_position_id);
CREATE INDEX idx_decisions_chain         ON decisions(related_decision_id);
```

- [ ] **Step 5: Write 004 — strategy_status**

```sql
CREATE TABLE strategy_status (
    agent_id      TEXT PRIMARY KEY,
    state            TEXT NOT NULL CHECK (state IN
                        ('active','halted_auto','halted_manual')),
    halted_at        INTEGER,
    halt_reason      TEXT,
    halted_by        TEXT,
    last_unhalted_at INTEGER,
    last_unhalted_by TEXT
);
```

- [ ] **Step 6: Write 005 — pending_approvals**

```sql
CREATE TABLE pending_approvals (
    approval_id      TEXT PRIMARY KEY,
    requested_at     INTEGER NOT NULL,
    expires_at       INTEGER NOT NULL,
    user_id          TEXT NOT NULL,
    agent_id      TEXT NOT NULL,
    decision_payload TEXT NOT NULL,
    notional_usdc    REAL NOT NULL,
    state            TEXT NOT NULL CHECK (state IN ('pending','approved','rejected','expired'))
                        DEFAULT 'pending',
    resolved_at      INTEGER,
    resolved_by      TEXT
);
CREATE INDEX idx_approvals_pending ON pending_approvals(state, expires_at);
```

- [ ] **Step 7: Write 006 — policy_changes**

```sql
CREATE TABLE policy_changes (
    change_id        TEXT PRIMARY KEY,
    occurred_at      INTEGER NOT NULL,
    agent_id      TEXT NOT NULL,
    field            TEXT NOT NULL,
    old_value_json   TEXT,
    new_value_json   TEXT NOT NULL,
    changed_by       TEXT NOT NULL,
    comment          TEXT
);
CREATE INDEX idx_policy_strategy ON policy_changes(agent_id, occurred_at);
```

- [ ] **Step 8: Write 007 — pending_reservations**

```sql
CREATE TABLE pending_reservations (
    reservation_id   TEXT PRIMARY KEY,
    agent_id      TEXT NOT NULL,
    user_id          TEXT NOT NULL,
    notional_usdc    REAL NOT NULL,
    created_at       INTEGER NOT NULL,
    expires_at       INTEGER NOT NULL
);
CREATE INDEX idx_reservations_strategy_active
    ON pending_reservations(agent_id, expires_at);
```

- [ ] **Step 9: Verify migrations apply**

Add a smoke test in `crates/xvision-data/tests/migrations_test.rs`:

```rust
use sqlx::sqlite::SqlitePoolOptions;

#[tokio::test]
async fn migrations_apply_cleanly() {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect in-memory sqlite");

    sqlx::migrate!("./src/migrations")
        .run(&pool)
        .await
        .expect("migrations apply");

    // Sanity-check one table
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM positions")
        .fetch_one(&pool)
        .await
        .expect("query positions");
    assert_eq!(count.0, 0);
}
```

Run: `cargo test -p xvision-data migrations_apply_cleanly`
Expected: PASS

- [ ] **Step 10: Commit**

```bash
git add crates/xvision-data/
git commit -m "feat(data): add SQLite migrations for ledger + audit log + status + reservations"
```

### Task 1.2: Audit log writer (append-only, content-hashed)

**Files:**
- Create: `crates/xvision-data/src/audit.rs`
- Modify: `crates/xvision-data/src/lib.rs`
- Test: `crates/xvision-data/tests/audit_test.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-data/tests/audit_test.rs`:

```rust
use xvision_data::audit::{AuditLog, Stage};
use xvision_data::audit::testkit::pool;

#[tokio::test]
async fn writes_decision_and_returns_id() {
    let pool = pool().await;
    let log = AuditLog::new(pool.clone());

    let id = log
        .write(Stage::Emit, "user-1", "strat-1", None, None,
               serde_json::json!({"action": "open", "asset": "PERP_BTC_USDC"}),
               None)
        .await
        .expect("write");

    let row = log.get(&id).await.expect("get").expect("row exists");
    assert_eq!(row.user_id, "user-1");
    assert_eq!(row.agent_id, "strat-1");
    assert_eq!(row.stage, Stage::Emit);
    assert_eq!(row.payload_sha256.len(), 64); // hex sha256
}

#[tokio::test]
async fn payload_hash_is_deterministic() {
    let pool = pool().await;
    let log = AuditLog::new(pool.clone());
    let payload = serde_json::json!({"a": 1, "b": 2});

    let id1 = log.write(Stage::Emit, "u", "s", None, None, payload.clone(), None).await.unwrap();
    let id2 = log.write(Stage::Emit, "u", "s", None, None, payload, None).await.unwrap();
    let r1 = log.get(&id1).await.unwrap().unwrap();
    let r2 = log.get(&id2).await.unwrap().unwrap();
    assert_eq!(r1.payload_sha256, r2.payload_sha256);
    assert_ne!(r1.decision_id, r2.decision_id); // distinct rows, same payload
}

#[tokio::test]
async fn no_update_or_delete_methods_compile() {
    // This is a compile-time test by absence: AuditLog must not expose
    // update/delete. Documented here so any future addition fails review.
}
```

- [ ] **Step 2: Run, verify it fails (no module yet)**

Run: `cargo test -p xvision-data --test audit_test`
Expected: COMPILE FAIL

- [ ] **Step 3: Implement audit.rs**

Create `crates/xvision-data/src/audit.rs`:

```rust
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "lowercase")]
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

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct DecisionRow {
    pub decision_id: String,
    pub occurred_at: i64,
    pub user_id: String,
    pub agent_id: String,
    pub stage: Stage,
    pub related_position_id: Option<String>,
    pub related_decision_id: Option<String>,
    pub payload_json: String,
    pub payload_sha256: String,
    pub notes: Option<String>,
}

#[derive(Clone)]
pub struct AuditLog {
    pool: SqlitePool,
}

impl AuditLog {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Append-only. Never expose update/delete.
    pub async fn write(
        &self,
        stage: Stage,
        user_id: &str,
        agent_id: &str,
        related_position_id: Option<&str>,
        related_decision_id: Option<&str>,
        payload: serde_json::Value,
        notes: Option<&str>,
    ) -> Result<String> {
        let decision_id = Ulid::new().to_string();
        let occurred_at = Utc::now().timestamp_millis();
        let payload_json = serde_json::to_string(&payload)?;
        let payload_sha256 = hex::encode(Sha256::digest(payload_json.as_bytes()));

        sqlx::query(
            r#"INSERT INTO decisions
               (decision_id, occurred_at, user_id, agent_id, stage,
                related_position_id, related_decision_id,
                payload_json, payload_sha256, notes)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&decision_id)
        .bind(occurred_at)
        .bind(user_id)
        .bind(agent_id)
        .bind(stage)
        .bind(related_position_id)
        .bind(related_decision_id)
        .bind(&payload_json)
        .bind(&payload_sha256)
        .bind(notes)
        .execute(&self.pool)
        .await?;

        Ok(decision_id)
    }

    pub async fn get(&self, id: &str) -> Result<Option<DecisionRow>> {
        let row = sqlx::query_as::<_, DecisionRow>(
            "SELECT * FROM decisions WHERE decision_id = ?",
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row)
    }

    pub async fn for_position(&self, position_id: &str) -> Result<Vec<DecisionRow>> {
        let rows = sqlx::query_as::<_, DecisionRow>(
            "SELECT * FROM decisions WHERE related_position_id = ? ORDER BY occurred_at ASC",
        )
        .bind(position_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }
}

#[cfg(test)]
pub mod testkit {
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::SqlitePool;

    pub async fn pool() -> SqlitePool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("./src/migrations").run(&pool).await.unwrap();
        pool
    }
}
```

Add `hex = "0.4"` to xvision-data deps (one-line cargo edit).

- [ ] **Step 4: Wire module into lib.rs**

Edit `crates/xvision-data/src/lib.rs`, add:

```rust
pub mod audit;
```

- [ ] **Step 5: Run tests**

Run: `cargo test -p xvision-data --test audit_test`
Expected: PASS (3 tests)

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-data/
git commit -m "feat(data): append-only audit log writer with content hashing"
```

### Task 1.3: Ledger (positions + funding) read/write helpers

**Files:**
- Create: `crates/xvision-data/src/ledger.rs`
- Modify: `crates/xvision-data/src/lib.rs`
- Test: `crates/xvision-data/tests/ledger_test.rs`

- [ ] **Step 1: Write the failing test**

Create `crates/xvision-data/tests/ledger_test.rs`:

```rust
use xvision_data::audit::testkit::pool;
use xvision_data::ledger::{Ledger, Position, Side};

#[tokio::test]
async fn open_close_round_trip() {
    let pool = pool().await;
    let ledger = Ledger::new(pool.clone());

    let pos = ledger
        .open_position(Position::new(
            "user-1", "strat-1", "PERP_BTC_USDC", Side::Long, 1000.0,
        ))
        .await
        .expect("open");

    ledger.record_fill(&pos.position_id, 78_000.0, Some("orderly-pos-1"))
        .await
        .expect("record fill");

    ledger.close_position(&pos.position_id, 79_000.0, 12.5)
        .await
        .expect("close");

    let open_count = ledger.open_count_for_strategy("strat-1").await.unwrap();
    assert_eq!(open_count, 0);

    let in_flight = ledger.in_flight_notional("strat-1").await.unwrap();
    assert_eq!(in_flight, 0.0);
}

#[tokio::test]
async fn in_flight_notional_sums_open_positions_only() {
    let pool = pool().await;
    let ledger = Ledger::new(pool.clone());

    let p1 = ledger.open_position(Position::new("u","s","PERP_BTC_USDC",Side::Long, 1000.0)).await.unwrap();
    let _p2 = ledger.open_position(Position::new("u","s","PERP_BTC_USDC",Side::Short, 500.0)).await.unwrap();

    let n = ledger.in_flight_notional("s").await.unwrap();
    assert_eq!(n, 1500.0);

    ledger.close_position(&p1.position_id, 0.0, 0.0).await.unwrap();
    let n = ledger.in_flight_notional("s").await.unwrap();
    assert_eq!(n, 500.0);
}
```

- [ ] **Step 2: Run, verify it fails**

Run: `cargo test -p xvision-data --test ledger_test`
Expected: COMPILE FAIL

- [ ] **Step 3: Implement ledger.rs**

Create `crates/xvision-data/src/ledger.rs`:

```rust
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "UPPERCASE")]
pub enum Side { Long, Short }

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Position {
    pub position_id: String,
    pub client_order_id: String,
    pub user_id: String,
    pub agent_id: String,
    pub asset: String,
    pub side: Side,
    pub size_usdc: f64,
    pub entry_price: Option<f64>,
    pub exit_price: Option<f64>,
    pub realized_pnl_usdc: Option<f64>,
    pub opened_at: i64,
    pub closed_at: Option<i64>,
    pub orderly_position_id: Option<String>,
}

impl Position {
    pub fn new(user_id: &str, agent_id: &str, asset: &str, side: Side, size_usdc: f64) -> Self {
        let id = Ulid::new().to_string();
        Self {
            position_id: id.clone(),
            client_order_id: id, // ULID doubles as client_order_id for idempotency
            user_id: user_id.into(),
            agent_id: agent_id.into(),
            asset: asset.into(),
            side,
            size_usdc,
            entry_price: None,
            exit_price: None,
            realized_pnl_usdc: None,
            opened_at: Utc::now().timestamp_millis(),
            closed_at: None,
            orderly_position_id: None,
        }
    }
}

#[derive(Clone)]
pub struct Ledger { pool: SqlitePool }

impl Ledger {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    pub async fn open_position(&self, p: Position) -> Result<Position> {
        sqlx::query(
            r#"INSERT INTO positions
               (position_id, client_order_id, user_id, agent_id, asset, side, size_usdc, opened_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&p.position_id).bind(&p.client_order_id).bind(&p.user_id)
        .bind(&p.agent_id).bind(&p.asset).bind(p.side).bind(p.size_usdc)
        .bind(p.opened_at)
        .execute(&self.pool).await?;
        Ok(p)
    }

    pub async fn record_fill(&self, position_id: &str, entry_price: f64, orderly_id: Option<&str>) -> Result<()> {
        sqlx::query(
            "UPDATE positions SET entry_price = ?, orderly_position_id = ? WHERE position_id = ?",
        )
        .bind(entry_price).bind(orderly_id).bind(position_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn close_position(&self, position_id: &str, exit_price: f64, realized_pnl_usdc: f64) -> Result<()> {
        sqlx::query(
            r#"UPDATE positions
               SET exit_price = ?, realized_pnl_usdc = ?, closed_at = ?
               WHERE position_id = ?"#,
        )
        .bind(exit_price).bind(realized_pnl_usdc)
        .bind(Utc::now().timestamp_millis()).bind(position_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn in_flight_notional(&self, agent_id: &str) -> Result<f64> {
        let row: (Option<f64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(size_usdc), 0.0) FROM positions
             WHERE agent_id = ? AND closed_at IS NULL",
        )
        .bind(agent_id)
        .fetch_one(&self.pool).await?;
        Ok(row.0.unwrap_or(0.0))
    }

    pub async fn open_count_for_strategy(&self, agent_id: &str) -> Result<i64> {
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM positions WHERE agent_id = ? AND closed_at IS NULL",
        )
        .bind(agent_id)
        .fetch_one(&self.pool).await?;
        Ok(row.0)
    }

    pub async fn realized_pnl_window(&self, agent_id: &str, since_ms: i64) -> Result<f64> {
        let row: (Option<f64>,) = sqlx::query_as(
            "SELECT COALESCE(SUM(realized_pnl_usdc), 0.0) FROM positions
             WHERE agent_id = ? AND closed_at >= ?",
        )
        .bind(agent_id).bind(since_ms)
        .fetch_one(&self.pool).await?;
        Ok(row.0.unwrap_or(0.0))
    }
}
```

- [ ] **Step 4: Wire and test**

Edit `crates/xvision-data/src/lib.rs`, add `pub mod ledger;`.

Run: `cargo test -p xvision-data --test ledger_test`
Expected: PASS (2 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-data/
git commit -m "feat(data): position ledger with in-flight notional + PnL window helpers"
```

### Task 1.4: Strategy status, policy_changes, pending_approvals, pending_reservations modules

**Files:**
- Create: `crates/xvision-data/src/status.rs`, `policy.rs`, `pending.rs`
- Modify: `crates/xvision-data/src/lib.rs`
- Test: `crates/xvision-data/tests/status_test.rs`, `policy_test.rs`, `pending_test.rs`

- [ ] **Step 1: Write status module**

Create `crates/xvision-data/src/status.rs`:

```rust
use anyhow::Result;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
pub enum State { Active, HaltedAuto, HaltedManual }

#[derive(Clone)]
pub struct StatusStore { pool: SqlitePool }

impl StatusStore {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    pub async fn get_or_init(&self, agent_id: &str) -> Result<State> {
        let row: Option<(State,)> = sqlx::query_as(
            "SELECT state FROM strategy_status WHERE agent_id = ?",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool).await?;

        if let Some((s,)) = row { return Ok(s); }

        sqlx::query("INSERT INTO strategy_status (agent_id, state) VALUES (?, 'active')")
            .bind(agent_id).execute(&self.pool).await?;
        Ok(State::Active)
    }

    pub async fn halt(&self, agent_id: &str, reason: &str, by: &str, automatic: bool) -> Result<()> {
        let new_state = if automatic { State::HaltedAuto } else { State::HaltedManual };
        sqlx::query(
            r#"INSERT INTO strategy_status (agent_id, state, halted_at, halt_reason, halted_by)
               VALUES (?, ?, ?, ?, ?)
               ON CONFLICT(agent_id) DO UPDATE SET
                  state = excluded.state, halted_at = excluded.halted_at,
                  halt_reason = excluded.halt_reason, halted_by = excluded.halted_by"#,
        )
        .bind(agent_id).bind(new_state)
        .bind(Utc::now().timestamp_millis()).bind(reason).bind(by)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn unhalt(&self, agent_id: &str, by: &str) -> Result<()> {
        sqlx::query(
            r#"UPDATE strategy_status
               SET state = 'active', last_unhalted_at = ?, last_unhalted_by = ?
               WHERE agent_id = ?"#,
        )
        .bind(Utc::now().timestamp_millis()).bind(by).bind(agent_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn is_active(&self, agent_id: &str) -> Result<bool> {
        Ok(matches!(self.get_or_init(agent_id).await?, State::Active))
    }
}
```

Test in `crates/xvision-data/tests/status_test.rs`:

```rust
use xvision_data::audit::testkit::pool;
use xvision_data::status::{State, StatusStore};

#[tokio::test]
async fn lifecycle_active_halted_unhalted() {
    let pool = pool().await;
    let s = StatusStore::new(pool);
    assert_eq!(s.get_or_init("s1").await.unwrap(), State::Active);

    s.halt("s1", "consecutive losses", "auto-trigger", true).await.unwrap();
    assert_eq!(s.get_or_init("s1").await.unwrap(), State::HaltedAuto);
    assert!(!s.is_active("s1").await.unwrap());

    s.unhalt("s1", "operator").await.unwrap();
    assert!(s.is_active("s1").await.unwrap());
}
```

- [ ] **Step 2: Write policy module**

Create `crates/xvision-data/src/policy.rs`:

```rust
use anyhow::Result;
use chrono::Utc;
use serde::Serialize;
use sqlx::SqlitePool;
use ulid::Ulid;

#[derive(Clone)]
pub struct PolicyJournal { pool: SqlitePool }

impl PolicyJournal {
    pub fn new(pool: SqlitePool) -> Self { Self { pool } }

    pub async fn record<T: Serialize, U: Serialize>(
        &self, agent_id: &str, field: &str,
        old: Option<&T>, new: &U, changed_by: &str, comment: Option<&str>,
    ) -> Result<String> {
        let id = Ulid::new().to_string();
        let old_json = match old {
            Some(v) => Some(serde_json::to_string(v)?),
            None => None,
        };
        let new_json = serde_json::to_string(new)?;
        sqlx::query(
            r#"INSERT INTO policy_changes
               (change_id, occurred_at, agent_id, field,
                old_value_json, new_value_json, changed_by, comment)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id).bind(Utc::now().timestamp_millis()).bind(agent_id).bind(field)
        .bind(old_json).bind(new_json).bind(changed_by).bind(comment)
        .execute(&self.pool).await?;
        Ok(id)
    }
}
```

Test minimally in `crates/xvision-data/tests/policy_test.rs` (one round-trip).

- [ ] **Step 3: Write pending module (approvals + reservations)**

Create `crates/xvision-data/src/pending.rs` covering both `pending_approvals` and `pending_reservations` with: insert, list-not-expired, mark-resolved (approvals), and reap-expired (reservations). Use the same `Ulid::new()` + `Utc::now().timestamp_millis()` patterns. Tests verify TTL semantics.

(Implementation: ~120 lines; mirror status.rs and policy.rs structure. Functions: `request_approval`, `list_pending_approvals`, `resolve_approval`, `reserve`, `list_active_reservations`, `release_reservation`, `reap_expired_reservations`.)

- [ ] **Step 4: Wire all three modules into lib.rs**

Edit `crates/xvision-data/src/lib.rs`:

```rust
pub mod audit;
pub mod ledger;
pub mod status;
pub mod policy;
pub mod pending;
```

- [ ] **Step 5: Run all xvision-data tests**

Run: `cargo test -p xvision-data`
Expected: PASS (all tests)

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-data/
git commit -m "feat(data): strategy status, policy journal, pending approvals + reservations"
```

### Task 1.5: Integrate ledger into existing Orderly executor (tag every trade)

**Files:**
- Modify: `crates/xvision-execution/src/orderly.rs`
- Test: `crates/xvision-execution/tests/orderly_ledger_integration.rs`

- [ ] **Step 1: Identify the existing trade-emitting call site**

Read `crates/xvision-execution/src/orderly.rs` and locate the function that submits an order to Orderly. (Likely named `submit_order` or similar; uses `client_order_id = cycle_id.to_string()` per the survey.)

- [ ] **Step 2: Pass agent_id + ledger into the executor**

Modify the executor's constructor to accept `Arc<xvision_data::ledger::Ledger>` and `Arc<xvision_data::audit::AuditLog>`. Modify `submit_order` to take an explicit `agent_id: &str` parameter.

After successful submission, call `ledger.open_position(...)` and `audit.write(Stage::Submit, ...)`. After fill confirmation, call `ledger.record_fill(...)` and `audit.write(Stage::Fill, ...)`.

- [ ] **Step 3: Update existing call sites**

`grep -rn "submit_order\|OrderlyExecutor" crates/` — every call site must now pass a `agent_id`. For the current single-strategy fixture, hardcode `"hackathon-baseline"` as `agent_id` until Phase 3 wires per-strategy dispatch.

- [ ] **Step 4: Integration test**

```rust
// crates/xvision-execution/tests/orderly_ledger_integration.rs
// Mock the Orderly HTTP layer; assert that on submit_order success,
// a positions row appears with the supplied agent_id and a decisions
// row appears with stage=submit.
```

- [ ] **Step 5: Run, verify pass**

Run: `cargo test -p xvision-execution --test orderly_ledger_integration`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-execution/
git commit -m "feat(execution): tag every Orderly order with agent_id; persist to ledger + audit"
```

---

## Phase 2 — Per-Strategy Rules + Reservations

End of phase: `xvision-risk` has per-strategy rules wired in, the Risk Engine evaluates them in the existing `RiskDecision` flow, and a reservation pattern prevents two concurrent trades from collectively exceeding a strategy's hard cap.

### Task 2.1: Per-strategy config schema

**Files:**
- Modify: `crates/xvision-risk/src/config.rs`
- Test: `crates/xvision-risk/tests/config_test.rs`

- [ ] **Step 1: Failing test for TOML round-trip**

```rust
// crates/xvision-risk/tests/config_test.rs
use xvision_risk::config::{StrategyConfig, parse_strategies_toml};

#[test]
fn parses_full_strategy_config() {
    let toml = r#"
        [strategies.btc-momentum-v3]
        hard_cap_usdc_notional        = 5000
        hard_cap_open_positions       = 2
        hard_cap_daily_loss_usdc      = 250
        allowed_chains                = ["mantle"]
        allowed_protocols             = ["orderly_perp_v3"]
        allowed_assets                = ["PERP_BTC_USDC"]
        max_slippage_bps              = 50
        max_orders_per_minute         = 10
        max_orders_per_hour           = 100
        active_hours_utc              = "00:00-24:00"
        require_manual_approval_above = 2500
    "#;
    let cfg = parse_strategies_toml(toml).expect("parse");
    let s = cfg.get("btc-momentum-v3").expect("strategy present");
    assert_eq!(s.hard_cap_usdc_notional, 5000.0);
    assert_eq!(s.allowed_assets, vec!["PERP_BTC_USDC".to_string()]);
    assert_eq!(s.max_slippage_bps, 50);
}
```

- [ ] **Step 2: Implement `StrategyConfig`**

Edit `crates/xvision-risk/src/config.rs`, add:

```rust
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyConfig {
    pub hard_cap_usdc_notional: f64,
    pub hard_cap_open_positions: u32,
    pub hard_cap_daily_loss_usdc: f64,
    pub allowed_chains: Vec<String>,
    pub allowed_protocols: Vec<String>,
    pub allowed_assets: Vec<String>,
    pub max_slippage_bps: u32,
    pub max_orders_per_minute: u32,
    pub max_orders_per_hour: u32,
    /// "HH:MM-HH:MM" UTC; "00:00-24:00" = always active.
    pub active_hours_utc: String,
    pub require_manual_approval_above: f64,
    /// Optional per-strategy margin mode override (only meaningful if G2 = ISOLATED_SUPPORTED).
    #[serde(default)]
    pub margin_mode: Option<MarginMode>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MarginMode { Isolated, Cross }

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StrategyConfigSet {
    #[serde(default)]
    pub strategies: HashMap<String, StrategyConfig>,
}

impl StrategyConfigSet {
    pub fn get(&self, id: &str) -> Option<&StrategyConfig> { self.strategies.get(id) }
}

pub fn parse_strategies_toml(s: &str) -> anyhow::Result<StrategyConfigSet> {
    Ok(toml::from_str(s)?)
}
```

- [ ] **Step 3: Run test, verify pass**

Run: `cargo test -p xvision-risk --test config_test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-risk/
git commit -m "feat(risk): per-strategy config schema (hard caps + scoped permissions)"
```

### Task 2.2: Per-strategy rules

**Files:**
- Create: `crates/xvision-risk/src/rules/per_strategy.rs`
- Modify: `crates/xvision-risk/src/rules/mod.rs`
- Test: `crates/xvision-risk/tests/per_strategy_rules_test.rs`

- [ ] **Step 1: Write the failing tests covering each rule**

```rust
// crates/xvision-risk/tests/per_strategy_rules_test.rs
use xvision_risk::config::StrategyConfig;
use xvision_risk::rules::per_strategy::{PerStrategyEvaluator, EvaluationInput, Verdict};

fn cfg() -> StrategyConfig {
    StrategyConfig {
        hard_cap_usdc_notional: 5000.0,
        hard_cap_open_positions: 2,
        hard_cap_daily_loss_usdc: 250.0,
        allowed_chains: vec!["mantle".into()],
        allowed_protocols: vec!["orderly_perp_v3".into()],
        allowed_assets: vec!["PERP_BTC_USDC".into()],
        max_slippage_bps: 50,
        max_orders_per_minute: 10,
        max_orders_per_hour: 100,
        active_hours_utc: "00:00-24:00".into(),
        require_manual_approval_above: 2500.0,
        margin_mode: None,
    }
}

#[test]
fn rejects_disallowed_asset() {
    let v = PerStrategyEvaluator::new(cfg()).evaluate(EvaluationInput {
        intended_asset: "PERP_ETH_USDC".into(),
        intended_notional: 100.0,
        in_flight_notional: 0.0,
        open_positions: 0,
        realized_loss_today_usdc: 0.0,
        orders_in_last_minute: 0,
        orders_in_last_hour: 0,
        now_utc_hh_mm: "12:00".into(),
        chain: "mantle".into(),
        protocol: "orderly_perp_v3".into(),
    });
    assert!(matches!(v, Verdict::Vetoed { reason } if reason.contains("asset not allowed")));
}

// (...one test per scoped permission: chain, protocol, asset, slippage handled in
//  Phase 3, frequency caps, active hours, hard cap, position cap, daily loss,
//  approval threshold)
```

(Add tests for: disallowed_chain, disallowed_protocol, hard_cap_exceeded, position_cap_exceeded, daily_loss_exceeded, frequency_minute_exceeded, frequency_hour_exceeded, outside_active_hours, requires_approval_above_threshold, fully_approved.)

- [ ] **Step 2: Implement evaluator**

Create `crates/xvision-risk/src/rules/per_strategy.rs`:

```rust
use crate::config::StrategyConfig;

#[derive(Debug, Clone)]
pub struct EvaluationInput {
    pub intended_asset: String,
    pub intended_notional: f64,
    pub in_flight_notional: f64,
    pub open_positions: u32,
    pub realized_loss_today_usdc: f64, // positive = loss
    pub orders_in_last_minute: u32,
    pub orders_in_last_hour: u32,
    pub now_utc_hh_mm: String, // "HH:MM"
    pub chain: String,
    pub protocol: String,
}

#[derive(Debug, Clone)]
pub enum Verdict {
    Approved,
    RequiresApproval { threshold: f64 },
    Vetoed { reason: String },
}

pub struct PerStrategyEvaluator { cfg: StrategyConfig }

impl PerStrategyEvaluator {
    pub fn new(cfg: StrategyConfig) -> Self { Self { cfg } }

    pub fn evaluate(&self, i: EvaluationInput) -> Verdict {
        if !self.cfg.allowed_chains.iter().any(|c| c == &i.chain) {
            return Verdict::Vetoed { reason: format!("chain not allowed: {}", i.chain) };
        }
        if !self.cfg.allowed_protocols.iter().any(|p| p == &i.protocol) {
            return Verdict::Vetoed { reason: format!("protocol not allowed: {}", i.protocol) };
        }
        if !self.cfg.allowed_assets.iter().any(|a| a == &i.intended_asset) {
            return Verdict::Vetoed { reason: format!("asset not allowed: {}", i.intended_asset) };
        }
        if i.in_flight_notional + i.intended_notional > self.cfg.hard_cap_usdc_notional {
            return Verdict::Vetoed {
                reason: format!("hard cap exceeded: in_flight {} + intended {} > cap {}",
                    i.in_flight_notional, i.intended_notional, self.cfg.hard_cap_usdc_notional),
            };
        }
        if i.open_positions + 1 > self.cfg.hard_cap_open_positions {
            return Verdict::Vetoed {
                reason: format!("open positions exceeded: would be {} > cap {}",
                    i.open_positions + 1, self.cfg.hard_cap_open_positions),
            };
        }
        if i.realized_loss_today_usdc >= self.cfg.hard_cap_daily_loss_usdc {
            return Verdict::Vetoed {
                reason: format!("daily loss kill: {} >= {}",
                    i.realized_loss_today_usdc, self.cfg.hard_cap_daily_loss_usdc),
            };
        }
        if i.orders_in_last_minute >= self.cfg.max_orders_per_minute {
            return Verdict::Vetoed { reason: "frequency cap (minute) exceeded".into() };
        }
        if i.orders_in_last_hour >= self.cfg.max_orders_per_hour {
            return Verdict::Vetoed { reason: "frequency cap (hour) exceeded".into() };
        }
        if !is_within_active_hours(&i.now_utc_hh_mm, &self.cfg.active_hours_utc) {
            return Verdict::Vetoed { reason: "outside active hours".into() };
        }
        if i.intended_notional > self.cfg.require_manual_approval_above {
            return Verdict::RequiresApproval { threshold: self.cfg.require_manual_approval_above };
        }
        Verdict::Approved
    }
}

/// "HH:MM" within "HH:MM-HH:MM" inclusive. "00:00-24:00" = always.
fn is_within_active_hours(now: &str, window: &str) -> bool {
    if window == "00:00-24:00" { return true; }
    let parts: Vec<&str> = window.splitn(2, '-').collect();
    if parts.len() != 2 { return true; } // malformed → fail open with warning at config-load
    let to_minutes = |s: &str| -> Option<u32> {
        let mut p = s.splitn(2, ':');
        let h = p.next()?.parse::<u32>().ok()?;
        let m = p.next()?.parse::<u32>().ok()?;
        Some(h * 60 + m)
    };
    let n = to_minutes(now); let s = to_minutes(parts[0]); let e = to_minutes(parts[1]);
    match (n, s, e) {
        (Some(n), Some(s), Some(e)) if s <= e => n >= s && n <= e,
        (Some(n), Some(s), Some(e)) => n >= s || n <= e, // wraps midnight
        _ => true,
    }
}
```

- [ ] **Step 3: Wire into rules/mod.rs**

```rust
pub mod per_strategy;
```

- [ ] **Step 4: Run, verify all 11 tests pass**

Run: `cargo test -p xvision-risk --test per_strategy_rules_test`
Expected: PASS (11 tests)

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-risk/
git commit -m "feat(risk): per-strategy rule evaluator with full scoped-permissions matrix"
```

### Task 2.3: Reservation table + write-locking

**Files:**
- Create: `crates/xvision-risk/src/reservations.rs`
- Modify: `crates/xvision-risk/src/lib.rs`
- Test: `crates/xvision-risk/tests/reservations_test.rs`

- [ ] **Step 1: Failing concurrency test**

```rust
// crates/xvision-risk/tests/reservations_test.rs
use std::sync::Arc;
use xvision_data::audit::testkit::pool;
use xvision_data::ledger::Ledger;
use xvision_risk::reservations::ReservationManager;

#[tokio::test]
async fn concurrent_reservations_respect_cap() {
    let pool = pool().await;
    let ledger = Arc::new(Ledger::new(pool.clone()));
    let mgr = Arc::new(ReservationManager::new(pool.clone(), ledger.clone()));

    // Cap = $1000. Two concurrent attempts to reserve $700 each → only one succeeds.
    let mgr1 = mgr.clone();
    let mgr2 = mgr.clone();
    let h1 = tokio::spawn(async move { mgr1.try_reserve("u", "s", 700.0, 1000.0).await });
    let h2 = tokio::spawn(async move { mgr2.try_reserve("u", "s", 700.0, 1000.0).await });
    let r1 = h1.await.unwrap();
    let r2 = h2.await.unwrap();
    assert!(r1.is_ok() ^ r2.is_ok(), "exactly one of two concurrent over-cap reservations must fail");
}

#[tokio::test]
async fn expired_reservations_are_reaped() {
    let pool = pool().await;
    let ledger = Arc::new(Ledger::new(pool.clone()));
    let mgr = ReservationManager::new_with_ttl(pool.clone(), ledger.clone(), std::time::Duration::from_millis(50));
    let r = mgr.try_reserve("u", "s", 100.0, 1000.0).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    let reaped = mgr.reap_expired().await.unwrap();
    assert_eq!(reaped, 1);
    // Now the cap is fully available again.
    let r2 = mgr.try_reserve("u", "s", 1000.0, 1000.0).await;
    assert!(r2.is_ok());
}
```

- [ ] **Step 2: Implement ReservationManager**

```rust
// crates/xvision-risk/src/reservations.rs
use anyhow::{anyhow, Result};
use chrono::Utc;
use sqlx::SqlitePool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use ulid::Ulid;
use xvision_data::ledger::Ledger;

const DEFAULT_TTL: Duration = Duration::from_secs(30);

pub struct ReservationManager {
    pool: SqlitePool,
    ledger: Arc<Ledger>,
    ttl: Duration,
    /// Per-strategy lock to serialize cap-check + insert atomically at the
    /// process level. SQLite's transaction is the durable cross-process guarantee.
    locks: Mutex<std::collections::HashMap<String, Arc<Mutex<()>>>>,
}

impl ReservationManager {
    pub fn new(pool: SqlitePool, ledger: Arc<Ledger>) -> Self {
        Self::new_with_ttl(pool, ledger, DEFAULT_TTL)
    }

    pub fn new_with_ttl(pool: SqlitePool, ledger: Arc<Ledger>, ttl: Duration) -> Self {
        Self { pool, ledger, ttl, locks: Mutex::new(Default::default()) }
    }

    async fn lock_for(&self, agent_id: &str) -> Arc<Mutex<()>> {
        let mut map = self.locks.lock().await;
        map.entry(agent_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Reserve `notional` for `agent_id` if `in_flight + already_reserved + notional <= cap`.
    pub async fn try_reserve(&self, user_id: &str, agent_id: &str, notional: f64, cap: f64) -> Result<String> {
        let lock = self.lock_for(agent_id).await;
        let _guard = lock.lock().await;

        let mut tx = self.pool.begin().await?;
        let in_flight = self.ledger.in_flight_notional(agent_id).await?;
        let now_ms = Utc::now().timestamp_millis();
        let reserved: (Option<f64>,) = sqlx::query_as(
            r#"SELECT COALESCE(SUM(notional_usdc), 0.0) FROM pending_reservations
               WHERE agent_id = ? AND expires_at > ?"#,
        )
        .bind(agent_id).bind(now_ms)
        .fetch_one(&mut *tx).await?;

        if in_flight + reserved.0.unwrap_or(0.0) + notional > cap {
            return Err(anyhow!("would exceed cap: in_flight {} + reserved {} + intended {} > cap {}",
                in_flight, reserved.0.unwrap_or(0.0), notional, cap));
        }

        let id = Ulid::new().to_string();
        let expires_at = now_ms + self.ttl.as_millis() as i64;
        sqlx::query(
            r#"INSERT INTO pending_reservations
               (reservation_id, agent_id, user_id, notional_usdc, created_at, expires_at)
               VALUES (?, ?, ?, ?, ?, ?)"#,
        )
        .bind(&id).bind(agent_id).bind(user_id).bind(notional)
        .bind(now_ms).bind(expires_at)
        .execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn release(&self, reservation_id: &str) -> Result<()> {
        sqlx::query("DELETE FROM pending_reservations WHERE reservation_id = ?")
            .bind(reservation_id).execute(&self.pool).await?;
        Ok(())
    }

    pub async fn reap_expired(&self) -> Result<u64> {
        let now_ms = Utc::now().timestamp_millis();
        let res = sqlx::query("DELETE FROM pending_reservations WHERE expires_at <= ?")
            .bind(now_ms).execute(&self.pool).await?;
        Ok(res.rows_affected())
    }
}
```

- [ ] **Step 3: Wire and test**

Edit `crates/xvision-risk/src/lib.rs`, add `pub mod reservations;`.

Run: `cargo test -p xvision-risk --test reservations_test`
Expected: PASS (2 tests)

- [ ] **Step 4: Commit**

```bash
git add crates/xvision-risk/
git commit -m "feat(risk): race-free reservation pattern for per-strategy cap enforcement"
```

---

## Phase 3 — Dispatcher + Pre-Trade Simulation

End of phase: every order goes through `OrderDispatcher`, which gates → reserves → simulates → signs → submits → audits, with the existing `OrderlyExecutor` doing only the network I/O.

### Task 3.1: Pre-trade simulation wrapper

**Files:**
- Create: `crates/xvision-execution/src/simulate.rs`
- Modify: `crates/xvision-execution/src/orderly.rs` (expose order-info call)
- Test: `crates/xvision-execution/tests/simulate_test.rs`

- [ ] **Step 1: Failing test using a mock orderbook fetcher**

```rust
// crates/xvision-execution/tests/simulate_test.rs
use xvision_execution::simulate::{Simulator, SimulationResult, OrderbookSnapshot};

#[tokio::test]
async fn estimates_slippage_against_static_book() {
    let book = OrderbookSnapshot {
        bids: vec![(78_000.0, 0.5), (77_990.0, 1.0)],
        asks: vec![(78_010.0, 0.3), (78_050.0, 1.0)],
    };
    let sim = Simulator::with_book(book);
    let r = sim.simulate("PERP_BTC_USDC", "BUY", 0.4).await.unwrap();
    // 0.3 BTC at 78010 + 0.1 BTC at 78050 → VWAP 78020
    // mid = (78000 + 78010) / 2 = 78005
    // slippage_bps = (78020 - 78005) / 78005 * 10_000 ≈ 1.92
    assert!(r.estimated_slippage_bps > 1.0 && r.estimated_slippage_bps < 3.0);
    assert_eq!(r.estimated_fill_price.round(), 78020.0);
}
```

- [ ] **Step 2: Implement `Simulator`**

```rust
// crates/xvision-execution/src/simulate.rs
use anyhow::Result;

#[derive(Debug, Clone)]
pub struct OrderbookSnapshot {
    pub bids: Vec<(f64, f64)>, // (price, qty), descending by price
    pub asks: Vec<(f64, f64)>, // (price, qty), ascending by price
}

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub estimated_fill_price: f64,
    pub estimated_slippage_bps: f64,
    pub fees_usdc: f64,
}

pub struct Simulator {
    /// In tests, supply a static book. In prod, fetch from Orderly per call.
    static_book: Option<OrderbookSnapshot>,
    // TODO: prod variant takes an Arc to an Orderly client
}

impl Simulator {
    pub fn with_book(book: OrderbookSnapshot) -> Self {
        Self { static_book: Some(book) }
    }

    pub async fn simulate(&self, asset: &str, side: &str, qty: f64) -> Result<SimulationResult> {
        let book = self.static_book.as_ref().ok_or_else(|| anyhow::anyhow!("no book"))?;
        let mid = (book.bids[0].0 + book.asks[0].0) / 2.0;
        let levels: &[(f64, f64)] = match side {
            "BUY" => &book.asks,
            "SELL" => &book.bids,
            _ => return Err(anyhow::anyhow!("invalid side {side}")),
        };
        let mut remaining = qty;
        let mut cost = 0.0;
        for (price, available) in levels {
            let take = remaining.min(*available);
            cost += take * price;
            remaining -= take;
            if remaining <= 0.0 { break; }
        }
        if remaining > 0.0 {
            return Err(anyhow::anyhow!("insufficient liquidity: {} remaining", remaining));
        }
        let fill = cost / qty;
        let slippage_bps = ((fill - mid).abs() / mid) * 10_000.0;
        let fees_usdc = cost * 0.0005; // 5 bps placeholder; replace with Orderly's actual fee schedule
        Ok(SimulationResult { estimated_fill_price: fill, estimated_slippage_bps: slippage_bps, fees_usdc })
    }
}
```

- [ ] **Step 3: Add a `from_orderly` constructor that fetches the live book**

(Body: call `GET /v3/orderbook/{symbol}` via the existing `orderly.rs` HTTP client; map response to `OrderbookSnapshot`. This is mechanical wrapping; ~30 lines.)

- [ ] **Step 4: Run test, verify pass**

Run: `cargo test -p xvision-execution --test simulate_test`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-execution/
git commit -m "feat(execution): pre-trade simulation against orderbook snapshots"
```

### Task 3.2: OrderDispatcher

**Files:**
- Create: `crates/xvision-execution/src/dispatcher.rs`
- Modify: `crates/xvision-execution/src/lib.rs`
- Test: `crates/xvision-execution/tests/dispatcher_test.rs`

- [ ] **Step 1: Failing tests covering the gate→reserve→simulate→sign→submit→audit pipeline**

```rust
// crates/xvision-execution/tests/dispatcher_test.rs
// Three tests:
// 1. happy path: dispatch → position row exists → 5 audit rows (emit, risk_eval, simulate, sign, submit)
// 2. risk veto: dispatch → no position row → 2 audit rows (emit, risk_eval)
// 3. simulation slippage exceeds cap → no position row → 3 audit rows (emit, risk_eval, simulate=reject)
```

- [ ] **Step 2: Implement `OrderDispatcher`**

```rust
// crates/xvision-execution/src/dispatcher.rs
use anyhow::Result;
use std::sync::Arc;
use xvision_core::trading::TraderDecision;
use xvision_data::audit::{AuditLog, Stage};
use xvision_data::ledger::{Ledger, Position, Side};
use xvision_data::status::StatusStore;
use xvision_risk::config::StrategyConfig;
use xvision_risk::reservations::ReservationManager;
use xvision_risk::rules::per_strategy::{EvaluationInput, PerStrategyEvaluator, Verdict};
use crate::simulate::Simulator;

pub struct OrderDispatcher {
    pub audit: Arc<AuditLog>,
    pub ledger: Arc<Ledger>,
    pub status: Arc<StatusStore>,
    pub reservations: Arc<ReservationManager>,
    pub simulator: Arc<Simulator>,
    pub orderly: Arc<dyn OrderlyOrderSubmit + Send + Sync>,
}

#[async_trait::async_trait]
pub trait OrderlyOrderSubmit {
    async fn submit_order(&self, client_order_id: &str, asset: &str, side: &str, size_usdc: f64)
        -> Result<String>; // returns orderly_position_id
}

#[derive(Debug)]
pub enum DispatchOutcome {
    Submitted { position_id: String, orderly_position_id: String },
    Vetoed { reason: String },
    AwaitingApproval { approval_id: String },
    Halted,
}

impl OrderDispatcher {
    pub async fn dispatch(
        &self,
        user_id: &str,
        agent_id: &str,
        cfg: &StrategyConfig,
        decision: TraderDecision,
        chain: &str,
        protocol: &str,
        now_utc_hh_mm: &str,
        orders_in_last_minute: u32,
        orders_in_last_hour: u32,
        realized_loss_today_usdc: f64,
    ) -> Result<DispatchOutcome> {
        // Stage 1: emit
        let emit_id = self.audit.write(
            Stage::Emit, user_id, agent_id, None, None,
            serde_json::to_value(&decision)?, None,
        ).await?;

        // Stage 2: status check
        if !self.status.is_active(agent_id).await? {
            self.audit.write(Stage::Reject, user_id, agent_id, None, Some(&emit_id),
                serde_json::json!({"reason": "strategy halted"}), None).await?;
            return Ok(DispatchOutcome::Halted);
        }

        // Stage 3: risk eval
        let in_flight = self.ledger.in_flight_notional(agent_id).await?;
        let open_count = self.ledger.open_count_for_strategy(agent_id).await? as u32;
        let intended_notional = decision.notional_usdc(); // helper on TraderDecision; add if missing
        let asset = decision.asset().to_string();
        let side = decision.side_str();

        let verdict = PerStrategyEvaluator::new(cfg.clone()).evaluate(EvaluationInput {
            intended_asset: asset.clone(),
            intended_notional,
            in_flight_notional: in_flight,
            open_positions: open_count,
            realized_loss_today_usdc,
            orders_in_last_minute,
            orders_in_last_hour,
            now_utc_hh_mm: now_utc_hh_mm.into(),
            chain: chain.into(),
            protocol: protocol.into(),
        });

        self.audit.write(Stage::RiskEval, user_id, agent_id, None, Some(&emit_id),
            serde_json::json!({"verdict": format!("{:?}", verdict)}), None).await?;

        match verdict {
            Verdict::Vetoed { reason } => return Ok(DispatchOutcome::Vetoed { reason }),
            Verdict::RequiresApproval { threshold } => {
                let approval_id = self.request_approval(user_id, agent_id, &decision, intended_notional, threshold).await?;
                return Ok(DispatchOutcome::AwaitingApproval { approval_id });
            }
            Verdict::Approved => {}
        }

        // Stage 4: reserve
        let reservation_id = self.reservations
            .try_reserve(user_id, agent_id, intended_notional, cfg.hard_cap_usdc_notional)
            .await?;

        // Stage 5: simulate
        let sim = self.simulator.simulate(&asset, side, intended_notional / 78_000.0 /* qty proxy; replace with mark */).await?;
        if sim.estimated_slippage_bps > cfg.max_slippage_bps as f64 {
            self.reservations.release(&reservation_id).await?;
            self.audit.write(Stage::Simulate, user_id, agent_id, None, Some(&emit_id),
                serde_json::json!({"slippage_bps": sim.estimated_slippage_bps, "rejected": true}), None).await?;
            return Ok(DispatchOutcome::Vetoed { reason: format!("slippage too high: {}", sim.estimated_slippage_bps) });
        }
        self.audit.write(Stage::Simulate, user_id, agent_id, None, Some(&emit_id),
            serde_json::to_value(&sim)?, None).await?;

        // Stage 6: open ledger row + sign + submit
        let position = Position::new(user_id, agent_id, &asset,
            if side == "BUY" { Side::Long } else { Side::Short }, intended_notional);
        let position = self.ledger.open_position(position).await?;

        self.audit.write(Stage::Sign, user_id, agent_id, Some(&position.position_id), Some(&emit_id),
            serde_json::json!({"client_order_id": &position.client_order_id}), None).await?;

        let orderly_pos_id = self.orderly
            .submit_order(&position.client_order_id, &asset, side, intended_notional)
            .await?;

        self.ledger.record_fill(&position.position_id, sim.estimated_fill_price, Some(&orderly_pos_id)).await?;
        self.reservations.release(&reservation_id).await?;

        self.audit.write(Stage::Submit, user_id, agent_id, Some(&position.position_id), Some(&emit_id),
            serde_json::json!({"orderly_position_id": &orderly_pos_id}), None).await?;

        Ok(DispatchOutcome::Submitted {
            position_id: position.position_id, orderly_position_id: orderly_pos_id,
        })
    }

    async fn request_approval(
        &self, _user_id: &str, _agent_id: &str, _decision: &TraderDecision,
        _notional: f64, _threshold: f64,
    ) -> Result<String> {
        // Implemented in Phase 4 Task 4.5; for now, write a pending row and return its id.
        unimplemented!("see Phase 4 Task 4.5")
    }
}
```

- [ ] **Step 3: Add helper methods on `TraderDecision`**

Modify `crates/xvision-core/src/trading.rs` to add `pub fn notional_usdc(&self) -> f64`, `pub fn asset(&self) -> &str`, `pub fn side_str(&self) -> &'static str`. (Mechanical; ~15 lines.)

- [ ] **Step 4: Wire module + run tests**

Edit `crates/xvision-execution/src/lib.rs`, add `pub mod dispatcher; pub mod simulate;`.

Run: `cargo test -p xvision-execution --test dispatcher_test`
Expected: PASS (3 tests; the approval-required path is gated behind `unimplemented!` for now and is exercised in Phase 4)

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-execution/ crates/xvision-core/
git commit -m "feat(execution): OrderDispatcher with full gate→reserve→simulate→sign→submit→audit pipeline"
```

### Task 3.3: Wire dispatcher into existing trade flow

**Files:**
- Modify: callers of the existing `OrderlyExecutor` (find via `grep -rn OrderlyExecutor crates/`)

- [ ] **Step 1: Identify call sites**

Run: `grep -rn "OrderlyExecutor\|submit_order" crates/ --include='*.rs'`
List the call sites. Likely in `crates/xvision-engine/`, `crates/xvision-cli/src/commands/fire_trade.rs`, and the live-deploy path if Plan 2c has shipped.

- [ ] **Step 2: Replace direct executor calls with `OrderDispatcher::dispatch`**

For each call site, construct the dispatcher (via a shared `AppContext` or DI struct) and call `.dispatch(user_id, agent_id, &cfg, decision, ...)`. The previous direct submission becomes the `OrderlyOrderSubmit` impl backing the dispatcher.

- [ ] **Step 3: Run integration tests**

Run: `cargo test --workspace --exclude m1-orderly-key-scope --exclude m2-orderly-margin-modes`
Expected: PASS (all existing tests still pass; new dispatcher tests pass)

- [ ] **Step 4: Commit**

```bash
git add crates/
git commit -m "refactor(execution): route all Orderly orders through OrderDispatcher"
```

---

## Phase 4 — Kill Switches + Approval Gates + Emergency-Close + CLI Surface

End of phase: every CLI command listed in spec §3.9 exists, is registered with clap, surfaces in `xvn --help`, and is documented in `docs/cli-reference.md`.

### Task 4.1: Auto-trigger logic for strategy halt

**Files:**
- Create: `crates/xvision-risk/src/auto_halt.rs`
- Modify: `crates/xvision-risk/src/lib.rs`
- Test: `crates/xvision-risk/tests/auto_halt_test.rs`

- [ ] **Step 1: Failing test for consecutive-losses trigger**

```rust
// crates/xvision-risk/tests/auto_halt_test.rs
use xvision_risk::auto_halt::{AutoHalter, AutoHaltConfig};

#[test]
fn halts_after_n_consecutive_losses() {
    let cfg = AutoHaltConfig { consecutive_losses_kill: 3, sharpe_floor_kill: -2.0 };
    let halter = AutoHalter::new(cfg);
    let pnls = vec![-10.0, -20.0, -5.0]; // three losses in a row
    let r = halter.should_halt(&pnls);
    assert!(r.is_some());
    assert!(r.unwrap().contains("consecutive_losses"));
}

#[test]
fn does_not_halt_with_intermittent_wins() {
    let cfg = AutoHaltConfig { consecutive_losses_kill: 3, sharpe_floor_kill: -10.0 };
    let halter = AutoHalter::new(cfg);
    let pnls = vec![-10.0, 5.0, -10.0, -10.0];
    assert!(halter.should_halt(&pnls).is_none());
}
```

- [ ] **Step 2: Implement `AutoHalter`**

```rust
// crates/xvision-risk/src/auto_halt.rs
#[derive(Debug, Clone)]
pub struct AutoHaltConfig {
    pub consecutive_losses_kill: u32,
    pub sharpe_floor_kill: f64,
}

pub struct AutoHalter { cfg: AutoHaltConfig }

impl AutoHalter {
    pub fn new(cfg: AutoHaltConfig) -> Self { Self { cfg } }

    /// `closed_pnls` is realized PnL of last N closed positions, oldest first.
    pub fn should_halt(&self, closed_pnls: &[f64]) -> Option<String> {
        // consecutive losses
        let consecutive = closed_pnls.iter().rev()
            .take_while(|p| **p < 0.0).count() as u32;
        if consecutive >= self.cfg.consecutive_losses_kill {
            return Some(format!("consecutive_losses_kill: {} losses in a row", consecutive));
        }
        // rolling sharpe (if enough samples)
        if closed_pnls.len() >= 30 {
            let last30 = &closed_pnls[closed_pnls.len()-30..];
            let mean = last30.iter().sum::<f64>() / 30.0;
            let var = last30.iter().map(|p| (p-mean).powi(2)).sum::<f64>() / 30.0;
            let std = var.sqrt();
            let sharpe = if std > 0.0 { mean / std } else { 0.0 };
            if sharpe < self.cfg.sharpe_floor_kill {
                return Some(format!("sharpe_floor_kill: {:.3} < {:.3}", sharpe, self.cfg.sharpe_floor_kill));
            }
        }
        None
    }
}
```

- [ ] **Step 3: Wire + test**

Edit `crates/xvision-risk/src/lib.rs`, add `pub mod auto_halt;`.

Run: `cargo test -p xvision-risk --test auto_halt_test`
Expected: PASS

- [ ] **Step 4: Hook auto-halter into close-position flow**

Modify the dispatcher's close path (Phase 3 Task 3.2 `OrderDispatcher` — extend with a `record_close` method that fetches recent PnLs from ledger, runs `AutoHalter`, calls `StatusStore::halt` if triggered).

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-risk/ crates/xvision-execution/
git commit -m "feat(risk): auto-halt on consecutive losses + sharpe floor"
```

### Task 4.2: `xvn kill` CLI command

**Files:**
- Create: `crates/xvision-cli/src/commands/kill.rs`
- Modify: `crates/xvision-cli/src/main.rs`, `crates/xvision-cli/src/commands/mod.rs`, `crates/xvision-cli/Cargo.toml`
- Test: `crates/xvision-cli/tests/kill_cli_test.rs`

- [ ] **Step 1: Add dialoguer dep**

Edit `crates/xvision-cli/Cargo.toml`:

```toml
dialoguer = { workspace = true }
```

- [ ] **Step 2: Failing test — `xvn kill --strategy <id>` halts the strategy**

```rust
// crates/xvision-cli/tests/kill_cli_test.rs
use std::process::Command;

#[test]
fn kill_strategy_sets_halted_manual() {
    // Use a dedicated test sqlite file under tempdir.
    let tmp = tempfile::tempdir().unwrap();
    let db = tmp.path().join("xvn.db");

    // Seed the DB by running migrations (call into xvision-data testkit, or
    // shell out to a helper).
    // ... (omitted; mechanical) ...

    let out = Command::new(env!("CARGO_BIN_EXE_xvn"))
        .env("XVN_DB_PATH", db.display().to_string())
        .env("XVN_OPERATOR_CONFIRMED", "1")
        .args(["kill", "--strategy", "s1", "--yes"])
        .output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // Verify status row...
}
```

- [ ] **Step 3: Implement the command**

```rust
// crates/xvision-cli/src/commands/kill.rs
use anyhow::{anyhow, Result};
use clap::Args;
use dialoguer::Confirm;

#[derive(Args, Debug)]
pub struct KillArgs {
    /// Halt one strategy by id.
    #[arg(long, group = "target")]
    pub strategy: Option<String>,
    /// Halt all strategies for one user (also revokes their trading authority).
    #[arg(long, group = "target")]
    pub user: Option<String>,
    /// Global halt: refuse new orders everywhere.
    #[arg(long, group = "target")]
    pub all: bool,
    /// Skip the confirmation prompt. Requires XVN_OPERATOR_CONFIRMED=1 in env.
    #[arg(long)]
    pub yes: bool,
    /// Reason logged to audit.
    #[arg(long, default_value = "operator kill")]
    pub reason: String,
}

pub async fn run(args: KillArgs) -> Result<()> {
    let target_desc = match (&args.strategy, &args.user, args.all) {
        (Some(s), None, false) => format!("strategy {}", s),
        (None, Some(u), false) => format!("user {}", u),
        (None, None, true)     => "ALL strategies (global)".into(),
        _ => return Err(anyhow!("specify exactly one of --strategy/--user/--all")),
    };

    if !args.yes {
        let proceed = Confirm::new()
            .with_prompt(format!("Halt {}? This rejects new orders but does NOT close positions.", target_desc))
            .default(false).interact()?;
        if !proceed { return Ok(()); }
    } else if std::env::var("XVN_OPERATOR_CONFIRMED").as_deref() != Ok("1") {
        return Err(anyhow!("--yes requires XVN_OPERATOR_CONFIRMED=1 in environment"));
    }

    // Connect to DB via shared AppContext (see Task 4.10 for context wiring).
    let ctx = crate::context::AppContext::from_env().await?;

    if let Some(strategy) = &args.strategy {
        ctx.status.halt(strategy, &args.reason, "operator-cli", false).await?;
        println!("✓ Halted strategy {}", strategy);
    } else if let Some(user) = &args.user {
        // Halt all strategies for this user.
        let strategies = ctx.list_user_strategies(user).await?;
        for s in &strategies {
            ctx.status.halt(s, &args.reason, "operator-cli", false).await?;
        }
        // Revoke trading authority for this user.
        ctx.revoke_user_trading_key(user).await?;
        println!("✓ Halted {} strategies for user {} and revoked trading key", strategies.len(), user);
    } else {
        ctx.global_halt(&args.reason, "operator-cli").await?;
        println!("✓ GLOBAL HALT engaged. All dispatchers refusing new orders.");
    }
    Ok(())
}
```

- [ ] **Step 4: Register subcommand in main.rs**

```rust
// crates/xvision-cli/src/main.rs
#[derive(clap::Subcommand)]
enum Cmd {
    // ...existing...
    /// Halt strategies or users (rejects new orders; does NOT close positions)
    Kill(commands::kill::KillArgs),
    // ...others added below...
}

match cli.command {
    Cmd::Kill(args) => commands::kill::run(args).await?,
    // ...
}
```

- [ ] **Step 5: Run, verify pass**

Run: `cargo test -p xvision-cli kill_cli`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-cli/
git commit -m "feat(cli): xvn kill --strategy/--user/--all"
```

### Task 4.3: `xvn unhalt` CLI command

**Files:**
- Create: `crates/xvision-cli/src/commands/unhalt.rs`

- [ ] **Step 1: Implement** (mirror `kill.rs`; only `--strategy` target; always prompts unless `--yes` + env var; calls `ctx.status.unhalt(...)`)

- [ ] **Step 2: Register + test + commit**

```bash
git commit -m "feat(cli): xvn unhalt --strategy"
```

### Task 4.4: `xvn emergency-close` CLI command

**Files:**
- Create: `crates/xvision-cli/src/commands/emergency_close.rs`
- Modify: `crates/xvision-execution/src/orderly.rs` (add `cancel_all_orders` and `market_close_all_positions` helpers)
- Test: `crates/xvision-execution/tests/emergency_close_test.rs`

- [ ] **Step 1: Implement Orderly helpers**

In `crates/xvision-execution/src/orderly.rs`:

```rust
impl OrderlyExecutor {
    /// Cancel every open order for `user_id`.
    pub async fn cancel_all_orders(&self, user_id: &str) -> Result<u32> {
        // 1. GET /v3/orders?status=NEW
        // 2. For each, DELETE /v3/order/{id}
        // 3. Return count cancelled.
        todo!("implement using existing reqwest+sign helpers")
    }

    /// Submit market orders to close every open position for `user_id`.
    pub async fn market_close_all_positions(&self, user_id: &str) -> Result<u32> {
        // 1. GET /v3/positions?status=OPEN
        // 2. For each, POST /v3/orders {symbol, side: opposite, type: MARKET, qty: position size}
        // 3. Return count submitted.
        todo!("implement using existing reqwest+sign helpers")
    }
}
```

- [ ] **Step 2: Implement the CLI command**

```rust
// crates/xvision-cli/src/commands/emergency_close.rs
// Same arg shape as kill.rs (--strategy/--user/--all); always prompts unless --yes + env.
// Body:
//   1. Confirm with strong wording: "This will MARKET-CLOSE all positions matching the selector.
//      Slippage may be significant. Type 'CLOSE' to proceed."
//   2. Call orderly.cancel_all_orders for the matched users.
//   3. Call orderly.market_close_all_positions for the matched users.
//   4. Update ledger close rows via reconcile (calls Phase 7's reconciler immediately).
//   5. Print summary: cancelled N orders, closed M positions.
```

- [ ] **Step 3: Test (mock Orderly)**

Mock `OrderlyExecutor` to return preset open orders/positions; assert cancel + market close called with right counts.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(execution+cli): xvn emergency-close cancels all open orders and market-closes all open positions"
```

### Task 4.5: Approval gate workflow

**Files:**
- Create: `crates/xvision-execution/src/approval.rs`
- Modify: `crates/xvision-execution/src/dispatcher.rs` (replace `unimplemented!` in `request_approval`)
- Test: `crates/xvision-execution/tests/approval_test.rs`

- [ ] **Step 1: Failing test — pending approval expires after TTL**

```rust
// crates/xvision-execution/tests/approval_test.rs
#[tokio::test]
async fn approval_request_creates_pending_row_and_expires() {
    // Submit decision over threshold → expect AwaitingApproval.
    // Verify a pending_approvals row exists with state=pending.
    // Sleep past TTL, run reaper, verify state=expired.
}
```

- [ ] **Step 2: Implement `ApprovalWorkflow`**

```rust
// crates/xvision-execution/src/approval.rs
use anyhow::Result;
use chrono::Utc;
use sqlx::SqlitePool;
use ulid::Ulid;
use std::time::Duration;

const DEFAULT_APPROVAL_TTL: Duration = Duration::from_secs(60);

pub struct ApprovalWorkflow { pool: SqlitePool, ttl: Duration }

impl ApprovalWorkflow {
    pub fn new(pool: SqlitePool) -> Self { Self { pool, ttl: DEFAULT_APPROVAL_TTL } }

    pub async fn request(
        &self, user_id: &str, agent_id: &str,
        decision_payload: &serde_json::Value, notional: f64,
    ) -> Result<String> {
        let id = Ulid::new().to_string();
        let now = Utc::now().timestamp_millis();
        let exp = now + self.ttl.as_millis() as i64;
        sqlx::query(
            r#"INSERT INTO pending_approvals
               (approval_id, requested_at, expires_at, user_id, agent_id,
                decision_payload, notional_usdc, state)
               VALUES (?, ?, ?, ?, ?, ?, ?, 'pending')"#,
        )
        .bind(&id).bind(now).bind(exp).bind(user_id).bind(agent_id)
        .bind(serde_json::to_string(decision_payload)?).bind(notional)
        .execute(&self.pool).await?;
        Ok(id)
    }

    pub async fn approve(&self, approval_id: &str, by: &str) -> Result<()> {
        sqlx::query(
            "UPDATE pending_approvals SET state='approved', resolved_at=?, resolved_by=? WHERE approval_id=? AND state='pending'",
        )
        .bind(Utc::now().timestamp_millis()).bind(by).bind(approval_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn reject(&self, approval_id: &str, by: &str) -> Result<()> {
        sqlx::query(
            "UPDATE pending_approvals SET state='rejected', resolved_at=?, resolved_by=? WHERE approval_id=? AND state='pending'",
        )
        .bind(Utc::now().timestamp_millis()).bind(by).bind(approval_id)
        .execute(&self.pool).await?;
        Ok(())
    }

    pub async fn list_pending(&self) -> Result<Vec<PendingApproval>> {
        let now = Utc::now().timestamp_millis();
        let rows = sqlx::query_as::<_, PendingApproval>(
            "SELECT * FROM pending_approvals WHERE state='pending' AND expires_at > ?",
        )
        .bind(now).fetch_all(&self.pool).await?;
        Ok(rows)
    }

    pub async fn reap_expired(&self) -> Result<u64> {
        let now = Utc::now().timestamp_millis();
        let r = sqlx::query(
            "UPDATE pending_approvals SET state='expired' WHERE state='pending' AND expires_at <= ?",
        ).bind(now).execute(&self.pool).await?;
        Ok(r.rows_affected())
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct PendingApproval {
    pub approval_id: String,
    pub requested_at: i64,
    pub expires_at: i64,
    pub user_id: String,
    pub agent_id: String,
    pub decision_payload: String,
    pub notional_usdc: f64,
    pub state: String,
}
```

- [ ] **Step 3: Replace `unimplemented!` in dispatcher**

In `dispatcher.rs`, the `request_approval` body becomes:

```rust
let decision_value = serde_json::to_value(decision)?;
self.approval_workflow.request(user_id, agent_id, &decision_value, notional).await
```

(Add `pub approval_workflow: Arc<ApprovalWorkflow>` to `OrderDispatcher` struct.)

- [ ] **Step 4: Run, verify, commit**

```bash
cargo test -p xvision-execution --test approval_test
git add crates/xvision-execution/
git commit -m "feat(execution): approval gate workflow with TTL expiry"
```

### Task 4.6: `xvn approve` CLI command

**Files:**
- Create: `crates/xvision-cli/src/commands/approve.rs`

- [ ] **Step 1: Subcommands — list, <id> (approve), <id> --reject**

```rust
#[derive(clap::Args, Debug)]
pub struct ApproveArgs {
    /// List pending approvals with TTL countdowns.
    #[arg(long, group = "action")]
    pub list: bool,
    /// Approval id to act on.
    #[arg(group = "action")]
    pub approval_id: Option<String>,
    /// Reject instead of approve.
    #[arg(long, requires = "approval_id")]
    pub reject: bool,
}

pub async fn run(args: ApproveArgs) -> Result<()> {
    let ctx = crate::context::AppContext::from_env().await?;
    if args.list {
        let pending = ctx.approvals.list_pending().await?;
        for p in pending {
            let secs = (p.expires_at - chrono::Utc::now().timestamp_millis()) / 1000;
            println!("{}  user={}  strategy={}  notional=${}  expires_in={}s",
                p.approval_id, p.user_id, p.agent_id, p.notional_usdc, secs);
        }
        return Ok(());
    }
    let id = args.approval_id.ok_or_else(|| anyhow!("provide --list or an approval-id"))?;
    if args.reject {
        ctx.approvals.reject(&id, "operator-cli").await?;
        println!("✗ Rejected {}", id);
    } else {
        ctx.approvals.approve(&id, "operator-cli").await?;
        println!("✓ Approved {}", id);
    }
    Ok(())
}
```

- [ ] **Step 2: Register, test, commit**

```bash
git commit -m "feat(cli): xvn approve list/<id>/--reject"
```

### Task 4.7: `xvn key` CLI commands

**Files:**
- Create: `crates/xvision-cli/src/commands/key.rs`
- Create: `crates/xvision-identity/src/trading_key.rs`
- Modify: `crates/xvision-identity/Cargo.toml` (add `aes-gcm`)

- [ ] **Step 1: Implement trading_key.rs**

```rust
// crates/xvision-identity/src/trading_key.rs
use aes_gcm::{
    aead::{Aead, KeyInit, OsRng, AeadCore},
    Aes256Gcm, Key, Nonce,
};
use anyhow::{anyhow, Result};

pub fn encrypt(plaintext: &[u8]) -> Result<String> {
    let secret_hex = std::env::var("CREDENTIAL_SECRET")
        .map_err(|_| anyhow!("CREDENTIAL_SECRET env var required"))?;
    let secret = hex::decode(secret_hex)?;
    if secret.len() != 32 { return Err(anyhow!("CREDENTIAL_SECRET must be 64 hex chars (32 bytes)")); }
    let key = Key::<Aes256Gcm>::from_slice(&secret);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ct = cipher.encrypt(&nonce, plaintext)
        .map_err(|e| anyhow!("encrypt: {e}"))?;
    Ok(format!("{}:{}", hex::encode(nonce), hex::encode(ct)))
}

pub fn decrypt(blob: &str) -> Result<Vec<u8>> {
    let secret_hex = std::env::var("CREDENTIAL_SECRET")?;
    let secret = hex::decode(secret_hex)?;
    let key = Key::<Aes256Gcm>::from_slice(&secret);
    let cipher = Aes256Gcm::new(key);
    let parts: Vec<&str> = blob.splitn(2, ':').collect();
    if parts.len() != 2 { return Err(anyhow!("malformed encrypted blob")); }
    let nonce = hex::decode(parts[0])?;
    let ct = hex::decode(parts[1])?;
    cipher.decrypt(Nonce::from_slice(&nonce), ct.as_ref())
        .map_err(|e| anyhow!("decrypt: {e}"))
}
```

Test in `crates/xvision-identity/tests/trading_key_test.rs`:

```rust
#[test]
fn round_trip() {
    std::env::set_var("CREDENTIAL_SECRET", "00".repeat(32));
    let blob = xvision_identity::trading_key::encrypt(b"hello world").unwrap();
    assert_ne!(blob, "hello world");
    let dec = xvision_identity::trading_key::decrypt(&blob).unwrap();
    assert_eq!(dec, b"hello world");
}
```

- [ ] **Step 2: Implement `xvn key` subcommands**

```rust
// crates/xvision-cli/src/commands/key.rs
#[derive(clap::Subcommand, Debug)]
pub enum KeyCmd {
    /// Generate + register a new trading key for the operator user (v1: single user).
    Issue {
        #[arg(long, default_value = "operator")] user: String,
        #[arg(long, default_value = "90")] expires_in_days: u32,
    },
    /// Show all stored keys with scopes + expiry (no private material).
    List,
    /// Rotate a key (re-registration UX with the user signing add_orderly_key).
    Rotate { #[arg(long)] user: String },
    /// Mark a key as revoked locally. User must also revoke on Orderly's web UI.
    Revoke { #[arg(long)] user: String },
}

pub async fn run(cmd: KeyCmd) -> Result<()> {
    match cmd {
        KeyCmd::Issue { user, expires_in_days } => issue(user, expires_in_days).await,
        KeyCmd::List => list().await,
        KeyCmd::Rotate { user } => rotate(user).await,
        KeyCmd::Revoke { user } => revoke(user).await,
    }
}

async fn issue(user: String, expires_in_days: u32) -> Result<()> {
    use ed25519_dalek::SigningKey;
    use rand_core::OsRng;
    let mut csprng = OsRng;
    let signing_key = SigningKey::generate(&mut csprng);
    let pubkey = signing_key.verifying_key();
    let pubhex = hex::encode(pubkey.to_bytes());

    println!("Generated trading key for user {user}.");
    println!();
    println!("  Public key (Ed25519):   {pubhex}");
    println!("  Permissions requested:  trading (NO withdraw, NO transfer)");
    println!("  Expires in:             {expires_in_days} days");
    println!();
    println!("⚠ NEXT STEP: sign the add_orderly_key request below from your EVM wallet.");
    println!("  We strongly recommend a hardware wallet for this single signature.");
    println!("  After signing, verify the registration in Orderly's web UI:");
    println!("    https://app.orderly.network/account/api-keys");
    println!();

    if !dialoguer::Confirm::new()
        .with_prompt("Open Orderly registration in browser now?")
        .interact()? {
        return Ok(());
    }

    // ... open browser to a templated URL with the pubkey + scope + expiry ...
    // ... prompt user to confirm registration completed ...

    // Encrypt + store.
    let encrypted = xvision_identity::trading_key::encrypt(signing_key.as_bytes())?;
    let ctx = crate::context::AppContext::from_env().await?;
    ctx.store_trading_key(&user, &pubhex, &encrypted, expires_in_days).await?;
    println!("✓ Stored trading key for {user}.");
    Ok(())
}

async fn list() -> Result<()> {
    let ctx = crate::context::AppContext::from_env().await?;
    let keys = ctx.list_trading_keys().await?;
    println!("{:<20} {:<70} {:<10}", "USER", "PUBKEY", "EXPIRES");
    for k in keys {
        println!("{:<20} {:<70} {:<10}", k.user, k.pubkey_hex, k.expires_at);
    }
    Ok(())
}

async fn rotate(user: String) -> Result<()> {
    println!("Rotating key for {user} — same flow as `xvn key issue`.");
    issue(user, 90).await
}

async fn revoke(user: String) -> Result<()> {
    let ctx = crate::context::AppContext::from_env().await?;
    ctx.revoke_user_trading_key(&user).await?;
    println!("✓ Revoked locally. ALSO revoke on Orderly's web UI:");
    println!("    https://app.orderly.network/account/api-keys");
    Ok(())
}
```

- [ ] **Step 3: Test, register, commit**

```bash
git commit -m "feat(identity+cli): xvn key issue/list/rotate/revoke with phishing-resistant pubkey display"
```

### Task 4.8: `xvn budget` CLI commands

**Files:**
- Create: `crates/xvision-cli/src/commands/budget.rs`

- [ ] **Step 1: Subcommands**

```rust
#[derive(clap::Subcommand, Debug)]
pub enum BudgetCmd {
    /// Spreadsheet-style table of all strategy budgets.
    Show {
        #[arg(long)] strategy: Option<String>,
    },
    /// Set one or more budget fields on one strategy.
    Set {
        #[arg(long)] strategy: String,
        #[arg(long)] hard_cap: Option<f64>,
        #[arg(long)] slippage: Option<u32>,
        #[arg(long)] orders_per_minute: Option<u32>,
        #[arg(long)] orders_per_hour: Option<u32>,
        #[arg(long)] active_hours: Option<String>,
        #[arg(long)] daily_loss: Option<f64>,
        #[arg(long)] approval_above: Option<f64>,
        #[arg(long)] comment: Option<String>,
    },
    /// Apply a TOML config across many strategies.
    BulkImport { #[arg(value_name = "TOML_PATH")] path: std::path::PathBuf },
    /// Launch the budget spreadsheet UI (Phase 8).
    Serve { #[arg(long, default_value = "127.0.0.1:7878")] addr: String },
}
```

- [ ] **Step 2: Implement `Show`** — read all `StrategyConfig` from disk + ledger state, render a fixed-width table with columns matching the spec §3.4 spreadsheet sketch (Strategy / Hard Cap / Slippage / Orders/min / Active Hours / Mode / Quota / Status).

- [ ] **Step 3: Implement `Set`** — read existing config, apply changed fields, write to `policy_changes` for each touched field, write the new config back, prompt-confirm with old → new diff before saving.

- [ ] **Step 4: Implement `BulkImport`** — parse TOML, diff against current config, prompt-confirm aggregate, apply.

- [ ] **Step 5: Implement `Serve`** — `xvision_budget_ui::serve(addr).await` (calls into Phase 8 crate; until that crate exists, returns "Phase 8 UI not built yet — run after Phase 8 ships").

- [ ] **Step 6: Test, register, commit**

```bash
git commit -m "feat(cli): xvn budget show/set/bulk-import/serve"
```

### Task 4.9: `xvn audit` CLI commands

**Files:**
- Create: `crates/xvision-cli/src/commands/audit.rs`

- [ ] **Step 1: Subcommands**

```rust
#[derive(clap::Subcommand, Debug)]
pub enum AuditCmd {
    /// Full pipeline trace for one position.
    Position { position_id: String },
    /// All decisions for one strategy since timestamp (RFC3339 or "1h"/"1d"/etc).
    Strategy { agent_id: String, #[arg(long)] since: Option<String> },
    /// Decisions awaiting operator approval.
    Pending,
}
```

- [ ] **Step 2: Implement** — call `AuditLog::for_position`, render decision rows in chronological order (one row per stage, with stage / occurred_at / payload-pretty-printed). For `Strategy`, parse the relative-time string into a millis offset. For `Pending`, delegate to `ApprovalWorkflow::list_pending`.

- [ ] **Step 3: Test, register, commit**

```bash
git commit -m "feat(cli): xvn audit position/strategy/pending"
```

### Task 4.10: `xvn reconcile` CLI command + AppContext

**Files:**
- Create: `crates/xvision-cli/src/context.rs` (shared AppContext used by all commands)
- Create: `crates/xvision-cli/src/commands/reconcile.rs`

- [ ] **Step 1: AppContext**

```rust
// crates/xvision-cli/src/context.rs
use anyhow::Result;
use std::sync::Arc;
use sqlx::SqlitePool;

pub struct AppContext {
    pub pool: SqlitePool,
    pub ledger: Arc<xvision_data::ledger::Ledger>,
    pub audit:  Arc<xvision_data::audit::AuditLog>,
    pub status: Arc<xvision_data::status::StatusStore>,
    pub policy: Arc<xvision_data::policy::PolicyJournal>,
    pub approvals: Arc<xvision_execution::approval::ApprovalWorkflow>,
    pub reconciler: Arc<xvision_execution::reconcile::Reconciler>,
    // ... add more as needed by commands ...
}

impl AppContext {
    pub async fn from_env() -> Result<Self> {
        let db_path = std::env::var("XVN_DB_PATH").unwrap_or_else(|_| "data/xvn.db".to_string());
        let pool = SqlitePool::connect(&format!("sqlite:{db_path}?mode=rwc")).await?;
        sqlx::migrate!("../xvision-data/src/migrations").run(&pool).await?;
        let ledger = Arc::new(xvision_data::ledger::Ledger::new(pool.clone()));
        let audit  = Arc::new(xvision_data::audit::AuditLog::new(pool.clone()));
        let status = Arc::new(xvision_data::status::StatusStore::new(pool.clone()));
        let policy = Arc::new(xvision_data::policy::PolicyJournal::new(pool.clone()));
        let approvals = Arc::new(xvision_execution::approval::ApprovalWorkflow::new(pool.clone()));
        let reconciler = Arc::new(xvision_execution::reconcile::Reconciler::new(/*...*/));
        Ok(Self { pool, ledger, audit, status, policy, approvals, reconciler })
    }
}
```

- [ ] **Step 2: `xvn reconcile`**

```rust
// crates/xvision-cli/src/commands/reconcile.rs
#[derive(clap::Args, Debug)]
pub struct ReconcileArgs {
    #[arg(long)] pub user: String,
    #[arg(long)] pub dry_run: bool,
}

pub async fn run(args: ReconcileArgs) -> Result<()> {
    let ctx = crate::context::AppContext::from_env().await?;
    let report = ctx.reconciler.run(&args.user, args.dry_run).await?;
    println!("{report}");
    Ok(())
}
```

- [ ] **Step 3: Test, register, commit**

```bash
git commit -m "feat(cli): xvn reconcile + shared AppContext"
```

### Task 4.11: CLI reference doc + help test

**Files:**
- Create: `docs/cli-reference.md`
- Create: `crates/xvision-cli/tests/help_test.rs`

- [ ] **Step 1: Write `docs/cli-reference.md`**

Mirror the structure from spec §3.9, fully expanded with each subcommand's flags, examples, and exit codes. Sections: Key, Budget, Kill, Unhalt, Emergency-Close, Approve, Audit, Reconcile.

- [ ] **Step 2: Help-surface test**

```rust
// crates/xvision-cli/tests/help_test.rs
#[test]
fn help_lists_all_phase4_commands() {
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_xvn"))
        .arg("--help").output().unwrap();
    let txt = String::from_utf8_lossy(&out.stdout);
    for cmd in &["key", "budget", "kill", "unhalt", "emergency-close", "approve", "audit", "reconcile"] {
        assert!(txt.contains(cmd), "xvn --help must surface `{cmd}`. Got:\n{txt}");
    }
}
```

- [ ] **Step 3: Run tests, fix any missing**

Run: `cargo test -p xvision-cli --test help_test`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add docs/cli-reference.md crates/xvision-cli/
git commit -m "docs(cli): full CLI reference + help-surface test asserting discoverability"
```

---

## Phase 5 — Aggregate Margin Guard (conditional on G2)

This phase forks based on ADR 0013.

### Task 5.1 (if G2 = ISOLATED_SUPPORTED): per-strategy margin_mode

**Files:**
- Modify: `crates/xvision-execution/src/orderly.rs` (set margin mode at order time)
- Modify: `crates/xvision-execution/src/dispatcher.rs` (read `cfg.margin_mode` and pass through)
- Test: `crates/xvision-execution/tests/margin_mode_test.rs`

- [ ] **Step 1: Per-strategy default in config loader: `margin_mode = Some(MarginMode::Isolated)` for new strategies**

- [ ] **Step 2: Wire margin_mode into order submission via the `POST /v3/positions/{symbol}/margin_mode` endpoint before order placement (idempotent)**

- [ ] **Step 3: Test asserting an isolated-margin strategy gets the `ISOLATED` parameter set**

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(execution): per-strategy margin_mode (isolated default per ADR 0013)"
```

### Task 5.2 (if G2 = CROSS_ONLY): aggregate margin utilization rule

**Files:**
- Create: `crates/xvision-risk/src/rules/aggregate_margin.rs`
- Modify: `crates/xvision-execution/src/dispatcher.rs` (add aggregate-margin check)
- Modify: `crates/xvision-risk/src/config.rs` (add `[risk.global]` section with `max_aggregate_margin_utilization`)
- Test: `crates/xvision-risk/tests/aggregate_margin_test.rs`

- [ ] **Step 1: Add global config section**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRiskConfig {
    /// Halt new orders if account margin utilization > this (0.0..1.0).
    #[serde(default = "default_max_margin")]
    pub max_aggregate_margin_utilization: f64,
}
fn default_max_margin() -> f64 { 0.60 }
```

- [ ] **Step 2: Implement the rule**

```rust
// crates/xvision-risk/src/rules/aggregate_margin.rs
pub struct AggregateMarginGuard { pub max_utilization: f64 }

impl AggregateMarginGuard {
    pub fn would_exceed(&self, current_margin_used: f64, total_collateral: f64, additional_margin: f64) -> bool {
        let after = current_margin_used + additional_margin;
        if total_collateral <= 0.0 { return true; } // fail closed
        (after / total_collateral) > self.max_utilization
    }
}
```

- [ ] **Step 3: Add a Margin info fetch in `OrderlyExecutor` and call from dispatcher pre-submission**

- [ ] **Step 4: Test, commit**

```bash
git commit -m "feat(risk): aggregate margin utilization guard for cross-margin contagion mitigation"
```

---

## Phase 6 — Dynamic Quota

End of phase: `quota_factor` is computed per dispatch and applied multiplicatively to the hard cap.

### Task 6.1: Pure `quota_factor` function

**Files:**
- Create: `crates/xvision-risk/src/quota.rs`
- Modify: `crates/xvision-risk/src/lib.rs`
- Test: `crates/xvision-risk/tests/quota_test.rs`

- [ ] **Step 1: Failing tests covering cold-start, hot-streak, drawdown decay, kill on full drawdown**

```rust
// crates/xvision-risk/tests/quota_test.rs
use xvision_risk::quota::{compute_quota_factor, QuotaInputs};

#[test]
fn cold_start_returns_floor() {
    let q = compute_quota_factor(QuotaInputs {
        closed_pnls_30d: vec![],   // brand new strategy, < 30 closes
        rolling_drawdown_30d: 0.0,
    });
    assert!((q - 0.25).abs() < 1e-6, "cold-start floor expected 0.25, got {q}");
}

#[test]
fn hot_streak_unlocks_full_cap() {
    // 30 winners of $100 each, no drawdown → quota near 1.0
    let q = compute_quota_factor(QuotaInputs {
        closed_pnls_30d: vec![100.0; 30],
        rolling_drawdown_30d: 0.0,
    });
    assert!(q > 0.9, "hot streak should unlock close to full cap, got {q}");
}

#[test]
fn full_drawdown_throttles_to_zero() {
    let q = compute_quota_factor(QuotaInputs {
        closed_pnls_30d: vec![100.0; 30],
        rolling_drawdown_30d: 0.20, // hits drawdown_floor
    });
    assert!(q < 0.05, "full drawdown should throttle to ~0, got {q}");
}

#[test]
fn loser_streak_throttles() {
    let q = compute_quota_factor(QuotaInputs {
        closed_pnls_30d: vec![-100.0; 30],
        rolling_drawdown_30d: 0.10,
    });
    assert!(q < 0.30, "loser streak should be near floor, got {q}");
}
```

- [ ] **Step 2: Implement**

```rust
// crates/xvision-risk/src/quota.rs
pub struct QuotaInputs {
    pub closed_pnls_30d: Vec<f64>,
    /// Max-drawdown over last 30d, as fraction (0.20 = 20%).
    pub rolling_drawdown_30d: f64,
}

const COLD_START_FLOOR: f64 = 0.25;
const SHARPE_NORMALIZER: f64 = 1.5;
const DRAWDOWN_FLOOR: f64 = 0.20;
const COLD_START_MIN_SAMPLES: usize = 30;

pub fn compute_quota_factor(i: QuotaInputs) -> f64 {
    if i.closed_pnls_30d.len() < COLD_START_MIN_SAMPLES {
        return COLD_START_FLOOR;
    }
    let mean = i.closed_pnls_30d.iter().sum::<f64>() / i.closed_pnls_30d.len() as f64;
    let var  = i.closed_pnls_30d.iter().map(|p| (p-mean).powi(2)).sum::<f64>() / i.closed_pnls_30d.len() as f64;
    let std  = var.sqrt();
    let sharpe = if std > 0.0 { mean / std } else { 0.0 };
    let sigmoid_sharpe = 1.0 / (1.0 + (-(sharpe / SHARPE_NORMALIZER)).exp());
    let drawdown_decay = (1.0 - (i.rolling_drawdown_30d / DRAWDOWN_FLOOR)).max(0.0);
    let raw = COLD_START_FLOOR + sigmoid_sharpe * drawdown_decay;
    raw.clamp(0.0, 1.0)
}
```

- [ ] **Step 3: Property test — quota_factor ∈ [0,1] for any inputs**

```rust
proptest::proptest! {
    #[test]
    fn quota_factor_bounded(pnls in proptest::collection::vec(-10000.0f64..10000.0, 0..100), dd in 0.0f64..1.0) {
        let q = compute_quota_factor(QuotaInputs { closed_pnls_30d: pnls, rolling_drawdown_30d: dd });
        assert!(q >= 0.0 && q <= 1.0);
    }
}
```

- [ ] **Step 4: Wire quota_factor into reservation acquisition**

In `OrderDispatcher::dispatch`, before `try_reserve(...)`, fetch recent PnLs from ledger and compute `quota_factor`. Pass `cap * quota_factor` as the cap argument to the reservation.

- [ ] **Step 5: Run all xvision-risk tests + commit**

```bash
cargo test -p xvision-risk
git commit -m "feat(risk): dynamic quota_factor (sigmoid Sharpe × drawdown decay) applied to hard cap"
```

---

## Phase 7 — Reconciliation Job

End of phase: a tokio task runs every 15 min in active sessions, fetches Orderly state, compares to ledger, surfaces drift to the operator.

### Task 7.1: Reconciler

**Files:**
- Create: `crates/xvision-execution/src/reconcile.rs`
- Test: `crates/xvision-execution/tests/reconcile_test.rs`

- [ ] **Step 1: Failing test covering orphan / closed-server-side / NAV diff**

```rust
#[tokio::test]
async fn reports_orphan_when_orderly_has_position_xvision_does_not() { /* ... */ }

#[tokio::test]
async fn marks_closed_when_xvision_has_open_orderly_does_not() { /* ... */ }

#[tokio::test]
async fn computes_nav_diff() { /* ... */ }
```

- [ ] **Step 2: Implement**

```rust
// crates/xvision-execution/src/reconcile.rs
use anyhow::Result;
use std::sync::Arc;
use xvision_data::ledger::Ledger;

pub struct Reconciler {
    pub ledger: Arc<Ledger>,
    pub orderly: Arc<dyn OrderlyAccountInfo + Send + Sync>,
}

#[async_trait::async_trait]
pub trait OrderlyAccountInfo {
    async fn open_positions(&self, user_id: &str) -> Result<Vec<OrderlyPositionRow>>;
    async fn account_nav(&self, user_id: &str) -> Result<f64>;
}

#[derive(Debug)]
pub struct OrderlyPositionRow {
    pub orderly_position_id: String,
    pub asset: String,
    pub size_usdc: f64,
    pub mark_price: f64,
    pub unrealized_pnl_usdc: f64,
}

pub struct ReconciliationReport {
    pub orphans: Vec<OrderlyPositionRow>,
    pub closed_server_side: Vec<String>, // position_ids
    pub nav_diff_usdc: f64,
    pub when: i64,
}

impl Reconciler {
    pub async fn run(&self, user_id: &str, dry_run: bool) -> Result<ReconciliationReport> {
        let orderly_open = self.orderly.open_positions(user_id).await?;
        // ... diff against ledger.open_positions(user_id) ... compute NAV diff ...
        // ... if !dry_run, mark closed_server_side rows in ledger ...
        todo!("implementation")
    }
}

impl std::fmt::Display for ReconciliationReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Reconciliation report ({}):", self.when)?;
        writeln!(f, "  orphans:           {}", self.orphans.len())?;
        writeln!(f, "  closed-server:     {}", self.closed_server_side.len())?;
        writeln!(f, "  NAV diff:          ${:.2}", self.nav_diff_usdc)?;
        Ok(())
    }
}
```

- [ ] **Step 3: Schedule the job**

If Plan 2c (durable scheduler) has shipped, register the reconciler as a scheduled job. Otherwise: spawn a tokio task in the live-deploy entry point (`xvn live deploy ...`) that loops every 15 minutes.

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(execution): periodic reconciliation against live Orderly state"
```

---

## Phase 8 — Strategy Budgets Spreadsheet UI

**Primary path: add `/budgets` route to `crates/xvision-dashboard` (from Plan 2d).** This plan does NOT build a new dashboard. It adds one route module and one template alongside the Wizard / Inspector / Live cockpit archetypes that 2d ships. The route MUST be registered in 2d's existing `Router` so it appears in the dashboard's navigation.

**Fallback (only if Plan 2d slips past 2026-06-01):** create a standalone `crates/xvision-budget-ui` crate with the same routes + templates, launched via `xvn budget serve --addr 127.0.0.1:7878`. When 2d ships later, lift the routes module + templates into `xvision-dashboard` unchanged.

### Task 8.1: Verify Plan 2d's dashboard exists; if not, branch to fallback

- [ ] **Step 1: Check dashboard crate**

```bash
ls crates/xvision-dashboard/src/lib.rs
```

- If present (Plan 2d shipped) → proceed with Tasks 8.2–8.5 as written.
- If absent → switch to the **fallback flow** documented at the bottom of this phase, then return here when Plan 2d ships and lift the modules into `xvision-dashboard`.

The rest of Phase 8 is written for the primary path. Differences for the fallback are noted in Task 8.6.

### Task 8.2: Add `/budgets` route to `xvision-dashboard`

**Files:**
- Create: `crates/xvision-dashboard/src/routes/budgets.rs`
- Modify: `crates/xvision-dashboard/src/lib.rs` (or wherever 2d defines its `Router::new()...`)
- Create: `crates/xvision-dashboard/src/templates/budgets.html`

- [ ] **Step 1: Read Plan 2d's existing router and locate where to register a new route**

```bash
grep -rn "Router::new\|\\.route(" crates/xvision-dashboard/src/
```

Identify the file where 2d's existing routes (e.g., `/`, `/authoring/:id`, `/live/:deployment_id`) are wired. The new `/budgets` route attaches to that same Router.

- [ ] **Step 2: Implement `routes/budgets.rs`**

```rust
// crates/xvision-dashboard/src/routes/budgets.rs
use askama::Template;
use axum::{extract::{State, Path}, response::Html, Json};
use sqlx::SqlitePool;
use serde::Deserialize;
use anyhow::Result;

use crate::AppState; // or whatever 2d names its shared state

#[derive(Template)]
#[template(path = "budgets.html")]
struct BudgetsTemplate {
    rows: Vec<BudgetRow>,
    total_committed_usdc: f64,
    collateral_usdc: f64,
}

pub struct BudgetRow {
    pub agent_id: String,
    pub hard_cap: f64,
    pub slippage_bps: u32,
    pub orders_per_minute: u32,
    pub active_hours: String,
    pub margin_mode: String,
    pub quota_factor: f64,
    pub status: String,
}

pub async fn list(State(state): State<AppState>) -> Html<String> {
    // Read all strategy configs (from disk or a future strategies table).
    // For each, compute current quota_factor, in-flight notional, status.
    // Fetch user collateral total from Orderly account-info.
    let rows: Vec<BudgetRow> = collect_rows(&state).await.unwrap_or_default();
    let total = rows.iter().map(|r| r.hard_cap).sum();
    let collateral = state.orderly.account_collateral_usdc().await.unwrap_or(0.0);
    let tpl = BudgetsTemplate { rows, total_committed_usdc: total, collateral_usdc: collateral };
    Html(tpl.render().expect("render"))
}

#[derive(Deserialize)]
pub struct UpdateBody { pub field: String, pub value: String }

pub async fn update(
    Path(agent_id): Path<String>,
    State(state): State<AppState>,
    Json(body): Json<UpdateBody>,
) -> axum::http::StatusCode {
    // Whitelist `field`; validate `value`; read current StrategyConfig;
    // apply change; write policy_changes row; persist new config.
    match apply_update(&state, &agent_id, &body).await {
        Ok(()) => axum::http::StatusCode::OK,
        Err(_) => axum::http::StatusCode::BAD_REQUEST,
    }
}

async fn collect_rows(state: &AppState) -> Result<Vec<BudgetRow>> {
    // ... ~30 lines: load configs + status + quota + in-flight ...
    todo!()
}

async fn apply_update(state: &AppState, agent_id: &str, body: &UpdateBody) -> Result<()> {
    // ... ~25 lines: validate field is in allowlist, parse value to typed field,
    //                read current config, journal change to policy_changes, write back ...
    todo!()
}
```

- [ ] **Step 3: Register the route in 2d's Router**

In the dashboard's router setup file (the one identified in Step 1), add:

```rust
use crate::routes::budgets;

let app = Router::new()
    // ...existing 2d routes...
    .route("/budgets", get(budgets::list))
    .route("/budgets/:strategy", post(budgets::update))
    .with_state(state);
```

Add `pub mod budgets;` to `crates/xvision-dashboard/src/routes/mod.rs` (create if absent, following 2d's module convention).

- [ ] **Step 4: Add a navigation entry**

Plan 2d's dashboard likely renders a nav bar. Add a "Budgets" link pointing at `/budgets` next to the Wizard / Inspector / Live cockpit links so the operator can reach it from anywhere.

### Task 8.3: Spreadsheet template (budgets.html)

- [ ] **Step 1: budgets.html (the spreadsheet) — lives at `crates/xvision-dashboard/src/templates/budgets.html`**

```html
<!DOCTYPE html>
<html>
<head>
  <title>Strategy Budgets</title>
  <style>
    body { font-family: ui-monospace, monospace; padding: 2rem; }
    table { width: 100%; border-collapse: collapse; }
    th, td { border-bottom: 1px solid #ddd; padding: .5rem; text-align: left; }
    th { background: #f5f5f5; cursor: pointer; }
    td.editable { background: #fffef0; }
    td.editable:hover { background: #fff7c2; cursor: text; }
    .status-active { color: #0a0; }
    .status-halted_auto, .status-halted_manual { color: #c00; }
    .row-aggregate { font-weight: bold; background: #fafafa; }
    .warn { color: #d80; }
  </style>
</head>
<body>
  <h1>Strategy Budgets</h1>
  <p>Edit any cell. Saves on blur with confirm. Status + Quota are read-only.</p>
  <table id="budgets">
    <thead>
      <tr>
        <th data-sort="agent_id">Strategy</th>
        <th data-sort="hard_cap">Hard Cap</th>
        <th data-sort="slippage_bps">Slippage</th>
        <th data-sort="orders_per_minute">Orders/min</th>
        <th data-sort="active_hours">Active Hours</th>
        <th data-sort="margin_mode">Mode</th>
        <th data-sort="quota_factor">Quota</th>
        <th data-sort="status">Status</th>
        <th>Actions</th>
      </tr>
    </thead>
    <tbody>
      {% for r in rows %}
      <tr data-strategy="{{ r.agent_id }}">
        <td>{{ r.agent_id }}</td>
        <td class="editable" data-field="hard_cap">${{ r.hard_cap }}</td>
        <td class="editable" data-field="slippage_bps">{{ r.slippage_bps }} bps</td>
        <td class="editable" data-field="orders_per_minute">{{ r.orders_per_minute }}</td>
        <td class="editable" data-field="active_hours">{{ r.active_hours }}</td>
        <td class="editable" data-field="margin_mode">{{ r.margin_mode }}</td>
        <td>{{ r.quota_factor }}</td>
        <td class="status-{{ r.status }}">{{ r.status }}</td>
        <td>
          <button onclick="kill('{{ r.agent_id }}')">kill</button>
          {% if r.status != "active" %}<button onclick="unhalt('{{ r.agent_id }}')">unhalt</button>{% endif %}
        </td>
      </tr>
      {% endfor %}
      <tr class="row-aggregate">
        <td>TOTAL</td>
        <td>${{ total_committed_usdc }}</td>
        <td colspan="7"></td>
      </tr>
    </tbody>
  </table>
  <script>
    // Inline-edit + confirm modal + sortable column handlers.
    // Plain ES modules; no bundler. Detail in Task 8.3.
  </script>
</body>
</html>
```

- [ ] **Step 2: Test (axum integration with a seeded DB)**

```rust
// crates/xvision-dashboard/tests/budgets_route_test.rs
#[tokio::test]
async fn list_renders_seeded_budgets() {
    // Seed pool with 3 strategy_status rows + 3 fake budget configs.
    // Build a Router with just the budgets routes attached.
    // Send GET /budgets via tower::ServiceExt::oneshot.
    // Assert HTML body contains all three agent_ids.
}
```

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-dashboard/
git commit -m "feat(dashboard): /budgets spreadsheet view route + template"
```

### Task 8.4: Inline edit + confirm modal

**Files:**
- Modify: `crates/xvision-dashboard/src/templates/budgets.html` (extend script block)
- Create: `crates/xvision-dashboard/src/templates/budgets_confirm_partial.html`

- [ ] **Step 1: Inline edit JS**

Add to the `<script>` block in `budgets.html`:

```javascript
document.querySelectorAll('td.editable').forEach(td => {
  td.contentEditable = true;
  td.dataset.original = td.innerText.trim();
  td.addEventListener('blur', async () => {
    const newVal = td.innerText.trim();
    if (newVal === td.dataset.original) return;
    const strategy = td.closest('tr').dataset.strategy;
    const field = td.dataset.field;
    const ok = confirm(`Change ${field} for ${strategy}: ${td.dataset.original} → ${newVal}?`);
    if (!ok) { td.innerText = td.dataset.original; return; }
    const r = await fetch(`/budgets/${strategy}`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ field, value: newVal }),
    });
    if (!r.ok) { alert('save failed'); td.innerText = td.dataset.original; return; }
    td.dataset.original = newVal;
    td.style.background = '#cfc';
    setTimeout(() => td.style.background = '', 800);
  });
});
```

- [ ] **Step 2: POST /budgets/:strategy implementation**

Body parses `{field, value}`, looks up current config, validates the field (whitelist), writes a `policy_changes` row, applies the change, returns 200.

- [ ] **Step 3: Test**

```rust
#[tokio::test]
async fn post_change_writes_policy_change_row() { /* ... */ }
```

- [ ] **Step 4: Commit**

```bash
git commit -m "feat(ui): inline-edit cells with confirm + policy_changes journaling"
```

### Task 8.5: Aggregate-row warning + bulk-edit

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/budgets.rs`, `templates/budgets.html`

- [ ] **Step 1: Pull total user collateral from Orderly account-info; render warning if total committed > 80% of collateral**

```html
{% if total_committed_usdc > collateral_usdc * 0.8 %}
<p class="warn">⚠ Total committed (${{ total_committed_usdc }}) exceeds 80% of available collateral (${{ collateral_usdc }})</p>
{% endif %}
```

- [ ] **Step 2: Bulk-edit via shift-click range select + apply**

Add JS: track shift-click range; "Apply to selection" button opens a small inline form for the chosen field; one POST per row in the selection (or a single bulk endpoint).

- [ ] **Step 3: Tests, commit**

```bash
git commit -m "feat(ui): aggregate-row warning + bulk-edit selection"
```

### Task 8.6: Wire `xvn budget serve` to launch the dashboard at the budgets route

The CLI's `xvn budget serve` is a convenience entry point that launches the existing dashboard (Plan 2d) and prints the `/budgets` URL.

**Files:**
- Modify: `crates/xvision-cli/Cargo.toml` (add `xvision-dashboard` dep)
- Modify: `crates/xvision-cli/src/commands/budget.rs` (`Serve` arm)

- [ ] **Step 1: Replace the placeholder in Task 4.8 Step 5**

```rust
BudgetCmd::Serve { addr } => {
    let ctx = crate::context::AppContext::from_env().await?;
    println!("Launching dashboard. Open http://{addr}/budgets");
    xvision_dashboard::serve(&addr, ctx.into_dashboard_state()).await?;
}
```

- [ ] **Step 2: Manual verification**

```bash
cargo run -p xvision-cli -- budget serve --addr 127.0.0.1:7878
# Open http://127.0.0.1:7878/budgets, edit a cell, verify policy_changes row written.
```

- [ ] **Step 3: Commit**

```bash
git commit -m "feat(cli): xvn budget serve launches dashboard at /budgets"
```

---

### Fallback flow (only if Plan 2d slipped past 2026-06-01)

If Task 8.1 finds no `crates/xvision-dashboard`, do this instead:

1. Add `crates/xvision-budget-ui/` to workspace `members` (and append `axum`, `askama`, `tower-http` to workspace deps if not present from Plan 2d).
2. Create `crates/xvision-budget-ui/Cargo.toml` with `axum`, `askama`, `tower-http`, `tokio`, `sqlx`, `xvision-data`, `xvision-risk` as deps.
3. Create `crates/xvision-budget-ui/src/lib.rs` with a `pub async fn serve(addr, pool) -> Result<()>` that builds a `Router::new().route("/budgets", get(routes::list)).route("/budgets/:strategy", post(routes::update))`.
4. Move the same `routes/budgets.rs` and `templates/budgets.html` from Tasks 8.2–8.4 into the standalone crate (under `src/routes.rs` and `src/templates/`).
5. `xvn budget serve` calls `xvision_budget_ui::serve(&addr, pool)` instead of the dashboard.
6. **When Plan 2d ships:** lift `routes/budgets.rs` and `templates/budgets.html` into `crates/xvision-dashboard/src/routes/` and `crates/xvision-dashboard/src/templates/` respectively (zero code changes — they're already structured for it). Delete `crates/xvision-budget-ui/` and remove from workspace `members`. Update `xvn budget serve` to call `xvision_dashboard::serve` per Task 8.6.

The fallback is intentionally short — it's the same routes + templates wrapped in a tiny `serve()` function, so the eventual lift is mechanical.

---

## Phase 9 — Documentation + Final Integration Test

End of phase: end-to-end story is documented and a single integration test exercises the whole pipeline against Orderly testnet.

### Task 9.1: End-to-end integration test

**Files:**
- Create: `crates/xvision-execution/tests/e2e_orderly_testnet.rs` (gated behind `#[ignore]` so it doesn't run in default CI)

- [ ] **Step 1: Test outline**

```rust
//! Run with: cargo test -p xvision-execution e2e -- --ignored --nocapture
//! Requires: ORDERLY_TESTNET_KEY, ORDERLY_TESTNET_ACCOUNT_ID env vars.

#[tokio::test]
#[ignore]
async fn end_to_end_full_dispatch_against_testnet() {
    // 1. Bootstrap pool, run migrations.
    // 2. Build full AppContext with real OrderlyExecutor (testnet endpoint).
    // 3. Insert one strategy_status row + one StrategyConfig (small caps).
    // 4. Construct a TraderDecision and dispatch.
    // 5. Assert: position row created, all 6 stages in audit log, Orderly fill returned.
    // 6. Run reconcile, assert no drift.
    // 7. Run xvn emergency-close --strategy <id>, assert position closed in ledger and on Orderly.
}
```

- [ ] **Step 2: Run it manually, capture log to results dir, commit**

```bash
cargo test -p xvision-execution e2e -- --ignored --nocapture 2>&1 | tee tests/e2e_$(date -u +%Y%m%dT%H%M%SZ).log
git add crates/xvision-execution/tests/
git commit -m "test(e2e): full pipeline against Orderly testnet (ignored by default)"
```

### Task 9.2: Operator runbook addendum

**Files:**
- Modify: `MANUAL.md` (append the wallet section)

- [ ] **Step 1: Add a "Wallet & Budget Operations" section**

Cover: first-time key issuance, daily budget review (`xvn budget show`), kill flow (`xvn kill`), emergency-close flow with worked example, approval-gate response, audit-log lookup for incidents, reconcile-drift triage.

- [ ] **Step 2: Commit**

```bash
git add MANUAL.md
git commit -m "docs(manual): wallet + budget operations runbook"
```

### Task 9.3: Update FOLLOWUPS.md to mark wallet items shipped

- [ ] **Step 1: Cross off SLF-related wallet items, add follow-ups for post-hackathon (multi-tenant, MPC migration, dashboard fold-in)**

- [ ] **Step 2: Commit**

```bash
git add FOLLOWUPS.md
git commit -m "docs(followups): mark non-custodial wallets v1 shipped; record post-hackathon follow-ups"
```

---

## Self-review

**Spec coverage check:**

| Spec section | Plan coverage |
|---|---|
| §1.1 Validation gates G1, G2 | Phase 0 Tasks 0.2–0.4 |
| §2 Two-rail architecture | Implicit; Phase 0 ADRs document the trading rail; marketplace rail = Plan 5 (out of scope here, cross-referenced) |
| §3.1 User-side onboarding | Out of scope (user does it); referenced in §3.2 |
| §3.2 Trading-key issuance + phishing-resistant UX | Phase 4 Task 4.7 (`xvn key issue`) |
| §3.3 OrderDispatcher | Phase 3 Task 3.2 |
| §3.4 Risk Engine: scoped permissions, hard caps | Phase 2 Tasks 2.1–2.2 |
| §3.4 Risk Engine: dynamic quota | Phase 6 |
| §3.4 Risk Engine: reservations | Phase 2 Task 2.3 |
| §3.4 Risk Engine: aggregate margin / margin_mode | Phase 5 Tasks 5.1 / 5.2 (conditional on G2) |
| §3.4 UI Strategy Budgets spreadsheet | Phase 8 — adds `/budgets` route + spreadsheet template to `xvision-dashboard` (Plan 2d). Standalone fallback documented if 2d slips. |
| §3.5 Attribution ledger + funding attributions | Phase 1 Tasks 1.1, 1.3 |
| §3.6 Marketplace fee router | Cross-referenced to Plan 5; not implemented here |
| §3.7 Settlement wallet | Operator-side, manual; documented in Phase 9 Task 9.2 |
| §3.8 Audit log + pre-trade simulation | Phase 1 Task 1.2 (audit), Phase 3 Task 3.1 (simulate) |
| §3.9 Kill switches + approval gates + emergency-close + CLI | Phase 4 Tasks 4.1–4.11 |
| §4 Data flow | Implicit in dispatcher; Phase 3 |
| §5 Security model | Threat-relevant code in Phases 4 (kill switches), 7 (reconciliation); ADRs in Phase 0 |
| §6 Failure modes | Tests in Phase 2 (reservations), 3 (dispatcher), 6 (quota proptest), 7 (reconcile) |
| §7 Component map | Mirrored in this plan's "File structure" |
| §8 Migration steps | The plan's Phases 0–9 (steps 1–6 of spec; step 7 multi-tenant explicitly deferred) |
| §9 Testing | Per-phase tests; Phase 9 e2e |
| §10 Deferred | Cross-referenced; not implemented here |
| §11 References | Linked from spec; not duplicated |

**Placeholder scan:**

- Phase 0 Task 0.2 has `todo!()` markers in the m1 probe scaffold — these are implemented in Step 5 of the same task. Acceptable (the plan explicitly walks through implementing them).
- Phase 3 Task 3.2 dispatcher has `unimplemented!()` for `request_approval` — explicitly forward-referenced to Phase 4 Task 4.5 which replaces it. Acceptable.
- Phase 7 Task 7.1 reconciler body is `todo!()` with bullet-pointed implementation steps; the inline plan steps describe the diff logic. **Tighten if more detail needed during execution; OK for plan-stage.**
- No "TBD" / "TODO comments meant as placeholders" / "implement later" in plan prose itself.

**Type consistency check:**

- `Stage` enum used in `audit.rs` (Task 1.2) and dispatcher (Task 3.2) — variants match: `Emit, RiskEval, Simulate, Sign, Submit, Response, Fill, Close, Cancel, Reject`.
- `Side` enum (`Long`, `Short`) used in ledger and dispatcher; consistent.
- `Verdict` enum (`Approved, RequiresApproval, Vetoed`) defined in Task 2.2 and matched in dispatcher Task 3.2.
- `StrategyConfig` field names match between Task 2.1 (definition) and Task 2.2 (consumer).
- `quota_factor` constant names match between spec §3.4 and Phase 6 Task 6.1.
- `ReservationManager::try_reserve` signature `(user_id, agent_id, notional, cap)` consistent across Task 2.3 (definition), Task 3.2 (dispatcher consumer), Task 6.1 step 4 (quota wiring).

No mismatches found.

---

**Plan complete and saved to `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md`.**
