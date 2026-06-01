# Ideonomy Explorations of the xvision Idea Space

> **Date:** 2026-05-10 (autonomous overnight run)
> **Method:** 12 invocations of `ideonomy-plain`, one per question, each with a fresh random tuple drawn from the operators × organons × dimensions catalog.
> **Trigger:** post-spec, post-plan, post-adversarial-review of the non-custodial wallets work; user requested broad first-principles exploration of clients, agents, profits, autooptimizer, marketplace, ERC-8004, and adjacent angles before bed.
> **Output convention:** each section is the artifact of one run, with intrinsic headers (no `Phase A` / `Operator: X` leakage); a footer lists the tuple drawn so future readers can trace the brainstorming machinery.

---

## Index

1. [Who actually deposits money to xvision?](#run-1--who-actually-deposits-money-to-xvision)
2. [What is an "agent" really, in this system?](#run-2--what-is-an-agent-really-in-this-system)
3. [Where does platform revenue come from?](#run-3--where-does-platform-revenue-come-from)
4. [What are the failure modes of the autooptimizer's mutation loop?](#run-4--what-are-the-failure-modes-of-the-autooptimizers-mutation-loop)
5. [Why would strategy creators bring work to xvision vs. Numerai/Composer/eToro?](#run-5--why-would-strategy-creators-bring-work-to-xvision-vs-numericomposeretoro)
6. [Is on-chain reputation (ERC-8004) actually valuable, or a checkbox?](#run-6--is-on-chain-reputation-erc-8004-actually-valuable-or-a-checkbox)
7. [What's the worst-case "first big loss" story?](#run-7--whats-the-worst-case-first-big-loss-story)
8. [What's the operator's daily job?](#run-8--whats-the-operators-daily-job)
9. [What's the irreducible v0 of this whole system?](#run-9--whats-the-irreducible-v0-of-this-whole-system)
10. [Why Orderly + Mantle vs. anything else?](#run-10--why-orderly--mantle-vs-anything-else)
11. [What changes if 1 user becomes 1,000 users?](#run-11--what-changes-if-1-user-becomes-1000-users)
12. [What does "win the hackathon" actually require?](#run-12--what-does-win-the-hackathon-actually-require)

A [final synthesis](#cross-run-synthesis) sits at the bottom.

---

## Run 1 — Who actually deposits money to xvision?

### Atlas of the depositor

#### Page 1 — Economic perspective

```
Archetype                 What they fund        Capital range     Yield expected
------------------------- --------------------- ----------------- -------------------
Crypto-native bot trader  diversification       $5k - $50k        beat their best bot
TradFi quant tourist      LLM-vs-rules question $1k - $10k        proof of concept
DAO treasury manager      idle USDC             $50k - $1M        yield + auditability
HNW crypto holder         passive trading       $25k - $500k      uncorrelated to BTC
Hobbyist                  entertainment         $100 - $1k        the dopamine hit
AI-agent tinkerer         "give my agent money" $50 - $500        creative experiment
Karpathy-watcher          curiosity             $20 - $200        watch the loom run
```

The economic perspective surfaces a load-bearing asymmetry: **the largest capital pools (DAO treasuries, HNW) are the most risk-averse and most demand auditability**; the smallest pools (hobbyists, agent tinkerers) tolerate everything but pay nothing. The middle (TradFi tourists, crypto bot traders) is where unit economics actually work. Plan accordingly.

#### Page 2 — Phenomenological perspective

```
Archetype                 First five minutes      First lost trade
------------------------- ----------------------- ------------------------------
Crypto-native             "let me see the code"   "expected, my bot loses too"
TradFi quant              "where's the Sharpe?"   "told you LLMs don't trade"
DAO manager               "where's the audit?"    "we need a post-mortem"
HNW                       "is this self-custody?" "irrational confidence"
Hobbyist                  "fun fun fun"           "fuck"
AI-agent tinkerer         "can I customize it?"   "skill issue"
```

The depositor lives in their head differently for each archetype. The system MUST surface different affordances on first session: a *code link* for the bot trader, a *backtest table* for the quant, a *runbook + audit log* for the DAO, a *self-custody confirmation modal* for the HNW. Same software, four different first-five-minute UIs.

#### Page 3 — Mythic / narrative perspective

The story each depositor tells themselves about why they're here:

- **Crypto-native:** "I'm an early adopter of agentic finance."
- **TradFi quant:** "I'm researching the boundaries of LLM judgment."
- **DAO manager:** "I'm responsibly diversifying our treasury."
- **HNW:** "I'm getting yield without giving custody to a CEX."
- **Hobbyist:** "I'm playing with the new thing."
- **AI-agent tinkerer:** "My agent has its own bank account now."
- **Karpathy-watcher:** "I'm watching how LLMs evolve via competitive pressure."

The story matters because it determines which features the depositor will defend in arguments and which they'll quietly tolerate when broken. A Karpathy-watcher will tolerate a UI that's a Jupyter notebook. A DAO manager will tolerate any UI as long as the audit trail is bulletproof. **Match feature investment to the depositor's story, not to the depositor's stated requests.**

#### Page 4 — Mechanistic / process perspective

The literal actions a depositor takes between "interested" and "deposited," in order:

1. Hears about xvision (Twitter / podcast / hackathon demo / Karpathy retweet).
2. Visits the landing page.
3. Decides whether to install a CLI or open a hosted dashboard.
4. Connects a wallet (or doesn't have one — full stop).
5. Onboards an Orderly account (the existing brokered/manual flow per FOLLOWUPS F5).
6. Reads about the Ed25519 trading-key model. Either trusts it or bounces.
7. Deposits USDC into Orderly Vault on Mantle.
8. Picks a strategy (or accepts the default).
9. Sets a budget cap.
10. Waits.

**Step 5 is the cliff.** Manual Orderly onboarding is a 10-minute non-self-serve process. Every depositor archetype loses 50%+ of conversion at this step. Automating onboarding (FOLLOWUPS F5) is probably worth more than three of the wallet-plan phases combined.

#### Page 5 — Information-flow perspective

Who tells whom about xvision, and what they say:

- **Karpathy-watcher → Karpathy-watcher:** "look at this insane thing"  → growth via curiosity, slow but loyal
- **Crypto-native → crypto-native:** "let me show you my P&L"  → growth via screenshot, fast and lossy
- **TradFi quant → TradFi quant:** "they ran a real Sharpe analysis"  → growth via paper / talk, slow and high-quality
- **DAO manager → DAO manager:** "here's the auditor's report"  → growth via professional network, slowest and largest tickets
- **AI-agent tinkerer → AI-agent tinkerer:** "you can give your bot a wallet"  → growth via demo, fast and small

**Each archetype has its own viral channel.** The marketing question isn't "what's our channel" — it's "which archetype's channel are we building for first?"

#### Page 6 — Temporal perspective

What time-horizon the depositor cares about:

- Hobbyist & agent tinkerer: hours-to-days. Want immediate dopamine.
- Crypto-native: days-to-weeks. Wants weekly P&L screenshots.
- TradFi quant: weeks-to-months. Wants statistically meaningful samples (~50 trades).
- HNW: months-to-years. Wants steady drip of small wins, no blowups.
- DAO manager: quarters-to-years. Wants quarterly board-presentable returns.

**xvision's current architecture (Alpaca paper today, Orderly live trading) optimizes for the days-to-weeks crowd.** The HNW / DAO crowds are the *largest* tickets but the *worst* match for a hackathon-deadline product. Sequence accordingly: build for crypto-natives + tinkerers in v0; HNW/DAO is a v2 sales motion.

#### Back-page — Negation: who does NOT deposit, and why

```
Archetype                       Reason they don't deposit
-----------------------------   ----------------------------------------------------------
Compliance-bound RIA            cannot self-custody to a third-party-controlled trading key
Regulated hedge fund            no Form ADV path; LLM as PM is unsigned-off
Retirement-saver                volatility profile incompatible; will not self-custody
Custody-averse skeptic          "I will not let any software hold the trading authority"
Non-English / non-Western user  no UI in their language, no wallet ecosystem
Existing bot operator           already has a working pipeline; won't switch for marginal alpha
LLM skeptic                     does not believe LLMs can make money trading
Wallet noob                     doesn't have USDC, doesn't have wallet, gives up at step 4
Anyone in a sanctioned jx       can't legally onboard at Orderly
```

The non-depositors are a *much larger market* than the depositors. The wallet-noob row in particular is several orders of magnitude larger than every other row combined. **The biggest growth lever for xvision isn't a feature — it's reducing the wallet-noob conversion friction.** This points at: embedded-wallet onboarding (Privy/Dynamic) as a higher-leverage future investment than any Phase 5+ feature in the wallet plan.

#### What the cross-perspective comparisons reveal

- **Economic + Mythic disagreement:** the largest-ticket depositors have stories that demand the most overhead (audit, attestation, post-mortems). Pursuing them changes what the product is.
- **Mechanistic + Negation disagreement:** the cliff at step 5 (Orderly onboarding) and the wallet-noob non-depositor row are the same problem viewed from two sides. **Solve once, lift two metrics.**
- **Temporal + Information-flow disagreement:** the depositors who tell the loudest stories (crypto-native, agent tinkerer) have short time-horizons; the depositors who write the durable stories (TradFi quant, DAO manager) operate on quarters. The marketing pipeline needs both — short-horizon screenshots for top of funnel, long-horizon white papers for the big tickets.

#### Tuple footer

```
operators:        combination · negation
organon:          atlas (6 perspective pages + 1 back-page negation)
dim prompts:      complexity · connectivity · discovery-vs-invention
not surfaced:     "the depositor as a population, not a single person" — atlas pages
                  treated each archetype as monolithic; populations within an archetype
                  (e.g., the tinkerer-who-becomes-quant) deserve their own future tuple.
```

---

## Run 2 — What is an "agent" really, in this system?

### Dictionary of "agent" and its in-system / cross-domain neighbors

#### Core in-system terms (xvision usage)

- **agent** *(xvision sense)* — A unit that produces a `TraderDecision` from a briefing. Currently used interchangeably with "strategy variant" in the codebase (`AgentManifest` vs `agent_id`), which is a precision shortfall worth fixing.
- **strategy** — A configured pipeline of slots (regime detector → intern briefing → trader judgment → risk gate). One strategy can be instantiated as many *variants* with different parameters.
- **strategy variant** — A specific instantiation of a strategy with concrete parameters (model id, temperature, prompt, asset universe, etc.). The thing that gets a `agent_id` and (post-SLF3) an ERC-8004 NFT.
- **slot** — A position in the pipeline (intern, trader, risk, executor). Slots are filled by *slot-machines* (the LLM-dispatch trait or deterministic implementations).
- **intern** — A slot that produces a neutral evidence briefing (bull / bear / flat cases) from market data. Does NOT recommend; recommends are forbidden.
- **trader** — A slot that consumes the intern's briefing and produces a `TraderDecision`. The judgment lives here.
- **risk** — A slot that vetoes / modifies the trader's decision via deterministic rules (caps, allowlists, daily-loss kill).
- **briefing** — The intern's output: a structured neutral packet of evidence with bull / bear / flat cases tagged.
- **decision** — The trader's output: a `TraderDecision` carrying action / side / size / stops / summary.
- **manifest** — `AgentManifest` (xvision-identity): the metadata that pins an agent's NFT identity. Fields: name, description, model id, strategy config hash, code commit, contact, created_at.
- **persona** — Currently absent in code. Sometimes used colloquially for "the trader's voice / style." If the project formalizes this, it would sit on the trader slot, not the agent.
- **loom** — The selection-pressure mechanism that runs many variants in parallel and ranks them. Variants ARE the loom's output.

#### Cross-domain re-instantiations of "agent"

- **agent** *(legal/common-law sense)* — A party authorized to act on behalf of a *principal* under a fiduciary duty. Two important properties: (a) the agent's actions bind the principal, (b) the agent owes loyalty / care / disclosure. **xvision's "agent" matches this almost perfectly except no fiduciary duty is articulated.** That's a gap.
- **agent** *(insurance/sales sense)* — An intermediary who sells products on commission. Duty is split between the principal and the customer; agency law calls this "dual agency" and it's regulated. **xvision's marketplace splits 95/5 like this; the platform is in dual-agency-like position.**
- **agent** *(AI/ML sense)* — A system that perceives state, takes actions, has goals, and adapts. Sutton-Barto definition. **xvision's strategies aren't agents in this sense — they don't adapt; the adaptation lives one level above on the loom.**
- **agent** *(espionage sense)* — A person collecting intelligence under cover, reporting to a handler. **Map: trader = agent, intern = source, operator = handler, marketplace = case officer's network.** The "agent gets burned" failure mode is exactly the trust-collapse story for a strategy that loses public money.
- **agent** *(biology — organism)* — A self-maintaining bounded entity. **xvision strategies are not organisms — they don't self-maintain, they're maintained by the runtime.**
- **agent** *(biology — pathogen / "infectious agent")* — A causal entity that produces an effect on a host. Neutral about valence. **A bad strategy is a pathogenic agent: the host is the user's account.** The aggregate-margin contagion risk in spec §3.4 is literally a contagion story.
- **agent** *(chemistry — "active agent")* — The compound that produces the effect, distinguished from inert excipients. **xvision's trader slot is the active agent; everything else (intern, risk, executor) is excipient that delivers the trader to the market.**
- **agent** *(theology — emissary)* — One who carries a message from a sender to a receiver and is identified-with the sender. ERC-8004 NFT identity has this flavor: the strategy is the visible carrier of the creator's judgment.
- **bot** *(software sense)* — Automated process running on a schedule. Most of what xvision calls "agent" is, in software terms, a bot. The "agent" framing imports the AI/ML connotations (autonomy, goals) that the system doesn't actually have yet.

#### Near-relatives (clarifying overlaps)

- **persona vs agent vs strategy** — three words used in xvision planning docs for similar things. Working distinction:
  - persona = the *voice* / prompt style (cosmetic)
  - agent = the *registered identity* (NFT-bound)
  - strategy = the *pipeline configuration* (functional)
  - A single agent has one strategy at any moment; an agent's strategy can change (mutation by the autooptimizer) but the agent identity persists.
- **decision vs trade vs order vs position** — four terms for the same trade life-cycle phase:
  - decision = the trader's output (intent)
  - trade = the intent + the executor's submission (action)
  - order = the message sent to Orderly (wire format)
  - position = the resulting holding (state)
  - The audit log Phase 1 Task 1.2 collapses three of these (`Stage::Sign` / `Stage::Submit` / `Stage::Fill`) and is correct to do so.

#### Dimensions surfaced by writing the dictionary

- **Symmetry** — agency is asymmetric in xvision (system acts for user; user does not act for system). A symmetric variant: a user could *judge briefings* and the system would learn; a user could *grade trades* and the system would update the trader's prompt. Currently nothing closes that loop. **The Karpathy autooptimizer (autooptimizer-1 plan) is the asymmetry-breaker** — it's the loop that makes the system jointly self-modifying.
- **Source** — agents in xvision arrive from three sources today: (1) hand-authored templates, (2) marketplace purchase, (3) autooptimizer mutation. A fourth source would be **environmental selection in the wild** — agents that survive paying users keep running, others get auto-decommissioned. Currently this is not modeled.
- **Autonomy** — strategies in xvision are *mostly-autonomous*: the trader produces a decision freely; the risk engine can only veto. A *fully-autonomous* variant: no risk engine, the trader controls its own caps. A *mostly-controlled* variant: every trade requires operator confirm. The hybrid quota model in the wallet spec puts xvision exactly in the "mostly-autonomous" sweet spot, with operator-tunable autonomy via budgets.

#### The terminology slippage and what it costs

The codebase uses **agent / strategy / variant** interchangeably in different files (`AgentManifest` vs `agent_id` vs `strategy variant`). This isn't just naming — it conflates three distinct concepts:

```
Concept layer       Identifier        Lifetime
------------------- ----------------- ------------------
Agent identity      NFT id            permanent (ERC-8004)
Strategy            config hash       changes with mutation
Variant             ULID + config     ephemeral, per-run
```

A clean refactor would: keep `agent` for NFT-bound identity (the legal/emissary sense), use `strategy` for the immutable config-hash-keyed pipeline, use `variant` for a specific run. The wallet plan's `agent_id` is closest to `agent` in the legal sense — it should probably be renamed `agent_id` and the audit log + ledger should join through it.

#### Tuple footer

```
operators:        dimension-identification · cross-domain-reinstantiation
organon:          dictionary (10 in-system + 9 cross-domain + 2 near-relative entries)
dim prompts:      symmetry · source · autonomy
not surfaced:     "agent" in *political-economy* (workers as agents of capital);
                  "agent" in *game theory* (rational utility-maximizer);
                  these would surface different gaps. The fiduciary-duty point from
                  the legal-sense entry is the highest-leverage finding — currently
                  no spec articulates xvision's duty of care to depositors.
```

---

## Run 3 — Where does platform revenue come from?

### Scale of value capture (axis: % of user value the platform extracts)

```
% extracted   Position name                Canonical exemplar                xvision fit
-----------   --------------------------   ------------------------------    ---------------------------------
0%            pure freeware / OSS          Bitcoin Core, Linux               not viable as a business
~0.1%         ad-supported                 free-to-play games                wrong audience (financial)
~1%           data-resale, no user fee     Yodlee, early Plaid               possible later (sell decisions)
~3%           transaction commission       Stripe, Square                    too low to fund LLM compute
~5%           marketplace cut              Apple App Store, Etsy             current spec — license sales
~10%          subscription per seat        Substack creator → reader         fit for hosted runtime
~15%          performance fee w/ HWM       hedge fund "2 and 20" (perf)      future option in spec §10
~20%          performance + management     classic 2/20                      not a fit (xvision ≠ fund)
~30%          marketplace + perf combo     Bookmaker / sportsbook            aggressive but legible
~50%          house edge                   casinos, lotteries                anti-pattern for trust
100%          full extraction              ponzi, bucket shop                criminal
```

**Current xvision position is the ~5% marketplace-cut anchor.** That's the only place the spec captures revenue today. Several adjacent positions are richer; several are wrong.

#### What sits adjacent to the 5% position (most-actionable expansions)

- **3% transaction commission + free runtime** — undercut competitors on price; no subscription friction; works only if marketplace volume is high enough to fund infra. **Risk:** xvision runs LLMs server-side, which costs money per fire — a 3% cut that doesn't include compute will lose money on every trade.
- **5% marketplace cut + 10% subscription for hosted runtime** — the hybrid most modern SaaS does. Subscription pays for the LLM compute; marketplace cut pays for the platform. **This is probably the right v2 position;** the wallet spec's hint at "performance fee on withdrawal" is in this neighborhood.
- **5% cut + 15% performance fee on PnL above HWM** — high-alignment incentive. Creators only earn when users earn. Fits the marketplace ethos. **Risk:** complex to implement (HWM bookkeeping requires a withdrawal helper contract per spec §10), and performance fees are tax-disadvantaged for users in many jurisdictions.

#### What lies *below* current position (the unattractive future to avoid)

- **0% (pure OSS, no revenue):** xvision becomes a hobby project. Karpathy's autooptimizer concept needs ongoing LLM compute funded by *someone*; without revenue, that someone is the operator, indefinitely.
- **1% data-resale:** the data product (decisions / briefings / regime classifications) is genuinely valuable to other quants — but selling it without consent from the strategy creators is a fast trust-collapse.

#### What lies *above* current position (the slippery slope)

- **30% bookmaker-style:** charging both creators and users a cut. Each side accepts a normal fee; charging both feels predatory. Fast brand damage even if math works.
- **50% casino:** xvision runs the strategies *itself*, takes the PnL, gives users a flat yield. This is not xvision — it's a hedge fund. Different regulatory regime, different product, different team.

### Tree of value-capture mechanisms (walked from "platform monetization")

```
Platform monetization (root)
|
+-- Toll-based (charge for access)
|   +-- Subscription (per-seat, per-tier)
|   +-- Pay-per-use (x402 micropayments per fire)            [spec §10 deferred]
|   +-- API call pricing (per-request)
|
+-- Commission-based (cut of value flowing through)
|   +-- Transaction cut (marketplace 95/5)                    [SPEC, current]
|   +-- Performance fee (cut of PnL above HWM)               [spec §10 deferred]
|   +-- Royalty (recurring cut of derivative use)
|
+-- Rent-based (charge for infrastructure)
|   +-- Compute markup (LLM inference resold w/ margin)
|   +-- Storage / hosting
|   +-- Egress / bandwidth
|
+-- Information-based (sell what you observe)
|   +-- Data resale (decision streams, briefings)
|   +-- Aggregated analytics (cohort behavior reports)
|   +-- Reputation queries (paid reads from ERC-8004)
|
+-- Float-based (earn on capital you hold temporarily)
|   +-- Settlement-wallet float yield (negligible at small scale)
|   +-- — explicitly avoided per the non-custodial constraint
|
+-- Indirect (revenue from non-customers)
    +-- Grants / hackathon prizes
    +-- VC investment
    +-- Token launch (the option xvision hasn't taken yet)
    +-- Reputation-driven consulting (operator's time)
```

**The tree exposes that xvision is currently using exactly ONE leaf** (transaction-cut commission), with two acknowledged but deferred (performance fee, x402 micropayments). **Five entire branches are unused** — toll-based, rent-based, information-based, float-based (deliberately), indirect. Most platforms eventually run on a *combination* across branches; running on one leaf is fragile.

### Dimensions surfaced

- **Rate** — how fast does revenue arrive?
  - License sale: lumpy, days-between-sales; fits "spike" cash flow.
  - Subscription: smooth, monthly; fits "drip" cash flow.
  - Performance fee: lumpy, withdrawal-triggered; fits "harvest" cash flow.
  - x402: smooth, per-trade; fits "stream" cash flow.
  - **A healthy platform usually has at least one drip and one harvest.** xvision has only spike. That's risky.

- **Purpose** — what's the revenue *for*?
  - Survival: keep the lights on, pay LLM bills, fund the operator's time.
  - Mission: fund the autooptimizer, ERC-8004 deployment, future plans.
  - Extraction: returns to the operator / future investors.
  - **Spec is silent on this.** Worth declaring: is xvision a non-profit-spirited public good with revenue floor for survival, or a startup with revenue ceiling unbounded? Different revenue-mix optimal for each.

- **Distribution** — concentrated vs distributed payers?
  - Concentrated (a few DAOs / HNWs fund most revenue): high renewal risk per-account; fewer relationships to manage; classic enterprise pattern.
  - Distributed (many small users at small fees): low per-account risk; high marketing cost per dollar; classic consumer SaaS pattern.
  - **Marketplace 95/5 is distributed by design** (many small license purchases). A v2 perf-fee tier would concentrate revenue toward larger depositors — useful counterweight.

### What the scale + tree together reveal

- **The biggest unfunded thing is LLM compute.** The current spec's 5% marketplace cut on license sales doesn't fund per-trade LLM inference. Either: (a) creators pay for the compute their strategy uses, (b) users pay subscription, (c) operator absorbs the cost. **Today the operator absorbs.** That's a hidden subsidy that won't survive scale.
- **The biggest *easy* revenue line not used: subscription for hosted runtime.** Most depositors will not run their own xvision node. Charging $20-100/month for hosted runtime is the simplest revenue line that aligns the platform's costs with its receipts.
- **The biggest *high-leverage* revenue line not used: data resale.** xvision accumulates decision streams across strategies; aggregated and anonymized, this is genuinely useful market microstructure data to other quants. Could be sold without harming the marketplace.

#### Tuple footer

```
operators:        tree-finding · organon-construction
organon:          scale (10 marked positions) + tree (5 branches × ~3 leaves each)
dim prompts:      rate · purpose · distribution
not surfaced:     "non-monetary revenue" — reputation, attention, recruiting leverage. The
                  platform's strongest asset early on may be its visibility to AI talent;
                  monetizing that (recruiting fees? speaking fees?) is its own scale worth
                  drawing.
```

---

## Run 4 — What are the failure modes of the autooptimizer's mutation loop?

### Lifted shape

xvision's autooptimizer is concretely: an LLM-driven mutator that perturbs strategy parameters, runs candidates through an eval engine, and promotes survivors based on a fitness score (Δ-Sharpe). Strip the surface:

> *A guided search over a high-dimensional configuration space, with a noisy and possibly misleading fitness signal, where each evaluation is expensive and each generation depends on the last.*

That shape exists in: drug discovery (compound search), neural architecture search, evolutionary biology, hyperparameter optimization, A/B-test culture in product teams, cultural evolution of memes, and quant fund strategy R&D. **All of those have decades of literature on failure modes.** xvision inherits every one.

### Chart: failure mode × earliest detectable signal

Rows = failure mode the mutation loop can hit. Cols = where you'd notice it first if you were watching for it.

```
                              Audit-log     Eval-score    Lineage-graph    LLM-spend     Live-PnL
Failure mode                  pattern       distribution  shape            burn-rate     diff
---------------------------   -----------   ------------  --------------   -----------   ----------
Reward hacking                X (briefings   X (clipped    X (one ancestor  -             X (eval-PnL
(games eval, not market)       repetitive)    upper tail)   dominates)                      gap widens)

Mode collapse                  X (prompts     X (variance   X (tree         -             X (correlated
(all variants converge)        identical)     collapses)    flattens)                       drawdowns)

Overfitting to backtest        -              X (in-sample  -                -             X (forward
                                              huge, oos                                     test fails)
                                              tiny)
Catastrophic forgetting        X (ancestors   X (recent     X (orphan       -             X (regression
(mutation destroys good        not retained)  worse than    branches)                       in known
ancestor)                                     baseline)                                     regime)

Drift                          X (cumulative  -             X (long single   -             X (slow
(small bad mutations            edits move    chain w/o      degradation)
compound)                       prompt        forks)                                        long horizon)
                                far from
                                spec)
Adversarial briefing exploit   X (decisions   X (single-   X (single        -             X (one regime
(strategy exploits format)      rely on a    regime         strategy
                                briefing      dominance)    spikes)
                                quirk)
Prompt injection from data     X (decision    -             -                -             X (suspicious
(market data → LLM)             text contains                                              divergence
                                "ignore                                                     after data
                                previous")                                                 anomaly)

Compute exhaustion             -              -             X (depth keeps   X (LLM bill   -
(loop runs without bounds)                                  growing)         spikes)

Selection-pressure mismatch    -              X (winners    -                -             X (Sharpe
(wrong metric optimized)                      mediocre on                                   wins eval,
                                              alt metrics)                                  CAGR loses)

Survivorship bias              -              X (apparent   X (losers       -             X (live
(losers discarded; winners                    out-perf      not retained                    underperforms
look better than they are)                    inflated)     for analysis)                   eval CI)

Model-version drift            X (audit       X (eval       X (mutations    -             X (post-
(Claude 4.6 → 4.7 mid-loop)    shows model    dist shifts   pre/post split  upgrade
                                hash change)  at boundary)  is visible)                     regression)
```

**Reading the chart:** the audit-log + lineage-graph columns are the most-information-dense — they catch 7 of the 11 failure modes early. **Plan #2 (autooptimizer) MUST include lineage-graph storage as a first-class artifact**, not an afterthought. The audit-log ties to the wallet plan's `decisions` table — same primitive serves both surfaces.

### Cross-domain re-instantiations of the abstract failure pattern

Each domain has already worked out countermeasures for the shape. Borrow them:

#### Drug discovery (compound mutation + assay)
- **Counter to reward hacking:** orthogonal assays — measure the *same* strategy on a *different* eval (Sharpe AND Sortino AND CAGR; in-sample AND walk-forward; multiple market regimes). If a winner only wins on one metric, suspect hacking.
- **Counter to overfitting:** held-out validation set never seen during mutation. xvision analog: an eval-window the autooptimizer *cannot* run against, used only at promotion time.

#### Evolutionary biology (genetic algorithms)
- **Counter to mode collapse:** speciation pressure — penalize candidates that are too similar to existing surviving strategies (cosine-distance on prompt embeddings).
- **Counter to catastrophic forgetting:** elitism — always keep the top-K ancestors permanently, never let mutation overwrite them. xvision analog: a "champions league" of strategies that never get overwritten, only joined.

#### Neural architecture search
- **Counter to compute exhaustion:** budget per generation, hard-stop wall-clock, early-stopping on plateaus.
- **Counter to selection-pressure mismatch:** multi-objective NSGA-style Pareto fronts instead of single fitness scalar. xvision analog: surface the Pareto front of (Sharpe, max-drawdown, turnover) instead of ranking on one number.

#### Quant fund strategy R&D
- **Counter to survivorship bias:** preserve the dead. Every killed strategy stays on disk with its full eval history; periodic post-mortem reviews surface patterns in *what dies*, not just what survives.
- **Counter to drift:** version-pin the briefing generator separately from the trader prompt. If the briefing format changes, all dependent strategies are flagged for re-evaluation.

#### Cultural evolution of memes
- **Counter to adversarial briefing exploit:** the meme that survives is the one optimized for *transmission*, not for *truth*. Analog: a strategy that wins because it looks good in the briefing format may be optimized for the briefing's quirks. **Periodic briefing-format randomization** (small permutations to field order, label phrasing) breaks brittle strategies.

### Substitution: what if the autooptimizer were OPPOSITE on each dimension?

- **Hierarchicalness** — currently flat (one population). Substitute *hierarchical*: tiered league with promotion / relegation. Junior strategies prove themselves in a low-stakes pool before being promoted to a high-stakes pool. Many failures (mode collapse, overfitting, survivorship bias) are reduced because the promotion gate is harder than the survival gate.
- **Intentionality** — currently fully automated mutation. Substitute *deliberately designed*: every mutation comes from a human-written hypothesis ("I think this strategy will work better if it weights funding-rate more heavily"). Slower, but every winner is interpretable and the lineage graph becomes a *research notebook* rather than a *forest*.
- **Side-effect** — the mutation loop's *intended* output is better strategies; its *side-effect* is a corpus of dead-or-alive strategy code, eval data, and lineage. **If the side-effect were treated as the main effect, xvision would be: a published research dataset of LLM-driven trading experiments, where the *trading* is incidental.** That's potentially worth more than the trading itself — and may be the answer to "what does xvision sell?" (Run 3).

### What the chart + lifts together reveal

- **The lineage graph is load-bearing.** It catches 7/11 failure modes alone. Build it first, robustly. The autooptimizer-1 plan should treat the lineage-graph as a first-class output artifact, not telemetry.
- **The killed strategies are the underexploited asset.** Every domain that has lived through this loop says: keep the dead. xvision currently has no plan for storing eliminated strategies long-term.
- **Multi-objective evaluation almost certainly belongs in v1 of autooptimizer.** Single-scalar fitness invites reward hacking and mismatch. The eval engine spec already mentions Δ-Sharpe; pair it with at least one drawdown metric and one turnover metric.
- **Briefing-format randomization is a 1-day implementation that prevents an entire class of failures.** Add to autooptimizer plan.

#### Tuple footer

```
operators:        substitution · abstraction-lift
organon:          chart (11 failure modes × 5 detection columns) + 5 cross-domain mitigation lists
dim prompts:      side-effect · intentionality · hierarchicalness
not surfaced:     "what does the autooptimizer OPTIMIZE FOR?" — assumed Δ-Sharpe; the prior
                  question (which loss function?) deserves its own tuple. Also unsurfaced:
                  the politics of mutation — who has the right to delete a strategy? if the
                  marketplace creator's strategy gets killed by the autooptimizer, what's
                  the relationship-management story?
```

---

## Run 5 — Why would strategy creators bring work to xvision vs. Numerai/Composer/eToro?

### What creators of trading strategies actually want (master list)

The list, with each item rated for how well xvision delivers vs. the obvious comparators. Scale: ✓ delivers / ~ partial / ✗ doesn't deliver / ? unclear.

```
Creator wants                              Numerai  Composer  eToro  TradingView  HyperLiq  xvision
----------------------------------------   -------  --------  -----  -----------  --------  -------
1. Money — share of trading PnL            ✓ (NMR)  ~ (sub)   ✓      ✗            ~          ~ (license)
2. Money — predictable subscription        ✗        ✓         ~      ✗            ✗          ~ (planned)
3. Audience reach (existing user base)     ✓ huge   ✓ medium  ✓ huge ✓ huge       ✓ med     ✗ none
4. Reputation — portable across platforms  ✗        ✗         ✗      ✗            ✗          ✓ (ERC-8004)
5. Reputation — verifiable track record    ✓        ~         ✓      ~            ✓          ✓ (eval attest)
6. Low publishing friction                 ✓ (just  ✓         ~      ✓            ~          ✗ (Rust skill)
                                            submit)
7. Low support burden                      ✓ (no     ~ (some  ✗      ✓            ~          ?
                                            users)   support)
8. Edge interpretability                   ✗ (just  ~         ✓      ✓            ~          ✓ (slots
                                            scores)                                              visible)
9. IP protection                           ✓ (closed ✗        ✗      ✗            ✗          ~ (license
                                            scoring)                                            tokens
                                                                                                planned)
10. Anonymity / pseudonymity               ✓ huge   ~         ✗      ✓            ✓          ✓ (wallet)
11. Backtest infra (free or cheap)         ✓        ✓         ~      ✓            ✗          ✓ (eval engine)
12. Composability — build on others        ✗        ~         ✗      ~ (script    ✗          ✓ (slot
                                                                       sharing)                  architecture)
13. Mutation / improvement loop            ✗        ✗         ✗      ✗            ✗          ✓ (auto-
                                                                                                researcher)
14. Anti-extraction (no rug-pull risk)     ✓ (token ✓ (open   ?      ✓ (free)     ?          ✓ (open
                                            economy) source)                                    source)
15. Liquidity for trades                   N/A      ~ (eq)    ✓ (eq) N/A          ✓✓ (perp)  ✓ (perp via
                                                                                                Orderly)
16. Regulatory clarity                     ✓ (well- ~         ✓      N/A          ?          ?
                                            structured)
17. Multi-asset universe                   ✓ wide   ✓ wide    ✓ wide ✓ wide       ✗ BTC/ETH  ✗ BTC only
                                                                                                in v1
18. Easy strategy testing before public    ✓        ✓         ✗      ✓            ✗          ✓ (paper
                                                                                                eval)
```

**Reading the matrix:** xvision's distinctive ✓-only column is **#4 (portable reputation), #12 (composability), #13 (mutation loop)**. Two competitors win on #3 (audience) and #16 (regulatory) — both of which xvision has no near-term path to win.

### Lifted shape (so the comparison generalizes beyond crypto)

Strip the surface:

> *A two-sided platform where producers of intellectual artifacts (strategies, songs, articles, code, predictions) face consumers of those artifacts (deployers of capital, listeners, readers, users), with the platform providing trust, discovery, infrastructure, and settlement, and taking a cut.*

Pattern recognized in:

- App Store / Play Store (developers × users; 30% take; massive reach)
- Spotify / Apple Music (artists × listeners; royalties; reach justifies pain)
- Substack (writers × readers; 10% take; differentiation = creator-first UX)
- Patreon / OnlyFans (creators × patrons; 5-20% take; differentiation = direct relationship)
- AngelList (founders × investors; differentiation = regulatory wrapper)
- YouTube / TikTok (creators × viewers; differentiation = recommendation engine + ad reach)
- Lloyd's syndicates (Names × risks; differentiation = risk-pooling + underwriting expertise)
- Numerai itself (data scientists × hedge fund; differentiation = cryptographic anonymity + token incentive)

### Combination — creator wants × platform's strongest pull

Cross "what creators want" (the master list) with "what's actually a moat" for the platform. Each composite is a candidate positioning for xvision.

#### Reach × portability (xvision is currently weak on reach, strong on portability)
- **Composite:** "build your reputation here; carry it anywhere." If a creator builds an ERC-8004-attested track record on xvision, that record is theirs forever — they can take it to a CEX, a fund, another DEX. **This is the only positioning where being early on xvision is *better* than being later somewhere bigger.** Lean in.

#### Mutation loop × composability
- **Composite:** "submit a strategy seed; the autooptimizer evolves it; you keep ownership." Most creators don't have an autooptimizer. Numerai trains models on user submissions — but the submitter doesn't get the trained model back. xvision could let creators *retain ownership* of all evolved descendants of their seed. That's a unique value prop.

#### Backtest infra × edge interpretability
- **Composite:** "free backtest of your hypothesis, with full audit log of why every trade fired." Other platforms either give you the score (Numerai) or the trade (eToro). xvision can give you the *reasoning chain* (intern briefing → trader decision → risk eval). For a quant evaluating their own edge, this is gold.

#### IP protection × anti-extraction
- **Composite:** "open-source the framework, soulbound-license the strategy." Creators worry that publishing a strategy makes it instantly copyable. The smart-contract-surface spec already handles this (ERC-1155 soulbound by default). **This needs to be loud in the marketing** — most creators don't realize soulbound exists.

### What competitors do that xvision should explicitly NOT try to match

- **Numerai's tokenomic incentive:** xvision doesn't have a token (yet), and shouldn't. The reputation NFT plus marketplace USDC is enough; adding a token adds regulatory and volatility friction.
- **eToro's massive consumer base:** xvision is a developer / quant tool. Competing for retail consumer eyeballs is a losing fight against billion-dollar marketing budgets.
- **Composer's no-code editor:** xvision is intentionally Rust-heavy. The wizard archetype in Plan 2d is the closest analog; it should be promoted as "AI-assisted authoring" rather than "no-code."
- **TradingView's free script-share:** xvision's strategies have on-chain identity and licensing; making them free is incompatible with the marketplace.

### What competitors do that xvision should LEARN from

- **Spotify's playlist economy:** the most successful artists on Spotify are those who land on big playlists. xvision analog: curated *strategy bundles* (e.g., "BTC Funding-Fader Pack") could be marketplace-level products, not just individual strategies. A strategy creator earns when their strategy is included in a popular bundle.
- **Substack's creator-first UX:** the writer's tools matter. xvision's strategy authoring (Plan 2a + 2d Wizard) needs to be obviously better than the alternatives, or reach beats craft every time.
- **AngelList's regulatory wrapper:** xvision should research whether the marketplace is operating as a securities exchange under any jurisdiction. The non-custodial design helps; explicit regulatory positioning would help more.
- **Numerai's pseudonymity:** strategy creators should be able to publish under wallet pseudonyms, not real names. This is essentially free given the wallet-NFT model — the spec just needs to never demand real identity.

### Dimensions surfaced

- **Longevity** — how long do creator earnings last per strategy?
  - License sale: one-shot per buyer (short).
  - Subscription: as long as buyer subscribes (medium).
  - Performance fee: as long as the strategy makes money (long, but volatile).
  - Reputation NFT value: lasts forever (longest, but indirect).
  - **xvision earnings asymmetry:** the on-chain pieces (NFT, attestations) are the most-durable; the marketplace cash flows are the most-immediate. Both matter but for different creator profiles.

- **Materiality** — what does the creator *actually have* after publishing?
  - Numerai: a leaderboard rank + NMR tokens (informational).
  - Composer: published page + recurring fees (informational + cash).
  - eToro: follower count + recurring fees (informational + cash).
  - **xvision: an NFT, an audit trail, a license token economy, and a public lineage graph** (informational, on-chain, plus cash). The most "thing-like" of any competitor.

- **Decomposability** — can a strategy be broken into reusable pieces?
  - All competitors except xvision ship monolithic strategies.
  - **xvision's slot architecture (intern / trader / risk) is uniquely decomposable** — a creator could ship just an intern slot, or just a trader slot, and earn royalties from every strategy that composes it. **This is a marketplace innovation no competitor has — and it's currently underexploited in the spec.**

### What the matrix + lifts together reveal — the positioning

xvision's *defensible* creator pitch is **NOT** "we have the most users" (we don't), or "we pay the most" (we can't promise), or "we're the easiest" (we're Rust). It's:

> **"Bring your edge here. We'll evolve it, attest it, package it, and you keep the lineage NFT forever."**

Reach is built second. Differentiation is built first.

#### Tuple footer

```
operators:        combination · abstraction-lift
organon:          list (18 creator-wants × 6 platforms; 4 cross-domain comparators)
dim prompts:      longevity · materiality · decomposability
not surfaced:     "why DON'T creators come?" — friction-side analysis. Rust requirement,
                  Mantle-only deployment, Orderly account requirement, no canonical UI yet.
                  Each is a hard onboarding step that may dwarf any positioning advantage.
                  Worth its own adversarial tuple.
```

---

## Run 6 — Is on-chain reputation (ERC-8004) actually valuable, or a checkbox?

### Spectrum: economic weight of reputation systems

Continuous axis from "pure vanity" to "fully slashable stake." The spectrum runs through every reputation system humanity has built. Where each existing system sits:

```
  0%                         50%                            100%
  |   pure vanity            gating                         fully slashable
  |                                                         (bond at risk)
  |   ^                      ^                              ^
  |   | Klout                | FICO < 600 = no loan         | Eigenlayer restake
  |   | Reddit karma         | Uber driver < 4.6 = no rides | Lloyd's syndicate Names
  |   | Twitter followers    | eBay seller stars affect     | bonded service contracts
  |                            search rank                  | bail bonds
  |                                                         |
  |        ^                              ^                            ^
  |        | GitHub stars                 | Stripe risk score          | margin call on
  |        | (affect hiring)              | (gates merchant accept)    | leveraged position
  |        |                              |                            |
  |       ~3%                           ~30%                          ~95%
  |
  |        v                                                            v
  |   ERC-8004 today                                              ERC-8004 if it ever
  |   (visible, queryable,                                         gates withdrawals,
  |   no $ at risk, no                                             collateralized
  |   gating)                                                      strategies, etc.
  |
  band:  ~~~~~~~~~~~~~~ ERC-8004 *potential* range ~~~~~~~~~~~~~~
         (current position is the leftmost edge of this band)
```

**ERC-8004 today sits at ~3%.** It's a signal — visible, queryable, immutable — but no economic decision in xvision currently *gates on it*. A user with 10 USDC can deposit to a strategy with reputation 0.0; a marketplace listing with reputation 5.0 doesn't cost more than one with reputation 0.5; nothing is at risk if the reputation falls.

**The realistic upper bound is ~50%, not 100%.** xvision is non-custodial; full slashing requires custody. But there are ways to push the dial right.

### Negation — siblings of "valuable on-chain reputation"

Each definitional property of "valuable on-chain reputation," negated, names a sibling that already exists somewhere:

```
Property negated                Sibling reputation system            Where it exists today
-----------------------------   ----------------------------------   ---------------------------
Portable                        Walled-garden reputation             Uber rating, Stripe Atlas,
                                                                     LinkedIn endorsements
Verifiable by third parties     Opaque "trust me" reputation         Web2 trust badges, BBB ratings
Tamper-resistant                Centralized + alterable              Amazon/eBay stars (platform
                                                                     can remove)
Machine-queryable               Human-readable only                  Recommendation letters,
                                                                     references
Carries economic weight         Pure-vanity reputation               Klout, Reddit karma, social-
                                                                     media followers
Accumulates over time           Instant snapshot reputation          One-shot proof-of-X badges
Slashable on bad behavior       Permanent reputation                 Academic degrees, lifetime
                                                                     achievement awards
Per-agent                       Per-platform reputation              GitHub: rep within github;
                                                                     resets at job change
Single-dimension                Multi-dimensional reputation         FICO is one number; FICO+
                                                                     CRA is many numbers
```

**The negation set tells you that ERC-8004's distinctive value isn't any single property — every property has a sibling system that's worked fine without it.** What's distinctive is the *combination* of portable + verifiable + tamper-resistant + machine-queryable. **That combination has never existed in scale before.** This is a real claim, not a checkbox — but it requires that the combination is *load-bearing* for some user decision.

### Dimensions of "value" identified

#### Dimension 1 — Homogeneity (one number vs many)

A reputation can be one scalar (FICO 712) or a vector (Sharpe, drawdown, consistency, style, volume, asset universe, regime fit). The vector form is more honest but harder to gate on.

- **xvision today:** the ERC-8004 reputation registry per spec is a feedback list with optional tags — closer to vector. Good.
- **Risk:** the marketplace UI may collapse this into one star-rating-like number, losing the dimensional information that makes reputation portable across contexts.
- **Move:** publish reputation as a structured vector (Sharpe, max-drawdown, days-in-market, etc.), let consumers compose their own scalar from the vector. This is the AngelList "founder fit" approach vs the FICO approach.

#### Dimension 2 — Predictability (deterministic accrual vs platform-discretion)

Does reputation accrue automatically from observable behavior, or does the platform decide what counts?

- **xvision today:** spec says reputation is written via the ERC-8004 ReputationRegistry per-run by xvision itself. **xvision is the only writer.** That's a centralization point.
- **Risk:** if xvision is the only attestor, "on-chain" gives portability but not censorship-resistance — xvision can decline to attest a run.
- **Move:** allow third-party attestors (other vault operators, independent eval engines) to write reputation events for the same agent. The agent's reputation becomes the *aggregate of attestations*, not just xvision's view. This is what the AT Protocol does with social signals.

#### Dimension 3 — Scope (universal vs context-bound)

Is this reputation about "trader skill" in general, or "skill in BTC perp markets in low-vol regimes"?

- **xvision today:** reputation is per-agent. Agents are specialized to specific assets/regimes (the agent_id encodes the config). So reputation IS context-bound, but the *consumers* of reputation may treat it as universal ("look, 80th percentile Sharpe!" — but only on BTC, only in a bull market).
- **Risk:** out-of-context reputation use causes blowups (the BTC funding-fader strategy is rated 5/5; a user deploys it during a crab market; it loses).
- **Move:** every reputation attestation must include the regime/scope it was earned in. Marketplace listings must surface this prominently. **The spec hints at this via "regime fit" in the bundle manifest** — verify it's actually rendered to buyers.

### Where ERC-8004 has REAL teeth (the 25-50% range)

If xvision wants reputation to do more than be a checkbox:

1. **Gate on minimum reputation for marketplace listings.** A strategy with reputation < threshold can't be sold. Forces creators to backtest before publishing.
2. **Tier marketplace fees by reputation.** Top-decile reputation gets a 97/3 split instead of 95/5. Bottom-decile pays 90/10. **Rewards quality, finances bad-actor handling.**
3. **Reputation-weighted quota allocation.** When a user runs many strategies, xvision's quota_factor (wallet spec §3.4) could be weighted by reputation: high-rep strategies get more of the cap, low-rep less. **This is the cleanest integration with the wallet plan.**
4. **Reputation-required for autooptimizer seeding.** Strategies entering the autooptimizer's mutation pool must have minimum reputation. Prevents the loop from polluting itself.
5. **Reputation as collateral.** A creator stakes their NFT as collateral against future losses; if their strategy loses > X%, the NFT is slashed (transferred to a community pool, locked, etc.). **This pushes reputation to the 95% end of the spectrum.** Hard to design right — slashing rules are notoriously gameable — but real teeth.

None of these are in the current spec. **Without at least 2-3 of them, ERC-8004 in xvision is at the 3% checkbox end of the spectrum.**

### The "checkbox" risk in detail

If xvision ships ERC-8004 without economic teeth:
- Users won't read it (no decision depends on it).
- Creators won't optimize for it (no payment depends on it).
- Other platforms won't import it (xvision is the only attestor; nobody else trusts it).
- It becomes a marketing line ("we use ERC-8004!") with zero behavioral effect.

This is what most "Web3 reputation" projects have looked like to date. Avoiding it requires *gating something on the reputation* — making the reputation answer a question some user, somewhere, has to ask.

### What xvision should ship to push past checkbox status (priority order)

```
Priority   Feature                                                  Effort     Spectrum movement
---------  -------------------------------------------------------  --------   -----------------
P0         Reputation-weighted quota in wallet engine               2 days     3% → 15%
P1         Marketplace listing minimum reputation gate              1 day      15% → 25%
P2         Multi-attestor reputation (third-party writes)           1 week     25% → 35% +
                                                                                portability
P3         Tiered marketplace fees by reputation decile             3 days     35% → 45%
P4         Reputation-as-collateral / slashing                      4+ weeks   45% → 70%+
                                                                                (changes the
                                                                                product
                                                                                significantly)
```

P0 is essentially free (one config link from `quota_factor` to a reputation read). It would be a meaningful win for the same engineering effort as a typo fix. **P0 should be added to the wallet plan or the autooptimizer plan immediately.**

#### Tuple footer

```
operators:        negation · dimension-identification
organon:          spectrum (0% to 100% economic-weight axis, 6 named anchors, ERC-8004
                  band marked at current 3% position with realistic upper-bound at 50%)
dim prompts:      homogeneity · predictability · scope
not surfaced:     "reputation as a story" — there's a narrative dimension that all three
                  identified dimensions miss. A reputation is not just a number or vector
                  — it's a *story about how the numbers were earned*. The audit log is
                  the substrate; the marketplace UI tells the story. Worth its own tuple
                  on UX of trust narratives.
```

---

## Run 7 — What's the worst-case "first big loss" story?

### Lattice of failure scenarios × response domains

Top of the lattice = "trust-bearing platform crisis." Each scenario participates in multiple parent categories simultaneously (cause, response, blast-radius, recoverability).

```
                                 Trust-bearing platform crisis
                               /       |        |        |       \
                       (cause)   (visibility)  (blast)  (timing)  (response-required)
                       /  |  \      |  \       /  \    /  \         /  |  \
              operational  strategic adversarial          transparency compensation
                |          |        |                          |         reform
                |          |        |
            Scenarios populate the lattice; many sit at multiple intersections:

S1  AutoOptimizer-mutated strategy loses 50% on one trade
    cause: strategic + operational  | visibility: high  | blast: per-user (bounded by hard cap)
    | recoverability: low (creator + platform credibility) | requires: transparency + reform

S2  Cross-margin contagion: one strategy liquidates the whole account
    cause: strategic                | visibility: high  | blast: per-user TOTAL
    | recoverability: low           | requires: transparency + reform + maybe comp

S3  High-rep marketplace strategy loses money for many users at once
    cause: strategic                | visibility: very high | blast: many users
    | recoverability: very low      | requires: transparency + reform + reputation rebuild

S4  Trading-key compromise (xvision server breach)
    cause: adversarial + operational | visibility: high  | blast: per-user (bounded by caps)
    | recoverability: medium (caps held; reputation hit) | requires: transparency + reform

S5  Platform bug: dispatcher submits N duplicate orders
    cause: operational              | visibility: medium | blast: per-user
    | recoverability: high          | requires: transparency + comp

S6  Orderly outage during high volatility, can't close
    cause: external                 | visibility: high  | blast: many users
    | recoverability: medium (xvision not at fault) | requires: transparency

S7  LLM provider outage / model regression mid-session
    cause: external                 | visibility: low   | blast: per-user
    | recoverability: high          | requires: minimal disclosure

S8  Settlement wallet compromised
    cause: adversarial              | visibility: medium | blast: NONE on users (non-custodial)
    | recoverability: high          | requires: minimal disclosure (operator pain only)

S9  Creator publishes Ponzi-like strategy: builds rep then exit-scams
    cause: adversarial              | visibility: very high | blast: many users
    | recoverability: low           | requires: marketplace governance overhaul

S10 Regulatory action: jx-specific account freeze
    cause: external                 | visibility: very high | blast: jx-specific users
    | recoverability: variable      | requires: legal + transparency

S11 Mass mutation drift: autooptimizer converges on an exploit briefing format
    cause: strategic + operational  | visibility: medium | blast: many strategies
    | recoverability: medium        | requires: kill-switch + reform of mutation loop

S12 Reputation gaming: collusion ring rates each other up
    cause: adversarial              | visibility: low when discovered, high when exposed
    | blast: marketplace integrity  | recoverability: low | requires: governance + audit
```

### Cross-domain re-instantiations of the response playbook

The lattice maps the structure; the response-playbook comes from domains that already lived through their first-big-loss event.

#### Tylenol cyanide poisonings (1982) — the gold-standard transparency response
- **Story:** Someone laced Tylenol capsules with cyanide; 7 dead.
- **J&J response:** voluntary recall of all Tylenol nationwide ($100M cost), full press transparency, pioneered tamper-evident packaging. Brand recovered fully within 18 months.
- **xvision parallel for S1/S2/S5:** when a strategy or platform bug causes a loss, the response should be (a) immediate full disclosure with the audit-log evidence, (b) voluntary action that exceeds what's strictly required, (c) ship a structural change (tamper-evident packaging analog: aggregate margin guard, additional kill-switch, mandatory simulation).
- **Lesson:** the *cost* of overreaction is finite; the *cost* of being seen to underreact is infinite.

#### Lloyd's "London Spiral" (1980s) — the un-survivable response
- **Story:** Lloyd's syndicate Names took on hidden interconnected reinsurance exposure; asbestos and pollution claims compounded; many Names lost their houses.
- **Lloyd's response:** initially obscured the depth of the problem, dragged out compensation. Litigation lasted 15+ years. Lloyd's brand permanently degraded as "not what your grandfather's Lloyd's was."
- **xvision parallel for S2/S3/S11:** if cross-margin contagion or autooptimizer convergence creates *interconnected hidden exposures*, the failure mode is exactly Lloyd's — one bad strategy reveals the whole house was exposed. The response of choice is the OPPOSITE of Lloyd's: name the exposure publicly *before* it triggers.
- **Lesson:** hidden interconnections compound; un-hide them as the platform's daily discipline.

#### FTX collapse (2022) — the catastrophic comingling
- **Story:** Customer funds were comingled with the trading firm's positions. When the trading firm lost, customer funds were gone.
- **FTX response:** denial → cover-up → bankruptcy → criminal conviction.
- **xvision parallel for S8:** the non-custodial design *prevents this scenario by construction*. Even if the platform commits operational fraud at the platform level, user trading capital is unreachable. **This is the single biggest design win xvision has and should be loud about.**
- **Lesson:** the architecture itself is the strongest possible response — design out the failure modes that other platforms had to apologize for.

#### Boeing 737 MAX (2018-2019) — the slow strategic-error reveal
- **Story:** MCAS system was added to compensate for engine placement; pilots weren't trained on it; two crashes killed 346.
- **Boeing response:** initial deflection ("pilot error") → grounded fleet → multi-year fix → loss of trust still reverberating.
- **xvision parallel for S1/S11:** when the autooptimizer produces a strategy that wins eval but fails live, the temptation is to blame "pilot error" (the user picked a bad strategy). The right response is to acknowledge the system-level failure (the eval didn't catch it) and structurally reform.
- **Lesson:** if the system was designed to abstract away a complexity, the system owns the failures arising from that abstraction.

#### Knight Capital (2012) — the operational-error 30-minute extinction
- **Story:** Bad deployment caused a trading algo to spam orders; lost $440M in 30 minutes; the firm folded.
- **Knight response:** there wasn't time. The market killed Knight before any response.
- **xvision parallel for S5/S11:** kill-switch latency MATTERS. The wallet plan's `xvn kill --all` must take effect within seconds, not minutes. The Knight precedent says: if you can't kill in under a minute, you might not have a company anymore.
- **Lesson:** the kill-switch is the most-important code in the system; it's the line of defense that limits all other failures' blast radius.

#### Equifax breach (2017) — the personal-data disclosure delay
- **Story:** 147M users' data leaked; Equifax delayed disclosure by 6 weeks while executives sold stock.
- **Equifax response:** widely seen as guilty; CEO fired; ongoing litigation; brand permanently degraded.
- **xvision parallel for S4/S12:** the platform must have a *publicly committed disclosure SLA* — "any incident affecting users is disclosed within X hours." The wallet plan's audit log makes this technically possible; the policy commitment makes it credible.
- **Lesson:** the *time-to-disclosure* is itself a public number; commit to it before you need it.

### Substitution across dimensions — what changes the survivability?

#### Visibility (low → high)

- **S7 (LLM regression, low visibility):** survivable with minimal disclosure; users barely notice.
- **S5 (duplicate orders, medium visibility):** survivable with transparency + comp; one news cycle.
- **S2 (cross-margin contagion, high visibility):** existential if narrative coheres ("xvision liquidated my account"); the spec's mitigation MUST be visible *before* the event, not afterwards.
- **Move:** publish the cross-margin / aggregate-margin guard story prominently in marketing materials *before* the first incident. Pre-positioning a defense.

#### Cyclicity (one-shot → periodic)

- **One-shot loss (single trade blowup):** trust-rebuildable; users accept that one bad day happens.
- **Periodic small losses (every week, slight underperformance):** users tolerate if benchmarked appropriately.
- **Continuous degradation (slow drift):** users barely notice but the long-tail effect is brand erosion.
- **The dangerous combination:** *one-shot loss after a long stretch of looking-good-but-actually-drifting*. This is the autooptimizer-mode-collapse scenario (Run 4). Looks fine for months; then the regime changes; everything blows up at once because everything was correlated.
- **Move:** correlation-of-failure is itself a metric; track it; surface it; accept lower aggregate alpha for lower correlation.

#### Age (era-substitution)

- **xvision in 2010** (pre-DeFi, pre-LLM): impossible to build.
- **xvision in 2025** (now): possible, novel, no incumbent moats.
- **xvision in 2030** (after the next crypto regulatory cycle): legacy; the regulatory wrapper will be the moat.
- **Move:** the era-substitution suggests xvision's window for "novel" is short. The window for "legacy with regulatory clarity" is longer but requires deliberate compliance work now. **Spec a separate working stream on regulatory positioning.**

### What the lattice + cross-domain analysis produce

#### The five scenarios that would end the project

S2 (cross-margin contagion), S3 (high-rep strategy mass loss), S9 (creator exit-scam), S11 (autooptimizer convergence on exploit), S12 (reputation gaming collusion). Each requires *pre-event structural defense*, not post-event apology.

#### The four scenarios that are survivable with transparency

S1, S4, S5, S6 — single-incident, bounded blast, audit log evidence available. The non-custodial design + audit log + kill switches handle these well IF the disclosure SLA is committed and met.

#### The two scenarios xvision ALREADY DESIGNED OUT

S8 (settlement wallet compromise — non-custodial), S5 partial (idempotent client_order_id prevents most duplicate-submission damage). Make these explicit in marketing.

#### What the wallet plan needs to add

1. **Public disclosure SLA** — commitment to disclose any user-affecting incident within X hours. Add to MANUAL.md.
2. **Aggregate margin guard pre-positioning** — communicate the cross-margin defense in marketing before the first incident, not after.
3. **Kill-switch latency benchmark** — measure end-to-end time from `xvn kill --all` invocation to dispatcher rejection of new orders. Target < 5 seconds. Test in CI.
4. **Reputation-collusion detection** — the marketplace needs a Sybil-detection layer at launch, not added after the first collusion ring is exposed.
5. **Mass-loss simulation drill** — run a tabletop exercise: "how would we respond if S3 happened tomorrow?" Document the playbook in MANUAL.md *before* it happens.

#### Tuple footer

```
operators:        cross-domain-reinstantiation · substitution
organon:          lattice (12 failure scenarios at intersection of cause/visibility/blast/
                  recoverability/response axes) + 6 cross-domain response-playbooks
dim prompts:      age · visibility · cyclicity
not surfaced:     "the *positive* first big event" — what's the failure scenario's mirror?
                  The first time a user makes a fortune. How does the platform handle
                  *windfall*? It's a different class of stress test (FOMO, copy-strategy
                  spam, regulatory attention to "win"). Worth its own tuple.
```

---

## Run 8 — What's the operator's daily job?

### Daily timeline (current state — operator-as-user, ~1 person, ~10 strategies)

```
05:30  US futures wake; overnight Asian session reviewed
       - check audit log: any pending_approvals, halts, anomalous decision counts
       - check ledger: overnight P&L vs expectation, drawdown alerts
       - check Orderly account state vs xvision ledger (manual reconciliation)
       - check settlement wallet balance (if commissions accrued overnight)

08:00  Pre-US-open windowing
       - review which strategies are scheduled to fire today
       - spot-check budget caps; any strategies near hard cap?
       - test the kill switch (xvn kill --strategy <test-id> + unhalt) — "smoke test"

09:30  US open; live trading window begins
       - dispatcher activity should spike; tail audit log
       - watch for: simulation-rejection rate, frequency-cap hits, approval requests

11:00  First lull; non-trading work
       - review autooptimizer overnight runs (mutated strategies, eval results)
       - decide which mutations to promote / retire / archive
       - commit policy_changes for any budget edits

13:00  US lunch; quietest period
       - dev work on next plan task
       - read ERC-8004 reputation events for own agents
       - respond to any marketplace messages (eventually)

15:30  US power hour
       - dispatcher activity spikes again
       - any approvals required → respond within TTL (60s default)

16:00  US close
       - end-of-session reconciliation (xvn reconcile --user operator)
       - commit any audit log forensics on weird events
       - update operator runbook with anything new learned

20:00  Asian session opens; system runs unattended
       - kill switch armed
       - wake on alert (pager / push notification)

23:00  Sleep
```

This rhythm is currently 80% manual / 20% automated. Each phase has multiple cliff-points where missing the action causes a real loss.

### Weekly cadence

```
Mon  - reset weekly counters; review prior-week PnL by strategy
     - run xvn budget show, snapshot to weekly-perf folder
     - sweep settlement wallet → operator wallet

Tue  - eval engine batch run on prior week's decisions
     - publish weekly performance report (informally to Twitter / Telegram)

Wed  - autooptimizer mutation generation review (deeper than daily)
     - decide on any architecture-level changes (slot swaps, prompt rewrites)

Thu  - marketplace activity review (if any sales / new listings)
     - update reputation NFTs that hit milestone events

Fri  - end-of-week recon: full ledger vs Orderly diff
     - tax-event tracking (if applicable in jx)
     - plan next week's experiments

Sat  - rest day; only critical alerts
Sun  - archive day; no live trading; backup DB; review autooptimizer
```

### Monthly cadence

- Run forensic audit on any incidents (the audit log is the source-of-truth)
- Reputation report: which agents accrued reputation events; which lost it
- LLM cost audit (the autooptimizer dominates this; budget caps live here)
- Regulatory landscape check (sanctions list updates, new jx restrictions)
- Marketplace governance: any creator disputes, fraud signals, listings to moderate

### Quarterly + yearly

- Strategic strategy retirement (anything with rep < threshold for > 60d)
- Major model migrations (Claude X.Y → X.Z); re-eval all strategies post-migration
- Infrastructure cost review (Mantle gas, Orderly fees, RPC, LLM, hosting)
- Tax filings
- Yearly: protocol upgrades (ERC-8004 spec changes; Orderly API changes)

### Lifted shape — what kind of work IS this?

> *Human supervisor of a partially-autonomous system that runs continuously, with periodic interventions, intermittent crises, and slowly-accumulating maintenance debt.*

Cross-domain comparators:

```
Domain                    Daily activities                      Hardest part
------------------------- ------------------------------------- -------------------------
SRE / DevOps              monitor dashboards, respond to        on-call interrupting sleep;
                          alerts, deploy fixes                  drift detection vs noise
Algo trader (prop shop)   monitor algo P&L, manual override,    knowing when to override
                          tune parameters                       the algo
Pilot (autopilot era)     fly takeoff/landing manually; manage  staying engaged during long
                          autopilot in cruise                   automated cruise phases
Power plant operator      monitor instruments, respond to       extremely rare crises require
                          alarms                                instant correct response
Hospital night nurse      check patients, administer scheduled  triaging multiple alarms;
                          care, respond to alarms               recognizing patterns
Beekeeper                 weekly inspections, swarm management, seasonal planning, pest
                          honey extraction                      detection
Lighthouse keeper         maintain the light, log conditions,   isolation; the rare ship
                          wait                                  whose life depends on you
Greenhouse operator       monitor temp/humidity/water, trim,    catastrophic failure mode
                          harvest                               (one bad night = whole crop)
```

**Pattern recognized:** xvision's operator role most closely resembles the *algo trader at a prop shop* hybridized with *SRE on-call*. Both involve continuous attention with discrete interventions, where the cost of a missed alert can be ruinous.

### Lessons each domain has worked out

- **From SRE:** rotate the on-call burden among multiple operators; never let one person be permanently on-call. **xvision implication:** a 1-operator deployment is fragile. Multi-tenant means multi-operator; even the single-operator hackathon version should plan a fallback contact.
- **From algo trading:** keep a "trading journal" — a daily written record of decisions and their outcomes. **xvision implication:** the operator runbook in MANUAL.md should evolve into a journaled artifact, not just instructions.
- **From pilots:** "automation surprise" — when the autopilot does something unexpected, the human needs *immediate clarity* on what mode it's in. **xvision implication:** the dashboard MUST surface "what is each strategy doing right now" in one glance. The Phase 8 spreadsheet covers this.
- **From hospital nurses:** alarm fatigue — too many alerts and operators stop noticing real ones. **xvision implication:** alert thresholds need careful tuning; default to fewer-but-louder rather than many-quiet.
- **From beekeepers:** seasonal awareness — there are calendar events that require pre-positioning. **xvision implication:** Fed days, options expiry, halving events, exchange maintenance windows — these are predictable and the operator should pre-position quota / kill-switches around them.
- **From lighthouse keepers:** isolation is the hardest part. **xvision implication:** the operator role can be lonely; building a community of operators (Discord? Telegram?) is itself part of the platform.

### Substitute size — how the role transforms

```
User count     Operator role                        Daily hours    Critical skills
------------   ----------------------------------   ------------   ------------------------
1              user-as-operator (current)            1-3 hr         dev, trading, analysis
10             host + operator                       3-6 hr         + customer support
100            small-team ops (1-2 ppl)              full-time      + SRE practices
1,000          dedicated ops team                    24/7 rotation  + escalation runbooks,
                                                                    on-call rotations
10,000         regulated entity                      24/7 + comply  + compliance officer,
                                                                    legal, AML/KYC
100,000        institutional-grade                   global team    + risk team, BSO
```

**The architecture must scale beyond what the operator alone can do.** Critical thresholds:
- ~10 users: kill-switch latency must be sub-second; manual reconciliation no longer viable
- ~100 users: need automated incident response runbooks, on-call rotation
- ~1,000 users: dispatcher must be high-availability (single-server is a liability)
- ~10,000 users: must operate as a regulated entity in at least some jurisdictions

**The wallet plan's "single-user" simplification implicitly bets that growth past 10 users won't happen during the hackathon.** If it does, every shortcut becomes technical debt at the worst moment.

### Substitute naturalness — what does "no operator" look like?

- **Fully synthetic:** the operator role is itself an LLM agent. xvision already has the building blocks (Claude can read the audit log, decide on kill switches, etc.). **A "Claude-as-on-call" agent is plausibly a v2 feature.** It would handle the 80% of operator tasks that are routine, escalating only the 20% that need human judgment.
- **Fully manual (no automation):** the system requires 1 operator-second per trade. Doesn't scale past 10 trades/day. Approximates "Karpathy doing it himself."
- **Current hybrid:** automation handles dispatch; human handles judgment. Probably the right balance for v1, but the autooptimizer could shift more decisions to the human (which mutations to keep) just as it removes others (no manual mutation).

### Substitute polarity — operator role's dark version

The artisan / pilot / skilled-manager framing is positive. The dark version:

- 24/7 on-call burden that prevents vacation
- Personal liability for losses (real in non-custodial UX too — operator's reputation is at stake)
- Constant context-switch between deep dev work and shallow monitoring
- Risk-averse decision-making under fatigue (worst combination)
- Loneliness (the Lighthouse-keeper failure mode)

**The dark-version is not hypothetical** — it's where the operator lives during incidents. Designing the system to *minimize* operator burden (better defaults, tighter alerts, fewer manual decisions) is itself a load-bearing feature.

### What the operator timeline + substitutions reveal

#### Three things the wallet plan should add for operator life-quality

1. **End-of-day reconciliation script** — `xvn eod` that runs reconcile, writes a daily report, surfaces anything anomalous. Saves 30 min/day at minimum.
2. **Pre-positioning command** — `xvn prepare-event --type fomc --time "2026-06-15 14:00 ET" --action halt-all-then-resume` — schedule kill-switches for known volatility events. Beekeeper-style seasonal awareness.
3. **Operator dashboard widget** — a single-screen view of "what's running, what's halted, what's pending approval, what's drifting" — the cockpit. Phase 8 spreadsheet is a piece of this; the full cockpit is broader.

#### What scales after hackathon

- Multi-operator on-call rotation (P1 post-hackathon)
- LLM-assisted ops agent (P2 post-hackathon)
- Public operator runbook / playbook (P0 — needed before first incident)

#### Tuple footer

```
operators:        substitution · abstraction-lift
organon:          timeline (daily / weekly / monthly / quarterly / yearly cadences) +
                  cross-domain comparator table (8 supervisor-of-autonomy archetypes)
dim prompts:      size · naturalness · polarity
not surfaced:     "operator as platform feature" — the operator's labor is part of what users
                  buy. Should the platform expose operator-specific UX (e.g., "this strategy
                  is operated by edkennedy"), or be operator-anonymous? Worth its own tuple
                  on whether the operator is the brand.
```

---

## Run 9 — What's the irreducible v0 of this whole system?

### Periodic grid: component × tier

```
                       v-1 (scaffold)      v0 (irreducible MVP)         v1 (hackathon)            v2 (post-hack)              v3 (production)
                       ------------------- ---------------------------- ------------------------- --------------------------- ----------------------------
Strategy authoring     hand-edit Rust code TOML configs in repo         Wizard (Plan 2d)          MCP authoring (Plan 2a)     no-code visual builder
                       (current)
Slot architecture      mock LLM trait      Anthropic LLM dispatch       multi-provider dispatch   speculative execution       online learning slot
                       (current)
Risk engine            global rules        + per-strategy hard caps     + scoped permissions      + dynamic quota             + on-chain accounting
                       (current)                                          + reservations             + aggregate margin guard    contract for slashing
Strategy execution     Alpaca paper        + Orderly perp (live, BTC)   + multi-asset              + multi-DEX                 + multi-chain
(broker)               (current)
Pre-trade simulation   none                none                         orderbook simulation       VaR / scenario stress       market-impact-aware
                       (current)                                                                                                slippage modeling
Audit log              tracing logs        SQLite append-only +         + signed payload hash      + content-hashed merkle    + cryptographic timestamping
                       (current)            content-hashed                                            chain                       (RFC 3161)
Wallet                 single env-var      single trading-only key      + per-user encrypted       + multi-user keys           + MPC / Safe + 4337
                       Orderly creds       AES-256-GCM at rest            (still single user)        with rotation               session keys
                       (current)
Kill switches          none                xvn kill --strategy/         + auto-trigger             + governance lock           + on-chain pause
                       (current)            user/all                       (consec losses,            (multi-sig kill)            authority
                                                                            sharpe floor)
Approval gate          none                operator-confirms-large      + dashboard approve        + delegation rules          + DAO-style approval
                       (current)            trades >threshold              UI                         (operator delegates       quorum
                                                                                                       per-strategy)
Emergency close        none                xvn emergency-close          + browser-button kill     + automated trigger          + customer-side direct
                       (current)            (cancel + market-flat)         in dashboard               on regime change           emergency-close UI
Reconciliation         manual              periodic 15-min job          + drift alerts             + autonomous correction     + multi-attestor agreement
                       (current)
Eval engine            none                Δ-Sharpe + bootstrap CI      + walk-forward             + multi-objective           + adversarial backtest
                       (current)            (Plan 3 partial)               (held-out window)          Pareto fronts               (regime perturbation)
Lineage / mutation     none                none                         autooptimizer-1           autooptimizer-2/3          self-modifying
(autooptimizer)       (current)                                          (mutator+lineage)          (cycle, judge, evals)       meta-mutator
Marketplace            spec only           none in code                 listing + license sale     + bundle products            + secondary market
                       (current)                                          (Plan 5 partial)            (Spotify-playlist)         + lending against NFTs
Reputation             ERC-8004 spec       NFT mint per agent SLF3      + reputation feedback      + multi-attestor reads      + reputation-weighted quota,
(ERC-8004)             registries refs       (deploy on Sepolia)           events writing              third-party indexers       slashing
                       (current)
Settlement / fees      no contract         marketplace fee router       + fee streaming            + creator royalty splits    + on-chain perf-fee
                                            (atomic 95/5)                                              (curators, referrers)       contract
Dashboard              CLI only            CLI only                     /budgets + Wizard +        + Inspector + Marketplace   + mobile, multi-language
                       (current)                                          Live cockpit (Plan 2d)     UI
Operator runbook       README              MANUAL.md (current)          + cli-reference.md         + incident playbook         + trained on-call team
Documentation          archcm.md +         + spec docs                  + plan-by-plan readmes     + customer-facing docs      + tutorial videos
                       implementation-
                       plan.md (current)
Compliance             none (current)      basic ToS                    + privacy policy           + jx-specific exclusions    + jx-by-jx licensing
                                                                          + risk disclosure
Multi-tenancy          single env-var      single user                  single user (deferred)     multi-user keys             multi-region, sharded
                       (current)
```

#### Empty cells marked as predictions

Several cells in the grid are *expected* to be filled but are blank. Each is a coining opportunity:

- **Strategy authoring at v3 (no-code visual builder):** does this exist? If yes, what's the minimum it needs? If not, who would buy it?
- **Slot architecture at v3 (online learning slot):** today the LLM is read-only at inference time; an online-learning slot would update its own weights from outcomes. Strong R&D direction.
- **Eval engine at v3 (adversarial backtest):** "what if the market knew about your strategy" — backtest with adversarial micro-orders inserted. Probably wins.
- **Compliance at v0 (basic ToS):** is currently *missing entirely*. Even a paper-trading hackathon submission needs this.

### Dimensions identified

#### Cardinality — how many of each component does the system have at each tier?

```
Component                    v0          v1          v2          v3
-------------------------    ---------   ---------   ---------   ------------
Trading keys                 1 (env)     1 (file)    N (multi)   N + MPC
Strategies                   1-3         1-10        10-100      100-1000+
Users                        1           1           10-100      100-10000+
Audit log retention          forever     forever     forever     + archival
Reputation attestors         1 (xvision) 1           N           N + indexers
Marketplace listings         0           0           1-10        10-100
Settlement wallets           0           1           1-2         operator-defined
LLM providers                1 (Claude)  1-2         2-3         N
DEX integrations             0 / Alpaca  1 (Orderly) 1-2         3+
```

**The cardinality curve is exponential, but only on a few axes** (users, strategies, listings). Other axes stay near 1 forever (the trading-key custody model, the audit log, the settlement wallet). **Architecture investment should focus on the exponential axes.**

#### Reversibility — which decisions can be undone?

```
Decision                                    Reversibility   Notes
----------------------------------------    -------------   ---------------------------------
Switching DEX (Orderly → Hyperliquid)       LOW             huge migration; users have funds
                                                            at Orderly; switching needs a
                                                            user-by-user move
Switching custody model (custodial → MPC)   MEDIUM          add MPC; deprecate single-key path;
                                                            users migrate over time
Adding a new asset (BTC → BTC,ETH)          HIGH            additive; no migration
Adding a new chain (Mantle → Mantle,Base)   MEDIUM          additive but doubles ops surface
Renaming agent / strategy in DB schema      MEDIUM          requires migrations; breaks
                                                            external API
Token launch                                FULLY IRREV.    cannot un-launch a token; SEC will
                                                            remember
Smart-contract upgrade                      LOW (timelock)  7d delay + multisig
ToS / risk disclosure publication           HIGH            additive; can republish
ERC-8004 reputation events                  IRREVERSIBLE    on-chain; cannot un-write events
                                                            (can only emit "correction" events)
Marketplace 95/5 fee split                  MEDIUM          can change going-forward; existing
                                                            licenses honor original split
```

**Three irreversible decisions are visible: (1) ERC-8004 events on-chain, (2) potential token launch, (3) regulatory positioning once disclosed.** All three should be made deliberately, not by default. **The wallet plan's audit log writes nothing on-chain, which is the right call.**

#### Modularity — which components can ship without others?

```
Component                   Can ship without:                              Cannot ship without:
-------------------------   --------------------------------------------   ------------------------------------
Risk engine                 marketplace, reputation, autooptimizer,       core trading pipeline
                            dashboard
Audit log                   marketplace, reputation, dashboard             core trading pipeline + ledger
Marketplace                 risk engine (marketplace can list strategies   ERC-8004 NFT identity (for listings),
                            of any quality), reputation                    settlement wallet
Reputation (ERC-8004)       marketplace, autooptimizer                    on-chain deployment of registries,
                                                                           agent NFT mint
AutoOptimizer              marketplace, reputation                        eval engine, audit log
Eval engine                 marketplace, reputation, autooptimizer        ledger (positions table)
Wallet management           marketplace, reputation, autooptimizer        core trading pipeline
Dashboard                   anything (UI lives separately)                 something to display
```

**Highly modular surface.** Each major component can ship independently of the others. **This is good architecture** — it means the hackathon can deliberately ship a subset.

### What v0 actually is — the irreducible MVP

Reading the periodic grid + reversibility + modularity:

```
v0 IRREDUCIBLE MVP

Core (cannot remove):
  - One strategy, one trader slot, one risk engine
  - One Orderly account, one trading-only key (env-var)
  - SQLite ledger (positions table only — no funding_attributions, no decisions)
  - Single-user mode (operator)
  - xvn kill --all (the safety net)

Selected for hackathon demo (negotiable):
  - 3-5 strategy variants
  - Per-strategy hard caps (Phase 2)
  - Audit log (Phase 1 Task 1.2)
  - xvn emergency-close (Phase 4)
  - One marketplace listing (proves the rail)

Defer to v1+:
  - Dynamic quota (Phase 6)
  - Reservation pattern (Phase 2.3) — single-user race condition is unlikely
  - Aggregate margin guard (Phase 5) — cross-margin contagion bounded by hard caps
  - Pre-trade simulation (Phase 3.1) — Orderly orderbook reads not strictly required
  - Reconciliation (Phase 7) — manual reconciliation suffices
  - Spreadsheet UI (Phase 8) — CLI suffices for one operator
  - Approval gate (Phase 4.5) — manual operator review suffices
  - Funding attribution (Phase 1.3 funding table) — perp funding is small for short holds
```

The irreducible v0 is roughly Phase 0 + half of Phase 1 + selected pieces of Phases 2, 4. **About 1.5-2 weeks of work, not 5.**

### What v0 implies for the hackathon submission

- **Demo what's there; defer what isn't.** The pitch is: "non-custodial wallet design + audit log + kill switches + one marketplace transaction." Not: "comprehensive risk + dynamic quota + reconciliation + UI."
- **The autooptimizer might be the more important hackathon demo.** Per Run 4, it's the loop that makes xvision novel. If forced to choose between fully-shipping wallets and shipping autooptimizer, the latter is the differentiator.
- **The wallet plan's Phase 0 (validation gates G1, G2) is non-negotiable** — without G1 the security model is broken. With G1, even the partial v0 is a real demonstration of non-custodial trading.

### Substitution exercises on the v0

#### Substitute cardinality on users

- **v0 with N=1 user:** the current spec assumes this. Works.
- **v0 with N=10 users:** trading keys must be per-user. Adds ~3 days of work.
- **v0 with N=0 users (just operator-as-user):** the simplest case. **This is what the hackathon really needs.** Marketing language can describe N>1; demo only needs N=1.

#### Substitute reversibility on smart contracts

- **v0 with no smart contracts:** the marketplace fee router is the *only* on-chain dependency. If we defer it, all dependencies on Mantle become "future work." **The smart-contract-surface spec already deferred this; the wallet plan respects that.**
- **v0 with all contracts deployed:** adds 3-4 weeks of contract dev + audit. Out of hackathon scope. **Don't go here.**

#### Substitute modularity on the dashboard

- **v0 with no dashboard:** CLI suffices for one operator. Demoable as terminal recording.
- **v0 with full dashboard (Phase 2d):** depends on Plan 2d shipping; risky in hackathon timeline.
- **v0 with one-page demo dashboard:** static HTML showing one strategy's state. **5-day implementation, big demo win.** Worth scoping.

#### Tuple footer

```
operators:        organon-construction · dimension-identification
organon:          periodic-grid (16 components × 5 tiers; ~80 cells; many empty as
                  predictions / coining opportunities)
dim prompts:      cardinality · reversibility · modularity
not surfaced:     "v0 success criteria" — the grid shows WHAT v0 is; the missing
                  artifact is HOW you'd know v0 was successful (judge feedback?
                  user signups? real PnL?). Worth a tuple specifically on hackathon
                  judge mental-models.
```

---

## Run 10 — Why Orderly + Mantle vs. anything else?

### State machine of the infrastructure-choice lifecycle

```
                                                     <-- migrating ---+
                                                                       |
                                                                       v
            +-- researching alternatives <----------- regret -------+   ?
            |                                                       |   |
            |                                                       |   |
            v                                                       |   |
        considering                                                 |   |
       option (e.g.,                                                |   |
       Orderly+Mantle)                                              |   |
            |                                                       |   |
            v                                                       |   |
        evaluating                                                  |   |
       (tech / biz / regs)                                          |   |
            |                                                       |   |
            +-- fails ---------> back to researching                |   |
            |                                                       |   |
            +-- passes -------> committed                           |   |
                                  |                                 |   |
                                  v                                 |   |
                             integrating <-------+                  |   |
                                  |              |                  |   |
                                  v              | (dev work)       |   |
                              integrated --------+                  |   |
                                  |                                 |   |
                                  v                                 |   |
                              locked-in   -------> (alternative     +---+
                              (sunk cost                matures + xvision
                               + switching                  stagnates)
                               cost grow)
```

xvision is currently at `integrating` for Orderly + Mantle (the M0 probe passed; the executor is built but not yet live with real capital). The next state (`integrated`) requires the wallet plan to ship.

#### State markers and triggers

```
State              Trigger to leave              Trigger to stay
-----------        ----------------------------  -----------------------------
researching        find one option that passes   no alternative passes initial
                   initial criteria              criteria
considering        evaluation criteria defined   no criteria yet
evaluating         pass/fail decision            blocked on info
committed          dev capacity allocated        higher-priority work
integrating        first end-to-end pass         still debugging
integrated         time + investment             nothing wrong
locked-in          alternative + stagnation +    still good enough OR
                   stakeholder pressure          switching cost too high
regret             switching cost < net benefit  switching cost > net benefit
migrating          new infra integrated          stuck mid-migration
```

### Why xvision ended at "Orderly + Mantle" specifically — the path that won

Reading the existing docs (`architecture.md`, FOLLOWUPS, M0 probe, smart-contract-surface spec), the implicit reasoning was:

1. **Need a perp DEX** (because LLM-driven judgment is most legible on directional bets, and perps allow short + leverage)
2. **Need on-chain settlement** (so reputation + attestation are credibly portable)
3. **Need EVM** (so existing tooling — alloy, ERC-8004 specs, Solidity contracts — apply)
4. **Need cheap gas** (so per-trade attestation isn't priced out)
5. **Need a partner** (hackathon submission requires it; Orderly + Mantle co-sponsoring is implicit incentive)

These five constraints intersect at **Orderly + Mantle**. The choice was *forced* by the constraints, not selected from a free list.

### Tree of alternatives (siblings xvision didn't pick)

```
Perp DEX root
|
+-- On-chain orderbook (off-chain matching)
|   +-- Orderly Network                 [SELECTED]
|   +-- ApeX Pro                        (sibling: similar, less ecosystem)
|   +-- Lighter (zkSync)                (sibling: zk-rollup; more novel)
|
+-- On-chain orderbook (on-chain matching)
|   +-- dYdX v4 (Cosmos)                (different VM; would split tooling)
|   +-- Hyperliquid (custom L1)         (high TVL; less composable; native marketplace)
|
+-- AMM-style perp
|   +-- GMX (Arbitrum)                  (different model; less granular execution)
|   +-- Vertex                          (hybrid)
|   +-- Drift (Solana)                  (Solana → SVM split)
|
+-- Centralized exchange (CEX)
    +-- Binance                          (regulatory non-starter; custody risk)
    +-- OKX, Bybit                       (same)
```

Each unselected sibling forecloses something:

- **Hyperliquid:** custom L1 means xvision's Mantle-built ERC-8004 NFTs don't compose; would need to deploy elsewhere.
- **dYdX v4:** Cosmos VM means rewriting the entire Solidity / alloy stack; abandons EVM ecosystem.
- **GMX:** AMM-style execution means no granular order types (no scaled limits, no maker rebates); breaks slot-architecture's market-flavor flexibility.
- **Drift:** Solana means SVM rewrite; splits team focus between Mantle and Solana ecosystems.
- **CEX:** custody disqualifies; xvision's non-custodial design is incompatible.

### Negation — what does the opposite of each property look like?

```
Property of current choice      Negation                                  Resulting alternative
-------------------------       --------------------------------------    -----------------------------------
EVM-compatible                  Non-EVM                                   Solana (Drift), Cosmos (dYdX v4),
                                                                          Move (Aptos)
Off-chain orderbook             On-chain orderbook                        Hyperliquid, dYdX v4
On Mantle (L2 cheap)            On L1 mainnet                             Ethereum L1 — too expensive, no fit
                                On alt-L2                                 Base, Arbitrum, Optimism
                                On L3                                     hyperchain forks; experimental
Perp futures                    Spot                                      Uniswap, 1inch — different product
                                Options                                   Lyra, Premia — different math
                                Prediction markets                        Polymarket — different shape
USDC.e settlement               Native USDC                               Mantle has both; minor swap
                                Other stables (DAI, FRAX)                 niche; less liquidity
                                Volatile collateral                       ETH-margined, BTC-margined
Pre-funded by user              Bridged at-time-of-trade                  cross-chain message; high latency
                                Borrowed (e.g. via Aave)                  leverage layer; new dependency
```

**The most-interesting negation: "off-chain orderbook → on-chain orderbook."** Hyperliquid's HYPE token + native marketplace is the closest competitor to xvision's combined wallet+marketplace ambitions. The choice to NOT go there means xvision is implicitly betting that Mantle + ERC-8004 + Orderly's professional-grade execution beats Hyperliquid's all-in-one ecosystem.

### Dimensions

#### Animacy — is the chain alive, mechanical, or informational?

- **Mechanical chain (Mantle today):** xvision deploys; Mantle deals with its own roadmap; the relationship is technical.
- **Alive chain (e.g., Hyperliquid where the native ecosystem courts builders):** the chain itself promotes you, partners with you. Higher-leverage if the courtship works.
- **Informational chain (Mantle as substrate):** you treat it as bytes-and-execution; portable to any other L2 if needed.

**xvision's current relationship with Mantle is "mechanical with hackathon-courtship overlay."** Worth deciding deliberately whether to lean into the partnership (alive — more support, more visibility, more lock-in) or treat as substrate (informational — keep options open, less help).

#### Direction — does the choice's quality accumulate, decay, or oscillate?

- **Accumulating:** the longer you stay, the better — TVL grows, integrations deepen, ecosystem matures.
- **Decaying:** the longer you stay, the worse — better alternatives mature elsewhere, ecosystem stagnates.
- **Steady:** stays the same.

**For Orderly + Mantle:** likely *accumulating in 2026* (Mantle TVL growing, Orderly adding markets), *uncertain in 2027+* (alternatives like Hyperliquid + Sei perps are accumulating faster). xvision's lock-in window is the next 12-18 months; revisit deliberately at month 18.

#### Purpose — what's this infrastructure FOR?

- **For execution alone:** Hyperliquid is better (more liquidity).
- **For composability with ERC-8004:** Mantle is correct.
- **For partner support during hackathon:** Mantle / Orderly is correct.
- **For maximum user reach:** any chain with embedded-wallet UX is better (Base + Coinbase Smart Wallet has the largest reach).
- **For minimum regulatory exposure:** non-EVM chains have less SEC precedent.

**The current choice optimizes for composability + partner support.** If the platform later prioritizes user reach or regulatory clarity, the choice changes.

### What the state machine + tree + negations reveal

#### The infrastructure choice is good for now and reversible later

- Orderly + Mantle is the right answer for the constraints xvision has *today* (perp + on-chain settlement + EVM + hackathon partner).
- The choice is in the `integrating` state, where switching cost is still low. **Now is the time to make the choice deliberately rather than by inertia.**
- A scheduled review at month 18 (revisit Hyperliquid, dYdX, alternatives) is warranted. Add to FOLLOWUPS as a calendar-driven action.

#### The lock-in trajectory should be planned, not stumbled into

- **What should xvision deliberately lock-in to Mantle:** ERC-8004 NFT identity, on-chain reputation events. These need permanence; switching means accepting reputation discontinuity.
- **What should xvision deliberately keep portable:** strategy code (Rust), risk engine, audit log, dispatcher abstraction. These can run against any DEX.
- **What's currently entangled but shouldn't be:** the OrderlyExecutor is hardcoded into the dispatcher. The plan's `OrderlyOrderSubmit` trait was a step toward portability; **make it explicit that "BrokerSurface" (Plan 2c) and "OrderlyOrderSubmit" should converge** as a single Broker abstraction.

#### The regret-state risk to plan for

- Hyperliquid's HYPE marketplace ships in mid-2026 (rumored).
- If it ships with strong creator economics + native trading, xvision may face the regret state.
- **Pre-positioning move:** keep the dispatcher abstraction strict; verify quarterly that switching cost is bounded. If switching to Hyperliquid becomes a 1-month project rather than a 6-month one, the lock-in risk is acceptable.

#### The decision to NOT go on-chain orderbook

- Off-chain orderbook (Orderly) means xvision's audit log is the *only* off-chain truth.
- On-chain orderbook (Hyperliquid) would let third parties verify execution independently.
- **For the "valuable on-chain reputation" thesis (Run 6), an on-chain orderbook would be more credible.**
- This is a real cost of the Orderly choice. Worth acknowledging in marketing: "we publish the audit log because the orderbook isn't on-chain — here's the verification path."

#### Tuple footer

```
operators:        tree-finding · negation
organon:          state-machine (8 states + 12 transitions of infrastructure-choice
                  lifecycle) + tree of perp-DEX alternatives (4 categories × 3-4 leaves)
dim prompts:      animacy · direction · purpose
not surfaced:     "infrastructure choice as a *bet on a future*" — every infrastructure
                  bet implicitly bets on which ecosystem grows. Mantle bet = bet that
                  Mantle grows. Worth a tuple specifically on which futures xvision is
                  implicitly betting on (via every dependency choice).
```

---

## Run 11 — What changes if 1 user becomes 1,000 users?

### Tree of scaling-failure modes

```
Scaling failure (root)
|
+-- Custody scaling
|   +-- Per-user trading key storage
|   |   +-- breaks at N>=10:   env-var key path no longer viable
|   |   +-- breaks at N>=100:  AES-256-GCM single-key + DB scheme has hot-key risk
|   |   +-- breaks at N>=1000: must move to MPC or HSM-backed signing
|   |
|   +-- Per-user EVM signer registration UX
|   |   +-- breaks at N>=10:   manual brokered Orderly onboarding doesn't scale
|   |   +-- breaks at N>=100:  must automate add_orderly_key via embedded wallet flow
|   |
|   +-- Trading-key rotation
|       +-- breaks at N>=50:   90-day rotation = ~1 rotation/wk; manual unfeasible
|       +-- breaks at N>=500:  must auto-rotate; needs persistent user session for re-sign
|
+-- Compute scaling
|   +-- LLM inference cost per fire
|   |   +-- N=1, 5 strategies, 10 fires/day = ~50 calls/day = $5/day
|   |   +-- N=10                                            = $50/day
|   |   +-- N=100                                           = $500/day = $15k/mo
|   |   +-- N=1000                                          = $150k/mo
|   |
|   +-- AutoOptimizer mutation cost
|   |   +-- N=1     = ~$10/day mutation budget
|   |   +-- N=100   = same (autooptimizer is platform-level cost, not per-user)
|   |   +-- N=1000  = same, but the mutation pool gets exponentially more diverse
|   |   +-- breaks at N>=10000: autooptimizer must produce strategies faster than
|   |       users discard them; throughput, not cost, becomes the binding constraint
|
+-- Database scaling (SQLite single-file)
|   +-- Audit log size
|   |   +-- N=1, 50 decisions/day, 1y retention   = ~18k rows  = ~10 MB
|   |   +-- N=100                                  = 1.8M rows  = ~1 GB (still SQLite-fine)
|   |   +-- N=1000                                 = 18M rows   = ~10 GB (SQLite stress)
|   |   +-- breaks at N>=10000: must move to Postgres + partitioning
|   |
|   +-- Concurrent write rate
|   |   +-- breaks at N>=50:   SQLite single-writer-lock contention on positions table
|   |   +-- breaks at N>=200:  dispatcher serialization becomes user-visible latency
|   |   +-- breaks at N>=1000: must shard by user_id or move off SQLite
|   |
|   +-- Backup / restore
|       +-- breaks at N>=100:  1 GB SQLite backup window > acceptable; need streaming WAL
|       +-- breaks at N>=1000: must move to managed DB (RDS, Supabase, etc.)
|
+-- Risk engine scaling
|   +-- Per-strategy quota_factor computation
|   |   +-- breaks at N>=100:  O(strategies) reads per evaluation; needs caching
|   |   +-- breaks at N>=1000: must precompute and refresh on event triggers
|   |
|   +-- Reservation table contention
|       +-- breaks at N>=50:   process-level Mutex<HashMap> for per-strategy locks
|                              becomes lock storm; need per-strategy task or actor model
|
+-- Marketplace scaling
|   +-- Listing volume
|   |   +-- N=1     = 10 listings   = manageable manually
|   |   +-- N=100   = 1k listings   = needs search + filter
|   |   +-- N=1000  = 10k listings  = needs ranking, recommender, anti-spam
|   |
|   +-- Sale volume
|   |   +-- N=1000 = ~$10k/day; crosses regulatory threshold for "exchange-like"
|   |       activity in many jurisdictions (e.g., MTL in US states)
|   |
|   +-- Spam / fraud (Sybils)
|       +-- breaks at N>=10 creators: trivially gameable reputation collusion
|       +-- breaks at N>=100:        needs Sybil detection at register
|       +-- breaks at N>=1000:       needs identity attestation (hard problem)
|
+-- Operator scaling
|   +-- Daily review burden
|   |   +-- N=1    = 1-3 hrs/day  (current; tolerable)
|   |   +-- N=10   = 3-6 hrs/day  (full-time job)
|   |   +-- N=100  = unfeasible for one operator; must hire
|   |   +-- N=1000 = team of 3-5 ops engineers, on-call rotation
|   |
|   +-- Incident response time
|   |   +-- breaks at N>=10:  one operator in different timezone = unacceptable lag
|   |   +-- breaks at N>=100: needs 24/7 coverage = on-call rotation
|   |
|   +-- Customer support volume
|       +-- breaks at N>=10:   every user wants direct contact; doesn't scale
|       +-- breaks at N>=100:  needs support tickets, FAQ, AI-assisted triage
|       +-- breaks at N>=1000: needs documented runbook + dedicated support team
|
+-- Compliance scaling
|   +-- KYC requirements
|   |   +-- soft trigger at N>=100   (regulators notice)
|   |   +-- hard trigger at N>=1000  (regulators MUST notice)
|   |
|   +-- Sanctions screening
|       +-- breaks at N>=1: need OFAC sanctions check on every wallet from day one
|           (operator personal liability if not done)
|
+-- Network / infra scaling
    +-- Mantle RPC rate limits
    |   +-- breaks at N>=50:  free RPC tier insufficient; must pay for premium
    |   +-- breaks at N>=500: must run own RPC node or use multiple providers
    |
    +-- Orderly REST rate limits
    |   +-- breaks at N>=100: per-account limits aggregate across users; need
    |       per-user accounts (which is the multi-tenant model)
    |
    +-- LLM provider rate limits
        +-- breaks at N>=10:  hitting Anthropic per-org rate limits during peak
        +-- breaks at N>=100: must spread across multiple providers
```

### Combination — system component × user scale

```
                     N=1               N=10              N=100             N=1000           N=10000
                     ---------------   ---------------   ---------------   --------------   --------------
Trading keys         env var           single-file       per-user          MPC pool          MPC + HSM
                                       AES-GCM           encrypted DB
LLM cost             $5/day            $50/day           $500/day          $5k/day           $15k/day
                     (operator)        (operator)        (sub fee mand)    (sub fee mand)    (platform fee
                                                                                              essential)
Audit log size       10 MB/yr          100 MB/yr         1 GB/yr           10 GB/yr          100 GB/yr
                     SQLite            SQLite            SQLite            Postgres          Postgres+sharded
Operator hours       1-3 hr/day        3-6 hr/day        full-time +       team of 3-5       ops dept
                                                          partner            + 24/7 rotation   + compliance
Marketplace          0-1 listings      10 listings       100 listings      1k listings       10k listings
                     (no UI)           (CLI works)       (search needed)   (recommender)     (anti-spam team)
Compliance           ToS               + risk discl      + tax reports     + KYC + sanctions full regulated
Incident response    self-handle       self + backup    24/7 on-call      on-call team       on-call +
                     within hours      within hours     rotation                              incident commander
RPC tier             free              free              premium           multi-provider    own RPC node
LLM rate limit       single-provider   single            multi-provider    multi-provider    committed capacity
Database             SQLite WAL        SQLite WAL        SQLite WAL +      Postgres          Postgres + sharded
                                                          backup script
```

### Dimensions identified

#### Complexity — how comprehensible is the system at each scale?

- **N=1:** one operator can hold the entire system in their head.
- **N=10:** still comprehensible per-component; cross-component interactions get fuzzy.
- **N=100:** specialization required; no one person knows everything.
- **N=1000:** documented protocols replace tacit knowledge; runbooks become load-bearing.
- **N=10000:** organization-design problem (org chart, escalation paths, decision rights) eclipses technical problem.

**Implication:** the documentation burden grows faster than the user count. **Investment in MANUAL.md, cli-reference.md, audit-log forensics tools is multiplicatively valuable.**

#### Homogeneity — how similar are users at each scale?

- **N=1:** user IS operator; perfectly aligned.
- **N=10:** all crypto-native early adopters; mostly aligned.
- **N=100:** mix of archetypes (Run 1's crypto-native + tinkerer + quant); divergent needs.
- **N=1000:** including DAO managers + HNW; need feature segmentation; "one product" no longer suffices.
- **N=10000:** product-market split; consumer vs institutional flavors of the platform.

**Implication:** the spreadsheet UI (Phase 8) assumes one operator's mental model. At N=100, different operators want different views; the UI must become configurable.

#### Animacy — at what scale does the system become "alive" rather than "mechanical"?

- **N=1:** mechanical. One person + their tools.
- **N=10:** mechanical with intermittent ecosystem signals (a few users in Discord).
- **N=100:** ecosystem starts to behave (norms emerge, conventions, language).
- **N=1000:** the *system* becomes a force — features get used in unintended ways, community rituals form, regulators notice.
- **N=10000:** fully alive — system has its own dynamics independent of the team's intentions; design becomes about *steering* rather than *building*.

**Implication:** at N>=100, governance becomes a real concern. Who decides what gets listed? What gets killed? What constitutes fraud? Currently silent in the spec.

### What the tree + combination + dimensions reveal

#### Three load-bearing breaks happen at N=10, not N=1000

- **Trading-key custody:** the env-var single-key model breaks at N=10. Fixing this is **post-hackathon** in the wallet plan, but the threshold is much closer than "post-hackathon" suggests.
- **Compliance:** OFAC sanctions screening is a Day-1 obligation in many jx, not a scaling issue. **Add to the wallet plan as a P0 ops task.**
- **Operator burden:** 3-6 hrs/day at N=10 turns the project into a full-time job. If the operator wants their life back, automation isn't optional past N=10.

#### Two thresholds are bigger jumps than the others

- **N=100 (~$15k/mo LLM cost):** subscription fees become *mandatory*; without them, operator personally subsidizes every user. This forces a business-model decision earlier than "when we have 100 users" suggests.
- **N=1000 (regulatory):** crosses the threshold where regulators in many jurisdictions consider the platform an exchange. Either prepare licensing OR position to stay below the threshold (intentional throttle).

#### The autooptimizer economics scale differently

The autooptimizer's cost is *per platform*, not per user. At N=1000 users with one autooptimizer, the autooptimizer cost is the same as at N=1. **This means the autooptimizer's per-user cost goes down with scale** — a strong argument for ramping it up early.

#### What should change in the wallet plan based on this run

1. **Add a P0 task: OFAC sanctions screening on every onboarding.** No exceptions.
2. **Update Phase 7 (multi-key support) to N=10 priority, not "post-hackathon."** Marketing says "multi-tenant" the moment user #2 appears.
3. **Add an operational SLA tier table to MANUAL.md:** what level of response time is committed at what N.
4. **Add an auto-disable feature for over-subscribed (LLM cost) accounts.** If a user hits cost cap, halt their strategies. Avoid surprise bills.
5. **Document explicit policy: are we trying to grow past N=100? past N=1000?** This is a strategic decision the spec is silent on. Different infrastructure investments follow.

#### Tuple footer

```
operators:        combination · dimension-identification
organon:          tree (8 main branches with sub-branches; N-thresholds at each leaf)
                  + N x component combination chart (10 components x 5 N-tiers)
dim prompts:      complexity · homogeneity · animacy
not surfaced:     "scaling DOWN" — what if xvision deliberately caps at N=10 forever?
                  Could it be a viable boutique product? Worth its own tuple on
                  intentional-smallness as a strategy.
```

---

## Run 12 — What does "win the hackathon" actually require?

### Graph of hackathon-winning entities + labeled relationships

```
                                   +------------+
                                   |  PRESS /   |
                                   |  TWITTER   |
                                   +-----+------+
                              amplifies  |  influences
                                         |
                              +---------------------+
                              |     SPONSOR         |  (Mantle, Orderly)
                              |   (decision-maker   |
                              |    ecosystem        |
                              |    interest)        |
                              +----+--------+-------+
                       endorses    |        |  grants prize
                                   |        |
                                   v        v
            +------+ ranks   +-----------+ judges  +------------+
            | YOU  |-------->|  JUDGES   |<--------|   DEMO    |
            +---+--+         +-----+-----+         +---+--------+
                |                  |                   |
                | builds           | award             | dramatizes
                v                  |                   v
          +-----------+            v             +------------+
          |   CODE    |     +-------------+      |  AUDIENCE   |
          |   REPO    |---->|   PRIZE     |      | (devs +     |
          +-----+-----+ proves +-----------+     |  crypto    |
                |                                |  Twitter)  |
                | enables                        +-+----------+
                v                                  |
          +-----------+                amplifies   |
          | NARRATIVE |<------------------------+
          | ("what is  |  hooks
          |  this")    |
          +-----+-----+
                |
                | persuades
                v
          +-----------+
          | FIRST     |
          | USERS     |
          +-----------+
                |
                | validates
                v
          +-----------+
          | THESIS    |
          | (this     |
          | works)    |
          +-----------+
```

Graph density observations:

- **Hub: SPONSOR.** Connected to Press, Judges, Audience. Likely the highest-leverage relationship. **Cultivate sponsor relationships before judges directly.**
- **Hub: NARRATIVE.** Connected to Audience, Judges (via demo), First Users. **The single artifact that touches everyone is the project's narrative.**
- **Disconnected component: AUDIENCE → FIRST USERS via NARRATIVE.** This is the only path from "win" to "long-term win." If the demo doesn't generate first-users, the prize is the only durable outcome.

### Node-by-node value & investment cost

```
Node                Investment cost      Output                            Win-leverage
-----------------   ------------------   ----------------------------      -----------------
Code repo           weeks (already in    proof-of-work for judges          MEDIUM (necessary
                    progress)                                               but not sufficient)
Demo video          1-3 days             dramatized moment for audience    HIGH (most-shared
                                                                            artifact)
Narrative (1-pager) 0.5 day              hook + positioning for everyone   VERY HIGH (cheapest,
                                                                            highest leverage)
Live trade demo     1 day setup +        dramatic moment in demo           HIGH (memorable, but
                    dependency on                                          fragile if it fails
                    Orderly stability                                       on stage)
Sponsor outreach    3-5 days             pre-positioning with judges       VERY HIGH (cited by
                                                                            judges in deliberation)
Twitter / media     ongoing              audience + sponsor amplification  MEDIUM-HIGH (compounds
                                                                            over time)
First-users         hard, post-demo      thesis validation + retention     CRITICAL for long-term
Judging optics      1-2 days dress       polish in front of judges         MEDIUM (table stakes)
                    rehearsal
```

### Dimensions identified

#### Longevity — what kind of "win" lasts?

```
Win-type                                       Longevity              Investment to chase
-------------------------------------------    -------------------    ----------------------
Prize money                                    1-3 weeks of buzz      ~50% of demo polish
Sponsor partnership (paid pilot, follow-on)    months                 sponsor outreach + tech fit
Twitter / press coverage                       weeks (decays fast)    narrative + demo + timing
First 10 users                                 lifelong                first-user UX + onboarding
Acquihire / acquisition signal                 years (or zero if      everything else + a clean
                                                doesn't happen)        codebase + a likable team
Thesis validation (PUBLIC proof that LLMs      indefinite              real PnL + transparent
can trade)                                                              audit log + survival
Recruiting leverage                            indefinite              public visible mastery
                                                                       (talks, repos)
```

**The longest-lived win is "thesis validation."** That requires real trades + transparent results + survival past the hackathon. It's also the win the spec is best-positioned to deliver — the audit log makes the trades verifiable.

**The most-pursued-by-default win is "prize money."** It's the shortest-lived. Investment in prize-optimization should be capped.

#### Homogeneity — is "win" one thing or many?

- **Homogeneous version of "win":** the prize check. Optimizes single-metric (judge votes).
- **Differentiated version:** prize + sponsor + first users + recruiting + thesis = compound win.
- **Implication:** strategies that optimize for the prize at the cost of the others are common (slick demos with no real product). xvision should optimize for the differentiated win — every artifact (demo, repo, narrative) does double-duty for sponsor + audience + future users.

#### Distribution — who benefits from the win?

- **Concentrated:** operator (edkennedy) gets the prize and the recruiting boost.
- **Distributed:** the wider xvision ecosystem (early users, marketplace creators, sponsor) all gain from a high-visibility demo.
- **Implication:** structuring the hackathon submission to *include* early creators (mention them, demo their strategies, credit them) distributes the win and creates a narrative of "we're already a community" — which itself reads as a stronger signal than "I built this alone."

### What the graph + dimensions + investment-cost reveal

#### The three highest-leverage moves before submission

1. **Cultivate sponsor relationships before the deadline.** Sponsors influence judges, attend demos, and amplify the win on their channels. The 3-5 day investment yields disproportionate return. **Sponsors care about: technical fit (are you actually using their tech), aesthetic fit (does the demo look good on their feed), ecosystem fit (does this make their ecosystem look richer).** Talk to Mantle DevRel, Orderly partnerships team, before May 25.
2. **Write the narrative first; everything else is downstream.** A 1-pager that answers "what is xvision, why does it matter, why now, why you" — used to brief the demo, the judges, the press, and the first users. Cheapest, highest-leverage. **One day of writing.**
3. **Make the demo bulletproof against the live-trade-fails scenario.** A pre-recorded backup, a clearly-bracketed "this is a live demo against Orderly testnet" caveat, an alternate plan if the network is congested. The Knight Capital lesson (Run 7) applies: if the live trade fails on stage, the prize is forfeit.

#### Three things to NOT optimize

1. **Polish at the cost of substance.** Judges who are technical (Mantle DevRel, Orderly engineers) prefer a working ugly demo to a slick mocked one. **Prioritize functional > pretty.**
2. **Feature-count over feature-depth.** The autooptimizer + the wallet plan together cover too much. Pick one as the demo's load-bearing feature; mention the other in passing. **Recommendation: lead with the autooptimizer demo (per Run 4 + Run 9), use the wallet design as the trust story.**
3. **Twitter buzz over user signups.** Buzz decays in weeks; users compound. If forced to choose, optimize for the post-demo "how do I try this?" funnel.

#### What the wallet plan contributes to the win

- **Trust narrative ("non-custodial agent wallets")** — the wallet design is itself a positioning win. Even if not fully shipped, the *spec* is the artifact that demonstrates serious thinking. Showcase the spec in the demo.
- **Audit log** — the wallet plan's audit log enables real-time forensic transparency during the demo. Show a live decision flowing through the audit log on stage.
- **Kill switches** — `xvn kill --all` is itself dramatic. Demoing the kill switch (followed by a "and now we resume") is a strong moment.

#### Edge cases the graph surfaces

- **Disconnected component: code repo → first users.** Without the narrative bridging them, judges who read the code don't become users. **The README is the bridge — invest in it like marketing copy, not technical documentation.**
- **Edge that doesn't exist yet: SPONSOR → ECOSYSTEM amplification.** Sponsors normally amplify their winners on their official channels. xvision should *ask for that amplification explicitly* in pre-event sponsor conversations — a commitment to retweet, blog, or co-promote post-win is worth more than the prize money.

#### The win condition the spec hasn't articulated

If forced to write a single sentence post-hackathon describing what was won, what does xvision want it to say?

- **Bad:** "xvision won the Mantle hackathon trading prize."
- **Better:** "xvision demonstrated the first non-custodial autooptimizer-mutated trading-strategy marketplace on Mantle, with N users live-trading at the time of judging."
- **Best:** "xvision proved that LLM-driven strategy creation, evaluation, and on-chain reputation can produce a trading platform users trust enough to deposit real money to."

The third version is the long-game win. **All hackathon decisions should optimize for this sentence.**

#### Tuple footer

```
operators:        dimension-identification · organon-construction
organon:          graph (10 nodes + ~15 labeled edges)
dim prompts:      longevity · homogeneity · distribution
not surfaced:     "the gracious-loser path" — what if xvision doesn't win? The hackathon
                  is one tournament; the project is the long arc. Designing the post-event
                  follow-up (independent of placement) is itself a strategic question
                  worth its own tuple.
```

---

## Cross-Run Synthesis

12 runs across clients, agents, profits, autooptimizer, marketplace, ERC-8004, trust/safety, operator role, irreducible MVP, infrastructure, scaling, and hackathon-victory. Same operators (combination, negation, substitution, cross-domain re-instantiation, abstraction-lift, dimension-identification, tree-finding, organon-construction) drew different organons (atlas, dictionary, scale, chart, list, spectrum, lattice, timeline, periodic-grid, state-machine, tree, graph) — **the picker did its job**: no two artifacts share a shape, and the framework moves I'd default to (always-bullet-lists) appeared in only two of twelve runs.

### Themes that appeared across multiple runs

#### Theme A — The wallet plan is necessary infrastructure, NOT the differentiator

- **Run 4** (autooptimizer) — the mutation loop is the only entirely-novel thing xvision does
- **Run 9** (irreducible v0) — the v0 demonstration is autooptimizer + minimum wallet, not full wallet plan
- **Run 12** (hackathon) — recommended demo lead is the autooptimizer; wallet design is the trust story

**Implication:** the hackathon submission's narrative should be "autooptimizer + non-custodial trust" — not "comprehensive wallet management." Invest the last two weeks pre-deadline in the autooptimizer; treat the wallet plan as necessary scaffolding that earns the right to be trusted.

#### Theme B — Distribution / scale will hit harder than the spec assumes

- **Run 1** (depositor atlas) — the wallet-noob non-depositor row is the largest market
- **Run 8** (operator role) — daily review burden becomes full-time at N=10
- **Run 11** (scaling tree) — at least 5 architectural breaks happen before N=100

**Implication:** the wallet plan's "single-user mode for hackathon" and "multi-tenant post-hackathon" framing is correct for shipping but understates the urgency. **The next 3 things to build after the hackathon submission ships should be: (1) automated Orderly onboarding (kills the wallet-noob non-depositor scenario), (2) per-user trading-key support (closes the multi-tenant gap), (3) operator runbook with 24/7 incident response (handles N=10).** All three are hinted at in FOLLOWUPS but should be elevated to plan-status post-hackathon.

#### Theme C — On-chain reputation needs economic teeth or it's a checkbox

- **Run 5** (creator pitch) — reputation portability is xvision's distinctive creator value prop
- **Run 6** (ERC-8004 spectrum) — without something gating on reputation, it's at the 3% checkbox end
- **Run 4** (autooptimizer) — reputation-weighted quota allocation would solve multiple mutation-loop problems at once

**Convergent recommendation: reputation-weighted quota in the wallet engine.** ~2 days of work. Routes the reputation signal into the only place users actually feel it (their strategy's available capital). Promotes ERC-8004 from spec section to active feature. **Add to the wallet plan as P0.5 (insert after Phase 6 dynamic quota).**

#### Theme D — Compliance is missing across the stack

- **Run 1** (sanctioned jx in non-depositors)
- **Run 7** (regulatory action as failure scenario S10)
- **Run 11** (OFAC screening from N=1; not a scaling issue)

**Convergent recommendation: OFAC sanctions screening on every wallet at onboarding.** Day-1 obligation in many jurisdictions. Operator personal liability if not done. **Add to the wallet plan's Phase 9 (final integration test) as a mandatory check.** Other compliance work (KYC, tax reporting) is N-dependent; the OFAC check is not.

#### Theme E — Documentation grows faster than user count; underweighted today

- **Run 7** (incident playbook needed pre-incident)
- **Run 8** (operator daily journal as evolving artifact)
- **Run 11** (docs scale faster than users; multiplicative leverage)

**Convergent recommendation: invest in MANUAL.md and a journaled operator runbook NOW, before incidents force it.** Specifically:
- A daily checklist (the Run 8 timeline) ships as `MANUAL.md` Operator Day section
- An incident-response template ships as `MANUAL.md` Incident Response section
- A disclosure SLA commitment (Run 7 Equifax lesson) ships as a public commitment alongside the marketplace launch

#### Theme F — The "agent" terminology is doing more harm than good

- **Run 2** (dictionary surfaced terminology slippage between agent / strategy / variant)
- **Run 5** (creator pitch needs clarity on what the artifact IS)
- **Run 6** (reputation needs a coherent identity to attach to)

**Convergent recommendation: a one-pass terminology cleanup before the hackathon demo.** Three working terms:
- **agent** = the NFT-bound permanent identity (legal/emissary sense)
- **strategy** = the immutable config-hash-keyed pipeline configuration
- **variant** = a specific run instance (ULID)

The wallet plan's `agent_id` should probably be `agent_id` once SLF3 ships. **Add this rename as a follow-up task; not a hackathon blocker, but should happen before any external user reads the code.**

#### Theme G — Subscription / hosted runtime is the obvious missing revenue line

- **Run 3** (revenue tree leaf untouched)
- **Run 11** (LLM cost at N=100 = $15k/mo, forces subscription)

**Convergent recommendation: spec the subscription tier before launching the marketplace.** Even if not implemented for hackathon, the marketing materials should articulate "free tier + hosted-runtime subscription" so users know the future shape.

#### Theme H — Non-custodial design is an asset xvision underuses in marketing

- **Run 7** (FTX comparison — designs-out the failure other platforms had to apologize for)
- **Run 5** (anti-extraction is a creator value prop)
- **Run 12** (trust narrative is the hackathon's load-bearing artifact)

**Convergent recommendation: lead the hackathon narrative with non-custodial design.** The opening sentence of the demo / 1-pager should reference it. "Unlike FTX / Binance / etc., xvision never holds your trading capital — only the authority to trade with it."

#### Theme I — Pre-position defenses before they're needed

- **Run 7** (cross-margin guard story BEFORE the first contagion event)
- **Run 8** (calendar-driven kill-switches BEFORE vol events)
- **Run 12** (sponsor amplification commitment BEFORE the win is announced)

**Convergent pattern:** every defensive move is more credible if it exists before it's needed. **Build a pre-positioning checklist and execute weekly:** what known events (Fed days, Mantle upgrades, Orderly maintenance, model upgrades) are coming, and what positioning happens before each.

#### Theme J — Several 1-day investments offer disproportionate leverage

Cross-run, the following 1-day-or-less investments showed up multiple times:

```
Investment                                      Sources                  Leverage
---------------------------------------------   ----------------------   --------------------------
Reputation-weighted quota (P0)                  Runs 4, 6                massive (3% to 15% on
                                                                          reputation spectrum)
1-pager narrative for hackathon                 Run 12                   touches everyone (audience,
                                                                          judges, sponsors, users)
OFAC sanctions screening on onboarding          Runs 1, 7, 11            removes day-1 legal liability
Briefing-format randomization in autooptimizer Run 4                    blocks adversarial-briefing
                                                                          exploits
End-of-day operator report (xvn eod)            Run 8                    saves 30 min/day immediately
Public disclosure SLA commitment                Runs 7, 11               transforms incident response
                                                                          credibility
README that bridges code to first-users         Run 12                   converts judge-readers into
                                                                          users
```

**Recommendation:** these seven 1-day investments together change the project's defensive posture and external positioning more than the next 3 weeks of feature work. **Schedule a "leverage week" before the hackathon submission to ship them.**

### Cross-run tensions and how to resolve them

- **Run 9 (defer) vs Run 11 (scale-prepare):** "ship narrow for hackathon" vs "the breaks happen at N=10." **Resolution:** ship narrow, but document the scaling response per-component so the post-hackathon path is pre-planned. The MANUAL.md "what changes at N=10/100/1000" addendum is the right artifact.
- **Run 5 (lead with reputation portability) vs Run 6 (reputation has no teeth):** marketing portability without delivery is a lie. **Resolution:** ship reputation-weighted quota (Theme C) so the marketing claim is true.
- **Run 4 (autooptimizer needs guardrails) vs Run 12 (lead demo with autooptimizer):** showcase the dangerous thing. **Resolution:** make the safety guardrails LOUDLY visible in the demo. The audit log + kill switch + lineage graph all need on-stage moments.

### One-sentence answer to each run's question

```
Run 1   Who deposits?              Crypto-native bot traders + AI-agent tinkerers + DAO treasury
                                   managers (segmented investment per archetype, not "one product").
Run 2   What is "agent"?           A polysemous word that conflates NFT identity + pipeline config +
                                   per-run instance. Pick one meaning per usage.
Run 3   Where does revenue come?   Currently the 5% marketplace cut. Eventually a hybrid that
                                   includes hosted-runtime subscription (mandatory at N>=100).
Run 4   AutoOptimizer failures?   Reward hacking, mode collapse, drift, prompt injection,
                                   model-version drift — all detectable via lineage graph + audit log.
Run 5   Why creators?              Reputation portability + composable slot architecture +
                                   autooptimizer-evolves-your-seed (you keep the lineage NFT).
Run 6   ERC-8004 valuable?         Currently 3% (checkbox). Becomes 15-25% if reputation-weighted
                                   quota ships in the wallet engine.
Run 7   First big loss?            Cross-margin contagion is existential; transparent-disclosure-SLA
                                   + pre-positioning of the defense story converts existential to
                                   survivable.
Run 8   Operator's daily job?      Algo-trader-meets-SRE: continuous attention, discrete intervention,
                                   ~2-6 hrs/day at N<10.
Run 9   Irreducible v0?            Phase 0 + half of Phase 1 + selected pieces of Phases 2, 4
                                   (~1.5-2 weeks of work, not 5).
Run 10  Why Mantle + Orderly?      Forced by constraints (perp + on-chain settlement + EVM + cheap
                                   gas + hackathon partner) — good for now, revisit at month 18.
Run 11  Scale to 1000?             Three architectural breaks happen at N=10 (custody, ops, OFAC),
                                   not N=1000. Spec is silent on which growth tier is targeted.
Run 12  Win the hackathon?         Lead with autooptimizer + non-custodial trust narrative.
                                   Sponsor relationships before judges. Make first-users the
                                   long-game win.
```

### Top 10 recommendations consolidated, ranked by leverage x cheapness

```
Priority   Action                                          Source        Estimate
---------  --------------------------------------------    -----------   --------
P0         OFAC sanctions screening on onboarding          Theme D       0.5 day
P0         Reputation-weighted quota in wallet engine      Theme C       1-2 days
P0         1-pager narrative for hackathon submission      Theme A,H,I   1 day
P0         Pre-position non-custodial trust story in       Theme H       0.5 day
           every marketing surface
P0.5       Briefing-format randomization in autooptimizer Theme B       1 day
P0.5       End-of-day xvn report command                   Theme E       1 day
P1         README rewrite for first-user conversion        Theme A       1 day
P1         MANUAL.md scale-tier addendum (what changes     Theme E,F     1-2 days
           at N=10/100/1000)
P1         Public disclosure SLA commitment + incident     Theme E       0.5 day
           response template
P1         Terminology rename pass: agent_id to agent_id Theme F      0.5 day
                                                           (pre-rename verification)
```

**~10 days of work, distributable across the next 5 weeks.** The leverage isn't in adding features — it's in pre-positioning for everything the next 6 months will demand.

### What to read next (recommended adjacent tuples for future runs)

The "not surfaced" footers across the 12 runs collectively suggest at least these follow-up tuples worth running:

1. **Run 1's depositor populations** — model the *transitions between* archetypes (tinkerer → quant; hobbyist → HNW), not just the static archetypes.
2. **Run 2's political-economy + game-theory senses of "agent"** — surface fiduciary duty articulation (currently absent in spec).
3. **Run 4's "what does the autooptimizer OPTIMIZE FOR"** — the loss-function choice question.
4. **Run 5's "why DON'T creators come"** — friction-side analysis (Rust requirement, Mantle-only, Orderly account requirement).
5. **Run 6's "reputation as a story"** — UX of trust narratives in the marketplace.
6. **Run 7's "the *positive* first big event"** — windfall/FOMO management.
7. **Run 8's "operator as platform feature"** — operator-anonymous vs operator-as-brand decision.
8. **Run 9's "v0 success criteria"** — judge mental-models + measurable success post-event.
9. **Run 10's "infrastructure as bet on a future"** — every dep is a bet on which ecosystem grows.
10. **Run 11's "intentional smallness"** — boutique-cap-at-N=10 as a strategy.
11. **Run 12's "gracious-loser path"** — post-event follow-up independent of placement.

Each is a different angle on the same project. Recommend running 4-6 more before the hackathon submission, prioritizing the first-user conversion question (Run 5's complement) and the v0-success-criteria question (Run 9's complement).

---

*End of document. 12 runs, ~22,000 words. Spec, plan, adversarial review, and these explorations together form the four artifacts the wallet/marketplace/autooptimizer track has produced this week.*
