# Non-Custodial Agent Wallets — Design

> **Status:** Draft · 2026-05-09
> **Terminology:** Updated 2026-05-10 — `strategy_id` renamed to `agent_id` per Option B (see [`docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`](../plans/2026-05-10-terminology-rename-option-b.md)). The id is a local ULID pre-mint, resolves to the NFT token id post-SLF3.
> **Depends on:** [`docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`](./2026-05-08-smart-contract-surface-design.md) (the marketplace contract surface this spec assumes), `architecture.md` §6.1 (Orderly executor), `crates/xvision-risk/` (the engine being extended).
> **Related:** [`crates/xvision-execution/src/orderly.rs`](../../../crates/xvision-execution/src/orderly.rs), [`docs/erc-8004-agent-uses.md`](../../erc-8004-agent-uses.md).

---

## 1. Scope

This spec defines how xvision gives strategy variants the authority to trade *without ever holding user trading capital*. It also defines per-strategy spending controls (the "agent budgets" question) and the relationship between the trading-rail and the marketplace-rail.

**In scope:**

- The two-rail architecture: trading rail (non-custodial) vs marketplace rail (narrow custody for fee routing only).
- How a user connects their Orderly account to xvision via a scoped Ed25519 trading key.
- How xvision stores, scopes, and uses that key per-strategy.
- The hybrid per-agent budget model (hard USDC cap + dynamic quota inside it) and how the Risk Analysis Engine enforces it.
- The off-chain attribution ledger that maps Orderly positions → strategy variants for reporting and marketplace settlement.
- The settlement wallet that receives commission drips from the Marketplace contract.

**Explicitly out of scope:**

- Custody of user trading funds. **xvision never holds user trading capital.** Any design that drifts toward this is rejected by construction.
- The Marketplace / LicenseToken / EvalAttestationRegistry contract internals — those are specified in [`2026-05-08-smart-contract-surface-design.md`](./2026-05-08-smart-contract-surface-design.md). This spec assumes that contract surface and only describes how the trading rail interacts with it.
- Multi-user scaling for the hackathon. v1 supports one operator-as-user; multi-tenant onboarding is a Plan 5 concern.
- Withdrawal-time performance fees. Recorded as a future option in §10.
- Per-trade x402 micropayments. Recorded as a future option in §10.

### 1.1 Validation gates (must clear before implementation)

Two assumptions in this spec are load-bearing for the security model. The plan-stage MUST verify them against Orderly's live API before any code is written. If either fails, the design changes materially.

| Gate | Assumption | If false |
|---|---|---|
| **G1 — trading-only key scope** | Orderly's `add_orderly_key` (or equivalent) supports a permission scope that includes order placement but excludes vault withdrawal and inter-account transfer. | Entire security model collapses. Move to a smart-account wrapper (Safe + custom session-key contract) or operate in deposit-only-working-capital mode (user funds Orderly with only the loss they can afford to lose). |
| **G2 — isolated-margin per position** | Orderly supports per-position isolated-margin mode in addition to the default cross-margin. | Cross-margin contagion risk is unmitigatable; aggregate-margin-utilization cap (§3.4) becomes the only defense and per-strategy hard caps degrade to "intentional-overallocation defense only." |

Both gates are checked once during the implementation plan's first probe. Result is recorded in `decisions/` as an ADR.

---

## 2. Architecture overview — two rails

```
┌────────────────────────────────────────────────────────────────────────────────┐
│  USER  (full custody of trading capital, always)                                │
│   ├─ Mantle EVM wallet           (holds USDC, signs ONE deposit)                │
│   └─ Orderly account, user-owned (holds collateral + positions)                 │
└────────────────────────────────────────────────────────────────────────────────┘
        ▲ trade orders                                       ▲ withdrawals
        │ (scoped Ed25519 trading key,                       │ (user-signed,
        │  cannot withdraw)                                  │  no xvision
        │                                                    │  involvement)
        │
┌───────┴────────────────────────────────────────────────────────────────────────┐
│  XVISION ORCHESTRATOR  (no custody)                                             │
│   ├─ strategy variants → produce trade intents                                  │
│   ├─ Risk Analysis Engine → scoped-permission gate + hard cap × dynamic quota   │
│   │                        + reservation lock + aggregate-margin guard          │
│   ├─ Pre-trade simulator → Orderly order-info; rejects on slippage breach       │
│   ├─ Orderly signer (encrypted Ed25519 key per user, scoped to orders)          │
│   ├─ Approval gate → operator confirm for trades above threshold                │
│   ├─ Kill switches (per-strategy auto + manual; per-user; global)               │
│   ├─ Emergency-close (cancel + market-flat all positions on demand)             │
│   ├─ Audit log (append-only: emit→eval→sim→sign→submit→fill→close)              │
│   └─ Attribution ledger (agent_id → realized PnL + funding)                  │
└────────────────────────────────────────────────────────────────────────────────┘

   ────────  MARKETPLACE RAIL  (separate; the only smart contract xvision owns)  ────────

┌────────────────────────────────────────────────────────────────────────────────┐
│  MARKETPLACE FEE ROUTER  (per smart-contract-surface spec, §3)                  │
│   buyer pays USDC ─→ split 95% creator / 5% platform ─→ mint license to buyer   │
│   atomic; never holds funds across transactions                                 │
└────────────────────────────────────────────────────────────────────────────────┘
                              ↓ 5%
┌────────────────────────────────────────────────────────────────────────────────┐
│  XVISION SETTLEMENT WALLET  (operator-owned EOA / multi-sig)                    │
│   accumulates 5% drips; operator sweeps periodically                            │
│   compromise loss = unswept fees only, NEVER user trading capital               │
└────────────────────────────────────────────────────────────────────────────────┘
```

**Key principles:**

