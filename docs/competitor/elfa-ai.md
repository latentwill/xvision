# Elfa.ai — competitor diligence and xvision integration recommendation

I’m treating this as market/product diligence plus an xvision build recommendation, verifying Elfa’s current product, API, docs, pricing, and integration fit.

Early findings: pricing signals conflict, so I’m weighting newer docs. Elfa looks strongest as crypto attention, intelligence, and trigger infrastructure—not something xvision should let own execution or risk.

## Verdict

Elfa.ai is worth integrating into xvision, but as a *read-only intelligence and trigger layer first*—not as an execution layer or a replacement for xvision’s Trader/Risk pipeline.

The best strategic fit is to feed Elfa’s social, narrative, token-news, and attention-spike data into xvision’s Intern stage, preserve the Trader as the decision maker, and keep all position sizing, vetoes, and execution inside xvision’s deterministic risk/execution stack. That matches xvision’s core design: eval-first, non-custodial, reproducible backtests/paper runs, and future Mantle/ERC-8004 validation receipts.

Elfa’s strongest value for xvision is social alpha infrastructure: “why is this token moving,” “who is talking about it,” “what narrative is emerging,” and “should this strategy wake up now?” Its weaker fit is autonomous trade execution, because that overlaps with xvision’s own safety, non-custodial, and auditable execution principles.

## 1. What Elfa.ai is now

Elfa positions itself as real-time data infrastructure for financial AI, aggregating social, on-chain, and market data for agents and trading products. It emphasizes four differentiators:

- context from credible speakers and an entity graph
- trigger infrastructure rather than only dashboards
- support for both humans and agents
- x402 pay-per-request access without a traditional API key

For xvision, the important point is that Elfa is not just a “sentiment API.” It has become a combined stack of:

### Data API
- token social metrics
- entity relationships
- event summaries
- token news
- trending tokens/contracts
- keyword mentions
- top mentions
- smart-account stats
- market/social features

### LLM analysis
- Ask Elfa / chat-style analysis
- modes such as macro, summary, token intro, token analysis, and account analysis

### Trigger engine
- Auto lets users define continuous conditions across price, technical indicators, market data, social signals, natural-language LLM checks, and time-based rules
- conditions can trigger notifications, webhooks, Telegram alerts, or downstream LLM analysis

### Execution-adjacent features
- Elfa can suggest trade setups and supports in-app trading paths, including Hyperliquid perps and LiFi swaps
- prefilled limit/TP/SL style setup flows
- disclosed trading fee structure

### Agent payment/access
- standard API-key access
- x402 pay-per-request access using USDC on Base
- x402 flow: agent can receive a `402 Payment Required`, sign a payment authorization, and retry the call

## 2. Feature set evaluation

### Core data features

Elfa covers the signals xvision would most likely want for crypto strategy evaluation:

- Trending tokens — candidate discovery and wake-up triggers
- Keyword mentions — ticker/project/event monitoring
- Top mentions — who said what, ranked by engagement
- Event summaries — condensed context for Intern evidence
- Trending narratives — narrative-level regime detection
- Token news — fundamental/event catalyst input
- Mindshare — cross-token attention allocation
- Sentiment-weighted mentions — better than raw mention count
- Account smart stats — source credibility and speaker quality
- Ask Elfa / chat analysis — optional second-opinion LLM analysis

The API reference groups endpoints across account, aggregation, data, chat, system, and Auto functionality. Elfa’s docs also say it tracks X/Twitter, Telegram, DEX/CEX market data, fund flows, news/announcements, and technical indicators, with coverage across Solana/EVM tokens and major DEX/CEX venues.

The strongest feature for xvision is structured social intelligence. Elfa explicitly distinguishes itself from raw Twitter by adding entity extraction, organic-attention filtering, structured sentiment, mention velocity, mindshare percentage, and a verified entity graph. That matters because xvision should not waste LLM context windows parsing noisy social feeds if it can ingest normalized, attributable features.

### Auto / trigger engine

Elfa Auto is promising but should be used carefully. Auto supports continuous condition trees with price, technical analysis, social/market conditions, and LLM-evaluated custom conditions. Its condition trees are limited to depth 3 and 10 leaves, which is enough for many practical triggers but not enough to replace xvision’s full strategy engine.

The right integration is:

- Elfa Auto wakes xvision up
- xvision decides
- xvision risk layer approves or vetoes
- xvision execution layer acts

