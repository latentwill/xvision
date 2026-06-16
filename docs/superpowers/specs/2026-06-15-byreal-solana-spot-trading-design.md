# Byreal Solana Spot Trading — Design Spec (first slice)

**Status:** Implemented (Phase C + Phase A), 2026-06-15. Built per
`docs/superpowers/plans/2026-06-15-byreal-solana-spot-trading.md`; CLI surface
grounded in `docs/superpowers/specs/2026-06-15-byreal-spot-cli-grounding.md`
(byreal-cli v0.3.6). Follow-up: surface a "Byreal — Solana ecosystem" venue in
Settings → Brokers on top of PR #1074 (BrokersReport rework) once it merges.
**Scope of this spec:** the FIRST slice only — agent-driven spot trading of a **curated SPL + xStocks set**, **live/forward-test only**, **gated**, built **C → A**. Long-tail discovery, LP/yield, and backtest integration are explicitly out of scope (separate future tracks).

---

## 1. Goal

Let an xvision agent trade **Solana spot** — the long tail Hyperliquid perps can't touch — starting with a bounded, safe, operator-curated universe of SPL tokens plus xStocks (tokenized equities). Reuse the gated single-path execution architecture shipped in the byreal-mainnet-parity PR (SafetyGate + `venue_label` + the pause/kill-switch), so spot is **not a second path**.

**Non-goals (this slice):** memecoin/new-launch discovery; CLMM LP / yield / copy-farming; backtest/optimizer integration; building any RFQ logic ourselves.

---

## 2. Key facts (web-validated 2026-06-15)

These removed most of the anticipated complexity:

- **Byreal** is Bybit's agent-native Solana DEX. Its swap engine **auto-routes** across CLMM pools + aggregated AMMs (Raydium/Orca/Meteora) + **off-chain RFQ** market makers, picking the best path per trade. → **RFQ comes for free** inside a byreal swap; we build nothing for it.
- **`@byreal-io/byreal-cli`** (github.com/byreal-git/byreal-cli) is purpose-built for agents:
  - `byreal-cli swap execute --input-mint <mint> --output-mint <mint> --amount <n> [--slippage <bps>] --dry-run` → preview (routing, price impact, slippage) with **no funds moved**; `--confirm` to execute.
  - token price/list queries; `catalog list` / `catalog show <id>` for capability/flag discovery; `-o json` for parsing.
  - **Custody:** the CLI manages the wallet in `~/.config/byreal/keys/` via `byreal-cli wallet set` / `setup` — the key is **never an env var and never handled by the agent** (an improvement over the perps CLI's `BYREAL_PRIVATE_KEY`).
  - Conventions the CLI enforces: preview-first (`--dry-run` then `--confirm`); warn on slippage > 200 bps; large amounts (> $1000) need explicit confirm.
- **xStocks** (Backed Finance: AAPLx/NVDAx/TSLAx, 60+ equities, 24/7) are **plain SPL tokens** you swap into with USDC/SOL. → **no special path**; they are entries in the curated set.
- **`--dry-run` is our no-funds forward-test mode** — validates routing, slippage, custody, and gating without moving real funds.

Sources: docs.byreal.io; github.com/byreal-git/byreal-cli; solflare.com/stocks.

---

## 3. Architecture

One integration point (`byreal-cli`) gives **execution + prices + dry-run preview**, so a single surface covers everything, and it plugs into the existing gated path.

### 3.1 `ByrealSpotSurface` — a `BrokerSurface` over `byreal-cli`

New module `crates/xvision-execution/src/byreal_spot.rs`, mirroring `byreal.rs`/`virtuals.rs` structure: an inner mockable trait (`ByrealSpotApi`, subprocess in prod / in-memory in tests) wrapped by `ByrealSpotSurface` which implements the engine's `BrokerSurface`.

| `BrokerSurface` method | Spot mapping (via `byreal-cli`) |
|---|---|
| `submit_order(req)` | **buy** = swap USDC→token, **sell** = swap token→USDC: `swap execute --input-mint --output-mint --amount --slippage --confirm -o json`. In **preview/forward-test mode** use `--dry-run` instead of `--confirm` (returns a simulated fill). |
| `position(asset)` | wallet token balance for the asset's mint |
| `balance()` | wallet USDC balance |
| `venue()` | `"byreal_spot"` |
| `signing_scheme()` | `"cli"` (CLI keystore) |
| `is_perp_venue()` | `false` (spot: Long/Flat only, no shorting) |

The surface holds a **mode** (`Preview` ⇒ `--dry-run`, `Live` ⇒ `--confirm`) and an optional default slippage (bps). It never reads or logs key material — the CLI owns the keystore.

> Confirm at implementation: exact `swap`/`price`/`balance` flags via `byreal-cli catalog show <id>` and a grounding probe (same method `byreal.rs` used for the perps CLI — see `docs/superpowers/specs/2026-06-13-byreal-perps-cli-grounding.md`). Pin behavior against `byreal-cli --version`.

### 3.2 Phase C — thin gated CLI (`xvn spot`), built first

`xvn spot --venue byreal --buy|--sell <mint-or-symbol> --amount <usd> [--slippage <bps>] [--i-understand-real-money]`:
- Defaults to **`--dry-run` preview** (no funds). Real execution requires `--i-understand-real-money` (→ `--confirm`).
- Reuses `live_guard` from the mainnet-parity work: `require_real_money_ack` + `check_not_paused` (the global kill-switch) run **before** any confirmed swap.
- Resolves the curated mint from a symbol via a curated-set config (3.4).

**Purpose:** prove the byreal-cli swap + auto-routing + CLI-keystore custody + gating end-to-end with minimal infra and zero real funds — *before* committing to the live-loop work in Phase A.

### 3.3 Phase A — agent-driven gated live, built second

Wire a `byreal_spot` `LiveVenue` into `resolve_live_venue` + `build_live_executor`, so a spot run flows through `Executor::live` + the `GatedBrokerSurface` decorator — inheriting the SafetyGate (pause + `venue_label`=Live mismatch), the run lifecycle, observability, and `check_venue_label_network` (a `BYREAL_SPOT_NETWORK`/mode arm). The agent emits the existing `TraderDecision` constrained to **Long/Flat** (buy / hold / sell-to-flat); `asset` identifies a curated SPL/xStock mint. Live marks come from `byreal-cli` token price (a Solana feed like Jupiter/Birdeye is a later enhancement, not v1).

`broker_label_for`/`REAL_MONEY_CREDS` gain a `byreal_spot` entry; the surface's `Preview`/`Live` mode is wired so a forward-test run uses `--dry-run` while a `venue_label=Live` run uses `--confirm`.

### 3.4 Curated set config

Operator-defined whitelist mapping `symbol → { mint, kind: spl|xstock, decimals }`, e.g. `SOL`, `JUP`, `AAPLx`, `NVDAx`. Drives symbol resolution for both `xvn spot` and the agent. Source: a config file under `xvn_home` (or settings table); xStocks need no special handling beyond a `kind` tag for display.

---

## 4. Data flow

```
Phase C:  operator → xvn spot --buy SOL→JUP --amount 50 [--dry-run default]
            → require_real_money_ack + check_not_paused (kill-switch)
            → ByrealSpotSurface(Preview|Live) → byreal-cli swap execute (--dry-run|--confirm) → JSON ack
            → print fill/preview

Phase A:  agent run (venue=byreal_spot) → Executor::live loop
            → marks via byreal-cli token price
            → TraderDecision(Long/Flat, mint) → engine risk veto
            → GatedBrokerSurface(ByrealSpotSurface) → SafetyGate.check_broker_submit
            → byreal-cli swap (--dry-run for Testnet/forward-test, --confirm for Live)
            → OrderConfirmation → run trace / observability
```

---

## 5. Safety & custody

- **Custody:** CLI-keystore (`~/.config/byreal/keys/`), set once via `byreal-cli wallet set`. The xvision process never reads/logs the key. (Note in the "no private keys" public claim alongside the perps caveat.)
- **No-funds default:** Phase C defaults to `--dry-run`; Phase A forward-test runs use Preview mode. Real funds move only with `--i-understand-real-money` / `venue_label=Live`.
- **Gating:** every real swap passes the existing SafetyGate (global pause/kill-switch + `venue_label` mismatch) — Phase A via `GatedBrokerSurface`, Phase C via `check_not_paused`.
- **Slippage guard:** surface a max-slippage bps (default conservative, e.g. 100 bps) and refuse / warn above it (byreal-cli warns > 200 bps; we cap lower).
- **Spot ≠ perps:** no leverage, no liquidation, no shorting — `TraderDecision` restricted to Long/Flat for this venue.

---

## 6. Components (boundaries)

| Unit | Responsibility | Depends on |
|---|---|---|
| `ByrealSpotApi` (trait) | mockable byreal-cli seam (swap, price, balance) | byreal-cli subprocess / mock |
| `ByrealSpotSurface` | `BrokerSurface` impl; buy/sell→swap, Preview/Live mode, slippage guard | `ByrealSpotApi` |
| `xvn spot` (CLI) | gated one-shot swap (Phase C) | `ByrealSpotSurface`, `live_guard` |
| curated-set config | symbol↔mint↔kind whitelist | xvn_home/settings |
| `byreal_spot` LiveVenue wiring (Phase A) | resolve/build/label/gate the venue | eval.rs, the gate |

Each is testable in isolation against the mock `ByrealSpotApi`.

---

## 7. Testing

- `ByrealSpotApi` mock → unit-test `ByrealSpotSurface`: buy maps to USDC→token swap, sell to token→USDC; Preview uses `--dry-run`; slippage over cap is refused; `venue()=="byreal_spot"`, `is_perp_venue()==false`.
- `xvn spot`: dry-run default proven (no `--confirm` emitted without the ack); ack + not-paused gates fire before a confirmed swap; symbol→mint resolution from the curated set.
- Phase A: a `gated_live_submit`-style test — paused gate ⇒ zero swaps reach the mock; `venue_label != Live` ⇒ blocked for a Live-labeled byreal_spot broker.
- Manual: a real `--dry-run` swap against `byreal-cli` on a couple of curated mints (incl. one xStock) to validate routing/preview JSON shape. No real funds.

---

## 8. Out of scope (future tracks, each its own spec)

- **Long-tail / discovery** (memecoins, new launches) — token discovery + rug/honeypot safety + liquidity checks.
- **LP / yield** (CLMM positions, copy-farming) — the `byreal_clmm.rs` seed; a different decision paradigm.
- **Backtest/optimizer integration** — needs historical SPL/xStock bars; spotty data.
- **Dedicated Solana price feed** (Jupiter/Birdeye/Pyth) — v1 uses byreal-cli prices; a feed is a later mark-quality enhancement.

---

## 9. Decision log

- **Sub-project first:** Spot trading (over LP/yield). — operator, 2026-06-15.
- **Universe:** curated SPL + xStocks (not long-tail discovery). — operator.
- **Backtest:** live/forward-test only first (no optimizer integration yet). — operator.
- **Approach:** C → A (thin gated CLI → agent-driven gated live), `byreal-cli` as the single integration. — recommended + approved after web validation.
- **RFQ/xStocks:** free (auto-routing / plain SPL tokens) — web-validated, no separate build.
- **Custody:** CLI keystore, not env key — web-validated.
