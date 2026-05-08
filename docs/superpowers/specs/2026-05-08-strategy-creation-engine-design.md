# Strategy Creation Engine — Design

> **Status:** Draft for user review · 2026-05-08
> **Pivot context:** Xianvec is pivoting fully to (1) the multi-strategy evaluation engine and (2) the marketplace + ERC-8004 trading agent strategy. This spec covers the **Strategy Creation Engine** — the authoring + bundling + sealing layer that produces the artifacts the eval engine evaluates and the marketplace lists. Eval engine has its own paused brainstorm at [`2026-05-08-eval-engine-decisions-so-far.md`](./2026-05-08-eval-engine-decisions-so-far.md); we resume it after this spec is approved.

---

## 1. Scope and product positioning

Xianvec is a software + marketplace + identity company for **AI trading agents**, not a broker, not a trade executor, not a custodian. The company operates four engines:

1. **Evaluation Engine** — runs strategies against historical and live data, produces metrics, comparisons, structured findings.
2. **Strategy Creation Engine** *(this spec)* — authors, validates, bundles, and seals strategy artifacts.
3. **Marketplace** — lists, licenses, distributes strategy content; handles purchases, license tokens, content rotation.
4. **Identity** — ERC-8004 reputation, attestations, receipts, license-token issuance.

**Trade execution is buyer-sovereign.** The xvn binary running on the buyer's machine or cloud calls the buyer's broker (Alpaca, Orderly) with the buyer's broker key, and calls the buyer's LLM provider with the buyer's LLM key. Xianvec the company never holds funds, never holds keys, never sees trades. The "broker tool" inside the xvn binary is a buyer-local convenience layer, not a Xianvec-operated service.

**All strategies are LLM-required.** Every strategy has at least one LLM agent in its slot stack. The "mechanical layers" (data, mechanical rules, risk, execution) are deterministic scaffolding around LLM decisions, not a separate agent-free strategy class. Pure rule-based bots belong on TradingView, not xvn.

## 2. KISS — the AI Agent Wizard is the product's face

Same binary, four wildly different surfaces. The default landing is **not the marketplace** — it is an AI Agent Wizard that interviews the user, drives xvn's MCP tools on their behalf, and produces a working strategy. The wizard *uses* the marketplace as a tool internally; the user can also browse the marketplace directly if they prefer.

The pitch in one line: **"An AI builds an AI for you."**

| Level | Who | Front door | Time to first run |
|---|---|---|---|
| **L1 — Wizard user** | "I heard about trading bots" | AI Agent Wizard (chat + visual strategy progress) | < 10 min, including LLM key setup |
| **L2 — Tweaker** | "I have a running bot, want to adjust" | Web UI with coarse risk presets, asset switcher; can re-engage wizard for changes | < 1 min |
| **L3 — Power user / external-agent driver** | "I want to author directly" | Web form, CLI, or external AI agent (Claude Code / Hermes) via MCP | < 30 min |
| **L4 — Researcher / publisher** | "I want to batch-test and publish" | Full CLI/MCP, fly.io deploy recipe, marketplace publishing | hours |

### KISS principles enforced at design time

- **AI Agent Wizard is `/`.** First boot of the dashboard opens directly into the wizard. Marketplace, authoring forms, and live status are accessible but not the default landing.
- **The wizard is itself an LLM agent.** It uses the user's LLM key to reason and calls xvn's MCP server tools to do work — `list_templates`, `create_strategy`, `update_slot`, `run_eval`, `deploy_live`. Same MCP API external agents (Claude Code, Hermes) call.
- **First step in the wizard: LLM key setup.** Buyer brings their own key from day 1. We do not provide starter credits. The wizard offers links to get a key (Anthropic, OpenAI, OpenRouter) and a paste box. No xvn-issued keys.
- **Plain English everywhere.** Template `display_name` and `plain_summary` are required fields. Technical labels live behind an "Advanced" toggle.
- **Pre-computed published evals.** Sellers run + attest evals at publish time. Browsers see performance without spending any LLM tokens. Custom evals on the buyer's own scenarios cost the buyer's tokens.
- **Coarse risk presets.** Conservative / Balanced / Aggressive map to (risk_pct_per_trade, max_leverage, position_concurrency, stop_loss_atr) bundles. Explicit values are L3+ surface.
- **Paper-mode default.** L1 uses Alpaca paper as the default broker. Real-money live trading via Orderly is an L2 unlock requiring the buyer's Orderly setup. Alpaca live exists as an optional secondary path but is not the v1 default.
- **One-binary install.** `curl install.sh | sh` then `xvn`. Dashboard opens at localhost. No Docker, no K8s for L1.
- **On-chain plumbing hidden.** Buyer sees "Free" or "Licensed". License token, content hash, 8004 contract calls live under a "Provenance" expander.
- **CLI is L3+/power-user/agent-focused.** L1 and L2 users never touch the CLI. CLI is for terminal-native users and for external AI agents driving xvn through scripted workflows.