- **Two rails, no crossover.** Trading capital moves only between user wallet ↔ user Orderly account. Marketplace fees move only between buyer ↔ creator + settlement wallet. The two rails do not share contracts, do not share custody, and do not share state beyond the strategy-id reference (a marketplace listing references a strategy NFT; that NFT also tags positions in the off-chain attribution ledger).
- **"xvision never holds user trading capital" is a precise claim.** The Marketplace contract briefly holds USDC during the atomic body of a `buy()` call (pull, split, forward — all in one transaction). That USDC is *marketplace fee capital* (a buyer's payment for a license), never *trading capital* (a user's collateral or PnL on Orderly). The two are physically separate funds in physically separate places.
- **The trading key is scoped to orders, not withdrawals.** Orderly's API key model permits this: a registered Ed25519 key signs REST orders against the account but cannot initiate vault withdrawals (those require the EVM signer the user retains).
- **Per-agent budgets are off-chain.** They live in `xvision-risk` and are enforced at order-submission time. No on-chain accounting contract holds collateral on behalf of strategies.
- **The smart-contract surface stays the same as the existing marketplace spec.** This spec adds no new contracts. The novelty is the *constraint*: the Marketplace contract is the only place xvision ever holds funds, and only atomically during a license sale.

---

## 3. Components

### 3.1 User-side: Mantle wallet + Orderly account

The user retains a normal Mantle EVM wallet (any signer they prefer — MetaMask, Privy embedded, hardware wallet). They onboard to Orderly via Orderly's standard registration flow: sign a registration message that creates an Orderly sub-account bound to their EVM address, and deposit USDC into the Orderly Vault contract on Mantle (`0x816f722424B49Cf1275cc86DA9840Fbd5a6167e9`).

xvision is not in this loop. The user could do all of this without xvision ever being installed.

### 3.2 Trading-key issuance

After Orderly onboarding, the user authorizes xvision to trade on their behalf by:

1. xvision generates an Ed25519 keypair locally (in the operator's environment for v1; in the user's browser for multi-tenant v2).
2. xvision presents the public key to the user and asks them to sign an Orderly `add_orderly_key` request from their EVM wallet, scoping the key to:
   - account_id: the user's Orderly account
   - permissions: `trading` only (no `withdraw`, no `transfer`)
   - ip_restriction (optional): xvision server IP
   - expiration: 90 days, rotatable
3. The signed registration goes to Orderly. Orderly returns the registered key id.
4. xvision stores the Ed25519 *private* key encrypted at rest using the AES-256-GCM scheme described in §5.

The user's EVM private key never touches xvision. The trading key xvision holds is bounded by Orderly's permission system: worst case (xvision server compromise), an attacker can place trades up to the user's risk limits but cannot drain funds, cannot transfer, cannot rotate the key.

**Phishing-resistant registration UX (mandatory).** The registration step is the highest-leverage attack surface — if the user signs a message that registers an attacker's key instead of xvision's, the security model fails on day zero. The UX MUST:

- Display the full Ed25519 public key (hex) prominently, in monospace, before the user signs.
- Display the permission scope being requested (`trading` only, no withdraw/transfer) in plain text.
- Display the expiration in human terms ("expires 2026-08-09").
- **Recommend hardware wallet** for the signing step specifically; the registration message is rare (every 90 days) so the friction is acceptable.
- Show a copy-paste-able verification command the user can run independently (e.g. `xvn key verify <pubkey>` that re-derives from the locally-stored private key).

After signing, the user can independently confirm registration via Orderly's web UI (their account → API keys list).

### 3.3 Strategy variants and the dispatcher

Strategy variants are unchanged from the Strategy Creation Engine spec. They produce `TraderDecision` values (`crates/xvision-core/src/trading.rs:114+`) carrying `{action, size_bps, direction, stops, summary}`.

A new component, `OrderDispatcher`, sits between the strategy and the Orderly executor:

```
strategy variant ──TraderDecision──▶ RiskEngine ──gated decision──▶ OrderDispatcher ──signed order──▶ Orderly REST
                                          │
                                          ├─ checks per-strategy hard cap
                                          ├─ checks per-strategy dynamic quota
                                          ├─ checks global daily-loss circuit breaker
                                          └─ records attribution row
```

The dispatcher is the only code path that uses the Ed25519 trading key. Strategies cannot reach the key.

### 3.4 Risk Analysis Engine — per-strategy budgets (hybrid model)

Today `xvision-risk` enforces global rules (position size bps, daily-loss circuit breaker, max open positions, correlation cluster cap, asset whitelist). This spec extends each rule to also accept a per-strategy override, adds the full set of scoped-permission rules required by 2026 agent-wallet best practice (chain · protocol · token · notional · slippage · frequency · time window), and introduces the hybrid hard-cap × dynamic-quota model.

**Per-strategy scoped permissions (operator-set, full envelope):**

```toml
[strategies.<agent_id>]
# --- existing rules, scoped per-strategy ---
hard_cap_usdc_notional          = 5000      # max in-flight notional ever
hard_cap_open_positions         = 2         # max simultaneous positions
hard_cap_daily_loss_usdc        = 250       # per-strategy daily kill switch

# --- new scoped permissions ---
allowed_chains                  = ["mantle"]                    # chain allowlist
allowed_protocols               = ["orderly_perp_v3"]           # protocol allowlist (just Orderly in v1)
allowed_assets                  = ["PERP_BTC_USDC"]             # token/symbol allowlist
max_slippage_bps                = 50         # reject orders whose simulated fill exceeds this
max_orders_per_minute           = 10         # frequency cap (anti-runaway)
max_orders_per_hour             = 100        # rolling-window frequency cap
active_hours_utc                = "00:00-24:00"  # restrict trading to a window (e.g. "13:30-20:00" for US session)
require_manual_approval_above   = 2500       # USDC notional threshold above which operator must approve
```

The hard-cap rules are **fixed envelopes**: the strategy cannot exceed them under any circumstances. Operator owns these knobs; they are the legible, deterministic guarantee.

**Per-strategy dynamic quota (engine-set, inside the hard cap):**

```
unlocked_notional(agent_id, t) = hard_cap_usdc_notional × quota_factor(agent_id, t)
```

`quota_factor ∈ [0.0, 1.0]`, computed each session from the attribution ledger:

```
quota_factor(s, t) =
    clamp(
        cold_start_floor                                                  # 0.25 if < 30 closed positions
        + sigmoid(rolling_sharpe(s, last_30d) / sharpe_normalizer)        # 0..1, midpoint at sharpe=0
        × (1 - max_drawdown(s, last_30d) / drawdown_floor)                # 0..1, hits 0 at full drawdown_floor
        ,
        0.0,
        1.0
    )

constants (v1, tune from observation):
    cold_start_floor   = 0.25
    sharpe_normalizer  = 1.5
    drawdown_floor     = 0.20   (20% drawdown → quota_factor → 0)
```

Cold strategies start at the floor (0.25). Hot strategies converge toward 1.0. Burned strategies throttle toward 0. The hard cap is never exceeded; the *unlocked fraction* is what changes.

Implementation lives in `crates/xvision-risk/src/rules/per_strategy.rs` (new file). The quota function is a pure function of the attribution ledger's recent history — easy to unit-test with synthetic histories.

**Race-free cap enforcement (reservation pattern):**

Two strategies at 90% of cap can both clear a check-then-submit pattern and both fill, exceeding the cap. To prevent this, the dispatcher uses a reservation:

```
1. risk_engine.evaluate(agent_id, decision) acquires a write-lock on the
   strategy's row in a `pending_reservations` table.
2. evaluate() re-reads current open notional + already-reserved notional.
3. if (open + reserved + this_decision_notional) ≤ unlocked_notional → reserve
   (insert row with TTL 30s) and return Approved.
4. dispatcher submits to Orderly. On response (success or fail), reservation
   is converted to a real position row OR released.
5. TTL expires reservations whose dispatcher crashed before responding.
```

This gives strict-correctness against concurrent decisions without serializing the entire dispatcher. Lock contention is per-strategy, so independent strategies never wait on each other.

**Aggregate margin utilization cap (cross-margin contagion mitigation):**

Orderly perps are cross-margin by default — Strategy A's blow-up can liquidate the whole account, defeating Strategy B's hard cap. Per validation gate G2 (§1.1), the implementation plan first verifies whether Orderly supports per-position **isolated-margin**:

- **If isolated-margin is supported:** add a per-strategy config `margin_mode = "isolated" | "cross"` (default isolated for new strategies). Each isolated strategy gets its own margin bucket; contagion is eliminated at the cost of capital efficiency.
- **If isolated-margin is NOT supported (cross-margin only):** add a global rule:

  ```toml
  [risk.global]
  max_aggregate_margin_utilization = 0.60   # halt new orders if account margin > 60%
  ```

  The Risk Engine refuses any new order that would push aggregate margin utilization above the ceiling, regardless of per-strategy headroom. Per-strategy hard caps still apply but degrade to "intentional-overallocation defense only" — they cannot prevent contagion via shared margin. Operators are warned of this in the dashboard.

Either way, the cross-margin risk is documented explicitly and the operator chooses the mode per-strategy or accepts the global ceiling.

**UI: strategy-budget spreadsheet (promoted to v1).**

The hybrid model (multiple knobs per strategy across many strategies) is unusable without a UI that surfaces all strategies in a single editable table. The dashboard MUST include a **"Strategy Budgets" spreadsheet view** as a first-class screen, not a sub-tab:

```
Strategy            Hard Cap  Slippage  Orders/min  Active Hours  Mode      Quota   Status   Actions
─────────────────────────────────────────────────────────────────────────────────────────────────────
btc-momentum-v3     $5,000    50 bps    10          24/7          isolated  0.78    active   [edit] [kill]
btc-mean-revert-v1  $3,000    30 bps    20          13:30-20:00   isolated  0.42    active   [edit] [kill]
btc-funding-fader   $8,000    20 bps    5           00:00-08:00   isolated  0.95    active   [edit] [kill]
btc-breakout-v2     $2,000    100 bps   30          24/7          cross     0.00    halted   [edit] [unhalt]
─────────────────────────────────────────────────────────────────────────────────────────────────────
                    $18,000                                                              [+ add strategy]
```

Every cell except `Quota` and `Status` is inline-editable. Edits write to `policy_changes` (§3.9) and apply on save with a confirm modal showing old → new for each touched field. The `Quota` column is engine-computed (read-only). The `Status` column shows `active` / `halted_auto` / `halted_manual` with click-through to the audit log of what triggered the halt.

Sortable columns. Filter by status. Aggregate row at the bottom showing total committed notional vs. user's Orderly account collateral (if total > X% of collateral, surface a warning). Bulk-edit via shift-click + apply.

This view is the daily working surface for the operator. It belongs in front of every other dashboard concern — without it, hybrid-budget management is theoretically nice but operationally unworkable.

### 3.5 Attribution ledger

`agent_id` throughout this spec is the strategy-variant id — one per strategy NFT minted by `IdentityRegistry` (per FOLLOWUPS SLF3). Pre-mint, the same id is used as a local ULID and resolves to the NFT id at mint time.

Every order the dispatcher submits is tagged with the originating strategy id and persisted to a SQLite table:

```sql
CREATE TABLE positions (
    position_id        TEXT PRIMARY KEY,        -- xvision-internal ULID
    client_order_id    TEXT NOT NULL UNIQUE,    -- mirrored to Orderly for idempotency
    user_id            TEXT NOT NULL,
    agent_id        TEXT NOT NULL,           -- strategy NFT id (or local id pre-mint)
    asset              TEXT NOT NULL,           -- e.g. PERP_BTC_USDC
    side               TEXT NOT NULL,           -- LONG | SHORT
    size_usdc          REAL NOT NULL,
    entry_price        REAL,
    exit_price         REAL,
    realized_pnl_usdc  REAL,
    opened_at          INTEGER NOT NULL,
    closed_at          INTEGER,
    orderly_position_id TEXT                    -- Orderly's id, populated on fill
);

CREATE INDEX idx_positions_strategy ON positions(agent_id, closed_at);
CREATE INDEX idx_positions_open ON positions(agent_id) WHERE closed_at IS NULL;
```

**Funding-payment attribution.** Orderly perps charge funding every funding period (typically 8h). A position held across a funding period accrues a funding charge or rebate. The reconciliation job (§4.3) attributes each funding event to the position holding it during that period:

```sql
CREATE TABLE funding_attributions (
    funding_id         TEXT PRIMARY KEY,        -- Orderly funding event id
    position_id        TEXT NOT NULL REFERENCES positions(position_id),
    agent_id        TEXT NOT NULL,           -- denormalized for fast roll-up
    asset              TEXT NOT NULL,
    funding_rate_bps   REAL NOT NULL,           -- can be negative
    notional_usdc      REAL NOT NULL,
    payment_usdc       REAL NOT NULL,           -- signed; positive = paid by us, negative = received
    funded_at          INTEGER NOT NULL
);
```

Funding payments are folded into `realized_pnl_usdc` at position close (or rolled up into per-period PnL for still-open positions). Without this, long-held positions look more profitable than they are and the marketplace leaderboard misranks holders vs scalpers.

This ledger is the source of truth for *attribution* (who traded what), and is reconciled against Orderly's account state on a schedule (§4.3).

The ledger is **reporting-only** — no settlement contract reads from it. Its purpose is:

1. Risk Engine reads it to compute quota_factor (§3.4).
2. Marketplace creator-payout reporting reads it to show "your strategy generated $X attributable PnL this period" (informational; v1 license sales are not performance-fee-based).
3. ERC-8004 reputation writes derive their feedback values from it (per the existing reputation-registry plan in SLF4).

### 3.6 Marketplace fee router (no changes)

Specified entirely in [`2026-05-08-smart-contract-surface-design.md`](./2026-05-08-smart-contract-surface-design.md). This spec only adds the constraint:

**This is the only smart contract xvision ever owns or upgrades.** The contract holds funds atomically during a `buy(listingId)` call (USDC pulled from buyer, split, forwarded) and never across transactions. There is no `withdraw` from the marketplace contract — the 95% creator share goes directly to the seller wallet recorded on the listing, and the 5% protocol fee goes directly to the settlement wallet (§3.7).

### 3.7 Settlement wallet

A single Mantle EOA (or 2-of-3 multi-sig for hardening) owned by the operator. Receives the 5% commission drips emitted by the Marketplace contract on every license sale.

The wallet:

- Holds USDC.e accumulated from commissions.
- Is swept manually by the operator on whatever cadence they prefer.
- Has no programmatic outflows xvision controls.
- Is recorded on the Marketplace contract as `protocolFeeRecipient`. Changing it is a privileged op behind the existing 7-day timelock + 2-of-3 multi-sig pattern from the smart-contract-surface spec.

If the wallet is compromised, the loss is bounded to unswept fees. User trading capital is unreachable from this wallet — they are not connected.

### 3.8 Audit log (full decision-to-outcome trace)

The `positions` table records *outcomes*. The 2026 agent-wallet best-practice playbook also requires logging the *decision pipeline*: model output → risk evaluation → simulation → signed payload → response → fill → close. This enables forensics after any anomaly (rogue strategy, unexpected loss, regulatory inquiry).

A separate append-only `decisions` table records every step:

```sql
CREATE TABLE decisions (
    decision_id            TEXT PRIMARY KEY,            -- ULID
    occurred_at            INTEGER NOT NULL,
    user_id                TEXT NOT NULL,
    agent_id            TEXT NOT NULL,
    stage                  TEXT NOT NULL,               -- one of: emit | risk_eval | simulate | sign | submit | response | fill | close | cancel | reject
    related_position_id    TEXT,                        -- nullable; populated once known
    related_decision_id    TEXT,                        -- chain to prior stage in same trade
    payload_json           TEXT NOT NULL,               -- stage-specific structured payload
    payload_sha256         TEXT NOT NULL,               -- content hash, never re-written
    notes                  TEXT
);

CREATE INDEX idx_decisions_strategy_time ON decisions(agent_id, occurred_at);
CREATE INDEX idx_decisions_position ON decisions(related_position_id);
```

**What each stage records:**

| Stage | Payload contents |
|---|---|
| `emit` | The full `TraderDecision` from the strategy variant + the briefing hash that produced it |
| `risk_eval` | Inputs (decision + ledger snapshot + active rules) → result (Approved/Modified/Vetoed) + reasons |
| `simulate` | Pre-flight Orderly order-info call: estimated fill price, slippage bps, fees |
| `sign` | The Ed25519-signed REST payload (with body and signature; key id, NOT the key) |
| `submit` | Orderly's HTTP response (status, body, request id) |
| `response` | Confirmed order id from Orderly |
| `fill` | Webhook/poll confirmation: actual fill price, qty, fees |
| `close` | Exit details: exit price, realized PnL, funding rolled in |
| `cancel` / `reject` | Reason + payload returned by Orderly |

Each row is content-hashed at write; the table is append-only at the application layer (no UPDATE / DELETE statements in dispatcher code paths). This gives a forensic trail that survives application bugs and supports later third-party audits.

**Pre-trade simulation (new step in dispatch flow).** Before submitting an order, the dispatcher MUST call Orderly's order-info endpoint (or equivalent simulation) and:

1. Compute expected fill price + slippage from the current orderbook.
2. Reject if `simulated_slippage_bps > strategy.max_slippage_bps`.
3. Log the `simulate` stage to the audit log.
4. Proceed to sign + submit.

The simulation is best-effort (not all market conditions can be predicted), but catches obvious issues like trading into thin books.

### 3.9 Kill switches, approval gates, and emergency-close

Three layers of stop, ordered by blast radius:

**Layer 1 — per-strategy auto-trigger:**

```toml
[strategies.<agent_id>.kill_triggers]
daily_loss_kill_usdc            = 250        # already in §3.4 hard caps
consecutive_losses_kill         = 5          # halt after N losing trades in a row
sharpe_floor_kill               = -2.0       # halt if rolling 30-trade Sharpe falls below
```

Auto-triggers set the strategy's status to `halted_auto` in a `strategy_status` table. The dispatcher rejects any decision from a halted strategy. Operator must manually un-halt (which requires explicit confirmation, journaled to the audit log).

**Layer 2 — operator manual kill:**

