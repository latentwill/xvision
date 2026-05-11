# Marketing methods — xvision

How we get xvision in front of the first 1,000 users without paid ads or
PR firms. Three pillars: programmatic video, podcast circuit, and crypto
Discord community presence. Each pillar exploits a specific asymmetry —
content the engine can produce cheaply from its own data, conversation surfaces
where the audience already lives, and verifiable on-chain claims that
short-circuit the usual "another trading bot" pattern-match.

**Author:** initial methods doc on 2026-05-11.
**Status:** Draft. Iterate based on what actually moves needle.

---

## Why these three

| Pillar | Asymmetry exploited | Cost per useful unit |
|---|---|---|
| Remotion + 11labs video | Eval runs already produce structured data (equity curves, findings, lineage trees) — Remotion turns that into video deterministically. No filming, no actors, no studio. | ~$2–5 in API calls per 2-min video |
| Podcast circuit | The "AI agents that improve themselves" + "non-custodial" narrative is novel enough to pitch into existing crypto/AI shows. One 90-min recording → 50k–500k impressions, evergreen. | ~$0 per appearance once the pitch deck lands |
| Crypto Discord communities | Mantle's, Orderly's, ERC-8004's, and the broader quant-crypto servers are where prospective users debate which bots to trust. Direct technical engagement compounds. | ~5 hrs/week of operator time |

What we're explicitly NOT doing in this doc: paid Twitter/X promotion,
influencer deals, sponsored newsletter slots, Telegram pump groups. Those are
fine ROI in some attribution models but they pattern-match to scam-coin
marketing and inverse-correlate with the trust narrative this project lives
on.

---

## Pillar 1 — Programmatic video (FreqTrade-style, Remotion + 11labs)

### What FreqTrade does well that we're copying