### The Agent Wizard flow (L1)

```
[User opens xvn at localhost — wizard greets in chat]

Wizard:  "Hi! I'm the xvn setup agent. I'll help you build or pick an AI
          trading bot. First I need an LLM key so I can think with you.
          [Get OpenAI key →] [Get Anthropic key →] [Get OpenRouter key →]
          Already have one? [paste]"

User: pastes key

Wizard:  "Great. What's your goal today?
          ① Try a free strategy from the marketplace
          ② Build a custom strategy from a template (free)
          ③ Describe what you want and I'll create it"

(① → wizard browses marketplace WITH the user, asks risk preference, suggests
       a free template, runs eval-preview in-page, offers paper deploy.)
(② → wizard walks through template selection, fills slots conversationally,
       validates, runs eval, offers paper deploy.)
(③ → wizard interviews on thesis/regime/asset, picks closest template, drafts
       the strategy via MCP calls, runs eval, refines based on results.)

Wizard:  "Eval looks good. Want to run this in paper mode for 7 days to
          validate? [Yes, paper trade] [Customize first] [Save & exit]"

[If yes — strategy starts running on the local xvn daemon. Wizard exits.
 Dashboard now shows the running bot's chart + decisions.]
```

Throughout, the visual panel beside the chat shows the strategy being assembled — selected template, filled slot prompts, mechanical params, risk preset, eval results — so the user can SEE what's being built, not just chat about it. This is the "visual indicator but not full no-code" the brainstorm specified.

## 3. The strategy artifact — scaffold + slots

A strategy is a structured bundle, not a free-form prompt. Seven layers; mechanical layers are deterministic scaffolding, LLM slots are author-customizable.

```
┌─────────────────────────────────────────────────────────────┐
│ PUBLIC MANIFEST  (visible on 8004 listing + marketplace)    │
│  · display_name · plain_summary · creator · template        │
│  · regime_fit · asset_universe · decision_cadence           │
│  · required_models · required_tools                         │
│  · risk_preset (or explicit risk_config)                    │
│  · published_eval_attestations · license_terms              │
└─────────────────────────────────────────────────────────────┘
┌─────────────────────────────────────────────────────────────┐
│ ① DATA LAYER                              (mechanical)      │
│   OHLCV · indicator panels · onchain panels                 │
├─────────────────────────────────────────────────────────────┤
│ ② REGIME CLASSIFIER                       (LLM SLOT)        │
│   prompt + model + tools → emits {regime, confidence}       │
├─────────────────────────────────────────────────────────────┤
│ ③ SIGNAL INTERPRETER (Intern)             (LLM SLOT)        │
│   prompt + model + tools → bull/bear/flat evidence          │
├─────────────────────────────────────────────────────────────┤
│ ④ DECISION ARBITER (Trader)               (LLM SLOT)        │
│   prompt + model + tools → {action, size_intent, conviction}│
├─────────────────────────────────────────────────────────────┤
│ ⑤ MECHANICAL ENTRY/EXIT RULES             (mechanical)      │
│   entry_condition · exit_condition · stop · take-profit     │
├─────────────────────────────────────────────────────────────┤
│ ⑥ RISK LAYER                              (mechanical, REQ) │
│   sizing · concurrency cap · leverage cap · daily-loss kill │
├─────────────────────────────────────────────────────────────┤
│ ⑦ EXECUTION                       (mechanical, buyer-local) │
│   broker call (Alpaca paper/live, Orderly) · idempotent     │
└─────────────────────────────────────────────────────────────┘
```

Slots ②③④ are author-customizable LLM agents. Each slot defines: prompt body, required model class (e.g., "claude-4.6-sonnet-or-better"), tool allowlist, output schema. At least one LLM slot must be filled for the strategy to validate. Strategies may use 1, 2, or 3 LLM slots; order is fixed.

