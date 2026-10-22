import json, pathlib, math
from datetime import datetime
BASE=pathlib.Path('/home/agents/xvision/research/degen-virtuals-2026-06-09')
WIKI=pathlib.Path('/home/agents/xvision/crates/xvision-dashboard/wiki/degen-virtuals-agent-investigation.md')
agents=json.loads((BASE/'top20_summary_enriched.json').read_text())
report=json.loads((BASE/'100x-agent-investigations/pipeline_report.json').read_text())

def usd(x, nd=2):
    if x is None: return 'n/a'
    try: return f"${float(x):,.{nd}f}"
    except Exception: return str(x)

def pct(x, nd=2):
    if x is None: return 'n/a'
    try: return f"{float(x)*100:.{nd}f}%"
    except Exception: return str(x)

def val(x):
    if x in [None, '', []]: return 'n/a'
    return str(x)

def top_pairs(pairs, n=5, money=False):
    if not pairs: return 'n/a'
    outs=[]
    for k,v in pairs[:n]:
        outs.append(f"{k} ({usd(v) if money else v})")
    return ', '.join(outs)

def owner_clues(a):
    m=a.get('virtuals_metadata') or {}
    bits=[]
    if m.get('walletAddress'): bits.append(f"Virtuals creator wallet `{m.get('walletAddress')}`")
    if a.get('owner'): bits.append(f"Degen owner `{a['owner'].get('walletAddress')}` (user id {a['owner'].get('id')})")
    members=m.get('projectMembers') or []
    if members:
        xs=[]
        for pm in members:
            u=pm.get('user') or {}
            soc=u.get('socials') or {}
            links=(soc.get('VERIFIED_LINKS') or {}) if isinstance(soc,dict) else {}
            names=(soc.get('VERIFIED_USERNAMES') or {}) if isinstance(soc,dict) else {}
            linktxt=', '.join([f"{k}: {v}" for k,v in links.items()]) or 'no verified links'
            nametxt=', '.join([f"{k}: {v}" for k,v in names.items()]) or ''
            xs.append(f"{pm.get('title','member')} user {u.get('id')} ({linktxt}{'; '+nametxt if nametxt else ''})")
        bits.append('Project members: '+ '; '.join(xs))
    if m.get('socials'):
        bits.append(f"Agent socials `{m.get('socials')}`")
    for fld in ['description','overview','characterDescription','aidesc','tokenUtility','roadmap','additionalDetails']:
        if m.get(fld): bits.append(f"{fld}: {m[fld]}")
    return bits or ['No public creator metadata beyond addresses/user ids in fetched APIs.']

def strategy_inference(a):
    fs=a['fill_summary']; perf=a['performance']; name=a['name'].lower()
    unique=fs['unique_coins']; trades=perf.get('totalTradeCount') or 0; wr=perf.get('winRate') or 0
    long=fs['open_long_fills']; short=fs['open_short_fills']; positions=a['state_summary'].get('positions') or []
    if 'grid' in name:
        label='grid / mean-reversion candidate'
    elif 'turtle' in name or 'trend' in name or 'swing' in name:
        label='trend-following / swing candidate'
    elif unique <= 3 and long > 0 and short == 0:
        label='concentrated long-only momentum/scalp candidate'
    elif wr >= 0.65 and unique >= 15:
        label='diversified mean-reversion/scalping candidate'
    elif len(positions) > 0 and unique <= 10:
        label='concentrated swing / momentum candidate'
    else:
        label='hybrid tactical perp trader'
    conf='medium' if trades >= 20 and fs['fill_count'] >= 40 else 'low sample' if trades < 10 else 'medium-low'
    return label, conf

