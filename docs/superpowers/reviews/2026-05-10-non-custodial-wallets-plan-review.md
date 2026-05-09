# Adversarial Review — Non-Custodial Agent Wallets Plan
**Spec:** `docs/superpowers/specs/2026-05-09-non-custodial-agent-wallets-design.md`
**Plan:** `docs/superpowers/plans/2026-05-10-blockchain-1-non-custodial-wallets-plan.md`
**Reviewer:** adversarial pass · 2026-05-10 · pre-implementation

---

## Triage summary

| Severity | Count |
|---|---|
| **Blocker** | **6** |
| **High** | **14** |
| **Medium** | **15** |
| **Low** | **9** |
| **Nit** | **5** |

**Headline conclusion.** The plan is comprehensive in scope but has three classes of problem that must be addressed before code starts:

1. **Existing-code mismatches (blockers).** The plan rewrites `OrderlyExecutor::submit` and `TraderDecision`'s shape without acknowledging that the current executor takes `&RiskDecision` (not the new `(client_order_id, asset, side, size_usdc)` tuple), that `TraderDecision` carries `size_bps` (not `size_usdc` or `notional`), and that the existing `RiskLayer` is not wired to per-strategy state. Phase 3 Task 3.2 will not compile against the current trunk; Task 3.3's "Replace direct executor calls" understates the work by an order of magnitude.
2. **Spec coverage gaps the self-review misses (blockers/highs).** Funding-payment attribution (spec §3.5) has a table but **no task that writes to it**. The phishing-resistant browser flow (spec §3.2) is collapsed to a `println!` + manual instruction. There is no `trading_keys` SQLite table though the CLI calls `ctx.store_trading_key`. Global halt (`xvn kill --all`) calls `ctx.global_halt(...)` which is never defined or tabled. The `policy_changes` journal is wired to `xvn budget set` but **no enforcement** that a policy edit reloads in the dispatcher's hot path.
3. **Hackathon feasibility (high).** Five weeks, single operator, eight phases of mostly new code, two probes that block everything, plus implicit dependencies on Plan 2c (scheduler) and Plan 2d (dashboard) that are themselves not done. Phase 8 alone is "build a spreadsheet UI" inside another team's still-iterating dashboard. A minimum viable subset is identified below — recommended hard-cut.

Severity floor for "blocker" is "would not compile or would silently corrupt safety property." For "high" it is "would ship but expose a real vulnerability or operational gap." Mediums and below are correctness, hygiene, and ergonomics.

---

## 1. Spec → plan coverage gaps

### 1.1 [BLOCKER] Funding-payment attribution table is created but never populated
- **Location:** spec §3.5 (`funding_attributions` table); plan Phase 1 Task 1.1 Step 3 creates the schema; plan Phase 7 reconciler does **not** ingest funding events; no other task does either.
- **Issue:** The spec is explicit that without funding attribution "long-held positions look more profitable than they are and the marketplace leaderboard misranks holders vs scalpers." `quota_factor` (Phase 6) reads `closed_pnls_30d` — if funding is not folded into `realized_pnl_usdc` at close time, the dynamic quota will systematically over-allocate to long-hold strategies during negative-funding regimes. The bias compounds across 30-day windows.
- **Fix:** Add a Phase 7 sub-task (7.2) "Funding ingestion": poll `GET /v3/funding_fees/history` (Orderly), match each event to the open position holding the symbol during the funding period, write a `funding_attributions` row, and on `close_position` add the cumulative funding payment to `realized_pnl_usdc`. Tests: a position held across two funding periods, both negative, must show the funding loss reflected in `realized_pnl_usdc`.

### 1.2 [BLOCKER] No `trading_keys` table or schema, but multiple commands write to / read from one
- **Location:** plan Task 4.7 Step 2 (`ctx.store_trading_key(...)`, `ctx.list_trading_keys()`, `ctx.revoke_user_trading_key(...)`); plan Task 4.2 Step 3 (`ctx.list_user_strategies(user)`, `ctx.revoke_user_trading_key(user)`); plan Task 4.10 (AppContext does not include a trading-keys repo).
- **Issue:** The CLI calls these methods but no migration creates a `trading_keys` table, no module under `xianvec-data` defines a writer/reader for it, and no `AppContext` field provides one. The phrase "encrypted at rest using the AES-256-GCM scheme described in §5" appears in the spec but the *persistence layer* for the encrypted blob is missing entirely. `xvn key issue` will fail at runtime.
- **Fix:** Add Task 1.6 to Phase 1 (or fold into 1.4): migration `008_trading_keys.sql` with `(user_id PK, pubkey_hex, encrypted_blob, scope, registered_at, expires_at, revoked_at, last_used_at)`; module `crates/xianvec-data/src/trading_keys.rs` with insert/list/revoke; wire into `AppContext`. Add a test: round-trip an encrypted blob through `TradingKeyStore::store` → `load` → `decrypt` returns the original Ed25519 bytes.

### 1.3 [BLOCKER] `xvn kill --all` calls undefined `ctx.global_halt(...)`
- **Location:** plan Task 4.2 Step 3 (`ctx.global_halt(&args.reason, "operator-cli").await?`); the AppContext in Task 4.10 has no `global_halt` field/method; no `global_halt_status` migration; no global-halt check in the dispatcher.
- **Issue:** Spec §3.9 promises `xvn kill --all` "global halt: all dispatchers refuse new orders." The plan never implements either side: no storage for the global-halt flag, no read of it in `OrderDispatcher::dispatch`, no test. This is the operator's nuclear button — must work.
- **Fix:** Add a `global_state` table (single row) with a `halted_at`, `halted_by`, `reason`. Implement `GlobalState::is_halted()` checked at the very top of `OrderDispatcher::dispatch` (before even the `emit` audit). Test: set halt, attempt dispatch, verify outcome is `Halted` and a single `Reject` audit row written.

### 1.4 [HIGH] Phishing-resistant browser flow (spec §3.2) is replaced by `println!` instructions
- **Location:** spec §3.2 ("Phishing-resistant registration UX (mandatory)"); plan Task 4.7 Step 2 (`xvn key issue`) — the user is asked "Open Orderly registration in browser now?" but the actual EIP-712 signing is left as ellipsis (`// ... open browser to a templated URL ...`).
- **Issue:** The spec calls out this as "the highest-leverage attack surface." The plan defers the entire registration round-trip — the `add_orderly_key` EIP-712 sign-and-POST path that the m1 probe (Task 0.2 Step 5) will figure out — to "manual operator follows printed steps." That is not a registration UX, that is a stub. The spec also requires a `xvn key verify <pubkey>` independent verification command; this is missing entirely.
- **Fix:** (a) Add Task 4.7a "implement EIP-712 add_orderly_key sign+POST in `crates/xianvec-execution/src/orderly.rs::register_trading_key(evm_signer, pubkey, scope, expiration_unix) -> Result<KeyId>` and call it from `xvn key issue` once the user has signed. The signing itself should be done by the user's wallet (e.g., MetaMask via WalletConnect or — for v1 single-operator — hardware-wallet path via `cast wallet sign --ledger`). (b) Add `xvn key verify <pubkey>` that re-derives the public key from the locally stored encrypted private key, prints the hex, and compares.

### 1.5 [HIGH] Policy-change hot-reload is not enforced
- **Location:** spec §3.4 ("Edits do not auto-apply to in-flight positions" — but DO apply to *future* dispatches); plan Task 4.8 Step 3 ("write to `policy_changes` for each touched field, write the new config back"); Phase 8 Task 8.4 (POST handler "applies the change").
- **Issue:** "Write the new config back" is hand-waved. There is no specified storage for `StrategyConfig` itself — the plan defines `parse_strategies_toml` (Task 2.1) that loads from a string, but never stores it. Where does the dispatcher read the *current* config from on each dispatch? If it's a TOML file, edits via the UI must rewrite the TOML; if it's a `strategies` table in SQLite, that table is missing. Either way, a config change must be visible to the next `dispatch()` call without a process restart.
- **Fix:** Add a migration `009_strategies.sql` with `(strategy_id PK, config_json, updated_at, updated_by)`; add `xianvec-data::strategies::StrategyConfigStore` with `get`, `set`, `list`. The dispatcher reads from the store on each `dispatch()`. The TOML loader (Task 2.1) becomes the bulk-import seed path only. Tests: `xvn budget set --strategy s1 --hard-cap 1000` then `dispatch(s1, …)` sees the new cap.

