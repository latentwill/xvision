# Agents

An agent is a reusable saved bundle of a system prompt, a provider/model
selection, a temperature, an output token budget, and an optional set of
skills. Agents live in the workspace library and are referenced by id from
strategies — the same agent can appear in multiple strategies simultaneously.
Editing an agent propagates to every strategy that references it on its next
run.

> **Rename in progress** — the "intern" role is being renamed to "default
> agent" in the wizard, rail, and settings UI. Both terms remain valid during
> the crossover period; strategies written with either name continue to work.

---

## Author an agent from the dashboard

Open `/agents/new`. The form has three sections:

- **Identity** — name (slug-style, e.g. `btc-mean-rev-v1`), description,
  and optional tags.
- **Behavior (slots)** — one or more agent slots. Each slot exposes:
  - **Provider** — picked from your enabled providers.
  - **Model** — picked from the models available for that provider.
  - **System prompt** — the prompt handed to the model at dispatch time.
  - **Skills** — attached skills from the workspace skill registry (managed
    at `/agents/skills`; shown only when skills are already linked).
- **Template picker** — before reaching the form, the dashboard offers three
  starter templates: *Single-prompt trader* (one slot), *Analyst → Executor*
  (two slots, sequential), and *Risk-checked trader* (three slots,
  trader / risk / executor). Templates seed the form; rename or extend freely
  after creation. You can also skip to a blank agent.

**Chat rail path:** describe the agent you want in plain English and the rail
composes one for you, then routes to the same form pre-filled.

The CLI does not have a create verb today — authoring uses the dashboard form.

---

## Memory

Every agent slot has a **Memory** selector, right next to the provider
and model pickers. It controls whether the slot carries anything
forward between cycles and between runs. It is **off by default** and
stays off until you change it — a slot with the selector untouched
behaves exactly like agents did before memory shipped.

Three positions:

- **Off** — no memory. Nothing is recorded, nothing is recalled. Use
  this when you want a clean A/B between two prompts or a backtest
  with hard guarantees that the agent only sees what's in its prompt.
- **Global** — the slot shares a memory pool with every other slot
  also set to Global. Useful when several agents in a strategy should
  pool what they learn.
- **Agent-scoped** — the slot has its own private memory keyed to the
  agent id. Other agents, including other slots in the same strategy,
  can't see it.

In v1, Memory mostly records — the recall side ships empty by design,
so you'll see the Memory panel on the eval-review page sit at "no
recall items" even with Global or Agent-scoped selected. That's
expected; the design behind it (and the reason backtest replays
don't leak future knowledge) is covered in the full memory
overview at `docs/v2d-memory-overview.md` in the repo.

---

## Read an agent from the CLI

```
xvn agent get <agent-id>
xvn agent get <agent-id> --format json-compact
```

`--format` accepts `json` (default, pretty-printed) or `json-compact`
(single-line, suitable for piping to `jq`). `show` is an accepted alias for
`get`.

Example output:

```json
{
  "agent_id": "01HZAGENT000000000000001",
  "name": "momentum-trader-v2",
  "description": "GPT-4o momentum strategy",
  "tags": ["momentum"],
  "archived": false,
  "created_at": "2026-05-01T09:00:00Z",
  "updated_at": "2026-05-15T14:22:00Z",
  "slots": [
    {
      "name": "main",
      "provider": "openai",
      "model": "gpt-4o-mini",
      "temperature": 0.2
    }
  ]
}
```

---

## Roles

By dashboard convention, agents play named roles in a strategy. Roles are
display labels set on the strategy's agent references via the Inspector — they
live on the reference, not on the agent itself.

| Role | Description |
|---|---|
| default agent / `intern` | General-purpose decision slot. "intern" is the legacy name; the UI is migrating to "default agent". |
| `trader` | Proposes a trade decision. |
| `risk` / `risk_check` | Vetoes or modifies the trader's proposal. |
| `executor` | Commits the final decision after risk review. |
| `analyst` | Produces a structured thesis consumed by a downstream slot. |

Roles are plain strings — strategies can freely rename or invent them.

---

## CLI verbs at a glance

Today's agent CLI surface is read-only. See [CLI Reference](/docs?slug=cli-reference)
for full flag documentation.

| Verb | Effect |
|---|---|
| `xvn agent get <agent-id> [--format json\|json-compact]` | Fetch a single agent by id. Alias: `show`. |

---

## Where agents live

Agents are stored in the workspace database at `$XVN_HOME/xvn.db` (defaults to
`$HOME/.xvn/xvn.db`; override with `--xvn-home` or the `XVN_HOME` env var).
The database is created automatically on first use; no separate init step is
required.
