# 0006 — On-chain executor choice: Orderly Network on Mantle

Date: 2026-05-03
Status: accepted

## Decision

The v1 on-chain trade executor is **Orderly Network on Mantle**, integrated via the Rust crate `orderly-connector-rs = "0.4"` (ranger-finance, MIT). Trades execute on Mantle (chain_id 5000) against Orderly's shared orderbook. ERC-8004 identity, reputation, and validation registries also live on Mantle, so the agent's full audit trail is single-chain.

The Stage 1 Intern still vendors the **Byreal Agent Skills** filesystem catalog under `.claude/skills/byreal/`. This satisfies the Path 1 brief endorsement of Byreal tooling (one of the three named tools — Byreal Agent Skills / Byreal Perps CLI / RealClaw) without forcing the trade path through Hyperliquid.

The **Byreal Perps CLI** path is documented as an evaluated alternate in this file. M0 probe at `probes/m0-byreal/` passes against the live CLI, so a fork to Byreal-Perps execution is mechanically straightforward if the hackathon brief turns out stricter than read.

## How we got here (one day, three pivots)

1. **Original architecture (pre-2026-05-03 morning).** Named "Byreal Perps on Mantle" — wrong on its face: Byreal CLMM is Solana, Byreal Perps CLI is Hyperliquid, the "Byreal-on-Mantle" association is the Mantle Super Portal bridging MNT into Byreal's *Solana* liquidity.
2. **Morning pivot to Vertex Protocol.** Tried `vertex-sdk = "0.3.7"` against `ClientMode::MantleProd` and `MantleTest`. **M0 failed catastrophically:** all Vertex gateways returned TLS-handshake-cut errors, all four marketing/docs/app/api domains 404, GitHub repos ~1 year stale (`vertex-rust-sdk` last push 2025-05-30, `vertex-contracts` last commit 2025-05-14). Vertex appears to have wound down operationally despite still being listed as "Featured · Official · Live" on the Mantle ecosystem registry.
3. **Mid-day fallback to Byreal Perps CLI on Hyperliquid.** M0 probe at `probes/m0-byreal/` PASSED — `npx -y @byreal-io/byreal-perps-cli@latest catalog -o json` returns 20 capabilities under a clean `{success, meta, data}` envelope, all Phase 6.3 primitives present (one minor naming note: `position.close` is split into `close-market` / `close-limit` / `close-all`). Architecture and implementation-plan committed at `1703b71`.
4. **End-of-day discovery of Orderly Network** (suggested by another agent): FusionX, which was on the Mantle ecosystem registry, runs on Orderly's shared orderbook (broker_id `fusionx_pro`). Orderly has a native Rust SDK and a Mantle deployment. M0 probe at `probes/m0-orderly/` PASSED — system status 0 (healthy), live BTC-PERP at $78,382, Mantle vault `0x816f722424B49Cf1275cc86DA9840Fbd5a6167e9` registered, all Phase 6.3 SDK methods resolve.

## Decision matrix (Byreal Perps CLI vs Orderly Network)

| Dimension | Byreal Perps CLI | Orderly Network |
|---|---|---|
| Execution chain | Hyperliquid | **Mantle** (chain 5000) + 17 other EVMs |
| Integration shape | Node.js subprocess shellout (`tokio::process::Command` + `serde_json` envelope parse) | **Native Rust async** (`OrderlyService::with_base_url`) |
| Hackathon Path 1 named tooling | ✓ ("Byreal Perps CLI" verbatim) | ✗ named, but ✓ Mantle-native infra |
| Live + healthy | ✓ CLI v0.3.7 | ✓ REST 200, mark live |
| SDK staleness | n/a (CLI is current) | `orderly-connector-rs 0.4.15` last published 2025-06 (~11 months); compiles + works against the live API today |
| ERC-8004 narrative | Cross-chain (identity Mantle, trades Hyperliquid) | **Single-chain** (identity + trades + reputation all on Mantle) |
| Liquidity | Hyperliquid-only | Shared orderbook with 340+ brokers (Ranger, FusionX, Aark, Ascendex, Kai, …) |
| Runtime deps | + Node.js 18+ | none (pure Rust) |

## Why Orderly wins

1. **Mantle-native execution.** Trades execute on the host chain of the hackathon. ERC-8004 identity and reputation live on the same chain as the trade record. The cross-chain audit story collapses to a single-chain audit story — fewer moving parts in the demo.
2. **Native Rust SDK.** The architecture's all-Rust runtime stays whole. No Node.js dependency, no `tokio::process::Command` shellout, no JSON parsing layer between subprocess output and typed structs. The executor becomes ~150 lines of `async fn` calls instead of subprocess wrangling.
3. **Bigger liquidity surface.** Orderly is shared-orderbook infrastructure for 340+ brokers including Mantle-native ones. The agent doesn't depend on a single venue's depth.

## Why we kept the door open to Byreal

1. **Hackathon Path 1 names Byreal Perps CLI explicitly.** If a stricter reading of the brief requires using the named CLI for trade execution (not just Byreal Agent Skills for context), forking is mechanical: the executor trait stays unchanged, only `crates/xianvec-execution/orderly.rs` swaps for `byreal.rs`. The M0 probe at `probes/m0-byreal/` proved that path works.
2. **Byreal Agent Skills remain in v1.** Vendored under `.claude/skills/byreal/`, loaded into the Stage 1 Intern's Claude context. This is the single integration point that satisfies the Path 1 endorsement without forcing the trade venue.

## Risks accepted

- **`orderly-connector-rs` is ~11 months stale.** The crate was last published 2025-06 (v0.4.15). M0 confirms it compiles and reaches the current Orderly API. If a schema drift surfaces during Phase 6.3, the `ranger-finance` org is alive (docs repo pushed 2026-05-01) and a PR upstream is the obvious move; failing that, fork the crate. The 30k crate-downloads count and 99 live perp markets make it unlikely to be silently broken.
- **Hackathon Path 1 ambiguity.** If the brief is stricter than the slash-list reading suggests and *requires* Byreal Perps CLI for execution (not just Byreal Agent Skills for the Intern), we lose Path 1. Mitigation: the Byreal probe is preserved, the executor trait makes the swap mechanical, and we're still using Byreal Agent Skills throughout v1.

## Probe artifacts

- `probes/m0-orderly/` — passing M0' probe for Orderly. `cargo run --release` reproduces.
- `probes/m0-byreal/` — passing M0 probe for Byreal Perps CLI. Retained as the fork option.

Both directories stay in-tree until Phase 6.3 lands, then can be deleted (or kept as smoke tests in CI).
