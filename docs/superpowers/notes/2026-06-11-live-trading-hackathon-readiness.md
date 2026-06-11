# Live Trading — hackathon readiness assessment (2026-06-11)

State of the live-trading surface after the `feat/live-trading-hackathon`
branch, plus the ranked gap list to a credible hackathon entry. Companion
to the testnet runbook (`../specs/2026-06-11-orderly-testnet-runbook.md`)
and the live-trading spec (`../specs/2026-06-08-live-trading-marketplace-spec.md`).

## What works today (verified, not aspirational)

| Piece | Evidence |
|---|---|
| Orderly testnet onboarding (register + key + faucet claim), scripted | `scripts/orderly_testnet_onboard.py` ran clean against the real gateway; creds in 1Password `xvision-orderly-testnet` |
| Rust executor auth + signed order POST on testnet | `xvn fire-trade --venue orderly` reached the venue, structured rejection (`-1005 qty invalid` on $0 equity) — signing, account resolution, order endpoint all good |
| Venue account snapshot through the full stack | `GET /api/live/venue-account` on a running dashboard returned `connected: true, network: testnet` with real account data |
| Live page UI | Filterable strip (LIVE/PAUSED/STOPPED, strict `isLiveRun`), overlap fix, LIVE trace capsule, `/live/runs/:id` live inspector, venue account panel — 1991 frontend tests green |
| Orderly as a live-run venue | `broker_creds_ref = "orderly_testnet"` accepted by `build_live_executor`, hard-walled to testnet base URLs; executes via the real `OrderlyLiveSurface` |

## Update (later 2026-06-11): round trip DONE

Funded via the on-chain Mantle Sepolia path (token `faucet()` → vault
deposit; `scripts/orderly_testnet_fund_mantle.py`) — 5,000 USDC credited
to the broker-`demo` account. Round trip completed through the real
executor: BUY 3380371731 (SOL @ 65.287) → position verified → reduce-only
CLOSE 3380371909 (@ 64.745) → flat. Four real executor bugs were found
and fixed by this validation (see the runbook).

## Blocked / pending

1. **End-to-end live strategy run on Orderly testnet** — wiring is in
   place (gate lifted, surface implemented, account funded); needs a
   dashboard booted with both `ORDERLY_*` (execution) and `APCA_*`
   (market data) env. Recipe in the runbook.

## Gap list to hackathon-ready (ranked)

1. **Funded round-trip demo** (blocked on faucet, everything else ready).
2. **Live-run UX glue**: a launch surface for `broker_creds_ref =
   "orderly_testnet"` — today a live run is launched via API/CLI JSON;
   the wizard only offers Alpaca. A minimal "venue" select in the deploy
   flow would let judges launch without curl.
3. **Wallet→venue identity link**: the venue panel shows the connected
   browser wallet and the Orderly account side by side, but nothing
   verifies the Orderly account was derived from that wallet. Cheap win:
   derive account id client-side (keccak(address, broker)) and badge
   "account ↔ wallet verified".
4. **On-chain wallet balances** (USDC on Arbitrum Sepolia / Mantle) via
   `eth_call` in the SPA — "from blockchain" optics for the demo beyond
   the venue ledger.
5. **Position/equity reconciliation**: LiveAccountStrip derives from the
   eval stream; VenueAccountPanel reads the venue. They can disagree
   (e.g. manual venue trades). A drift badge would preempt judge
   questions.
6. **Safety story slide**: the safety gate, paused/flatten flags, stop
   policies, and the testnet-only hard walls (both venues) are strong
   differentiators — document them as a feature, not fine print.
7. **Mainnet path note**: `VenueLabel::Live` is rejected by design;
   state the criteria to open it (per-strategy verdict + kill-switch
   hardening) so "is it real?" has an answer.

## Suggested demo script (once funded)

1. `scripts/orderly-testnet-smoke.sh` — CLI round trip, receipts on screen.
2. Boot dashboard with both env sets; `/live` shows the venue panel
   connected (testnet) with the faucet USDC.
3. Launch a live run (BTC, `decision_limit: 2`, `time_limit_secs: 900`,
   `venue_label: "testnet"`, `broker_creds_ref: "orderly_testnet"`).
4. Strip shows the run under LIVE; trace capsule reads LIVE; click into
   `/live/runs/:id` — config chips + timeline; venue panel position
   appears when the trader fires.
5. Stop → flatten; position closes on the venue.
