# ADR 0008 — ERC-8004 Registry Deployment on Mantle

> **2026-05-10:** Project renamed `xianvec` → `xvision`. References below reflect the post-rename name; project history prior to this date used `xianvec`.

## Status: Accepted, deferred (2026-05-08)

> **Deferral note (2026-05-08):** Operator execution of this ADR is paused.
> v1 of Xvision ships as Alpaca-paper eval only with no on-chain function.
> The deployment runbook below stays valid as-is; it gets picked back up
> after the Strategy Creation Engine and Eval Engine are shipped and
> battle-tested end-to-end. The broader marketplace + commerce contract
> surface that builds on these registries is designed in
> [`docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md`](../docs/superpowers/specs/2026-05-08-smart-contract-surface-design.md)
> and is deferred under the same gate.

## Context

Phase 6.5 ships the `xvision-identity` client crate.  It uses stub `sol!`
interfaces for the ERC-8004 IdentityRegistry and ReputationRegistry because:

1. **ERC-8004 is Draft** (EIP repo, 2025-05).  No canonical ABI has been
   finalised and no reference implementation is deployed on Mantle mainnet
   (chain 5000) or Mantle Sepolia testnet (chain 5003).

2. The stub interface matches the EIP draft §3 signatures:
   - `register(string agentURI) → uint256 agentId`
   - `giveFeedback(uint256 agentId, int128 value, uint8 valueDecimals, string tag1, string tag2, string endpoint, string feedbackURI, bytes32 feedbackHash)`
   - `getFeedback(uint256 agentId, uint256 index) → (...)`
   - `getFeedbackCount(uint256 agentId) → uint256`

## Decision

**v1 deploys its own minimal registry contracts** to Mantle Sepolia testnet
before Phase 11.5 (forward Orderly run), and to Mantle mainnet only after
Phase 9 eval clears.

## Deployment steps (operator runbook)

### Prerequisites
- Foundry installed (`curl -L https://foundry.paradigm.xyz | bash && foundryup`)
- Funded operator wallet on Mantle Sepolia (faucet: `https://faucet.sepolia.mantle.xyz`)
- 1Password entry `op://xvision/mantle-operator/private-key` provisioned

### Contracts to deploy

**IdentityRegistry** — minimal ERC-721 + URIStorage:

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

import "@openzeppelin/contracts/token/ERC721/extensions/ERC721URIStorage.sol";
import "@openzeppelin/contracts/access/Ownable.sol";

contract IdentityRegistry is ERC721URIStorage, Ownable {
    uint256 private _nextTokenId;

    constructor() ERC721("XvisionAgent", "XVNA") Ownable(msg.sender) {}

    function register(string calldata agentURI) external returns (uint256 agentId) {
        agentId = _nextTokenId++;
        _safeMint(msg.sender, agentId);
        _setTokenURI(agentId, agentURI);
    }
}
```

**ReputationRegistry** — minimal feedback store:

```solidity
// SPDX-License-Identifier: Apache-2.0
pragma solidity ^0.8.20;

contract ReputationRegistry {
    struct Feedback {
        address rater;
        int128  value;
        uint8   valueDecimals;
        string  tag1;
        string  tag2;
        string  endpoint;
        string  feedbackURI;
        bytes32 feedbackHash;
        uint256 timestamp;
    }

    mapping(uint256 => Feedback[]) private _feedback;

    function giveFeedback(
        uint256 agentId,
        int128  value,
        uint8   valueDecimals,
        string calldata tag1,
        string calldata tag2,
        string calldata endpoint,
        string calldata feedbackURI,
        bytes32 feedbackHash
    ) external {
        _feedback[agentId].push(Feedback({
            rater: msg.sender,
            value: value,
            valueDecimals: valueDecimals,
            tag1: tag1,
            tag2: tag2,
            endpoint: endpoint,
            feedbackURI: feedbackURI,
            feedbackHash: feedbackHash,
            timestamp: block.timestamp
        }));
    }

    function getFeedback(uint256 agentId, uint256 index)
        external view
        returns (
            address rater, int128 value, uint8 valueDecimals,
            string memory tag1, string memory tag2,
            string memory endpoint, string memory feedbackURI,
            bytes32 feedbackHash, uint256 timestamp
        )
    {
        Feedback storage fb = _feedback[agentId][index];
        return (fb.rater, fb.value, fb.valueDecimals, fb.tag1, fb.tag2,
                fb.endpoint, fb.feedbackURI, fb.feedbackHash, fb.timestamp);
    }

    function getFeedbackCount(uint256 agentId) external view returns (uint256) {
        return _feedback[agentId].length;
    }
}
```

### Deploy commands (Mantle Sepolia, chain 5003)

```sh
PRIVATE_KEY=$(op read op://xvision/mantle-operator/private-key)

forge create IdentityRegistry \
  --rpc-url https://rpc.sepolia.mantle.xyz \
  --private-key "$PRIVATE_KEY" \
  --broadcast

forge create ReputationRegistry \
  --rpc-url https://rpc.sepolia.mantle.xyz \
  --private-key "$PRIVATE_KEY" \
  --broadcast
```

### After deployment: update `RegistryAddresses`

Replace the `None` returns in `RegistryAddresses::mantle_testnet()` in
`crates/xvision-identity/src/client.rs`:

```rust
pub fn mantle_testnet() -> Option<Self> {
    Some(Self {
        identity_registry:  "0x<DEPLOYED_IDENTITY_REGISTRY>".parse().unwrap(),
        reputation_registry: "0x<DEPLOYED_REPUTATION_REGISTRY>".parse().unwrap(),
    })
}
```

And the same for `mantle_mainnet()` after Phase 9 eval clears.

### ABI migration note

If/when ERC-8004 is finalised and a reference registry is deployed by the
EIP authors on Mantle, replace the stub `sol!` interfaces in `client.rs`
with the canonical verified ABI.  The `giveFeedback` / `getFeedback` /
`getFeedbackCount` signatures match the draft and are unlikely to change
materially; the only expected delta is the event definitions (not currently
used in `read_reputation`).

## Consequences

- Integration tests (`#[ignore]`d) will pass once the operator deploys
  contracts and updates the addresses above.
- Phase 11.5 forward Mantle run is unblocked after testnet mint succeeds.
- Production mainnet mint is gated on Phase 9 eval clearing (see
  `FOLLOWUPS.md §F4`).