### 1.6 [HIGH] Spec §3.4 cold-start floor of "0.25 if < 30 closed positions" — ambiguous and the formula in Phase 6 doesn't match
- **Location:** spec §3.4 (formula adds `cold_start_floor + sigmoid(...) × (1 - dd/floor)`); plan Phase 6 Task 6.1 implementation does the same, but the cold-start branch returns `COLD_START_FLOOR` *only* when `pnls < 30`. The spec's intent is that the floor is **always added** as a baseline even after 30 samples, but the implementation matches that. **However:** when there are >= 30 samples and Sharpe is mildly positive, the result is `0.25 + ~0.6 × 1.0 = 0.85`. If Sharpe is very high (e.g. mean 100, std 1), `sigmoid(66) ≈ 1.0` and result is `1.25` then clamped to `1.0`. That's fine. The bug is at `Sharpe = 0`: `sigmoid(0) = 0.5`, drawdown 0 → `quota = 0.25 + 0.5 = 0.75`. So a strategy with literally zero Sharpe (mean 0 PnL) gets 75% of cap unlocked. That's not what the spec implies ("Cold strategies start at the floor (0.25). Hot strategies converge toward 1.0. Burned strategies throttle toward 0.")
- **Issue:** Mean-zero performance shouldn't earn 0.75 quota. The cold-start floor + sigmoid produce a misleading mid-range default. The spec's formula is one possible reading, but the operator's stated *intent* (cold = 0.25, hot = 1.0, burned = 0) suggests the floor should anchor the bottom, not be additive.
- **Fix (two options):**
  - **Option A (match operator intent literally):** `quota = max(0.25, sigmoid(sharpe/k) × (1 - dd/dd_floor))`. Mean-zero strategy gets `max(0.25, 0.5 × 1.0) = 0.5` — still high but at least not 0.75. Tune `k=1.5` or higher to push midpoint down.
  - **Option B (preserve formula but tune):** Subtract `0.25` from the sigmoid term so cold-start = floor and not floor + 0.5: `quota = floor + (sigmoid - 0.5) × (1 - dd/dd_floor) × 1.5`. Documented constants change.
- **Recommend:** Operator chooses; the plan should encode whichever and add tests for the three named regimes (cold, mean-zero, hot, burned, deep-drawdown).

### 1.7 [HIGH] No CLI/storage for the **scoped permission set being requested at registration time**
- **Location:** spec §3.2 enumerates "permissions: trading only (no withdraw, no transfer); ip_restriction (optional): xianvec server IP; expiration: 90 days, rotatable"; plan Task 0.2/4.7 only mentions `permissions: ["trading"]` and 90-day expiration.
- **Issue:** `ip_restriction` is silently dropped. For a server-side single-tenant deploy this is the cheapest meaningful defense in depth — Orderly enforces it, the cost is nil. Also there's no test that the registered key actually expires or that the dispatcher refuses to use an expired key.
- **Fix:** (a) Add `ip_restriction` to the registration payload, defaulted to `XVN_PUBLIC_IP` env var if set, no-op if absent. (b) Dispatcher fast-path check: if `now > key.expires_at - 24h`, write `Reject` audit row with reason `key_expiring_soon` and refuse new opens (existing positions can still be closed). Surface a critical alert.

### 1.8 [MEDIUM] No `delete_orderly_key` path for revoke
- **Location:** spec §3.9 (`xvn key revoke`); spec §5 ("user can revoke key on Orderly side instantly"); plan Task 4.7 `revoke()` only sets a local revoked flag and tells the user to do the Orderly side themselves.
- **Issue:** Local revoke without Orderly-side revoke leaves the key still usable from anywhere holding the encrypted blob + secret. The plan should at least *attempt* the Orderly-side delete and fall back to the print message.
- **Fix:** `revoke` calls `DELETE /v3/orderly_key/{pubkey}` (signed by the EVM key, which the operator must produce — same way as registration); on success, mark local revoked; on failure, print the manual fallback. Document the failure mode (operator key not available) explicitly.

### 1.9 [MEDIUM] Aggregate-margin guard is not wired into the dispatcher even when G2 = CROSS_ONLY
- **Location:** plan Task 5.2 Step 3 ("Add a Margin info fetch in `OrderlyExecutor` and call from dispatcher pre-submission") — one bullet, no implementation, no test.
- **Issue:** The whole point of the aggregate-margin guard is that it must short-circuit the dispatcher *before* simulate/sign/submit. A one-line bullet ignoring caching strategy, fetch frequency (every dispatch is too slow), and what happens when the margin endpoint is down (fail-closed = block all trading; fail-open = defeats the guard) is not enough.
- **Fix:** Specify: dispatcher fetches account margin via a 5s-TTL in-memory cache; on cache miss + endpoint down, fail-closed with a `Reject` audit row reason `margin_fetch_unavailable`. Add a test with a mock OrderlyExecutor that returns a stale-then-down margin; assert that the second dispatch is rejected.

### 1.10 [LOW] `OrderDispatcher` has no `record_close` method but Task 4.1 Step 4 says to add one
- **Location:** plan Task 4.1 Step 4 ("Modify the dispatcher's close path... extend with a `record_close` method").
- **Issue:** The `OrderDispatcher` skeleton in Task 3.2 has no close path at all — `close` is mentioned only in audit-log stages. Need to clarify where the close pipeline lives (Orderly webhook? polling? `xvn` CLI command?). Without a defined trigger, `record_close` is dead code.
- **Fix:** Either fold close-handling into the Phase 7 reconciler (which already detects server-side closes) or add an explicit "close pipeline" subtask. State which.

### 1.11 [LOW] Spec §10 "MPC migration evaluation" is referenced as deferred but no FOLLOWUPS update task
- **Location:** plan Task 9.3 ("Update FOLLOWUPS.md").
- **Issue:** The plan should explicitly list "MPC trading-key signing", "browser-issued ephemeral session keys", "smart-account migration", "OKX/Kite Passport eval", "Fystack pilot" as new SLF entries with triggers and scope. Right now the spec discusses them at length and the plan says "add follow-ups" without naming them.
- **Fix:** Enumerate the v2 candidates as a checklist in Task 9.3 Step 1.

---

## 2. Internal inconsistencies in the plan

### 2.1 [BLOCKER] `OrderDispatcher` and existing `OrderlyExecutor::submit` have incompatible signatures
- **Location:** existing `crates/xianvec-execution/src/orderly.rs:633` — `submit(&self, decision: &RiskDecision) -> Result<ExecutionReceipt>`; plan Task 3.2 — defines `OrderlyOrderSubmit` trait with `submit_order(&self, client_order_id, asset, side, size_usdc) -> Result<String>`; plan Task 3.3 says "the previous direct submission becomes the `OrderlyOrderSubmit` impl backing the dispatcher."
- **Issue:** `OrderlyExecutor::submit` does dramatically more than place an order — it converts `bps` to USDC notional via live equity, computes BTC qty from mark price, places TP/SL bracket legs, polls for fill, returns an `ExecutionReceipt`. The plan's `OrderlyOrderSubmit` trait throws all of that away. Either (a) the plan must port the bps-to-notional, mark-price, TP/SL bracket logic into `OrderDispatcher` (which would explode the dispatcher's scope), or (b) `OrderlyOrderSubmit` needs a much richer surface than `(client_order_id, asset, side, size_usdc) -> orderly_position_id`. Without that, the dispatcher cannot place a real order against the real Orderly API.
- **Fix:** Redesign the trait surface. Suggested: `OrderlyOrderSubmit::submit(&self, client_order_id, decision: &TraderDecision, equity: f64) -> Result<ExecutionReceipt>` — pass the decision through, let the executor keep its bracketing logic. Then dispatcher writes the audit/ledger rows around the existing `OrderlyExecutor::submit` as a wrapper, not a replacement.

### 2.2 [BLOCKER] `TraderDecision` does not have `notional_usdc()`, `asset()`, `side_str()`, and adding them is non-trivial
- **Location:** plan Task 3.2 Step 3 ("Mechanical; ~15 lines"); existing `crates/xianvec-core/src/trading.rs:114+` — `TraderDecision { setup_id, action, size_bps, direction, stop_loss_pct, take_profit_pct, trader_summary }`; no `asset` field at all.
- **Issue:** `size_bps` is a fraction of NAV, not a USDC amount. To compute notional you need the equity. `asset` is *not on `TraderDecision`* in v1 — see FOLLOWUPS F18 ("Add `asset: AssetSymbol` to `TraderDecision` ... mechanical but wide ... blocking for multi-asset"). The dispatcher in Task 3.2 calls `decision.asset()` and `decision.notional_usdc()` as if they exist; the helper would have to (a) take a portfolio-state argument and (b) for asset, return a hardcoded BTC. The "~15 lines" estimate is wrong. F18 estimates it as wide but mechanical — touches xianvec-trader, xianvec-intern, xianvec-risk, xianvec-execution, xianvec-eval.
- **Fix:** Either (a) execute F18 as Phase 0 prerequisite (estimate: 0.5–1 day), or (b) explicitly accept that v1 dispatcher is BTC-pinned and pass `asset = "PERP_BTC_USDC"` constant + take portfolio state in `dispatch(...)` to compute notional. Document the choice.

### 2.3 [HIGH] `Verdict::Approved` (per_strategy.rs) vs existing `RiskDecision::Approved` (xianvec-core) overlap
- **Location:** existing `crates/xianvec-core/src/trading.rs:235` defines `RiskDecision { Approved, Modified, Vetoed }`; plan Task 2.2 introduces a parallel `Verdict { Approved, RequiresApproval, Vetoed }`.
- **Issue:** Two enums with overlapping but non-identical variants will create fan-out: existing rules emit `RiskDecision`, new rules emit `Verdict`, dispatcher must reconcile both. Also `Verdict` lacks `Modified`, which is the existing system's primary mechanism for "size cut by max-position rule." If the new per-strategy rules ever need to modify (e.g., "hard cap exceeded by 10% — modify to fit"), the surface is wrong.
- **Fix:** Either (a) have `PerStrategyEvaluator` return `RiskDecision` and add a `RequiresApproval` variant to it (one new variant; touches the existing match-arms throughout xianvec-risk), or (b) rename `Verdict` → `PerStrategyVerdict` and document explicitly that the dispatcher composes the two. Option (a) is cleaner long-term; option (b) is faster.