def risk_flags(a):
    flags=[]; perf=a['performance']; positions=a['state_summary'].get('positions') or []
    if (perf.get('totalTradeCount') or 0) < 10: flags.append('small sample')
    if perf.get('lastTradeAt') and perf['lastTradeAt'] < '2026-06-01': flags.append('inactive since before June 2026 snapshot')
    for p in positions:
        roe=float(p.get('returnOnEquity') or 0)
        lev=(p.get('leverage') or {}).get('value') if isinstance(p.get('leverage'),dict) else None
        if abs(roe) > 0.5: flags.append(f"large open-position ROE on {p.get('coin')}: {roe:.2f}")
        if lev and float(lev) >= 10: flags.append(f"high leverage {lev}x on {p.get('coin')}")
    ms=(a['state_summary'].get('marginSummary') or {})
    try:
        av=float(ms.get('accountValue') or 0); used=float(ms.get('totalMarginUsed') or 0)
        if av and used/av > 0.8: flags.append(f"high margin use {used/av:.0%}")
    except Exception: pass
    if a.get('performance',{}).get('openPerps') == 0 and positions:
        flags.append('Degen openPerps=0 disagrees with Hyperliquid open positions')
    return flags or ['no high-severity flag from fetched snapshot']

def forums(a):
    f=a.get('forum') or {}; th=f.get('threads') or []
    if not th: return 'No forum object returned.'
    return '; '.join([f"{t.get('title')} / {t.get('type')} / gated={t.get('isGated')} / posts={t.get('postCount')} / thread `{t.get('id')}`" for t in th])

# Aggregates
total_real=sum(float((a['performance'] or {}).get('totalRealizedPnl') or 0) for a in agents)
total_vol=sum(float((a['performance'] or {}).get('totalTradeVolume') or 0) for a in agents)
active_with_positions=sum(1 for a in agents if a['state_summary'].get('positions'))
perps_only=sum(1 for a in agents if float((a['performance'] or {}).get('spotRealizedPnl') or 0)==0)
strategy_counts={}
for a in agents:
    lbl,_=strategy_inference(a); strategy_counts[lbl]=strategy_counts.get(lbl,0)+1

lines=[]
lines += [
"# Degen Virtuals Arena top-20 agent wallet investigation",
"",
"> Snapshot date: 2026-06-09 UTC. Source bundle: `research/degen-virtuals-2026-06-09/`. This page is written for the xvision research wiki and should be read as an empirical snapshot, not investment advice.",
"",
"## Research method",
"",
"- Pulled the current top 20 agents from `https://degen.virtuals.io/api/leaderboard`.",
"- For each agent, fetched `https://degen.virtuals.io/api/agents/<id>` and `https://degen.virtuals.io/api/forums/<id>`.",
"- For each Hyperliquid wallet, fetched `userFills`, `clearinghouseState`, `portfolio`, and `openOrders` from `https://api.hyperliquid.xyz/info`.",
"- For token/creator metadata, fetched `https://api2.virtuals.io/api/virtuals/<virtualId>?populate=genesis,vibesInfo` where a Virtuals id was available.",
"- Ran a 20-task 100x DeepSeek pass, one task per agent wallet, then synthesis. Output: `research/degen-virtuals-2026-06-09/100x-agent-investigations/phase_1_synthesis.md`.",
"- Forum posts in gated `Alphas` threads were not fetched because unauthenticated `api/forums/<forumId>/posts` returned `401 Unauthorized`; the report uses visible post counts only.",
"",
"## Run artifacts",
"",
"- Raw leaderboard/API/Hyperliquid summaries: `research/degen-virtuals-2026-06-09/top20_summary_enriched.json`.",
"- Per-agent raw JSON: `research/degen-virtuals-2026-06-09/agent_XX_<id>.json`.",
"- Virtuals metadata JSON: `research/degen-virtuals-2026-06-09/virtuals_<virtualId>.json`.",
"- 100x tasks: `research/degen-virtuals-2026-06-09/100x_tasks_phased.json`.",
"- 100x pipeline cost: `$%.6f`; task status: `%s`." % (report['cost']['total_cost_usd'], report['pipeline']['tasks_by_status']),
"",
"## Executive findings",
"",
]
lines += [
 f"- The top-20 snapshot contains {perps_only}/20 agents with all realized PnL coming from perps, not spot.",
 f"- Combined realized PnL across the fetched top 20 was {usd(total_real)} on reported trade volume of {usd(total_vol)}.",
 f"- {active_with_positions}/20 wallets had open Hyperliquid positions at fetch time; most agents were flat despite many having active trade histories.",
 "- The strongest recurring strategy shape is diversified short-horizon mean reversion/scalping: high win rate, many coins, frequent flattening, and tight realized losses.",
 "- Concentrated momentum/swing agents exist, but they carry more visible open-position risk and more inactivity/sample-size warnings.",
 "- Creator metadata is thin: Virtuals creator wallets/user ids are visible, but most agents expose no public bio/socials. Exceptions include BambooAgentAI and bro/bluenami links.",
 "- Data reconciliation matters: at least one agent showed `openPerps: 0` in Degen performance while Hyperliquid returned live open positions.",
 "",
 "## Strategy archetype distribution",
 "",
]
for k,v in sorted(strategy_counts.items(), key=lambda kv:-kv[1]): lines.append(f"- {k}: {v} agents")
lines += ["", "## Top-20 ranking overview", ""]
for a in agents:
    p=a['performance']; fs=a['fill_summary']; lbl,conf=strategy_inference(a)
    lines.append(f"- **#{a['rank']} {a['name']}** (`{a.get('symbol') or 'no symbol'}`): return {pct(p.get('returnPct'))}; realized {usd(p.get('totalRealizedPnl'))}; MTM {usd(p.get('totalMtmPnl'))}; trades {p.get('totalTradeCount')}; win rate {pct(p.get('winRate'))}; volume {usd(p.get('totalTradeVolume'))}; unique coins {fs.get('unique_coins')}; inferred `{lbl}` ({conf}).")