```
xvn kill --strategy <id>          # halt one strategy
xvn kill --user <id>              # halt all strategies for one user (revokes their trading key)
xvn kill --all                    # global halt: all dispatchers refuse new orders
```

Kill commands take effect within one dispatcher loop tick (≤ 1s). They do NOT cancel open positions automatically (see Layer 3).

**Layer 3 — emergency close (manual close-all):**

```
xvn emergency-close --user <id>   # cancels all open orders + market-closes all open positions
xvn emergency-close --strategy <id>
```

Emergency-close is the analog of "drain funding back to treasury" from the research playbook. xvision doesn't hold a treasury, so the analog is: rapidly return the user's Orderly account to a flat (no-position) state so they can withdraw freely.

Emergency-close is a privileged operator action. It uses the user's existing trading key (which has order-placement authority). It does NOT trigger withdrawal — the user still does that themselves from their EVM wallet, after positions are flat.

**Approval gate for large trades:**

If a decision's notional exceeds `require_manual_approval_above` (per-strategy, §3.4), the dispatcher does NOT submit. Instead:

1. Writes a `pending_approval` row with the decision payload.
2. Notifies operator (CLI prompt + push notification).
3. Operator approves or rejects within a TTL (default 60s). On TTL expiry, the decision is rejected.
4. Approval/rejection is journaled to the audit log.

This handles the research playbook's "human approval for first-time contracts, large trades, bridging, withdrawals, or changes to policy" — though in v1 the only relevant trigger is large trades (no bridging, no contract diversity, withdrawals are user-controlled).

**Risk-policy edits are journaled.** Any change to a strategy's risk config (hard caps, allowed assets, etc.) writes a row to a `policy_changes` table with old → new values, who made the change, and a comment. Edits do not auto-apply to in-flight positions.

**CLI surface (must be discoverable via `xvn --help`).**

All wallet/budget/kill operations are surfaced as first-class subcommands under `xvn`, listed in `xvn --help`'s top-level summary so an operator never has to dig through documentation to find emergency tooling:

```
xvn key             Issue, list, rotate, and revoke Orderly trading keys
  xvn key issue --user <id>             # generate + register a new trading key
  xvn key list                          # show all keys + scopes + expiry
  xvn key rotate --user <id>            # rotate a key (re-registration UX)
  xvn key revoke --user <id>            # revoke locally; user must also revoke on Orderly UI

xvn budget          View and edit per-strategy budgets and risk caps
  xvn budget show [--strategy <id>]     # spreadsheet-style table in terminal
  xvn budget set --strategy <id> --hard-cap <usd> [--slippage <bps>] [...]
  xvn budget bulk-import <toml>         # apply a config file across many strategies

xvn kill            Halt strategies or users (rejects new orders; does NOT close positions)
  xvn kill --strategy <id>
  xvn kill --user <id>                  # revokes that user's trading authority too
  xvn kill --all                        # global halt; new orders refused everywhere

xvn unhalt          Resume a halted strategy (requires confirm; journaled)
  xvn unhalt --strategy <id>

xvn emergency-close Cancel open orders + market-flat all positions
  xvn emergency-close --strategy <id>
  xvn emergency-close --user <id>
  xvn emergency-close --all             # operator nuclear option

xvn approve         Manually approve / reject pending large-trade approvals
  xvn approve list                      # show pending with TTL countdowns
  xvn approve <approval-id>             # approve
  xvn approve <approval-id> --reject

xvn audit           Query the audit log
  xvn audit position <position-id>      # full pipeline trace for one position
  xvn audit strategy <strategy-id> --since <time>
  xvn audit pending                     # decisions awaiting approval

xvn reconcile       Force a reconciliation against Orderly state
  xvn reconcile --user <id> [--dry-run]
```

Documentation lives in `docs/cli-reference.md` (new). Every subcommand surfaces its full flags via `xvn <command> --help`. Emergency commands (`kill`, `emergency-close`) display a confirmation prompt unless `--yes` is passed; `--yes` requires `XVN_OPERATOR_CONFIRMED=1` in the environment to prevent accidental scripting.

### 4.1 Trading rail — placing a trade

```
1.  Strategy variant runs and emits TraderDecision { action: Open, asset: BTC-PERP, size_bps: 100 }
    → audit log: stage=emit
2.  Check strategy_status: not in halted_auto / halted_manual → proceed; else reject and log
3.  RiskEngine.evaluate(agent_id, decision, ledger_snapshot)
      → checks scoped permissions    (chain/protocol/asset allowlist, active hours, frequency caps)
      → checks per-strategy hard cap (would this exceed hard_cap_usdc_notional?)
      → checks per-strategy dynamic quota (is current notional > unlocked_notional?)
      → checks aggregate margin utilization (cross-margin contagion guard)
      → checks approval-required threshold
      → returns Approved | Modified | Vetoed | RequiresApproval
    → audit log: stage=risk_eval
4.  If RequiresApproval: write pending_approval row, notify operator, await TTL
5.  If Approved/Modified:
      a. RiskEngine acquires reservation on per-strategy notional (pending_reservations row, TTL 30s)
      b. dispatcher calls Orderly order-info to simulate fill + slippage
         → reject if simulated_slippage_bps > strategy.max_slippage_bps; release reservation
         → audit log: stage=simulate
      c. dispatcher generates client_order_id = ULID(); writes pending row to positions table
      d. dispatcher signs Orderly REST POST /v3/orders with the user's Ed25519 key
         → audit log: stage=sign (records signed payload + key id, NEVER the key)
      e. dispatcher submits; on response, converts reservation → position row OR releases
         → audit log: stage=submit + stage=response
6.  Webhook / poll updates fill → entry_price recorded → audit log: stage=fill
7.  On close (TP/SL/manual): exit_price + realized_pnl_usdc + funding rolled in; closed_at set
    → audit log: stage=close
```

Idempotency: `client_order_id` is propagated to Orderly (already done in `crates/xvision-execution/src/orderly.rs:34`). If xvision crashes between (c) and (d), retry uses the same id and Orderly deduplicates. Reservations have TTL so a crashed dispatcher doesn't permanently freeze a strategy's quota.