**Inter-agent communication.** Outputs flow ② → ③ → ④ as structured JSON. The scheduler routes each agent's output to the next agent's input. Outputs are typed and validated.

**Single-LLM "Custom" template.** A strategy with only slot ④ filled (no separate regime classifier or intern) is the freeform path — closest to the existing `trader_arm` strategy.

## 4. Templates and seed catalog

Eight templates ship in v1. Each template comes with a `display_name`, `plain_summary`, default mechanical fields, default LLM-slot prompts, and at least one **published_eval_attestation** so the marketplace is non-empty on day 1.

| Template | Display name (L1) | Regime fit | Maps to existing strategies |
|---|---|---|---|
| `trend_follower` | "Catches uptrends" | Trending | `ema_50_200_golden_cross`, `ema_slope_momentum`, `ema_ribbon_alignment`, `ma_crossover` |
| `breakout` | "Buys breakouts" | Trending volatile | `bb_squeeze_breakout`, `donchian_breakout`, `ema_squeeze_breakout` |
| `mean_reversion` | "Buys dips" | Range-bound | `bb_meanrev_zscore`, `bb_climax_fade`, `rsi_mean_reversion`, `ema_pullback_bounce` |
| `momentum` | "Rides momentum" | Trending | `macd_momentum`, `ema_acceleration_reversal` |
| `range_trade` | "Trades the range" | Sideways | `bb_pctb_oscillator`, `bb_band_walk_follow` |
| `scalping` | "Quick small trades" | Microstructure | (new in v1, no direct existing match) |
| `news_trader` | "Trades news events" | Event-driven | (new in v1, requires sentiment skill) |
| `custom` | "Single-agent freeform" | Any | `trader_arm` (existing LLM-driven baseline) |

**Onchain bonus template** (not in original research, but you have the assets): `onchain_smart_money` (display name "Follows the whales") maps to the Nansen family — `smart_money_accumulation`, `cex_outflow_accumulation`, `stablecoin_inflow_riskon`. Can ship as a 9th template if Nansen integration is in v1.

The existing `strategies/` markdown format already maps almost 1:1 onto template defaults. Reuse: thesis → plain_summary; parameters → mechanical_fields; decision rule → mechanical_entry_exit; expected regime → regime_fit; data dependencies → required_tools.

## 5. Permission tiers and sealing

Two tiers in v1. Seller picks tier per listing; buyer sees the tier ahead of purchase.

### Tier A — Open

Full bundle published as plaintext on IPFS or attached to the 8004 listing. Buyer downloads once, runs offline, can fork and republish. 8004 = pure attribution and provenance, not IP protection. Right for community contributions, reference strategies, and "calling card" listings. Zero centralized dependency on Xianvec.

### Tier B — Sealed (OSShip-style centralized hosting)

- Bundle content hosted on the **xvn API server** (modeled on `one-shot-ship-api.onrender.com`).
- Plaintext stored server-side with access controls + audit logs. Xianvec commits not to exfiltrate via legal terms and signed audit log; this is the OSShip trust-platform model, not envelope encryption.
- **Ed25519 signing** on every fetch — runtime verifies signatures before executing, catches tampering between server and buyer.
- Auth: API key + device fingerprint per buyer, license token bound to 8004 wallet identity.
- **Per-execution fetch.** Buyer's runtime fetches strategy content per agent fire from xvn API. Optional local cache with auth-gated re-validation; cache TTL controlled by seller (rotation policy).
- Seller can rotate strategy content (publish new version), revoke (stop serving), and version (8004 listing pins to a content hash; new version = new hash).

**Tier C (envelope-encrypted such that Xianvec cannot see plaintext) is deferred to v2.** Adds significant complexity (per-buyer envelope keys, key rotation on update) for marginal additional protection beyond Tier B's audit + access controls. Sellers who don't trust Xianvec with plaintext should pick Tier A.

## 6. Skill bundle format — OSShip-style markdown

Skills are reusable LLM-prompt + tool-allowlist units that can be composed into strategy slots. Adopted from OSShip's pattern, not Hermes's `agentskills.io` multi-file bundle format.

**One markdown file per skill.** YAML frontmatter + body.

