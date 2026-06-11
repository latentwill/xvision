# T1.2 — Mantle Sepolia ERC-8004 round-trip sanity (2026-06-10)

On-chain proof that the Rust alloy client (`crates/xvision-identity`) works
against the real Mantle Sepolia (chain 5003) deployment, not just anvil.
Run via the new example `crates/xvision-identity/examples/testnet_roundtrip.rs`.

## Result: PASS

| Step | Outcome |
|---|---|
| Connect + chain_id check (5003) | OK |
| `register()` — mint NEW test agent | OK — **tokenId 1** (platform token #0 untouched) |
| `post_reputation()` — first `giveFeedback` entry | OK |
| `read_reputation()` decode of full log | OK — JSON `TradeOutcome` round-trips exactly |
| Second `giveFeedback` run (idempotency criterion) | OK — appended as index 1, index 0 unchanged, count 1 → 2 |
| Receipt statuses (all 3 txs) | `status=0x1` |

## Transactions

| Action | Tx | Block |
|---|---|---|
| Mint (register, agentURI `https://xvision.example/t1.2-roundtrip/9e3b76ac-4023-47aa-bd37-7888e13bef10.json`) | [`0xfb249bdc13bc38486b7b0ebd8185d4ab21f8f3f2659f2d5a43df71f2d2937d49`](https://sepolia.mantlescan.xyz/tx/0xfb249bdc13bc38486b7b0ebd8185d4ab21f8f3f2659f2d5a43df71f2d2937d49) | 39776565 |
| giveFeedback #1 (cycle `adb48184-345b-4104-8c5b-906ce8b74cc1`) | [`0x86896d34982a2101301da41d47b5e033d654d05bf5fbe8fb9ccaa784a5c0f3ef`](https://sepolia.mantlescan.xyz/tx/0x86896d34982a2101301da41d47b5e033d654d05bf5fbe8fb9ccaa784a5c0f3ef) | 39776569 |
| giveFeedback #2 (cycle `af5bb9fe-a6e6-4a80-a797-0d228417dffb`) | [`0x5813f682ca4a14c8bbb1bf0d5c7dab93180e61f3a96b59c73df5b780b1c25177`](https://sepolia.mantlescan.xyz/tx/0x5813f682ca4a14c8bbb1bf0d5c7dab93180e61f3a96b59c73df5b780b1c25177) | 39776582 |

Token: [IdentityRegistry token 1](https://sepolia.mantlescan.xyz/token/0x1DE1ccb2bBB5e1dE856BA096698b1A97f4484Fe4?a=1),
owner = operator EOA `0xb5d2a3734aF76eFb7bC258b35c970F1Cc9c4E553`.

## Decode verdict (F7/AM7)

**No decode mismatches.** The `sol!` stub bindings in
`crates/xvision-identity/src/client.rs` are bit-compatible with the deployed
contracts:

- `register(string) returns (uint256)` — tokenId extracted from the ERC-721
  `Transfer(0x0, to, tokenId)` event topics; the mint receipt carried 3 logs
  (Transfer + AgentRegistered + MetadataUpdate-style) and the Transfer parse
  found `tokenId = 1` with `to = operator EOA`.
- `giveFeedback(uint256,int128,uint8,string,string,string,string,bytes32)
  returns (uint256)` — matches the deployed
  `contracts/src/registries/ReputationRegistry.sol` signature exactly
  (contract doc explicitly promises bit-compatibility).
- `getFeedback` 9-tuple decoded cleanly for both entries: `feedbackURI` JSON
  parsed back into `TradeOutcome` (cycle_id, pnl 12.34, action "close")
  with no `IdentityError::Manifest`/`Encode` errors.
- `getFeedbackCount` returned 1 then 2 across runs, as expected.

Known cosmetic limitation (pre-existing, not a decode bug):
`read_reputation` reports `tx_hash` as `block:<timestamp>` because the
on-chain `getFeedback` view does not return the original tx hash
(documented in `client.rs`).

## Env vars needed on the demo host

```sh
export XVN_NETWORK=sepolia
export XVN_RPC_URL=https://rpc.sepolia.mantle.xyz          # default in the example
export XVN_CHAIN_ID=5003                                    # default in the example
export MANTLE_TESTNET_IDENTITY_REGISTRY=0x1DE1ccb2bBB5e1dE856BA096698b1A97f4484Fe4
export MANTLE_TESTNET_REPUTATION_REGISTRY=0xbb8A920d6d342a4FcF3929D91668cfbbfb14d2D8
# signer — never persist; inject per-command from 1Password:
#   PRIVATE_KEY=$(op read "op://Olympus/XVN Wallet/private key") <command>
```

## Repro commands

```sh
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
scripts/cargo build -p xvision-identity --example testnet_roundtrip

# fresh mint + feedback + readback:
MANTLE_TESTNET_IDENTITY_REGISTRY=0x1DE1ccb2bBB5e1dE856BA096698b1A97f4484Fe4 \
MANTLE_TESTNET_REPUTATION_REGISTRY=0xbb8A920d6d342a4FcF3929D91668cfbbfb14d2D8 \
PRIVATE_KEY=$(op read "op://Olympus/XVN Wallet/private key") \
scripts/cargo run -p xvision-identity --example testnet_roundtrip

# feedback-only re-run against an existing token:
XVN_TOKEN_ID=1 ... (same env) ...
```

Notes:
- The brief referenced `examples/mint_identity.rs`; no examples existed in the
  crate before this run — `testnet_roundtrip.rs` was created for T1.2.
- `xvision-identity` is excluded from workspace `default-members`; `-p` is
  required on every cargo invocation.
