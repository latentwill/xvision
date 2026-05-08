# ERC-8004 in practice

ERC-8004 appears to be used mostly as a *trust layer for autonomous agents* - not as a payment rail by itself.

## What builders are doing with it

- **Agent identity / registration** - teams are minting an on-chain agent ID and pointing it to a JSON agent profile with endpoints, wallets, and capabilities. The EIP defines this as the Identity Registry, and the Base / Polygon docs show it being used as a discoverable agent card. See [ERC-8004: Trustless Agents](https://eips.ethereum.org/EIPS/eip-8004) and [Polygon’s ERC-8004 docs](https://docs.polygon.technology/payment-services/agentic-payments/agent-integration/erc8004/).

- **Reputation and validation** - people are attaching feedback, attestations, and verification results to agents so other agents or apps can decide whether to trust them. The standard’s own framing is “discover agents and establish trust through reputation and validation.” See [ERC-8004: Trustless Agents](https://eips.ethereum.org/EIPS/eip-8004) and [8004.org](https://www.8004.org/learn).

- **Agent registries and explorers** - builders are indexing ERC-8004 contracts so agents can be searched and browsed like a directory. Examples include [0xbits/8004-indexer](https://github.com/0xbits/8004-indexer), [dmihal/erc-8004-indexer](https://github.com/dmihal/erc-8004-indexer), and explorer-style tools like [Agent City 8004 Scan](https://agentcity.dev/8004scan/).

- **Agentic wallets and payments** - ERC-8004 is being paired with wallet stacks like ERC-4337 and x402 so agents can prove who they are before they transact. Cobo’s writeup and related agentic-wallet docs frame ERC-8004 as the identity layer that makes autonomous payments safer. See [Cobo’s agentic wallet article](https://www.cobo.com/post/erc-8004-on-chain-identity-standard-for-ai-agents-the-future-of-agentic-wallets) and [eco’s support note](https://eco.com/support/en/articles/14730441-ai-agent-wallets-erc-4337-erc-8004).

- **Autonomous commerce / agent-to-agent work** - the big thesis is “agents publish capabilities, find each other, verify trust, then transact.” That shows up in the official site and in partner explainers that connect ERC-8004 with x402 and job/escrow standards. See [8004.org](https://www.8004.org/) and [The Graph’s x402 + ERC-8004 explainer](https://thegraph.com/blog/understanding-x402-erc8004/).

- **Real workloads like DeFi ops and treasury actions** - some examples are already moving beyond demos. I found references to agents managing DeFi portfolios and to a hackathon project where five ERC-8004 agents coordinate a Mantle treasury action with policy checks, execution proof, validation, and reputation feedback. See [RedStone’s ERC-8004 writeup](https://blog.redstone.finance/2026/02/12/erc-8004-gives-ai-agents-identity-redstone-and-credora-power-them-with-data-and-risk-intelligence/) and [this GitHub example](https://github.com/ychenfen/agentic-wallet-treasury).

## Plain-English takeaway

People are using ERC-8004 to give agents a *name, reputation, and proof trail* so other agents, wallets, and marketplaces can trust them enough to work with them.

In other words: ERC-8004 is showing up as the coordination and trust layer for agentic systems, while payment rails, execution, and escrow often come from adjacent standards.

## Handy references

- [ERC-8004: Trustless Agents](https://eips.ethereum.org/EIPS/eip-8004)
- [8004.org](https://www.8004.org/)
- [Ethereum Foundation dAI Team intro](https://ai.ethereum.foundation/blog/intro-erc-8004)
- [Forbes - AI Agents Gain Trust Via Ethereum: ERC-8004 On Mainnet](https://www.forbes.com/sites/digital-assets/2026/02/05/ai-agents-gain-trust-via-ethereum-erc-8004-on-mainnet/)