```markdown
---
name: regime-classifier-base
display_name: "Crypto regime classifier"
description: "Classifies BTC/ETH market regime as trending|range|chop with confidence"
version: 1.2.0
allowed_tools: [ohlcv, indicator_panel]
model_requirement: "anthropic.claude-sonnet-4.6+"
output_schema:
  type: object
  properties:
    regime: { enum: [trending_bull, trending_bear, range, chop] }
    confidence: { type: number, minimum: 0, maximum: 1 }
---

You classify the current market regime for a crypto pair given recent indicator
state. Use the OHLCV history and indicator panel to determine which of four
regimes the market is in. Be conservative — when uncertain, return "chop" with
moderate confidence rather than committing to a directional regime weakly.

Inputs:
- ohlcv_history: last 200 bars
- indicator_panel: RSI(14), MA(50/200), ADX(14), ATR(14)

Output a JSON object matching the schema above.
```

- Skills are server-hosted on the xvn API. Each skill has a content hash, version, and Ed25519 signature.
- A strategy bundle's manifest references skills by `skill_id@version` + content hash; the runtime fetches them per execution.
- The xvn API exposes a **skill marketplace** dimension — skills can be sold/licensed independently of strategies. Same tier model (A/B) applies.
- Authors can compose multiple skills per agent slot — e.g., a Trader slot can compose `crypto-trader-base` + `news-aware-decision` + `risk-conservative` into one agent.

## 7. Tool registry

Tools available to a strategy's LLM agents at execution time. Author declares required tools in the strategy manifest; runtime grants them at fire time.

| Category | Tools | Built-in vs author-defined |
|---|---|---|
| **Market data** | OHLCV, order book, funding rate, indicator panels (RSI, MACD, Bollinger, Donchian, ATR, MA, EMA, ADX) | Built-in (xianvec-data, xianvec-core) |
| **Execution** | Alpaca paper (default for paper mode), Orderly (default for live mode, hackathon priority), Alpaca live (optional secondary), position/balance/fill | Built-in (xianvec-execution) — buyer-local only, never xvn-managed |
| **Onchain / external signals** | Nansen smart-money flows, funding-rate feeds, social sentiment, news API | Built-in (xianvec-data extensions, with author-supplied API keys) |
| **Author-defined skills** | OSShip-style skill markdown composed into agent slots | Author-defined; sealed (Tier B) or public (Tier A) |

**Sandboxing for author-defined skills (security model):** v1 ships skills as prompt-only (no executable code). v1.5 adds inline scripts (Python via Pyodide or Rust-WASM sandbox); deferred until prompt-only proves insufficient.

## 8. Authoring entry points

Four paths, all writing to the same MCP API surface so the bundle is identical regardless of how it was authored. The first path — the built-in Agent Wizard — is the default L1 entry point and the product's face.

### Path 0 — Built-in Agent Wizard (default at `/`)

Browser-native chat interface backed by an LLM agent. Calls xvn's MCP server tools to do all work. The user's LLM key powers both the wizard's reasoning AND any LLM slots in the strategies it builds. The wizard's UI is split: chat on one side, live visual progress on the other (selected template, filling slots, eval results). See §2 for the conversational flow.

### Path A — Web UI form

Browser-based, structured form per layer. Located at `/authoring/<draft_id>`. Not full no-code drag-and-drop. L2/L3 users who want direct control without chat can author here. The wizard can also drop the user into the form mid-conversation when they want fine-grained control over a specific field.

### Path B — Power-user CLI

`xvn strategy new --template <name>` (non-interactive) and `xvn strategy edit <draft_id>` (opens the form). The interactive `xvn strategy new --interactive` wizard mirrors the web wizard but runs in terminal — same MCP tool surface, terminal renderer. **CLI is L3+/power-user/agent-focused — not an L1 path.** L1 users are expected to be in the web wizard.

### Path C — External AI agent via MCP

Hermes / Claude Code / Cursor / any MCP-aware AI calls xvn's MCP server tools to author strategies on the user's behalf. The AI agent can compose, test, iterate, and publish in a single conversation:

```
User to Claude Code: "Make me a mean-reversion bot for ETH that uses
  sentiment when news is volatile."

Claude Code → xvn MCP:
  create_strategy(template="mean_reversion", name="eth-news-aware-mr") → draft_id
  update_slot(draft_id, "regime", "<custom prompt>")
  attach_skill(draft_id, agent="trader", skill="news-aware-decision")
  set_mechanical_param(draft_id, "rsi_oversold", 25)
  set_risk_preset(draft_id, "conservative")
  run_eval(draft_id, scenario="ETH-2025q1-bull")
  → reports findings to user, iterates
  publish_strategy(draft_id, tier="sealed", price="15USDC/mo")
```

## 9. CLI surface

CLI is L3+/power-user/agent-focused. L1/L2 users live in the dashboard wizard. The CLI surface mirrors the MCP surface — every CLI verb has a corresponding MCP tool and vice versa.

```
xvn install                                  # installer / setup
xvn                                          # start dashboard, opens browser at /
xvn config set llm-provider <provider>       # configure LLM key
xvn config set orderly                       # set up Orderly account (interactive)

xvn marketplace browse                       # list listings (with filters)
xvn marketplace get <listing_id>             # show details + published evals
xvn marketplace try <listing_id>             # one-shot eval-preview
xvn marketplace buy <listing_id>             # acquire license token

xvn strategy new --template <name>           # create draft from template
xvn strategy new --interactive               # terminal wizard (mirrors web wizard)
xvn strategy edit <draft_id>                 # opens web UI form for this draft
xvn strategy validate <draft_id>             # check before publish
xvn strategy publish <draft_id> --tier <A|B> # to marketplace + 8004

xvn skill new <name>                         # create skill from template
xvn skill list                               # list skills (mine + marketplace)
xvn skill attach <draft_id> <agent> <skill>  # attach skill to agent slot

xvn eval run <strategy_id> <scenario>        # run eval (defined in eval engine spec)
xvn eval batch <grid.json>                   # parallel batch (L4 power-user)
xvn eval compare <run_ids...>                # comparison view

xvn live deploy <strategy_id> --mode paper           # default; Alpaca paper
xvn live deploy <strategy_id> --mode live            # Orderly (default for live)
xvn live deploy <strategy_id> --mode live --broker alpaca-live   # optional secondary
xvn live status <deployment_id>              # uptime, P&L, last decision
xvn live stop <deployment_id>                # graceful shutdown

xvn deploy <strategy_id> --target fly        # fly.io recipe (only target in v1)

xvn agent serve --mcp                        # run MCP server for external agents
```

## 10. MCP server surface

xvn exposes the same verb groups as MCP tools so external AI agents (Hermes, Claude Code, Cursor) can drive xvn natively.

**Strategy authoring:** `create_strategy`, `update_slot`, `set_mechanical_param`, `set_risk_config`, `set_risk_preset`, `validate_draft`, `list_templates`, `get_strategy`.

**Skill management:** `create_skill`, `update_skill`, `attach_skill_to_agent`, `list_skills`.

**Eval lifecycle:** `run_eval`, `eval_status`, `eval_metrics`, `compare_runs`, `list_findings`.

**Marketplace + live:** `publish_strategy`, `list_listings`, `buy_strategy`, `deploy_live`, `live_status`, `revoke_license`, `attest_eval`.

All MCP tools are typed (JSON Schema), idempotent where reasonable, and return structured results that AI agents can reason over.

## 11. Live execution and deployment

xvn binary on the buyer's machine or cloud is the executor. Xianvec the company is never in the execution path.

### L1 default — buyer's machine, paper mode (Alpaca)

`xvn live deploy <strategy_id> --mode paper --capital 10000` runs a long-lived xvn daemon on the user's laptop. Strategy fires per its scheduler, calls user's LLM with user's key, calls Alpaca paper API for fill simulation, logs trades to `~/.xvn/runs/<run_id>/trades.jsonl`. Buyer manages uptime. **Default broker for paper mode is Alpaca.**

### L2 — buyer's machine, real money via Orderly

`xvn live deploy <strategy_id> --mode live --broker orderly` deploys to live trading via the Orderly setup integrated through xvn (hackathon priority). Buyer connects their Orderly account through xvn's Orderly onboarding flow during the deploy step. Real-money guardrails: explicit confirmation, capital cap shown clearly, daily-loss kill-switch active. **Alpaca live is optional secondary** (`--broker alpaca-live`) — supported but not the default v1 path.

### L4 — buyer's cloud, fly.io recipe

