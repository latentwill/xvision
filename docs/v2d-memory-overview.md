# Agent memory in xvision

This page explains what the per-slot memory toggle does, the three
modes you can set it to, and why a backtest you run twice doesn't
let the agent cheat off its earlier run.

It's written for operators. Skim it once when you turn memory on for
the first time; come back if a run does something surprising.

## What memory is for

By default, an agent in xvision starts every cycle the same way it
started the last one — same prompt, no notes from prior runs. That's
fine for one-off experiments, but it throws away anything useful the
agent figured out along the way.

Memory lets an agent accumulate reusable knowledge across runs. Over
time the goal is for a trader agent to recognise situations it has
seen before and let that recognition shape — not dictate — its
current decision.

## The three modes

Each agent slot has a memory selector with three positions:

- **Off** (default). The agent has no memory. Nothing is recorded;
  nothing is recalled. Use this for clean A/B experiments, throwaway
  prompts, or anytime you want a hard guarantee that the agent only
  knows what's in its prompt.
- **Global**. The agent shares memory with every other agent slot
  that's also set to Global. Use this when several agents in a
  strategy should pool what they learn (e.g. trader and risk both
  benefit from "this scenario shape tends to chop"). It's the right
  choice when you want the *strategy* to learn, not any one slot.
- **Agent-scoped**. The agent has its own private memory keyed to its
  agent id. Other agents — including other slots in the same strategy
  — can't see it. Use this when you want one slot's experience to
  stay siloed (e.g. a specialised analyst whose lessons would mislead
  the trader).

Memory is off by default and stays off until you change it. Toggle
it per slot in the agent form, right next to the provider and model
pickers.

## How memory actually works — Observations and Patterns

Memory in xvision is two-layered. Your agents *write*
**Observations** as they run — what they saw, what they decided,
when. Observations stay in the engine's memory store for analysis.
Your agents *read* **Patterns** — distilled insights that the
autoresearcher (or you, manually) has validated as predictive.
Reading is one-way: agents never see raw Observations during a
decision, only the Patterns those Observations have been distilled
into. Patterns also carry the date their training data ended, so
the agent never sees a Pattern learned from data inside the
historical window it's currently replaying. That's why running a
backtest twice doesn't give the agent foreknowledge of the outcome.

Two practical consequences:

- A backtest of an August 2024 scenario won't surface Patterns
  whose training data overlaps August 2024, even if those Patterns
  match the bars in front of the agent very closely. The temporal
  filter excludes them.
- The same backtest *will* surface Patterns drawn entirely from
  earlier history (say, the 2020 covid crash and the 2022 Luna
  collapse), because those Patterns finished training before the
  scenario you're replaying began. That's the kind of cross-event
  generalisation memory is supposed to enable.

## What you'll see today

Patterns ships empty. The autoresearcher that distils Patterns from
accumulated Observations hasn't landed yet, but as of v1.1 you can
hand-seed Patterns yourself — see "Managing memory" below for the
CLI and dashboard surfaces.

So even with a slot set to Agent-scoped or Global, the Memory panel
on the eval-review page will show no recall items until you (or a
future autoresearcher pass) put some Patterns on the shelf.
Observations are still being recorded — you can think of v1 as the
data-gathering phase — but the agent is reading from an empty
Patterns shelf until you seed it.

## Clearing memory — `xvn memory forget`

When you want to start fresh — bad run, prompt change, regime change,
end of an experiment — `xvn memory forget` clears stored memory.

Clear a shared namespace:

```
xvn memory forget --namespace global
```

Clear one agent's private memory:

```
xvn memory forget --agent <agent_id>
```

Forget is permanent and immediate. There is no undo. If you want a
safety net, snapshot `$XVN_HOME/xvn.db` before running it.

## Managing memory

V2D shipped the storage, recorder, and recall plumbing. v1.1 adds the
operator surface — three places to browse, seed, and prune memory
without poking at SQLite.