Webhook/Telegram/SSE notifications make Auto useful as an upstream event source. But xvision should not delegate final order logic to Elfa Auto in the first integration.

### API access modes

Elfa supports two access modes:

- standard API-key access for `/v2` and `/v2/auto`
- x402 access under `/x402/v2`

Auto mutation endpoints require extra HMAC signing, replay protection, and timestamp validation.

For xvision:

- Use API-key mode first for reproducible evals, dashboard usage, and production reliability.
- Use x402 later for a hackathon-friendly or agent-economy demo, especially if xvision wants to show agents paying for external intelligence.
- Note the chain mismatch: Elfa’s x402 payments are on Base, while xvision’s roadmap emphasizes Mantle/ERC-8004. That is not fatal, but it means xvision should treat x402 as a vendor-payment rail, while strategy identity/reputation receipts remain anchored on Mantle.

### Pricing and cost profile

Elfa’s public pricing includes Free, Chill, Grow, Enterprise, PAYG, and x402 tiers. The docs list:

- Free — 1,000 credits/month
- Chill — $40/month for 5,000 credits
- Grow — $290/month for 40,000 credits
- PAYG — $0.009/credit
- x402 — also $0.009/credit with higher request limits

That is startup-friendly for a POC. The real cost risk is not the monthly plan; it is uncontrolled eval usage. A strategy loom that evaluates hundreds of strategies across many symbols and time windows can burn credits quickly unless xvision snapshots Elfa responses and replays them from local storage.

## 3. Revenue, growth, and ecosystem

Elfa appears to be an early but active crypto intelligence startup. I did not find audited revenue, ARR, or profitability numbers. So the revenue assessment should be conservative: Elfa has multiple plausible revenue lines, but public financials are not available.

Its visible revenue model includes:

- subscriptions: Chill, Grow, Enterprise
- PAYG API credits: $0.009/credit
- x402 pay-per-request: also listed at $0.009/credit
- trading fees: 0.05% default trade fee, with lower fees/rebates at higher trading tiers
- enterprise embedding / white-label / custom integrations

Elfa’s official enterprise page claims that integrations increased D1/D14 retention by 40%, session rate by 35%, and trade completion by 27%. Treat those as company-reported business claims, not independently audited metrics.

Growth indicators are stronger than financial transparency. Elfa’s own API page claims over 300 builders/product teams, a CoinGecko integration surfacing insights to a large user base, Drift users interacting with Ask Elfa, and Virtuals/Pacifica ecosystem usage. A company LinkedIn post also claimed active users grew 13x year-over-year, beta testers grew from 20+ to 700+, and Elfa was present in 300+ trader group chats.

Strategic read: Elfa has enough traction to justify a platform integration, but not enough public financial disclosure to justify deep dependency without fallback providers.

## 4. Competitive positioning

Elfa’s best positioning is:

- agent-native crypto social intelligence and trigger infrastructure

It is not obviously the broadest dataset, the most mature on-chain intelligence provider, or the strongest InfoFi/social-capital platform. Its edge is the combination of real-time social context + structured API + LLM analysis + Auto triggers + x402 agent access.

### Elfa vs Kaito

Kaito is stronger in the InfoFi / mindshare / creator-economy / community-distribution layer. Its public product surface includes Mindshare Arena, Kaito Studio, Capital Launchpad, Trading Rewards, Airdrops, KAITO staking, and Kaito Suite including Pro, API, and MCP.

Elfa is more directly useful for xvision’s strategy engine because it exposes a practical agent/API/trigger layer. Kaito may be better for “which narratives and communities are winning attention,” while Elfa may be better for “feed this signal into an agent workflow right now.”

### Elfa vs Santiment

Santiment is a more mature research/data platform for historical social, on-chain, development, and market data. Its API page emphasizes backtesting strategies, historical data, GraphQL access, many metrics, and broad asset coverage. Santiment also offers social-trends tooling, hype scores, unique authors, social dominance, sentiment analysis, and AI summaries.

For xvision, Santiment may be better for historical reproducible research. Elfa may be better for real-time agent triggers and fast narrative context. They are more complementary than mutually exclusive.

### Elfa vs Nansen

Nansen is stronger on on-chain wallet intelligence, smart money, labels, and multichain wallet profiling. Its API is positioned around structured on-chain intelligence for agents, including wallet labels, smart money, token screening, and multichain workflows.