### 4.2 Marketplace rail — license sale

```
1.  Buyer calls Marketplace.buy(listingId) on Mantle, with USDC approval pre-set
2.  Marketplace contract pulls USDC from buyer
3.  Marketplace contract atomically splits:
      - 95% → seller wallet (from listing)
      - 5%  → settlement wallet (xvision)
4.  Marketplace mints LicenseToken (ERC-1155, soulbound) to buyer
5.  Sold(buyer, seller, listingId, agentNftId, price) event emitted
6.  Off-chain indexer picks up Sold event → updates xvision UI / leaderboard
```

The marketplace and trading rails do not communicate at runtime. The only shared identifier is `agentNftId` (which is `agent_id` in the trading rail) — used purely as a foreign key for joining reports.

### 4.3 Reconciliation

A scheduled job (cadence: every 15 minutes during active sessions) does:

1. Fetch live Orderly account state via `GET /v3/positions` and `GET /v3/account`.
2. For each open Orderly position, find the matching `positions` row (by `orderly_position_id`).
3. If a position exists in Orderly but not in xvision ledger → flag as "orphan" (manual investigation; could indicate user manually traded outside xvision, which is fine, just not attributable).
4. If a position exists in xvision ledger but not in Orderly → mark as closed (Orderly closed it server-side, e.g. liquidation) and surface to operator.
5. Compute account-level NAV vs sum of attributed PnL → diff is "untracked PnL" (manual user trades, funding payments, etc.).

Reconciliation is observability, not enforcement. The Orderly account state is canonical for funds; xvision ledger is canonical for attribution.

---

## 5. Security model

**Cryptographic surface area xvision controls:**

1. The Ed25519 trading key per user — encrypted at rest with AES-256-GCM, key derived from `CREDENTIAL_SECRET` env var. Decrypted only in process memory, only inside the OrderDispatcher. Never logged. Never sent over the wire (used to sign locally, signature is sent).
2. The Mantle EOA / multi-sig that owns the Marketplace contract upgrade rights — held in 1Password / hardware wallet by the operator, used only for upgrades behind the existing 7-day timelock.
3. The settlement wallet private key — held by the operator, used only to sweep accumulated fees.

**Cryptographic surface area xvision does NOT control:**

- The user's Mantle EVM signer (their wallet).
- The user's Orderly EVM signer (same as above, used for vault deposits and key registration).
- Any direct ability to withdraw from the Orderly Vault contract.
- Any allowance setting on the user's USDC token (the user approves the Orderly Vault directly; xvision never touches `approve()` on the user's behalf).

**Threat scenarios:**

| Compromise | Worst-case impact | Mitigation |
|---|---|---|
| xvision server (Ed25519 key extracted from disk OR memory dump during signing window) | Attacker can place trades up to user's risk-engine limits. Cannot withdraw. | Per-strategy hard caps; daily-loss circuit breaker; frequency caps; anomaly alerts; user can revoke key on Orderly side instantly (`delete_orderly_key` from their EVM wallet); operator can `xvn kill --user <id>` and `xvn emergency-close --user <id>`. v2 mitigation: MPC self-hosted signer (§10) eliminates the in-memory window entirely. |
| Settlement wallet | Loss of unswept commission fees only. | Periodic sweeping; multi-sig optional. |
| Marketplace contract (worst case: malicious upgrade redirects fees) | Loss of in-flight USDC during a `buy()` call + redirected future fees until reverted. **Cannot reach user trading capital** — the trading rail does not read from or trust the marketplace contract. | UUPS proxy + 7-day timelock + 2-of-3 multi-sig per existing spec; the timelock window is the operator's response budget. |
| User EVM wallet | User's full Orderly account drained — but this is the user's responsibility, not xvision's. | N/A (user-owned). |
| Phishing during key registration (user signs malicious `add_orderly_key` thinking it's xvision's) | Attacker controls trading key from day zero. | §3.2 mandates pubkey display, scope display, hardware-wallet recommendation, independent verification path via Orderly's web UI. |

**The deliberate property:** no compromise of *any* xvision-controlled key or contract can result in loss of user trading capital. This is the design constraint the spec exists to satisfy.

**Acknowledged residual risk: in-memory key during signing.** Even with AES-256-GCM at rest, the Ed25519 trading key sits in process memory while the dispatcher signs each order. A memory-extraction attack (e.g. via a kernel exploit on the host) could exfiltrate the key. This is the same single-point-of-compromise pattern swarmclaw has, and is the strongest argument for migrating to MPC signing in v2 (recorded in §10). For v1, the mitigation is that the *blast radius* is bounded by the trading-only scope (no withdrawal possible regardless of key compromise) and by the kill-switch + emergency-close path (§3.9).

---

## 6. Failure modes