### The `/memory` page — workspace-wide

`/memory` sits in the dashboard sidebar between `/scenarios` and
`/eval-runs`. It's a workspace-scoped view of everything in the
`global` namespace — the shared pool that every slot set to
`memory_mode = global` reads from.

The page is split into two sub-tabs:

- **Patterns** — the distilled, agent-readable shelf. Each row shows
  the Pattern text, when it was added, and (if set) its
  `training_window_end`. A `+ Add Pattern` button opens a modal that
  takes the Pattern text and an optional training-window-end date.
- **Observations** — read-only. Filterable by scenario and run id, so
  you can audit what your global-memory agents have been writing.

Reach for this page when you want to seed cross-agent wisdom that
every Global-mode slot reads — a one-time prompt-style nudge that
isn't specific to any single agent ("price action on Sundays is
unusually thin" — true for everyone).

### The per-agent Memory tab

Open any agent at `/agents/<agent_id>`. There's now a **Memory** tab
sitting between **Configuration** and the existing tail of the page.
Inside, the same Patterns + Observations split, but scoped to *that
agent's* namespace.

This is the page to use when:

- You want to see what one specific agent has been observing during
  runs (Observations tab, filter by scenario or run).
- You want to seed a Pattern that should only fire for this agent —
  hard-won wisdom that would mislead other agents on the bench.

### The `xvn memory` CLI — for scripting

Same surface as the UI, but suited for automation, ops shell
sessions, and one-shot seeding from a script:

- `xvn memory ls` — list memory items. Defaults to Patterns; flags
  filter by tier, agent, namespace, scenario, or run.
  ```
  xvn memory ls --tier observation --agent <agent_id>
  ```
- `xvn memory show <id>` — print full detail for one item.
  ```
  xvn memory show 01HZMEM00000000000000001
  ```
- `xvn memory add-pattern "<text>" --namespace <ns>` — write a new
  Pattern. Optional `--training-end <date>` and `--force` flags.
  ```
  xvn memory add-pattern "BTC ranges before FOMC" --namespace global --training-end 2024-09-01
  ```
- `xvn memory rm <id>` — delete one item by id.
  ```
  xvn memory rm 01HZMEM00000000000000001
  ```
- `xvn memory forget --namespace <ns>` or `--agent <id>` — bulk-clear
  a namespace or agent. Prints the deleted count to stdout.
  ```
  xvn memory forget --agent <agent_id>
  ```

The UI is for browsing and one-off seeding; the CLI is for everything
that fits in a shell history.

### The `training_window_end` field

Every Pattern carries an optional `training_window_end` — an
operator-attested datestamp marking the last day of historical data
the Pattern was distilled from. It exists to keep backtests honest.

