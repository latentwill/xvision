# Byreal Mainnet Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make real-money mainnet Byreal trade through the *same single execution path* as live/eval/optimizer (`Arc<dyn BrokerSurface>` → engine executor → `RealBrokerFills`), with the safety gate actually firing before funds move — eliminating every ungated real-money path (the `fire-trade` and `close-position` CLI bypasses).

**Architecture:** The engine already has one parity execution path: `Executor::live()` → `RealBrokerFills` → `Arc<dyn BrokerSurface>`. Byreal's parity member (`ByrealLiveSurface`) exists and is already mainnet-capable; it's only *gated* to testnet by two guards. This plan introduces a **`GatedBrokerSurface` decorator** that wraps any `Arc<dyn BrokerSurface>` and calls the (currently-dead) `SafetyGate::check_broker_submit` inside `submit_order` before delegating — making the gate fire for every venue at one seam with **zero changes to `Executor::live()` or its ~30 callers**. It then (2) relaxes the two guards so `venue_label=Live` Byreal mainnet runs flow through the parity builder, (3) adds a guarded `xvn live` CLI verb that sets `venue_label=Live`, and (4) routes the two CLI submit bypasses (`fire-trade`, `close-position`) through the gate + a real-money confirmation. No real funds move in this push — verification is testnet + an engine-local mock broker mainnet-labeled dry-run.

**Tech Stack:** Rust workspace (`xvision-engine`, `xvision-execution`, `xvision-cli`, `xvision-dashboard`), `async_trait`, `tokio`, SQLx/SQLite, `@byreal-io/byreal-perps-cli` via `npx`. TDD with `scripts/cargo test`.

**Decisions locked with operator (2026-06-14):**
- Bypass path → **re-route** the CLI submit paths through the parity stack (keep manual entry, remove bypass); not full `Executor`-trait retirement (Q1).
- Safety gate → wire **once at the shared broker-submit boundary**, all venues (Q2).
- Launch surface → **guarded CLI verb now**, `/live` dashboard surface as a follow-up (Q3).
- This push → **code path + dry-run/testnet only**; no real funds until explicit green-light (Q4).

**Revision note (post plan-review-gate iteration 1):** three adversarial reviewers found four true blocking issues, all fixed below: (F1) fabricated `AuthContext::from(Actor)` → use `AuthContext::system()`; (F2) non-existent `MockByrealApi` helpers + cross-crate `#[cfg(test)]` scope → use engine-local mock `BrokerSurface`; (C1) `AppState::api_context()` never wired the gate → now in scope (`state.rs`) with a test; (C2) the ~30-caller `Executor::live()` cascade → eliminated by the decorator (signature unchanged); (C3) gate only blocked `Paper→Live`, leaving `Testnet→Live` (unset `BYREAL_NETWORK` defaults mainnet) open → gate broadened to `non-Live run → Live broker` + a `venue_label↔network` consistency check; (S1) ungated `xvn close-position --venue byreal` → now gated (Phase 4).

**Revision note (post iteration 2):** Feasibility + Scope PASS; Completeness flagged two TDD test-code bugs, both fixed: Task 1.1's red-step test used `SafetyGate::allow_all()` (short-circuits before the mismatch check → false green) → now `SafetyGate::new(open_manager().await)`; Task 1.3's test used a non-existent `AppState::for_test()` → now `AppState::new(tempdir)`. Also made the decorator forward all defaulted `BrokerSurface` methods.

