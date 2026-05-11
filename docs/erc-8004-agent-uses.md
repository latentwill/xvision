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

## Adjacent ecosystems — how other agent stacks treat identity

ERC-8004 isn't the only place agent identity is being defined. Several adjacent
stacks are converging on the same primitive (wallet-linked identity + trust
signals + permissions) but emphasizing different layers of the stack. Worth
watching as we decide how `xvision` agents present themselves on-chain.

### Olas — identity as a monetizable role layer

Olas treats agent identity as a brand and a revenue surface. The homepage
pitches "enables everyone to own and monetize their AI agents," and the
rotating role carousel includes "AI Influencer," "Stock Trader," and "Crypto
Trader." That reads less like a bot framework and more like a directory of
agent personas people can own, brand, and monetize. See
[Olas](https://olas.network/).

### ElizaOS — identity as trust, permissions, and receipts

Recent ElizaOS repo discussions are pushing identity toward cryptographic trust
and on-chain provenance:

- [AgentID](https://github.com/elizaOS/eliza/issues/6688) — cryptographic
  identity layer with wallet-linked keys, trust levels, challenge-response,
  and blockchain receipts.
- [AgentFolio](https://github.com/elizaOS/eliza/issues/6635) — verified
  on-chain identity, trust scores, and discoverable profiles.
- [SafeAgent](https://github.com/elizaOS/eliza/issues/6706) — pre-trade token
  safety checks that block risky swaps; identity gates trading rights.

In practice, the identity layer is being used to decide whether an agent is
legitimate enough to trade, delegate to, or interact with. The dominant
pattern: *prove who you are, prove you behaved safely, then earn the right to
trade*.

### Trading is the destination, not a side example

The Eliza repo now lists on-chain trading examples as first-class: `polyagent`,
`polymarket`, `trader`, and `lp-manager` (see the
[Eliza README](https://github.com/elizaOS/eliza/blob/main/README.md)). A recent
plugin proposal,
[plugin-hiveexchange](https://github.com/elizaOS/eliza/pull/6963), goes further
by giving agents access to live prediction markets and autonomous Genesis
agents already trading on-chain. The identity layer is being used to route
capital and market access — not just to decorate a bot profile.

### Bittensor — identity as protocol credentials, not branding

Bittensor is more protocol-native and less persona-heavy. The identity docs
focus on coldkeys, hotkeys, SS58 addresses, and subnet registration rather
than public-facing agent brands; the docs frame Bittensor as producing
commodities including financial-markets prediction. See
[Bittensor Accounts & Identity](https://subtensor.com/learn/core/accounts-identity)
and [Bittensor docs](https://docs.bittensor.com/). The clearest public trading
example is
[Subnet 8 :: Proprietary Trading Network](https://www.youtube.com/watch?v=qUEZfGT8LnY),
where identity is mostly wallet/key roles inside a subnet, not a social
profile.

### Cross-ecosystem patterns

1. **Identity is becoming a trust primitive** — wallet-linked keys, trust
   scores, and receipts decide whether an agent can be trusted to trade or be
   delegated to. See
   [AgentID](https://github.com/elizaOS/eliza/issues/6688) and
   [AgentFolio](https://github.com/elizaOS/eliza/issues/6635).
2. **Trading agents are being wrapped in safety rails** — pre-trade risk
   checks and token safety filters are part of the identity/permission story.
   See [SafeAgent](https://github.com/elizaOS/eliza/issues/6706).
3. **Agent-native markets are the real destination** — prediction markets, LP
   management, and on-chain trading are not side examples anymore. See
   [plugin-hiveexchange](https://github.com/elizaOS/eliza/pull/6963) and the
   [Eliza README](https://github.com/elizaOS/eliza/blob/main/README.md).
4. **Bittensor uses identity more like protocol credentials than branding** —
   focus on keys, roles, and subnet registration. See
   [Bittensor Accounts & Identity](https://subtensor.com/learn/core/accounts-identity).

### Implications for xvision

The convergence across Olas / ElizaOS / Bittensor / ERC-8004 says the same
thing from four directions: an agent that wants to *trade* or *be delegated
to* needs (a) a wallet-linked identity, (b) a trust signal someone else can
read, and (c) a permission gate that blocks unsafe actions before they
execute. ERC-8004 covers (a) and (b); SafeAgent-style pre-trade checks cover
(c). The wallet plan v1.1 + risk-gate + agent-card stack we're building maps
cleanly onto this — the open question is whether `xvision` wants the
*persona/monetization* surface (Olas-style) or just the *protocol credential*
surface (Bittensor-style).