### 2.4 [HIGH] Dispatcher writes `Sign` audit stage **before** signing (plan inverts the order)
- **Location:** plan Task 3.2 Step 2 — Stage 6 "open ledger row + sign + submit" — the audit row `Stage::Sign` is written before `self.orderly.submit_order(...)` is called, so the audit "sign" payload only contains `client_order_id`, not the signed REST payload that spec §3.8 says it should contain.
- **Issue:** Spec §3.8 explicitly says the `sign` stage payload is "The Ed25519-signed REST payload (with body and signature; key id, NOT the key)." The plan's audit row contains only `client_order_id`. The actual signed bytes are computed inside `OrderlyExecutor::submit` and never returned to the dispatcher. So the audit log loses the most forensically valuable artifact.
- **Fix:** `OrderlyOrderSubmit::submit` must return (or accept a sink for) the signed payload bytes + signature so the dispatcher can write them to the `Sign` audit row. This is another reason to widen the trait surface (see 2.1).

### 2.5 [HIGH] Reservation pattern's `try_reserve(notional, cap)` call is incompatible with quota_factor wiring
- **Location:** plan Task 2.3 (`try_reserve(user, strategy, notional, cap)`); plan Task 6.1 Step 4 ("Pass `cap * quota_factor` as the cap argument to the reservation").
- **Issue:** The reservation table stores `notional_usdc` reserved against a strategy. Two concurrent dispatches with *different* quota_factor calls (a fresh quota_factor read each dispatch) will pass different `cap` values. The reservation logic (`if in_flight + reserved + notional > cap`) is only correct against a *single* cap value. If dispatch A reserves $400 against cap=$1000 (quota=1.0) and dispatch B simultaneously evaluates against cap=$700 (quota=0.7, ledger updated meanwhile), B's check `400 + 0 + 400 > 700` correctly rejects. But if dispatch A's quota is 0.7 ($700 cap) and B's is 1.0 ($1000 cap), the cap is non-monotonic across concurrent calls and the system can over-allocate.
- **Fix:** Either (a) compute quota_factor *inside* `try_reserve` against the latest ledger snapshot (single source of truth), passing the hard_cap and the quota_inputs, not the post-quota cap; or (b) lock the strategy across the quota read + reservation write. Option (a) is cleaner. Test: two concurrent dispatches with different ledger states must produce a correct combined reservation.