**Evaluate/implement against `origin/main`.** A worktree at `origin/main` (tip `d815f05c` / #1048 at time of writing) already exists at `/Users/edkennedy/Code/xvision/.worktrees/byreal-mainnet-parity`. Implement there.

---

## Pre-flight facts (verified against the origin/main worktree — do not re-derive)

| Fact | Location |
|---|---|
| Parity executor consumes `Arc<dyn BrokerSurface>` | `crates/xvision-engine/src/eval/executor/backtest.rs:227`; live broker built into `RealBrokerFills::new(broker)` at `:248` |
| Live submit chokepoint (wraps `broker.submit_order`; error → `rejected_no_fill`) | `crates/xvision-engine/src/eval/executor/real_broker_fills.rs:57` (struct), `:83` (`FillSink::submit`), `:392` (`rejected_no_fill`), `:371` (`noop_fill_record`) |
| Safety gate — **dead** (only `tests/safety_gate.rs` calls it; NO production caller) | `crates/xvision-engine/src/safety/gate.rs:125` `check_broker_submit(&self, auth:&AuthContext, venue:&str, asset:Option<&str>, notional_usd:Option<f64>, run_venue_label:VenueLabel, broker_venue_label:VenueLabel, limits:Option<&SafetyLimits>, limit_check:Option<&SafetyLimitCheck>)`; mismatch check at `:157` |
| Gate constructors | `gate.rs:89` `new(manager)`, `:99` `allow_all()` |
| `AuthContext` has only `system()` and `api_anonymous()` — **no** `From<Actor>` | `crates/xvision-engine/src/safety/auth.rs:21` (struct), `:31` `system()`, `:38` `api_anonymous()` |
| `BrokerSurface` methods: `submit_order`, `position`, `balance`, `venue`, `signing_scheme`, `is_perp_venue` — **no** `venue_label()` | `crates/xvision-execution/src/byreal.rs:625` (impl); `OrderRequest` at `broker_surface.rs:90` (`asset, side, size, reference_price_usd, stop_loss_pct, take_profit_pct, idempotency_key`) |
| Byreal testnet guard | `crates/xvision-engine/src/api/eval.rs:3532-3551` (`resolve_live_venue("byreal",..)` arm); sig at `:3506-3509` |
| `venue_label=Live` rejection | `crates/xvision-engine/src/eval/live_config.rs:231`; Alpaca-crypto whitelist on assets at `:191-197` |
| Live builder (validates, resolves venue, builds broker, requires Alpaca creds for market data; `broker_override` param) | `crates/xvision-engine/src/api/eval.rs:3563` `build_live_executor`; `broker_override` at `:3566`; broker match at `:3637-3660`; callers at `:2922`, `:3904` |
| Byreal parity surface, already mainnet-capable (`BYREAL_NETWORK` default `mainnet`, no `--network` ⇒ mainnet) | `crates/xvision-execution/src/byreal.rs:601` (`from_env`), `:397` (`from_env` default) |
| `ApiContext` has **no** safety field today | `crates/xvision-engine/src/api/mod.rs:298` |
| Server holds the `SafetyManager` singleton; `api_context()` builds `ApiContext` but does **not** pass it | `crates/xvision-dashboard/src/state.rs:110` (field), `:366` (`SafetyManager::new`), `:455` (`safety_manager()` accessor), `:488` (`api_context()` → `ApiContext::new(...)`) |
| CLI bypass family — `Executor` trait + `*Executor` structs; **submit-capable** consumers = `fire_trade.rs` (submit) + `venue.rs` (`close_position` → `close_market`) | `fire_trade.rs:115`; `venue.rs:33` (`executor_from_env`), `:60-66` (`close_position` → `exec.close_position`); `byreal.rs:221-233` (`close_position` → `api.close_market`) |
| `api/live_broker.rs` uses `OrderlyExecutor` **read-only** (account snapshot) — not a submit path | `crates/xvision-engine/src/api/live_broker.rs:83` |
| Reusable deterministic perps veto | `crates/xvision-engine/src/strategies/risk/perps.rs:22` `perps_entry_veto(cfg:&RiskConfig, is_perp_venue:bool, is_new_open:bool, direction:Direction, funding_rate_8h:Option<f64>, min_position_liq_distance_pct:Option<f64>) -> Option<VetoReason>` |
| `fire-trade` already on the remote-CLI denylist | `crates/xvision-dashboard/src/cli_jobs/allowlist.rs:168` |

**Constraint (document only, no code change):** Alpaca supplies the live market-data stream for *every* venue including Byreal (`build_live_executor` requires Alpaca creds). A Byreal mainnet run's assets must be on the Alpaca crypto whitelist (BTC/ETH/SOL/etc.) to get bars — the `validate()` whitelist check is correct and stays.

**Out of scope (note as follow-ups):** retiring the `Executor` trait + `*Executor` structs entirely (still used by read-only `live_broker.rs` + `venue.rs portfolio` + the gated `close_position`); the `/live` dashboard real-money launcher; migrating `close_position` to a first-class `BrokerSurface::close` (this push gates it instead).

---

## Task 0: Worktree + baseline

**Files:** none (environment).

- [ ] **Step 1: Use the existing origin/main worktree**

```bash
cd /Users/edkennedy/Code/xvision/.worktrees/byreal-mainnet-parity
git switch -c feat/byreal-mainnet-parity   # currently detached at origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git log --oneline -1   # expect d815f05c (#1048) or later
```

- [ ] **Step 2: Capture the pre-existing baseline** (so known-red tests aren't attributed to us)

Run: `scripts/cargo test -p xvision-engine -p xvision-execution -p xvision-cli -p xvision-dashboard --no-run`
Expected: compiles. Record any pre-existing failures now.

---

## Phase 1 — Wire the safety gate at the shared boundary (all venues)

`SafetyGate::check_broker_submit` is dead. We add a `GatedBrokerSurface` decorator (engine crate, so it can use `SafetyGate`) that wraps any live broker and calls the gate inside `submit_order` before delegating. Wrapping happens in `build_live_executor`, so every live run of every venue is gated at one seam — `Executor::live()` and its ~30 callers are untouched.

### Task 1.1: Broaden the gate so a non-Live run can never hit a Live broker

**Files:**
- Modify: `crates/xvision-engine/src/safety/gate.rs:157-164`
- Test: `crates/xvision-engine/tests/safety_gate.rs`

- [ ] **Step 1: Write a failing test for the Testnet→Live block**

Add to `crates/xvision-engine/tests/safety_gate.rs` (mirror the existing `check_broker_submit` test setup there):

```rust
#[tokio::test]
async fn testnet_run_against_live_broker_is_rejected() {
    // A non-Live run (Testnet here) must never reach a Live (real-money)
    // broker — mirrors the existing Paper→Live block, generalized.
    // IMPORTANT: `SafetyGate::allow_all()` short-circuits with `Ok(())` BEFORE
    // the mismatch check (gate.rs:136-138), so it would false-green this test.
    // Use a real, UNPAUSED manager-backed gate so the Testnet→Live mismatch
    // branch is actually exercised (and so Step 2 genuinely fails pre-fix).
    // `open_manager()` is the unpaused-manager helper already used throughout
    // `safety_gate.rs` (e.g. the existing check_broker_submit tests).
    let gate = SafetyGate::new(open_manager().await);
    let err = gate
        .check_broker_submit(
            &AuthContext::system(),
            "byreal",
            Some("BTC/USD"),
            Some(100.0),
            VenueLabel::Testnet, // run
            VenueLabel::Live,    // broker
            None,
            None,
        )
        .await
        .unwrap_err();
    assert!(matches!(err, SafetyGateError::VenueLabelMismatch { .. }));
}
```

> `open_manager()` (unpaused `SafetyManager`) is confirmed present in `crates/xvision-engine/tests/safety_gate.rs` and is the form every existing `check_broker_submit` test uses. Keep the existing `paper_run_against_live_broker_is_rejected` test passing (Paper→Live is a subset of the broadened check).

- [ ] **Step 2: Run, verify failure**

Run: `scripts/cargo test -p xvision-engine --test safety_gate testnet_run_against_live_broker`
Expected: FAIL (currently only `Paper` is blocked).

- [ ] **Step 3: Generalize the mismatch check**

`gate.rs:157`:

```rust
        // A run that is not itself Live must never reach a Live (real-money)
        // broker. Covers Paper→Live (paper run) and Testnet→Live (a testnet
        // run whose env resolved to a mainnet broker, e.g. BYREAL_NETWORK unset
        // defaulting to mainnet). Live→Live is allowed; the launch surface +
        // build_live_executor consistency check (Task 2.3) keep labels honest.
        if run_venue_label != VenueLabel::Live && broker_venue_label == VenueLabel::Live {
            self.write_audit(auth, &action, AuditResult::DeniedVenueMismatch, pause_state)
                .await;
            return Err(SafetyGateError::VenueLabelMismatch {
                scenario_label: run_venue_label,
                broker_label: broker_venue_label,
            });
        }
```

- [ ] **Step 4: Run both mismatch tests, verify pass.** Run: `scripts/cargo test -p xvision-engine --test safety_gate` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/safety/gate.rs crates/xvision-engine/tests/safety_gate.rs
git commit -m "fix(safety): block any non-Live run from hitting a Live broker (Paper+Testnet → Live)"
```

### Task 1.2: Create the `GatedBrokerSurface` decorator

**Files:**
- Create: `crates/xvision-engine/src/eval/executor/gated_broker.rs`
- Modify: `crates/xvision-engine/src/eval/executor/mod.rs` (`mod gated_broker; pub use gated_broker::GatedBrokerSurface;`)
- Test: in `gated_broker.rs` `#[cfg(test)]` (engine-local mock `BrokerSurface` — NO dependency on the execution crate's `#[cfg(test)]` `MockByrealApi`)

- [ ] **Step 1: Write failing tests with an engine-local recording mock broker**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    use async_trait::async_trait;
    use xvision_execution::broker_surface::{BrokerSurface, OrderRequest, OrderConfirmation, Side};
    use crate::safety::{SafetyGate, AuthContext};
    use crate::safety::venue::VenueLabel;

    #[derive(Default, Clone)]
    struct RecordingBroker { calls: Arc<Mutex<u32>>, perp: bool }
    #[async_trait]
    impl BrokerSurface for RecordingBroker {
        async fn submit_order(&self, _req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
            *self.calls.lock().unwrap() += 1;
            Ok(OrderConfirmation { broker_order_id: "ok".into(), fill_price: Some(100.0), fill_size: 1.0, fee: None })
        }
        async fn position(&self, _a: &str) -> anyhow::Result<f64> { Ok(0.0) }
        async fn balance(&self) -> anyhow::Result<f64> { Ok(1000.0) }
        fn venue(&self) -> &str { "byreal" }
        fn signing_scheme(&self) -> &str { "cli" }
        fn is_perp_venue(&self) -> bool { self.perp }
    }

    fn order() -> OrderRequest {
        OrderRequest { asset: "BTC/USD".into(), side: Side::Buy, size: 1.0,
            reference_price_usd: 100.0, stop_loss_pct: None, take_profit_pct: None,
            idempotency_key: "k".into() }
    }

    #[tokio::test]
    async fn paused_gate_blocks_and_does_not_delegate() {
        let inner = RecordingBroker::default();
        let calls = inner.calls.clone();
        let g = GatedBrokerSurface::new(
            Arc::new(inner), paused_gate().await, VenueLabel::Live, VenueLabel::Live, AuthContext::system());
        assert!(g.submit_order(order()).await.is_err());
        assert_eq!(*calls.lock().unwrap(), 0, "paused submit must not reach the inner broker");
    }

    #[tokio::test]
    async fn allowed_gate_delegates() {
        let inner = RecordingBroker::default();
        let calls = inner.calls.clone();
        let g = GatedBrokerSurface::new(
            Arc::new(inner), SafetyGate::allow_all(), VenueLabel::Live, VenueLabel::Live, AuthContext::system());
        assert!(g.submit_order(order()).await.is_ok());
        assert_eq!(*calls.lock().unwrap(), 1);
    }
}
```

> `paused_gate()` helper: build a `SafetyGate::new(manager)` over a `SafetyManager` set to paused. Reuse / mirror the helper in `crates/xvision-engine/tests/safety_gate.rs` (lift it into a small `#[cfg(test)]` fn here, or a shared test-support module).

- [ ] **Step 2: Run, verify failure (module missing).** Run: `scripts/cargo test -p xvision-engine gated_broker` → FAIL (no `GatedBrokerSurface`).

- [ ] **Step 3: Implement the decorator**

```rust
//! `GatedBrokerSurface` — wraps any live `BrokerSurface` and runs the engine
//! `SafetyGate` before delegating `submit_order`. The single production seam
//! where `SafetyGate::check_broker_submit` fires: build_live_executor wraps
//! every live broker (every venue) in this, so global pause + non-Live-run→
//! Live-broker mismatch + per-run limits are enforced before any real order
//! leaves the process. Read paths (position/balance) and metadata pass through.

use std::sync::Arc;
use async_trait::async_trait;
use xvision_execution::broker_surface::{BrokerSurface, OrderRequest, OrderConfirmation};
use crate::safety::{SafetyGate, AuthContext};
use crate::safety::venue::VenueLabel;

pub struct GatedBrokerSurface {
    inner: Arc<dyn BrokerSurface>,
    gate: SafetyGate,
    run_venue_label: VenueLabel,
    broker_venue_label: VenueLabel,
    auth: AuthContext,
}

impl GatedBrokerSurface {
    pub fn new(
        inner: Arc<dyn BrokerSurface>,
        gate: SafetyGate,
        run_venue_label: VenueLabel,
        broker_venue_label: VenueLabel,
        auth: AuthContext,
    ) -> Self {
        Self { inner, gate, run_venue_label, broker_venue_label, auth }
    }
}

#[async_trait]
impl BrokerSurface for GatedBrokerSurface {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let notional = (req.size * req.reference_price_usd).abs();
        self.gate
            .check_broker_submit(
                &self.auth,
                self.inner.venue(),
                Some(req.asset.as_str()),
                Some(notional),
                self.run_venue_label,
                self.broker_venue_label,
                None, // per-run limits: Task 1.4 (optional)
                None,
            )
            .await
            .map_err(|e| anyhow::anyhow!("safety_gate_denied: {e}"))?;
        self.inner.submit_order(req).await
    }
    async fn position(&self, asset: &str) -> anyhow::Result<f64> { self.inner.position(asset).await }
    async fn balance(&self) -> anyhow::Result<f64> { self.inner.balance().await }
    fn venue(&self) -> &str { self.inner.venue() }
    fn signing_scheme(&self) -> &str { self.inner.signing_scheme() }
    fn is_perp_venue(&self) -> bool { self.inner.is_perp_venue() }
}
```

> A gate denial returns `Err` from `submit_order`; `RealBrokerFills::submit` already classifies broker errors into a `rejected_no_fill` (`real_broker_fills.rs:392`) — so funds don't move and the run continues. **Forward EVERY `BrokerSurface` method to `inner` except `submit_order`** — including any with trait defaults (e.g. a defaulted `buying_power`), so the decorator is fully transparent and never silently substitutes the default for the inner broker's behavior. Confirm the exact trait method set against `crates/xvision-execution/src/broker_surface.rs` (trait def) and add any forwarder omitted above.

- [ ] **Step 4: Run tests, verify pass.** Run: `scripts/cargo test -p xvision-engine gated_broker` → 2 PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/executor/gated_broker.rs crates/xvision-engine/src/eval/executor/mod.rs
git commit -m "feat(safety): GatedBrokerSurface decorator — fires SafetyGate before any live submit"
```

### Task 1.3: Carry `SafetyGate` on `ApiContext` and wire it in `AppState`

**Files:**
- Modify: `crates/xvision-engine/src/api/mod.rs:298` (`ApiContext` field) + `ApiContext::new` (default `allow_all`) + a builder `with_safety_gate`
- Modify: `crates/xvision-dashboard/src/state.rs:488` (`api_context()` → set the real gate)
- Modify: `crates/xvision-engine/src/safety/gate.rs` (add `pub fn is_enforcing(&self) -> bool { self.manager.is_some() }`)
- Test: `crates/xvision-dashboard/src/state.rs` `#[cfg(test)]` (or an existing dashboard test module)

- [ ] **Step 1: Add `is_enforcing()` to `SafetyGate`** (so a test can assert the production gate is real, not allow-all):

```rust
    /// True when this gate is backed by a real SafetyManager (enforcing),
    /// false for `allow_all()`.
    pub fn is_enforcing(&self) -> bool { self.manager.is_some() }
```

- [ ] **Step 2: Add the field + builder to `ApiContext`**

```rust
pub struct ApiContext {
    // ...existing fields...
    pub safety_gate: crate::safety::SafetyGate,
}
```
In `ApiContext::new(...)`, default `safety_gate: crate::safety::SafetyGate::allow_all()`. Add:
```rust
    pub fn with_safety_gate(mut self, gate: crate::safety::SafetyGate) -> Self {
        self.safety_gate = gate; self
    }
```

- [ ] **Step 3: Write a failing test that the production `api_context()` carries a real gate**

```rust
#[tokio::test]
async fn api_context_carries_enforcing_safety_gate() {
    // AppState::new(xvn_home) is the real constructor (state.rs:274); dashboard
    // integration tests use it with a tempdir (see tests/chat_rail_routes.rs).
    let tmp = tempfile::tempdir().unwrap();
    let state = AppState::new(tmp.path().to_path_buf()).await.unwrap();
    assert!(state.api_context().safety_gate.is_enforcing(),
        "production ApiContext must wire the real SafetyManager, not allow_all");
}
```

> Constructor confirmed: `AppState::new(PathBuf)` at `state.rs:274` (async, returns a Result), used with a tempdir by the existing dashboard integration tests. There is no `for_test`.

- [ ] **Step 4: Run, verify failure.** Run: `scripts/cargo test -p xvision-dashboard api_context_carries_enforcing` → FAIL.

- [ ] **Step 5: Wire the gate in `AppState::api_context()`** (`state.rs:488`)

```rust
    pub fn api_context(&self) -> ApiContext {
        ApiContext::new(/* ...existing args... */)
            // ...existing builder chain...
            .with_safety_gate(crate::... ::SafetyGate::new(self.safety_manager.clone()))
    }
```

> `SafetyGate` is `xvision_engine::...::SafetyGate` from the dashboard crate's perspective — confirm the import path. `SafetyManager: Clone` (it wraps a pool); confirm at `safety/state.rs`.

- [ ] **Step 6: Run tests, verify pass.** Run: `scripts/cargo test -p xvision-dashboard api_context_carries_enforcing` → PASS. Also `scripts/cargo test -p xvision-engine` to confirm `ApiContext::new` callers still compile (new field has a default via `new`, not positional).

- [ ] **Step 7: Commit**

```bash
git add crates/xvision-engine/src/api/mod.rs crates/xvision-engine/src/safety/gate.rs crates/xvision-dashboard/src/state.rs
git commit -m "feat(safety): carry enforcing SafetyGate on ApiContext, wired from AppState SafetyManager"
```

### Task 1.4: Wrap the live broker with the gate in `build_live_executor`

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs:3637-3660` (after the broker match)
- Test: covered by the Phase 5 e2e (Task 5.1); add a focused unit test here if `build_live_executor` is unit-constructable.

- [ ] **Step 1: Derive the broker venue label and wrap the broker**

After the `let broker: Arc<dyn BrokerSurface> = match broker_override { ... }` block (`eval.rs:3637-3660`):

```rust
    let broker_venue_label = match venue {
        LiveVenue::AlpacaPaper => VenueLabel::Paper,
        LiveVenue::OrderlyTestnet => VenueLabel::Testnet,
        LiveVenue::ByrealLive => {
            // No --network ⇒ the perps CLI defaults to mainnet (byreal.rs:397),
            // so an unset/empty BYREAL_NETWORK is treated as Live here. The
            // gate (Task 1.1) then blocks any non-Live run from reaching it.
            match byreal_network.as_deref().map(|s| s.to_ascii_lowercase()) {
                Some(n) if n.contains("testnet") => VenueLabel::Testnet,
                _ => VenueLabel::Live,
            }
        }
    };
    let broker: Arc<dyn BrokerSurface> = Arc::new(crate::eval::executor::GatedBrokerSurface::new(
        broker,
        ctx.safety_gate.clone(),
        cfg.venue_label,           // run label (from LiveConfig)
        broker_venue_label,
        AuthContext::system(),     // engine-internal live run
    ));
```

> `ctx.safety_gate` is the field added in Task 1.3. `AuthContext::system()` is the correct identity for an automated engine live run (`auth.rs:31`). The wrapped `broker` then flows unchanged into `Executor::live(cfg, broker, ...)` — no signature change.

- [ ] **Step 2: Compile + run engine tests.** Run: `scripts/cargo test -p xvision-engine` → PASS (no `live()` caller churn — the decorator is transparent).

- [ ] **Step 3: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(safety): wrap every live broker in GatedBrokerSurface at build_live_executor"
```

> **Optional Task 1.5 (defer if time-boxed):** thread per-run `SafetyLimits`/`SafetyLimitCheck` into `GatedBrokerSurface` (currently `None`). The pause + non-Live→Live checks (1.1–1.4) are the real-money-blocking ones; limits are an enhancement.

---

## Phase 2 — Allow Byreal mainnet through the parity builder (lift the two guards)

### Task 2.1: Make `venue_label=Live` venue-aware in `LiveConfig::validate()`

**Files:**
- Modify: `crates/xvision-engine/src/eval/live_config.rs:231-233`
- Test: `crates/xvision-engine/src/eval/live_config.rs` test module

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn live_label_rejected_for_alpaca_paper_creds() {
    let cfg = cfg_with("alpaca", VenueLabel::Live);
    assert!(matches!(cfg.validate(), Err(LiveConfigValidationError::VenueLabelLiveRejected)));
}
#[test]
fn live_label_allowed_for_byreal_creds() {
    let cfg = cfg_with("byreal", VenueLabel::Live);
    assert!(cfg.validate().is_ok(), "byreal is real-money; Live is allowed");
}
```

> `cfg_with(creds, label)` builds a minimal valid `LiveConfig` (whitelisted asset, non-empty stop policy, positive capital) varying `broker_creds_ref` + `venue_label`. Model on the existing `validate()` tests in this file.

- [ ] **Step 2: Run, verify the byreal case fails.** Run: `scripts/cargo test -p xvision-engine live_config` → FAIL on `live_label_allowed_for_byreal_creds`.

- [ ] **Step 3: Make the check venue-aware** (`live_config.rs:231`)

```rust
        // Real-money `Live` is allowed only for venues that settle real funds
        // (Byreal perps / Hyperliquid). Alpaca live scope is paper only.
        const REAL_MONEY_CREDS: &[&str] = &["byreal"];
        if self.venue_label == VenueLabel::Live
            && !REAL_MONEY_CREDS.contains(&self.broker_creds_ref.as_str())
        {
            return Err(E::VenueLabelLiveRejected);
        }
```

- [ ] **Step 4: Run, verify pass.** Run: `scripts/cargo test -p xvision-engine live_config` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/eval/live_config.rs
git commit -m "feat(live): allow venue_label=Live for real-money Byreal creds, keep Alpaca paper-only"
```

### Task 2.2: Allow Byreal mainnet in `resolve_live_venue`

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs:3532-3551` ("byreal" arm) + venues help string `:3555-3557`
- Test: `crates/xvision-engine/src/api/eval.rs` test module (existing testnet-only-message test at `:4798`)

- [ ] **Step 1: Update the existing test + add mainnet/testnet resolution tests**

The current test (`eval.rs:4798`) asserts the error message contains `fire-trade --venue byreal`. Change it: mainnet Byreal now resolves (no error). Add:

```rust
#[test]
fn byreal_mainnet_resolves_to_byreal_live() {
    assert_eq!(resolve_live_venue("byreal", None, Some("mainnet")).unwrap(), LiveVenue::ByrealLive);
}
#[test]
fn byreal_testnet_resolves_to_byreal_live() {
    assert_eq!(resolve_live_venue("byreal", None, Some("testnet")).unwrap(), LiveVenue::ByrealLive);
}
```

- [ ] **Step 2: Run, verify failure.** Run: `scripts/cargo test -p xvision-engine resolve_live_venue` → FAIL.

- [ ] **Step 3: Remove the testnet-only guard in the "byreal" arm** (`eval.rs:3533-3550`)

```rust
        "byreal" => {
            // Byreal perps execute on Hyperliquid via the perps CLI; the live
            // bar stream is still Alpaca. The testnet/mainnet split is carried
            // by the run's venue_label (Testnet vs Live), enforced by the
            // SafetyGate + the venue_label↔network consistency check
            // (Task 2.3), NOT by refusing to resolve mainnet here.
            Ok(LiveVenue::ByrealLive)
        }
```

Update the venues help string (`:3555-3557`) to drop byreal's "testnet only" wording.

- [ ] **Step 4: Run, verify pass.** Run: `scripts/cargo test -p xvision-engine resolve_live_venue` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(live): resolve Byreal mainnet through the parity builder (gate via venue_label, not refusal)"
```

### Task 2.3: Enforce `venue_label ↔ BYREAL_NETWORK` consistency in `build_live_executor`

Defense-in-depth backstop to the gate: refuse to *construct* a mismatched Byreal run (Live label with testnet network, or Testnet label with mainnet network), so the operator can't accidentally point a testnet-labeled run at mainnet funds (or vice-versa).

**Files:**
- Modify: `crates/xvision-engine/src/api/eval.rs` in `build_live_executor`, right after `broker_venue_label` is derived (Task 1.4 Step 1)
- Test: `crates/xvision-engine/src/api/eval.rs` test module

- [ ] **Step 1: Write a failing test** (if `build_live_executor` is awkward to unit-test, factor the check into a free fn `check_byreal_label_network(venue_label, broker_venue_label) -> ApiResult<()>` and test that):

```rust
#[test]
fn byreal_live_label_requires_mainnet_network() {
    // Live run label but the resolved broker is Testnet ⇒ reject.
    assert!(check_byreal_label_network(VenueLabel::Live, VenueLabel::Testnet).is_err());
}
#[test]
fn byreal_testnet_label_requires_testnet_network() {
    assert!(check_byreal_label_network(VenueLabel::Testnet, VenueLabel::Live).is_err());
}
#[test]
fn byreal_matching_labels_ok() {
    assert!(check_byreal_label_network(VenueLabel::Live, VenueLabel::Live).is_ok());
    assert!(check_byreal_label_network(VenueLabel::Testnet, VenueLabel::Testnet).is_ok());
}
```

- [ ] **Step 2: Run, verify failure.** `scripts/cargo test -p xvision-engine check_byreal_label_network` → FAIL.

- [ ] **Step 3: Implement + call the check** (only for `LiveVenue::ByrealLive`):

```rust
fn check_byreal_label_network(run: VenueLabel, broker: VenueLabel) -> ApiResult<()> {
    if run != broker {
        return Err(ApiError::Validation(format!(
            "Byreal run venue_label ({run:?}) must match BYREAL_NETWORK \
             (resolved broker label {broker:?}): set BYREAL_NETWORK=mainnet for a \
             Live run or BYREAL_NETWORK=testnet for a Testnet run."
        )));
    }
    Ok(())
}
```
Call it in `build_live_executor` for the Byreal venue before wrapping the broker.

- [ ] **Step 4: Run, verify pass.** `scripts/cargo test -p xvision-engine check_byreal_label_network` → PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-engine/src/api/eval.rs
git commit -m "feat(live): enforce Byreal venue_label↔BYREAL_NETWORK consistency at build time"
```

---

## Phase 3 — Guarded real-money launch CLI verb (`xvn live`)

The eval page hardcodes `paper` (forward-test surface). Real-money launch gets its own guarded verb that builds a `venue_label=Live` `LiveConfig` and runs it through the parity builder. `/live` dashboard surface is a follow-up.

### Task 3.1: Add `xvn live --venue byreal --network mainnet` with explicit confirmation

**Files:**
- Create: `crates/xvision-cli/src/commands/live.rs`
- Modify: `crates/xvision-cli/src/commands/mod.rs` (`pub mod live;`), `crates/xvision-cli/src/lib.rs` (clap wiring)
- Modify: `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` (deny `live` real-money over remote CLI, mirroring `:168`)
- Test: `crates/xvision-cli/tests/live_verb.rs` (new), `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` test module

- [ ] **Step 1: Write failing tests for the confirmation guard + label mapping**

```rust
#[test]
fn real_money_requires_explicit_ack() {
    let err = build_live_launch(LiveArgs { venue: "byreal".into(), network: "mainnet".into(),
        confirm_real_money: false, /* assets, stop policy, capital */ }).unwrap_err();
    assert!(err.to_string().contains("--i-understand-real-money"));
}
#[test]
fn ack_yields_live_label_for_mainnet() {
    let cfg = build_live_launch(LiveArgs { venue: "byreal".into(), network: "mainnet".into(),
        confirm_real_money: true, /* ... */ }).unwrap();
    assert_eq!(cfg.venue_label, VenueLabel::Live);
    assert_eq!(cfg.broker_creds_ref, "byreal");
}
#[test]
fn testnet_stays_testnet_no_ack() {
    let cfg = build_live_launch(LiveArgs { venue: "byreal".into(), network: "testnet".into(),
        confirm_real_money: false, /* ... */ }).unwrap();
    assert_eq!(cfg.venue_label, VenueLabel::Testnet);
}
```

- [ ] **Step 2: Run, verify failure.** `scripts/cargo test -p xvision-cli live_verb` → FAIL.

- [ ] **Step 3: Implement `build_live_launch` + the verb.** Pure builder `build_live_launch(LiveArgs) -> Result<LiveConfig>`: mainnet→`VenueLabel::Live` (requires `confirm_real_money`, else error naming `--i-understand-real-money`), testnet→`VenueLabel::Testnet`; `broker_creds_ref = venue`; assets/stop-policy/capital from args. The async `run()` sets `BYREAL_NETWORK` for the child run and invokes the same engine live-launch entry the dashboard Run button uses (trace `build_live_executor` callers `eval.rs:2922`/`:3904`); print the run id. **Never** print/log key material.

> Confirm at implementation: the exact engine entry that launches a live run from a `LiveConfig`, and the `LiveConfig` constructor field set (model on what the eval page builds; the only differences are `venue_label=Live` + `broker_creds_ref="byreal"`).

- [ ] **Step 4: Deny `live` real-money over remote CLI**

```rust
#[test]
fn live_real_money_subcommand_is_rejected_over_remote_cli() {
    assert_reject(&["live", "--venue", "byreal", "--network", "mainnet"], "not allowed over remote cli");
}
```
Add `live` to the denylist (`allowlist.rs:168` neighborhood).

- [ ] **Step 5: Run tests, verify pass.** `scripts/cargo test -p xvision-cli live_verb` + `scripts/cargo test -p xvision-dashboard allowlist` → PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/xvision-cli/src/commands/live.rs crates/xvision-cli/src/commands/mod.rs crates/xvision-cli/src/lib.rs crates/xvision-cli/tests/live_verb.rs crates/xvision-dashboard/src/cli_jobs/allowlist.rs
git commit -m "feat(cli): guarded real-money 'xvn live' verb (venue_label=Live, explicit ack, local-only)"
```

---

## Phase 4 — Close the CLI real-money bypasses (`fire-trade` + `close-position`)

Both CLI submit paths currently use the `Executor` trait ungated. Route both through a shared gated helper that (a) requires a real-money ack on mainnet, (b) runs the `SafetyGate` pause check, before any submit. `fire-trade` additionally submits via `BrokerSurface` (killing the hardcoded `RiskDecision::Approved`).

### Task 4.1: Shared gated CLI submit guard

**Files:**
- Create: `crates/xvision-cli/src/commands/live_guard.rs` (helper)
- Modify: `crates/xvision-cli/src/commands/mod.rs`
- Test: `crates/xvision-cli/tests/live_guard.rs` (new)

- [ ] **Step 1: Write failing tests**

```rust
#[test]
fn mainnet_submit_requires_ack() {
    let err = require_real_money_ack("byreal", /*network*/ "mainnet", /*ack*/ false).unwrap_err();
    assert!(err.to_string().contains("--i-understand-real-money"));
}
#[test]
fn testnet_submit_no_ack_needed() {
    assert!(require_real_money_ack("byreal", "testnet", false).is_ok());
}
```

- [ ] **Step 2: Run, verify failure.** `scripts/cargo test -p xvision-cli live_guard` → FAIL.

- [ ] **Step 3: Implement** `require_real_money_ack(venue, network, ack) -> Result<()>` (error names `--i-understand-real-money` when the resolved network is mainnet and `!ack`) and an async `pause_check(...)` that builds `SafetyGate::new(manager)` from the CLI's SQLite pool (`xvn_home`) and returns an error if paused.

> Confirm at implementation: opening a `SqlitePool` + `SafetyManager` from `xvn_home` in CLI context (no existing CLI command does this — reference `AppState::new` at `state.rs:366` for the construction). If a pool genuinely can't be opened in a given CLI invocation, fail closed (refuse mainnet), never `allow_all` silently.

- [ ] **Step 4: Run, verify pass.** `scripts/cargo test -p xvision-cli live_guard` → PASS. Commit.

### Task 4.2: Route `fire-trade` through `BrokerSurface` + the guard

**Files:**
- Modify: `crates/xvision-cli/src/commands/fire_trade.rs` (replace the `Executor`-trait submit block `:96-122` + drop the `RiskDecision::Approved` synthesis)
- Test: `crates/xvision-cli/tests/fire_trade_parity.rs` (new) — engine-local mock `BrokerSurface` (as in Task 1.2)

- [ ] **Step 1: Write failing tests** — `fire_one_order(broker: Arc<dyn BrokerSurface>, gate: SafetyGate, order: OrderRequest)` runs `gate.check_broker_submit(...)` then `broker.submit_order(order)`; assert paused → err (no delegate), allowed → delegates. Mirror Task 1.2's `RecordingBroker`.
- [ ] **Step 2: Run, verify failure.**
- [ ] **Step 3: Reimplement `fire_trade::run`** over `BrokerSurface`: call `require_real_money_ack` (Task 4.1); build `OrderRequest` from CLI args; construct the matching surface (`ByrealLiveSurface::from_env()` / `OrderlyLiveSurface::from_env()` / `AlpacaPaperSurface::from_credentials`); call `fire_one_order`. Drop the `{AlpacaExecutor, ByrealPerpsExecutor, Executor, OrderlyExecutor}` import; add `{AlpacaPaperSurface, OrderlyLiveSurface, ByrealLiveSurface, broker_surface::{OrderRequest, Side}}`. Optionally run `perps_entry_veto(&cfg, true, true, dir, None, None)` first (None funding/liq ⇒ permissive when data absent — documented).
- [ ] **Step 4: Run, verify pass.** Update the module doc (`fire_trade.rs:1-12`). Commit.

```bash
git commit -m "refactor(cli): route fire-trade through BrokerSurface + safety gate (kill the Approved bypass)"
```

### Task 4.3: Gate `close-position`

**Files:**
- Modify: `crates/xvision-cli/src/commands/venue.rs:60-66` (`close_position`)
- Test: `crates/xvision-cli/tests/close_position_guard.rs` (new)

- [ ] **Step 1: Write a failing test** that `close_position` for `byreal` + mainnet without ack errors with `--i-understand-real-money`, and that it runs the pause check before `exec.close_position`.
- [ ] **Step 2: Run, verify failure.**
- [ ] **Step 3: Implement** — `close_position` calls `require_real_money_ack` + `pause_check` (Task 4.1) before `executor_from_env(venue)` / `exec.close_position(asset)`. (Full migration to a `BrokerSurface::close` is the documented follow-up; gating makes it non-bypassing now.) Thread an `--i-understand-real-money` flag from the clap wiring (`lib.rs`).
- [ ] **Step 4: Run, verify pass.** Commit.

```bash
git commit -m "fix(cli): gate close-position (pause check + real-money ack) — close the last ungated mainnet path"
```

---

## Phase 5 — Verification (no real funds), docs, memory

### Task 5.1: End-to-end dry-run proof

**Files:** `crates/xvision-engine/tests/byreal_mainnet_parity_e2e.rs` (new)

- [ ] **Step 1: Write an integration test** that builds a `venue_label=Live`, `broker_creds_ref="byreal"` `LiveConfig`, injects an engine-local mock `BrokerSurface` (recording, labeled Live) via `build_live_executor`'s `broker_override` (`eval.rs:3566`), runs a few synthetic bars, and asserts: (a) the run's persisted `venue_label == "live"`; (b) with the server SafetyManager **paused**, the mock receives **zero** orders (gate blocked); (c) unpaused, the mock receives the order. Proves parity path + production gate + Live label without real funds or `npx`.
- [ ] **Step 2: Run.** `scripts/cargo test -p xvision-engine byreal_mainnet_parity_e2e` → PASS.
- [ ] **Step 3: Full workspace test + format check.** `scripts/cargo test --workspace`; format only changed files (`rustfmt --config-path` per repo convention — do NOT run workspace `cargo fmt`).
- [ ] **Step 4:** (manual, optional) Byreal **testnet** run via `xvn live --venue byreal --network testnet` with `BYREAL_NETWORK=testnet` + testnet creds (OP Olympus / "XVN Wallet" per memory) + `npx` on PATH. Confirm it badges **Testnet** (not Live). **Do not run mainnet.**
- [ ] **Step 5: Commit the e2e test.**

### Task 5.2: Docs + claim update + memory

- [ ] **Step 1:** Update `crates/xvision/CLAUDE.md` live-scope notes: Byreal mainnet runs through the parity path with `venue_label=Live`; `fire-trade`/`close-position` are gated; document the Alpaca-market-data + whitelist constraint.
- [ ] **Step 2:** Update the "no private keys" public claim (`/Users/edkennedy/Code/xvnapp-landing` per memory) to reflect Byreal mainnet reading `BYREAL_PRIVATE_KEY` via the unpinned `@byreal-io/byreal-perps-cli` (operator-approved).
- [ ] **Step 3:** Update `MANUAL.md` with `xvn live`, the safety-gate behavior, and the `--i-understand-real-money` ack.
- [ ] **Step 4:** Fix the stale memory `xvision-risk-risklayer-is-active-perps-gate.md` (retired in #1038; risk unified on the engine veto path). Add `byreal-single-execution-path` memory.
- [ ] **Step 5: Push branch + open draft PR.**

```bash
git add -A && git commit -m "docs(byreal): mainnet parity notes, real-money claim update, MANUAL"
git push -u origin feat/byreal-mainnet-parity
gh pr create --draft --title "Byreal mainnet parity: single gated execution path" --body "..."
```

---

## Self-review checklist (run before handing off)

1. **Spec coverage:** Q1 re-route → Phase 4 (both CLI submit paths). Q2 gate at shared boundary → Phase 1 (decorator wraps every live broker). Q3 guarded CLI verb → Phase 3 (`/live` dashboard = follow-up). Q4 code+dry-run only → Phase 5 (no mainnet trade). ✅
2. **Safety invariant — now provably true:** after this plan, **every** real-money submit passes a `SafetyGate` check — live/eval/optimizer runs via `GatedBrokerSurface` (Phase 1), `fire-trade` + `close-position` via the CLI guard (Phase 4). A non-Live run can't reach a Live broker (gate broadened, Task 1.1) and a mislabeled Byreal run can't be constructed (Task 2.3). No ungated path to mainnet funds remains. ✅
3. **No 30-caller cascade:** `Executor::live()` signature is unchanged; the gate is a transparent broker decorator. ✅
4. **Production gate is live, not allow-all:** Task 1.3 wires `SafetyGate::new(manager)` into `AppState::api_context()` with a test (`is_enforcing()`). ✅
5. **Reviewer fixes applied:** F1 (`AuthContext::system()`, not fabricated `From`), F2 (engine-local mock `BrokerSurface`, no cross-crate `#[cfg(test)]` `MockByrealApi`), C1 (`state.rs` in scope + test), C2 (decorator avoids the cascade), C3 (gate broadened + consistency check), S1 (`close-position` gated). ✅
6. **Type consistency:** `GatedBrokerSurface::new(inner, gate, run_label, broker_label, auth)` used identically in Tasks 1.2/1.4; `require_real_money_ack`/`pause_check`/`fire_one_order`/`build_live_launch`/`check_byreal_label_network` consistent across Phases 3–4.
7. **Confirm-at-implementation flags (exact lookup targets, not placeholders):** the `AppState` test constructor name; the engine live-launch entry that `xvn live` calls; the `LiveConfig` constructor field set; CLI `SqlitePool`/`SafetyManager` construction from `xvn_home` (ref `state.rs:366`); whether `SafetyGate::allow_all()` short-circuits before the mismatch check (Task 1.1 test note).