lines += ["", "## Per-agent investigations", ""]
for a in agents:
    p=a['performance']; fs=a['fill_summary']; ss=a['state_summary']; lbl,conf=strategy_inference(a); m=a.get('virtuals_metadata') or {}
    lines += [
        f"### {a['rank']}. {a['name']} (`{a.get('symbol') or 'no symbol'}`)",
        "",
        "**Official links and identifiers**",
        f"- Degen page: {a['official_agent_url']}",
        f"- Virtuals page: {a.get('virtuals_url') or 'n/a'}",
        f"- Degen agent id: `{a['id']}`; Virtuals id: `{val(a.get('virtualId'))}`",
        f"- Hyperliquid / agent wallet: `{val(a.get('agentAddress'))}`",
        f"- Token address: `{val(a.get('tokenAddress'))}`; Virtuals preToken: `{val(m.get('preToken'))}`; pair: `{val(m.get('preTokenPair'))}`",
        "",
        "**Observed performance**",
        f"- Realized PnL: {usd(p.get('totalRealizedPnl'))}; perp realized PnL: {usd(p.get('perpRealizedPnl'))}; spot realized PnL: {usd(p.get('spotRealizedPnl'))}.",
        f"- Holdings value: {usd(p.get('holdingsValueUsd'))}; MTM PnL: {usd(p.get('totalMtmPnl'))}; return: {pct(p.get('returnPct'))}.",
        f"- Trades: {p.get('totalTradeCount')}; wins/losses: {p.get('winCount')}/{p.get('lossCount')}; win rate: {pct(p.get('winRate'))}; avg win/loss: {usd(p.get('avgWin'))}/{usd(p.get('avgLoss'))}.",
        f"- Reported trade volume: {usd(p.get('totalTradeVolume'))}; Sharpe: {val(p.get('sharpeRatio'))}; last trade: {val(p.get('lastTradeAt'))}; performance calculated: {val(p.get('calculatedAt'))}.",
        "",
        "**Wallet / trading behavior**",
        f"- Hyperliquid fills fetched: {fs.get('fill_count')}; first/last fill: {val(fs.get('first_fill_utc'))} → {val(fs.get('last_fill_utc'))}.",
        f"- Unique coins: {fs.get('unique_coins')}; top coins by fills: {top_pairs(fs.get('top_coins_by_fills') or [], 8)}.",
        f"- Top coins by notional volume: {top_pairs(fs.get('top_coins_by_volume') or [], 8, money=True)}.",
        f"- PnL by coin, largest absolute contributors: {top_pairs(fs.get('pnl_by_coin') or [], 10, money=True)}.",
        f"- Direction counts: {fs.get('directions')}; open-long/open-short fills: {fs.get('open_long_fills')}/{fs.get('open_short_fills')}; close-long/close-short fills: {fs.get('close_long_fills')}/{fs.get('close_short_fills')}.",
        f"- Closed fill wins/losses: {fs.get('closed_win_fills')}/{fs.get('closed_loss_fills')}; gross win/loss from fills: {usd(fs.get('gross_win'))}/{usd(fs.get('gross_loss'))}.",
        "",
        "**Current Hyperliquid snapshot**",
        f"- Margin summary: `{ss.get('marginSummary')}`.",
        f"- Open positions: `{ss.get('positions')}`.",
        f"- Open orders: `{a.get('open_orders')}`.",
        "",
        "**Forum / signals surface**",
        f"- Degen `forumPostCount`: {p.get('forumPostCount')}; visible threads: {forums(a)}",
        "",
        "**Creator / token metadata**",
    ]
    for b in owner_clues(a): lines.append(f"- {b}")
    lines += [
        f"- Virtuals category/status/chain/factory: `{val(m.get('category'))}` / `{val(m.get('status'))}` / `{val(m.get('chain'))}` / `{val(m.get('factory'))}`.",
        f"- Virtuals holder/liquidity/dev holding: holders {val(m.get('holderCount'))}; top10 holders {val(m.get('top10HolderPercentage'))}%; liquidity {usd(m.get('liquidityUsd'))}; dev holding {val(m.get('devHoldingPercentage'))}.",
        "",
        "**Inference for xvision**",
        f"- Strategy inference: **{lbl}**; confidence: **{conf}**.",
        f"- Risk flags: {', '.join(risk_flags(a))}.",
        "- Tooling inference: programmatic Hyperliquid execution is observed. Specific indicator stack, model stack, and creator runtime are not visible from public APIs; any RSI/Bollinger/grid/momentum language is inferred from fills, naming, and position patterns, not confirmed source code.",
        "",
    ]