Elfa is stronger for social attention, market narrative, and social-triggered strategy context. A serious xvision data stack could eventually use both: Elfa for social/narrative evidence, Nansen for smart-money/on-chain evidence.

### Elfa vs LunarCrush

LunarCrush is a known social-intelligence competitor with crypto/social sentiment APIs and broader market/social metrics. Public search results describe LunarCrush APIs as covering social metrics, sentiment scores, and market data across thousands of coins and NFT collections.

Elfa’s sharper wedge is agent-oriented workflow: x402 access, Auto triggers, LLM analysis, and embeddable product integrations. LunarCrush may compete well on established social metrics breadth; Elfa competes on agent-native packaging.

## 5. Strategic fit for xvision

Elfa fits xvision in four places.

### A. Intern evidence layer

This is the cleanest fit. xvision’s Intern prepares neutral bull/bear/flat evidence before the Trader decides. Elfa can supply:

- “Top social catalysts for this token in the last 1h/4h/24h”
- “Narratives currently driving attention”
- “Mention velocity and mindshare change”
- “Smart or credible accounts discussing the asset”
- “Event summary for recent token/news/social activity”
- “Sentiment-weighted mentions”

The Intern should convert Elfa outputs into neutral evidence, for example:

**Bull evidence**
- Mindshare increased from X to Y over 4h
- Smart-account mentions increased
- Event summary indicates credible catalyst

**Bear evidence**
- Attention spike is mostly low-credibility or spam-like
- Sentiment-weighted mentions deteriorated
- Narrative is crowded/late

**Flat evidence**
- Mentions rose, but no confirming price/volume/on-chain signal

The Trader then decides independently.

### B. Eval and backtesting features

Elfa can be valuable for testing whether social/narrative signals improve strategies. But this only works if xvision solves reproducibility.

xvision should never call live Elfa during a replayed backtest. It should snapshot Elfa responses into SQLite with:

- endpoint
- params
- time window
- API version
- fetched timestamp
- normalized response
- raw response hash
- credit cost
- strategy/run ID
- source metadata

Then backtests replay the snapshot. That preserves xvision’s eval-first promise.

### C. Live trigger layer

Elfa Auto can trigger xvision when conditions become interesting:

- token mindshare spike
- trending narrative emerges
- credible-account mention
- token news appears
- price + social condition overlap
- LLM condition passes

But the trigger should only start an xvision analysis job. It should not place trades directly.

### D. Dashboard / operator UX

Elfa can materially improve xvision’s dashboard by adding:

- social/narrative panel per token
- attention-spike overlays on price charts
- “why this strategy fired” summaries
- social-feature contribution in eval comparisons
- Elfa-powered catalyst cards on live chart routes
- strategy authoring filters like “only trade when mindshare delta > threshold”

This aligns well with xvision’s existing dashboard, eval detail/compare pages, live chart route, authoring inspector, and operator workflows.

## 6. Recommended integration architecture

Build an optional `xvision-elfa` provider/module.

### Read-only POC scope

Start with these endpoints/features:

- Trending tokens
- Keyword mentions
- Top mentions
- Event summaries
- Trending narratives
- Token news
- Token mindshare / sentiment-weighted mentions, if available under the chosen tier
- Ask Elfa/chat analysis only as an optional secondary analysis, not as primary decision logic

Avoid Auto trade actions and in-app execution at first.

### Suggested internal types

```text
ElfaSignal {
    asset: Symbol,
    window: TimeWindow,
    signal_type: SocialSignalType,
    score: f64,
    confidence: f64,
    direction: BullBearFlat,
    summary: String,
    source_endpoint: String,
    fetched_at: DateTime<Utc>,
    response_hash: String,
}

SocialFeatureVector {
    asset: Symbol,
    window: TimeWindow,
    mindshare: Option<f64>,
    mindshare_delta: Option<f64>,
    mention_count: Option<u64>,
    mention_velocity: Option<f64>,
    sentiment_score: Option<f64>,
    smart_account_mentions: Option<u64>,
    narrative_rank: Option<u32>,
    event_intensity: Option<f64>,
}
```

### CLI commands

Useful initial commands:

- `xvn provider elfa setup`
- `xvn elfa pull --symbols BTC,ETH,SOL --window 24h`
- `xvn elfa narratives --window 4h`
- `xvn elfa snapshot --scenario social-momentum-sol`
- `xvn eval run --strategy social_momentum --social-provider elfa --snapshot-id <id>`