- **Orderly key registration fails or expires.** Dispatcher fails closed: emits an explicit "key invalid" alert, halts new orders for that user, leaves existing positions untouched. Operator runs `xvn key rotate <user>` to issue a new key.
- **Risk Engine quota function buggy and starves all strategies.** Hard caps still permit trading at full ceiling — the dynamic quota is multiplicative and bounded by the hard cap. Worst case: all strategies effectively run with global rules + hard caps but no dynamic damping. Surfaces as "all strategies stuck at quota_factor=0" alert.
- **Rogue strategy spamming orders.** Frequency caps (`max_orders_per_minute`, `max_orders_per_hour`) reject excess orders. Auto-trigger (`consecutive_losses_kill`, `sharpe_floor_kill`) eventually halts the strategy. Operator can manually `xvn kill --strategy <id>` for instant stop.
- **Cross-margin contagion (Strategy A's blow-up liquidates positions in Strategy B's "safe" range).** Mitigation depends on validation gate G2 outcome: isolated-margin per strategy if Orderly supports it; otherwise the global `max_aggregate_margin_utilization` rule throttles new orders before margin gets dangerous. Operator alerted to mode in dashboard.
- **Pre-trade simulation fails or returns stale data.** Dispatcher rejects the order (fail-closed). If the simulation endpoint is unavailable for an extended period, all new orders are rejected — surfaces as a critical alert; operator can temporarily disable the simulation gate via `xvn config set simulate_required false` (audit-logged).
- **Reservation TTL leaks (crashed dispatcher leaves a strategy's quota partially reserved).** TTL of 30s self-resolves; a stuck reservation eventually expires and is reaped. If reservations consistently leak (suggesting a bug), surfaces as "reservation expiry rate exceeds threshold" alert.
- **Race on simultaneous decisions.** Reservation pattern (§3.4) makes cap checks strictly serializable per-strategy. Independent strategies do not block each other.
- **Attribution ledger out of sync with Orderly state.** Reconciliation job (§4.3) detects and surfaces. PnL reporting becomes stale until resolved; trading is unaffected (orders still execute, just attribution is delayed).
- **Marketplace contract compromised mid-sale.** Atomic transaction either completes or reverts; no partial-state risk. Pause via existing multi-sig if needed.
- **Settlement wallet compromised.** Operator loses unswept fees. Issue a contract upgrade to point `protocolFeeRecipient` at a new address (7-day timelock applies).

---

## 7. Component map

| Concern | Crate / location | Status |
|---|---|---|
| Strategy variants | `crates/xvision-engine/`, `crates/xvision-trader/` | exists |
| Order signing (Orderly) | `crates/xvision-execution/src/orderly.rs` | exists; needs per-user key parameter (currently single env-var key) |
| Order dispatcher | `crates/xvision-execution/src/dispatcher.rs` | **new** — sits between engine and orderly executor |
| Pre-trade simulation | `crates/xvision-execution/src/simulate.rs` | **new** — Orderly order-info wrapper |
| Risk Engine global rules | `crates/xvision-risk/src/rules/` | exists |
| Risk Engine per-strategy rules | `crates/xvision-risk/src/rules/per_strategy.rs` | **new** |
| Reservation table + locking | `crates/xvision-risk/src/reservations.rs` | **new** — race-free quota enforcement |
| Aggregate margin guard | `crates/xvision-risk/src/rules/aggregate_margin.rs` | **new** — cross-margin contagion mitigation |
| Attribution ledger schema | `crates/xvision-data/` (or new `crates/xvision-ledger/`) | **new** — SQLite migrations (positions, funding_attributions, decisions, policy_changes, strategy_status, pending_approvals) |
| Reconciliation job | `crates/xvision-execution/src/reconcile.rs` | **new** |
| Audit log writer | `crates/xvision-data/src/audit.rs` | **new** — append-only decisions table |
| Kill switch + emergency-close CLI | `crates/xvision-cli/src/commands/kill.rs`, `emergency_close.rs` | **new** |
| Approval gate workflow | `crates/xvision-execution/src/approval.rs` | **new** |
| Trading-key storage | `crates/xvision-identity/src/trading_key.rs` | **new** — AES-256-GCM at rest |
| Trading-key registration UX | `crates/xvision-cli/src/commands/key.rs` | **new** — phishing-resistant pubkey display |
| Marketplace contract | `contracts/Marketplace.sol` (per smart-contract-surface spec) | spec'd, not implemented |
| Settlement wallet management | operator-side (1Password / hardware) | manual |

---

## 8. Migration from current state

Current state: one shared `ORDERLY_KEY/SECRET/ACCOUNT_ID` from env, used globally; no per-strategy attribution; risk engine is global.

Migration path (incremental, each step ships independently):

1. **Step 0 — Validation gates.** Probe Orderly testnet to verify G1 (trading-only key scope, §1.1) and G2 (isolated-margin support, §1.1). Record results as ADRs in `decisions/`. Block all subsequent steps until G1 passes — if it fails, redesign first.
2. **Step 1 — Attribution + audit log.** Add `agent_id` tag to existing trades. Build `positions`, `decisions` (audit log), `funding_attributions`, `policy_changes`, `strategy_status`, `pending_approvals` tables. Populate from existing single-key flow. Validates the ledger end-to-end without changing order routing or risk.
3. **Step 2 — Per-strategy hard caps + scoped permissions + reservations.** Extend Risk Engine with the full per-strategy rule set (hard caps + chain/protocol/asset allowlist + slippage + frequency + active hours). Implement reservation pattern for race-free cap enforcement.
4. **Step 3 — Dispatcher refactor + pre-trade simulation.** Introduce OrderDispatcher abstraction. Add pre-trade simulation step (Orderly order-info or equivalent). Route all current orders through the dispatcher. Trading key still single (from env).
5. **Step 4 — Kill switches + approval gates + emergency-close.** Add `xvn kill` and `xvn emergency-close` CLI commands. Implement per-strategy auto-triggers (consecutive-losses, sharpe-floor). Approval-gate workflow with TTL.
6. **Step 5 — Aggregate margin guard (conditional).** If G2 fell back to cross-margin only, add `max_aggregate_margin_utilization` global rule.
7. **Step 6 — Dynamic quota.** Add `quota_factor` computation and enforce as multiplier on hard caps.
8. **Step 7 — Multi-key support.** Allow per-user encrypted Ed25519 keys. Provide CLI to add/rotate keys (with phishing-resistant pubkey display per §3.2). Deprecate the single env-var key path.

Steps 0–6 ship in single-user mode (operator is the only "user"). Step 7 unlocks multi-tenant. Each step adds tests; no step ships without its corresponding tests passing in CI.

---

## 9. Testing

- **Unit:** Risk Engine per-strategy rules with synthetic ledger histories (cold-start, hot-streak, drawdown, kill-switch hit). Pure-function `quota_factor` is the easiest thing in the system to test; aim for >95% coverage.
- **Integration:** dispatcher + risk engine + ledger + Orderly (using the existing `probes/m0-orderly/` pattern against testnet). End-to-end: strategy emits decision → simulation runs → reservation acquired → signed and submitted → fill recorded → close recorded → ledger consistent → reconciliation passes → audit log complete.
- **Validation gates (G1, G2):** before any code is written, run a probe that calls Orderly's `add_orderly_key` with trading-only scope and verifies the key cannot withdraw; and verify whether per-position isolated-margin is supported. Record results as ADRs.
- **Adversarial:** simulate compromised trading key by spamming order requests at full hard cap → verify no withdrawal possible, daily-loss circuit breaker fires, frequency caps reject excess orders, user can revoke key via Orderly UI, operator can `xvn kill --user <id>` and `xvn emergency-close --user <id>` within seconds.
- **Concurrency:** N concurrent strategies all at 90% of cap submitting simultaneously → verify reservation pattern keeps `sum(notional in flight) ≤ hard_cap_usdc_notional` for each strategy.
- **Property:** `for any sequence of decisions and any quota_factor, sum(notional in flight) ≤ hard_cap_usdc_notional` — encoded as a property test.
- **Audit-log completeness:** for every order, verify that all expected stages (`emit`, `risk_eval`, `simulate`, `sign`, `submit`, `response`, `fill`, `close`) are present with valid payload hashes.
- **Cross-margin contagion (if G2 falls back to cross-margin only):** simulate one strategy taking max-leverage position → verify aggregate margin cap halts other strategies before liquidation cascade.

---

## 10. Open questions / deferred

- **Multi-tenant onboarding UX.** v1 assumes operator-as-user. Browser-side Ed25519 keypair generation + Orderly registration flow is needed for multi-tenant. Defer to Plan 2d (dashboard) or later.
- **Performance fee on withdrawal.** Recorded as a future fee model: when a user withdraws from their Orderly account through xvision's withdrawal helper, an off-chain calculator computes realized PnL since deposit, and the user is offered the choice to pay an X% performance fee to creators of the strategies that contributed to that PnL. Strictly opt-in (user can always withdraw directly via Orderly's UI). Adds a helper contract and a UI flow; not required for hackathon. **Operator's note:** this is a "fun" idea — keep on the table for v2.
- **Per-trade x402 micropayments.** Recorded as a future fee model alongside license purchases. Adds pre-authorization UX and per-fire fee accounting. Out of v1 scope.
- **Trading-key auto-rotation.** Currently 90-day expiry with manual rotation. Auto-rotation requires the user to be online to sign the new registration; not a hackathon priority.
- **Stake bonds / costly signaling for strategies.** Not surfaced in the brainstorm but adjacent: strategies could be required to lock a USDC bond (slashed on poor performance) to be eligible for high-quota allocation. Could be added as an optional layer in the quota function. v2.
- **Reconciliation alerting.** §4.3 defines the reconciliation job; alerting thresholds (when does a "drift" become "page the operator"?) are TBD with operational experience.
- **MPC migration for trading-key signing.** v1 stores the Ed25519 trading key encrypted at rest with AES-256-GCM, decrypted in process memory at signing time. The 2026 agent-wallet research consensus (Fystack, Kite Passport, etc.) is that the in-memory window is the strongest remaining attack surface for this class of system. v2 should evaluate self-hosted MPC signers (e.g. Fystack-style) that keep no full key in any single process. Migration should be transparent to dispatchers — the OrderDispatcher abstraction already isolates signing behind a trait.
- **Smart-account migration for trading scope.** Even with MPC signing, the authority model is "off-chain key with API-level scope." A v2+ migration could use a Safe / ERC-4337 smart account on Mantle with on-chain session-key permissions — same trading-only scope but enforced at the contract level rather than at Orderly's API. Requires Orderly to accept smart-account signatures (currently does not without a relayer); not a near-term path.
- **Browser-issued ephemeral session keys.** When the multi-tenant dashboard ships, consider issuing short-lived (hours-scoped) trading keys per active dashboard session, in addition to (or instead of) the 90-day per-user key. Limits exposure window further. Adds rotation complexity.
- **Comparison with OKX Agentic Wallet + Kite Passport.** OKX recently shipped an "agentic wallet" model with Kite Passport-style scoped permissions. Worth a deeper evaluation: do they offer primitives we should adopt (e.g. a permission grammar more expressive than Orderly's binary trading/withdraw scope)? Are there integration points that would let xvision ride on their custody primitives instead of building our own trading-key issuance flow? Time-box: 1 day of research before v2 planning.
- **Comparison with Fystack self-hosted MPC.** Fystack's blog post on "Who controls the key when your AI agent signs" makes the case for MPC over single-server custody. Worth piloting their open-source signer (or equivalent: Sodot, Lit Protocol) as the v2 trading-key backend. Time-box: 2 days of integration spike.

---

## 11. References

**Internal:**

- [`docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`](./2026-05-08-smart-contract-surface-design.md) — Marketplace contract surface (95/5 split, license tokens, fee routing). This spec assumes that one verbatim.
- [`docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`](./2026-05-08-strategy-creation-engine-design.md) — Strategy variants and TraderDecision.
- [`crates/xvision-execution/src/orderly.rs`](../../../crates/xvision-execution/src/orderly.rs) — current Orderly executor (raw reqwest + Ed25519 signing).
- [`crates/xvision-risk/`](../../../crates/xvision-risk/) — Risk Analysis Engine being extended.
- [`crates/xvision-identity/src/manifest.rs`](../../../crates/xvision-identity/src/manifest.rs) — agent / strategy NFT manifest.
- ERC-8004 v0.1-draft — strategy-as-agent identity model.
- Orderly Network API key permissions docs — for trading vs withdrawal scope split.

**External (research-derived comparisons; v2 evaluation candidates):**

- swarmclaw (https://github.com/swarmclawai/swarmclaw) — reference for AES-256-GCM key-at-rest pattern; we borrow the storage shape but reject the per-agent-EOA model.
- OKX Agentic Wallet + Kite Passport — production agentic-wallet system with scoped permissions; reference for permission-grammar expressiveness and possible v2 integration target.
- Fystack — "Who Controls the Key When Your AI Agent Signs?" — argues for self-hosted MPC over single-server custody; v2 candidate for trading-key signing backend.
- Sodot, Lit Protocol — alternative MPC signer infrastructure; evaluate alongside Fystack.
- Perplexity Sonar Pro report on AI agent wallet custody best practices (2026-04 to 2026-05) — origin of the scoped-permissions playbook this spec adopts (chain · protocol · token · notional · slippage · frequency · time window).