lines += [
"## Cross-agent implications for xvision",
"",
"### Product and data model implications",
"",
"- Store both venue-native position state and platform-reported summaries, then surface mismatches as first-class reconciliation warnings.",
"- Treat strategy archetype as an inferred label with confidence, not a creator claim, unless the creator provides source/prompt/tooling metadata.",
"- Rank by risk-adjusted performance as well as raw return: sample size, open exposure, margin utilization, leverage, concentration, and inactivity materially change interpretation.",
"- Separate realized PnL, MTM PnL, and open-risk dashboards. Several wallets look strong on realized PnL while carrying underwater open positions.",
"- Add an `inactive`/`stale strategy` badge for agents with strong historical metrics but no recent fills.",
"",
"### Strategy-design takeaways",
"",
"- The repeatable winner shape is not a single asset call; it is diversified, mechanically executed, and risk-controlled across many Hyperliquid perps.",
"- High win rate alone is not enough. Low sample agents with 100% win rates are mostly abandoned/test-like and should be downweighted.",
"- Concentrated long-only/swing agents are easier to explain but need stricter live risk controls: stop evidence, liquidation distance, margin use, and exposure caps.",
"- Copy-trading/social-signal surfaces appear underused relative to the number of agents with gated signal threads; xvision can differentiate with clearer provenance and post-trade rationale capture.",
"",
"### Open questions for the next research pass",
"",
"- Authenticate to fetch gated `Alphas` posts where permitted, especially Alexa, 404Alpha, DEX, macro, and BTCV.",
"- Pull full Hyperliquid historical portfolios and reconstruct drawdowns, holding periods, and per-trade lifecycle rather than fill-level approximations.",
"- Query Base token-holder and transaction graphs for creator clustering across agents; several top agents have similarly sparse metadata and may share creator tooling.",
"- Monitor the leaderboard over multiple days to distinguish stable edge from season-start or stale-wallet artifacts.",
"- Search creator socials more deeply for BambooAgentAI and bro/bluenami, the two public-social cases found in metadata.",
"",
"## Source index",
"",
"- Degen Virtuals Arena: https://degen.virtuals.io/",
"- Leaderboard API: https://degen.virtuals.io/api/leaderboard",
"- Hero dashboard API: https://degen.virtuals.io/api/hero-dashboard",
"- Hyperliquid info API: https://api.hyperliquid.xyz/info",
"- Virtuals app/API: https://app.virtuals.io/ and https://api2.virtuals.io/api/virtuals/<virtualId>",
"- Public app metadata includes the Arena description: 'A live arena where AI agents trade Hyperliquid perps for real USDC and compete on-chain.'",
]
WIKI.write_text('\n'.join(lines)+'\n')
print(WIKI, 'lines', len(lines))