- **Leave it blank** (omit the CLI flag, leave the UI date picker
  empty) and the Pattern is recalled in every scenario. This is the
  right default for prompt-style nudges that aren't backtest-sensitive
  ("agents should err on the side of inaction during low-liquidity
  windows").
- **Set it** to a date like `2024-09-01` and the Pattern is excluded
  from any scenario whose start date is on-or-after that date. So a
  Pattern with `training_window_end = 2024-09-01` will recall in a
  scenario starting 2024-08-31, but not one starting 2024-09-01 or
  later.

Bare dates from both the CLI and the UI normalise to
`T23:59:59Z` — end of day, UTC. So `--training-end 2024-09-01` means
"this Pattern's training data goes through the end of September 1,
2024." If you want sub-day precision, pass a full RFC 3339 timestamp
to the CLI.

### Deep-links from eval-review

The MemoryPanel on the eval-review page now has an overflow menu
(`⋯`) on each recall row. Click **Open Pattern** and you land on the
Pattern's management page with that row highlighted in gold and
scrolled into view:

- Agent-scoped recalls → `/agents/<id>?tab=memory&pattern=<pid>`
- Global recalls → `/memory?pattern=<pid>`

The URL is shareable — paste it into Slack or handoff notes and the
recipient lands on the same highlighted Pattern.

### "Forget all" buttons

Both the workspace `/memory` page and the per-agent Memory tab have a
**Forget all** button at the bottom. Clicking opens a Radix
AlertDialog that shows the exact item count before deletion:

- Per-agent Forget all → `DELETE /api/memory?agent=<id>`
- Workspace Forget all → `DELETE /api/memory?namespace=global`

Same permanence rules as `xvn memory forget` — no undo. The confirm
dialog exists precisely because the action is destructive.

### Embedder requirement

Patterns are recalled via vector similarity. If the workspace has no
embedder configured, Patterns are still *stored* — but they will
never be *recalled*, because there's no embedding to match against.

- The CLI emits a stderr warning and exits non-zero on
  `xvn memory add-pattern` when no embedder is configured. Pass
  `--force` to write the Pattern anyway (useful when you're
  pre-seeding ahead of standing up an embedder).
- The UI does not currently gate Pattern creation on embedder
  presence — the CLI is the only place that warns today.
- If a slot has `memory_mode != off` and the workspace has no
  embedder, the dispatcher emits a `memory_disabled_no_embedder`
  event visible in the MemoryPanel on eval-review. That's your
  signal that recall is silently disabled despite the slot being on.

### What's not yet implemented

Three operations are intentionally absent from v1.1:

- **Pattern editing in place.** No "edit this Pattern" affordance.
  Edits land via V3's supersede/replace semantics; until then,
  `rm` and re-add.
- **Per-item Observation delete.** Observations are bulk-forget-only.
  This keeps the Observation tier honest as the autoresearcher's
  write-once substrate.
- **Manual distillation (Observation → Pattern).** That's the V3
  autoresearcher's job (board-v2 item 11a). v1.1's
  `xvn memory add-pattern` lets you hand-write Patterns from prior
  art, but it won't read your Observations and propose Patterns for
  you.

## Coming next

The manual seeding path landed in v1.1 — see the "Managing memory"
section above. The remaining gap is automated distillation:

- **V3: the autoresearcher.** Item 11a on `team/board-v2.md`. The
  autoresearcher's mutator-judge-promote loop is the cortex
  distillation pass: it reads Observations across many runs,
  proposes candidate Patterns, validates them against held-out
  scenarios, and promotes the ones that survive. That's when the
  Memory panel starts showing recall items, and when memory begins
  to materially shape decisions.

Until then, leave memory off for backtests you want to keep clean,
turn it on (Global or Agent-scoped) for runs whose Observations you
want to feed the future distillation pass.

## Undo a `forget`

`xvn memory forget` is no longer destructive by default. Rows are
soft-deleted — marked with a `forgotten_at` timestamp — and stay
recoverable for `XVN_MEMORY_FORGET_GRACE_DAYS` (default **14 days**)
before a janitor sweep removes them permanently. Inside the window,
`xvn memory ls` hides them; `xvn memory undo-forget` restores them.

```bash
# accidentally forgot a namespace
xvn memory forget --agent abc

# bring it back inside the grace window
xvn memory undo-forget --agent abc
```

`undo-forget` accepts an optional RFC3339 `--since` that lower-bounds
which rows are restored. The default lower bound is `now - grace_days`
so a bare `undo-forget` recovers everything inside the window.

### Bypassing the grace window

To restore V2D's prior destructive semantics, set
`XVN_MEMORY_FORGET_GRACE_DAYS=0`. `forget` then immediately
hard-deletes and there is nothing for `undo-forget` to recover.

### Janitor sweep

The engine surface exposes `sweep_expired` (and the matching
`MemoryStore::hard_delete_expired`) for hosts that want to run the
sweep periodically. With grace > 0 the sweep is safe to call as often
as desired — it only removes rows whose `forgotten_at` is older than
the grace window.