`xvn deploy <strategy_id> --target fly` ships a fly.io deployment recipe (Dockerfile + `fly.toml` template). Buyer's fly.io account, buyer's keys, buyer-managed uptime. **fly.io is the only deploy recipe in v1.** Modal/Daytona/Railway recipes deferred. xvn provides templates; xvn does not run cloud infra for buyers and does not provide free hosting credits — buyer rolls their own account.

## 12. Durable scheduler — port from SwarmClaw

xvn implements its own durable event scheduler in Rust, **porting the design pattern from SwarmClaw** (TypeScript). Not a runtime dependency — a native Rust implementation modeled on SwarmClaw's heartbeats, durable runs, retries, and agent-to-agent handoff.

**Storage:** SQLite-backed event store (matches the eval engine's planned persistence layer per the paused eval-engine brainstorm).

**Trigger types:**
- Time-based (cron)
- Event-based (price tick, indicator threshold)
- Handoff-based (slot ② output triggers slot ③, slot ③ output triggers slot ④)

**Lifecycle:** schedule → enqueue → execute → retry-on-failure (capped) → record-result → emit-events. Heartbeats every N seconds; observable via `xvn live status`.

**SwarmClaw research action item:** before implementation, read SwarmClaw's actual scheduler source (`swarmclawai/swarmclaw` on GitHub) and document the specific patterns we're porting. Out of scope for this spec; tracked in plan.

## 13. Marketplace and 8004 integration

The Strategy Creation Engine's terminal action is `publish_strategy`, which hands off to the marketplace + identity engines.

**Publish flow:**
1. Validate draft (all required slots filled, risk config valid, eval attestation present).
2. Compute content hash of full bundle.
3. Run a "publish-time eval" (canonical scenario, deterministic seed) and produce a signed eval attestation. Persist for marketplace browse-without-keys preview.
4. Tier A: upload plaintext to IPFS or attach to 8004 listing.
   Tier B: upload to xvn API server, generate Ed25519 signature, register license-token-issuance contract.
5. Mint 8004 listing transaction with: content hash, tier, license terms, eval attestation, creator identity.
6. Listing appears in marketplace.

**Buy flow:**
1. Buyer selects listing, pays via 8004 contract (USDC on Mantle, or whatever the marketplace contract specifies).
2. Contract issues license token bound to buyer's wallet.
3. Tier A: buyer downloads bundle from IPFS.
   Tier B: buyer's xvn runtime authenticates to xvn API with license token + device fingerprint, fetches strategy content per execution.

**Identity / reputation:** every eval run (publish-time or custom) and every live decision can be attested via `xianvec-identity` (per ADR 0008). Receipts accumulate per-strategy, per-creator, per-buyer. Deferred eval-engine work pins down the receipt schema.

## 14. Crate structure

Greenfield `xianvec-engine` crate is the strategy-creation home (per Approach B from earlier brainstorm). New sibling crates added:

```
crates/
  xianvec-core/                # already exists, types: Strategy, IndicatorPanel, ...
  xianvec-data/                # already exists, OHLCV + onchain feeds
  xianvec-execution/           # already exists, broker calls, buyer-local only
  xianvec-risk/                # already exists, deterministic veto rules
  xianvec-engine/              # NEW — strategy creation + bundling + sealing + eval orchestration
    src/
      bundle/                  # strategy bundle format, validation, hashing
      templates/               # 8 ship-templates
      slots/                   # LLM slot dispatch, agent loop
      scheduler/               # durable scheduler (ported from SwarmClaw)
      tools/                   # tool registry + per-agent allowlists
      sealing/                 # Tier A/B publishing, Ed25519 signing
      mcp/                     # MCP server surface
      cli/                     # CLI verbs
  xianvec-skills/              # NEW — OSShip-style skill markdown parsing, validation, signing
  xianvec-marketplace/         # NEW — listing, buy, license token, 8004 integration
  xianvec-identity/            # already exists per ADR 0008, expand for license token issuance
  xianvec-dashboard/           # NEW — axum server + SPA assets (web UI, marketplace, authoring forms)
  xianvec-cli/                 # already exists, expanded with new subcommands
```

## 15. Open questions and what's still TBD

- **Concrete bundle file format.** JSON for human-readability and JSON-Schema validation, or CBOR/MessagePack for size? Decision: JSON for v1 (readability wins; bundles are small).
- **Skill marketplace pricing model.** Same as strategies (one-time license, subscription, royalty)?
- **Cron expression dialect for scheduler.** Standard cron + extended event syntax? Decision deferred to scheduler implementation.
- **L1 default Alpaca paper account model.** Buyer brings their own free Alpaca paper account, vs xvn-issued shared paper credentials. Lean toward "bring your own" to keep xvn off the keys.
- **Orderly onboarding UX.** What does "connect your Orderly account through xvn" actually look like — wallet signature, API key paste, or hosted onboarding flow?
- **Eval-time vs publish-time vs live-time metrics format.** Must be unified per the paused eval-engine brainstorm; resolve when we resume that spec.
- **License token contract ABI on Mantle.** Coordinate with `xianvec-identity` and ADR 0008 on contract surface.
- **Exact handoff between Strategy Creation Engine's `publish_strategy` and the Marketplace engine.** Probably a function call within the same Rust binary, but contract surface should be specified.
- **Wizard agent's default LLM model.** Whatever the user picks at key-paste time? Or pin a default (e.g., Claude Sonnet 4.6 via the user's Anthropic key, OpenRouter Claude via the user's OpenRouter key)?

## 16. Out of scope

- Trade execution as a Xianvec-the-company offering (it's buyer-sovereign).
- Custody of buyer funds.
- Hosting buyer LLM keys or broker keys.
- xvn-issued LLM credits, starter credits, or any subsidized API usage. Buyer brings their own keys.
- xvn-hosted compute or hosted-runtime offerings. Buyer runs on their own machine or their own fly.io account.
- Fiat onramps.
- Modal / Daytona / Railway deploy recipes — only fly.io in v1.
- The Eval Engine itself — separate spec, currently paused at [`2026-05-08-eval-engine-decisions-so-far.md`](./2026-05-08-eval-engine-decisions-so-far.md), resumed after this spec is approved.
- The Marketplace + Identity engines — touched here at integration seams but separately specced later.
- Tier C envelope-encrypted sealing.
- Synthetic stress scenarios in eval (deferred per eval-engine brainstorm).
- The Karpathy autoresearcher improvement loop (consumes findings but doesn't constrain this spec).
- Mobile app, native desktop wrappers (Tauri/electron) — web dashboard at localhost is the v1 surface.
- Pure rule-based (no LLM) strategies — all strategies are LLM-required.

## 17. Decision log (this brainstorm, 2026-05-08)

- **Architecture:** single-binary Rust xvn. Self-contained.
- **External runtime deps:** none. Hermes, ACPX, SwarmClaw all dropped as deps. SwarmClaw scheduler design ported (Rust). Hermes-style skill bundle dropped in favor of OSShip-style.
- **All strategies LLM-required.** No mechanical-only path.
- **xvn doesn't execute trades** (the company); buyer's local binary does.
- **Permission tiers:** A (Open) + B (Sealed-Hosted, OSShip trust-platform). C deferred.
- **Skill format:** OSShip-style single markdown per skill, YAML frontmatter, server-hosted, Ed25519-signed.
- **Tool registry:** market data + indicators (built-in), execution (built-in, buyer-local), onchain/external (with author API keys), author-defined skills (sealed inside bundle).
- **Authoring entry points:** four — Agent Wizard (default `/`), web form, power-user CLI, external MCP — all writing to the same API.
- **MCP surface:** all four verb groups (authoring, skills, eval lifecycle, marketplace+live).
- **Default landing:** Agent Wizard (`/`), not the marketplace. Wizard *uses* marketplace as a tool. Pitch: "An AI builds an AI for you."
- **Live execution:** buyer's machine (L1 paper via Alpaca, L2 live via Orderly) + fly.io deploy recipe (L4 only). Modal/Daytona/Railway deferred. Alpaca live optional secondary. xvn-managed compute never.
- **Templates v1:** 8 (trend, breakout, mean-rev, momentum, range, scalp, news, custom) + optional 9th (onchain). All free Tier-A on the marketplace; authors who fork+customize can sell their forks.
- **KISS layered surface:** L1 Agent Wizard, L2 web tweaks, L3 web form / CLI / external agent, L4 researcher.
- **Onboarding:** LLM key on day 1 (no starter credits, buyer brings own); Orderly setup on going live. **No xvn-issued credits or hosting subsidies — Xianvec is not a charity.**
- **CLI is L3+/power-user/agent-focused.** L1 users never touch the CLI.
