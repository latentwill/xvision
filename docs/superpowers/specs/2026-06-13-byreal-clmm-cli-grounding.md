# Byreal CLMM CLI grounding — LP position lifecycle (2026-06-13)

Stretch 4: a thin wrapper (`crates/xvision-execution/src/byreal_clmm.rs`) over
`@byreal-io/byreal-cli` (the Byreal **CLMM DEX on Solana** CLI — distinct from
`@byreal-io/byreal-perps-cli`) to open → rebalance → close a liquidity position,
surfaced in the run trace.

## Verified command surface (`positions`, from the CLI source)

| Op | Command |
|---|---|
| Open | `positions open --pool <addr> --price-lower <p> --price-upper <p> --amount-usd <usd> [--slippage <bps>] --confirm` → returns NFT mint |
| Add liquidity | `positions increase --nft-mint <addr> … --confirm` |
| Remove part | `positions decrease --nft-mint <addr> --percentage <1-100> --confirm` |
| Close (all) | `positions close --nft-mint <addr> --confirm` |
| List | `positions list [--user <addr>]` |

- `--confirm` actually executes (the CLI defaults to dry-run); `--dry-run` /
  `--unsigned-tx` are available for preview / external signing.
- **Rebalance has no single command** — a CLMM range change is modeled as
  `close(old) + open(new range)`. `ClmmLpAction::rebalance` does exactly that.

## What this PR grounds vs. defers

- **Grounded + unit-tested:** the CLI argument construction (pure
  `open_position_args` / `close_position_args`) and the lifecycle ordering
  (open → [close+open] → close) via a mock seam.
- **Not validated here:** the CLI's JSON output schema (the `nft_mint` field)
  and real on-chain execution — those need a funded Solana wallet (the CLI
  signs itself; `BYREAL_CLMM_NETWORK` selects the network). Same live-creds
  boundary as the perps path (bead `xvision-ym9v.9`).
- **Trace integration scope:** each lifecycle step emits a structured
  `tracing` event (`target: "xvision::byreal_clmm"`) and the action returns a
  `ClmmLifecycle { steps }` the caller can record. Wiring the action into a
  strategy/eval-run pipeline (ObsEmitter spans in the run trace) is a follow-up;
  this PR delivers the reusable, traced building block.

## Runtime dependency

`SubprocessByrealClmmApi` shells out to `npx -y @byreal-io/byreal-cli@latest`
(`npm i -g @byreal-io/byreal-cli` for a global install). The CLI manages its own
Solana wallet/keypair.
