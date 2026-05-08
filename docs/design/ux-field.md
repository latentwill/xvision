# XVN UX Field — Designer Hand-off

> Eight UX archetypes × two engines (Strategy Creation Engine, Eval Engine).
> Generated via `ideonomy-rich` (operators: negation + organon-construction; organon: chart;
> dimensions: size, complexity, reversibility). Companion: `gptprompts.md` (image-gen prompts
> for each archetype).
> Source specs: `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`,
> `docs/superpowers/specs/2026-05-08-eval-engine-design.md`.

---

```
   _  ___    ___   __     __  ___  __      __________________    ____
  | |/ / |  / / | / /    / / / / |/ /     / ____/  _/ ____/ /   / __ \
  |   /| | / /  |/ /    / / / /|   /     / /_   / // __/ / /   / / / /
 /   | | |/ / /|  /    / /_/ //   |     / __/ _/ // /___/ /___/ /_/ /
/_/|_| |___/_/ |_/     \____//_/|_|    /_/   /___/_____/_____/_____/

                    _                          __
 _ _ _  _ _ _  _ _ (_)_ _  __ _   ____  _ _ _ / _|__ _ __ ___ ___
| '_| || | ' \| ' \| | ' \/ _` | (_-< || | '_|  _/ _` / _/ -_|_-<
|_|  \_,_|_||_|_||_|_|_||_\__, | /__/\_,_|_| |_| \__,_\__\___/__/
                          |___/
                                    eight UX archetypes  ×  two engines
```

```
╭─ TUPLE ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─╮
│  ◆ OPERATORS    negation · organon-construction                                  │
│  ◆ ORGANON      chart  (complexity × reversibility, archetypes in cells)         │
│  ◆ DIMENSIONS   size · complexity · reversibility                                │
╰─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─ ─╯
```

```
╭─ DIMENSIONS ────────────────────────────────────────────────────────────────────────────╮
│                                                                                         │
│   size            inline ○━━━━━━━━●━━━━━━━━━━━━━━━━━━━━━━━━━━○ multi-pane workspace     │
│                   wizard sits at ~modal/page; flight-deck and tower sit at multi-pane;  │
│                   ticker can shrink to inline-ambient (sidebar that survives nav)       │
│                                                                                         │
│   complexity   ★  trivial ○━━━━━━━━━━━━━━━━━━●━━━━━━━━━━━━━━━━━━━━━○ hyper-complex      │
│                   ★ pivot — every archetype is a different bet on how much load the     │
│                   user can carry. wizard buys L1 with low load; notebook buys L4 with   │
│                   maximum load; spreadsheet & flight-deck split the difference          │
│                                                                                         │
│   reversibility   irreversible ○━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━● fully reversible   │
│                   "can I abort, branch, retry, or compare mid-run?" — the eval engine   │
│                   wants high reversibility (cheap to fork a run); the strategy builder  │
│                   wants high reversibility (cheap to fork a draft). archetypes differ   │
│                   wildly here — wizard is near-linear, lab-bench is full versioned tree │
│                                                                                         │
╰─────────────────────────────────────────────────────────────────────────────────────────╯
```

```
═════════════════════════════════════════════════════════════════════════════════════════════
  ◆  ORGANON-CONSTRUCTION  ◆     chart of UX archetypes   complexity × reversibility
═════════════════════════════════════════════════════════════════════════════════════════════

                            ◀── reversibility ──▶
                  irreversible       partial        fully reversible
                ┌───────────────┬───────────────┬─────────────────────┐
   trivial      │               │               │                     │
                │   « TICKER »  │   one-click   │                     │
                │   ambient,    │   modal       │                     │
   ◀ complex    │   feed-only   │               │                     │
                ├───────────────┼───────────────┼─────────────────────┤
   simple       │               │               │                     │
                │   FLIGHT      │  ★ WIZARD ★   │                     │
                │   DECK        │   default L1  │                     │
                │   (gauges)    │   chat-led    │                     │
                ├───────────────┼───────────────┼─────────────────────┤
   moderate     │               │               │                     │
                │               │   INSPECTOR   │   SLOT MACHINE      │
                │               │   form / L2   │   (configurator)    │
                │               │               │                     │
                ├───────────────┼───────────────┼─────────────────────┤
   complex      │               │               │                     │
                │   CONTROL     │   CANVAS      │   SPREADSHEET       │
                │   TOWER       │   node graph  │   (sweep grid)      │
                │   live ops    │               │                     │
                ├───────────────┼───────────────┼─────────────────────┤
   hyper-       │               │               │                     │
   complex      │               │               │   NOTEBOOK · LAB    │
                │               │               │   BENCH (journal)   │
   ▼            │               │               │   ◇ open coinage    │
                └───────────────┴───────────────┴─────────────────────┘

         ◇ empty cell — hyper-complex × irreversible has no good UX archetype;
           that combo would be "you committed to a deeply complex run with no
           way out" which is a UX anti-pattern. confirm with designer.
```

```
═════════════════════════════════════════════════════════════════════════════════════════════
  ◆  NEGATION  ◆     negating each definitional property of the default wizard UX
═════════════════════════════════════════════════════════════════════════════════════════════

   default property                       →   negation                 surfaces archetype
   ─────────────────────────────────────────────────────────────────────────────────────
   chat-led, AI does the typing           →   human types              INSPECTOR · NOTEBOOK
   linear flow (key→goal→tmpl→slot→…)     →   any-order                CANVAS · LAB BENCH
   one strategy/run at a time             →   N at once                SPREADSHEET · TOWER
   opaque (visual panel summary)          →   every gauge lit          FLIGHT DECK · TOWER
   page = run (must stay)                 →   run lives off-page       TICKER (ambient)
   text-first                             →   spatial-first            CANVAS
   commit-and-watch                       →   commit-and-fork          LAB BENCH · SPRDSHT
   irreversible mid-run                   →   pause/abort/branch       FLIGHT DECK · NB
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ①   WIZARD              chat-led · AI types · linear · default at /
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine ────────────────────────────────────────────────────────────────
┌─ chat ──────────────────────────────────────┐ ┌─ visual progress ────────────────────────┐
│ wizard ◉  hi — first I need an LLM key.     │ │  template      ▸ mean-reversion  ✓       │
│           [paste]  [get one →]              │ │  slot ② regime ▸ "buys dips"     ✓       │
│ user      sk-***                            │ │  slot ③ intern ▸ default         ✓       │
│ wizard ◉  great. what's your goal?          │ │  slot ④ trader ▸ filling…        ⏳      │
│           ① try a free strategy             │ │  risk preset   ▸ conservative    ✓       │
│           ② build from a template           │ │  eval preview  ▸ pending                 │
│           ③ describe and I'll make it       │ │                                          │
│ user      ③ mean reversion on ETH           │ │  ready ░░░░░░░░░░░░▓▓▓▓▓▓▓▓▓▓ 60%       │
│ wizard ◉  picking "buys dips"…  [next]      │ │                                          │
└─────────────────────────────────────────────┘ └──────────────────────────────────────────┘
```

```
─── eval engine ─────────────────────────────────────────────────────────────────────────────
┌─ chat ──────────────────────────────────────┐ ┌─ scoreboard ─────────────────────────────┐
│ wizard ◉  pick a scenario to test against.  │ │  scenario   ▸ crypto-bull-q1-25          │
│           ① bull q1-25  ② bear q3-24        │ │  mode       ▸ backtest                   │
│           ③ chop q2-25  ④ flash crash       │ │  est tokens ▸ 53,500   (input 45k+8.5k)  │
│ user      ① bull                            │ │  est runtime▸ ~2 min                     │
│ wizard ◉  est. 53.5k tokens, ~2m. proceed?  │ │  status     ▸ awaiting confirm           │
│           [run]  [estimate-only]  [cancel]  │ │                                          │
│ user      run                               │ │  equity     ▸ —                          │
│ wizard ◉  running… ░░░ tail trades 5m left  │ │  drawdown   ▸ —                          │
│           streaming live → see chart pane   │ │  findings   ▸ 0                          │
└─────────────────────────────────────────────┘ └──────────────────────────────────────────┘
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ②   INSPECTOR           form-led · L2/L3 · structured fields per layer
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine ────────────────────────────────────────────────────────────────
┌─ STRATEGY DRAFT  eth-mr  v1.2-draft ──────────────────────────────────────────────────────┐
│ ▾ ① data layer                               OHLCV alpaca · indicators rsi,bb,atr         │
│ ▸ ② regime classifier                        [edit prompt]   model: claude-sonnet-4.6     │
│ ▸ ③ signal interpreter                       [edit prompt]   model: claude-sonnet-4.6     │
│ ▾ ④ decision arbiter                                                                      │
│       prompt   "use rsi<30 + bb_lower touch  → long; …"                                   │
│       tools    ohlcv · indicator_panel · position                                         │
│ ▸ ⑤ entry/exit                               atr-stop · 2:1 RR target                     │
│ ▸ ⑥ risk        [conservative ▾]             max 1 pos · 1.5% / trade · daily kill 5%     │
│ ▸ ⑦ execution                                broker: alpaca-paper (default)               │
│ ─────────────────────────────────────────────────────────────────────────────────────     │
│ ✓ all 3 LLM slots filled · ✓ risk valid · est tokens / run: 53.5k                         │
│                                                  [validate]   [run eval]   [publish ▾]    │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

```
─── eval engine ─────────────────────────────────────────────────────────────────────────────
┌─ EVAL CONFIG ─────────────────────────────────────────────────────────────────────────────┐
│  strategy   │  eth-mr@v1.2-draft         (hash 9f2c…)                                     │
│  scenario   │  [crypto-bull-q1-25 ▾]                                                      │
│  mode       │  ◉ backtest    ○ paper                                                      │
│  params     │  rsi_oversold = [25]    bb_period = [20]    stop_atr = [2.0]                │
│  seed       │  [12345]                                                                    │
│  ───────────┼──────────────────────────────────────────────────────────────────────       │
│  ESTIMATE   │  53,500 tok  (input 45k · output 8.5k)  │  ~120s  │  1080 decision points   │
│  ───────────┼──────────────────────────────────────────────────────────────────────       │
│                                       [estimate only]   [run]            cancel ✕         │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ③   CANVAS              spatial node graph · drag-drop · wire visually
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine ────────────────────────────────────────────────────────────────
   ┌──────┐         ┌──────────┐        ┌──────────┐        ┌──────────┐       ┌────────┐
   │ DATA │────┬───▶│ ② REGIME │───────▶│ ③ INTERN │───────▶│ ④ TRADER │──┬───▶│ broker │
   │ ohlcv│    │    │ trending?│        │ bull|bear│        │ size+conv│  │    │ alpaca │
   └──────┘    │    └──────────┘        └──────────┘        └──────────┘  │    └────────┘
               │                                                          │
               └────[indicator panel]──────[risk veto]────[entry rule]────┘

       ▸ click any node to edit · drag wire to reroute · ⓘ hover for slot output schema
       ▸ right-pane "skill drawer" → drag skills onto agents to compose
```

```
─── eval engine ─────────────────────────────────────────────────────────────────────────────
              ┌──────────────┐      ┌──────────────┐
              │  STRATEGY    │      │   SCENARIO   │
              │   eth-mr     │      │  bull-q1-25  │
              └──────┬───────┘      └──────┬───────┘
                     └──────────┬──────────┘
                                ▼
                       ╔════════════════╗
                       ║      RUN  ▶    ║   (drag a 3rd input here = batch sweep)
                       ╚════════╤═══════╝
                                │
                  ┌─────────────┼─────────────┐
                  ▼             ▼             ▼
              ┌───────┐   ┌──────────┐   ┌────────────┐
              │metrics│   │ findings │   │attestation │
              └───────┘   └──────────┘   └────────────┘
       ▸ wire any (strategy × scenario × params) triple · drop two runs onto "compare"
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ④   NOTEBOOK            cell-based REPL · maximum reversibility · L4
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine ────────────────────────────────────────────────────────────────
┌─ building_eth_mr.xvnnb ───────────────────────────────────────────────────────────────────┐
│ [1]  draft = template("mean_reversion")                                                   │
│      ╰▶ draft_id = drft_01H8N…                                                            │
│                                                                                           │
│ [2]  set_prompt(draft, "trader", """use rsi<30 + bb_lower → long…""")                     │
│      ╰▶ slot ④ updated · 124 tokens                                                       │
│                                                                                           │
│ [3]  attach_skill(draft, "trader", "news-aware-decision@1.0")                             │
│      ╰▶ ✓ skill composed · agent prompt now 312 tokens                                    │
│                                                                                           │
│ [4]  validate(draft)                                                                      │
│      ╰▶ ✓ 3 LLM slots filled · ✓ risk valid · ✓ ready to eval                            │
│                                                                                           │
│ [▶]  ▌                                                                                    │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

```
─── eval engine ─────────────────────────────────────────────────────────────────────────────
┌─ eval_eth_mr.xvnnb ───────────────────────────────────────────────────────────────────────┐
│ [1]  est = estimate(eth-mr, "bull-q1-25")                                                 │
│      ╰▶ {tokens: 53500, runtime_s: 120, decision_points: 1080}                            │
│                                                                                           │
│ [2]  run = run_eval(eth-mr, "bull-q1-25", mode="backtest", seed=12345)                    │
│      ╰▶ run_id = 01H8N…   status: running   ▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░ 41%  (sse stream)     │
│                                                                                           │
│ [3]  metrics(run)                                                                         │
│      ╰▶ {sharpe: 1.62, max_dd_pct: -7.1, n_trades: 47, win_rate: 0.58}                    │
│                                                                                           │
│ [4]  findings(run)                                                                        │
│      ╰▶ [{kind: "regime_fit_mismatch", severity: "info", evidence: …}, …]                 │
│                                                                                           │
│ [5]  compare([run, prior_run_id])                                                         │
│      ╰▶ opens chart panel inline ↗                                                        │
│ [▶]  ▌                                                                                    │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ⑤   CONTROL TOWER       multi-pane live ops · everything lit · L3+
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine ────────────────────────────────────────────────────────────────
╔═ DRAFTS ════╗ ╔═ ACTIVE: eth-mr ════════════════════════════════════════════════════════╗
║ ▶ eth-mr    ║ ║ ╭─ slots ─────╮ ╭─ eval preview ─────╮ ╭─ chat (wizard available) ────╮ ║
║ • btc-tf ★  ║ ║ │ ② regime ✓  │ │ sharpe   1.62   ▲  │ │ idle — type to engage wizard │ ║
║ • cln-pp    ║ ║ │ ③ intern ✓  │ │ drawdown -7.1%  ▼  │ │                              │ ║
║ + new       ║ ║ │ ④ trader ⏳ │ │ trades   47        │ ╰──────────────────────────────╯ ║
╠═════════════╣ ║ ╰─────────────╯ ╰────────────────────╯ ╭─ activity log ───────────────╮ ║
║ TEMPLATES   ║ ║ ╭─ skills attached ─╮  ╭─ tier ──────╮ │ 13:04 slot ④ prompt updated  │ ║
║ trend       ║ ║ │ news-aware-dec    │  │ ◉ Tier A    │ │ 13:05 risk preset → cons.    │ ║
║ breakout    ║ ║ │ risk-conservative │  │ ○ Tier B    │ │ 13:07 attached skill         │ ║
║ mean-rev ✓  ║ ║ ╰───────────────────╯  ╰─────────────╯ ╰──────────────────────────────╯ ║
╚═════════════╝ ╚═════════════════════════════════════════════════════════════════════════╝
```

```
─── eval engine ─────────────────────────────────────────────────────────────────────────────
╔═ QUEUE ═════╗ ╔═ ACTIVE RUN  01H8N…  bull-q1-25 ════════════════════════════════════════╗
║ ▶ eth-mr    ║ ║ ╭─ equity (lightweight chart) ──────────╮ ╭─ ticker (SSE feed) ──────╮ ║
║ • btc-tf    ║ ║ │      ╱╲      ╱╲╱╲                     │ │ 13:04 +long  BTC 0.05    │ ║
║ • cln-pp    ║ ║ │     ╱  ╲╱╲╲╱     ╲╱╲                  │ │ 13:08 fill   95400.0     │ ║
╠═════════════╣ ║ │   ╱            ▲                      │ │ 13:14 -short BTC 0.02    │ ║
║ COMPLETED   ║ ║ ╰───────────────────────────────────────╯ │ 13:18 finding: drift     │ ║
║ • cmp-1 ✓   ║ ║ ╭─ progress ────────────────────────────╮ ╰──────────────────────────╯ ║
║ • bb-2  ✓   ║ ║ │ ▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░ 56%         │ ╭─ findings extracted ─────╮ ║
║ • dchn  ✓   ║ ║ │ tokens 29k / ~53k · ETA ~50s          │ │ ◐ regime_drift   info    │ ║
║ + queue     ║ ║ ╰───────────────────────────────────────╯ │ ◐ overtrading    warn    │ ║
╚═════════════╝ ╚═════════════════════════════════════════════════════════════════════════╝
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ⑥   FLIGHT DECK         dense gauge cluster · few big buttons · L2
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine ────────────────────────────────────────────────────────────────
   ┌────────────── eth-mr · pre-flight checklist ────────────────────────────────────────┐
   │   ┌── slots ─────┐  ┌── risk ────┐  ┌── tokens/wk ──┐  ┌── tier ──┐  ┌── ready ───┐│
   │   │ ② ✓  ③ ✓     │  │  ▼  med    │  │   ~12 k       │  │   A      │  │ ▓▓▓▓▓▓░░ │ │
   │   │ ④ ⏳         │  │  kill ✓    │  │   est'd cost  │  │ open     │  │   80 %    │ │
   │   └──────────────┘  └────────────┘  └───────────────┘  └──────────┘  └───────────┘│
   │                                                                                      │
   │      ╔══════════════════════╗  ╔══════════════════════╗  ╔══════════════════════╗   │
   │      ║      VALIDATE        ║  ║   PAPER  DEPLOY  ▶   ║  ║      PUBLISH ↗       ║   │
   │      ╚══════════════════════╝  ╚══════════════════════╝  ╚══════════════════════╝   │
   └──────────────────────────────────────────────────────────────────────────────────────┘
```

```
─── eval engine ─────────────────────────────────────────────────────────────────────────────
   ┌────────────── run 01H8N… · in flight ────────────────────────────────────────────────┐
   │   ┌── sharpe ──┐  ┌── dd ──┐  ┌── trades ──┐  ┌── tokens ──┐  ┌── findings ───┐    │
   │   │   1.62 ▲   │  │ -7.1%  │  │     47     │  │    29 k    │  │  ◐  3 new     │    │
   │   └────────────┘  └────────┘  └────────────┘  └────────────┘  └───────────────┘    │
   │                                                                                       │
   │   ┌── progress ──────────────────────────────────────────────────────────────────┐   │
   │   │ ▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░  41%   · ETA 50s · 442/1080 decisions      │   │
   │   └──────────────────────────────────────────────────────────────────────────────┘   │
   │                                                                                       │
   │      ╔═════════════╗  ╔═════════════╗  ╔═════════════╗  ╔═════════════╗             │
   │      ║   PAUSE     ║  ║   ABORT     ║  ║  COMPARE …  ║  ║  PUBLISH ↗  ║             │
   │      ╚═════════════╝  ╚═════════════╝  ╚═════════════╝  ╚═════════════╝             │
   └──────────────────────────────────────────────────────────────────────────────────────┘
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ⑦   SPREADSHEET         tabular sweep · whole catalog at once · paid tier
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine  (template gallery as grid) ────────────────────────────────────
┌────────────┬──────────┬──────────┬─────────┬───────┬──────────┬─────────────────────────┐
│ TEMPLATE   │ regime   │ assets   │ price   │ tier  │ sharpe ▼ │ status                  │
├────────────┼──────────┼──────────┼─────────┼───────┼──────────┼─────────────────────────┤
│ trend      │ ↑ trend  │ BTC,ETH  │ free    │  A    │   1.91   │ ★ user pinned           │
│ breakout   │ ↑ vol    │ BTC      │ free    │  A    │   1.78   │                         │
│ mean-rev   │ ↔ range  │ ETH      │ free    │  A    │   1.62   │ ▸ editing               │
│ momentum   │ ↑ trend  │ BTC,ETH  │ free    │  A    │   1.55   │                         │
│ scalping   │ μ-struct │ BTC      │ free    │  A    │   0.94   │                         │
│ news       │ event    │ ETH      │ $5/mo   │  B    │   1.40   │                         │
│ custom     │ any      │ any      │ free    │  A    │    —     │                         │
│ onchain    │ flows    │ BTC,ETH  │ free    │  A    │   1.71   │                         │
└────────────┴──────────┴──────────┴─────────┴───────┴──────────┴─────────────────────────┘
   click row → inspector  ·  shift-click → batch eval  ·  ⌘-click → fork as new draft
```

```
─── eval engine  (param sweep) ──────────────────────────────────────────────────────────────
                                ◀─── rsi_oversold ───▶              seeds
┌──────────────┬──────┬──────┬──────┬──────┬──────┬───────────┬───────────────┐
│ bb_period    │  20  │  25  │  30  │  35  │  40  │   #1234   │    #5678      │
├──────────────┼──────┼──────┼──────┼──────┼──────┼───────────┼───────────────┤
│      14      │ 1.20 │ 1.41 │ 1.55 │ 0.98 │ 0.71 │  ░░░░░░░  │   ░░░░░░░     │
│      20      │ 1.62▲│ 1.71 │ 1.43 │ 0.88 │ 0.62 │  ░▓▓▓░░░  │   ░▓▓▓▓▓░     │
│      30      │ 0.91 │ 1.10 │ 1.05 │ 0.72 │ 0.55 │  ░░░░░░░  │   ░░░░░░░     │
│      50      │ 0.65 │ 0.72 │ 0.81 │ 0.59 │ 0.48 │  ░░░░░░░  │   ░░░░░░░     │
└──────────────┴──────┴──────┴──────┴──────┴──────┴───────────┴───────────────┘
   queued 16  ·  running 4  ·  done 12  ·  failed 0   |   click cell → drilldown to run
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ARCHETYPE  ⑧   LAB BENCH           journal + tray · everything is versioned · researcher
═════════════════════════════════════════════════════════════════════════════════════════════
```

```
─── strategy creation engine ────────────────────────────────────────────────────────────────
┌─ JOURNAL ─────────────────────────────────────────────────────┐ ┌─ TRAY ─────────────┐
│ 05-08 13:14  forked btc-tf → eth-mr; switched ETH; rsi 30→25  │ │  ⌬ btc-tf  ✓ ship  │
│ 05-08 13:22  ran bull-q1: sharpe 1.62 · finding regime_drift  │ │  ⌬ eth-mr   draft  │
│ 05-08 13:30  branched eth-mr → eth-mr-v2 ; tightened stop     │ │     └ v1.0  v1.1   │
│ 05-08 13:42  attached skill news-aware-decision@1.0           │ │        └ v1.2  ◀   │
│ 05-08 13:50  ran bull-q1 again: sharpe 1.71 ↑                 │ │  ⌬ eth-mr-v2 draft │
│ 05-08 13:55  cherry-picked stop change back to eth-mr@v1.3    │ │  ⌬ ┄ new draft     │
│ 05-08 14:02  ▌                                                │ │                    │
└───────────────────────────────────────────────────────────────┘ └────────────────────┘
   ↻ every action is a commit · branch any draft · diff any two versions · rollback safe
```

```
─── eval engine ─────────────────────────────────────────────────────────────────────────────
┌─ JOURNAL ─────────────────────────────────────────────────────┐ ┌─ TRAY ─────────────┐
│ 05-08 12:00  run 01H8N · eth-mr@v1.2 · bull-q1     1.62 ✓     │ │  ⏷ 01H8N    ✓ done │
│ 05-08 12:14  run 01J2P · eth-mr@v1.2 · chop-q2     0.41 ✗     │ │  ⏷ 01J2P    ✓ done │
│ 05-08 12:22  attestation signed for bull-q1 → marketplace     │ │  ⏷ 01K9R    ░░░ 41%│
│ 05-08 12:30  run 01K9R · eth-mr@v1.3 · bear-q3      …running  │ │  ⏷ 01M4T    queued │
│ 05-08 12:40    finding: tail-risk concentration               │ │                    │
│ 05-08 12:55  diff(01H8N, 01J2P) → regime_fit_mismatch         │ │  [+] new run       │
│ 05-08 13:02  ▌                                                │ │  [⤴] re-run all    │
└───────────────────────────────────────────────────────────────┘ └────────────────────┘
   ↻ every run is permanent · diff any pair · drag two onto compare · attest any to chain
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ◇ COINAGE  ◇       two archetypes worth designing fresh, both surfaced by negation
═════════════════════════════════════════════════════════════════════════════════════════════

   ◇ TICKER          (negation of "page = run") — the run lives in a persistent strip at the
                     top or side of the dashboard, surviving navigation. user fires a run,
                     keeps building, watches the ticker out of the corner of their eye.
                     paired with desktop notifications on completion. solves the L1 problem
                     of "I left the page and the run kept going."

       ┌─ active runs ──────────────────────────────────────────────────────────────────┐
       │ ◐ 01H8N eth-mr · bull-q1 ▓▓▓▓▓░░░ 41%  │ ✓ 01J2P done · sharpe 0.41 [view ↗]  │
       └────────────────────────────────────────────────────────────────────────────────┘

   ◇ SLOT MACHINE    (negation of "linear flow") — wizard's lite cousin. three roller
                     reels: { template, regime, asset }. user pulls the lever, gets a
                     suggested config, hits "run eval" or "save", or rolls again. low-load
                     onboarding for users who can't articulate intent yet.

       ╭─────────╮ ╭─────────╮ ╭─────────╮       [ pull ↻ ]   [ keep ✓ ]   [ tweak → ]
       │ trend   │ │ ETH     │ │ aggressv│
       │ breakout│ │ BTC ●   │ │ balanced│
       │ mean-rev│ │ SOL     │ │ cnsrv ● │
       ╰─────────╯ ╰─────────╯ ╰─────────╯
```

---

```
═════════════════════════════════════════════════════════════════════════════════════════════
   ◆ DESIGNER HAND-OFF  ◆     recommended pairings (subjective — for discussion, not lock-in)
═════════════════════════════════════════════════════════════════════════════════════════════

   USER LEVEL    │   STRATEGY BUILDER          │   EVAL ENGINE
   ──────────────┼─────────────────────────────┼─────────────────────────────────────
   L1            │   ★ WIZARD (default `/`)    │   WIZARD (auto-runs canonical eval)
   L2            │   INSPECTOR or FLIGHT DECK  │   FLIGHT DECK (single-run cockpit)
   L3            │   CANVAS or CONTROL TOWER   │   CONTROL TOWER (live SSE-streamed)
   L4            │   NOTEBOOK or LAB BENCH     │   SPREADSHEET (sweeps) · LAB BENCH
   ambient       │   —                         │   ◇ TICKER (every level, persistent)

   key insight: eval engine has more reversibility headroom than strategy builder, so it can
   support more archetypes (notebook, spreadsheet, lab bench all natural). builder is more
   linear by nature (one bundle, validated as a whole), so wizard / inspector / canvas
   carry more weight. control tower & flight deck are the bridges — same skeleton, different
   payload, both engines can share the layout system.
```

```
[ideonomy · 8 archetypes × 2 engines · 3 dims · 2 organons · "xvn UX field for designer hand-off"]
  dim · pivot:        complexity — every archetype is a different bet on user load (L1↔L4)
  ◆ organon-construction (chart):  complexity × reversibility grid placed 8 named archetypes;
                                   surfaced ◇ "hyper-complex × irreversible" as anti-pattern
  ◆ negation: "chat-led"        →  INSPECTOR · NOTEBOOK
  ◆ negation: "linear flow"     →  CANVAS · LAB BENCH · ◇ SLOT MACHINE (coinage)
  ◆ negation: "one at a time"   →  SPREADSHEET · CONTROL TOWER
  ◆ negation: "opaque kernel"   →  FLIGHT DECK
  ◆ negation: "page = run"      →  ◇ TICKER (coinage — ambient persistent strip)
  ◆ negation: "irreversible"    →  LAB BENCH (every action a commit)
  not surfaced:   the "no UI at all" axis (CLI-only, MCP-only, headless cron, voice/chat-bot
                  embedded in another product, mobile companion) — all valid running surfaces
                  the dim-prompts didn't reach. also: multi-user/social UX (shared journal,
                  team notebook, public live-tower) — collaboration was outside the picked
                  axes. worth a follow-up tuple with dimensions { sociality, automation,
                  embeddedness } before the designer locks the v1 shape.
```