### 2.6 [HIGH] `ReservationManager` per-strategy locks leak forever
- **Location:** plan Task 2.3 — `locks: Mutex<HashMap<String, Arc<Mutex<()>>>>` — `lock_for(strategy_id)` does `or_insert_with(|| Arc::new(Mutex::new(())))`.
- **Issue:** Every strategy_id ever passed creates an entry. After a long-running daemon serves many ephemeral test strategy_ids (e.g. during integration tests, or during the autoresearcher's mutator producing N variants/night), the HashMap grows unbounded. Not catastrophic for v1 single-operator, but the pattern is wrong.
- **Fix:** Use a `dashmap` or a periodic sweep; or weak-arc + drop-when-zero-refs; or accept it for v1 and add a TODO. At minimum, document the leak.

### 2.7 [HIGH] `Reservation::release` is best-effort but `submit_order` failure path doesn't release
- **Location:** plan Task 3.2 Step 2 — after `self.orderly.submit_order(&position.client_order_id, ...).await?` — the `?` propagates the error, the function returns, **but the reservation is never released**.
- **Issue:** A flaky Orderly submission (network blip, 503) returns Err, the reservation sits until TTL expiry (30s), so a strategy whose dispatcher crashed mid-submit can't trade for 30s afterward. Worse: the position row was already inserted via `ledger.open_position` *before* the submit attempt. So on failure, the ledger has a phantom open position with no entry_price, no orderly_position_id — and `in_flight_notional()` includes its `size_usdc`. Now the strategy is double-blocked (reservation + phantom open) for at least 30s.
- **Fix:** Wrap the submit in a `match` that releases the reservation AND deletes the phantom position row on Err. Test: induce submission failure, assert reservation released and position row absent.

### 2.8 [MEDIUM] `client_order_id = position_id = ULID` collision risk vs Orderly's 36-char limit
- **Location:** plan Task 1.3 (`Position::new` sets `client_order_id = id` where id is `Ulid::new().to_string()`); existing `crates/xianvec-execution/src/orderly.rs:34` ("max 36 chars").
- **Issue:** ULID strings are 26 chars, fits fine. But the existing code uses `td.setup_id.to_string()` (UUID, 36 chars). The plan replaces this with ULID. Existing TP/SL legs use `format!("tp-{}", td.setup_id)` and `format!("sl-{}", td.setup_id)` → 39-char strings now (`tp-` + 26-char ULID = 29, fits). Inconsistency: now there are two id schemes for client_order_id (UUID for legacy in `orderly.rs`, ULID from `Position::new`). The plan's Task 1.5 says to "tag every trade" but doesn't address the existing `setup_id` pattern.
- **Fix:** Decide one scheme. Recommend: ULID for new dispatcher path; document that old setup_id-based paths are gone after Task 3.3.

### 2.9 [MEDIUM] `Stage::Reject` is used in dispatcher but not in the spec §3.8 stage enumeration
- **Location:** plan Task 3.2 (`audit.write(Stage::Reject, ...)`); spec §3.8 stage list: `emit | risk_eval | simulate | sign | submit | response | fill | close | cancel | reject` — actually it IS there (lowercase in spec, `Reject` in plan). Migration 003 lists `'reject'`. No issue. Withdrawn — see correction below.
- **Issue:** Withdrawn — re-reading shows `reject` is in the schema. Nit: the plan's `Stage::Reject` enum variant is consistent with the migration's `'reject'` string via `#[sqlx(rename_all = "lowercase")]`.

### 2.10 [MEDIUM] Plan's "risk_eval" rename does not match `sqlx(rename_all = "lowercase")` mapping
- **Location:** plan Task 1.2 (`#[sqlx(rename_all = "lowercase")]` on `Stage` enum); migration 003 (`stage TEXT NOT NULL CHECK (stage IN ('emit','risk_eval','simulate', ...))`).
- **Issue:** `lowercase` with sqlx maps `RiskEval` → `riskeval`, not `risk_eval`. The CHECK constraint will fail on insert.
- **Fix:** Use `#[sqlx(rename_all = "snake_case")]` on the Stage enum, and re-verify all stage strings round-trip.

### 2.11 [MEDIUM] `Side` enum sqlx mapping inconsistent
- **Location:** plan Task 1.3 (`#[sqlx(rename_all = "UPPERCASE")]` on `Side { Long, Short }` → maps to `LONG`/`SHORT`); migration 001 CHECK constraint matches `'LONG'`/`'SHORT'` — fine. Plan Task 3.2 dispatcher does `if side == "BUY" { Side::Long } else { Side::Short }` — fine. Plan Task 3.2 simulator call: `self.simulator.simulate(&asset, side, intended_notional / 78_000.0 /* qty proxy; replace with mark */)` — passes `side` which is `&str` "BUY"/"SELL" not Side enum — fine.
- **Issue:** The 78,000 placeholder is a literal for BTC price. Will silently produce nonsense slippage estimates if BTC price moves materially or for any other asset. Marked as `/* qty proxy; replace with mark */` but that means the plan ships unfinished into Phase 3.
- **Fix:** Replace the placeholder before merging Phase 3; require the dispatcher to take a mark price or fetch it. Add a property test that the dispatcher rejects when mark_price is unavailable.

### 2.12 [LOW] Files structure section lists `crates/xianvec-execution/src/lib.rs` modify with `pub mod approval` but Task 4.5 Step 3 doesn't mention adding it
- **Location:** plan File structure (line 70); Task 4.5.
- **Fix:** Add a Step 3a: "Edit `crates/xianvec-execution/src/lib.rs`: `pub mod approval;`"

### 2.13 [LOW] `AppContext::from_env` migration path uses a relative path that breaks under `cargo test`
- **Location:** plan Task 4.10 Step 1 — `sqlx::migrate!("../xianvec-data/src/migrations").run(&pool).await?;`
- **Issue:** Relative paths in `sqlx::migrate!` are resolved at compile time relative to `CARGO_MANIFEST_DIR` of the calling crate. From `crates/xianvec-cli`, `../xianvec-data/src/migrations` is correct. From `crates/xianvec-cli/tests/`, also correct (still resolves at compile time from the crate root). OK, not a bug, but fragile if the cli crate moves.
- **Fix:** Add a comment, or expose a `xianvec_data::migrate(&pool).await?` helper that owns the path internally.

### 2.14 [LOW] `xvn key list` shows `expires_at` as a raw integer (millis since epoch)
- **Location:** plan Task 4.7 Step 2 (`println!("{:<20} {:<70} {:<10}", k.user, k.pubkey_hex, k.expires_at);`).
- **Fix:** Format as RFC3339 or human-relative ("expires in 47 days").

### 2.15 [NIT] Plan claims "all 11 tests pass" for Phase 2 Task 2.2 but enumerates only ~8 in the test outline
- **Location:** plan Task 2.2 Step 4 ("PASS (11 tests)").
- **Fix:** Either add the missing test bodies or change the count.

---

## 3. Hidden complexity / underestimated work

### 3.1 [HIGH] Phase 0 probes ("scaffold + 3 todo!()") understates the EIP-712 + signing-helper extraction
- **Location:** plan Task 0.2 Step 5.
- **Issue:** "Open `crates/xianvec-execution/src/orderly.rs` and locate the existing EIP-712 onboarding flow. Copy the request-signing helper into `m1`" — the existing `orderly.rs` does Ed25519 signing for orders but **not** EIP-712 onboarding. Onboarding (registering an Orderly account from an EVM signer) is currently manual (per FOLLOWUPS F5 — "complete brokered onboarding once via `xvn setup --orderly-onboard` per plan §6.3"). The probe needs to implement EIP-712 from scratch — this is non-trivial (typed data hashing, domain separator, struct hashing, secp256k1 signing). 1–2 days, not a 4-hour probe.
- **Fix:** Acknowledge the EIP-712 implementation as a real subtask. Use `alloy-sol-types` or `ethers-core` for the typed-data hashing. Add deps to the probe Cargo.toml.

### 3.2 [HIGH] Phase 4 Task 4.4 emergency-close: `cancel_all_orders` and `market_close_all_positions` have correctness traps
- **Location:** plan Task 4.4 Step 1 (todo!() bodies).
- **Issue:** (a) `cancel_all_orders` between fetching open orders and DELETE-each, new orders may arrive (race). Need to loop until empty or a few iterations. (b) `market_close_all_positions` when called with cross-margin and a large position can experience massive slippage and partial fills. (c) Need to handle the case where market-close orders themselves get rejected (insufficient margin? reduce-only flag missing?). (d) Need idempotency: rerunning `xvn emergency-close` after a partial run should resume, not double-close. None of this is in the plan.
- **Fix:** Spec out the algorithm: (1) set global halt to prevent new opens; (2) cancel-loop until two consecutive iterations report zero new orders; (3) for each open position, submit a reduce-only market order matching the size; (4) verify via reconcile that all positions are flat; (5) loop steps 3–4 up to 3 times. Test against testnet with 2+ open positions.

### 3.3 [HIGH] Plan 2c (scheduler) integration is unspecified
- **Location:** plan Phase 7 Task 7.1 Step 3 ("If Plan 2c (durable scheduler) has shipped, register the reconciler as a scheduled job. Otherwise: spawn a tokio task in the live-deploy entry point").
- **Issue:** Plan 2c's scheduler stores jobs in `crates/xianvec-engine/migrations/001_scheduler.sql` — a different SQLite database (or at least different tables) than this plan's `xianvec-data` migrations. Wiring the reconciler through Plan 2c's `Scheduler` API is nontrivial: needs a shared pool, a job-payload serialization for `Reconcile { user_id }`, and the runner must hold an `AppContext` to do any work. None of this is sketched. The fallback ("spawn a tokio task") works for single-user single-process but not in a deployed `xvn live deploy` flow if the live daemon already owns the runtime.
- **Fix:** Pick one path before Phase 7. If Plan 2c ships first, write the wiring task explicitly; if not, use the tokio-task fallback and document that it doesn't survive process restart between reconciles (the next reconcile will pick up the drift on next run).

### 3.4 [HIGH] Phase 8 dashboard integration assumes Plan 2d's `AppState` is extensible without conflict
- **Location:** plan Phase 8 Task 8.2 Step 2 (`use crate::AppState; // or whatever 2d names its shared state`).
- **Issue:** Plan 2d's `AppState { xvn_home }` (line 146 of 2d plan) is intentionally minimal — just a `PathBuf`. Phase 8 needs database access (ledger, audit, status, policy) plus an Orderly client (for `account_collateral_usdc()`). Adding all of those to `AppState` would force every other route in Plan 2d to depend on database initialization + Orderly creds. Plan 2d doesn't currently take any DB config. Either (a) Phase 8 must extend `AppState` (but Plan 2d hasn't shipped, so the extension is theoretical), or (b) Phase 8 needs its own sub-state passed via `Router::with_state` at sub-router level, or (c) Phase 8 ships the fallback standalone crate.
- **Fix:** Default to the fallback path. Document that the lift-into-2d is a follow-up, not a Phase 8 deliverable. This also derisks Plan 2d slipping.

### 3.5 [MEDIUM] Phase 6 dynamic quota's "fetch recent PnLs from ledger" needs a concrete query
- **Location:** plan Task 6.1 Step 4 ("In `OrderDispatcher::dispatch`, before `try_reserve(...)`, fetch recent PnLs from ledger and compute `quota_factor`").
- **Issue:** `Ledger::realized_pnl_window(strategy_id, since_ms)` returns a sum, not the per-trade list `closed_pnls_30d` that `compute_quota_factor` expects. Need a new ledger method: `closed_pnls_window(strategy_id, since_ms) -> Vec<f64>`. Same for `rolling_drawdown_30d` — needs a peak-trough computation across the 30-day equity curve, which the ledger doesn't currently expose.
- **Fix:** Add the missing ledger queries to Phase 1 Task 1.3 (or as a Phase 6 sub-task). Drawdown computation in particular is non-trivial — needs to walk the equity curve, not just sum PnLs.

### 3.6 [MEDIUM] Bulk-edit (Phase 8 Task 8.5) is a two-bullet "add JS" but interaction model is undefined
- **Location:** plan Task 8.5 Step 2.
- **Issue:** Shift-click range select + apply-to-selection across heterogeneous fields (e.g., user shift-clicks 4 cells in the Hard Cap column, then 2 cells in the Active Hours column — what's the semantics?). Atomic transaction or per-cell? Confirm modal shows old→new for all? Server-side bulk endpoint or N parallel POSTs? None specified.
- **Fix:** Drop bulk-edit from v1 or scope it to "select rows, apply value to one field across all selected." Add a server-side `POST /budgets/bulk` taking `{strategy_ids: [..], field: "...", value: "..."}`.

### 3.7 [MEDIUM] AES-256-GCM key derivation from `CREDENTIAL_SECRET` env var — no KDF, no rotation
- **Location:** plan Task 4.7 Step 1 (`secret = hex::decode(env::var("CREDENTIAL_SECRET")?)`).
- **Issue:** Using a raw 32-byte hex secret as the AES key is fine cryptographically but operationally fragile — one secret encrypts all trading keys for all users. If `CREDENTIAL_SECRET` is rotated, every encrypted blob in the DB must be re-encrypted. No migration story. Also, no per-user salt means the same plaintext under the same secret produces detectably similar ciphertexts (well, the random nonce mostly mitigates this, but per-user key derivation is hygienic).
- **Fix:** (a) Derive per-user key via HKDF(secret, salt=user_id). (b) Add a `key_rotation` migration path: decrypt with old secret, re-encrypt with new. (c) Document the rotation procedure in MANUAL.md. For v1 hackathon, (a) is the minimum; (b) and (c) can be follow-ups.

### 3.8 [MEDIUM] No connection-pool sizing or WAL mode for SQLite under concurrent reservations
- **Location:** Phase 1 + Phase 2 throughout — `SqlitePoolOptions::new().max_connections(1)` in tests.
- **Issue:** SQLite with default settings serializes all writes through a single writer. Under concurrent reservations from the dispatcher (Phase 2 Task 2.3 is explicitly tested with concurrent writes), this works but each write blocks. For a single-operator hackathon load this is fine. For autoresearcher-mutator-spawned strategies (potentially N variants firing simultaneously), reservation throughput is bounded by SQLite's serialized-write rate (~hundreds/sec, fine; but document).
- **Fix:** Enable WAL mode at pool init: `PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL;` Add a comment that for v1 single-operator load, this is more than enough. For multi-tenant v2, evaluate Postgres.

### 3.9 [MEDIUM] Pre-trade simulation depends on a static OrderbookSnapshot in tests but the prod variant is "~30 lines, mechanical wrapping"
- **Location:** plan Task 3.1 Step 3.
- **Issue:** Fetching `GET /v3/orderbook/{symbol}` per dispatch adds 100–500ms tail latency to every order. For a strategy with `max_orders_per_minute=10`, this is fine. For a HFT-ish strategy (would not exist in v1, but the plan's allowlist permits up to `max_orders_per_minute=30` per the dashboard sketch), 500ms slippage in the simulation step could matter. Also, what happens when the orderbook fetch fails? The plan doesn't say. Per spec §6 "Pre-trade simulation fails or returns stale data → Dispatcher rejects the order (fail-closed)" — the plan needs an explicit test for this.
- **Fix:** Add a test: simulator returns Err → dispatcher writes `Simulate` audit row with `error: ...` and returns `Vetoed { reason: "simulation failed" }`. Cache orderbook for 1s to amortize.

### 3.10 [MEDIUM] `xvn audit strategy --since 1h` parsing is one-liner in plan but is multi-format
- **Location:** plan Task 4.9 Step 2 ("parse the relative-time string into a millis offset").
- **Issue:** Operators will pass "1h", "1d", "yesterday", "2026-05-01", "2026-05-01T13:00:00Z". The plan glosses this. Use `humantime` crate (already in many Rust projects) or `chrono-english` — but the plan doesn't add either dep.
- **Fix:** Add `humantime = "2"` to workspace deps; document accepted formats in `cli-reference.md`.

### 3.11 [LOW] Task 4.7 Step 2 "Open browser to a templated URL" is one ellipsis line
- **Location:** plan Task 4.7 Step 2.
- **Issue:** Opening a browser cross-platform: `webbrowser` crate. Templating an Orderly registration URL: not currently a feature of Orderly's web UI (there's no deep-link path to "pre-fill add_orderly_key with this pubkey"). This is wishful — the operator has to open the URL, paste the pubkey by hand, click through Orderly's UI. Plan should either drop the auto-open or document the manual fallback.
- **Fix:** Drop the auto-open promise; print explicit copy-paste-able steps including the pubkey hex, scope, and expiration unix timestamp.

---

## 4. Sequencing / dependency problems

### 4.1 [HIGH] Phase 4 Task 4.7 (`xvn key issue`) lands in Phase 4 but Phase 1 Task 1.5 already needs per-user key plumbing
- **Location:** Phase 1 Task 1.5 (modify Orderly executor to take per-user key); Phase 4 Task 4.7 (issue/store the key).
- **Issue:** Task 1.5 says "modify the executor's constructor to accept... `strategy_id: &str`" but doesn't mention the trading key — yet the spec explicitly says the dispatcher should use the user's encrypted key. If Phase 1 still uses the env-var key (`ORDERLY_KEY/SECRET`), the test in Task 1.5 passes but the security model is not validated end-to-end until Phase 4. That's OK for an incremental ship (spec §8 step 7 explicitly defers multi-key), but the plan's File structure includes `xianvec-identity/src/trading_key.rs` as new in Phase 4 — and the spec's component map says "needs per-user key parameter (currently single env-var key)" against `orderly.rs`. There's no task that bridges single-env-key → multi-user-key.
- **Fix:** Add a Phase 4 sub-task "Refactor `OrderlyExecutor` constructor to accept a `Box<dyn KeyProvider>` returning the Ed25519 key for a given user_id. v1 default impl: `EnvKeyProvider` (returns the single env-var key for any user). Phase 4 Task 4.7 wires the encrypted-store backed `EncryptedStoreKeyProvider`."

### 4.2 [HIGH] Phase 0 ADR outcomes drive Phase 5 but also affect Phase 3 dispatcher
- **Location:** Phase 0 Task 0.4 ADR 0013; Phase 3 Task 3.2 dispatcher.
- **Issue:** If G2 = ISOLATED_SUPPORTED, the dispatcher must set the margin mode on Orderly *before* placing the first order against a symbol (per Task 5.1 Step 2). That's a dispatcher-side change, not just a Phase 5 add-on. The plan's Phase 3 dispatcher does not have a margin-mode-set hook.
- **Fix:** In Phase 3 Task 3.2, add a parameter `margin_mode: Option<MarginMode>` to dispatch and an idempotent "ensure margin mode" call before submit. Then Phase 5 Task 5.1 just toggles whether the parameter is non-None.

### 4.3 [HIGH] Phase 8 Task 8.6 makes `xvn budget serve` depend on `xianvec-dashboard`, but Phase 4 Task 4.8 already declares `Serve` arm calling that dep
- **Location:** Phase 4 Task 4.8 Step 5; Phase 8 Task 8.6.
- **Issue:** If Phase 4 ships before Phase 8, `xvn budget serve` is a placeholder that says "Phase 8 UI not built yet." If `xianvec-dashboard` isn't a workspace member at Phase 4 time, even adding a dep would fail. The plan has Phase 4 build CLI before Phase 8 builds the route, with no compile-time guard. Either Phase 4 needs a feature flag or the Serve arm must conditionally compile.
- **Fix:** Phase 4 ships `Serve` returning a "not yet implemented" message. Phase 8 wires it up. Document the order.

### 4.4 [MEDIUM] Phase 2 Task 2.3 reservations crate-level circular dep
- **Location:** plan Task 2.3 — `xianvec-risk/src/reservations.rs` imports `xianvec_data::ledger::Ledger`.
- **Issue:** Currently `xianvec-risk` does NOT depend on `xianvec-data`. Adding the dep is fine, but it's not in the plan's `xianvec-risk/Cargo.toml` modify list.
- **Fix:** Phase 2 Task 2.1 Step 1 should add `xianvec-data = { path = "../xianvec-data" }` to xianvec-risk's Cargo.toml.

### 4.5 [MEDIUM] Phase 1 Task 1.5 integration test "mock the Orderly HTTP layer" — mock infrastructure unspecified
- **Location:** plan Task 1.5 Step 4.
- **Issue:** No existing mock pattern in `crates/xianvec-execution/tests/` is named or extended. The existing `submit_buy_with_bracket_constructs_correct_orders` test (line 1064 of `orderly.rs`) presumably uses a `MockOrderlyApi` — but the plan doesn't reference it.
- **Fix:** Reference the existing mock pattern explicitly. If it doesn't exist, build one in Phase 1.

### 4.6 [MEDIUM] Phase 3 Task 3.3 wiring breaks every existing call site of `OrderlyExecutor`
- **Location:** plan Task 3.3 Step 2.
- **Issue:** Existing call sites in `xianvec-engine`, `xianvec-cli/src/commands/fire_trade.rs`, harness paths, eval paths — many of them. Each needs a `(strategy_id, user_id, cfg)` triple. For backtest paths (`ab_compare`, paper-only flows), there is no real "user" or "strategy_id" outside the new schema. The plan says "hardcode `hackathon-baseline` as `strategy_id`" — but for `ab_compare` running 10+ strategies, hardcoding one breaks attribution.
- **Fix:** Provide a `default_strategy_id_for(arm_name)` helper that maps arm names to deterministic ULID-shaped strategy_ids. Document the migration: post-SLF3 (NFT mint), this resolves to the on-chain id. Also: backtest paths should bypass the dispatcher entirely (no risk envelope, no audit log) and only the live path uses dispatch. Add a `BypassDispatcher` mode for backtests.

### 4.7 [LOW] Phase 9 e2e test depends on Phase 7 reconciler being non-todo
- **Location:** plan Task 9.1 Step 1 ("Run reconcile, assert no drift").
- **Issue:** Phase 7 Task 7.1 leaves `Reconciler::run` as `todo!()` per the plan's own placeholder scan. Phase 9 e2e depends on a working reconciler.
- **Fix:** Tighten Phase 7 Task 7.1 with a real implementation before claiming Phase 9 ready.

---

## 5. Hackathon-deadline feasibility

### 5.1 [HIGH] Five-week deadline; plan as-written is 6–8 weeks of focused single-operator work
- **Location:** plan header "Hackathon deadline: 2026-06-15. Phases 0–9 ship pre-deadline."
- **Issue:** Estimate per phase, single-operator, conservatively, with the issues found above factored:
  - Phase 0: 3–5 days (probes need real EIP-712, see §3.1)
  - Phase 1: 3–4 days (six new tables + ledger + audit + status + policy + pending + tests)
  - Phase 2: 3 days (config schema, eval matrix, race-free reservations + concurrency tests)
  - Phase 3: 4–6 days (dispatcher + simulation + wiring through existing paths, see §4.6)
  - Phase 4: 6–8 days (eight CLI commands, key issuance + EIP-712, approval workflow, auto-halt; this is the chubbiest phase)
  - Phase 5: 1–2 days (one branch only)
  - Phase 6: 2 days (quota + drawdown query + property tests)
  - Phase 7: 2–3 days (reconciler + funding ingestion if blockers §1.1 honored)
  - Phase 8: 4–6 days (UI integration with another team's dashboard or fallback crate)
  - Phase 9: 2 days (e2e test against testnet, runbook)
  - **Subtotal: 30–41 working days = 6–8 calendar weeks** for a single contributor with no parallelization.
- **Issue compounded:** This plan is one of *several* hackathon plans (autoresearcher, marketplace contracts, dashboard, scheduler, ERC-8004 deployment, demo polish). The operator cannot dedicate 6–8 weeks to wallets alone.
- **Fix (recommended minimum-viable subset for hackathon demo):**
  - **Ship:** Phase 0 (probes), Phase 1 (schema + ledger + audit), Phase 2 (per-strategy rules + reservations), Phase 3 (dispatcher with bps→USDC notional + simulation), Phase 4 Tasks 4.2 + 4.4 + 4.7 + 4.10 (kill, emergency-close, key issue, reconcile CLI), Phase 6 (quota), Phase 9 (e2e + runbook).
  - **Defer:** Phase 4 Tasks 4.5 (approval gate), 4.8 (`budget set` UI sub-flow keeping `budget show` only as TOML printout), 4.9 (audit CLI — operators can SQL the DB), 4.11 (CLI reference doc); Phase 5 (margin guard — accept manual operator vigilance for hackathon); Phase 7 reconciler in cron mode (run `xvn reconcile` manually every few hours); all of Phase 8 (UI — hackathon demo can show the spreadsheet via a CLI table).
  - **Cut entirely from v1 demo, add to FOLLOWUPS:** funding attribution (§1.1) — accept it as a known v2 gap; trading-keys migration table (§1.2) — single-key env-var path holds for single-operator demo; phishing-resistant browser flow (§1.4) — operator does the registration manually once.
  - This subset is ~3–4 weeks, leaving buffer for the autoresearcher and demo polish.

### 5.2 [HIGH] Phase 8 hard-couples to Plan 2d shipping — a separate plan with its own slippage risk
- **Location:** plan Phase 8 preamble.
- **Issue:** Plan 2d (web dashboard) is itself ~17 tasks. The plan acknowledges this with the fallback path but treats the primary path as default. If Plan 2d slips, Phase 8 either rebuilds the standalone or waits.
- **Fix:** Make the fallback the **default** path. The lift-into-2d becomes a post-hackathon cleanup task. This eliminates the dependency.

### 5.3 [MEDIUM] Phase 0 probes require Orderly *mainnet* with real $10 USDC to verify G1
- **Location:** Task 0.2 README ("Run on Orderly mainnet with a test account holding ≤ $10 USDC").
- **Issue:** Mainnet onboarding takes a deposit (the Orderly Vault contract on Mantle mainnet, USDC bridge if not already on Mantle). For a hackathon team, this is friction — and bridging USDC to Mantle is its own minor saga. Why not Orderly testnet?
- **Fix:** Verify if Orderly Sepolia testnet exposes `add_orderly_key` with the same scope semantics. If yes, run probe there; risk = zero. If no, document the mainnet $10 funding step as a Day 1 ops task.

---

## 6. Testing gaps

### 6.1 [HIGH] Spec §9 "Adversarial: simulate compromised trading key" not in plan
- **Location:** spec §9.
- **Issue:** The spec calls for an adversarial test that "spams order requests at full hard cap → verifies no withdrawal possible, daily-loss circuit breaker fires, frequency caps reject excess orders, user can revoke key via Orderly UI, operator can `xvn kill --user <id>`." Plan has no such test.
- **Fix:** Add Task 9.1a "Adversarial test": script that uses the operator's own trading key (against testnet or mock), spams 1000 order intents in 60s, asserts: (a) `> max_orders_per_minute × 60` are rejected by frequency cap; (b) within 1s of `xvn kill --user op`, no further orders submitted; (c) any attempt to call withdraw via the trading key returns 403/401.

### 6.2 [HIGH] No test that audit log is actually append-only at the SQL level
- **Location:** plan Task 1.2 Step 1 (`no_update_or_delete_methods_compile` is a no-op test by absence).
- **Issue:** Spec §3.8 says "the table is append-only at the application layer (no UPDATE / DELETE statements in dispatcher code paths)." Plan acknowledges this but does not enforce it. A future contributor could add an UPDATE in audit.rs and break it.
- **Fix:** Add a SQLite trigger: `CREATE TRIGGER decisions_no_update BEFORE UPDATE ON decisions BEGIN SELECT RAISE(ABORT, 'decisions is append-only'); END;` and same for DELETE. Test: attempt UPDATE in a test, expect failure.

### 6.3 [HIGH] Spec §9 "Concurrency: N concurrent strategies all at 90% of cap" — plan tests only 2 strategies
- **Location:** plan Task 2.3 test (`concurrent_reservations_respect_cap` — 2 concurrent attempts).
- **Issue:** The spec is explicit about N. With only 2, you don't catch the case where the per-strategy lock map's entry-creation race causes a second-arrival lock to be a different `Arc<Mutex>` than the first-arrival's lock (it doesn't — `or_insert_with` is atomic via `Mutex<HashMap>`, but the test doesn't prove this).
- **Fix:** Parameterize the test for N=10, N=100. Use proptest to randomize notional + cap.

### 6.4 [HIGH] No test for the property "audit log records ALL stages for every order"
- **Location:** spec §9 ("Audit-log completeness: for every order, verify that all expected stages are present with valid payload hashes"); plan Phase 9 Task 9.1 mentions stages but doesn't enforce the property.
- **Fix:** Add a property test: generate random sequences of dispatches (some succeed, some veto, some fail at simulate, some at submit). For each `position_id`, assert the audit log contains the expected stages for its outcome (e.g., a simulate-rejected position has `emit, risk_eval, simulate(reject)` and no more).

### 6.5 [MEDIUM] No test for content-hash determinism across processes / restarts
- **Location:** plan Task 1.2 (test `payload_hash_is_deterministic` only verifies same-process).
- **Issue:** `serde_json::to_string` does not guarantee field-order stability across versions/platforms. A serializer change (or a HashMap-backed payload) could produce different hashes for "the same" payload, breaking the forensic-trail property.
- **Fix:** Use `serde_json::to_string_pretty` with a canonical-JSON variant, or sort keys before hashing. Or document the limitation.

### 6.6 [MEDIUM] No test for the global halt
- **Location:** §1.3 (per blocker above) — also: no test even exists for the missing functionality.
- **Fix:** Once §1.3 is implemented, add a test.

### 6.7 [MEDIUM] No test for AppContext bootstrap failure modes
- **Location:** plan Task 4.10.
- **Issue:** `from_env` ignores missing `XVN_DB_PATH`, missing `CREDENTIAL_SECRET`. If `CREDENTIAL_SECRET` is missing when `xvn key issue` runs, the failure surface is unclear. Same for missing migrations.
- **Fix:** Add tests for each missing-env-var failure mode with a clear error message asserted.

### 6.8 [LOW] `Side` parsing test missing
- **Location:** plan Task 1.3 (Side is `LONG`/`SHORT` string, but no test that an invalid string fails the CHECK constraint cleanly).

### 6.9 [LOW] Phase 8 spreadsheet has no test that `policy_changes` is journaled on POST
- **Location:** plan Task 8.4 Step 3 stub.
- **Fix:** Add the test body explicitly.

---

## 7. Operational / deployment gaps

### 7.1 [HIGH] No backup / restore plan for the SQLite event store
- **Location:** Plan-wide.
- **Issue:** The audit log is the forensic record. If the SQLite file is on ephemeral storage (Fly.io VMs default to this unless a volume is attached), a process restart wipes it. The MANUAL.md addendum (Task 9.2) doesn't mention persistence.
- **Fix:** Add a deploy note: SQLite file lives on a mounted Fly volume; nightly `sqlite3 .backup` to S3 or equivalent. For hackathon, at least document the current single-volume risk.

### 7.2 [HIGH] No alerting for "all strategies stuck at quota_factor=0" (spec §6 explicit failure mode)
- **Location:** spec §6; plan does not implement.
- **Fix:** Add a Phase 7 sub-task: a metrics emitter that exports `quota_factor` per strategy. A simple "if N strategies all have quota=0 for > 1h, log critical" check in the reconciler loop.

### 7.3 [HIGH] `CREDENTIAL_SECRET` rotation procedure undocumented
- **Location:** Task 4.7 Step 1 — no rotation story.
- **Fix:** Document in MANUAL.md: "to rotate, decrypt all `trading_keys.encrypted_blob` with old secret, re-encrypt with new, swap env var, restart." Provide a `xvn key migrate-secret` helper that does this transactionally.

### 7.4 [HIGH] No monitoring of reservation-leak rate (spec §6 explicit)
- **Location:** spec §6 ("If reservations consistently leak..., surfaces as 'reservation expiry rate exceeds threshold' alert.").
- **Fix:** Reaper logs a metric `reservations_reaped_total` and `reservations_reaped_rate_5m`. Alert at >5/min sustained.

### 7.5 [HIGH] Settlement wallet operations (sweep cadence, multi-sig procedure) not documented in runbook
- **Location:** Task 9.2 Step 1 enumerates topics — does not mention settlement wallet.
- **Fix:** Add a "Settlement wallet" sub-section: how to check balance, how to sweep, what address it points to, who has multi-sig signing rights, escalation path on compromise.

### 7.6 [MEDIUM] No log redaction for the trading key
- **Location:** Spec §5 ("Never logged"); plan does not enforce.
- **Issue:** The `tracing::info!(pubkey = ?hex::encode(...))` pattern in the m1 probe is fine for the public key, but the same `?` debug-formatter on a `SigningKey` would dump the secret. There is no compile-time guard. A future contributor adding a debug-print could exfiltrate the key into Fly.io logs.
- **Fix:** (a) Newtype the trading key with `Drop` zeroing and a `Debug` impl that prints `"<redacted>"`. (b) Add a clippy lint denying `Debug` on `SigningKey` directly.

### 7.7 [MEDIUM] No metrics export at all
- **Location:** Plan-wide.
- **Issue:** No Prometheus / OpenTelemetry surface. Given the operator is the only user, manual `tail -f`-on-tracing is acceptable for v1, but the plan doesn't say so.
- **Fix:** Add a one-line acknowledgement in MANUAL.md: "v1 ops is `tail -f` on tracing; metrics export deferred to v2."

### 7.8 [MEDIUM] Disaster recovery: how does the operator restore from an audit-log-only backup?
- **Location:** Plan-wide.
- **Issue:** The audit log claims to be the forensic source of truth. Could the entire `positions` table be reconstructed from the `decisions` table? Probably yes for stages with full payloads — but the plan doesn't say.
- **Fix:** Document or test this property: starting from an empty positions table and a full decisions table, replay all `Submit` and `Fill` and `Close` audit rows to rebuild positions. If this holds, document it; if not, identify the gap.

### 7.9 [LOW] `XVN_OPERATOR_CONFIRMED=1` env-var prevents accidental scripting but is itself trivial to set
- **Location:** Task 4.2 Step 3.
- **Issue:** Belt-and-suspenders defense, but the bar is low. A compromised CI workflow with that env var set could `xvn kill --all --yes`. Worth noting in MANUAL: do not set this in any persistent shell.
- **Fix:** Documentation only.

---

## 8. Integration with the rest of xianvec

### 8.1 [HIGH] Plan 2c ships `Plan 2c BrokerSurface` which abstracts Alpaca + Orderly — this plan modifies `OrderlyExecutor` directly, bypassing `BrokerSurface`
- **Location:** Plan 2c Task 7 — `BrokerSurface` trait in `crates/xianvec-execution/src/broker_surface.rs`; this plan modifies `crates/xianvec-execution/src/orderly.rs` and adds a separate `OrderlyOrderSubmit` trait.
- **Issue:** Two parallel abstractions over Orderly. Plan 2c's `BrokerSurface` is the cross-broker abstraction (Alpaca paper, Alpaca live, Orderly live). This plan's `OrderlyOrderSubmit` is Orderly-specific. The dispatcher in this plan is hard-bound to Orderly. For a hackathon demo against Orderly only this is fine, but the conceptual fit with Plan 2c is wrong: the dispatcher should route through `BrokerSurface`, not bypass it.
- **Fix:** Either (a) rename `OrderlyOrderSubmit` → reuse `BrokerSurface` from Plan 2c if Plan 2c has shipped, or (b) document that the dispatcher is Orderly-only in v1 and integrating with `BrokerSurface` is a follow-up. Either works; pick.

### 8.2 [HIGH] SLF3 NFT-mint dependency: pre-mint strategy_id is "local ULID" but the plan never specifies the mapping
- **Location:** spec §3.5 ("Pre-mint, the same id is used as a local ULID and resolves to the NFT id at mint time"); plan does not specify the resolution mechanism.
- **Issue:** When SLF3 ships and mints an NFT for an existing strategy, the on-chain `agent_id` (a different number — ERC-721 token id) needs to map back to the existing local ULID in the `positions` and `decisions` tables. Otherwise the historical attribution is broken or requires migration.
- **Fix:** Add a `strategy_id_aliases` table: `(local_ulid TEXT, onchain_token_id TEXT, mapped_at INTEGER)`. After mint, write a row. Reads of `positions.strategy_id` resolve via this table (or just keep the ULID and reference the NFT by alias).

### 8.3 [HIGH] FOLLOWUPS F18 (`asset` on `TraderDecision`) is a hard prerequisite per §2.2 above
- **Location:** FOLLOWUPS F18.
- **Issue:** F18 is marked "blocking for multi-asset" but is also blocking for this plan's dispatcher (which dispatches per-asset). If the v1 demo is BTC-only, the dispatcher hardcodes `PERP_BTC_USDC` and F18 stays deferred. Plan should say so.
- **Fix:** Add explicit BTC-only assertion in Phase 3 Task 3.2; reference F18.

### 8.4 [MEDIUM] Plan 5 marketplace settlement-wallet address is referenced as a config knob but no task sets it
- **Location:** plan File structure mentions `protocolFeeRecipient` settlement wallet; no task creates one or wires it.
- **Issue:** Marketplace contract's `protocolFeeRecipient` is set at deploy time. Wallet generation, multi-sig setup, address recording — all manual operator work. Not in plan.
- **Fix:** Phase 9 runbook should include "generate settlement wallet via 1Password, record in ops doc, set as `protocolFeeRecipient` when Plan 5 deploys."

### 8.5 [MEDIUM] ERC-8004 (SLF2) deployment status — wallet rail does not depend on it, but reputation writes (SLF4) DO read from `positions`
- **Location:** plan header ("SLF2 — Independent of this plan; wallet rail does not depend on registries existing").
- **Issue:** True for this plan. But SLF4's reputation writes will read from `positions` and `funding_attributions`. If §1.1 (funding attribution) is dropped from v1 per §5.1's minimum subset, SLF4's PnL inputs will be biased. Worth flagging the cross-dependency.
- **Fix:** Document in FOLLOWUPS.md: "SLF4 reputation writes assume `realized_pnl_usdc` includes funding; if funding attribution is deferred, reputation values will be biased high for long-hold strategies in negative-funding regimes."

### 8.6 [LOW] Plan 2d's Live cockpit shows trade decisions — does it read from `decisions` or its own scheduler_events?
- **Location:** Plan 2d Live cockpit; this plan's `decisions` table.
- **Issue:** Two event streams. The Live cockpit will need to merge or pick one. Not a blocker for this plan, but a coordination point.
- **Fix:** Note in FOLLOWUPS that the Live cockpit should join `scheduler_events` (Plan 2c) + `decisions` (this plan).

---

## 9. Code-level problems in the plan's example code

### 9.1 [HIGH] Dispatcher reservation cap calculation is wrong when quota_factor < 1
- See §2.5 above for full detail. Specifically: the dispatcher passes `cap = cfg.hard_cap_usdc_notional` to `try_reserve`, with no quota application. Phase 6 Task 6.1 Step 4 promises to multiply, but the code in Phase 3 Task 3.2 doesn't have the multiplication.

### 9.2 [HIGH] `Simulator` `mid` calculation crashes on empty book
- **Location:** plan Task 3.1 Step 2 — `let mid = (book.bids[0].0 + book.asks[0].0) / 2.0;`
- **Issue:** Index panic if either side is empty. In a thin or one-sided book this is plausible.
- **Fix:** `match (book.bids.first(), book.asks.first()) { (Some(b), Some(a)) => ..., _ => return Err(anyhow!("empty book")) }`.

### 9.3 [HIGH] `is_within_active_hours` is wrong for the wrap-midnight case
- **Location:** plan Task 2.2 implementation — `(Some(n), Some(s), Some(e)) => n >= s || n <= e, // wraps midnight`.
- **Issue:** "20:00-04:00" with `now=03:00`: `s=1200, e=240, n=180`. `180 >= 1200` false, `180 <= 240` true → in-window. OK. With `now=05:00`: `n=300`. `300 >= 1200` false, `300 <= 240` false → out-of-window. OK. With `now=23:30`: `n=1410`. `1410 >= 1200` true → in-window. OK. With `now=12:00`: `n=720`. `720 >= 1200` false, `720 <= 240` false → out-of-window. OK. **Actually this is correct.** Withdrawn — but I am leaving the entry to flag that the parsing is brittle: "24:00" parses as 1440 minutes which would compare badly elsewhere. Test "24:00-24:00" or "00:00-24:00" with now="24:00" (which wouldn't be valid time anyway).
- **Fix (revised, MEDIUM):** Reject "24:XX" in the parser; only "00:00-24:00" is allowed as the always-on sentinel. Add tests for the wrap-midnight case explicitly.

### 9.4 [HIGH] `quota_factor` returns 0.25 for cold start regardless of drawdown — even with rolling_drawdown_30d = 1.0
- **Location:** plan Task 6.1 — `if i.closed_pnls_30d.len() < COLD_START_MIN_SAMPLES { return COLD_START_FLOOR; }`
- **Issue:** A brand-new strategy whose first 5 trades are all losses producing 50% drawdown still gets 0.25 quota. The drawdown_decay term is bypassed entirely during cold start. Counterintuitive and unsafe.
- **Fix:** Apply drawdown decay during cold start too: `return COLD_START_FLOOR * (1 - dd / DRAWDOWN_FLOOR).max(0.0);`

### 9.5 [HIGH] AES-GCM nonce reuse risk: `OsRng` per-message is correct, but no AAD
- **Location:** plan Task 4.7 Step 1.
- **Issue:** No additional authenticated data (AAD) — fine per se, but if the encryption format ever needs versioning, there's no header byte. Also: storing as `nonce_hex:ct_hex` is a colon-delimited tuple; if ciphertext ever contains a colon (it can — colons are valid hex output? actually no, hex output is `0-9a-f`, no colon, so OK). But the format has no version byte. Future migration to a different AEAD (e.g., XChaCha20-Poly1305) would require a flag day.
- **Fix:** Prefix with a version byte: `v1:nonce_hex:ct_hex`. Decrypt routes on `v1` to AES-GCM.

### 9.6 [MEDIUM] `OrderDispatcher::dispatch` never reads `orders_in_last_minute` / `orders_in_last_hour` — passed in as parameters
- **Location:** plan Task 3.2 — dispatch takes these as args.
- **Issue:** Where do they come from? The caller has to track them. There's no helper that queries the audit log "count emit-stage rows for strategy X in last minute" — which is the natural source. Pushing this responsibility to every caller is leaky.
- **Fix:** Compute internally from the audit log (`SELECT COUNT(*) FROM decisions WHERE strategy_id = ? AND stage = 'submit' AND occurred_at > ?`). Add to ledger or audit module.

### 9.7 [MEDIUM] `compute_quota_factor`'s sigmoid arg uses `i.closed_pnls_30d.len() as f64` for division but the spec's Sharpe is over a normalized window
- **Location:** plan Task 6.1 implementation; spec §3.4 formula.
- **Issue:** `mean = sum / len`, `var = sum-of-squares / len`. This is mean-PnL, not annualized-Sharpe. Two strategies with different trade frequencies but identical per-trade returns will get different "Sharpe"s. The spec's intent ("rolling_sharpe(s, last_30d)") suggests time-normalized Sharpe, not per-trade.
- **Fix:** Either annualize (multiply by sqrt(periods/year)) or document that this is per-trade Sharpe and tune `SHARPE_NORMALIZER` accordingly. For hackathon, per-trade is OK; document.

### 9.8 [LOW] `m1` probe's `register_trading_only_key` payload uses `chain_id: 5000` (Mantle mainnet) for the withdraw attempt
- **Location:** plan Task 0.2 Step 5.
- **Issue:** OK if probe runs against Mantle mainnet (per README "Run on Orderly mainnet"). But if probe is moved to testnet (per §5.3 fix), `chain_id` must be Mantle Sepolia (5003).
- **Fix:** Read `MANTLE_CHAIN_ID` from env, default to mainnet for v1.

### 9.9 [LOW] `tokio::sync::Mutex` for the per-strategy lock map is overkill — `std::sync::Mutex` would do (no `.await` inside the critical section is held)
- **Location:** plan Task 2.3.
- **Issue:** The `lock_for(strategy_id)` only inserts/clones; no async work. Using `std::sync::Mutex` avoids the dependency on tokio's mutex (which has more overhead).
- **Fix:** Cosmetic; use `std::sync::Mutex<HashMap<...>>` for the outer map. Inner mutex must remain `tokio::sync::Mutex` (held across await points).

---

## 10. Failure modes the spec acknowledges but the plan misses

Mapping spec §6 failure modes against the plan:

| Spec §6 failure mode | Plan handling | Verdict |
|---|---|---|
| Orderly key registration fails or expires | "fails closed: emits 'key invalid' alert, halts new orders" | **Missing** — plan has no key-expiry check in dispatcher (see §1.7); no alerting (see §7.4) |
| Risk Engine quota function buggy and starves | "Surfaces as 'all strategies stuck at quota_factor=0' alert" | **Missing** — see §7.2 |
| Rogue strategy spamming orders | "Frequency caps reject excess orders. Auto-trigger eventually halts." | **Partial** — frequency caps tested in Task 2.2, auto-trigger in Task 4.1; but the *signal* (rogue detection beyond frequency cap) is absent — what about a strategy that emits 9/min for an hour, all approved, all losers? |
| Cross-margin contagion | Phase 5 fork | **OK** (with caveat from §1.9) |
| Pre-trade simulation fails or returns stale data | "Dispatcher rejects (fail-closed)... operator can disable via xvn config set" | **Partial** — plan implements rejection but not the `xvn config set simulate_required false` audit-logged escape hatch |
| Reservation TTL leaks | "TTL self-resolves; surfaces as alert if rate exceeds threshold" | **Missing alerting** — see §7.4 |
| Race on simultaneous decisions | "Reservation pattern serializable per-strategy" | **Tested** with N=2 (see §6.3 — needs N=10+) |
| Attribution ledger out of sync | "Reconciliation job detects and surfaces" | **Reconciler is `todo!()`** in plan |
| Marketplace contract compromised mid-sale | "Atomic; pause via multi-sig" | **N/A** for this plan (Plan 5) |
| Settlement wallet compromised | "Operator loses unswept fees. Issue contract upgrade." | **Documented missing** — Task 9.2 doesn't cover this; see §7.5 |

**Net:** at least four explicit failure modes have no detection/handling in the plan. Add tasks (or runbook entries) for each.

---

## 11. Validation gates G1/G2 outcome handling

### 11.1 [MEDIUM] G1=REJECTED has no actual remediation path beyond "halt the plan and escalate"
- **Location:** Plan Task 0.4 Step 1 ADR template ("Halt this plan and trigger redesign — likely path is a smart-account wrapper or deposit-only-working-capital mode").
- **Issue:** Both alternatives are major redesigns, not branches of this plan. The plan should at least sketch the deposit-only-working-capital path as a fallback (it's not a full redesign — it's "user funds Orderly with $X they accept losing entirely; trading-only key compromise = full $X loss"). For hackathon with $100 testnet, this is acceptable; the spec rejects it as a primary design but it's a valid fallback.
- **Fix:** Add ADR template alternative: "If G1=REJECTED: ship in deposit-only-working-capital mode for hackathon demo. Document the regression in spec §1.1 and demo with a test account holding $100 USDC. v2 must redesign before mainnet expansion."

### 11.2 [LOW] G2 outcome doesn't gate Phase 3 dispatcher per §4.2 — just Phase 5
- See §4.2.

### 11.3 [LOW] No ADR for the case where G2 returns "Unknown" (probe can't determine)
- **Location:** Task 0.3 Step 2 has `MarginModeResult::Unknown(reason)` but Task 0.4 ADR 0013 only has two outcome states.
- **Fix:** Add a third outcome — "MANUAL_REVIEW_NEEDED" → block Phase 5 until resolved.

---

## 12. Security claims in the spec that the plan does not enforce

### 12.1 [HIGH] "Never logged" (key) is not enforced anywhere
- See §7.6 above.

### 12.2 [HIGH] "Memory zeroization on key drop" implied by §10 ("v1 stores the Ed25519 trading key encrypted at rest with AES-256-GCM, decrypted in process memory at signing time") not implemented
- **Location:** Plan Task 4.7.
- **Issue:** After `decrypt(blob)` returns the raw 32-byte key, that `Vec<u8>` lives until dropped — and there's no zeroization. The `zeroize` crate is the standard solution (Rust ecosystem common). Worth using even in v1.
- **Fix:** Wrap decrypted key in `Zeroizing<Vec<u8>>` from the `zeroize` crate. Add to workspace deps.

### 12.3 [HIGH] No CI test that secrets / keys never appear in tracing output
- **Location:** Plan-wide.
- **Fix:** Add a CI step: `cargo test 2>&1 | grep -E '[a-f0-9]{64}'` should not match the trading key's hex. Or, more rigorously, use a CI grep for known secret patterns in test output.

### 12.4 [MEDIUM] "Operator owns these knobs; they are the legible, deterministic guarantee" (spec §3.4) — plan allows runtime edits via UI without authentication
- **Location:** Spec §3.4; plan Phase 8 (POST `/budgets/:strategy` is unauthenticated).
- **Issue:** Anyone with network access to the dashboard can modify hard caps. For single-operator localhost-only this is OK (binds to 127.0.0.1 by default). But the moment the dashboard is exposed (Fly deploy), this is an open admin endpoint.
- **Fix:** Document the local-bind-only constraint loudly, OR add a shared-secret auth middleware (`X-XVN-OPERATOR-SECRET` header matching env var) on all mutating routes. For hackathon demo, the local-bind-only constraint is fine.

### 12.5 [MEDIUM] No rate-limit on `xvn approve` API
- **Location:** Plan Task 4.6.
- **Issue:** `approve` resolves the human-in-loop gate. Without a rate-limit, a compromised operator-CLI can approve thousands of pending requests in milliseconds.
- **Fix:** Document that approve is a CLI-only command (no HTTP surface in v1) and that CLI rate-limiting is the operator's responsibility; or add a per-process rate-limit (max 10 approvals/minute).

---

## Summary recommendations

**Before writing a single line of code:**

1. Resolve §1.1, §1.2, §1.3 — the three blocker spec gaps (funding, trading_keys table, global halt).
2. Resolve §2.1, §2.2 — the two existing-code mismatches around `OrderlyExecutor::submit` and `TraderDecision` shape.
3. Decide §5.1's minimum viable subset — explicitly cut Phases / tasks the operator does NOT have time for.

**During Phase 0:**

4. Run probes on testnet if possible (§5.3).
5. Implement EIP-712 signing properly with `alloy-sol-types` (§3.1).

**During Phase 1:**

6. Add `trading_keys` migration and store; add `global_state` migration; add `strategies` migration with config_json; add `strategy_id_aliases` for SLF3 forward-compat.
7. Use `#[sqlx(rename_all = "snake_case")]` not `lowercase` for Stage (§2.10).

**During Phase 3:**

8. Redesign `OrderlyOrderSubmit` trait to wrap-not-replace `OrderlyExecutor::submit` (§2.1); pass signed payload back for audit (§2.4); release reservations on submit failure (§2.7).
9. Drop the `78_000.0` placeholder; pass mark price (§2.11).

**During Phase 4:**

10. Implement Orderly-side `delete_orderly_key` in `xvn key revoke` (§1.8).
11. Implement `xvn key verify` (§1.4).
12. Add `Zeroizing` for decrypted key (§12.2).

**During Phase 6:**

13. Resolve cold-start formula ambiguity (§1.6) and apply drawdown decay during cold start (§9.4).

**During Phase 7:**

14. Implement reconciler body (not `todo!()`) before Phase 9 e2e.
15. Implement funding-payment ingestion (§1.1).

**During Phase 9:**

16. Add adversarial spam-key test (§6.1), audit-log append-only triggers (§6.2), N-strategy concurrency (§6.3), audit-log completeness property test (§6.4).
17. Document settlement wallet ops, secret rotation, backup/restore, log redaction in MANUAL.md.

**Cross-cutting:**

18. Make Phase 8 fallback the default path (§5.2).
19. Add `humantime` workspace dep (§3.10).
20. Decide BrokerSurface vs OrderlyOrderSubmit story before either Phase 3 or Plan 2c lands (§8.1).

---

**End of review.** Findings emphasize correctness and security over cosmetics. The plan is *thorough* — most of the surface is covered. The risks are concentrated in (a) under-acknowledging the existing-code refactor cost, (b) hand-waved sub-tasks at phase boundaries, (c) a feasibility cliff for the hackathon deadline. Trim early; trim hard.
