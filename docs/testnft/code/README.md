# Xvision test NFT bitfield prototypes

## Production engine: Bitfields v3

The production generator (one engine, symmetry-as-a-trait: free/mirror/quad/diagonal/anti-diagonal/rot180/rot90, 33-palette roster, compact stroke-path SVG tokenURI) lives at `crates/xvision-identity/src/genart.rs` and `frontend/web/src/features/marketplace/lib/genartGrid.ts` + `genart.ts`, byte-parity enforced by `crates/xvision-identity/tests/fixtures/genart_v3.json`. Spec: `docs/superpowers/specs/2026-06-11-strategy-nft-genart-onchain-design.md`. The prototypes below (v1 lanes, v2 studies) are parked design research.

Browser-openable generative-art prototypes for Xvision strategy NFT exploration.

## Structure lanes

Open `index.html` directly or serve this directory with any static file server.
The main prototype renders four deterministic bitfield structure lanes:

- Strategy Cathedral — recursive nave/chapel composition for stable slow systems.
- Regime Faultline — stratified regime history with drawdown fracture.
- Circuit City — market-making / execution topology as PCB-city modules.
- Lineage Mutation Tree — inherited strategy skeleton with child mutations.

The renderer intentionally keeps structure before texture: macro composition is chosen first, then restrained bitfield fills add local detail.

## Xvision Bitfields v2

Open `v2/index.html` for the earlier structured bitfield study set. It preserves the closer p5_bitfields-inspired language from the second iteration:

- Calm Sunset Stack
- Punolit Signal Mass
- Liquidity Relic
- Strategy Genome

The v2 renderer emphasizes coarse grids, low-state palettes, transparent state skips, row/radial banding, and bitwise `AND` / `XOR` / `OR` composition.