### Strategy config example

```toml
[data.elfa]
enabled = true
mode = "api_key"
cache_ttl_seconds = 300
snapshot_for_eval = true
max_credits_per_run = 250

[strategy.social_filters]
require_social_confirmation = true
min_mindshare_delta = 0.03
min_smart_account_mentions = 3
ignore_if_event_summary_confidence_below = 0.60
```

### Live workflow

```text
Elfa Auto / webhook
        ↓
xvision remote job / MCP/API
        ↓
Intern gathers neutral Elfa + OHLCV + indicators + on-chain evidence
        ↓
Trader decides bull/bear/flat
        ↓
Deterministic Risk approves/vetoes/sizes
        ↓
Backtest / Alpaca paper / Orderly-Mantle path
        ↓
Run receipt + evidence hash + future ERC-8004 validation receipt
```

## 7. What not to do

- Do not let Elfa’s suggested trade setup become the xvision strategy decision. It can be evidence, not authority.
- Do not use Elfa Auto execution as the default xvision execution path. That would blur xvision’s core differentiation: deterministic risk, non-custodial execution boundaries, and auditable evaluation.
- Do not run Elfa live calls inside reproducible backtests. Snapshot first, replay later.
- Do not make Elfa a hard dependency. Build the provider abstraction so Santiment, Nansen, Kaito, LunarCrush, raw X/TG, or future in-house data can be added later.
- Do not redistribute Elfa-derived data in a public marketplace without checking licensing/terms. This matters because xvision’s roadmap includes marketplace, strategy identities, lineage, reputation, and validation receipts.

## 8. Main risks

| Risk | Why it matters | Mitigation |
|---|---|---|
| Vendor dependency | Elfa is early and private; public revenue data is limited | Optional provider, fallback data sources |
| API instability | Some docs mark endpoints as active development / experimental | Pin API version, schema tests, response hashing |
| Cost blowups | Eval loops can multiply API usage | Credit budgets, caching, snapshots |
| Backtest leakage | Live social data can contaminate historical evals | Time-windowed snapshots only |
| Social-signal noise | Attention spikes can be spam, manipulation, or late-cycle hype | Require confirmation from price/volume/on-chain/risk rules |
| Execution conflict | Elfa has trading/execution-adjacent features | Keep xvision execution authoritative |
| Chain mismatch | Elfa x402 is Base; xvision roadmap is Mantle | Use x402 as optional demo/payment rail; anchor strategy receipts on Mantle |
| Compliance/licensing | Social data redistribution may be constrained | Legal review before marketplace exposure |

## 9. POC recommendation

Run a read-only Elfa integration POC with three goals:

1. **Signal quality** — Does Elfa improve strategy selection or reduce false positives versus OHLCV-only or indicator-only strategies?
2. **Reproducibility** — Can xvision snapshot and replay Elfa-derived evidence cleanly inside eval runs?
3. **Operator value** — Does the dashboard become meaningfully clearer when it shows social/narrative reasons for trades?

### Suggested POC scope

- 5–10 liquid crypto assets: BTC, ETH, SOL, DOGE, XRP, HYPE, PEPE, WIF, LINK, AVAX
- 3 strategy templates:
  - social momentum confirmation
  - event-driven breakout
  - narrative exhaustion / fade
- Compare:
  - baseline TA strategy
  - TA + Elfa social confirmation
  - TA + Elfa narrative/event veto
- Success metrics:
  - better drawdown control
  - fewer low-quality entries
  - improved post-event timing
  - clear evidence trails in eval detail pages
  - bounded credit cost per run

Decision rule: advance only if Elfa signals produce measurable lift or operator clarity. If not, keep it as an optional dashboard intelligence plugin rather than a core strategy dependency.

## Final strategic call

Elfa should be integrated into xvision as a social intelligence provider, narrative catalyst detector, and live trigger source. That is a strong fit.

It should *not* be integrated as a trading executor or autonomous decision authority in the first version. That would weaken xvision’s differentiation.

The best version is:

- Elfa finds the market’s social/narrative pulse.
- xvision evaluates whether that pulse is tradable.
- xvision risk decides whether it is safe.
- xvision records the evidence trail for reputation and validation.

That pairing is strategically coherent and especially strong for a Mantle/ERC-8004 “strategy reputation” product: Elfa can provide external intelligence, while xvision provides evaluation, safety, provenance, and trust.