[FreqTrade](https://www.freqtrade.io/)'s YouTube + docs combination is the
state of the art for open-source trading-bot marketing:

- **Code-walkthrough authority** — videos show real strategy code, real
  backtests, real configs. No "stock-photo trader looking at screens."
- **Backtest visualizations** — equity curves with parameter sweeps overlaid;
  trade markers tied to indicator state; before/after comparisons.
- **Educational density** — each video teaches one specific capability of the
  framework. Useful even to viewers who never install the bot.
- **Cadence** — multiple per month, kept consistent over years. The library
  is the asset, not any single video.

The lift we add to that template: **the engine generates the video.** Every
xvision eval run produces a `BacktestResult` with equity series, trade ledger,
findings, and (for perp strategies) funding/leverage metrics. Remotion can
read that JSON and render a 90-second "this is what happened on run X" video
deterministically. The marginal cost of the Nth video is ~$3 in API calls.

### Pipeline

```
xvision eval run finishes
        │
        ▼
ResultBundle.json  ───▶  marketing/scripts/render-run-summary.tsx (Remotion)
        │                          │
        │                          ├─▶ Lightweight Charts series rendered frame-by-frame
        │                          ├─▶ Findings list animated in
        │                          ├─▶ Trade markers timeline
        │                          └─▶ Equity-curve key moments highlighted
        │
        ▼
script.txt (templated from result fields)
        │
        ▼
11labs API  ──▶  voiceover.mp3
        │
        ▼
Remotion <Audio src={voiceover} /> + visuals
        │
        ▼
mp4 (1920×1080, 60fps, ~90s)
        │
        ▼
Upload pipeline (manual approval first; later automated)
        ├─▶ YouTube (full quality)
        ├─▶ Twitter/X (clipped to 60s)
        ├─▶ TikTok / Reels (9:16 reframe)
        └─▶ Discord #showcase channels (gif preview + link)
```

### Video formats — order of leverage

Build these in this order. Each is a Remotion template + an 11labs voice
config that takes a single `RunBundle` as input.

1. **Run summary** — `marketing/templates/run-summary.tsx`. Inputs: a single
   `BacktestResult`. Output: 60–90s explainer ("Strategy X ran on Q4 2024
   high-vol regime; final equity $X; key finding Y; here's the equity curve
   moment by moment"). Highest-leverage template — the rest of these are
   variations.
2. **Compare-runs** — two runs side by side. Same RunBundle pair feeds into a
   delta-narrative ("Arm A used 8x leverage, Arm B used spot. Funding drag
   cost Arm A $X. Here's where each one decided differently"). Great for
   showcasing perp vs spot trade-offs.
3. **Strategy lineage** — overnight autoresearcher output. Show the parent
   strategy at the top, fork tree below, sealed survivors highlighted with
   their `agent_id` NFT. "11 mutations attempted, 2 sealed. Here's why
   `agent-018K…` won."
4. **Live-build demo** — chat-driven authoring captured as a screen recording,
   then re-rendered through Remotion with the chat replay + the bundle tree
   building up alongside. 3–5 minute walkthrough; replaces the "person at a
   keyboard" YouTube tutorial template.
5. **Verdict trace** — risk-gate decision deep-dive. Take a specific
   `cycle_id`; visualize the briefing → trader proposal → risk verdict →
   execution path. Show the audit log as the verifiable artifact.
6. **Concept explainers** — non-data videos for the harder ideas: "Why
   non-custodial matters", "What ERC-8004 actually does", "How the
   autoresearcher avoids overfitting." Manually-storyboarded; longer
   production cycle, but they're the videos that win over skeptics.

### Stack

| Layer | Tool | Why |
|---|---|---|
| Composition | [Remotion](https://www.remotion.dev/) | React-based; reuses dashboard components (Lightweight Charts, design tokens); data-driven; deterministic renders |
| Voiceover | [ElevenLabs](https://elevenlabs.io/) | Multi-voice; voice cloning if we want a consistent narrator; API-driven so scripts are programmable; ~$0.30 per minute of audio at the Creator tier |
| Visuals (charts) | TradingView Lightweight Charts via Remotion | Same charts as the dashboard, no second implementation; matches what users see in-app |
| Visuals (motion graphics) | Remotion + Tailwind tokens | Brand consistency with dashboard; warm-black + amber-gold tokens carry through |
| Render farm | Remotion Lambda (initially local; Lambda once cadence > 4 videos/week) | Pay-per-render; ~$0.10 per minute of output |
| Voice persona | Operator-selected from 11labs library; consider voice clone of the founder for AMA-style clips | Continuity across the catalog |
| Caption generation | Whisper (local) on the voiceover mp3 → SRT → Remotion `<Captions />` | Required for silent-autoplay on Twitter/Reels |

### Voice + visual brand

The dashboard's Folio dark tokens (warm-black `#0F0E0C`, amber-gold `#D4A547`,
Cormorant Garamond display, Inter UI, JetBrains Mono for numerics) carry
through to video. This is non-negotiable — every visual asset that touches a
viewer should reinforce the same identity.

Voice register: **technical, specific, low-hype.** No exclamation points in
voiceover scripts. No "to the moon." Match the README's "use at your own
risk" candor. Numbers stated precisely (basis points, percentages with two
decimals). The product is a serious tool; the marketing reflects that.

### What we deliberately don't do

- **Face-on-camera videos.** Founder face only appears in podcast appearances
  (different surface). Programmatic video is data-driven, no human in frame —
  cleaner narrative + production stays cheap.
- **AI-generated thumbnails with shocked faces.** Standard YouTube thumbnail
  bait inverse-correlates with the trust narrative. Thumbnails are the
  equity curve + the strategy name in our typography.
- **Made-up backtests.** Every video sources a real `BacktestResult.run_id`
  shown on-screen. Anyone can re-run that ID in their own deploy. This is
  the FreqTrade trust pattern; we double down on it via the audit log.

### Cadence target

- Phase 1 (weeks 1–4): 1 run-summary video per week, manually approved.
  Pipeline shakedown.
- Phase 2 (weeks 5–12): 2 videos/week — one run summary, one of the other
  formats. Operator approves each.
- Phase 3 (week 13+): 3–5 videos/week. Operator approves topics, not each
  video. Automation handles the rest.

---

## Pillar 2 — Podcast circuit

One 90-minute podcast appearance is worth ~50,000 to ~500,000 impressions on a
mid-to-large crypto/AI show, evergreen for years, and the audience is already
filtered for relevance. ROI on operator hours is the highest of the three
pillars *if* the pitch lands.

### Pitch deck (one page, sent inline)

The pitch is one paragraph + three bullet points. No PDF, no Calendly link in
the cold email — those signal "marketing person" and route to spam.

```
Subject: AI trading agents that improve themselves overnight — pitch for <show>

Hi <host>,

I'm building xvision — non-custodial AI trading agents that run on Orderly +
Mantle, with an overnight autoresearcher that mutates strategies, evaluates
variants against held-out backtests, and seals survivors as ERC-8004 lineage
NFTs. There are three threads in here I'd love to talk through on <show>:

- Why "non-custodial" matters more than people think — what happens when the
  trading platform can place orders but can't withdraw, and why this changes
  the FTX failure mode at the substrate level.

- The autoresearcher: how an LLM judge avoids overfitting in a domain where
  every wrong call costs real money. (Spoiler: held-out windows, lineage
  attestation, kill-the-parent-if-the-child-wins.)

- ERC-8004 in production: portable reputation as the actual product, not the
  marketing. What happens when an agent's track record is on-chain.

I can demo a live overnight run if useful. Pre-recorded fallback available.

— <operator name>
```

### Target list (tiered by reach + fit)

**Tier 1 — high fit, large reach. Start here.**

- **Bankless** (Ryan Adams + David Hoffman) — DeFi-first, deep on infrastructure narratives. Pitch: non-custodial trust model.
- **Empire** (Jason Yanowitz + Santiago Santos) — macro + alpha-seeking. Pitch: alpha generation as a market.
- **Unchained** (Laura Shin) — investigative + skeptical. Good for the "what could go wrong" segment of the non-custodial story.
- **The Chopping Block** (Haseeb + Tom + Tarun + Robert) — extremely technical audience. Pitch: ERC-8004 + autoresearcher mechanics.
- **The Defiant Podcast** (Camila Russo) — DeFi natives, EU-leaning audience.

**Tier 2 — high fit, smaller reach. Build momentum.**

- **Onchain Things** — small + technical
- **The Mint Condition** — NFT-leaning; ERC-8004 angle resonates
- **Robot Overlordz** — AI + crypto crossover
- **The Block's "0xResearch"** — research-oriented
- **Crypto Sapiens** — long-form interviews

**Tier 3 — AI / non-crypto. Cross-pollinate.**

- **Latent Space** (swyx) — AI engineering audience. Pitch: LLM-as-judge in adversarial domains, autoresearcher loop.
- **MLST** (Machine Learning Street Talk) — technical depth, longer episodes.
- **Practical AI** — applied ML audience.
- **No Priors** (Sarah Guo + Elad Gil) — AI VC audience; not their typical guest profile but the autoresearcher narrative fits.

**Tier 4 — Mantle / Orderly / partner-ecosystem podcasts.** If they don't
have one, propose a one-off episode. Hackathon sponsors are often eager for
content.

### Workflow

1. Hit list (15–20 shows) in a spreadsheet with host, show name, last
   relevant episode, contact, status, follow-up date.
2. One outreach batch per week — 4–5 cold emails. Personal, references a
   recent episode, never feels templated. Reuse the pitch above but vary the
   thread emphasis per host's interests.
3. When a yes lands: 30-min prep call → record → operator gets ~2 weeks of
   passive distribution per episode.
4. Re-purpose every appearance: 60-second highlight clip rendered via
   Remotion (re-uses the video pipeline), pinned-tweet thread, podcast
   transcript pasted into the project's `marketing/` archive.

### Topic threads to develop

Reusable narrative arcs the operator can pull from in any interview.
Refine each into a polished 3–5 minute version before the first recording so
the talking points are tight.

| Thread | Hook | Where it leads |
|---|---|---|
| Non-custodial trust | "What if your trading bot literally couldn't take your money?" | Scope-enforced trading keys; broker-layer enforcement; failure mode contrast with custodial platforms |
| AI judges in adversarial domains | "Why most LLM evaluation is wrong for trading" | Held-out windows; survivorship bias; lineage attestation; kill-the-parent rule |
| On-chain reputation | "Strategy track records that move between platforms" | ERC-8004 mechanics; portability across marketplaces; what governance looks like |
| Autoresearch as a product | "What if your trading bot wrote better trading bots overnight?" | Mutation manifold; cost economics; the operator-load reduction story |
| What scales break at | "The N=10 problem" | MANUAL.md's scale tier section; operational maturity vs. capital scaling |

---

## Pillar 3 — Crypto Discord communities

Discord is where prospective users *debate which bots to trust* before they
deposit a dollar. Showing up technically, consistently, and helpfully in the
right servers compounds. This is the slowest pillar to ramp and the
highest-trust output.

### Target servers (Tier-1, mandatory presence)

| Server | Why we're there | Engagement mode |
|---|---|---|
| **Mantle Discord** | Hackathon-native; our chain. Sponsor relationship + judges live here. | Daily presence in #general, #builders, #hackathon-projects. Submit build-in-public threads. |
| **Orderly Network Discord** | Our perps execution rail. | Active in #dev-support, #strategy-chat. Surface bugs, propose integrations. |
| **ERC-8004 / Identity Working Group** | The substrate our reputation claims sit on. | Technical contributions to the spec discussion; reference our deploy as a live use case. |

### Target servers (Tier-2, active presence)

- **CrunchDAO / Numerai / NuminAI** — quant-leaning crypto, ML for trading
- **MetricsDAO** — analytics-leaning, good for performance attestation discussions
- **Mantle MetaTraders / Aevo / Hyperliquid trader communities** — perps-strategy traders are our target user
- **DeFi research communities** — Index Coop research, Bankless DAO research guild
- **AI x crypto crossover servers** — varies; new ones appearing constantly

### Target servers (Tier-3, monitor only)

Don't post; read the room. Listening posts to understand what trading-bot
buyers actually argue about. Notes go in `marketing/discord-pulse.md`
(create as needed). Examples: r/algotrading-adjacent Discords, generic
"crypto trading" servers that pattern-match to scam shilling.

### Engagement model — what to actually do

**Don't:**
- Spam the project link in #general
- DM users to "check out my bot"
- Use any "act friendly to get traction" account
- Post performance numbers without a verifiable `run_id` + audit-log pointer

**Do:**

1. **Answer technical questions.** Every server has people asking about LLMs
   for trading, perp risk management, scope-enforced keys, ERC-8004 mechanics.
   Show up as the person who actually knows. Sign with operator handle,
   include relevant link only when it directly answers the question.
2. **Publish build-in-public threads.** One-per-week update in target
   server's #showcase or #builders. Include a `run_id` + audit-log
   pointer + the actual code change. Verifiability is the differentiator.
3. **Host AMAs.** Once visibility is established (~6 weeks of consistent
   presence), propose an AMA. 90 minutes, technical depth, no marketing
   slides. Re-render the highlights as Remotion videos.
4. **Sponsor / partner relationships.** Mantle and Orderly have official
   Builder / Partner tracks. Apply early — gives access to a "Verified"
   badge that short-circuits the trust check.
5. **Ship a Discord bot integration.** A small `xvision-discord-bot` that
   responds to `/strategy lookup <agent_id>` with the on-chain attestation
   + recent performance. Demonstrates the portability claim concretely.
   Build it after the engine API stabilizes.

### Operator hours allocation

Realistic budget for Pillar 3:

| Activity | Time/week |
|---|---|
| Tier-1 server presence (read + respond) | 3 hrs |
| Build-in-public weekly thread (write + post) | 1 hr |
| Tier-2 server engagement | 1 hr |
| Pulse-check on Tier-3 servers | 0.5 hr |
| **Total** | **~5.5 hrs/week** |

Goes up to ~10 hrs/week during AMA + hackathon weeks.

---

## Cross-cutting

### Asset inventory — what every pillar can reuse

Everything below is owned by `marketing/` once produced. Versioned, indexed,
not regenerated from scratch each time.

| Asset | Lives at | Reused in |
|---|---|---|
| Brand tokens (colors, type, spacing) | `frontend/web/src/styles/tokens.css` (imported into Remotion) | Video, slide decks, social cards |
| Voiceover persona config | `marketing/voices/<persona>.json` | Every Remotion video |
| Run-summary template | `marketing/templates/run-summary.tsx` | Pillar 1 (daily); Pillar 3 (showcase posts) |
| Podcast pitch | `marketing/pitch/cold-email.md` (this file's section above) | Pillar 2 |
| One-pager | `docs/HACKATHON-1-PAGER.md` | Pillar 2, Pillar 3 onboarding |
| Topic threads | this file's Pillar 2 § "Topic threads" | Pillar 2 (interviews), Pillar 1 (concept-explainer videos) |
| Live demo script | `marketing/demos/live-overnight-run.md` (create when first needed) | Pillar 2 (podcast demos), Pillar 3 (AMAs) |

### Voice consistency check

Three rules every output passes through before publish:

1. **Numbers are specific.** "Saved 3.2% on funding drag" beats "saved a lot
   on funding drag."
2. **Risks are named.** Every pitch, video, and thread acknowledges what
   could go wrong. Strategies overfit; risk gates miss; non-custodial doesn't
   mean risk-free. README's alpha warning is the canonical phrasing.
3. **Verifiability is one link away.** Every performance claim has a
   `run_id` + audit-log link + (when applicable) an ERC-8004 attestation
   hash. If a claim can't be verified, it doesn't ship.

---

## Phased rollout

**Phase 0 — foundations (weeks 1–2)**

- Create `marketing/` folder structure: `templates/`, `voices/`, `pitch/`,
  `demos/`, `discord-pulse.md`.
- First Remotion template (`run-summary.tsx`) end-to-end against a real
  `BacktestResult`. Get to a 60-second mp4 that passes the voice
  consistency check.
- Tier-1 Discord presence established — operator joins all three servers,
  reads a week before posting.
- Podcast hit list spreadsheet populated; first 5 cold emails drafted (not
  sent yet).

**Phase 1 — pipeline shakedown (weeks 3–6)**

- First video shipped to YouTube + Twitter. Iterate based on feedback for two
  cycles.
- First 2–3 podcast outreach sends. Goal: 1 confirmation by week 6.
- First build-in-public thread in Mantle Discord.

**Phase 2 — cadence (weeks 7–12)**

- 2 videos/week.
- 1–2 podcast appearances per month.
- Weekly build-in-public threads across Tier-1 servers.
- First AMA proposal lands (Mantle or Orderly).

**Phase 3 — leverage (weeks 13+)**

- 3–5 videos/week, partially automated.
- Hosted on a project YouTube channel with consistent thumbnails + playlists.
- Podcast cadence steady; founder is becoming a known voice in the space.
- Discord bot ships, demonstrating portability claim.
- First user-generated videos appear (users running their own xvision
  instances re-render their own run summaries with our template).

---

## Metrics — what we actually track

Ranked by signal quality (not by what's easy to measure).

**Tier 1 — load-bearing signals.** Improvement here means we're winning.

- **Deposit conversions to non-testnet** — N first-time users moving past
  testnet to a funded Alpaca paper or Orderly testnet account
- **GitHub stars from technical accounts** (not coinbots) — proxy for
  developer reach
- **PR / issue submissions from non-team contributors** — strongest signal of
  community traction
- **Returning users in Discord** — same handles asking questions over weeks
  is a stronger signal than one-time joins

**Tier 2 — supporting signals.** Useful, but easy to game.

- Video views, watch-time, retention curve
- Podcast download counts (when available)
- Twitter / X impressions on linked content
- Discord member count

**Tier 3 — vanity.** Track for trend lines, don't optimize for.

- Total YouTube subscribers
- Follower counts
- Hackathon mentions

### Quarterly review

Each quarter, the operator writes a one-page review:

- What worked (with attribution to pillar where possible)
- What flopped (and the operator's read on why)
- What's the bottleneck for next quarter
- What we drop

This file is updated based on those reviews — the methods evolve.

---

## Open questions for the operator

1. **Voice persona.** Cloned founder voice (more authentic, narrower brand
   risk if the founder later distances from the project) vs. a stable
   11labs library voice (more flexible across topics, less personal).
   Default proposal: library voice for run-summary / data videos; founder
   voice for concept-explainer videos.
2. **YouTube channel branding.** Solo brand (`xvision`) vs. operator-led
   brand (`<founder name>'s xvision`). Default proposal: project brand
   first; founder presence via podcast appearances.
3. **Discord persona policy.** Single operator handle vs. team handles
   (when team scales). Default proposal: one operator handle for now;
   add team handles only with explicit role disclosure.
4. **Pre-launch vs. post-launch sequencing.** Some of this content (run
   summaries with real `run_id`s) requires the v1 vertical slice to be
   demoable end-to-end. The pillars 2 + 3 work can start earlier (build-in-
   public threads, podcast pitches against the architectural narrative
   even before the product is shippable to users). Default proposal:
   Pillar 1 starts at v1 ship; Pillars 2 + 3 start now.
