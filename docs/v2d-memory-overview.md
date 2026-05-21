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

Patterns ships empty in v1. The autoresearcher that produces
Patterns from accumulated Observations hasn't landed yet, and the
manual seeding command is a v1.1 follow-up.

So even with a slot set to Agent-scoped or Global, the Memory panel
on the eval-review page will show no recall items. Observations are
still being recorded — you can think of v1 as the data-gathering
phase — but the agent is reading from an empty Patterns shelf.

This is expected. It's not a bug, it's not a config error, and there
is no toggle you missed. The next two sections describe when that
shelf starts to fill up.

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

## Coming next

Patterns start populating when one of two things ships:

- **v1.1: manual seeding CLI.** A small `xvn memory add-pattern`
  verb lets you hand-write Patterns from accumulated observations
  or domain expertise. Useful when you've watched enough runs to
  know what's worth distilling but the autoresearcher isn't online
  yet.
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
